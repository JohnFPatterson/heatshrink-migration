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

None yet.

## SonarQube findings summary

Stored analysis (do not start a scan): SonarQube Cloud project for this heatshrink-migration repo, branch `master` (analysis date 2026-10-02). PR analysis for the skilled-migration hooks PR had zero open issues at plan time. Project key is not recorded here.

**Library C** (`heatshrink_encoder.c`, `heatshrink_decoder.c`): **zero** open issues and **zero** `TO_REVIEW` security hotspots. No `PE-NNN` rows.

**Skipped (not library modules):** open issues on CLI `heatshrink.c` (`c:S2612`, `c:S128`) and `test_heatshrink_dynamic.c` (`c:S1763` ×7).

No Rust analysis uploaded yet. CI (`.travis.yml`) runs `make ci` only — there is no Sonar quality gate that fails on legacy C alone.

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

- `heatshrink-core`: only `forbid(unsafe_code)` (no executable `unsafe`)
- `heatshrink-ffi`: only crate with `unsafe`; every block has `SAFETY:`
- `heatshrink-ffi` denies `unsafe_op_in_unsafe_fn` and `clippy::undocumented_unsafe_blocks`

## Verification commands

```sh
make parity          # differential gate
make test            # C suites
make ffi-tests       # C suites linked to Rust FFI
make export-check
make asan-oracle
cargo test --workspace --target-dir target
cargo clippy --workspace --all-targets --target-dir target -- -D warnings
cargo fmt --all --check
```
