# Heatshrink differential driver format

Both `build/oracle` (C) and `target/release/heatshrink-driver` (Rust) print the same text for the same fixture path argument.

## Fixed parameters

| Parameter | Value |
|-----------|-------|
| `window_sz2` | 8 |
| `lookahead_sz2` | 4 |
| decoder `input_buffer_size` | 256 |
| sink/poll I/O chunk | 16 bytes (forces streaming) |

## Usage

```
./build/oracle [--sections NAME[,NAME...]] <fixture-path>
./target/release/heatshrink-driver [--sections NAME[,NAME...]] <fixture-path>
```

Without `--sections`, print all sections in order: `encoder`, `decoder`, `roundtrip`.

Exit status `0` on success for every requested section; non-zero if any requested section fails.

## Sections

### `encoder`

Read the fixture as **raw** bytes. Compress. Print:

```
encoder ok <nbytes>
hex:<lowercase-hex>
```

`<nbytes>` is the compressed length in decimal. `hex:` is followed by exactly `2*nbytes` hex digits (no spaces). Empty compressed output is `encoder ok 0` then `hex:`.

### `decoder`

Behavior depends on the fixture path:

- If the path contains `/compressed/` (module fixtures under `tests/compressed/`): treat bytes as **already compressed**. Decompress and print:

```
decoder ok <nbytes>
hex:<lowercase-hex>
```

- Otherwise (raw fixtures under `tests/inputs/`): compress with this side's encoder, then decompress, and print the recovered plaintext in the same `decoder ok` / `hex:` form.

On API failure:

```
decoder err <message>
```

and non-zero exit.

### `roundtrip`

Read the fixture as raw bytes. Encode then decode. If recovered bytes equal the input:

```
roundtrip ok <nbytes>
hex:<lowercase-hex>
```

Otherwise:

```
roundtrip mismatch
```

and non-zero exit.

## Module mapping

| `.cursor/parity.json` module | `--sections` | fixtures |
|------------------------------|--------------|----------|
| `encoder` | `encoder` | `tests/inputs/**/*` |
| `decoder` | `decoder` | `tests/compressed/**/*` |

Full `stop` compare uses top-level fixtures (`tests/inputs/**/*`) with no extra `driver_args` (all three sections on raw inputs).
