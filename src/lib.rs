//! heatshrink — incremental LZSS compression for embedded and streaming use.
//!
//! Rust port of the C library by Scott Vokes (atomicobject/heatshrink).

pub mod common;
pub mod decoder;
pub mod encoder;

pub use common::*;
pub use decoder::{
    Decoder, FinishRes as DecoderFinishRes, PollRes as DecoderPollRes, SinkRes as DecoderSinkRes,
};
pub use encoder::{
    Encoder, FinishRes as EncoderFinishRes, PollRes as EncoderPollRes, SinkRes as EncoderSinkRes,
};
