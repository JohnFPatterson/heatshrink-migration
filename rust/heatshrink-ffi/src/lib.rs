#![allow(non_camel_case_types)]

use heatshrink_core::{
    Decoder, DecoderFinishRes, DecoderPollRes, DecoderSinkRes, Encoder, EncoderFinishRes,
    EncoderPollRes, EncoderSinkRes,
};
use std::ptr;

#[repr(C)]
pub struct heatshrink_encoder {
    _private: [u8; 0],
}

#[repr(C)]
pub struct heatshrink_decoder {
    _private: [u8; 0],
}

struct EncBox(Encoder);
struct DecBox(Decoder);

#[repr(i8)]
pub enum HSE_sink_res {
    HSER_SINK_OK = 0,
    HSER_SINK_ERROR_NULL = -1,
    HSER_SINK_ERROR_MISUSE = -2,
}

#[repr(i8)]
pub enum HSE_poll_res {
    HSER_POLL_EMPTY = 0,
    HSER_POLL_MORE = 1,
    HSER_POLL_ERROR_NULL = -1,
    HSER_POLL_ERROR_MISUSE = -2,
}

#[repr(i8)]
pub enum HSE_finish_res {
    HSER_FINISH_DONE = 0,
    HSER_FINISH_MORE = 1,
    HSER_FINISH_ERROR_NULL = -1,
}

#[repr(i8)]
pub enum HSD_sink_res {
    HSDR_SINK_OK = 0,
    HSDR_SINK_FULL = 1,
    HSDR_SINK_ERROR_NULL = -1,
}

#[repr(i8)]
pub enum HSD_poll_res {
    HSDR_POLL_EMPTY = 0,
    HSDR_POLL_MORE = 1,
    HSDR_POLL_ERROR_NULL = -1,
    HSDR_POLL_ERROR_UNKNOWN = -2,
}

#[repr(i8)]
pub enum HSD_finish_res {
    HSDR_FINISH_DONE = 0,
    HSDR_FINISH_MORE = 1,
    HSDR_FINISH_ERROR_NULL = -1,
}

fn enc_ptr(hse: *mut heatshrink_encoder) -> Option<&'static mut EncBox> {
    if hse.is_null() {
        None
    } else {
        // SAFETY: pointer originates from Box::into_raw in alloc and is unique.
        Some(unsafe { &mut *(hse as *mut EncBox) })
    }
}

fn dec_ptr(hsd: *mut heatshrink_decoder) -> Option<&'static mut DecBox> {
    if hsd.is_null() {
        None
    } else {
        // SAFETY: pointer originates from Box::into_raw in alloc and is unique.
        Some(unsafe { &mut *(hsd as *mut DecBox) })
    }
}

#[no_mangle]
pub extern "C" fn heatshrink_encoder_alloc(
    window_sz2: u8,
    lookahead_sz2: u8,
) -> *mut heatshrink_encoder {
    match Encoder::alloc(window_sz2, lookahead_sz2) {
        Some(enc) => Box::into_raw(Box::new(EncBox(enc))) as *mut heatshrink_encoder,
        None => ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "C" fn heatshrink_encoder_free(hse: *mut heatshrink_encoder) {
    if !hse.is_null() {
        // SAFETY: paired with Box::into_raw from alloc.
        drop(unsafe { Box::from_raw(hse as *mut EncBox) });
    }
}

#[no_mangle]
pub extern "C" fn heatshrink_encoder_reset(hse: *mut heatshrink_encoder) {
    if let Some(b) = enc_ptr(hse) {
        b.0.reset();
    }
}

#[no_mangle]
/// # Safety
/// `hse` must come from `heatshrink_encoder_alloc`. `in_buf` must point to at least `size` bytes;
/// `input_size` must be a valid writable pointer.
pub unsafe extern "C" fn heatshrink_encoder_sink(
    hse: *mut heatshrink_encoder,
    in_buf: *mut u8,
    size: usize,
    input_size: *mut usize,
) -> HSE_sink_res {
    if hse.is_null() || in_buf.is_null() || input_size.is_null() {
        return HSE_sink_res::HSER_SINK_ERROR_NULL;
    }
    let slice = unsafe { std::slice::from_raw_parts(in_buf, size) };
    let (res, n) = enc_ptr(hse).unwrap().0.sink(slice);
    unsafe { *input_size = n };
    match res {
        EncoderSinkRes::Ok => HSE_sink_res::HSER_SINK_OK,
        EncoderSinkRes::ErrorNull => HSE_sink_res::HSER_SINK_ERROR_NULL,
        EncoderSinkRes::ErrorMisuse => HSE_sink_res::HSER_SINK_ERROR_MISUSE,
    }
}

#[no_mangle]
/// # Safety
/// `hse` must come from `heatshrink_encoder_alloc`. `out_buf` must point to at least `out_buf_size`
/// bytes; `output_size` must be a valid writable pointer.
pub unsafe extern "C" fn heatshrink_encoder_poll(
    hse: *mut heatshrink_encoder,
    out_buf: *mut u8,
    out_buf_size: usize,
    output_size: *mut usize,
) -> HSE_poll_res {
    if hse.is_null() || out_buf.is_null() || output_size.is_null() {
        return HSE_poll_res::HSER_POLL_ERROR_NULL;
    }
    let slice = unsafe { std::slice::from_raw_parts_mut(out_buf, out_buf_size) };
    let (res, n) = enc_ptr(hse).unwrap().0.poll(slice);
    unsafe { *output_size = n };
    match res {
        EncoderPollRes::Empty => HSE_poll_res::HSER_POLL_EMPTY,
        EncoderPollRes::More => HSE_poll_res::HSER_POLL_MORE,
        EncoderPollRes::ErrorNull => HSE_poll_res::HSER_POLL_ERROR_NULL,
        EncoderPollRes::ErrorMisuse => HSE_poll_res::HSER_POLL_ERROR_MISUSE,
    }
}

#[no_mangle]
pub extern "C" fn heatshrink_encoder_finish(hse: *mut heatshrink_encoder) -> HSE_finish_res {
    if hse.is_null() {
        return HSE_finish_res::HSER_FINISH_ERROR_NULL;
    }
    match enc_ptr(hse).unwrap().0.finish() {
        EncoderFinishRes::Done => HSE_finish_res::HSER_FINISH_DONE,
        EncoderFinishRes::More => HSE_finish_res::HSER_FINISH_MORE,
        EncoderFinishRes::ErrorNull => HSE_finish_res::HSER_FINISH_ERROR_NULL,
    }
}

#[no_mangle]
pub extern "C" fn heatshrink_decoder_alloc(
    input_buffer_size: u16,
    window_sz2: u8,
    lookahead_sz2: u8,
) -> *mut heatshrink_decoder {
    match Decoder::alloc(input_buffer_size, window_sz2, lookahead_sz2) {
        Some(dec) => Box::into_raw(Box::new(DecBox(dec))) as *mut heatshrink_decoder,
        None => ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "C" fn heatshrink_decoder_free(hsd: *mut heatshrink_decoder) {
    if !hsd.is_null() {
        drop(unsafe { Box::from_raw(hsd as *mut DecBox) });
    }
}

#[no_mangle]
pub extern "C" fn heatshrink_decoder_reset(hsd: *mut heatshrink_decoder) {
    if let Some(b) = dec_ptr(hsd) {
        b.0.reset();
    }
}

#[no_mangle]
/// # Safety
/// `hsd` must come from `heatshrink_decoder_alloc`. `in_buf` must point to at least `size` bytes;
/// `input_size` must be a valid writable pointer.
pub unsafe extern "C" fn heatshrink_decoder_sink(
    hsd: *mut heatshrink_decoder,
    in_buf: *mut u8,
    size: usize,
    input_size: *mut usize,
) -> HSD_sink_res {
    if hsd.is_null() || in_buf.is_null() || input_size.is_null() {
        return HSD_sink_res::HSDR_SINK_ERROR_NULL;
    }
    let slice = unsafe { std::slice::from_raw_parts(in_buf, size) };
    let (res, n) = dec_ptr(hsd).unwrap().0.sink(slice);
    unsafe { *input_size = n };
    match res {
        DecoderSinkRes::Ok => HSD_sink_res::HSDR_SINK_OK,
        DecoderSinkRes::Full => HSD_sink_res::HSDR_SINK_FULL,
        DecoderSinkRes::ErrorNull => HSD_sink_res::HSDR_SINK_ERROR_NULL,
    }
}

#[no_mangle]
/// # Safety
/// `hsd` must come from `heatshrink_decoder_alloc`. `out_buf` must point to at least `out_buf_size`
/// bytes; `output_size` must be a valid writable pointer.
pub unsafe extern "C" fn heatshrink_decoder_poll(
    hsd: *mut heatshrink_decoder,
    out_buf: *mut u8,
    out_buf_size: usize,
    output_size: *mut usize,
) -> HSD_poll_res {
    if hsd.is_null() || out_buf.is_null() || output_size.is_null() {
        return HSD_poll_res::HSDR_POLL_ERROR_NULL;
    }
    let slice = unsafe { std::slice::from_raw_parts_mut(out_buf, out_buf_size) };
    let (res, n) = dec_ptr(hsd).unwrap().0.poll(slice);
    unsafe { *output_size = n };
    match res {
        DecoderPollRes::Empty => HSD_poll_res::HSDR_POLL_EMPTY,
        DecoderPollRes::More => HSD_poll_res::HSDR_POLL_MORE,
        DecoderPollRes::ErrorNull => HSD_poll_res::HSDR_POLL_ERROR_NULL,
        DecoderPollRes::ErrorUnknown => HSD_poll_res::HSDR_POLL_ERROR_UNKNOWN,
    }
}

#[no_mangle]
pub extern "C" fn heatshrink_decoder_finish(hsd: *mut heatshrink_decoder) -> HSD_finish_res {
    if hsd.is_null() {
        return HSD_finish_res::HSDR_FINISH_ERROR_NULL;
    }
    match dec_ptr(hsd).unwrap().0.finish() {
        DecoderFinishRes::Done => HSD_finish_res::HSDR_FINISH_DONE,
        DecoderFinishRes::More => HSD_finish_res::HSDR_FINISH_MORE,
        DecoderFinishRes::ErrorNull => HSD_finish_res::HSDR_FINISH_ERROR_NULL,
    }
}
