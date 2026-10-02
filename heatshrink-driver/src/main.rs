//! Rust differential driver — same stdout as `tools/heatshrink-oracle.c`.

use heatshrink_core::{
    Decoder, DecoderFinish, DecoderPoll, DecoderSink, Encoder, EncoderFinish, EncoderPoll,
};
use std::env;
use std::fs;
use std::process;

const WBITS: u8 = 8;
const LBITS: u8 = 4;
const IBS: u16 = 32;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!(
            "usage: {} <fixture> [--sections name[,name…]]",
            args.first().map(|s| s.as_str()).unwrap_or("rust-driver")
        );
        process::exit(1);
    }

    let mut want_encode = true;
    let mut want_decode = true;
    let mut want_roundtrip = true;
    let mut want_stream = true;

    let mut i = 2;
    while i < args.len() {
        if args[i] == "--sections" && i + 1 < args.len() {
            want_encode = false;
            want_decode = false;
            want_roundtrip = false;
            want_stream = false;
            let list = args[i + 1].clone();
            for tok in list.split(',') {
                match tok {
                    "encode" => want_encode = true,
                    "decode" => want_decode = true,
                    "roundtrip" => want_roundtrip = true,
                    "stream" => want_stream = true,
                    _ => {}
                }
            }
            i += 2;
        } else {
            i += 1;
        }
    }

    let path = &args[1];
    let input = match fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("cannot read {path}: {e}");
            process::exit(1);
        }
    };

    if want_encode {
        do_encode(&input);
    }
    if want_decode {
        do_decode(&input);
    }
    if want_roundtrip {
        do_roundtrip(&input, 256, false);
    }
    if want_stream {
        do_roundtrip(&input, 1, true);
    }
}

fn print_hex(buf: &[u8]) {
    for b in buf {
        print!("{b:02x}");
    }
}

fn encode_all(input: &[u8], chunk: usize) -> Result<(Vec<u8>, &'static str), ()> {
    let mut enc = Encoder::alloc(WBITS, LBITS).map_err(|_| ())?;
    let mut comp = Vec::new();
    let tsz = chunk.max(1);
    let mut tmp = vec![0u8; tsz];
    let mut offset = 0;
    while offset < input.len() {
        let to_sink = (input.len() - offset).min(tsz);
        let (_, sunk) = enc.sink(&input[offset..offset + to_sink]).map_err(|_| ())?;
        offset += sunk;
        loop {
            let (pres, polled) = enc.poll(&mut tmp).map_err(|_| ())?;
            comp.extend_from_slice(&tmp[..polled]);
            if pres != EncoderPoll::More {
                break;
            }
        }
    }
    let fin_name;
    loop {
        let fres = enc.finish().map_err(|_| ())?;
        loop {
            let (pres, polled) = enc.poll(&mut tmp).map_err(|_| ())?;
            comp.extend_from_slice(&tmp[..polled]);
            if pres != EncoderPoll::More {
                break;
            }
        }
        match fres {
            EncoderFinish::Done => {
                fin_name = "DONE";
                break;
            }
            EncoderFinish::More => continue,
        }
    }
    Ok((comp, fin_name))
}

fn decode_all(input: &[u8], chunk: usize) -> Result<(Vec<u8>, &'static str), ()> {
    let mut dec = Decoder::alloc(IBS, WBITS, LBITS).map_err(|_| ())?;
    let mut exp = Vec::new();
    let tsz = chunk.max(1);
    let mut tmp = vec![0u8; tsz];
    let mut offset = 0;
    while offset < input.len() {
        let to_sink = (input.len() - offset).min(tsz);
        let (sres, sunk) = dec.sink(&input[offset..offset + to_sink]).map_err(|_| ())?;
        if !(matches!(sres, DecoderSink::Full) && sunk == 0) {
            offset += sunk;
        }
        loop {
            let (pres, polled) = dec.poll(&mut tmp).map_err(|_| ())?;
            exp.extend_from_slice(&tmp[..polled]);
            if pres != DecoderPoll::More {
                break;
            }
        }
    }
    let fin_name;
    loop {
        let fres = dec.finish().map_err(|_| ())?;
        loop {
            let (pres, polled) = dec.poll(&mut tmp).map_err(|_| ())?;
            exp.extend_from_slice(&tmp[..polled]);
            if pres != DecoderPoll::More {
                break;
            }
        }
        match fres {
            DecoderFinish::Done => {
                fin_name = "DONE";
                break;
            }
            DecoderFinish::More => continue,
        }
    }
    Ok((exp, fin_name))
}

fn do_encode(input: &[u8]) {
    println!("=== encode ===");
    println!("w={WBITS} l={LBITS} ibs={IBS}");
    match encode_all(input, 256) {
        Ok((comp, fin)) => {
            print!(
                "enc_ok=1\nenc_finish={fin}\nenc_len={}\nenc_hex=",
                comp.len()
            );
            print_hex(&comp);
            println!();
        }
        Err(()) => {
            println!("enc_ok=0\nenc_finish=ERROR\nenc_len=0\nenc_hex=");
        }
    }
}

fn do_decode(input: &[u8]) {
    println!("=== decode ===");
    println!("w={WBITS} l={LBITS} ibs={IBS}");
    let Ok((comp, _)) = encode_all(input, 256) else {
        println!("dec_ok=0\ndec_finish=ERROR\ndec_len=0\ndec_hex=");
        return;
    };
    match decode_all(&comp, 256) {
        Ok((exp, fin)) => {
            print!(
                "dec_ok=1\ndec_finish={fin}\ndec_len={}\ndec_hex=",
                exp.len()
            );
            print_hex(&exp);
            println!();
        }
        Err(()) => {
            println!("dec_ok=0\ndec_finish=ERROR\ndec_len=0\ndec_hex=");
        }
    }
}

fn do_roundtrip(input: &[u8], chunk: usize, stream: bool) {
    if stream {
        println!("=== stream ===");
        println!("w={WBITS} l={LBITS} ibs={IBS} chunk=1");
    } else {
        println!("=== roundtrip ===");
        println!("w={WBITS} l={LBITS} ibs={IBS}");
    }
    let Ok((comp, _)) = encode_all(input, chunk) else {
        println!("match=no\nin_len={}\nout_len=0", input.len());
        return;
    };
    match decode_all(&comp, chunk) {
        Ok((exp, _)) => {
            let match_ok = exp == input;
            println!(
                "match={}\nin_len={}\nout_len={}",
                if match_ok { "yes" } else { "no" },
                input.len(),
                exp.len()
            );
        }
        Err(()) => {
            println!("match=no\nin_len={}\nout_len=0", input.len());
        }
    }
}
