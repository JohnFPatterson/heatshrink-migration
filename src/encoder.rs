use crate::common::{
    BACKREF_MARKER, LITERAL_MARKER, MATCH_NOT_FOUND, MIN_LOOKAHEAD_BITS, MIN_WINDOW_BITS,
    MAX_WINDOW_BITS,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i8)]
pub enum SinkRes {
    Ok = 0,
    ErrorNull = -1,
    ErrorMisuse = -2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i8)]
pub enum PollRes {
    Empty = 0,
    More = 1,
    ErrorNull = -1,
    ErrorMisuse = -2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i8)]
pub enum FinishRes {
    Done = 0,
    More = 1,
    ErrorNull = -1,
}

const FLAG_IS_FINISHING: u8 = 0x01;

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

struct OutputInfo<'a> {
    buf: &'a mut [u8],
    output_size: &'a mut usize,
}

pub struct Encoder {
    window_sz2: u8,
    lookahead_sz2: u8,
    buffer: Vec<u8>,
    search_index: Vec<i16>,
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
}

impl Encoder {
    pub fn alloc(window_sz2: u8, lookahead_sz2: u8) -> Option<Self> {
        if window_sz2 < MIN_WINDOW_BITS
            || window_sz2 > MAX_WINDOW_BITS
            || lookahead_sz2 < MIN_LOOKAHEAD_BITS
            || lookahead_sz2 >= window_sz2
        {
            return None;
        }
        let buf_sz = 2usize << window_sz2;
        let mut enc = Self {
            window_sz2,
            lookahead_sz2,
            buffer: vec![0; buf_sz],
            search_index: vec![0; buf_sz],
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
        };
        enc.reset();
        Some(enc)
    }

    pub fn reset(&mut self) {
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
    }

    pub fn window_bits(&self) -> u8 {
        self.window_sz2
    }

    pub fn lookahead_bits(&self) -> u8 {
        self.lookahead_sz2
    }

    pub fn sink(&mut self, in_buf: &[u8], input_size: &mut usize) -> SinkRes {
        if self.is_finishing() {
            return SinkRes::ErrorMisuse;
        }
        if self.state != HseState::NotFull {
            return SinkRes::ErrorMisuse;
        }

        let write_offset = self.get_input_offset() + self.input_size;
        let ibs = self.get_input_buffer_size();
        let rem = ibs - self.input_size;
        let cp_sz = rem.min(in_buf.len() as u16);

        let wo = write_offset as usize;
        let cs = cp_sz as usize;
        self.buffer[wo..wo + cs].copy_from_slice(&in_buf[..cs]);
        *input_size = cs;
        self.input_size += cp_sz;

        if cp_sz == rem {
            self.state = HseState::Filled;
        }
        SinkRes::Ok
    }

    pub fn poll(&mut self, out_buf: &mut [u8], output_size: &mut usize) -> PollRes {
        if out_buf.is_empty() {
            return PollRes::ErrorMisuse;
        }
        *output_size = 0;

        loop {
            let in_state = self.state;
            self.state = match in_state {
                HseState::NotFull => return PollRes::Empty,
                HseState::Filled => {
                    self.do_indexing();
                    HseState::Search
                }
                HseState::Search => self.st_step_search(),
                HseState::YieldTagBit => self.st_yield_tag_bit(out_buf, output_size),
                HseState::YieldLiteral => self.st_yield_literal(out_buf, output_size),
                HseState::YieldBrIndex => self.st_yield_br_index(out_buf, output_size),
                HseState::YieldBrLength => self.st_yield_br_length(out_buf, output_size),
                HseState::SaveBacklog => self.st_save_backlog(),
                HseState::FlushBits => {
                    self.state = self.st_flush_bit_buffer(out_buf, output_size);
                    return PollRes::Empty;
                }
                HseState::Done => return PollRes::Empty,
            };

            if self.state == in_state {
                if *output_size == out_buf.len() {
                    return PollRes::More;
                }
            }
        }
    }

    pub fn finish(&mut self) -> FinishRes {
        self.flags |= FLAG_IS_FINISHING;
        if self.state == HseState::NotFull {
            self.state = HseState::Filled;
        }
        if self.state == HseState::Done {
            FinishRes::Done
        } else {
            FinishRes::More
        }
    }

    fn is_finishing(&self) -> bool {
        self.flags & FLAG_IS_FINISHING != 0
    }

    fn get_input_buffer_size(&self) -> u16 {
        1 << self.window_sz2
    }

    fn get_input_offset(&self) -> u16 {
        self.get_input_buffer_size()
    }

    fn get_lookahead_size(&self) -> u16 {
        1 << self.lookahead_sz2
    }

    fn can_take_byte(&self, oi: &OutputInfo<'_>) -> bool {
        *oi.output_size < oi.buf.len()
    }

    fn do_indexing(&mut self) {
        let input_offset = self.get_input_offset();
        let end = input_offset + self.input_size;
        let mut last = [0i16; 256];
        for v in last.iter_mut() {
            *v = -1;
        }

        for i in 0..end as usize {
            let v = self.buffer[i] as usize;
            let lv = last[v];
            self.search_index[i] = lv;
            last[v] = i as i16;
        }
    }

    fn st_step_search(&mut self) -> HseState {
        let window_length = self.get_input_buffer_size();
        let lookahead_sz = self.get_lookahead_size();
        let msi = self.match_scan_index;
        let fin = self.is_finishing();

        if msi > self.input_size - if fin { 1 } else { lookahead_sz } {
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
        if self.input_size - msi < lookahead_sz {
            max_possible = self.input_size - msi;
        }

        let (match_pos, match_length) = self.find_longest_match(start, end, max_possible);

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

    fn st_yield_tag_bit(&mut self, out_buf: &mut [u8], output_size: &mut usize) -> HseState {
        let mut oi = OutputInfo {
            buf: out_buf,
            output_size,
        };
        if self.can_take_byte(&oi) {
            if self.match_length == 0 {
                self.add_tag_bit(&mut oi, LITERAL_MARKER);
                HseState::YieldLiteral
            } else {
                self.add_tag_bit(&mut oi, BACKREF_MARKER);
                self.outgoing_bits = self.match_pos - 1;
                self.outgoing_bits_count = self.window_sz2;
                HseState::YieldBrIndex
            }
        } else {
            HseState::YieldTagBit
        }
    }

    fn st_yield_literal(&mut self, out_buf: &mut [u8], output_size: &mut usize) -> HseState {
        let mut oi = OutputInfo {
            buf: out_buf,
            output_size,
        };
        if self.can_take_byte(&oi) {
            self.push_literal_byte(&mut oi);
            HseState::Search
        } else {
            HseState::YieldLiteral
        }
    }

    fn st_yield_br_index(&mut self, out_buf: &mut [u8], output_size: &mut usize) -> HseState {
        let mut oi = OutputInfo {
            buf: out_buf,
            output_size,
        };
        if self.can_take_byte(&oi) {
            if self.push_outgoing_bits(&mut oi) > 0 {
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

    fn st_yield_br_length(&mut self, out_buf: &mut [u8], output_size: &mut usize) -> HseState {
        let mut oi = OutputInfo {
            buf: out_buf,
            output_size,
        };
        if self.can_take_byte(&oi) {
            if self.push_outgoing_bits(&mut oi) > 0 {
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

    fn st_flush_bit_buffer(&mut self, out_buf: &mut [u8], output_size: &mut usize) -> HseState {
        if self.bit_index == 0x80 {
            HseState::Done
        } else {
            let mut oi = OutputInfo {
                buf: out_buf,
                output_size,
            };
            if self.can_take_byte(&oi) {
                oi.buf[*oi.output_size] = self.current_byte;
                *oi.output_size += 1;
                HseState::Done
            } else {
                HseState::FlushBits
            }
        }
    }

    fn add_tag_bit(&mut self, oi: &mut OutputInfo<'_>, tag: u8) {
        self.push_bits(1, tag, oi);
    }

    fn find_longest_match(
        &self,
        start: u16,
        end: u16,
        maxlen: u16,
    ) -> (u16, u16) {
        let buf = &self.buffer;
        let mut match_maxlen: u16 = 0;
        let mut match_index = MATCH_NOT_FOUND;
        let needlepoint = end as usize;
        let start_i = start as i16;

        let mut pos = self.search_index[end as usize];
        while pos - start_i >= 0 {
            let pos_usize = pos as usize;
            let pospoint = &buf[pos_usize..];
            let needle = &buf[needlepoint..];

            if pospoint[match_maxlen as usize] != needle[match_maxlen as usize] {
                pos = self.search_index[pos_usize];
                continue;
            }

            let mut len: u16 = 1;
            while len < maxlen {
                if pospoint[len as usize] != needle[len as usize] {
                    break;
                }
                len += 1;
            }

            if len > match_maxlen {
                match_maxlen = len;
                match_index = pos as u16;
                if len == maxlen {
                    break;
                }
            }
            pos = self.search_index[pos_usize];
        }

        let break_even_point =
            1u16 + self.window_sz2 as u16 + self.lookahead_sz2 as u16;

        if match_maxlen > break_even_point / 8 {
            (end - match_index, match_maxlen)
        } else {
            (MATCH_NOT_FOUND, 0)
        }
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
                if self.bit_index == 0x00 {
                    self.bit_index = 0x80;
                    oi.buf[*oi.output_size] = self.current_byte;
                    *oi.output_size += 1;
                    self.current_byte = 0x00;
                }
            }
        }
    }

    fn push_literal_byte(&mut self, oi: &mut OutputInfo<'_>) {
        let processed_offset = self.match_scan_index - 1;
        let input_offset = self.get_input_offset() + processed_offset;
        let c = self.buffer[input_offset as usize];
        self.push_bits(8, c, oi);
    }

    fn save_backlog(&mut self) {
        let input_buf_sz = self.get_input_buffer_size();
        let msi = self.match_scan_index;
        let rem = input_buf_sz - msi;
        let shift_sz = input_buf_sz + rem;

        let src_start = (input_buf_sz - rem) as usize;
        let shift = shift_sz as usize;
        self.buffer.copy_within(src_start..src_start + shift, 0);

        self.match_scan_index = 0;
        self.input_size -= input_buf_sz - rem;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode_all(window: u8, lookahead: u8, input: &[u8]) -> Vec<u8> {
        let mut enc = Encoder::alloc(window, lookahead).unwrap();
        let mut sunk = 0usize;
        while sunk < input.len() {
            let mut n = 0usize;
            assert_eq!(enc.sink(&input[sunk..], &mut n), SinkRes::Ok);
            sunk += n;
        }
        assert_eq!(enc.finish(), FinishRes::More);
        let mut out = Vec::new();
        loop {
            let mut chunk = [0u8; 1024];
            let mut n = 0usize;
            let pres = enc.poll(&mut chunk, &mut n);
            out.extend_from_slice(&chunk[..n]);
            if pres == PollRes::Empty {
                if enc.finish() == FinishRes::Done {
                    break;
                }
            }
        }
        out
    }

    #[test]
    fn literals_no_repetition() {
        let input: Vec<u8> = (0..5).collect();
        let out = encode_all(8, 7, &input);
        assert_eq!(out, &[0x80, 0x40, 0x60, 0x50, 0x38, 0x20]);
    }

    #[test]
    fn same_byte_series() {
        let input = b"aaaaa";
        let out = encode_all(8, 7, input);
        assert_eq!(out, &[0xb0, 0x80, 0x01, 0x80]);
    }

    #[test]
    fn repeated_substring() {
        let input = b"abcdabcd";
        let out = encode_all(8, 3, input);
        assert_eq!(out, &[0xb0, 0xd8, 0xac, 0x76, 0x40, 0x1b]);
    }
}
