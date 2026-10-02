# heatshrink C → Rust migration

## Oracle shape

**Streaming binary codec** (plus chunked stream).

Drivers (`tools/heatshrink-oracle.c` and `rust-driver`) compare:

| Section | Observable |
|---------|------------|
| `encode` | Compress fixture; finish status + hex |
| `decode` | Encode then decode; decompressed hex |
| `roundtrip` | Identity after encode→decode (256-byte chunks) |
| `stream` | Identity with 1-byte sink/poll chunks |

See `tools/DRIVER_FORMAT.md`. Default params: `w=8`, `l=4`, `ibs=32`.

## Layout

| Path | Role |
|------|------|
| `heatshrink-core/` | All codec logic; `#![forbid(unsafe_code)]` |
| `heatshrink-ffi/` | Thin C ABI (`staticlib` + `cdylib`) |
| `heatshrink-driver/` | Rust differential driver binary `rust-driver` |
| `*.c` / `*.h` | Original C oracle (unpatched) |
| `tools/heatshrink-oracle.c` | C differential driver |

## ABI scope

Dynamic allocation API (`HEATSHRINK_DYNAMIC_ALLOC=1`), matching shipped headers:

- `heatshrink_encoder_{alloc,free,reset,sink,poll,finish}`
- `heatshrink_decoder_{alloc,free,reset,sink,poll,finish}`

## Out of scope

- Rewrite of CLI `heatshrink.c` (may still link against C or FFI)
- `HEATSHRINK_HAS_THEFT` property suite (disabled in Makefile)
- Separate static-alloc link ABI (`HEATSHRINK_DYNAMIC_ALLOC=0`); static *behavior* is covered in core tests

## Hook-trace

N/A — only compile-time `HEATSHRINK_MALLOC` / `HEATSHRINK_FREE` macros; no runtime callbacks.

## Behavior changed on purpose (approved)

None yet.

## SonarQube findings summary

(Pending stored analysis read.)

## Tests

(Pending.)

## Export check

(Pending.)

## Unsafe audit

(Pending.)

## CI / Sonar policy

(Pending.)
