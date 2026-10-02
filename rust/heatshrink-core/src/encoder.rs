use crate::common::{
    valid_window_lookahead, HEATSHRINK_BACKREF_MARKER, HEATSHRINK_LITERAL_MARKER, MATCH_NOT_FOUND,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i8)]
pub enum EncoderSinkRes {
    Ok = 0,
    ErrorNull = -1,
    ErrorMisuse = -2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i8)]
pub enum EncoderPollRes {
    Empty = 0,
    More = 1,
    ErrorNull = -1,
    ErrorMisuse = -2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i8)]
pub enum EncoderFinishRes {
    Done = 0,
    More = 1,
    ErrorNull = -1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HseState {
    NotFull,
    Filled,
    Search,
    YieldTagBit,
    YieldLiteral,
    YieldBrIndex,
    YieldBrLength,
    SaveBacklog,
    FlushBits,
    Done,
}

const FLAG_IS_FINISHING: u8 = 0x01;

struct OutputInfo<'a> {
    buf: &'a mut [u8],
    output_size: &'a mut usize,
}

pub struct Encoder {
    input_size: u16,
    match_scan_index: u16,
    match_length: u16,
    match_pos: u16,
    outgoing_bits: u16,
    outgoing_bits_count: u8,
    flags: u8,
    state: HseState,
    current_byte: u8,
    bit_index: u8,
    window_sz2: u8,
    lookahead_sz2: u8,
    buffer: Vec<u8>,
    #[cfg(feature = "use-index")]
    search_index: Vec<i16>,
}

impl Encoder {
    pub fn alloc(window_sz2: u8, lookahead_sz2: u8) -> Option<Self> {
        if !valid_window_lookahead(window_sz2, lookahead_sz2) {
            return None;
        }
        let buf_sz = 2usize << window_sz2;
        let mut enc = Self {
            input_size: 0,
            match_scan_index: 0,
            match_length: 0,
            match_pos: 0,
            outgoing_bits: 0,
            outgoing_bits_count: 0,
            flags: 0,
            state: HseState::NotFull,
            current_byte: 0,
            bit_index: 0x80,
            window_sz2,
            lookahead_sz2,
            buffer: vec![0; buf_sz],
            #[cfg(feature = "use-index")]
            // One extra slot: C may read index[end] when end == buf_sz (see find_longest_match).
            search_index: vec![-1i16; buf_sz + 1],
        };
        enc.reset();
        Some(enc)
    }

    pub fn reset(&mut self) {
        let buf_sz = 2usize << self.window_sz2;
        self.buffer.fill(0);
        self.input_size = 0;
        self.state = HseState::NotFull;
        self.match_scan_index = 0;
        self.flags = 0;
        self.bit_index = 0x80;
        self.current_byte = 0;
        self.match_length = 0;
        self.outgoing_bits = 0;
        self.outgoing_bits_count = 0;
        let _ = buf_sz;
    }

    pub fn window_sz2(&self) -> u8 {
        self.window_sz2
    }

    pub fn lookahead_sz2(&self) -> u8 {
        self.lookahead_sz2
    }

    pub fn sink(&mut self, in_buf: &[u8]) -> (EncoderSinkRes, usize) {
        if is_finishing(self) {
            return (EncoderSinkRes::ErrorMisuse, 0);
        }
        if self.state != HseState::NotFull {
            return (EncoderSinkRes::ErrorMisuse, 0);
        }

        let write_offset = self.get_input_offset() + self.input_size;
        let ibs = self.get_input_buffer_size();
        let rem = ibs - self.input_size;
        // Compare in usize: `len as u16` wraps multiples of 65536 to 0 and sinks nothing.
        let cp_sz = (rem as usize).min(in_buf.len()) as u16;

        let end = write_offset as usize + cp_sz as usize;
        self.buffer[write_offset as usize..end].copy_from_slice(&in_buf[..cp_sz as usize]);
        self.input_size += cp_sz;

        if cp_sz == rem {
            self.state = HseState::Filled;
        }

        (EncoderSinkRes::Ok, cp_sz as usize)
    }

    pub fn poll(&mut self, out_buf: &mut [u8]) -> (EncoderPollRes, usize) {
        if out_buf.is_empty() {
            return (EncoderPollRes::ErrorMisuse, 0);
        }

        let mut output_size = 0usize;
        let out_buf_size = out_buf.len();

        loop {
            let in_state = self.state;
            match in_state {
                HseState::NotFull => return (EncoderPollRes::Empty, output_size),
                HseState::Filled => {
                    self.do_indexing();
                    self.state = HseState::Search;
                }
                HseState::Search => self.state = self.st_step_search(),
                HseState::YieldTagBit => {
                    self.state = self.st_yield_tag_bit(&mut OutputInfo {
                        buf: &mut out_buf[..out_buf_size],
                        output_size: &mut output_size,
                    });
                }
                HseState::YieldLiteral => {
                    self.state = self.st_yield_literal(&mut OutputInfo {
                        buf: &mut out_buf[..out_buf_size],
                        output_size: &mut output_size,
                    });
                }
                HseState::YieldBrIndex => {
                    self.state = self.st_yield_br_index(&mut OutputInfo {
                        buf: &mut out_buf[..out_buf_size],
                        output_size: &mut output_size,
                    });
                }
                HseState::YieldBrLength => {
                    self.state = self.st_yield_br_length(&mut OutputInfo {
                        buf: &mut out_buf[..out_buf_size],
                        output_size: &mut output_size,
                    });
                }
                HseState::SaveBacklog => self.state = self.st_save_backlog(),
                HseState::FlushBits => {
                    self.state = self.st_flush_bit_buffer(&mut OutputInfo {
                        buf: &mut out_buf[..out_buf_size],
                        output_size: &mut output_size,
                    });
                    // C fallthrough from HSES_FLUSH_BITS to HSES_DONE (heatshrink_encoder.c:236-239)
                    return (EncoderPollRes::Empty, output_size);
                }
                HseState::Done => return (EncoderPollRes::Empty, output_size),
            }

            if self.state == in_state && output_size == out_buf_size {
                return (EncoderPollRes::More, output_size);
            }
        }
    }

    pub fn finish(&mut self) -> EncoderFinishRes {
        self.flags |= FLAG_IS_FINISHING;
        if self.state == HseState::NotFull {
            self.state = HseState::Filled;
        }
        if self.state == HseState::Done {
            EncoderFinishRes::Done
        } else {
            EncoderFinishRes::More
        }
    }
}

fn is_finishing(hse: &Encoder) -> bool {
    hse.flags & FLAG_IS_FINISHING != 0
}

fn can_take_byte(oi: &OutputInfo<'_>) -> bool {
    *oi.output_size < oi.buf.len()
}

impl Encoder {
    fn get_input_offset(&self) -> u16 {
        self.get_input_buffer_size()
    }

    fn get_input_buffer_size(&self) -> u16 {
        1 << self.window_sz2
    }

    fn get_lookahead_size(&self) -> u16 {
        1 << self.lookahead_sz2
    }

    fn st_step_search(&mut self) -> HseState {
        let window_length = self.get_input_buffer_size();
        let lookahead_sz = self.get_lookahead_size();
        let msi = self.match_scan_index;

        let fin = is_finishing(self);
        let threshold = if fin { 1 } else { lookahead_sz };
        if msi > self.input_size.wrapping_sub(threshold) {
            return if fin {
                HseState::FlushBits
            } else {
                HseState::SaveBacklog
            };
        }

        let input_offset = self.get_input_offset();
        let end = input_offset + msi;
        let start = end - window_length;

        let mut max_possible = lookahead_sz;
        if self.input_size.wrapping_sub(msi) < lookahead_sz {
            max_possible = self.input_size.wrapping_sub(msi);
        }

        let mut match_length = 0u16;
        let match_pos = self.find_longest_match(start, end, max_possible, &mut match_length);

        if match_pos == MATCH_NOT_FOUND {
            self.match_scan_index += 1;
            self.match_length = 0;
            HseState::YieldTagBit
        } else {
            self.match_pos = match_pos;
            self.match_length = match_length;
            HseState::YieldTagBit
        }
    }

    fn st_yield_tag_bit(&mut self, oi: &mut OutputInfo<'_>) -> HseState {
        if can_take_byte(oi) {
            if self.match_length == 0 {
                self.add_tag_bit(oi, HEATSHRINK_LITERAL_MARKER);
                HseState::YieldLiteral
            } else {
                self.add_tag_bit(oi, HEATSHRINK_BACKREF_MARKER);
                self.outgoing_bits = self.match_pos - 1;
                self.outgoing_bits_count = self.window_sz2;
                HseState::YieldBrIndex
            }
        } else {
            HseState::YieldTagBit
        }
    }

    fn st_yield_literal(&mut self, oi: &mut OutputInfo<'_>) -> HseState {
        if can_take_byte(oi) {
            self.push_literal_byte(oi);
            HseState::Search
        } else {
            HseState::YieldLiteral
        }
    }

    fn st_yield_br_index(&mut self, oi: &mut OutputInfo<'_>) -> HseState {
        if can_take_byte(oi) {
            if self.push_outgoing_bits(oi) > 0 {
                HseState::YieldBrIndex
            } else {
                self.outgoing_bits = self.match_length - 1;
                self.outgoing_bits_count = self.lookahead_sz2;
                HseState::YieldBrLength
            }
        } else {
            HseState::YieldBrIndex
        }
    }

    fn st_yield_br_length(&mut self, oi: &mut OutputInfo<'_>) -> HseState {
        if can_take_byte(oi) {
            if self.push_outgoing_bits(oi) > 0 {
                HseState::YieldBrLength
            } else {
                self.match_scan_index += self.match_length;
                self.match_length = 0;
                HseState::Search
            }
        } else {
            HseState::YieldBrLength
        }
    }

    fn st_save_backlog(&mut self) -> HseState {
        self.save_backlog();
        HseState::NotFull
    }

    fn st_flush_bit_buffer(&mut self, oi: &mut OutputInfo<'_>) -> HseState {
        if self.bit_index == 0x80 {
            HseState::Done
        } else if can_take_byte(oi) {
            oi.buf[*oi.output_size] = self.current_byte;
            *oi.output_size += 1;
            HseState::Done
        } else {
            HseState::FlushBits
        }
    }

    fn add_tag_bit(&mut self, oi: &mut OutputInfo<'_>, tag: u8) {
        self.push_bits(1, tag, oi);
    }

    fn do_indexing(&mut self) {
        #[cfg(feature = "use-index")]
        {
            let mut last = [-1i16; 256];
            let input_offset = self.get_input_offset();
            let end = input_offset + self.input_size;

            for i in 0..end as usize {
                let v = self.buffer[i] as usize;
                let lv = last[v];
                self.search_index[i] = lv;
                last[v] = i as i16;
            }
        }
    }

    fn find_longest_match(&self, start: u16, end: u16, maxlen: u16, match_length: &mut u16) -> u16 {
        let mut match_maxlen = 0u16;
        let mut match_index = MATCH_NOT_FOUND;
        let maxlen = maxlen as usize;
        let end = end as usize;
        let start = start as usize;

        #[cfg(feature = "use-index")]
        {
            let mut pos = self.search_index[end];
            while pos - start as i16 >= 0 {
                let pos_u = pos as usize;
                if self.buffer[pos_u + match_maxlen as usize]
                    != self.buffer[end + match_maxlen as usize]
                {
                    pos = self.search_index[pos_u];
                    continue;
                }
                let mut len = 1usize;
                while len < maxlen && self.buffer[pos_u + len] == self.buffer[end + len] {
                    len += 1;
                }
                if len as u16 > match_maxlen {
                    match_maxlen = len as u16;
                    match_index = pos as u16;
                    if len == maxlen {
                        break;
                    }
                }
                pos = self.search_index[pos_u];
            }
        }

        #[cfg(not(feature = "use-index"))]
        {
            let mut pos = end as i16 - 1;
            while pos - start as i16 >= 0 {
                let pos = pos as usize;
                if self.buffer[pos + match_maxlen as usize]
                    == self.buffer[end + match_maxlen as usize]
                    && self.buffer[pos] == self.buffer[end]
                {
                    let mut len = 1usize;
                    while len < maxlen && self.buffer[pos + len] == self.buffer[end + len] {
                        len += 1;
                    }
                    if len as u16 > match_maxlen {
                        match_maxlen = len as u16;
                        match_index = pos as u16;
                        if len == maxlen {
                            break;
                        }
                    }
                }
                pos -= 1;
            }
        }

        let break_even_point = 1 + self.window_sz2 as u16 + self.lookahead_sz2 as u16;

        if match_maxlen > break_even_point / 8 {
            *match_length = match_maxlen;
            return (end - match_index as usize) as u16;
        }
        MATCH_NOT_FOUND
    }

    fn push_outgoing_bits(&mut self, oi: &mut OutputInfo<'_>) -> u8 {
        let (count, bits) = if self.outgoing_bits_count > 8 {
            (
                8u8,
                (self.outgoing_bits >> (self.outgoing_bits_count - 8)) as u8,
            )
        } else {
            (self.outgoing_bits_count, self.outgoing_bits as u8)
        };

        if count > 0 {
            self.push_bits(count, bits, oi);
            self.outgoing_bits_count -= count;
        }
        count
    }

    fn push_bits(&mut self, count: u8, bits: u8, oi: &mut OutputInfo<'_>) {
        debug_assert!(count <= 8);

        if count == 8 && self.bit_index == 0x80 {
            oi.buf[*oi.output_size] = bits;
            *oi.output_size += 1;
        } else {
            for i in (0..count).rev() {
                let bit = bits & (1 << i);
                if bit != 0 {
                    self.current_byte |= self.bit_index;
                }
                self.bit_index >>= 1;
                if self.bit_index == 0 {
                    self.bit_index = 0x80;
                    oi.buf[*oi.output_size] = self.current_byte;
                    *oi.output_size += 1;
                    self.current_byte = 0;
                }
            }
        }
    }

    fn push_literal_byte(&mut self, oi: &mut OutputInfo<'_>) {
        let processed_offset = self.match_scan_index.wrapping_sub(1);
        let idx = self.get_input_offset() as usize + processed_offset as usize;
        let c = self.buffer.get(idx).copied().unwrap_or(0);
        self.push_bits(8, c, oi);
    }

    fn save_backlog(&mut self) {
        let input_buf_sz = self.get_input_buffer_size();
        let msi = self.match_scan_index;
        let rem = input_buf_sz - msi;
        let shift_sz = input_buf_sz + rem;

        let src_start = (input_buf_sz - rem) as usize;
        let src_end = src_start + shift_sz as usize;
        let chunk = self.buffer[src_start..src_end].to_vec();
        self.buffer[..chunk.len()].copy_from_slice(&chunk);

        self.match_scan_index = 0;
        self.input_size = self.input_size.wrapping_sub(input_buf_sz - rem);
    }
}

fn encoder_sink_read(
    hse: &mut Encoder,
    out: &mut Vec<u8>,
    scratch: &mut [u8],
    data: &[u8],
) -> bool {
    let mut sunk = 0usize;
    loop {
        if sunk < data.len() {
            let (_, n) = hse.sink(&data[sunk..]);
            sunk += n;
        }

        let mut poll_sz;
        loop {
            let (pres, w) = hse.poll(scratch);
            out.extend_from_slice(&scratch[..w]);
            poll_sz = w;
            if pres != EncoderPollRes::More {
                break;
            }
        }

        if poll_sz == 0 && data.is_empty() && hse.finish() == EncoderFinishRes::Done {
            return true;
        }

        if sunk >= data.len() {
            break;
        }
    }
    false
}

/// Run encoder to completion (matches C `encoder_sink_read` + EOF loop).
pub fn encode_all(input: &[u8], window_sz2: u8, lookahead_sz2: u8) -> Option<Vec<u8>> {
    if input.is_empty() {
        // Matches C CLI: compressing an empty file yields no output (see /dev/null encode).
        return Some(Vec::new());
    }
    let mut hse = Encoder::alloc(window_sz2, lookahead_sz2)?;
    let mut out = Vec::with_capacity(input.len() + input.len() / 2 + 4);
    let mut scratch = [0u8; 4096];

    let mut sunk = 0usize;
    let window = 1usize << window_sz2;
    while sunk < input.len() {
        let chunk_end = (sunk + window).min(input.len());
        if encoder_sink_read(&mut hse, &mut out, &mut scratch, &input[sunk..chunk_end]) {
            return Some(out);
        }
        sunk = chunk_end;
    }

    while !encoder_sink_read(&mut hse, &mut out, &mut scratch, &[]) {}

    Some(out)
}
