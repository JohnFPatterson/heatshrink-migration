//! Public-API style tests ported from `test_heatshrink_dynamic.c` (encoding/decoding/integration).

use heatshrink_core::{
    Decoder, DecoderFinishRes, DecoderPollRes, DecoderSinkRes, Encoder, EncoderFinishRes,
    EncoderPollRes, EncoderSinkRes, Error,
};

fn encode_all(hse: &mut Encoder, input: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut sunk = 0usize;
    while sunk < input.len() {
        let (_, n) = hse.sink(&input[sunk..]).unwrap();
        sunk += n;
        loop {
            let mut buf = [0u8; 64];
            let (pres, pn) = hse.poll(&mut buf).unwrap();
            out.extend_from_slice(&buf[..pn]);
            if pres != EncoderPollRes::More {
                break;
            }
        }
    }
    loop {
        if hse.finish() == EncoderFinishRes::Done {
            break;
        }
        loop {
            let mut buf = [0u8; 64];
            let (pres, pn) = hse.poll(&mut buf).unwrap();
            out.extend_from_slice(&buf[..pn]);
            if pres != EncoderPollRes::More {
                break;
            }
        }
    }
    out
}

fn decode_all(hsd: &mut Decoder, input: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut sunk = 0usize;
    while sunk < input.len() {
        let (sres, n) = hsd.sink(&input[sunk..]).unwrap();
        sunk += n;
        loop {
            let mut buf = [0u8; 64];
            let (pres, pn) = hsd.poll(&mut buf).unwrap();
            out.extend_from_slice(&buf[..pn]);
            if pres != DecoderPollRes::More {
                break;
            }
        }
        if sres == DecoderSinkRes::Full && n == 0 {
            continue;
        }
    }
    loop {
        if hsd.finish() == DecoderFinishRes::Done {
            break;
        }
        loop {
            let mut buf = [0u8; 64];
            let (pres, pn) = hsd.poll(&mut buf).unwrap();
            out.extend_from_slice(&buf[..pn]);
            if pres != DecoderPollRes::More {
                break;
            }
        }
    }
    out
}

#[test]
fn encoder_alloc_should_reject_invalid_arguments() {
    assert!(matches!(Encoder::alloc(3, 3), Err(Error::InvalidConfig)));
    assert!(matches!(Encoder::alloc(8, 8), Err(Error::InvalidConfig))); // lookahead >= window
    assert!(matches!(Encoder::alloc(8, 2), Err(Error::InvalidConfig))); // lookahead too small
    assert!(Encoder::alloc(8, 4).is_ok());
}

#[test]
fn encoder_sink_should_accept_input_when_it_will_fit() {
    let mut hse = Encoder::alloc(8, 4).unwrap();
    let input = [0x01u8, 0x02, 0x03];
    let (res, n) = hse.sink(&input).unwrap();
    assert_eq!(res, EncoderSinkRes::Ok);
    assert_eq!(n, 3);
}

#[test]
fn encoder_poll_should_indicate_when_no_input_is_provided() {
    let mut hse = Encoder::alloc(8, 4).unwrap();
    let mut out = [0u8; 16];
    let (res, n) = hse.poll(&mut out).unwrap();
    assert_eq!(res, EncoderPollRes::Empty);
    assert_eq!(n, 0);
}

#[test]
fn decoder_alloc_should_reject_zero_byte_input_buffer() {
    assert!(matches!(Decoder::alloc(0, 8, 4), Err(Error::InvalidConfig)));
}

#[test]
fn decoder_alloc_should_reject_lookahead_equal_to_window_size() {
    assert!(matches!(
        Decoder::alloc(32, 8, 8),
        Err(Error::InvalidConfig)
    ));
}

#[test]
fn decoder_poll_should_return_empty_if_empty() {
    let mut hsd = Decoder::alloc(32, 8, 4).unwrap();
    let mut out = [0u8; 16];
    let (res, n) = hsd.poll(&mut out).unwrap();
    assert_eq!(res, DecoderPollRes::Empty);
    assert_eq!(n, 0);
}

#[test]
fn data_without_duplication_should_match() {
    let mut hse = Encoder::alloc(8, 4).unwrap();
    let input: Vec<u8> = (0..100u8).collect();
    let enc = encode_all(&mut hse, &input);
    let mut hsd = Decoder::alloc(256, 8, 4).unwrap();
    let dec = decode_all(&mut hsd, &enc);
    assert_eq!(dec, input);
}

#[test]
fn data_with_simple_repetition_should_compress_and_decompress_properly() {
    let mut hse = Encoder::alloc(8, 4).unwrap();
    let input = vec![b'a'; 100];
    let enc = encode_all(&mut hse, &input);
    assert!(enc.len() < input.len());
    let mut hsd = Decoder::alloc(256, 8, 4).unwrap();
    let dec = decode_all(&mut hsd, &enc);
    assert_eq!(dec, input);
}

#[test]
fn decoder_poll_should_expand_short_literal() {
    // Compressed stream for a few literal bytes produced by the encoder.
    let mut hse = Encoder::alloc(8, 4).unwrap();
    let input = b"abc";
    let enc = encode_all(&mut hse, input);
    let mut hsd = Decoder::alloc(32, 8, 4).unwrap();
    let dec = decode_all(&mut hsd, &enc);
    assert_eq!(dec, input);
}
