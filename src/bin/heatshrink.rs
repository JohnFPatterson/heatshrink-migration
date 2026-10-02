//! Command-line heatshrink compressor/decompressor (Rust port of heatshrink.c).

use heatshrink::decoder::{Decoder, FinishRes as DFin, PollRes as DPoll, SinkRes as DSink};
use heatshrink::encoder::{Encoder, FinishRes as EFin, PollRes as EPoll, SinkRes as ESink};
use heatshrink::{AUTHOR, URL, VERSION_MAJOR, VERSION_MINOR, VERSION_PATCH};
use std::env;
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::process;

const DEF_WINDOW_SZ2: u8 = 11;
const DEF_LOOKAHEAD_SZ2: u8 = 4;
const DEF_DECODER_INPUT_BUFFER_SIZE: u16 = 256;
const POLL_OUT: usize = 4096;

struct Config {
    window_sz2: u8,
    lookahead_sz2: u8,
    decoder_input_buffer_size: u16,
    verbose: u8,
    decode: bool,
    in_fname: String,
    out_fname: String,
}

fn usage() -> ! {
    eprintln!(
        "heatshrink version {}.{}.{} by {}",
        VERSION_MAJOR, VERSION_MINOR, VERSION_PATCH, AUTHOR
    );
    eprintln!("Home page: {}\n", URL);
    eprintln!(
        "Usage:\n  heatshrink [-h] [-e|-d] [-v] [-w SIZE] [-l BITS] [IN_FILE] [OUT_FILE]\n"
    );
    process::exit(1);
}

fn parse_args() -> Config {
    let mut cfg = Config {
        window_sz2: DEF_WINDOW_SZ2,
        lookahead_sz2: DEF_LOOKAHEAD_SZ2,
        decoder_input_buffer_size: DEF_DECODER_INPUT_BUFFER_SIZE,
        verbose: 0,
        decode: false,
        in_fname: "-".into(),
        out_fname: "-".into(),
    };

    let mut args: Vec<String> = env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" => usage(),
            "-e" => {
                cfg.decode = false;
                i += 1;
            }
            "-d" => {
                cfg.decode = true;
                i += 1;
            }
            "-v" => {
                cfg.verbose = cfg.verbose.saturating_add(1);
                i += 1;
            }
            "-w" => {
                i += 1;
                cfg.window_sz2 = args.get(i).map(|s| s.parse().unwrap_or(0)).unwrap_or(0);
                i += 1;
            }
            "-l" => {
                i += 1;
                cfg.lookahead_sz2 = args.get(i).map(|s| s.parse().unwrap_or(0)).unwrap_or(0);
                i += 1;
            }
            "-i" => {
                i += 1;
                cfg.decoder_input_buffer_size = args
                    .get(i)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(DEF_DECODER_INPUT_BUFFER_SIZE);
                i += 1;
            }
            s if s.starts_with('-') => usage(),
            _ => break,
        }
    }

    let rest = args.split_off(i);
    if let Some(in_f) = rest.first() {
        cfg.in_fname = in_f.clone();
    }
    if let Some(out_f) = rest.get(1) {
        cfg.out_fname = out_f.clone();
    }
    cfg
}

fn main() {
    let cfg = parse_args();
    if cfg.in_fname == cfg.out_fname && cfg.in_fname != "-" {
        eprintln!(
            "Refusing to overwrite file '{}' with itself.",
            cfg.in_fname
        );
        process::exit(1);
    }

    let mut input = match open_input(&cfg.in_fname) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            process::exit(1);
        }
    };
    let mut output = match open_output(&cfg.out_fname) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("{e}");
            process::exit(1);
        }
    };

    let result = if cfg.decode {
        decode_stream(
            &mut input,
            &mut output,
            cfg.window_sz2,
            cfg.lookahead_sz2,
            cfg.decoder_input_buffer_size,
        )
    } else {
        encode_stream(&mut input, &mut output, cfg.window_sz2, cfg.lookahead_sz2)
    };

    let (total_in, total_out) = match result {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{e}");
            process::exit(1);
        }
    };

    if cfg.verbose > 0 {
        let pct = if total_in > 0 {
            100.0 - (100.0 * total_out as f64) / total_in as f64
        } else {
            0.0
        };
        eprintln!(
            "{} {:.2} %\t {} -> {} (-w {} -l {})",
            cfg.in_fname, pct, total_in, total_out, cfg.window_sz2, cfg.lookahead_sz2
        );
    }
}

fn open_input(path: &str) -> io::Result<Box<dyn Read>> {
    if path == "-" {
        Ok(Box::new(io::stdin()))
    } else {
        Ok(Box::new(BufReader::new(File::open(path)?)))
    }
}

fn open_output(path: &str) -> io::Result<Box<dyn Write>> {
    if path == "-" {
        Ok(Box::new(io::stdout()))
    } else {
        Ok(Box::new(BufWriter::new(File::create(path)?)))
    }
}

fn encode_stream(
    input: &mut dyn Read,
    output: &mut dyn Write,
    window_sz2: u8,
    lookahead_sz2: u8,
) -> io::Result<(u64, u64)> {
    let mut hse = Encoder::alloc(window_sz2, lookahead_sz2).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "failed to init encoder: bad settings",
        )
    })?;

    let window_sz = 1usize << window_sz2;
    let mut chunk = vec![0u8; window_sz];
    let mut total_in = 0u64;
    let mut total_out = 0u64;

    loop {
        let read_sz = input.read(&mut chunk)?;
        if encoder_sink_read(output, &mut hse, &chunk[..read_sz], &mut total_out)? {
            break;
        }
        if read_sz > 0 {
            total_in += read_sz as u64;
        } else if read_sz == 0 {
            // Keep calling with empty input until finish completes (matches heatshrink.c).
            continue;
        }
    }
    Ok((total_in, total_out))
}

fn encoder_sink_read(
    out: &mut dyn Write,
    hse: &mut Encoder,
    data: &[u8],
    total_out: &mut u64,
) -> io::Result<bool> {
    let mut sunk = 0usize;
    let mut last_poll = 0usize;
    loop {
        if !data.is_empty() {
            let mut sink_sz = 0usize;
            if hse.sink(&data[sunk..], &mut sink_sz) != ESink::Ok {
                return Err(io::Error::new(io::ErrorKind::Other, "sink"));
            }
            sunk += sink_sz;
        }

        loop {
            let mut poll_buf = [0u8; POLL_OUT];
            let mut poll_sz = 0usize;
            let pres = hse.poll(&mut poll_buf, &mut poll_sz);
            if pres == EPoll::ErrorMisuse || pres == EPoll::ErrorNull {
                return Err(io::Error::new(io::ErrorKind::Other, "poll"));
            }
            out.write_all(&poll_buf[..poll_sz])?;
            *total_out += poll_sz as u64;
            last_poll = poll_sz;
            if pres != EPoll::More {
                break;
            }
        }

        if data.is_empty() && last_poll == 0 {
            match hse.finish() {
                EFin::Done => return Ok(true),
                EFin::More => {}
                EFin::ErrorNull => return Err(io::Error::new(io::ErrorKind::Other, "finish")),
            }
        }

        if sunk >= data.len() {
            break;
        }
    }
    Ok(false)
}

fn decode_stream(
    input: &mut dyn Read,
    output: &mut dyn Write,
    window_sz2: u8,
    lookahead_sz2: u8,
    input_buffer_size: u16,
) -> io::Result<(u64, u64)> {
    let mut hsd = Decoder::alloc(input_buffer_size, window_sz2, lookahead_sz2).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "failed to init decoder")
    })?;

    let window_sz = 1usize << window_sz2;
    let mut chunk = vec![0u8; window_sz];
    let mut total_in = 0u64;
    let mut total_out = 0u64;

    loop {
        let read_sz = input.read(&mut chunk)?;
        if read_sz == 0 {
            if hsd.finish() == DFin::Done {
                break;
            }
            if decoder_sink_read(output, &mut hsd, &[], &mut total_out)? {
                break;
            }
            continue;
        }
        total_in += read_sz as u64;
        if decoder_sink_read(output, &mut hsd, &chunk[..read_sz], &mut total_out)? {
            break;
        }
    }
    Ok((total_in, total_out))
}

fn decoder_sink_read(
    out: &mut dyn Write,
    hsd: &mut Decoder,
    data: &[u8],
    total_out: &mut u64,
) -> io::Result<bool> {
    let mut sunk = 0usize;
    let mut last_poll = 0usize;
    loop {
        if !data.is_empty() {
            let mut sink_sz = 0usize;
            match hsd.sink(&data[sunk..], &mut sink_sz) {
                DSink::Ok => sunk += sink_sz,
                DSink::Full | DSink::ErrorNull => {
                    return Err(io::Error::new(io::ErrorKind::Other, "sink"));
                }
            }
        }

        loop {
            let mut poll_buf = [0u8; POLL_OUT];
            let mut poll_sz = 0usize;
            let pres = hsd.poll(&mut poll_buf, &mut poll_sz);
            if pres == DPoll::ErrorNull || pres == DPoll::ErrorUnknown {
                return Err(io::Error::new(io::ErrorKind::Other, "poll"));
            }
            out.write_all(&poll_buf[..poll_sz])?;
            *total_out += poll_sz as u64;
            last_poll = poll_sz;
            if pres != DPoll::More {
                break;
            }
        }

        if data.is_empty() && last_poll == 0 {
            match hsd.finish() {
                DFin::Done => return Ok(true),
                DFin::More => {}
                DFin::ErrorNull => return Err(io::Error::new(io::ErrorKind::Other, "finish")),
            }
        }

        if sunk >= data.len() {
            break;
        }
    }
    Ok(false)
}
