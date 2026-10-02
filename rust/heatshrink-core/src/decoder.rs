use crate::common::valid_window_lookahead;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i8)]
pub enum DecoderSinkRes {
    Ok = 0,
    Full = 1,
    ErrorNull = -1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i8)]
pub enum DecoderPollRes {
    Empty = 0,
    More = 1,
    ErrorNull = -1,
    ErrorUnknown = -2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i8)]
pub enum DecoderFinishRes {
    Done = 0,
    More = 1,
    ErrorNull = -1,
}

const NO_BITS: u16 = u16::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HsdState {
    TagBit,
    YieldLiteral,
    BackrefIndexMsb,
    BackrefIndexLsb,
    BackrefCountMsb,
    BackrefCountLsb,
    YieldBackref,
}

struct OutputInfo<'a> {
    buf: &'a mut [u8],
    output_size: &'a mut usize,
}

pub struct Decoder {
    input_size: u16,
    input_index: u16,
    output_count: u16,
    output_index: u16,
    head_index: u16,
    state: HsdState,
    current_byte: u8,
    bit_index: u8,
    window_sz2: u8,
    lookahead_sz2: u8,
    input_buffer_size: u16,
    buffers: Vec<u8>,
}

impl Decoder {
    pub fn alloc(input_buffer_size: u16, window_sz2: u8, lookahead_sz2: u8) -> Option<Self> {
        if input_buffer_size == 0 || !valid_window_lookahead(window_sz2, lookahead_sz2) {
            return None;
        }
        let buffers_sz = (1usize << window_sz2) + input_buffer_size as usize;
        let mut dec = Self {
            input_size: 0,
            input_index: 0,
            output_count: 0,
            output_index: 0,
            head_index: 0,
            state: HsdState::TagBit,
            current_byte: 0,
            bit_index: 0,
            window_sz2,
            lookahead_sz2,
            input_buffer_size,
            buffers: vec![0; buffers_sz],
        };
        dec.reset();
        Some(dec)
    }

    pub fn reset(&mut self) {
        self.buffers.fill(0);
        self.state = HsdState::TagBit;
        self.input_size = 0;
        self.input_index = 0;
        self.bit_index = 0;
        self.current_byte = 0;
        self.output_count = 0;
        self.output_index = 0;
        self.head_index = 0;
    }

    pub fn input_size_field(&self) -> u16 {
        self.input_size
    }

    pub fn input_index_field(&self) -> u16 {
        self.input_index
    }

    pub fn sink(&mut self, in_buf: &[u8]) -> (DecoderSinkRes, usize) {
        let rem = self.input_buffer_size - self.input_size;
        if rem == 0 {
            return (DecoderSinkRes::Full, 0);
        }

        // Compare in usize: `len as u16` wraps multiples of 65536 to 0 and sinks nothing.
        let size = (rem as usize).min(in_buf.len()) as u16;
        let end = self.input_size as usize + size as usize;
        self.buffers[self.input_size as usize..end].copy_from_slice(&in_buf[..size as usize]);
        self.input_size += size;
        (DecoderSinkRes::Ok, size as usize)
    }

    pub fn poll(&mut self, out_buf: &mut [u8]) -> (DecoderPollRes, usize) {
        let mut output_size = 0usize;

        loop {
            let in_state = self.state;
            match in_state {
                HsdState::TagBit => self.state = self.st_tag_bit(),
                HsdState::YieldLiteral => {
                    self.state = self.st_yield_literal(&mut OutputInfo {
                        buf: out_buf,
                        output_size: &mut output_size,
                    });
                }
                HsdState::BackrefIndexMsb => self.state = self.st_backref_index_msb(),
                HsdState::BackrefIndexLsb => self.state = self.st_backref_index_lsb(),
                HsdState::BackrefCountMsb => self.state = self.st_backref_count_msb(),
                HsdState::BackrefCountLsb => self.state = self.st_backref_count_lsb(),
                HsdState::YieldBackref => {
                    self.state = self.st_yield_backref(&mut OutputInfo {
                        buf: out_buf,
                        output_size: &mut output_size,
                    });
                }
            }

            if self.state == in_state {
                if output_size == out_buf.len() {
                    return (DecoderPollRes::More, output_size);
                }
                return (DecoderPollRes::Empty, output_size);
            }
        }
    }

    pub fn finish(&mut self) -> DecoderFinishRes {
        match self.state {
            HsdState::TagBit => {
                if self.input_size == 0 {
                    DecoderFinishRes::Done
                } else {
                    DecoderFinishRes::More
                }
            }
            HsdState::BackrefIndexLsb
            | HsdState::BackrefIndexMsb
            | HsdState::BackrefCountLsb
            | HsdState::BackrefCountMsb => {
                if self.input_size == 0 {
                    DecoderFinishRes::Done
                } else {
                    DecoderFinishRes::More
                }
            }
            HsdState::YieldLiteral => {
                if self.input_size == 0 {
                    DecoderFinishRes::Done
                } else {
                    DecoderFinishRes::More
                }
            }
            _ => DecoderFinishRes::More,
        }
    }

    fn input_buffer_size(&self) -> u16 {
        self.input_buffer_size
    }

    fn backref_count_bits(&self) -> u8 {
        self.lookahead_sz2
    }

    fn backref_index_bits(&self) -> u8 {
        self.window_sz2
    }

    fn st_tag_bit(&mut self) -> HsdState {
        let bits = self.get_bits(1);
        if bits == NO_BITS {
            HsdState::TagBit
        } else if bits != 0 {
            HsdState::YieldLiteral
        } else if self.window_sz2 > 8 {
            HsdState::BackrefIndexMsb
        } else {
            self.output_index = 0;
            HsdState::BackrefIndexLsb
        }
    }

    fn st_yield_literal(&mut self, oi: &mut OutputInfo<'_>) -> HsdState {
        if *oi.output_size < oi.buf.len() {
            let byte = self.get_bits(8);
            if byte == NO_BITS {
                return HsdState::YieldLiteral;
            }
            let ibs = self.input_buffer_size() as usize;
            let buf = &mut self.buffers[ibs..];
            let mask = (1 << self.window_sz2) - 1;
            let c = (byte & 0xFF) as u8;
            buf[(self.head_index & mask) as usize] = c;
            self.head_index = self.head_index.wrapping_add(1);
            oi.buf[*oi.output_size] = c;
            *oi.output_size += 1;
            HsdState::TagBit
        } else {
            HsdState::YieldLiteral
        }
    }

    fn st_backref_index_msb(&mut self) -> HsdState {
        let bit_ct = self.backref_index_bits();
        debug_assert!(bit_ct > 8);
        let bits = self.get_bits(bit_ct - 8);
        if bits == NO_BITS {
            HsdState::BackrefIndexMsb
        } else {
            self.output_index = bits << 8;
            HsdState::BackrefIndexLsb
        }
    }

    fn st_backref_index_lsb(&mut self) -> HsdState {
        let bit_ct = self.backref_index_bits();
        let n = if bit_ct < 8 { bit_ct } else { 8 };
        let bits = self.get_bits(n);
        if bits == NO_BITS {
            HsdState::BackrefIndexLsb
        } else {
            self.output_index |= bits;
            self.output_index += 1;
            self.output_count = 0;
            let br_bit_ct = self.backref_count_bits();
            if br_bit_ct > 8 {
                HsdState::BackrefCountMsb
            } else {
                HsdState::BackrefCountLsb
            }
        }
    }

    fn st_backref_count_msb(&mut self) -> HsdState {
        let br_bit_ct = self.backref_count_bits();
        debug_assert!(br_bit_ct > 8);
        let bits = self.get_bits(br_bit_ct - 8);
        if bits == NO_BITS {
            HsdState::BackrefCountMsb
        } else {
            self.output_count = bits << 8;
            HsdState::BackrefCountLsb
        }
    }

    fn st_backref_count_lsb(&mut self) -> HsdState {
        let br_bit_ct = self.backref_count_bits();
        let n = if br_bit_ct < 8 { br_bit_ct } else { 8 };
        let bits = self.get_bits(n);
        if bits == NO_BITS {
            HsdState::BackrefCountLsb
        } else {
            self.output_count |= bits;
            self.output_count += 1;
            HsdState::YieldBackref
        }
    }

    fn st_yield_backref(&mut self, oi: &mut OutputInfo<'_>) -> HsdState {
        let mut count = oi.buf.len() - *oi.output_size;
        if count > 0 {
            // Promote the remaining count: `count as u16` wraps a 64KiB poll and skips the clamp.
            if (self.output_count as usize) < count {
                count = self.output_count as usize;
            }
            let ibs = self.input_buffer_size() as usize;
            let mask = (1 << self.window_sz2) - 1;
            let neg_offset = self.output_index;

            for _ in 0..count {
                let c = self.buffers
                    [ibs + ((self.head_index.wrapping_sub(neg_offset) & mask) as usize)];
                oi.buf[*oi.output_size] = c;
                *oi.output_size += 1;
                self.buffers[ibs + (self.head_index & mask) as usize] = c;
                self.head_index = self.head_index.wrapping_add(1);
            }
            self.output_count -= count as u16;
            if self.output_count == 0 {
                HsdState::TagBit
            } else {
                HsdState::YieldBackref
            }
        } else {
            HsdState::YieldBackref
        }
    }

    fn get_bits(&mut self, count: u8) -> u16 {
        if count > 15 {
            return NO_BITS;
        }

        if self.input_size == 0 && self.bit_index < (1 << (count - 1)) {
            return NO_BITS;
        }

        let mut accumulator = 0u16;
        for _ in 0..count {
            if self.bit_index == 0 {
                if self.input_size == 0 {
                    return NO_BITS;
                }
                self.current_byte = self.buffers[self.input_index as usize];
                if self.input_index + 1 == self.input_size {
                    self.input_index = 0;
                    self.input_size = 0;
                } else {
                    self.input_index += 1;
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

pub fn decode_all(
    input: &[u8],
    input_buffer_size: u16,
    window_sz2: u8,
    lookahead_sz2: u8,
) -> Option<Vec<u8>> {
    let mut hsd = Decoder::alloc(input_buffer_size, window_sz2, lookahead_sz2)?;
    let mut out = Vec::with_capacity(input.len() * 2);
    let mut scratch = [0u8; 4096];

    let mut sunk = 0usize;
    while sunk < input.len() {
        let (sres, n) = hsd.sink(&input[sunk..]);
        if matches!(sres, DecoderSinkRes::Full) && n == 0 {
            break;
        }
        sunk += n;

        loop {
            let (pres, w) = hsd.poll(&mut scratch);
            out.extend_from_slice(&scratch[..w]);
            if pres != DecoderPollRes::More {
                break;
            }
        }
    }

    loop {
        match hsd.finish() {
            DecoderFinishRes::Done => break,
            DecoderFinishRes::More => loop {
                let (pres, w) = hsd.poll(&mut scratch);
                out.extend_from_slice(&scratch[..w]);
                if pres != DecoderPollRes::More {
                    break;
                }
            },
            DecoderFinishRes::ErrorNull => break,
        }
    }

    Some(out)
}
