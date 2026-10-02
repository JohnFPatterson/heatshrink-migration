# Parity: heatshrink C vs Rust

## Summary

Differential drivers are **byte-identical** on all 12 fixtures. Parity gate: **PASS**.

## Method

- C oracle: `./build/oracle` (`tools/heatshrink-oracle.c` + original encoder/decoder)
- Rust: `./target/release/rust-driver` (`heatshrink-core`)
- Format: `tools/DRIVER_FORMAT.md`
- One-command: `make parity`

## Per-fixture results

| Fixture | Result |
|---------|--------|
| `tests/inputs/empty.bin` | identical |
| `tests/inputs/hello_world.bin` | identical |
| `tests/inputs/literal_abc.bin` | identical |
| `tests/inputs/pattern_abab.bin` | identical |
| `tests/inputs/prng_1024_s2.bin` | identical |
| `tests/inputs/prng_256_s1.bin` | identical |
| `tests/inputs/prng_4096_s3.bin` | identical |
| `tests/inputs/prng_64_s1.bin` | identical |
| `tests/inputs/repeat_a_256.bin` | identical |
| `tests/inputs/repeat_a_32.bin` | identical |
| `tests/inputs/single.bin` | identical |
| `tests/inputs/zeros_64.bin` | identical |

## Quirks matched (with C `file:line`)

1. **Encoder poll fall-through** — `heatshrink_encoder.c:236-239`: `HSES_FLUSH_BITS` lacks `break` before `HSES_DONE`, so flush always returns `HSER_POLL_EMPTY` in that call. Rust mirrors this in `Encoder::poll`.
2. **Integer promotion in search end check** — `heatshrink_encoder.c:268`: `msi > input_size - (fin ? 1 : lookahead_sz)` is evaluated in promoted `int` space, so finishing with empty input yields `0 > -1` and goes to `HSES_FLUSH_BITS` immediately. Rust uses `i32` for the same compare.
3. **Break-even match length** — `heatshrink_encoder.c:510-518`: reject matches with `match_maxlen <= break_even_point / 8`.

## Divergences

None.

## Oracle defect found

None. ASan/UBSan oracle sweep over all fixtures (`make asan-oracle`, gcc) printed nothing.

## Hook-trace

N/A — no runtime allocator hooks or callbacks (only compile-time `HEATSHRINK_MALLOC` / `FREE` macros).

## Known gaps

- Inputs outside `tests/inputs/**/*`
- stderr not compared
- Theft property suite out of scope
- Static-alloc *link* ABI out of scope (behavior covered in core tests)
- CLI `heatshrink.c` not rewritten

## Final parity gate report

```
# Parity report

Result: PASS. 12 fixtures, 12 compared: 12 identical, 0 logged exceptions, 0 diverged; 0 gate problems.

## Method

- Workspace: `/workspace`
- Build: `make parity-build` (exit 0)
- C oracle: `./build/oracle {input}`
- Rust port: `./target/release/rust-driver {input}`
- Compared: stdout bytes and exit status (stderr not compared)
- Fixture globs: `tests/inputs/**/*` (12 files)
- Exceptions file: `PARITY_EXCEPTIONS.md`
- Exception tests: not run (no exception rows)

## Per-fixture results

| Fixture | Result |
|---|---|
| `tests/inputs/empty.bin` | identical |
| `tests/inputs/hello_world.bin` | identical |
| `tests/inputs/literal_abc.bin` | identical |
| `tests/inputs/pattern_abab.bin` | identical |
| `tests/inputs/prng_1024_s2.bin` | identical |
| `tests/inputs/prng_256_s1.bin` | identical |
| `tests/inputs/prng_4096_s3.bin` | identical |
| `tests/inputs/prng_64_s1.bin` | identical |
| `tests/inputs/repeat_a_256.bin` | identical |
| `tests/inputs/repeat_a_32.bin` | identical |
| `tests/inputs/single.bin` | identical |
| `tests/inputs/zeros_64.bin` | identical |

## Divergences

None.

## Exceptions used

None.

## Gate problems

None.

## Pins

- 12 oracle files pinned
- 12 fixtures pinned
```
