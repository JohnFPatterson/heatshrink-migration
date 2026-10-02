#!/usr/bin/env bash
# Compare compressed output from the C oracle CLI and the Rust CLI on sample inputs.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
C_BIN="$ROOT/heatshrink"
RUST_BIN="$ROOT/rust/target/release/heatshrink"
W=8
L=4

make -C "$ROOT" heatshrink >/dev/null
cargo build --release --manifest-path "$ROOT/rust/Cargo.toml" -p heatshrink-cli >/dev/null

tmpdir=$(mktemp -d)
trap 'rm -rf "$tmpdir"' EXIT

samples=(
  ""
  "a"
  "aaaaa"
  "abcdabcd"
  "hello world"
)

fail=0
for s in "${samples[@]}"; do
  printf '%s' "$s" >"$tmpdir/in"
  "$C_BIN" -w "$W" -l "$L" -e "$tmpdir/in" "$tmpdir/c.out"
  "$RUST_BIN" -w "$W" -l "$L" -e "$tmpdir/in" "$tmpdir/rust.out"
  if ! cmp -s "$tmpdir/c.out" "$tmpdir/rust.out"; then
    echo "encode mismatch for sample len=${#s}" >&2
    fail=1
  fi
  "$C_BIN" -w "$W" -l "$L" -d "$tmpdir/c.out" "$tmpdir/c.dec"
  "$RUST_BIN" -w "$W" -l "$L" -d "$tmpdir/rust.out" "$tmpdir/rust.dec"
  if ! cmp -s "$tmpdir/c.dec" "$tmpdir/rust.dec"; then
    echo "decode mismatch for sample len=${#s}" >&2
    fail=1
  fi
  if ! cmp -s "$tmpdir/in" "$tmpdir/c.dec"; then
    echo "C roundtrip failed len=${#s}" >&2
    fail=1
  fi
done

if [[ $fail -eq 0 ]]; then
  echo "C/Rust CLI parity OK (${#samples[@]} samples, -w $W -l $L)"
fi
exit "$fail"
