# Parity report

Differential harness: `.cursor/hooks/c-rust-parity/parity_gate.py` comparing `build/oracle` vs `target/release/heatshrink-driver`.

## Known quirks reproduced

| Quirk | C location | Notes |
|-------|------------|-------|
| Signed promotion in search end check | `heatshrink_encoder.c:268` | `input_size - threshold` is signed; empty finish flushes immediately |
| Missing `break` after `HSES_FLUSH_BITS` | `heatshrink_encoder.c:236-239` | Fall-through returns `HSER_POLL_EMPTY` after flush |

## Oracle defect found

None yet (ASan results recorded below when run).

## Divergences

None intended. See `PARITY_EXCEPTIONS.md`.

## Gate reports

Paste final `build/parity-gate/parity-report.md` here when the full gate PASSes.
