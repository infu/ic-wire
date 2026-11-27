# Standalone Wire consumer

Private `ic-wire-consumer` qualifies an application-owned upload contract using
Wire's public API and Serde.

The request contains revision u64, UTF-8 key (32 bytes), opaque content (1 KiB),
explicit packed i16 samples (64 elements), and an ordinary u16 tag list (8 items).
The response contains revision and acceptance. Both records derive `Wire` and
end in an empty extension block. Its zero length adds one byte per frame. The
independent literal vectors fix the current method ID (7), direction, core field
order and extension block. JavaScript uses payload node limits. The Rust receipt
sets a weighted allocation allowance of 2,112 bytes and a 36-visit limit for
staging, sizing and writing the projected root.

The library uses `no_std` and `alloc`. The standalone binary uses `std` for its
entry point and assertions. It runs both codec directions and retains work with
`black_box`. It builds natively and for `wasm32-unknown-unknown` without an SDK. The library uses the
current codec directly. Its four application functions select the contract
identity, direction and allowances.

Run these commands from the repository root.

```sh
make test-consumer
make check-consumer-wasm
cargo run --manifest-path qualification/Cargo.toml -p ic-wire-consumer --features schema --bin wire-consumer-generate --locked -- qualification/target/consumer-client
IC_WIRE_CHECK_TYPESCRIPT=1 cargo test --manifest-path qualification/Cargo.toml -p ic-wire-compatibility --features typescript --test typescript consumer_cases --locked
```

The generator writes payload.ts, methods.ts and the matching runtime.ts into the
selected directory. The host CLI enables schema generation with the `schema`
feature. The native and TypeScript exchange runs the generated clients against
the bundled runtime and checks the returned frames in Rust.

The client generator and the host-side manifest example use the same
`upload_contract()` registry and method descriptor. Run `make check-api-manifest`
to compare current output with the checked-in manifest. A mismatch prints a
unified diff and fails CI. [Upload baseline](api/upload.json)

Run this command to compare a candidate while preserving the baseline.

```sh
cargo run --quiet --locked --manifest-path qualification/Cargo.toml -p ic-wire-consumer \
  --features schema,wire/api-manifest --example upload-manifest > /tmp/upload-candidate.json
diff -u qualification/ic-wire-consumer/api/upload.json /tmp/upload-candidate.json
```

The baseline records the sparse request and reply schemas, method ID and client
allowances. Final canister export coverage is tracked in
[IC Tooling #16](https://github.com/dragginzgame/ic-tooling/issues/16).

Linux CI runs `make check-wasm-host` to build and execute a Wasm consumer
that derives a schema for a payment containing a Platform Account, checks its
TypeScript declaration and round-trips account bytes. Node verifies zero host imports.
Strict checking requires the private compiler installation described in the
[parent README](../README.md). The generator itself needs neither npm nor Node.

Generation always prints bulk-field diagnostics. The bounded ordinary `tags`
list exercises the advice to use packed words. Append `--deny-warnings` after
the output directory to fail before artifact creation when warnings occur.
The library `clients(runtime, diagnostics)` requires a handler and reports the
payload and method generations separately. Production users acknowledge exact
paths with reasons through the Wire generation APIs, preserving field constraints.

Native tests check independent literal frames, every truncation, wrong method and
direction, oversized frames, trailing bytes and malformed boolean replies. The
Wasm workload runs under Node with zero host imports. The codec-only build uses
Wire and Serde derives at compile time. Schema generation and IC exports have
separate feature flags.

`make measure-consumer-wasm` measures current opt-z and opt-3 artifacts. The IC
instruction probe measures platform execution. [Measurement scope](../../docs/design/performance.md)
