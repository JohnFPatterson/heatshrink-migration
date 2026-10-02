//! Pointer-level ABI checks for heatshrink-ffi.

use heatshrink_ffi::{
    heatshrink_decoder_alloc, heatshrink_decoder_finish, heatshrink_decoder_free,
    heatshrink_decoder_poll, heatshrink_decoder_reset, heatshrink_decoder_sink,
    heatshrink_encoder_alloc, heatshrink_encoder_finish, heatshrink_encoder_free,
    heatshrink_encoder_poll, heatshrink_encoder_reset, heatshrink_encoder_sink, HSDR_FINISH_DONE,
    HSDR_POLL_EMPTY, HSDR_SINK_OK, HSER_FINISH_ERROR_NULL, HSER_FINISH_MORE, HSER_POLL_EMPTY,
    HSER_POLL_ERROR_NULL, HSER_SINK_ERROR_NULL, HSER_SINK_OK,
};

#[test]
fn null_encoder_apis_return_error_codes() {
    // SAFETY: exercising documented C ABI null/pointer contracts in unit tests.
    unsafe {
        assert_eq!(
            heatshrink_encoder_sink(
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut()
            ),
            HSER_SINK_ERROR_NULL
        );
        assert_eq!(
            heatshrink_encoder_poll(
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut()
            ),
            HSER_POLL_ERROR_NULL
        );
        assert_eq!(
            heatshrink_encoder_finish(std::ptr::null_mut()),
            HSER_FINISH_ERROR_NULL
        );
    }
}

#[test]
fn encoder_alloc_reset_free_roundtrip() {
    // SAFETY: exercising documented C ABI null/pointer contracts in unit tests.
    unsafe {
        let hse = heatshrink_encoder_alloc(8, 4);
        assert!(!hse.is_null());
        heatshrink_encoder_reset(hse);
        let mut in_buf = [b'a', b'b', b'c'];
        let mut sunk = 0usize;
        let sres = heatshrink_encoder_sink(hse, in_buf.as_mut_ptr(), in_buf.len(), &mut sunk);
        assert_eq!(sres, HSER_SINK_OK);
        assert_eq!(sunk, 3);
        let mut out = [0u8; 64];
        let mut out_sz = 0usize;
        let _ = heatshrink_encoder_finish(hse);
        let pres = heatshrink_encoder_poll(hse, out.as_mut_ptr(), out.len(), &mut out_sz);
        assert!(pres == HSER_POLL_EMPTY || out_sz > 0 || pres >= 0);
        heatshrink_encoder_free(hse);
    }
}

#[test]
fn decoder_alloc_sink_poll_finish() {
    // SAFETY: exercising documented C ABI null/pointer contracts in unit tests.
    unsafe {
        let hse = heatshrink_encoder_alloc(8, 4);
        assert!(!hse.is_null());
        let input = b"HelloHello";
        let mut sunk = 0usize;
        assert_eq!(
            heatshrink_encoder_sink(hse, input.as_ptr() as *mut u8, input.len(), &mut sunk),
            HSER_SINK_OK
        );
        let mut comp = Vec::new();
        let mut tmp = [0u8; 64];
        loop {
            let fres = heatshrink_encoder_finish(hse);
            let mut out_sz = 0usize;
            let pres = heatshrink_encoder_poll(hse, tmp.as_mut_ptr(), tmp.len(), &mut out_sz);
            comp.extend_from_slice(&tmp[..out_sz]);
            if fres == heatshrink_ffi::HSER_FINISH_DONE {
                break;
            }
            let _ = pres;
        }
        heatshrink_encoder_free(hse);

        let hsd = heatshrink_decoder_alloc(32, 8, 4);
        assert!(!hsd.is_null());
        heatshrink_decoder_reset(hsd);
        let mut sunk = 0usize;
        assert_eq!(
            heatshrink_decoder_sink(hsd, comp.as_mut_ptr(), comp.len(), &mut sunk),
            HSDR_SINK_OK
        );
        let mut exp = Vec::new();
        loop {
            let mut out_sz = 0usize;
            let pres = heatshrink_decoder_poll(hsd, tmp.as_mut_ptr(), tmp.len(), &mut out_sz);
            exp.extend_from_slice(&tmp[..out_sz]);
            if pres == HSDR_POLL_EMPTY {
                break;
            }
        }
        assert_eq!(heatshrink_decoder_finish(hsd), HSDR_FINISH_DONE);
        assert_eq!(exp, input);
        heatshrink_decoder_free(hsd);
    }
}

#[test]
fn finish_more_then_done_on_nonempty() {
    // SAFETY: exercising documented C ABI null/pointer contracts in unit tests.
    unsafe {
        let hse = heatshrink_encoder_alloc(8, 4);
        let mut data = [b'z'; 32];
        let mut sunk = 0;
        heatshrink_encoder_sink(hse, data.as_mut_ptr(), data.len(), &mut sunk);
        let fres = heatshrink_encoder_finish(hse);
        assert_eq!(fres, HSER_FINISH_MORE);
        heatshrink_encoder_free(hse);
    }
}
