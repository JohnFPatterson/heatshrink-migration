# Differential driver format (heatshrink)

Both `tools/heatshrink-oracle.c` (linked against original C) and the Rust
`rust-driver` binary print the same text for the same fixture path.

## Invocation

```
./build/oracle <fixture> [--sections name[,name…]]
./target/release/rust-driver <fixture> [--sections name[,name…]]
```

Without `--sections`, print all sections in this order: `encode`, `decode`,
`roundtrip`, `stream`.

With `--sections`, print only the named sections, still in the order listed
above (not the order given on the command line).

## Fixed parameters

| Name | Value |
|------|-------|
| window bits (`w`) | 8 |
| lookahead bits (`l`) | 4 |
| decoder input buffer (`ibs`) | 32 |
| stream I/O chunk size | 1 |

Fixtures are raw uncompressed bytes. Drivers encode and/or decode them with
the parameters above. No fixture sidecars.

## Section formats

Each section begins with a header line `=== <name> ===` followed by
newline-terminated key=value lines. Hex is lowercase, no spaces.

### encode

Compress the fixture.

```
=== encode ===
w=8 l=4 ibs=32
enc_ok=<0|1>
enc_finish=<DONE|MORE|ERROR>
enc_len=<decimal>
enc_hex=<hex or empty>
```

`enc_ok=0` only if alloc/sink/poll fails. Empty input yields `enc_len=0` and
empty `enc_hex`.

### decode

Compress the fixture, then decompress that compressed byte stream. Print the
decompressed bytes (should match the fixture on success).

```
=== decode ===
w=8 l=4 ibs=32
dec_ok=<0|1>
dec_finish=<DONE|MORE|ERROR>
dec_len=<decimal>
dec_hex=<hex or empty>
```

### roundtrip

Encode then decode with a 256-byte I/O buffer. Report identity.

```
=== roundtrip ===
w=8 l=4 ibs=32
match=<yes|no>
in_len=<decimal>
out_len=<decimal>
```

### stream

Same as roundtrip but sink/poll with 1-byte buffers.

```
=== stream ===
w=8 l=4 ibs=32 chunk=1
match=<yes|no>
in_len=<decimal>
out_len=<decimal>
```

## Exit status

| Code | Meaning |
|------|---------|
| 0 | Fixture read and all requested sections printed |
| 1 | Usage / I/O / unexpected driver failure |

Library encode/decode failures inside a section set `*_ok=0` or `match=no`
but still exit 0 so the gate can compare the report text.
