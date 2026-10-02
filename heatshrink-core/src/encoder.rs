//! Encoder — port of `heatshrink_encoder.c` (dynamic alloc + index).

use crate::error::{Error, Result};
use crate::{
    BACKREF_MARKER, LITERAL_MARKER, MAX_WINDOW_BITS, MIN_LOOKAHEAD_BITS, MIN_WINDOW_BITS, USE_INDEX,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncoderSink {
    Ok,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncoderPoll {
    Empty,
    More,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncoderFinish {
    Done,
    More,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum State {
    NotFull = 0,
    Filled = 1,
    Search = 2,
    YieldTagBit = 3,
    YieldLiteral = 4,
    YieldBrIndex = 5,
    YieldBrLength = 6,
    SaveBacklog = 7,
    FlushBits = 8,
    Done = 9,
}

const FLAG_IS_FINISHING: u8 = 0x01;
const MATCH_NOT_FOUND: u16 = 0xffff;

struct OutputInfo<'a> {
    buf: &'a mut [u8],
    output_size: usize,
}

impl OutputInfo<'_> {
    fn can_take_byte(&self) -> bool {
        self.output_size < self.buf.len()
    }
}

pub struct Encoder {
    input_size: u16,
    match_scan_index: u16,
    match_length: u16,
    match_pos: u16,
    outgoing_bits: u16,
    outgoing_bits_count: u8,
    flags: u8,
    state: State,
    current_byte: u8,
    bit_index: u8,
    window_sz2: u8,
    lookahead_sz2: u8,
    search_index: Vec<i16>,
    buffer: Vec<u8>,
}

impl Encoder {
    pub fn alloc(window_sz2: u8, lookahead_sz2: u8) -> Result<Self> {
        if !(MIN_WINDOW_BITS..=MAX_WINDOW_BITS).contains(&window_sz2)
            || lookahead_sz2 < MIN_LOOKAHEAD_BITS
            || lookahead_sz2 >= window_sz2
        {
            return Err(Error::InvalidConfig);
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
            state: State::NotFull,
            current_byte: 0,
            bit_index: 0x80,
            window_sz2,
            lookahead_sz2,
            search_index: if USE_INDEX {
                vec![0; buf_sz]
            } else {
                Vec::new()
            },
            buffer: vec![0; buf_sz],
        };
        enc.reset();
        Ok(enc)
    }

    pub fn reset(&mut self) {
        self.buffer.fill(0);
        self.input_size = 0;
        self.state = State::NotFull;
        self.match_scan_index = 0;
        self.flags = 0;
        self.bit_index = 0x80;
        self.current_byte = 0x00;
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

    pub fn input_size(&self) -> u16 {
        self.input_size
    }
    pub fn match_scan_index(&self) -> u16 {
        self.match_scan_index
    }
    pub fn match_length(&self) -> u16 {
        self.match_length
    }
    pub fn match_pos(&self) -> u16 {
        self.match_pos
    }
    pub fn outgoing_bits(&self) -> u16 {
        self.outgoing_bits
    }
    pub fn outgoing_bits_count(&self) -> u8 {
        self.outgoing_bits_count
    }
    pub fn flags(&self) -> u8 {
        self.flags
    }
    pub fn state_u8(&self) -> u8 {
        self.state as u8
    }
    pub fn current_byte(&self) -> u8 {
        self.current_byte
    }
    pub fn bit_index(&self) -> u8 {
        self.bit_index
    }

    fn input_buffer_size(&self) -> u16 {
        1u16 << self.window_sz2
    }

    fn input_offset(&self) -> u16 {
        self.input_buffer_size()
    }

    fn lookahead_size(&self) -> u16 {
        1u16 << self.lookahead_sz2
    }

    fn is_finishing(&self) -> bool {
        self.flags & FLAG_IS_FINISHING != 0
    }

    pub fn sink(&mut self, in_buf: &[u8]) -> Result<(EncoderSink, usize)> {
        if self.is_finishing() {
            return Err(Error::Misuse);
        }
        if self.state != State::NotFull {
            return Err(Error::Misuse);
        }
        let write_offset = self.input_offset() as usize + self.input_size as usize;
        let ibs = self.input_buffer_size();
        let rem = ibs - self.input_size;
        let cp_sz = (rem as usize).min(in_buf.len());
        self.buffer[write_offset..write_offset + cp_sz].copy_from_slice(&in_buf[..cp_sz]);
        self.input_size += cp_sz as u16;
        if cp_sz as u16 == rem {
            self.state = State::Filled;
        }
        Ok((EncoderSink::Ok, cp_sz))
    }

    pub fn poll(&mut self, out_buf: &mut [u8]) -> Result<(EncoderPoll, usize)> {
        if out_buf.is_empty() {
            return Err(Error::Misuse);
        }
        let mut oi = OutputInfo {
            buf: out_buf,
            output_size: 0,
        };
        loop {
            let in_state = self.state;
            match in_state {
                State::NotFull => return Ok((EncoderPoll::Empty, oi.output_size)),
                State::Filled => {
                    self.do_indexing();
                    self.state = State::Search;
                }
                State::Search => self.state = self.st_step_search(),
                State::YieldTagBit => self.state = self.st_yield_tag_bit(&mut oi),
                State::YieldLiteral => self.state = self.st_yield_literal(&mut oi),
                State::YieldBrIndex => self.state = self.st_yield_br_index(&mut oi),
                State::YieldBrLength => self.state = self.st_yield_br_length(&mut oi),
                State::SaveBacklog => self.state = self.st_save_backlog(),
                State::FlushBits => {
                    // Match C fall-through: after flush, always return EMPTY
                    // (heatshrink_encoder.c poll switch lacks `break` before DONE).
                    self.state = self.st_flush_bit_buffer(&mut oi);
                    return Ok((EncoderPoll::Empty, oi.output_size));
                }
                State::Done => return Ok((EncoderPoll::Empty, oi.output_size)),
            }
            if self.state == in_state && oi.output_size == oi.buf.len() {
                return Ok((EncoderPoll::More, oi.output_size));
            }
        }
    }

    pub fn finish(&mut self) -> Result<EncoderFinish> {
        self.flags |= FLAG_IS_FINISHING;
        if self.state == State::NotFull {
            self.state = State::Filled;
        }
        Ok(if self.state == State::Done {
            EncoderFinish::Done
        } else {
            EncoderFinish::More
        })
    }

    fn st_step_search(&mut self) -> State {
        let window_length = self.input_buffer_size();
        let lookahead_sz = self.lookahead_size();
        let msi = self.match_scan_index;
        let fin = self.is_finishing();
        // C compares after integer promotions to `int`:
        //   msi > input_size - (fin ? 1 : lookahead_sz)
        // so empty finishing yields `0 > -1` → FLUSH_BITS
        // (heatshrink_encoder.c:st_step_search).
        let threshold: i32 = if fin { 1 } else { i32::from(lookahead_sz) };
        if i32::from(msi) > i32::from(self.input_size) - threshold {
            return if fin {
                State::FlushBits
            } else {
                State::SaveBacklog
            };
        }
        let input_offset = self.input_offset();
        let end = input_offset + msi;
        let start = end - window_length;
        let mut max_possible = lookahead_sz;
        let remain = self.input_size.wrapping_sub(msi);
        if remain < lookahead_sz {
            max_possible = remain;
        }
        let mut match_length = 0u16;
        let match_pos = self.find_longest_match(start, end, max_possible, &mut match_length);
        if match_pos == MATCH_NOT_FOUND {
            self.match_scan_index += 1;
            self.match_length = 0;
            State::YieldTagBit
        } else {
            self.match_pos = match_pos;
            self.match_length = match_length;
            State::YieldTagBit
        }
    }

    fn st_yield_tag_bit(&mut self, oi: &mut OutputInfo<'_>) -> State {
        if oi.can_take_byte() {
            if self.match_length == 0 {
                self.add_tag_bit(oi, LITERAL_MARKER);
                State::YieldLiteral
            } else {
                self.add_tag_bit(oi, BACKREF_MARKER);
                self.outgoing_bits = self.match_pos - 1;
                self.outgoing_bits_count = self.window_sz2;
                State::YieldBrIndex
            }
        } else {
            State::YieldTagBit
        }
    }

    fn st_yield_literal(&mut self, oi: &mut OutputInfo<'_>) -> State {
        if oi.can_take_byte() {
            self.push_literal_byte(oi);
            State::Search
        } else {
            State::YieldLiteral
        }
    }

    fn st_yield_br_index(&mut self, oi: &mut OutputInfo<'_>) -> State {
        if oi.can_take_byte() {
            if self.push_outgoing_bits(oi) > 0 {
                State::YieldBrIndex
            } else {
                self.outgoing_bits = self.match_length - 1;
                self.outgoing_bits_count = self.lookahead_sz2;
                State::YieldBrLength
            }
        } else {
            State::YieldBrIndex
        }
    }

    fn st_yield_br_length(&mut self, oi: &mut OutputInfo<'_>) -> State {
        if oi.can_take_byte() {
            if self.push_outgoing_bits(oi) > 0 {
                State::YieldBrLength
            } else {
                self.match_scan_index += self.match_length;
                self.match_length = 0;
                State::Search
            }
        } else {
            State::YieldBrLength
        }
    }

    fn st_save_backlog(&mut self) -> State {
        self.save_backlog();
        State::NotFull
    }

    fn st_flush_bit_buffer(&mut self, oi: &mut OutputInfo<'_>) -> State {
        if self.bit_index == 0x80 {
            State::Done
        } else if oi.can_take_byte() {
            oi.buf[oi.output_size] = self.current_byte;
            oi.output_size += 1;
            State::Done
        } else {
            State::FlushBits
        }
    }

    fn add_tag_bit(&mut self, oi: &mut OutputInfo<'_>, tag: u8) {
        self.push_bits(1, tag, oi);
    }

    fn do_indexing(&mut self) {
        if !USE_INDEX {
            return;
        }
        let mut last = [-1i16; 256];
        let input_offset = self.input_offset();
        let end = input_offset + self.input_size;
        for i in 0..end {
            let v = self.buffer[i as usize];
            let lv = last[v as usize];
            self.search_index[i as usize] = lv;
            last[v as usize] = i as i16;
        }
    }

    fn find_longest_match(&self, start: u16, end: u16, maxlen: u16, match_length: &mut u16) -> u16 {
        let buf = &self.buffer;
        let mut match_maxlen = 0u16;
        let mut match_index = MATCH_NOT_FOUND;
        let needlepoint = end as usize;
        // C compares after integer promotions to `int`:
        //   pos - (int16_t)start >= 0
        // so `pos == i16::MIN` (the index slot `do_indexing` stores for
        // offset 32768) ends the walk. Subtracting in `i16` overflows.
        // (heatshrink_encoder.c:find_longest_match).
        let start_i = i32::from(start as i16);

        if USE_INDEX {
            let mut pos = self.search_index[end as usize];
            while i32::from(pos) - start_i >= 0 {
                let pospoint = pos as usize;
                if buf[pospoint + match_maxlen as usize] != buf[needlepoint + match_maxlen as usize]
                {
                    pos = self.search_index[pos as usize];
                    continue;
                }
                let mut len = 1u16;
                while len < maxlen {
                    if buf[pospoint + len as usize] != buf[needlepoint + len as usize] {
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
                pos = self.search_index[pos as usize];
            }
        } else {
            // C: `int16_t pos = end - 1` subtracts in `int`, then narrows.
            let mut pos = (i32::from(end) - 1) as i16;
            while i32::from(pos) - start_i >= 0 {
                let pospoint = pos as usize;
                if buf[pospoint + match_maxlen as usize] == buf[needlepoint + match_maxlen as usize]
                    && buf[pospoint] == buf[needlepoint]
                {
                    let mut len = 1u16;
                    while len < maxlen {
                        if buf[pospoint + len as usize] != buf[needlepoint + len as usize] {
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
                }
                pos -= 1;
            }
        }

        let break_even_point = 1u16 + u16::from(self.window_sz2) + u16::from(self.lookahead_sz2);
        if match_maxlen > break_even_point / 8 {
            *match_length = match_maxlen;
            end - match_index
        } else {
            MATCH_NOT_FOUND
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
        if count == 8 && self.bit_index == 0x80 {
            oi.buf[oi.output_size] = bits;
            oi.output_size += 1;
        } else {
            for i in (0..count).rev() {
                let bit = bits & (1 << i) != 0;
                if bit {
                    self.current_byte |= self.bit_index;
                }
                self.bit_index >>= 1;
                if self.bit_index == 0x00 {
                    self.bit_index = 0x80;
                    oi.buf[oi.output_size] = self.current_byte;
                    oi.output_size += 1;
                    self.current_byte = 0x00;
                }
            }
        }
    }

    fn push_literal_byte(&mut self, oi: &mut OutputInfo<'_>) {
        let processed_offset = self.match_scan_index - 1;
        let input_offset = self.input_offset() + processed_offset;
        let c = self.buffer[input_offset as usize];
        self.push_bits(8, c, oi);
    }

    fn save_backlog(&mut self) {
        let input_buf_sz = self.input_buffer_size();
        let msi = self.match_scan_index;
        let rem = input_buf_sz - msi;
        let shift_sz = (input_buf_sz + rem) as usize;
        let src = (input_buf_sz - rem) as usize;
        self.buffer.copy_within(src..src + shift_sz, 0);
        self.match_scan_index = 0;
        self.input_size -= input_buf_sz - rem;
    }
}
