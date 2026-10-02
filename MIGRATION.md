# Heatshrink C-to-Rust migration

## Oracle shape

**Streaming binary codec.** Public surface is the incremental `sink` / `poll` / `finish` state machines on `heatshrink_encoder` and `heatshrink_decoder` (dynamic allocation). Drivers compare:

| Section | Observable |
|---------|------------|
| `encoder` | Compressed bytes for raw fixtures (hex + length) |
| `decoder` | Decompressed bytes for compressed fixtures, or encode-then-decode recovery for raw fixtures |
| `roundtrip` | Recovered plaintext must equal input |

Fixed driver parameters: `window_sz2=8`, `lookahead_sz2=4`, decoder input buffer 256, I/O chunk 16. See `tools/DRIVER_FORMAT.md`.

## Layout

| Path | Role |
|------|------|
| `heatshrink-core/` | All compression logic; `#![forbid(unsafe_code)]` |
| `heatshrink-ffi/` | Thin C ABI (`staticlib` + `cdylib`) |
| `heatshrink-driver/` | Rust differential driver |
| `tools/heatshrink-oracle.c` | C oracle driver (public headers only) |
| `*.c` / `*.h` | Original C oracle (unpatched) |

## Out of scope

- CLI program `heatshrink.c` (companion tool, not the library ABI)
- `test_heatshrink_dynamic_theft.c` / libtheft property suite (optional dependency)
- Static-allocation compile mode (`HEATSHRINK_DYNAMIC_ALLOC=0`) — dynamic API is the default and the FFI export set

## Hook-trace

**N/A.** Allocator hooks are compile-time macros (`HEATSHRINK_MALLOC` / `HEATSHRINK_FREE`), not runtime-installable callbacks.

## Behavior changed on purpose (approved)

| ID | Change | Approval | Pinning test |
|----|--------|----------|--------------|
| — | None yet | — | — |

## SonarQube findings summary

*(Filled in Phase 3 after reading the stored analysis via SonarQube cloud-hosted MCP — no new scan.)*

## Parity modules

| Module | Owner | `ready` |
|--------|-------|---------|
| `encoder` | lead | flipped when encoder section identical |
| `decoder` | subagent | flipped when decoder section identical |
