#![forbid(unsafe_code)]
#![deny(warnings)]

mod common;
mod decoder;
mod encoder;

pub use common::{
    HEATSHRINK_AUTHOR, HEATSHRINK_BACKREF_MARKER, HEATSHRINK_LITERAL_MARKER,
    HEATSHRINK_MAX_WINDOW_BITS, HEATSHRINK_MIN_LOOKAHEAD_BITS, HEATSHRINK_MIN_WINDOW_BITS,
    HEATSHRINK_URL, HEATSHRINK_VERSION_MAJOR, HEATSHRINK_VERSION_MINOR, HEATSHRINK_VERSION_PATCH,
};
pub use decoder::{decode_all, Decoder, DecoderFinishRes, DecoderPollRes, DecoderSinkRes};
pub use encoder::{encode_all, Encoder, EncoderFinishRes, EncoderPollRes, EncoderSinkRes};
