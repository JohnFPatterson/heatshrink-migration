# heatshrink (Rust)

Rust port of the [heatshrink](https://github.com/atomicobject/heatshrink) LZSS compressor. The original C sources in the repository root remain the behavioral reference; this workspace is intended to match their observable output for the dynamic-allocation API.

## Layout

| Crate | Role |
|-------|------|
| `heatshrink-core` | Safe encoder/decoder (`#![forbid(unsafe_code)]`) |
| `heatshrink-ffi` | C ABI (`heatshrink_encoder_*`, `heatshrink_decoder_*`) for dynamic allocation |
| `heatshrink-cli` | `heatshrink` command-line tool |

## Build & test

```bash
cd rust
cargo test
cargo clippy --all-targets -- -D warnings
cargo build --release -p heatshrink-cli
```

Compare CLI output against the C binary:

```bash
./scripts/c_cli_parity.sh
```

## Library usage (Rust)

```rust
use heatshrink_core::{encode_all, decode_all};

let data = b"hello world";
let compressed = encode_all(data, 8, 4).unwrap();
let restored = decode_all(&compressed, 256, 8, 4).unwrap();
assert_eq!(restored, data);
```

## Notes

- Static-allocation (`HEATSHRINK_DYNAMIC_ALLOC=0`) and embedding the C struct layout in application memory are not yet exposed through the Rust FFI crate; use the original C objects or dynamic FFI allocators.
- Encoder indexing follows `HEATSHRINK_USE_INDEX=1` (enabled by default via the `use-index` feature on `heatshrink-core`).
