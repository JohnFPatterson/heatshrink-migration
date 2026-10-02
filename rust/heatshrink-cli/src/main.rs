use heatshrink_core::{
    Decoder, DecoderFinishRes, DecoderPollRes, DecoderSinkRes, Encoder, EncoderFinishRes,
    EncoderPollRes, EncoderSinkRes,
};
use std::env;
use std::fs::File;
use std::io::{self, Read, Write};
use std::process;

fn read_all(path: &str) -> io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    if path == "-" {
        io::stdin().read_to_end(&mut buf)?;
    } else {
        File::open(path)?.read_to_end(&mut buf)?;
    }
    Ok(buf)
}

fn write_all(path: &str, data: &[u8]) -> io::Result<()> {
    if path == "-" {
        io::stdout().write_all(data)?;
    } else {
        File::create(path)?.write_all(data)?;
    }
    Ok(())
}

fn encoder_sink_read(
    hse: &mut Encoder,
    out: &mut Vec<u8>,
    scratch: &mut [u8],
    data: &[u8],
) -> io::Result<bool> {
    let mut sunk = 0usize;
    loop {
        if sunk < data.len() {
            let (sres, n) = hse.sink(&data[sunk..]);
            if sres != EncoderSinkRes::Ok {
                return Err(io::Error::new(io::ErrorKind::Other, "encoder sink failed"));
            }
            sunk += n;
        }

        let mut poll_sz;
        loop {
            let (pres, w) = hse.poll(scratch);
            out.extend_from_slice(&scratch[..w]);
            poll_sz = w;
            if pres != EncoderPollRes::More {
                break;
            }
        }

        if poll_sz == 0 && data.is_empty() && hse.finish() == EncoderFinishRes::Done {
            return Ok(true);
        }

        if sunk >= data.len() {
            break;
        }
    }
    Ok(false)
}

fn stream_encode(input: &[u8], window: u8, lookahead: u8) -> io::Result<Vec<u8>> {
    if input.is_empty() {
        return Ok(Vec::new());
    }
    let mut hse = Encoder::alloc(window, lookahead)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "bad encoder settings"))?;
    let mut out = Vec::with_capacity(input.len() + input.len() / 2 + 4);
    let mut scratch = vec![0u8; 4096];
    let window_sz = 1usize << window;

    let mut sunk = 0usize;
    while sunk < input.len() {
        let chunk_end = (sunk + window_sz).min(input.len());
        if encoder_sink_read(&mut hse, &mut out, &mut scratch, &input[sunk..chunk_end])? {
            return Ok(out);
        }
        sunk = chunk_end;
    }

    while !encoder_sink_read(&mut hse, &mut out, &mut scratch, &[])? {}

    Ok(out)
}

fn decoder_sink_read(
    hsd: &mut Decoder,
    out: &mut Vec<u8>,
    scratch: &mut [u8],
    data: &[u8],
) -> io::Result<bool> {
    let mut sunk = 0usize;
    loop {
        if sunk < data.len() {
            let (sres, n) = hsd.sink(&data[sunk..]);
            if matches!(sres, DecoderSinkRes::ErrorNull) {
                return Err(io::Error::new(io::ErrorKind::Other, "decoder sink failed"));
            }
            sunk += n;
        }

        let mut poll_sz;
        loop {
            let (pres, w) = hsd.poll(scratch);
            out.extend_from_slice(&scratch[..w]);
            poll_sz = w;
            if pres != DecoderPollRes::More {
                break;
            }
        }

        if poll_sz == 0 && data.is_empty() && hsd.finish() == DecoderFinishRes::Done {
            return Ok(true);
        }

        if sunk >= data.len() {
            break;
        }
    }
    Ok(false)
}

fn stream_decode(input: &[u8], ibs: u16, window: u8, lookahead: u8) -> io::Result<Vec<u8>> {
    let mut hsd = Decoder::alloc(ibs, window, lookahead)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "bad decoder settings"))?;
    let mut out = Vec::with_capacity(input.len() * 2);
    let mut scratch = vec![0u8; 4096];
    let window_sz = 1usize << window;

    let mut sunk = 0usize;
    while sunk < input.len() {
        let chunk_end = (sunk + window_sz).min(input.len());
        if decoder_sink_read(&mut hsd, &mut out, &mut scratch, &input[sunk..chunk_end])? {
            return Ok(out);
        }
        sunk = chunk_end;
    }

    while !decoder_sink_read(&mut hsd, &mut out, &mut scratch, &[])? {}

    Ok(out)
}

fn usage() -> ! {
    eprintln!(
        "Usage: heatshrink [-h] [-e|-d] [-v] [-w SIZE] [-l BITS] [-i SIZE] [IN] [OUT]\n\
         Defaults: encode, -w 11, -l 4, -i 256, in/out '-'"
    );
    process::exit(1);
}

fn main() {
    let mut args = env::args().skip(1);
    let mut decode = false;
    let mut verbose = false;
    let mut window: u8 = 11;
    let mut lookahead: u8 = 4;
    let mut ibs: u16 = 256;
    let mut in_path = "-".to_string();
    let mut out_path = "-".to_string();

    while let Some(a) = args.next() {
        match a.as_str() {
            "-h" | "--help" => usage(),
            "-e" => decode = false,
            "-d" => decode = true,
            "-v" => verbose = true,
            "-w" | "-l" | "-i" => {
                let opt = a;
                let v = args.next().unwrap_or_else(|| usage());
                let n: u16 = v.parse().unwrap_or_else(|_| usage());
                match opt.as_str() {
                    "-w" => window = n as u8,
                    "-l" => lookahead = n as u8,
                    "-i" => ibs = n,
                    _ => unreachable!(),
                }
            }
            x if x.starts_with('-') => usage(),
            x => {
                in_path = x.to_string();
                if let Some(o) = args.next() {
                    out_path = o;
                }
                break;
            }
        }
    }

    if in_path == out_path && in_path != "-" {
        eprintln!("Refusing to overwrite in-place");
        process::exit(1);
    }

    let input = read_all(&in_path).unwrap_or_else(|e| {
        eprintln!("read: {e}");
        process::exit(1);
    });
    let output = if decode {
        stream_decode(&input, ibs, window, lookahead)
    } else {
        stream_encode(&input, window, lookahead)
    }
    .unwrap_or_else(|e| {
        eprintln!("process: {e}");
        process::exit(1);
    });

    if verbose {
        eprintln!(
            "{in_path} {} -> {} (-w {window} -l {lookahead})",
            input.len(),
            output.len()
        );
    }

    write_all(&out_path, &output).unwrap_or_else(|e| {
        eprintln!("write: {e}");
        process::exit(1);
    });
}
