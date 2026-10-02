//! C ABI for heatshrink — thin shim over `heatshrink-core`.
//! Every `unsafe` block has a `SAFETY:` comment.

#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]

use core::ptr;
use heatshrink_core::{
    Decoder, DecoderFinishRes, DecoderPollRes, DecoderSinkRes, Encoder, EncoderFinishRes,
    EncoderPollRes, EncoderSinkRes,
};
use std::os::raw::c_void;

/// Opaque encoder handle matching C `heatshrink_encoder *` usage at the API level.
pub struct HeatshrinkEncoder {
    inner: Encoder,
}

/// Opaque decoder handle.
pub struct HeatshrinkDecoder {
    inner: Decoder,
}

#[no_mangle]
pub extern "C" fn heatshrink_encoder_alloc(
    window_sz2: u8,
    lookahead_sz2: u8,
) -> *mut HeatshrinkEncoder {
    match Encoder::alloc(window_sz2, lookahead_sz2) {
        Ok(inner) => Box::into_raw(Box::new(HeatshrinkEncoder { inner })),
        Err(_) => ptr::null_mut(),
    }
}

/// Free an encoder allocated by [`heatshrink_encoder_alloc`].
///
/// # Safety
/// `hse` must be null or a pointer from `heatshrink_encoder_alloc` not already freed.
#[no_mangle]
pub unsafe extern "C" fn heatshrink_encoder_free(hse: *mut HeatshrinkEncoder) {
    if hse.is_null() {
        return;
    }
    // SAFETY: caller passes a pointer from alloc, or null (handled above).
    unsafe {
        drop(Box::from_raw(hse));
    }
}

/// Reset an encoder to its initial state.
///
/// # Safety
/// `hse` must be a valid encoder from `heatshrink_encoder_alloc`.
#[no_mangle]
pub unsafe extern "C" fn heatshrink_encoder_reset(hse: *mut HeatshrinkEncoder) {
    if hse.is_null() {
        return;
    }
    // SAFETY: non-null encoder from alloc.
    unsafe {
        (*hse).inner.reset();
    }
}

/// Sink input bytes into the encoder.
///
/// # Safety
/// `hse`, `in_buf`, and `input_size` must be valid; `in_buf` readable for `size` bytes.
#[no_mangle]
pub unsafe extern "C" fn heatshrink_encoder_sink(
    hse: *mut HeatshrinkEncoder,
    in_buf: *mut u8,
    size: usize,
    input_size: *mut usize,
) -> i8 {
    if hse.is_null() || in_buf.is_null() || input_size.is_null() {
        return EncoderSinkRes::ErrorNull as i8;
    }
    // SAFETY: pointers checked non-null; `in_buf` valid for `size` bytes per contract.
    unsafe {
        let slice = core::slice::from_raw_parts(in_buf, size);
        match (*hse).inner.sink(slice) {
            Ok((res, n)) => {
                *input_size = n;
                res as i8
            }
            Err(_) => EncoderSinkRes::ErrorMisuse as i8,
        }
    }
}

/// Poll compressed output from the encoder.
///
/// # Safety
/// `hse`, `out_buf`, and `output_size` must be valid; `out_buf` writable for `out_buf_size`.
#[no_mangle]
pub unsafe extern "C" fn heatshrink_encoder_poll(
    hse: *mut HeatshrinkEncoder,
    out_buf: *mut u8,
    out_buf_size: usize,
    output_size: *mut usize,
) -> i8 {
    if hse.is_null() || out_buf.is_null() || output_size.is_null() {
        return EncoderPollRes::ErrorNull as i8;
    }
    // SAFETY: pointers checked; `out_buf` writable for `out_buf_size`.
    unsafe {
        let slice = core::slice::from_raw_parts_mut(out_buf, out_buf_size);
        match (*hse).inner.poll(slice) {
            Ok((res, n)) => {
                *output_size = n;
                res as i8
            }
            Err(_) => EncoderPollRes::ErrorMisuse as i8,
        }
    }
}

/// Notify the encoder that the input stream is finished.
///
/// # Safety
/// `hse` must be a valid encoder or null.
#[no_mangle]
pub unsafe extern "C" fn heatshrink_encoder_finish(hse: *mut HeatshrinkEncoder) -> i8 {
    if hse.is_null() {
        return EncoderFinishRes::ErrorNull as i8;
    }
    // SAFETY: non-null encoder from alloc.
    unsafe { (*hse).inner.finish() as i8 }
}

#[no_mangle]
pub extern "C" fn heatshrink_decoder_alloc(
    input_buffer_size: u16,
    expansion_buffer_sz2: u8,
    lookahead_sz2: u8,
) -> *mut HeatshrinkDecoder {
    match Decoder::alloc(input_buffer_size, expansion_buffer_sz2, lookahead_sz2) {
        Ok(inner) => Box::into_raw(Box::new(HeatshrinkDecoder { inner })),
        Err(_) => ptr::null_mut(),
    }
}

/// Free a decoder allocated by [`heatshrink_decoder_alloc`].
///
/// # Safety
/// `hsd` must be null or from `heatshrink_decoder_alloc` not already freed.
#[no_mangle]
pub unsafe extern "C" fn heatshrink_decoder_free(hsd: *mut HeatshrinkDecoder) {
    if hsd.is_null() {
        return;
    }
    // SAFETY: pointer from alloc.
    unsafe {
        drop(Box::from_raw(hsd));
    }
}

/// Reset a decoder to its initial state.
///
/// # Safety
/// `hsd` must be a valid decoder from alloc.
#[no_mangle]
pub unsafe extern "C" fn heatshrink_decoder_reset(hsd: *mut HeatshrinkDecoder) {
    if hsd.is_null() {
        return;
    }
    // SAFETY: non-null decoder from alloc.
    unsafe {
        (*hsd).inner.reset();
    }
}

/// Sink compressed bytes into the decoder.
///
/// # Safety
/// `hsd`, `in_buf`, and `input_size` must be valid; `in_buf` readable for `size`.
#[no_mangle]
pub unsafe extern "C" fn heatshrink_decoder_sink(
    hsd: *mut HeatshrinkDecoder,
    in_buf: *mut u8,
    size: usize,
    input_size: *mut usize,
) -> i8 {
    if hsd.is_null() || in_buf.is_null() || input_size.is_null() {
        return DecoderSinkRes::ErrorNull as i8;
    }
    // SAFETY: pointers checked; `in_buf` valid for `size`.
    unsafe {
        let slice = core::slice::from_raw_parts(in_buf, size);
        match (*hsd).inner.sink(slice) {
            Ok((res, n)) => {
                *input_size = n;
                res as i8
            }
            Err(_) => DecoderSinkRes::ErrorNull as i8,
        }
    }
}

/// Poll decompressed output from the decoder.
///
/// # Safety
/// `hsd`, `out_buf`, and `output_size` must be valid; `out_buf` writable for `out_buf_size`.
#[no_mangle]
pub unsafe extern "C" fn heatshrink_decoder_poll(
    hsd: *mut HeatshrinkDecoder,
    out_buf: *mut u8,
    out_buf_size: usize,
    output_size: *mut usize,
) -> i8 {
    if hsd.is_null() || out_buf.is_null() || output_size.is_null() {
        return DecoderPollRes::ErrorNull as i8;
    }
    // SAFETY: pointers checked; `out_buf` writable for `out_buf_size`.
    unsafe {
        let slice = core::slice::from_raw_parts_mut(out_buf, out_buf_size);
        match (*hsd).inner.poll(slice) {
            Ok((res, n)) => {
                *output_size = n;
                res as i8
            }
            Err(_) => DecoderPollRes::ErrorUnknown as i8,
        }
    }
}

/// Notify the decoder that the input stream is finished.
///
/// # Safety
/// `hsd` must be a valid decoder or null.
#[no_mangle]
pub unsafe extern "C" fn heatshrink_decoder_finish(hsd: *mut HeatshrinkDecoder) -> i8 {
    if hsd.is_null() {
        return DecoderFinishRes::ErrorNull as i8;
    }
    // SAFETY: non-null decoder from alloc.
    unsafe { (*hsd).inner.finish() as i8 }
}

/// Keep a symbol referenced so linkers retain the staticlib when unused.
#[no_mangle]
pub extern "C" fn heatshrink_ffi_keepalive() -> *const c_void {
    ptr::null()
}
