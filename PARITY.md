# Parity report

Differential harness: `.cursor/hooks/c-rust-parity/parity_gate.py` comparing `build/oracle` vs `target/release/heatshrink-driver`.

## Known quirks reproduced

| Quirk | C location | Notes |
|-------|------------|-------|
| Signed promotion in search end check | `heatshrink_encoder.c:268` | `input_size - threshold` is signed; empty finish flushes immediately |
| Missing `break` after `HSES_FLUSH_BITS` | `heatshrink_encoder.c:236-239` | Fall-through returns `HSER_POLL_EMPTY` after flush |

## Oracle defect found

None. AddressSanitizer (gcc `-fsanitize=address,undefined`) over all `tests/inputs/*` and `tests/compressed/*` fixtures printed nothing (ASAN_OPTIONS=`detect_leaks=0`).

## Divergences

None. See `PARITY_EXCEPTIONS.md` (no PE rows).

## Hook-trace

N/A — no runtime allocator hooks.

## Final gate report

**[DEMO: lead stop / full parity gate]** — `parity_gate.py --force` after both modules ready:

```
# Parity report

Result: PASS. 7 fixtures, 7 compared: 7 identical, 0 logged exceptions, 0 diverged; 0 gate problems.

## Method

- Workspace: `/workspace`
- Build: `make parity-build` (exit 0)
- C oracle: `./build/oracle {input}`
- Rust port: `./target/release/heatshrink-driver {input}`
- Compared: stdout bytes and exit status (stderr not compared)
- Fixture globs: `tests/inputs/**/*` (7 files)

## Per-fixture results

| Fixture | Result | C status | Rust status |
|---|---|---|---|
| `tests/inputs/empty.bin` | identical | exit 0 | exit 0 |
| `tests/inputs/literal.bin` | identical | exit 0 | exit 0 |
| `tests/inputs/pattern.bin` | identical | exit 0 | exit 0 |
| `tests/inputs/prandom_4k.bin` | identical | exit 0 | exit 0 |
| `tests/inputs/repeat.bin` | identical | exit 0 | exit 0 |
| `tests/inputs/single.bin` | identical | exit 0 | exit 0 |
| `tests/inputs/tiny_buffers.bin` | identical | exit 0 | exit 0 |

## Divergences

None.

## Pins

- 12 oracle files pinned
- 14 fixtures pinned
- 2 ready modules pinned
```
