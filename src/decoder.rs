use crate::common::{MIN_LOOKAHEAD_BITS, MIN_WINDOW_BITS, MAX_WINDOW_BITS, NO_BITS};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i8)]
pub enum SinkRes {
    Ok = 0,
    Full = 1,
    ErrorNull = -1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i8)]
pub enum PollRes {
    Empty = 0,
    More = 1,
    ErrorNull = -1,
    ErrorUnknown = -2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i8)]
pub enum FinishRes {
    Done = 0,
    More = 1,
    ErrorNull = -1,
}

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

pub struct Decoder {
    window_sz2: u8,
    lookahead_sz2: u8,
    input_buffer_size: u16,
    buffers: Vec<u8>,
    input_size: u16,
    input_index: u16,
    output_count: u16,
    output_index: u16,
    head_index: u16,
    state: HsdState,
    current_byte: u8,
    bit_index: u8,
}

impl Decoder {
    pub fn alloc(input_buffer_size: u16, window_sz2: u8, lookahead_sz2: u8) -> Option<Self> {
        if window_sz2 < MIN_WINDOW_BITS
            || window_sz2 > MAX_WINDOW_BITS
            || input_buffer_size == 0
            || lookahead_sz2 < MIN_LOOKAHEAD_BITS
            || lookahead_sz2 >= window_sz2
        {
            return None;
        }
        let buffers_sz = (1usize << window_sz2) + input_buffer_size as usize;
        let mut dec = Self {
            window_sz2,
            lookahead_sz2,
            input_buffer_size,
            buffers: vec![0; buffers_sz],
            input_size: 0,
            input_index: 0,
            output_count: 0,
            output_index: 0,
            head_index: 0,
            state: HsdState::TagBit,
            current_byte: 0,
            bit_index: 0x00,
        };
        dec.reset();
        Some(dec)
    }

    pub fn reset(&mut self) {
        self.buffers.fill(0);
        self.state = HsdState::TagBit;
        self.input_size = 0;
        self.input_index = 0;
        self.bit_index = 0x00;
        self.current_byte = 0x00;
        self.output_count = 0;
        self.output_index = 0;
        self.head_index = 0;
    }

    pub fn input_size_field(&self) -> u16 {
        self.input_size
    }

    pub fn input_index(&self) -> u16 {
        self.input_index
    }

    pub fn sink(&mut self, in_buf: &[u8], input_size: &mut usize) -> SinkRes {
        let rem = self.input_buffer_size - self.input_size;
        if rem == 0 {
            *input_size = 0;
            return SinkRes::Full;
        }

        let size = rem.min(in_buf.len() as u16);
        let sz = size as usize;
        let start = self.input_size as usize;
        self.buffers[start..start + sz].copy_from_slice(&in_buf[..sz]);
        self.input_size += size;
        *input_size = sz;
        SinkRes::Ok
    }

    pub fn poll(&mut self, out_buf: &mut [u8], output_size: &mut usize) -> PollRes {
        *output_size = 0;

        loop {
            let in_state = self.state;
            self.state = match in_state {
                HsdState::TagBit => self.st_tag_bit(),
                HsdState::YieldLiteral => self.st_yield_literal(out_buf, output_size),
                HsdState::BackrefIndexMsb => self.st_backref_index_msb(),
                HsdState::BackrefIndexLsb => self.st_backref_index_lsb(),
                HsdState::BackrefCountMsb => self.st_backref_count_msb(),
                HsdState::BackrefCountLsb => self.st_backref_count_lsb(),
                HsdState::YieldBackref => self.st_yield_backref(out_buf, output_size),
            };

            if self.state == in_state {
                if *output_size == out_buf.len() {
                    return PollRes::More;
                }
                return PollRes::Empty;
            }
        }
    }

    pub fn finish(&self) -> FinishRes {
        match self.state {
            HsdState::TagBit => {
                if self.input_size == 0 {
                    FinishRes::Done
                } else {
                    FinishRes::More
                }
            }
            HsdState::BackrefIndexLsb
            | HsdState::BackrefIndexMsb
            | HsdState::BackrefCountLsb
            | HsdState::BackrefCountMsb => {
                if self.input_size == 0 {
                    FinishRes::Done
                } else {
                    FinishRes::More
                }
            }
            HsdState::YieldLiteral => {
                if self.input_size == 0 {
                    FinishRes::Done
                } else {
                    FinishRes::More
                }
            }
            _ => FinishRes::More,
        }
    }

    fn backref_count_bits(&self) -> u8 {
        self.lookahead_sz2
    }

    fn backref_index_bits(&self) -> u8 {
        self.window_sz2
    }

    fn window_base(&self) -> usize {
        self.input_buffer_size as usize
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

    fn st_yield_literal(&mut self, out_buf: &mut [u8], output_size: &mut usize) -> HsdState {
        if *output_size < out_buf.len() {
            let byte = self.get_bits(8);
            if byte == NO_BITS {
                return HsdState::YieldLiteral;
            }
            let mask = (1u16 << self.window_sz2) - 1;
            let c = (byte & 0xFF) as u8;
            let wbase = self.window_base();
            let idx = self.head_index & mask;
            self.buffers[wbase + idx as usize] = c;
            self.head_index = self.head_index.wrapping_add(1);
            self.push_byte(out_buf, output_size, c);
            HsdState::TagBit
        } else {
            HsdState::YieldLiteral
        }
    }

    fn st_backref_index_msb(&mut self) -> HsdState {
        debug_assert!(self.backref_index_bits() > 8);
        let bit_ct = self.backref_index_bits();
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
            let br_bit_ct = self.backref_count_bits();
            self.output_count = 0;
            if br_bit_ct > 8 {
                HsdState::BackrefCountMsb
            } else {
                HsdState::BackrefCountLsb
            }
        }
    }

    fn st_backref_count_msb(&mut self) -> HsdState {
        debug_assert!(self.backref_count_bits() > 8);
        let br_bit_ct = self.backref_count_bits();
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

    fn st_yield_backref(&mut self, out_buf: &mut [u8], output_size: &mut usize) -> HsdState {
        let mut count = out_buf.len() - *output_size;
        if count > 0 {
            if self.output_count < count as u16 {
                count = self.output_count as usize;
            }
            let mask = (1u16 << self.window_sz2) - 1;
            let neg_offset = self.output_index;
            let wbase = self.window_base();

            for _ in 0..count {
                let idx = (self.head_index.wrapping_sub(neg_offset) & mask) as usize;
                let c = self.buffers[wbase + idx];
                self.push_byte(out_buf, output_size, c);
                let widx = (self.head_index & mask) as usize;
                self.buffers[wbase + widx] = c;
                self.head_index = self.head_index.wrapping_add(1);
            }
            self.output_count -= count as u16;
            if self.output_count == 0 {
                return HsdState::TagBit;
            }
        }
        HsdState::YieldBackref
    }

    fn get_bits(&mut self, count: u8) -> u16 {
        if count > 15 {
            return NO_BITS;
        }

        if self.input_size == 0 {
            if self.bit_index < (1 << (count - 1)) {
                return NO_BITS;
            }
        }

        let mut accumulator: u16 = 0;
        for _ in 0..count {
            if self.bit_index == 0x00 {
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

    fn push_byte(&self, out_buf: &mut [u8], output_size: &mut usize, byte: u8) {
        out_buf[*output_size] = byte;
        *output_size += 1;
    }
}

pub fn roundtrip(window: u8, lookahead: u8, input_buffer_size: u16, data: &[u8]) -> Option<Vec<u8>> {
    use crate::encoder::{Encoder, FinishRes as EFin, PollRes as EPoll, SinkRes as ESink};

    let mut enc = Encoder::alloc(window, lookahead)?;
    let mut compressed = Vec::with_capacity(data.len() + data.len() / 2 + 4);
    let mut sunk = 0usize;
    while sunk < data.len() {
        let mut count = 0usize;
        if enc.sink(&data[sunk..], &mut count) != ESink::Ok {
            return None;
        }
        sunk += count;
        if sunk == data.len() && enc.finish() != EFin::More {
            return None;
        }

        loop {
            let mut chunk = [0u8; 4096];
            let mut out_count = 0usize;
            let pres = enc.poll(&mut chunk, &mut out_count);
            if pres == EPoll::ErrorMisuse || pres == EPoll::ErrorNull {
                return None;
            }
            compressed.extend_from_slice(&chunk[..out_count]);
            if pres != EPoll::More {
                break;
            }
        }

        if sunk == data.len() && enc.finish() == EFin::Done {
            break;
        }
    }

    let mut dec = Decoder::alloc(input_buffer_size, window, lookahead)?;
    let mut out = Vec::with_capacity(data.len() + data.len() / 2 + 4);
    let mut sunk = 0usize;
    while sunk < compressed.len() {
        let mut count = 0usize;
        if dec.sink(&compressed[sunk..], &mut count) != SinkRes::Ok {
            return None;
        }
        sunk += count;
        if sunk == compressed.len() && dec.finish() != FinishRes::More {
            return None;
        }

        loop {
            let mut chunk = [0u8; 4096];
            let mut out_count = 0usize;
            let pres = dec.poll(&mut chunk, &mut out_count);
            if pres == PollRes::ErrorNull || pres == PollRes::ErrorUnknown {
                return None;
            }
            out.extend_from_slice(&chunk[..out_count]);
            if pres != PollRes::More {
                break;
            }
        }

        if sunk == compressed.len() && dec.finish() == FinishRes::Done {
            break;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloc_validation() {
        assert!(Decoder::alloc(256, MIN_WINDOW_BITS - 1, 4).is_none());
        assert!(Decoder::alloc(0, MIN_WINDOW_BITS, MIN_WINDOW_BITS - 1).is_none());
        assert!(Decoder::alloc(256, MIN_WINDOW_BITS, MIN_WINDOW_BITS).is_none());
    }

    #[test]
    fn sink_full() {
        let mut dec = Decoder::alloc(1, MIN_WINDOW_BITS, MIN_WINDOW_BITS - 1).unwrap();
        let input = [0u8, 1, 2, 3, 4, 5];
        let mut count = 0usize;
        assert_eq!(dec.sink(&input, &mut count), SinkRes::Ok);
        assert_eq!(count, 1);
        assert_eq!(dec.sink(&input[1..], &mut count), SinkRes::Full);
        assert_eq!(count, 0);
    }

    #[test]
    fn expand_aaaaa() {
        let input = [0xb0u8, 0x80, 0x01, 0x80];
        let mut dec = Decoder::alloc(256, 8, 7).unwrap();
        let mut count = 0usize;
        assert_eq!(dec.sink(&input, &mut count), SinkRes::Ok);
        let mut output = [0u8; 6];
        let mut out_sz = 0usize;
        assert_eq!(dec.poll(&mut output, &mut out_sz), PollRes::Empty);
        assert_eq!(out_sz, 5);
        assert_eq!(&output[..5], b"aaaaa");
    }

    #[test]
    fn roundtrip_pseudo_random() {
        let mut data = vec![0u8; 512];
        let mut rn: u64 = 9223372036854775783;
        let seed: u64 = 0xdeadbeef;
        for b in data.iter_mut() {
            rn = rn.wrapping_mul(seed).wrapping_add(seed);
            *b = ((rn % 26) + b'a' as u64) as u8;
        }
        let out = roundtrip(8, 4, 32, &data).unwrap();
        assert_eq!(out, data);
    }
}
