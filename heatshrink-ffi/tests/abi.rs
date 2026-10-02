//! Pointer-level ABI smoke tests for heatshrink-ffi.

use heatshrink_ffi::{
    heatshrink_decoder_alloc, heatshrink_decoder_finish, heatshrink_decoder_free,
    heatshrink_decoder_poll, heatshrink_decoder_reset, heatshrink_decoder_sink,
    heatshrink_encoder_alloc, heatshrink_encoder_finish, heatshrink_encoder_free,
    heatshrink_encoder_poll, heatshrink_encoder_reset, heatshrink_encoder_sink,
};

#[test]
fn null_encoder_ops_return_error_codes() {
    unsafe {
        assert_eq!(heatshrink_encoder_finish(std::ptr::null_mut()), -1);
        let mut n = 0usize;
        assert_eq!(
            heatshrink_encoder_sink(std::ptr::null_mut(), std::ptr::null_mut(), 0, &mut n),
            -1
        );
        assert_eq!(
            heatshrink_encoder_poll(std::ptr::null_mut(), std::ptr::null_mut(), 0, &mut n),
            -1
        );
    }
}

#[test]
fn null_decoder_ops_return_error_codes() {
    unsafe {
        assert_eq!(heatshrink_decoder_finish(std::ptr::null_mut()), -1);
        let mut n = 0usize;
        assert_eq!(
            heatshrink_decoder_sink(std::ptr::null_mut(), std::ptr::null_mut(), 0, &mut n),
            -1
        );
        assert_eq!(
            heatshrink_decoder_poll(std::ptr::null_mut(), std::ptr::null_mut(), 0, &mut n),
            -1
        );
    }
}

#[test]
fn alloc_free_roundtrip_encoder_decoder() {
    unsafe {
        let hse = heatshrink_encoder_alloc(8, 4);
        assert!(!hse.is_null());
        heatshrink_encoder_reset(hse);
        heatshrink_encoder_free(hse);

        let hsd = heatshrink_decoder_alloc(64, 8, 4);
        assert!(!hsd.is_null());
        heatshrink_decoder_reset(hsd);
        heatshrink_decoder_free(hsd);
    }
}

#[test]
fn invalid_alloc_returns_null() {
    assert!(heatshrink_encoder_alloc(3, 3).is_null());
    assert!(heatshrink_decoder_alloc(0, 8, 4).is_null());
}
