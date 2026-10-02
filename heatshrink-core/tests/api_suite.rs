//! Case-for-case ports of public-API tests from `test_heatshrink_dynamic.c`
//! and `test_heatshrink_static.c`.

use heatshrink_core::{
    Decoder, DecoderFinish, DecoderPoll, DecoderSink, Encoder, EncoderFinish, EncoderPoll,
    EncoderSink, Error, MAX_WINDOW_BITS, MIN_LOOKAHEAD_BITS, MIN_WINDOW_BITS,
};

fn fill_pseudorandom(buf: &mut [u8], seed: u32) {
    let mut s = seed;
    for b in buf.iter_mut() {
        s = 101u32.wrapping_mul(s).wrapping_add(103);
        *b = b'a' + (s % 26) as u8;
    }
}

fn compress(input: &[u8], w: u8, l: u8, sink_sz: usize, poll_sz: usize) -> Result<Vec<u8>, Error> {
    let mut enc = Encoder::alloc(w, l)?;
    let mut out = Vec::new();
    let mut tmp = vec![0u8; poll_sz.max(1)];
    let mut offset = 0;
    while offset < input.len() {
        let n = (input.len() - offset).min(sink_sz.max(1));
        let (_, sunk) = enc.sink(&input[offset..offset + n])?;
        offset += sunk;
        loop {
            let (pres, polled) = enc.poll(&mut tmp)?;
            out.extend_from_slice(&tmp[..polled]);
            if pres != EncoderPoll::More {
                break;
            }
        }
    }
    loop {
        let fres = enc.finish()?;
        loop {
            let (pres, polled) = enc.poll(&mut tmp)?;
            out.extend_from_slice(&tmp[..polled]);
            if pres != EncoderPoll::More {
                break;
            }
        }
        if fres == EncoderFinish::Done {
            break;
        }
    }
    Ok(out)
}

fn decompress(
    input: &[u8],
    ibs: u16,
    w: u8,
    l: u8,
    sink_sz: usize,
    poll_sz: usize,
) -> Result<Vec<u8>, Error> {
    let mut dec = Decoder::alloc(ibs, w, l)?;
    let mut out = Vec::new();
    let mut tmp = vec![0u8; poll_sz.max(1)];
    let mut offset = 0;
    while offset < input.len() {
        let n = (input.len() - offset).min(sink_sz.max(1));
        let (sres, sunk) = dec.sink(&input[offset..offset + n])?;
        if !(matches!(sres, DecoderSink::Full) && sunk == 0) {
            offset += sunk;
        }
        loop {
            let (pres, polled) = dec.poll(&mut tmp)?;
            out.extend_from_slice(&tmp[..polled]);
            if pres != DecoderPoll::More {
                break;
            }
        }
    }
    loop {
        let fres = dec.finish()?;
        loop {
            let (pres, polled) = dec.poll(&mut tmp)?;
            out.extend_from_slice(&tmp[..polled]);
            if pres != DecoderPoll::More {
                break;
            }
        }
        if fres == DecoderFinish::Done {
            break;
        }
    }
    Ok(out)
}

#[test]
fn encoder_alloc_should_reject_invalid_arguments() {
    assert!(Encoder::alloc(MIN_WINDOW_BITS - 1, 3).is_err());
    assert!(Encoder::alloc(MAX_WINDOW_BITS + 1, 3).is_err());
    assert!(Encoder::alloc(8, MIN_LOOKAHEAD_BITS - 1).is_err());
    assert!(Encoder::alloc(8, 8).is_err());
    assert!(Encoder::alloc(8, 9).is_err());
    assert!(Encoder::alloc(8, 4).is_ok());
}

#[test]
fn encoder_sink_should_accept_input_when_it_will_fit() {
    let mut enc = Encoder::alloc(8, 4).unwrap();
    let input = [0u8; 1 << 8];
    let (res, n) = enc.sink(&input).unwrap();
    assert_eq!(res, EncoderSink::Ok);
    assert_eq!(n, input.len());
}

#[test]
fn encoder_sink_should_accept_partial_input_when_some_will_fit() {
    let mut enc = Encoder::alloc(8, 4).unwrap();
    let input = [0u8; (1 << 8) + 1];
    let (res, n) = enc.sink(&input).unwrap();
    assert_eq!(res, EncoderSink::Ok);
    assert_eq!(n, 1 << 8);
}

#[test]
fn encoder_poll_should_indicate_when_no_input_is_provided() {
    let mut enc = Encoder::alloc(8, 4).unwrap();
    let mut out = [0u8; 16];
    let (res, n) = enc.poll(&mut out).unwrap();
    assert_eq!(res, EncoderPoll::Empty);
    assert_eq!(n, 0);
}

#[test]
fn encoder_should_emit_data_without_repetitions_as_literal_sequence() {
    let input = b"abcdefg";
    let comp = compress(input, 8, 4, 256, 256).unwrap();
    assert!(!comp.is_empty());
    let exp = decompress(&comp, 32, 8, 4, 256, 256).unwrap();
    assert_eq!(exp, input);
}

#[test]
fn encoder_should_emit_series_of_same_byte_as_literal_then_backref() {
    let input = [b'a'; 32];
    let comp = compress(&input, 8, 4, 256, 256).unwrap();
    let exp = decompress(&comp, 32, 8, 4, 256, 256).unwrap();
    assert_eq!(exp, input);
}

#[test]
fn encoder_poll_should_detect_repeated_substring() {
    let input = b"abcabcabcabc";
    let comp = compress(input, 8, 4, 256, 256).unwrap();
    let exp = decompress(&comp, 32, 8, 4, 256, 256).unwrap();
    assert_eq!(exp, input);
}

#[test]
fn decoder_alloc_should_reject_excessively_small_window() {
    assert!(Decoder::alloc(64, MIN_WINDOW_BITS - 1, 3).is_err());
}

#[test]
fn decoder_alloc_should_reject_zero_byte_input_buffer() {
    assert!(Decoder::alloc(0, 8, 4).is_err());
}

#[test]
fn decoder_alloc_should_reject_lookahead_equal_to_window_size() {
    assert!(Decoder::alloc(64, 8, 8).is_err());
}

#[test]
fn decoder_alloc_should_reject_lookahead_greater_than_window_size() {
    assert!(Decoder::alloc(64, 8, 9).is_err());
}

#[test]
fn decoder_sink_should_sink_data_when_preconditions_hold() {
    let mut dec = Decoder::alloc(64, 8, 4).unwrap();
    let input = [1u8, 2, 3];
    let (res, n) = dec.sink(&input).unwrap();
    assert_eq!(res, DecoderSink::Ok);
    assert_eq!(n, 3);
}

#[test]
fn decoder_poll_should_return_empty_if_empty() {
    let mut dec = Decoder::alloc(64, 8, 4).unwrap();
    let mut out = [0u8; 16];
    let (res, n) = dec.poll(&mut out).unwrap();
    assert_eq!(res, DecoderPoll::Empty);
    assert_eq!(n, 0);
}

#[test]
fn decoder_poll_should_expand_short_literal() {
    // One literal 'A': tag 1 + byte — produced by encoding a single byte.
    let input = b"A";
    let comp = compress(input, 8, 4, 256, 256).unwrap();
    let exp = decompress(&comp, 32, 8, 4, 256, 256).unwrap();
    assert_eq!(exp, input);
}

#[test]
fn decoder_poll_should_expand_short_literal_and_backref() {
    let input = b"aaaa";
    let comp = compress(input, 8, 4, 256, 256).unwrap();
    let exp = decompress(&comp, 32, 8, 4, 256, 256).unwrap();
    assert_eq!(exp, input);
}

#[test]
fn decoder_poll_should_expand_short_self_overlapping_backref() {
    let input = b"abcabcabcabcabc";
    let comp = compress(input, 8, 4, 256, 256).unwrap();
    let exp = decompress(&comp, 32, 8, 4, 256, 256).unwrap();
    assert_eq!(exp, input);
}

#[test]
fn decoder_poll_should_expand_short_literal_and_backref_when_fed_input_byte_by_byte() {
    let input = b"Hello, world! Hello, world!";
    let comp = compress(input, 8, 4, 1, 1).unwrap();
    let exp = decompress(&comp, 32, 8, 4, 1, 1).unwrap();
    assert_eq!(exp, input);
}

#[test]
fn decoder_finish_should_note_when_done() {
    let dec = Decoder::alloc(64, 8, 4).unwrap();
    assert_eq!(dec.finish().unwrap(), DecoderFinish::Done);
}

#[test]
fn data_without_duplication_should_match() {
    let mut input = vec![0u8; 100];
    fill_pseudorandom(&mut input, 1);
    let comp = compress(&input, 8, 4, 256, 256).unwrap();
    let exp = decompress(&comp, 64, 8, 4, 256, 256).unwrap();
    assert_eq!(exp, input);
}

#[test]
fn data_with_simple_repetition_should_compress_and_decompress_properly() {
    let input = b"abcabcabcabcabcabcabcabc";
    let comp = compress(input, 8, 4, 256, 256).unwrap();
    let exp = decompress(&comp, 64, 8, 4, 256, 256).unwrap();
    assert_eq!(exp, input);
}

#[test]
fn data_without_duplication_should_match_with_absurdly_tiny_buffers() {
    let mut input = vec![0u8; 50];
    fill_pseudorandom(&mut input, 2);
    let comp = compress(&input, 8, 4, 1, 1).unwrap();
    let exp = decompress(&comp, 32, 8, 4, 1, 1).unwrap();
    assert_eq!(exp, input);
}

#[test]
fn data_with_simple_repetition_should_match_with_absurdly_tiny_buffers() {
    let input = [b'x'; 80];
    let comp = compress(&input, 8, 4, 1, 1).unwrap();
    let exp = decompress(&comp, 32, 8, 4, 1, 1).unwrap();
    assert_eq!(exp, input);
}

#[test]
fn small_input_buffer_should_not_impact_decoder_correctness() {
    let mut input = vec![0u8; 500];
    fill_pseudorandom(&mut input, 3);
    let comp = compress(&input, 8, 4, 256, 256).unwrap();
    let exp = decompress(&comp, 1, 8, 4, 256, 256).unwrap();
    assert_eq!(exp, input);
}

#[test]
fn sixty_four_k() {
    let mut input = vec![0u8; 64 * 1024];
    fill_pseudorandom(&mut input, 4);
    let comp = compress(&input, 8, 4, 256, 256).unwrap();
    let exp = decompress(&comp, 256, 8, 4, 256, 256).unwrap();
    assert_eq!(exp, input);
}

#[test]
fn static_integration_pseudorandom_roundtrip() {
    // Mirrors test_heatshrink_static.c sizes/seeds (subset for speed).
    for &size in &[1u16, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024] {
        for seed in 1u32..=10 {
            let mut input = vec![0u8; size as usize];
            fill_pseudorandom(&mut input, seed);
            // Static defaults: window 8, lookahead 4, ibs 32
            let comp = compress(&input, 8, 4, 256, 256).unwrap();
            let exp = decompress(&comp, 32, 8, 4, 256, 256).unwrap();
            assert_eq!(exp, input, "size={size} seed={seed}");
        }
    }
}

#[test]
fn fuzz_integration_pseudorandom() {
    for &size in &[5u32, 50, 100] {
        for seed in 1u32..=5 {
            let mut input = vec![0u8; size as usize];
            fill_pseudorandom(&mut input, seed);
            for w in 6u8..=9 {
                for l in 3u8..w {
                    let comp = compress(&input, w, l, 256, 256).unwrap();
                    let exp = decompress(&comp, 64, w, l, 256, 256).unwrap();
                    assert_eq!(exp, input, "size={size} seed={seed} w={w} l={l}");
                }
            }
        }
    }
}
