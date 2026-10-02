//! Safe heatshrink LZSS codec — logic only, no `unsafe`.

#![forbid(unsafe_code)]

mod decoder;
mod encoder;
mod error;

pub use decoder::{Decoder, DecoderFinish, DecoderPoll, DecoderSink};
pub use encoder::{Encoder, EncoderFinish, EncoderPoll, EncoderSink};
pub use error::{Error, Result};

pub const VERSION_MAJOR: u8 = 0;
pub const VERSION_MINOR: u8 = 4;
pub const VERSION_PATCH: u8 = 1;

pub const MIN_WINDOW_BITS: u8 = 4;
pub const MAX_WINDOW_BITS: u8 = 15;
pub const MIN_LOOKAHEAD_BITS: u8 = 3;

pub const LITERAL_MARKER: u8 = 0x01;
pub const BACKREF_MARKER: u8 = 0x00;

/// Whether the search index is enabled (matches `HEATSHRINK_USE_INDEX=1`).
pub const USE_INDEX: bool = true;
