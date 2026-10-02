//! Encoder — ports `heatshrink_encoder.c` (dynamic alloc + index).

use crate::{
    Error, BACKREF_MARKER, LITERAL_MARKER, MAX_WINDOW_BITS, MIN_LOOKAHEAD_BITS, MIN_WINDOW_BITS,
};

const FLAG_IS_FINISHING: u8 = 0x01;
const MATCH_NOT_FOUND: u16 = 0xffff;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

struct OutputInfo<'a> {
    buf: &'a mut [u8],
    output_size: usize,
}

impl OutputInfo<'_> {
    fn can_take_byte(&self) -> bool {
        self.output_size < self.buf.len()
    }
}

/// Heatshrink encoder state machine (owned buffers; matches dynamic C layout).
pub struct Encoder {
    window_sz2: u8,
    lookahead_sz2: u8,
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
    /// Search index: size field then `buf_sz` i16 entries (matches `struct hs_index`).
    search_index: Vec<i16>,
    /// Input buffer + sliding window: `2 << window_sz2` bytes.
    buffer: Vec<u8>,
}

impl Encoder {
    pub fn alloc(window_sz2: u8, lookahead_sz2: u8) -> Result<Self, Error> {
        if !(MIN_WINDOW_BITS..=MAX_WINDOW_BITS).contains(&window_sz2)
            || lookahead_sz2 < MIN_LOOKAHEAD_BITS
            || lookahead_sz2 >= window_sz2
        {
            return Err(Error::InvalidConfig);
        }
        let buf_sz = 2usize << window_sz2;
        let mut enc = Self {
            window_sz2,
            lookahead_sz2,
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
            // C allocates index_sz = buf_sz * sizeof(uint16_t) then treats as int16_t[]
            search_index: vec![0; buf_sz],
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

    fn input_buffer_size(&self) -> u16 {
        1u16 << self.window_sz2
    }

    fn lookahead_size(&self) -> u16 {
        1u16 << self.lookahead_sz2
    }

    fn input_offset(&self) -> u16 {
        self.input_buffer_size()
    }

    fn is_finishing(&self) -> bool {
        self.flags & FLAG_IS_FINISHING != 0
    }

    pub fn sink(&mut self, in_buf: &[u8]) -> Result<(EncoderSinkRes, usize), Error> {
        if self.is_finishing() {
            return Ok((EncoderSinkRes::ErrorMisuse, 0));
        }
        if self.state != State::NotFull {
            return Ok((EncoderSinkRes::ErrorMisuse, 0));
        }
        let write_offset = self.input_offset() as usize + self.input_size as usize;
        let ibs = self.input_buffer_size() as usize;
        let rem = ibs - self.input_size as usize;
        let cp_sz = rem.min(in_buf.len());
        self.buffer[write_offset..write_offset + cp_sz].copy_from_slice(&in_buf[..cp_sz]);
        self.input_size += cp_sz as u16;
        if cp_sz == rem {
            self.state = State::Filled;
        }
        Ok((EncoderSinkRes::Ok, cp_sz))
    }

    pub fn poll(&mut self, out_buf: &mut [u8]) -> Result<(EncoderPollRes, usize), Error> {
        if out_buf.is_empty() {
            return Ok((EncoderPollRes::ErrorMisuse, 0));
        }
        let mut oi = OutputInfo {
            buf: out_buf,
            output_size: 0,
        };
        loop {
            let in_state = self.state;
            match in_state {
                State::NotFull => return Ok((EncoderPollRes::Empty, oi.output_size)),
                State::Filled => {
                    self.do_indexing();
                    self.state = State::Search;
                }
                State::Search => {
                    self.state = self.st_step_search();
                }
                State::YieldTagBit => {
                    self.state = self.st_yield_tag_bit(&mut oi);
                }
                State::YieldLiteral => {
                    self.state = self.st_yield_literal(&mut oi);
                }
                State::YieldBrIndex => {
                    self.state = self.st_yield_br_index(&mut oi);
                }
                State::YieldBrLength => {
                    self.state = self.st_yield_br_length(&mut oi);
                }
                State::SaveBacklog => {
                    self.state = self.st_save_backlog();
                }
                State::FlushBits => {
                    // Match C fall-through: missing `break` after FLUSH_BITS
                    // (heatshrink_encoder.c:236-239).
                    self.state = self.st_flush_bit_buffer(&mut oi);
                    return Ok((EncoderPollRes::Empty, oi.output_size));
                }
                State::Done => return Ok((EncoderPollRes::Empty, oi.output_size)),
            }
            if self.state == in_state && oi.output_size == oi.buf.len() {
                return Ok((EncoderPollRes::More, oi.output_size));
            }
        }
    }

    pub fn finish(&mut self) -> EncoderFinishRes {
        self.flags |= FLAG_IS_FINISHING;
        if self.state == State::NotFull {
            self.state = State::Filled;
        }
        if self.state == State::Done {
            EncoderFinishRes::Done
        } else {
            EncoderFinishRes::More
        }
    }

    fn do_indexing(&mut self) {
        let mut last = [-1i16; 256];
        let input_offset = self.input_offset() as usize;
        let end = input_offset + self.input_size as usize;
        for i in 0..end {
            let v = self.buffer[i] as usize;
            let lv = last[v];
            self.search_index[i] = lv;
            last[v] = i as i16;
        }
    }

    fn st_step_search(&mut self) -> State {
        let window_length = self.input_buffer_size();
        let lookahead_sz = self.lookahead_size();
        let msi = self.match_scan_index;
        let fin = self.is_finishing();
        // Match C usual-arithmetic-conversions in heatshrink_encoder.c:268:
        // `msi > input_size - (fin ? 1 : lookahead_sz)` is signed int math, so
        // empty finishing (0 - 1) yields -1 and immediately flushes.
        let threshold = if fin { 1i32 } else { i32::from(lookahead_sz) };
        let limit = i32::from(self.input_size) - threshold;
        if i32::from(msi) > limit {
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
        if self.input_size - msi < lookahead_sz {
            max_possible = self.input_size - msi;
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

    fn find_longest_match(&self, start: u16, end: u16, maxlen: u16, match_length: &mut u16) -> u16 {
        let buf = &self.buffer;
        let mut match_maxlen = 0u16;
        let mut match_index = MATCH_NOT_FOUND;
        let needlepoint_base = end as usize;
        let mut pos = self.search_index[end as usize];

        while pos - start as i16 >= 0 {
            let pos_usize = pos as usize;
            if buf[pos_usize + match_maxlen as usize]
                != buf[needlepoint_base + match_maxlen as usize]
            {
                pos = self.search_index[pos_usize];
                continue;
            }
            let mut len = 1u16;
            while len < maxlen {
                if buf[pos_usize + len as usize] != buf[needlepoint_base + len as usize] {
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
        let input_buf_sz = self.input_buffer_size() as usize;
        let msi = self.match_scan_index as usize;
        let rem = input_buf_sz - msi;
        let shift_sz = input_buf_sz + rem;
        let src_start = input_buf_sz - rem;
        // memmove overlapping regions
        let tmp: Vec<u8> = self.buffer[src_start..src_start + shift_sz].to_vec();
        self.buffer[..shift_sz].copy_from_slice(&tmp);
        self.match_scan_index = 0;
        self.input_size -= (input_buf_sz - rem) as u16;
    }
}

#[doc(hidden)]
pub mod internals {
    // Placeholder for white-box ports if needed.
}
