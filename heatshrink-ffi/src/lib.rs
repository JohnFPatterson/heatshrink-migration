//! Thin C ABI for heatshrink. All library logic lives in `heatshrink-core`.

#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]
#![allow(non_camel_case_types)] // C ABI type names match the public headers.

use heatshrink_core::{
    Decoder, DecoderFinish, DecoderPoll, DecoderSink, Encoder, EncoderFinish, EncoderPoll,
};

/// C-layout-compatible encoder handle. Field offsets through `lookahead_sz2`
/// match `heatshrink_encoder` in `heatshrink_encoder.h` (dynamic alloc).
/// The flexible `buffer[]` / search index are owned inside `inner`.
#[repr(C)]
pub struct heatshrink_encoder {
    pub input_size: u16,
    pub match_scan_index: u16,
    pub match_length: u16,
    pub match_pos: u16,
    pub outgoing_bits: u16,
    pub outgoing_bits_count: u8,
    pub flags: u8,
    pub state: u8,
    pub current_byte: u8,
    pub bit_index: u8,
    pub window_sz2: u8,
    pub lookahead_sz2: u8,
    // C would place search_index pointer + buffer[] here when USE_INDEX=1.
    // Callers that only use the functional API do not depend on those offsets.
    inner: Encoder,
}

/// C-layout-compatible decoder handle. Field offsets through
/// `input_buffer_size` match `heatshrink_decoder` in `heatshrink_decoder.h`.
#[repr(C)]
pub struct heatshrink_decoder {
    pub input_size: u16,
    pub input_index: u16,
    pub output_count: u16,
    pub output_index: u16,
    pub head_index: u16,
    pub state: u8,
    pub current_byte: u8,
    pub bit_index: u8,
    pub window_sz2: u8,
    pub lookahead_sz2: u8,
    pub input_buffer_size: u16,
    inner: Decoder,
}

impl heatshrink_encoder {
    fn sync_from_inner(&mut self) {
        self.input_size = self.inner.input_size();
        self.match_scan_index = self.inner.match_scan_index();
        self.match_length = self.inner.match_length();
        self.match_pos = self.inner.match_pos();
        self.outgoing_bits = self.inner.outgoing_bits();
        self.outgoing_bits_count = self.inner.outgoing_bits_count();
        self.flags = self.inner.flags();
        self.state = self.inner.state_u8();
        self.current_byte = self.inner.current_byte();
        self.bit_index = self.inner.bit_index();
        self.window_sz2 = self.inner.window_bits();
        self.lookahead_sz2 = self.inner.lookahead_bits();
    }
}

impl heatshrink_decoder {
    fn sync_from_inner(&mut self) {
        let (
            input_size,
            input_index,
            output_count,
            output_index,
            head_index,
            state,
            current_byte,
            bit_index,
            window_sz2,
            lookahead_sz2,
            input_buffer_size,
        ) = self.inner.abi_header();
        self.input_size = input_size;
        self.input_index = input_index;
        self.output_count = output_count;
        self.output_index = output_index;
        self.head_index = head_index;
        self.state = state;
        self.current_byte = current_byte;
        self.bit_index = bit_index;
        self.window_sz2 = window_sz2;
        self.lookahead_sz2 = lookahead_sz2;
        self.input_buffer_size = input_buffer_size;
    }
}

pub type HSE_sink_res = i32;
pub const HSER_SINK_OK: HSE_sink_res = 0;
pub const HSER_SINK_ERROR_NULL: HSE_sink_res = -1;
pub const HSER_SINK_ERROR_MISUSE: HSE_sink_res = -2;

pub type HSE_poll_res = i32;
pub const HSER_POLL_EMPTY: HSE_poll_res = 0;
pub const HSER_POLL_MORE: HSE_poll_res = 1;
pub const HSER_POLL_ERROR_NULL: HSE_poll_res = -1;
pub const HSER_POLL_ERROR_MISUSE: HSE_poll_res = -2;

pub type HSE_finish_res = i32;
pub const HSER_FINISH_DONE: HSE_finish_res = 0;
pub const HSER_FINISH_MORE: HSE_finish_res = 1;
pub const HSER_FINISH_ERROR_NULL: HSE_finish_res = -1;

pub type HSD_sink_res = i32;
pub const HSDR_SINK_OK: HSD_sink_res = 0;
pub const HSDR_SINK_FULL: HSD_sink_res = 1;
pub const HSDR_SINK_ERROR_NULL: HSD_sink_res = -1;

pub type HSD_poll_res = i32;
pub const HSDR_POLL_EMPTY: HSD_poll_res = 0;
pub const HSDR_POLL_MORE: HSD_poll_res = 1;
pub const HSDR_POLL_ERROR_NULL: HSD_poll_res = -1;
pub const HSDR_POLL_ERROR_UNKNOWN: HSD_poll_res = -2;

pub type HSD_finish_res = i32;
pub const HSDR_FINISH_DONE: HSD_finish_res = 0;
pub const HSDR_FINISH_MORE: HSD_finish_res = 1;
pub const HSDR_FINISH_ERROR_NULL: HSD_finish_res = -1;

/// Allocate an encoder. Returns null on invalid config or OOM.
///
/// # Safety
/// Safe to call; returns an owned pointer that must be freed with
/// `heatshrink_encoder_free`.
#[no_mangle]
pub unsafe extern "C" fn heatshrink_encoder_alloc(
    window_sz2: u8,
    lookahead_sz2: u8,
) -> *mut heatshrink_encoder {
    match Encoder::alloc(window_sz2, lookahead_sz2) {
        Ok(inner) => {
            let mut h = heatshrink_encoder {
                input_size: 0,
                match_scan_index: 0,
                match_length: 0,
                match_pos: 0,
                outgoing_bits: 0,
                outgoing_bits_count: 0,
                flags: 0,
                state: 0,
                current_byte: 0,
                bit_index: 0x80,
                window_sz2,
                lookahead_sz2,
                inner,
            };
            h.sync_from_inner();
            Box::into_raw(Box::new(h))
        }
        Err(_) => std::ptr::null_mut(),
    }
}

/// Free an encoder from `heatshrink_encoder_alloc`.
///
/// # Safety
/// `hse` must be null or a pointer from `heatshrink_encoder_alloc` that has
/// not already been freed.
#[no_mangle]
pub unsafe extern "C" fn heatshrink_encoder_free(hse: *mut heatshrink_encoder) {
    if hse.is_null() {
        return;
    }
    // SAFETY: caller passes alloc'd pointer exclusive ownership.
    unsafe {
        drop(Box::from_raw(hse));
    }
}

/// Reset encoder state.
///
/// # Safety
/// `hse` must be a valid encoder from `heatshrink_encoder_alloc`.
#[no_mangle]
pub unsafe extern "C" fn heatshrink_encoder_reset(hse: *mut heatshrink_encoder) {
    if hse.is_null() {
        return;
    }
    // SAFETY: non-null encoder pointer owned by caller.
    unsafe {
        (*hse).inner.reset();
        (*hse).sync_from_inner();
    }
}

/// Sink input bytes into the encoder.
///
/// # Safety
/// `hse`, `in_buf`, and `input_size` must be valid (or null → ERROR_NULL).
/// `in_buf` must point to at least `size` bytes.
#[no_mangle]
pub unsafe extern "C" fn heatshrink_encoder_sink(
    hse: *mut heatshrink_encoder,
    in_buf: *mut u8,
    size: usize,
    input_size: *mut usize,
) -> HSE_sink_res {
    if hse.is_null() || in_buf.is_null() || input_size.is_null() {
        return HSER_SINK_ERROR_NULL;
    }
    // SAFETY: pointers checked non-null; `in_buf` has `size` bytes per contract.
    unsafe {
        let slice = std::slice::from_raw_parts(in_buf, size);
        match (*hse).inner.sink(slice) {
            Ok((_, n)) => {
                *input_size = n;
                (*hse).sync_from_inner();
                HSER_SINK_OK
            }
            Err(_) => HSER_SINK_ERROR_MISUSE,
        }
    }
}

/// Poll compressed output from the encoder.
///
/// # Safety
/// `hse`, `out_buf`, `output_size` non-null (or ERROR_NULL). `out_buf` must
/// have `out_buf_size` writable bytes.
#[no_mangle]
pub unsafe extern "C" fn heatshrink_encoder_poll(
    hse: *mut heatshrink_encoder,
    out_buf: *mut u8,
    out_buf_size: usize,
    output_size: *mut usize,
) -> HSE_poll_res {
    if hse.is_null() || out_buf.is_null() || output_size.is_null() {
        return HSER_POLL_ERROR_NULL;
    }
    // SAFETY: pointers checked; out buffer sized by caller.
    unsafe {
        let slice = std::slice::from_raw_parts_mut(out_buf, out_buf_size);
        match (*hse).inner.poll(slice) {
            Ok((EncoderPoll::Empty, n)) => {
                *output_size = n;
                (*hse).sync_from_inner();
                HSER_POLL_EMPTY
            }
            Ok((EncoderPoll::More, n)) => {
                *output_size = n;
                (*hse).sync_from_inner();
                HSER_POLL_MORE
            }
            Err(_) => HSER_POLL_ERROR_MISUSE,
        }
    }
}

/// Notify the encoder that input is finished.
///
/// # Safety
/// `hse` null → ERROR_NULL; else valid encoder pointer.
#[no_mangle]
pub unsafe extern "C" fn heatshrink_encoder_finish(hse: *mut heatshrink_encoder) -> HSE_finish_res {
    if hse.is_null() {
        return HSER_FINISH_ERROR_NULL;
    }
    // SAFETY: non-null encoder.
    unsafe {
        match (*hse).inner.finish() {
            Ok(EncoderFinish::Done) => {
                (*hse).sync_from_inner();
                HSER_FINISH_DONE
            }
            Ok(EncoderFinish::More) => {
                (*hse).sync_from_inner();
                HSER_FINISH_MORE
            }
            Err(_) => HSER_FINISH_ERROR_NULL,
        }
    }
}

/// Allocate a decoder. Returns null on invalid config or OOM.
///
/// # Safety
/// Safe to call; returns owned pointer freed with `heatshrink_decoder_free`.
#[no_mangle]
pub unsafe extern "C" fn heatshrink_decoder_alloc(
    input_buffer_size: u16,
    expansion_buffer_sz2: u8,
    lookahead_sz2: u8,
) -> *mut heatshrink_decoder {
    match Decoder::alloc(input_buffer_size, expansion_buffer_sz2, lookahead_sz2) {
        Ok(inner) => {
            let mut h = heatshrink_decoder {
                input_size: 0,
                input_index: 0,
                output_count: 0,
                output_index: 0,
                head_index: 0,
                state: 0,
                current_byte: 0,
                bit_index: 0,
                window_sz2: expansion_buffer_sz2,
                lookahead_sz2,
                input_buffer_size,
                inner,
            };
            h.sync_from_inner();
            Box::into_raw(Box::new(h))
        }
        Err(_) => std::ptr::null_mut(),
    }
}

/// Free a decoder from `heatshrink_decoder_alloc`.
///
/// # Safety
/// `hsd` null or from `heatshrink_decoder_alloc` and not already freed.
#[no_mangle]
pub unsafe extern "C" fn heatshrink_decoder_free(hsd: *mut heatshrink_decoder) {
    if hsd.is_null() {
        return;
    }
    // SAFETY: alloc'd decoder pointer.
    unsafe {
        drop(Box::from_raw(hsd));
    }
}

/// Reset decoder state.
///
/// # Safety
/// `hsd` must be a valid decoder from `heatshrink_decoder_alloc`.
#[no_mangle]
pub unsafe extern "C" fn heatshrink_decoder_reset(hsd: *mut heatshrink_decoder) {
    if hsd.is_null() {
        return;
    }
    // SAFETY: non-null decoder.
    unsafe {
        (*hsd).inner.reset();
        (*hsd).sync_from_inner();
    }
}

/// Sink compressed bytes into the decoder.
///
/// # Safety
/// Null-checked; `in_buf` has `size` bytes.
#[no_mangle]
pub unsafe extern "C" fn heatshrink_decoder_sink(
    hsd: *mut heatshrink_decoder,
    in_buf: *mut u8,
    size: usize,
    input_size: *mut usize,
) -> HSD_sink_res {
    if hsd.is_null() || in_buf.is_null() || input_size.is_null() {
        return HSDR_SINK_ERROR_NULL;
    }
    // SAFETY: pointers validated.
    unsafe {
        let slice = std::slice::from_raw_parts(in_buf, size);
        match (*hsd).inner.sink(slice) {
            Ok((DecoderSink::Ok, n)) => {
                *input_size = n;
                (*hsd).sync_from_inner();
                HSDR_SINK_OK
            }
            Ok((DecoderSink::Full, n)) => {
                *input_size = n;
                (*hsd).sync_from_inner();
                HSDR_SINK_FULL
            }
            Err(_) => HSDR_SINK_ERROR_NULL,
        }
    }
}

/// Poll decompressed output from the decoder.
///
/// # Safety
/// Null-checked; `out_buf` has `out_buf_size` bytes.
#[no_mangle]
pub unsafe extern "C" fn heatshrink_decoder_poll(
    hsd: *mut heatshrink_decoder,
    out_buf: *mut u8,
    out_buf_size: usize,
    output_size: *mut usize,
) -> HSD_poll_res {
    if hsd.is_null() || out_buf.is_null() || output_size.is_null() {
        return HSDR_POLL_ERROR_NULL;
    }
    // SAFETY: pointers validated.
    unsafe {
        let slice = std::slice::from_raw_parts_mut(out_buf, out_buf_size);
        match (*hsd).inner.poll(slice) {
            Ok((DecoderPoll::Empty, n)) => {
                *output_size = n;
                (*hsd).sync_from_inner();
                HSDR_POLL_EMPTY
            }
            Ok((DecoderPoll::More, n)) => {
                *output_size = n;
                (*hsd).sync_from_inner();
                HSDR_POLL_MORE
            }
            Err(_) => HSDR_POLL_ERROR_UNKNOWN,
        }
    }
}

/// Notify the decoder that input is finished.
///
/// # Safety
/// `hsd` null → ERROR_NULL; else valid decoder.
#[no_mangle]
pub unsafe extern "C" fn heatshrink_decoder_finish(hsd: *mut heatshrink_decoder) -> HSD_finish_res {
    if hsd.is_null() {
        return HSDR_FINISH_ERROR_NULL;
    }
    // SAFETY: non-null decoder.
    unsafe {
        match (*hsd).inner.finish() {
            Ok(DecoderFinish::Done) => {
                (*hsd).sync_from_inner();
                HSDR_FINISH_DONE
            }
            Ok(DecoderFinish::More) => {
                (*hsd).sync_from_inner();
                HSDR_FINISH_MORE
            }
            Err(_) => HSDR_FINISH_ERROR_NULL,
        }
    }
}
