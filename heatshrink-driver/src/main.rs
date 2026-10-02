//! Differential driver matching `tools/heatshrink-oracle.c` / `tools/DRIVER_FORMAT.md`.

use heatshrink_core::{
    Decoder, DecoderFinishRes, DecoderPollRes, DecoderSinkRes, Encoder, EncoderFinishRes,
    EncoderPollRes,
};
use std::env;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

const WINDOW_SZ2: u8 = 8;
const LOOKAHEAD_SZ2: u8 = 4;
const DECODER_INPUT_SIZE: u16 = 256;
const IO_CHUNK: usize = 16;

fn encode_buf(input: &[u8]) -> Result<Vec<u8>, ()> {
    let mut hse = Encoder::alloc(WINDOW_SZ2, LOOKAHEAD_SZ2).map_err(|_| ())?;
    let mut out = Vec::with_capacity(input.len() + input.len() / 8 + 16);
    let mut sunk = 0usize;
    while sunk < input.len() {
        let chunk = (input.len() - sunk).min(IO_CHUNK);
        let (sres, n) = hse.sink(&input[sunk..sunk + chunk]).map_err(|_| ())?;
        if (sres as i8) < 0 {
            return Err(());
        }
        sunk += n;
        loop {
            let mut tmp = [0u8; IO_CHUNK];
            let (pres, pn) = hse.poll(&mut tmp).map_err(|_| ())?;
            if (pres as i8) < 0 {
                return Err(());
            }
            out.extend_from_slice(&tmp[..pn]);
            if pres != EncoderPollRes::More {
                break;
            }
        }
    }
    loop {
        let fres = hse.finish();
        if (fres as i8) < 0 {
            return Err(());
        }
        if fres == EncoderFinishRes::Done {
            break;
        }
        loop {
            let mut tmp = [0u8; IO_CHUNK];
            let (pres, pn) = hse.poll(&mut tmp).map_err(|_| ())?;
            if (pres as i8) < 0 {
                return Err(());
            }
            out.extend_from_slice(&tmp[..pn]);
            if pres != EncoderPollRes::More {
                break;
            }
        }
    }
    Ok(out)
}

fn decode_buf(input: &[u8]) -> Result<Vec<u8>, ()> {
    let mut hsd = Decoder::alloc(DECODER_INPUT_SIZE, WINDOW_SZ2, LOOKAHEAD_SZ2).map_err(|_| ())?;
    let mut out = Vec::with_capacity(input.len() * 2 + 64);
    let mut sunk = 0usize;
    while sunk < input.len() {
        let chunk = (input.len() - sunk).min(IO_CHUNK);
        let (sres, n) = hsd.sink(&input[sunk..sunk + chunk]).map_err(|_| ())?;
        if (sres as i8) < 0 {
            return Err(());
        }
        sunk += n;
        loop {
            let mut tmp = [0u8; IO_CHUNK];
            let (pres, pn) = hsd.poll(&mut tmp).map_err(|_| ())?;
            if (pres as i8) < 0 {
                return Err(());
            }
            out.extend_from_slice(&tmp[..pn]);
            if pres != DecoderPollRes::More {
                break;
            }
        }
        if sres == DecoderSinkRes::Full && n == 0 {
            continue;
        }
    }
    loop {
        let fres = hsd.finish();
        if (fres as i8) < 0 {
            return Err(());
        }
        if fres == DecoderFinishRes::Done {
            break;
        }
        loop {
            let mut tmp = [0u8; IO_CHUNK];
            let (pres, pn) = hsd.poll(&mut tmp).map_err(|_| ())?;
            if (pres as i8) < 0 {
                return Err(());
            }
            out.extend_from_slice(&tmp[..pn]);
            if pres != DecoderPollRes::More {
                break;
            }
        }
    }
    Ok(out)
}

fn print_hex(data: &[u8]) {
    print!("hex:");
    for b in data {
        print!("{b:02x}");
    }
    println!();
}

fn path_is_compressed(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str() == "compressed")
}

fn parse_sections(arg: &str) -> (bool, bool, bool) {
    let mut e = false;
    let mut d = false;
    let mut r = false;
    for tok in arg.split(',') {
        match tok {
            "encoder" => e = true,
            "decoder" => d = true,
            "roundtrip" => r = true,
            _ => {
                eprintln!("unknown section: {tok}");
                std::process::exit(2);
            }
        }
    }
    (e, d, r)
}

fn main() -> ExitCode {
    let mut want_encoder = true;
    let mut want_decoder = true;
    let mut want_roundtrip = true;
    let mut path: Option<String> = None;
    let mut args = env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--sections" {
            let Some(s) = args.next() else {
                eprintln!("--sections needs an argument");
                return ExitCode::from(2);
            };
            let (e, d, r) = parse_sections(&s);
            want_encoder = e;
            want_decoder = d;
            want_roundtrip = r;
        } else if a.starts_with('-') {
            eprintln!("unknown flag");
            return ExitCode::from(2);
        } else {
            path = Some(a);
        }
    }
    let Some(path) = path else {
        eprintln!("usage: heatshrink-driver [--sections a,b] <fixture>");
        return ExitCode::from(2);
    };
    let input = match fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("read: {e}");
            return ExitCode::from(2);
        }
    };
    let mut rc: u8 = 0;
    let p = Path::new(&path);

    if want_encoder {
        match encode_buf(&input) {
            Ok(enc) => {
                println!("encoder ok {}", enc.len());
                print_hex(&enc);
            }
            Err(()) => {
                println!("encoder err encode_failed");
                rc = 1;
            }
        }
    }

    if want_decoder {
        let compressed: Result<Vec<u8>, ()> = if path_is_compressed(p) {
            Ok(input.clone())
        } else {
            encode_buf(&input)
        };
        match compressed.and_then(|c| decode_buf(&c)) {
            Ok(dec) => {
                println!("decoder ok {}", dec.len());
                print_hex(&dec);
            }
            Err(()) => {
                println!("decoder err decode_failed");
                rc = 1;
            }
        }
    }

    if want_roundtrip {
        match encode_buf(&input).and_then(|e| decode_buf(&e)) {
            Ok(dec) if dec == input => {
                println!("roundtrip ok {}", dec.len());
                print_hex(&dec);
            }
            _ => {
                println!("roundtrip mismatch");
                rc = 1;
            }
        }
    }

    ExitCode::from(rc)
}
