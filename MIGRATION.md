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

Export check uses plain column-0 prototypes from both headers.

## Out of scope

- Rewrite of CLI `heatshrink.c` (may still link against C or FFI)
- `HEATSHRINK_HAS_THEFT` property suite (disabled in Makefile)
- Separate static-alloc link ABI (`HEATSHRINK_DYNAMIC_ALLOC=0`); static *behavior* is covered in core tests

## Hook-trace

N/A — only compile-time `HEATSHRINK_MALLOC` / `HEATSHRINK_FREE` macros; no runtime callbacks.

## Behavior changed on purpose (approved)

None.

## SonarQube findings summary

Stored analysis read via **SonarQube (cloud-hosted MCP)** (no new scan). Project for this heatshrink-migration repo, long-lived branch `master` (analysis date `2026-10-02T00:38:18+0000`). PR analysis key `6` (hooks branch) had **zero** open issues. No Rust analysis uploaded yet.

### Library C (`heatshrink_encoder.c`, `heatshrink_decoder.c`)

**Zero** open issues and **zero** `TO_REVIEW` security hotspots. No `PE-NNN` rows.

### Skipped (not library modules)

| Rule | file:line | Severity / impact | C behavior | Rust belief |
|------|-----------|-------------------|------------|-------------|
| `c:S2612` | `heatshrink.c:139` | MAJOR / SECURITY | CLI `open`/`chmod`-style world-accessible permissions | CLI not ported; out of scope |
| `c:S128` | `heatshrink.c:412` | BLOCKER / MAINTAINABILITY | Unannotated switch fall-through in CLI | CLI not ported; out of scope |
| `c:S1763` ×7 | `test_heatshrink_dynamic.c` (173, 202, 228, 406, 433, 569, 748) | MAJOR / RELIABILITY | Unreachable code after control-flow exits in tests | Test harness kept as C; public-API cases ported to `api_suite.rs` without those dead paths |

## Tests

| Suite | vs C | vs FFI | Rust port |
|-------|------|--------|-----------|
| `test_heatshrink_dynamic.c` (public API) | `make test` | `make ffi-tests` | `heatshrink-core/tests/api_suite.rs` |
| `test_heatshrink_static.c` | `make test` | N/A (static alloc) | `static_integration_pseudorandom_roundtrip` |
| `test_heatshrink_dynamic_theft.c` | out of scope | out of scope | out of scope |

No white-box cases that `#include` `.c` internals. One dynamic test reads public struct fields (`hsd->input_size` / `input_index`); FFI keeps matching `#[repr(C)]` header field offsets and syncs them after each call.

Removed from FFI: none (0 cases).

## Export check

Pattern: plain prototypes (multi-line). Extraction: flatten headers, collect `heatshrink_{encoder,decoder}_*` names. Expected count: **12**. `make export-check` → OK (0 missing).

## Unsafe audit

```
grep -rn "unsafe" --include='*.rs' --exclude-dir=target
```

- `heatshrink-core`: only `#![forbid(unsafe_code)]` / docs
- `heatshrink-ffi`: all executable `unsafe` with adjacent `SAFETY:` (or `# Safety` on `unsafe extern "C"` allocators that only call safe core + `Box::into_raw`)
- `heatshrink-ffi/tests/abi.rs`: test-only `unsafe` calling the C ABI

## CI / Sonar policy

`.travis.yml` runs `make ci` (`make test`) only. There is no Sonar quality gate that fails on legacy C alone. A future Sonar gate must use new-code / PR analysis so legacy CLI/test smells do not block the Rust port.

## One-command demo

```sh
make parity
```
