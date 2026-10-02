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

**[DEMO: SonarQube cloud-hosted MCP]** — read stored analysis only (no new scan).

- Project: heatshrink-migration (resolved via `search_my_sonarqube_projects`; key not written here)
- Context: long-lived branch `master`, analysis date `2026-10-02T00:38:18+0000`
- No pull-request analysis for this port branch yet; used `master`
- Security hotspots (`TO_REVIEW`): none
- Open issues on library encoder/decoder `.c`: **none**
- Open issues total: 9 (CLI + tests only). No `PE-NNN` proposed (no driver-visible safer behavior change on library C)

| Rule | C behavior | Rust belief |
|------|------------|-------------|
| `c:S2612` SECURITY @ `heatshrink.c:139` (MAJOR) | CLI `open(..., S_IRWXO)` grants world permissions on output files | Out of scope (CLI companion). Library/FFI never create files. Not treated as a port fix. |
| `c:S128` MAINTAINABILITY @ `heatshrink.c:412` (BLOCKER) | CLI switch fall-through | Out of scope (CLI). Note: library encoder has a related intentional fall-through at `heatshrink_encoder.c:236-239` which the port **matches** (documented in `PARITY.md`). |
| `c:S1763` RELIABILITY @ `test_heatshrink_dynamic.c:173,202,228,406,433,569,748` (MAJOR) | Unreachable code after early returns in tests | Out of scope (tests). Not a library behavior change. |

**CI / quality gate policy:** A Sonar quality gate must not fail the port on legacy C issues alone. Prefer new-code conditions. This repo has no Sonar CI gate wired in-tree yet; document when one is added.

**No `PE-NNN` rows.** Matching C remains the default for the library.

## Parity modules

| Module | Owner | `ready` |
|--------|-------|---------|
| `encoder` | lead | `true` |
| `decoder` | subagent | `true` |

## Unsafe audit

```sh
grep -rn "unsafe" --include='*.rs' --exclude-dir=target .
```

- `heatshrink-core`: only `#![forbid(unsafe_code)]` (no executable `unsafe`)
- `heatshrink-ffi`: all `unsafe` blocks/fns have `SAFETY:` comments and `# Safety` docs; `#![deny(unsafe_op_in_unsafe_fn)]` and `#![deny(clippy::undocumented_unsafe_blocks)]`
- `heatshrink-driver`: no `unsafe`

## Export check

Pattern: plain prototypes in `heatshrink_encoder.h` / `heatshrink_decoder.h` (no export macro / `{PREFIX}` N/A).

12 public functions; `nm` on `target/release/libheatshrink_ffi.a` exports all 12 (`comm -23` empty):

`heatshrink_encoder_{alloc,free,reset,sink,poll,finish}`, `heatshrink_decoder_{alloc,free,reset,sink,poll,finish}`.

## Tests ported

| Suite | Approach |
|-------|----------|
| `test_heatshrink_dynamic.c` encoding/decoding/integration samples | Ported case-for-case names into `heatshrink-core/tests/public_api.rs` (9 tests) |
| Null / alloc ABI | `heatshrink-ffi/tests/abi.rs` (4 tests) |
| `test_heatshrink_static.c` | Out of scope (static alloc mode) |
| `test_heatshrink_dynamic_theft.c` | Out of scope (libtheft) |
| White-box `static` internals | Not present as separate suites beyond state machines; no FFI removals |

## One-command demo

```sh
make parity
```

