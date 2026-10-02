use heatshrink_core::{
    decode_all, encode_all, Decoder, DecoderPollRes, Encoder, EncoderFinishRes, EncoderPollRes,
    EncoderSinkRes, HEATSHRINK_MAX_WINDOW_BITS, HEATSHRINK_MIN_LOOKAHEAD_BITS,
    HEATSHRINK_MIN_WINDOW_BITS,
};

#[test]
fn encoder_alloc_rejects_invalid() {
    assert!(Encoder::alloc(HEATSHRINK_MIN_WINDOW_BITS - 1, 8).is_none());
    assert!(Encoder::alloc(HEATSHRINK_MAX_WINDOW_BITS + 1, 8).is_none());
    assert!(Encoder::alloc(8, HEATSHRINK_MIN_LOOKAHEAD_BITS - 1).is_none());
    assert!(Encoder::alloc(8, 9).is_none());
}

#[test]
fn encoder_literals_no_repetition() {
    let mut hse = Encoder::alloc(8, 7).unwrap();
    let input: [u8; 5] = [0, 1, 2, 3, 4];
    let expected: [u8; 6] = [0x80, 0x40, 0x60, 0x50, 0x38, 0x20];
    let mut out = [0u8; 1024];

    let (sres, n) = hse.sink(&input);
    assert_eq!(sres, EncoderSinkRes::Ok);
    assert_eq!(n, 5);

    let (pres, w) = hse.poll(&mut out);
    assert_eq!(pres, EncoderPollRes::Empty);
    assert_eq!(w, 0);

    assert_eq!(hse.finish(), EncoderFinishRes::More);
    let (pres, _w) = hse.poll(&mut out);
    assert_eq!(pres, EncoderPollRes::Empty);
    for (i, &e) in expected.iter().enumerate() {
        assert_eq!(out[i], e, "byte {i}");
    }
    assert_eq!(hse.finish(), EncoderFinishRes::Done);
}

#[test]
fn encoder_same_byte_series() {
    let mut hse = Encoder::alloc(8, 7).unwrap();
    let input = [b'a'; 5];
    let expected: [u8; 4] = [0xb0, 0x80, 0x01, 0x80];
    let mut out = [0u8; 1024];

    let (_, n) = hse.sink(&input);
    assert_eq!(n, 5);
    assert_eq!(hse.finish(), EncoderFinishRes::More);
    let (_, w) = hse.poll(&mut out);
    assert_eq!(w, 4);
    assert_eq!(&out[..4], &expected);
    assert_eq!(hse.finish(), EncoderFinishRes::Done);
}

#[test]
fn encoder_repeated_substring() {
    let mut hse = Encoder::alloc(8, 3).unwrap();
    let input = b"abcdabcd";
    let expected: [u8; 6] = [0xb0, 0xd8, 0xac, 0x76, 0x40, 0x1b];
    let mut out = [0u8; 1024];

    let (sres, n) = hse.sink(input);
    assert_eq!(sres, EncoderSinkRes::Ok);
    assert_eq!(n, input.len());
    assert_eq!(hse.finish(), EncoderFinishRes::More);
    let (pres, w) = hse.poll(&mut out);
    assert_eq!(pres, EncoderPollRes::Empty);
    assert_eq!(w, expected.len());
    assert_eq!(hse.finish(), EncoderFinishRes::Done);
    assert_eq!(&out[..w], &expected);
}

#[test]
fn decoder_short_literal() {
    let input: [u8; 4] = [0xb3, 0x5b, 0xed, 0xe0];
    let mut hsd = Decoder::alloc(256, 7, 3).unwrap();
    let mut out = [0u8; 4];
    let (_, n) = hsd.sink(&input);
    assert_eq!(n, 4);
    let (pres, w) = hsd.poll(&mut out);
    assert_eq!(pres, DecoderPollRes::Empty);
    assert_eq!(w, 3);
    assert_eq!(&out[..3], b"foo");
}

#[test]
fn decoder_self_overlapping_backref() {
    let input: [u8; 4] = [0xb0, 0x80, 0x01, 0x80];
    let mut hsd = Decoder::alloc(256, 8, 7).unwrap();
    let mut out = [0u8; 6];
    let (_, n) = hsd.sink(&input);
    assert_eq!(n, 4);
    let (_, w) = hsd.poll(&mut out);
    assert_eq!(w, 5);
    assert_eq!(&out[..5], b"aaaaa");
}

#[test]
fn roundtrip_pseudorandom() {
    let mut buf = vec![0u8; 512];
    let mut rn: u64 = 9_223_372_036_854_775_783;
    let seed: u64 = 0xdeadbeef;
    for b in &mut buf {
        rn = rn.wrapping_mul(seed).wrapping_add(seed);
        *b = (rn % 26) as u8 + b'a';
    }
    let comp = encode_all(&buf, 8, 4).unwrap();
    let decomp = decode_all(&comp, 32, 8, 4).unwrap();
    assert_eq!(decomp, buf);
}

#[test]
fn encode_empty_input() {
    let out = encode_all(b"", 8, 4).unwrap();
    assert!(out.is_empty());
}
