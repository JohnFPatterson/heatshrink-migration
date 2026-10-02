//! Decoder — port of `heatshrink_decoder.c` (dynamic alloc).

use crate::error::{Error, Result};
use crate::{MAX_WINDOW_BITS, MIN_LOOKAHEAD_BITS, MIN_WINDOW_BITS};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecoderSink {
    Ok,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecoderPoll {
    Empty,
    More,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecoderFinish {
    Done,
    More,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum State {
    TagBit = 0,
    YieldLiteral = 1,
    BackrefIndexMsb = 2,
    BackrefIndexLsb = 3,
    BackrefCountMsb = 4,
    BackrefCountLsb = 5,
    YieldBackref = 6,
}

const NO_BITS: u16 = 0xffff;

struct OutputInfo<'a> {
    buf: &'a mut [u8],
    output_size: usize,
}

pub struct Decoder {
    input_size: u16,
    input_index: u16,
    output_count: u16,
    output_index: u16,
    head_index: u16,
    state: State,
    current_byte: u8,
    bit_index: u8,
    window_sz2: u8,
    lookahead_sz2: u8,
    input_buffer_size: u16,
    /// Input buffer, then expansion window buffer (same layout as C `buffers[]`).
    buffers: Vec<u8>,
}

impl Decoder {
    pub fn alloc(input_buffer_size: u16, window_sz2: u8, lookahead_sz2: u8) -> Result<Self> {
        if !(MIN_WINDOW_BITS..=MAX_WINDOW_BITS).contains(&window_sz2)
            || input_buffer_size == 0
            || lookahead_sz2 < MIN_LOOKAHEAD_BITS
            || lookahead_sz2 >= window_sz2
        {
            return Err(Error::InvalidConfig);
        }
        let buffers_sz = (1usize << window_sz2) + input_buffer_size as usize;
        let mut dec = Self {
            input_size: 0,
            input_index: 0,
            output_count: 0,
            output_index: 0,
            head_index: 0,
            state: State::TagBit,
            current_byte: 0,
            bit_index: 0,
            window_sz2,
            lookahead_sz2,
            input_buffer_size,
            buffers: vec![0; buffers_sz],
        };
        dec.reset();
        Ok(dec)
    }

    pub fn reset(&mut self) {
        let buf_sz = 1usize << self.window_sz2;
        let input_sz = self.input_buffer_size as usize;
        self.buffers[..buf_sz + input_sz].fill(0);
        self.state = State::TagBit;
        self.input_size = 0;
        self.input_index = 0;
        self.bit_index = 0x00;
        self.current_byte = 0x00;
        self.output_count = 0;
        self.output_index = 0;
        self.head_index = 0;
    }

    pub fn window_bits(&self) -> u8 {
        self.window_sz2
    }

    pub fn lookahead_bits(&self) -> u8 {
        self.lookahead_sz2
    }

    pub fn input_buffer_size(&self) -> u16 {
        self.input_buffer_size
    }

    /// Snapshot of fields that appear before `buffers[]` in the C header.
    pub fn abi_header(&self) -> (u16, u16, u16, u16, u16, u8, u8, u8, u8, u8, u16) {
        (
            self.input_size,
            self.input_index,
            self.output_count,
            self.output_index,
            self.head_index,
            self.state as u8,
            self.current_byte,
            self.bit_index,
            self.window_sz2,
            self.lookahead_sz2,
            self.input_buffer_size,
        )
    }

    pub fn sink(&mut self, in_buf: &[u8]) -> Result<(DecoderSink, usize)> {
        let rem = self.input_buffer_size as usize - self.input_size as usize;
        if rem == 0 {
            return Ok((DecoderSink::Full, 0));
        }
        let size = rem.min(in_buf.len());
        let start = self.input_size as usize;
        self.buffers[start..start + size].copy_from_slice(&in_buf[..size]);
        self.input_size += size as u16;
        Ok((DecoderSink::Ok, size))
    }

    pub fn poll(&mut self, out_buf: &mut [u8]) -> Result<(DecoderPoll, usize)> {
        let mut oi = OutputInfo {
            buf: out_buf,
            output_size: 0,
        };
        loop {
            let in_state = self.state;
            match in_state {
                State::TagBit => self.state = self.st_tag_bit(),
                State::YieldLiteral => self.state = self.st_yield_literal(&mut oi),
                State::BackrefIndexMsb => self.state = self.st_backref_index_msb(),
                State::BackrefIndexLsb => self.state = self.st_backref_index_lsb(),
                State::BackrefCountMsb => self.state = self.st_backref_count_msb(),
                State::BackrefCountLsb => self.state = self.st_backref_count_lsb(),
                State::YieldBackref => self.state = self.st_yield_backref(&mut oi),
            }
            if self.state == in_state {
                if oi.output_size == oi.buf.len() {
                    return Ok((DecoderPoll::More, oi.output_size));
                }
                return Ok((DecoderPoll::Empty, oi.output_size));
            }
        }
    }

    pub fn finish(&self) -> Result<DecoderFinish> {
        match self.state {
            State::TagBit
            | State::BackrefIndexLsb
            | State::BackrefIndexMsb
            | State::BackrefCountLsb
            | State::BackrefCountMsb
            | State::YieldLiteral => Ok(if self.input_size == 0 {
                DecoderFinish::Done
            } else {
                DecoderFinish::More
            }),
            _ => Ok(DecoderFinish::More),
        }
    }

    fn st_tag_bit(&mut self) -> State {
        let bits = self.get_bits(1);
        if bits == NO_BITS {
            State::TagBit
        } else if bits != 0 {
            State::YieldLiteral
        } else if self.window_sz2 > 8 {
            State::BackrefIndexMsb
        } else {
            self.output_index = 0;
            State::BackrefIndexLsb
        }
    }

    fn st_yield_literal(&mut self, oi: &mut OutputInfo<'_>) -> State {
        if oi.output_size < oi.buf.len() {
            let byte = self.get_bits(8);
            if byte == NO_BITS {
                return State::YieldLiteral;
            }
            let win_start = self.input_buffer_size as usize;
            let mask = (1u16 << self.window_sz2) - 1;
            let c = (byte & 0xff) as u8;
            let idx = (self.head_index & mask) as usize;
            self.buffers[win_start + idx] = c;
            self.head_index = self.head_index.wrapping_add(1);
            oi.buf[oi.output_size] = c;
            oi.output_size += 1;
            State::TagBit
        } else {
            State::YieldLiteral
        }
    }

    fn st_backref_index_msb(&mut self) -> State {
        let bit_ct = self.window_sz2;
        let bits = self.get_bits(bit_ct - 8);
        if bits == NO_BITS {
            return State::BackrefIndexMsb;
        }
        self.output_index = bits << 8;
        State::BackrefIndexLsb
    }

    fn st_backref_index_lsb(&mut self) -> State {
        let bit_ct = self.window_sz2;
        let bits = self.get_bits(if bit_ct < 8 { bit_ct } else { 8 });
        if bits == NO_BITS {
            return State::BackrefIndexLsb;
        }
        self.output_index |= bits;
        self.output_index += 1;
        self.output_count = 0;
        if self.lookahead_sz2 > 8 {
            State::BackrefCountMsb
        } else {
            State::BackrefCountLsb
        }
    }

    fn st_backref_count_msb(&mut self) -> State {
        let br_bit_ct = self.lookahead_sz2;
        let bits = self.get_bits(br_bit_ct - 8);
        if bits == NO_BITS {
            return State::BackrefCountMsb;
        }
        self.output_count = bits << 8;
        State::BackrefCountLsb
    }

    fn st_backref_count_lsb(&mut self) -> State {
        let br_bit_ct = self.lookahead_sz2;
        let bits = self.get_bits(if br_bit_ct < 8 { br_bit_ct } else { 8 });
        if bits == NO_BITS {
            return State::BackrefCountLsb;
        }
        self.output_count |= bits;
        self.output_count += 1;
        State::YieldBackref
    }

    fn st_yield_backref(&mut self, oi: &mut OutputInfo<'_>) -> State {
        let mut count = oi.buf.len() - oi.output_size;
        if count > 0 {
            if (self.output_count as usize) < count {
                count = self.output_count as usize;
            }
            let win_start = self.input_buffer_size as usize;
            let mask = (1u16 << self.window_sz2) - 1;
            let neg_offset = self.output_index;
            for _ in 0..count {
                let src = ((self.head_index.wrapping_sub(neg_offset)) & mask) as usize;
                let c = self.buffers[win_start + src];
                oi.buf[oi.output_size] = c;
                oi.output_size += 1;
                let dst = (self.head_index & mask) as usize;
                self.buffers[win_start + dst] = c;
                self.head_index = self.head_index.wrapping_add(1);
            }
            self.output_count -= count as u16;
            if self.output_count == 0 {
                return State::TagBit;
            }
        }
        State::YieldBackref
    }

    fn get_bits(&mut self, count: u8) -> u16 {
        if count > 15 {
            return NO_BITS;
        }
        if self.input_size == 0 && self.bit_index < (1 << (count - 1)) {
            return NO_BITS;
        }
        let mut accumulator: u16 = 0;
        for _ in 0..count {
            if self.bit_index == 0x00 {
                if self.input_size == 0 {
                    return NO_BITS;
                }
                self.current_byte = self.buffers[self.input_index as usize];
                self.input_index += 1;
                if self.input_index == self.input_size {
                    self.input_index = 0;
                    self.input_size = 0;
                }
                self.bit_index = 0x80;
            }
            accumulator <<= 1;
            if self.current_byte & self.bit_index != 0 {
                accumulator |= 0x01;
            }
            self.bit_index >>= 1;
        }
        accumulator
    }
}
