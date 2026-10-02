//! Integration tests ported from test_heatshrink_dynamic.c (encoding fixtures).

use heatshrink::decoder::{roundtrip, Decoder, PollRes, SinkRes};
use heatshrink::encoder::{Encoder, FinishRes, PollRes as EPoll, SinkRes as ESink};
use heatshrink::{MIN_LOOKAHEAD_BITS, MIN_WINDOW_BITS};

fn encode_finish_poll(window: u8, lookahead: u8, input: &[u8]) -> Vec<u8> {
    let mut enc = Encoder::alloc(window, lookahead).unwrap();
    let mut sunk = 0usize;
    let mut out = Vec::new();
    while sunk < input.len() {
        let mut n = 0usize;
        assert_eq!(enc.sink(&input[sunk..], &mut n), ESink::Ok);
        sunk += n;
        if sunk == input.len() {
            assert_eq!(enc.finish(), FinishRes::More);
        }
        loop {
            let mut buf = [0u8; 1024];
            let mut n = 0usize;
            let pres = enc.poll(&mut buf, &mut n);
            out.extend_from_slice(&buf[..n]);
            if pres != EPoll::More {
                break;
            }
        }
        if sunk == input.len() {
            assert_eq!(enc.finish(), FinishRes::Done);
        }
    }
    out
}

#[test]
fn encoder_rejects_bad_window() {
    assert!(Encoder::alloc(MIN_WINDOW_BITS - 1, 8).is_none());
    assert!(Encoder::alloc(8, MIN_LOOKAHEAD_BITS - 1).is_none());
    assert!(Encoder::alloc(8, 9).is_none());
}

#[test]
fn encoder_literal_sequence() {
    let input: Vec<u8> = (0..5).collect();
    let out = encode_finish_poll(8, 7, &input);
    assert_eq!(out, vec![0x80, 0x40, 0x60, 0x50, 0x38, 0x20]);
}

#[test]
fn encoder_abcdabcd_with_trailing_e() {
    let input = b"abcdabcde";
    let out = encode_finish_poll(8, 3, input);
    assert_eq!(
        out,
        vec![0xb0, 0xd8, 0xac, 0x76, 0x40, 0x1b, 0xb2, 0x80]
    );
}

#[test]
fn decoder_expands_foo() {
    let input = b"fooooo";
    let comp = encode_finish_poll(8, 7, input);
    let mut dec = Decoder::alloc(256, 8, 7).unwrap();
    let mut count = 0usize;
    assert_eq!(dec.sink(&comp, &mut count), SinkRes::Ok);
    let mut out = [0u8; 8];
    let mut out_sz = 0usize;
    assert_eq!(dec.poll(&mut out, &mut out_sz), PollRes::Empty);
    assert_eq!(&out[..out_sz], input);
}

#[test]
fn roundtrip_many_sizes() {
    for size in [0usize, 1, 17, 255, 512, 2000] {
        let mut data = vec![0u8; size];
        for (i, b) in data.iter_mut().enumerate() {
            *b = (i % 251) as u8;
        }
        let out = roundtrip(8, 4, 32, &data).unwrap_or_else(|| panic!("size {size}"));
        assert_eq!(out, data, "size {size}");
    }
}
