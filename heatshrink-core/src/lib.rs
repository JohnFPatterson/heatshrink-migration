//! Safe heatshrink compression / decompression core.
//! Ports `heatshrink_encoder.c` and `heatshrink_decoder.c` with `HEATSHRINK_DYNAMIC_ALLOC=1`
//! and `HEATSHRINK_USE_INDEX=1`.

#![forbid(unsafe_code)]

mod decoder;
mod encoder;
mod error;

pub use decoder::{Decoder, DecoderFinishRes, DecoderPollRes, DecoderSinkRes};
pub use encoder::{Encoder, EncoderFinishRes, EncoderPollRes, EncoderSinkRes};
pub use error::Error;

pub const MIN_WINDOW_BITS: u8 = 4;
pub const MAX_WINDOW_BITS: u8 = 15;
pub const MIN_LOOKAHEAD_BITS: u8 = 3;
pub const LITERAL_MARKER: u8 = 0x01;
pub const BACKREF_MARKER: u8 = 0x00;

/// Hidden internals for white-box tests ported from C static helpers.
#[doc(hidden)]
pub mod internals {
    pub use crate::decoder::internals as decoder;
    pub use crate::encoder::internals as encoder;
}
