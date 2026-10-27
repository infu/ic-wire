# SDK output and payload clients: focused 0.7.19 evidence

These additions provide caller-owned bounded output and payload-only clients
without an SDK codec fork. They are not a blanket speed optimization. Small
scoped outputs improve in this fixture; ordinary large lists and records have
regressions, existing API counters can regress when compiled with the added
paths, and the canister grows. Consumer adoption needs its own measurements.
Existing framed bytes, identities, resource admission and error variants remain
unchanged. Payload selection is explicit and never strips or detects `IW`.

## Source, inputs and scope

Baseline is 0.7.18 at `c748379325266d7fa614fefee9d745eb4a88acca`, with only
the new unpublished fixture sources and their manifest/lock changes copied in.
Candidate is this PR's source, identified by the exact
[compiled-input hashes](candidate-build-inputs.sha256); the
[baseline hashes](baseline-build-inputs.sha256) identify the old runtime.
Both use the same lock graph, fixture, values, compiler and profile settings.
[Identity](identity.json) and [toolchain/host details](toolchain-host.txt) retain
compiler, dependencies, source context and executable hashes. No SDK or endpoint
library is a build dependency of these canisters.

Measurements use Rust 1.99.1 (`58cf8f9c3`), wasm32-unknown-unknown,
`evaluation`/opt-z and `benchmark-speed`/opt-3, LTO, one codegen unit, aborting
panics, stripped symbols and no extra RUSTFLAGS. The prepared PocketIC 16.1.0
executable is SHA-256
`6a9a75ad9b4551d2b0f4a0d954ffb8bd5bbd0ebc44ac00290679b06ad0b2f8e6`.
This identifies the measured binary; a complete matching IC source closure is
not asserted. These are codec counter fixtures, not mainnet or Heavy16 results.

The [canister](../../../crates/ic-wire-benchmarks/examples/caller_output/mod.rs)
prepares unit, three u64 scalars, ordinary u8 lists, byte buffers, wide u64 lists
and records: 20 identical cases per variant. Limits are 2 MiB bytes, 64 MiB
allocation, 9 Mi nodes and depth 64. Each case has eight physical update calls per
variant, alternating their order (320 calls per profile). Every reply's exact
payload bytes match. All eight counter samples are equal within each case/path.
Small cases repeat 16 encodes per window, input counts above 4,096 repeat four,
and above 65,536 repeat once. One warm encode precedes each window.

Each update measures four encoding paths in both variants. Historical
`slice` alternatives use `encode_into` with sufficient reusable capacity;
historical `scoped` alternatives use fresh `encode`. All four paths admit the
same root type, a reference to the prepared value. Caller scratch is 128 bytes;
full slice storage and reusable capacity are prepared equally before counters.
The measured Counter0 windows include loop/call overhead and result disposal;
there is no subtraction. Preparation, expected-byte construction, extra candidate
validation encodes and IC transport are outside them. The raw `host_millis`
column spans those complete, unequal validation calls: it is not a matched
per-operation latency or an endpoint speed comparison.

## Isolated API wrappers

The maintained fixture puts each API family behind an identical `inline(never)`
wrapper boundary; library optimization inside each remains ordinary. This
isolates caller bodies without pretending to reproduce an installed SDK.
Each cell is mean instructions per encode. Negative changes are fewer
instructions. Old alternative and new path have identical encoded payloads.

### opt-3

| Case / input count | Path (old alternative → new) | Before | After | Change |
|---|---|---:|---:|---:|
| unit / 0 | reused → slice | 74.50 | 53.50 | -28.19% |
| unit / 0 | fresh → scoped | 69.38 | 53.38 | -23.06% |
| u64 / 0 | reused → slice | 253.50 | 135.50 | -46.55% |
| u64 / 0 | fresh → scoped | 543.38 | 82.38 | -84.84% |
| u64 / 2 | reused → slice | 549.50 | 450.50 | -18.02% |
| u64 / 2 | fresh → scoped | 839.38 | 384.38 | -54.21% |
| u8 / 16 | reused → slice | 1,122.50 | 1,366.50 | +21.74% |
| u8 / 16 | fresh → scoped | 1,414.38 | 1,339.38 | -5.30% |
| u8 / 262,144 | reused → slice | 12,583,513.00 | 15,467,163.00 | +22.92% |
| u8 / 262,144 | fresh → scoped | 12,583,791.00 | 19,399,521.00 | +54.16% |
| bytes / 262,144 | reused → slice | 262,880.00 | 262,891.00 | +0.00% |
| bytes / 262,144 | fresh → scoped | 263,158.00 | 263,158.00 | +0.00% |
| u64-list / 8,192 | reused → slice | 3,333,675.00 | 3,292,888.00 | -1.22% |
| u64-list / 8,192 | fresh → scoped | 3,333,962.50 | 3,587,996.50 | +7.62% |
| records / 256 | reused → slice | 684,113.50 | 717,830.50 | +4.93% |
| records / 256 | fresh → scoped | 684,401.38 | 854,974.38 | +24.92% |

### opt-z controls

| Case / input count | Path (old alternative → new) | Before | After | Change |
|---|---|---:|---:|---:|
| unit / 0 | reused → slice | 288.38 | 307.38 | +6.59% |
| unit / 0 | fresh → scoped | 387.38 | 256.38 | -33.82% |
| u8 / 262,144 | reused → slice | 24,380,547.00 | 25,429,151.00 | +4.30% |
| u8 / 262,144 | fresh → scoped | 24,381,000.00 | 33,031,780.00 | +35.48% |
| bytes / 262,144 | reused → slice | 263,412.00 | 263,380.00 | -0.01% |
| bytes / 262,144 | fresh → scoped | 263,865.00 | 263,895.00 | +0.01% |
| records / 256 | reused → slice | 1,198,842.38 | 1,100,094.38 | -8.24% |
| records / 256 | fresh → scoped | 1,199,295.38 | 1,364,446.38 | +13.77% |

### Existing API controls

The old API bodies were not rewritten, but their counters in this canister also
change. These regressions remain part of the result. Added generic paths can
change a compiled consumer; no regression-free adoption claim is made. The
cause of the code-generation changes was not attributed.

| Case / input count | Path (old alternative → new) | Before | After | Change |
|---|---|---:|---:|---:|
| unit / 0 | fresh | 69.50 | 69.50 | +0.00% |
| unit / 0 | reused | 74.50 | 74.50 | +0.00% |
| u8 / 262,144 | fresh | 12,583,793.00 | 19,399,669.00 | +54.16% |
| u8 / 262,144 | reused | 12,583,513.00 | 19,399,380.00 | +54.17% |
| records / 256 | fresh | 684,401.50 | 855,127.50 | +24.95% |
| records / 256 | reused | 684,113.50 | 854,838.50 | +24.96% |

The [complete isolated comparison](isolated-comparison.csv) includes every case,
profile, old alternative, absolute value, percentage and min/max. Its raw
[opt-z](isolated-evaluation/samples.jsonl) and
[opt-3](isolated-benchmark-speed/samples.jsonl) samples retain repetitions,
output hashes, capacity and complete-call host times.

## Mixed generic fixture sensitivity

A separately retained [mixed fixture](mixed-fixture.rs) performs the same work
with the four API bodies in one generic caller. Its
[complete comparison](mixed-comparison.csv),
[opt-z samples](mixed-evaluation/samples.jsonl) and
[opt-3 samples](mixed-benchmark-speed/samples.jsonl) are separate from the wrapper
dataset. It has the same root types and exact paired outputs. Large ordinary u8
fresh encoding rises 24,380,978 → 33,031,952 instructions in opt-z (+35.48%), and
12,583,811 → 19,399,687 in opt-3 (+54.16%). The wrapper fixture retains similar
bulk regressions. Changing the harness boundary is not evidence that consumer
regressions disappear. No instruction samples from the two fixtures are pooled.

## Uncompressed Wasm and memory capacity

| Fixture / profile | Total bytes before → after | Change | Code bytes before → after | Change |
|---|---:|---:|---:|---:|
| isolated / evaluation | 36,232 → 52,596 | +45.16% | 33,346 → 49,462 | +48.33% |
| isolated / benchmark-speed | 42,090 → 58,489 | +38.96% | 39,478 → 55,613 | +40.87% |
| mixed / evaluation | 36,661 → 51,309 | +39.96% | 33,755 → 48,213 | +42.83% |
| mixed / benchmark-speed | 45,663 → 59,400 | +30.08% | 43,025 → 56,542 | +31.42% |

The candidate fixture imports two additional generic API families; these are
whole-fixture footprint costs, not a universal crate size or an additive cost
for each API. [Isolated](isolated-sizes.json) and [mixed](mixed-sizes.json)
receipts retain code-section/total sizes and artifact hashes. Code sections were
read with `wasm-objdump -h`, not inferred from symbols or compressed size.

In both profiles/scopes, final per-call Wasm capacity ranges from 1,114,112 to
3,276,800 bytes for baseline and from 1,114,112 to 2,490,368 for candidate. The
same canister processes cases sequentially, so these are retained high-water
linear-memory capacities with preparation and validation included. They do not
measure output allocation counts, live heap, RSS, isolated per-case peaks or an
SDK's memory reservation policy. No live-memory savings claim follows.

## Host generator time

The [generator fixture](../../../crates/ic-wire-benchmarks/examples/caller_generation/mod.rs)
prepares identical valid metadata before timing: either one or 128 record types
and methods. It times validation, generation, diagnostics formatting and output
disposal. Eight blocks alternate executable order; each has four samples of 128
warm generations per family/size (32 samples per row). Both native executables
use `benchmark-speed` on x86_64 Linux. Every existing framed generated source is
byte-identical across variants, including its SHA-256 and length.

| Family / types and methods | Before mean µs | After mean µs | Change | Before sample range µs | After sample range µs |
|---|---:|---:|---:|---:|---:|
| Framed / 1 | 5.639 | 5.595 | −0.77% | 5.279–10.764 | 5.253–7.447 |
| Framed / 128 | 670.649 | 747.291 | +11.43% | 651.821–717.038 | 722.674–799.945 |
| New payload / 1 | — | 5.734 | — | — | — |
| New payload / 128 | — | 678.411 | — | — | — |

The large framed-generation regression is retained; tiny differences do not
establish a speed improvement. The shared host was not isolated from other work;
[load](host-load.txt) and [all generation samples](generation-samples.csv) are
retained. Payload generation has no released predecessor. Native codec time and
JavaScript codec time were not measured; IC encoding counters and host generation
are distinct metrics.

## Reproduce

Use the pinned Rust toolchains, Wasm target, Node/TypeScript versions and locked
dependencies from [development](../../development.md). Prepare PocketIC at the
recorded hash and set `POCKET_IC_BIN` explicitly; the runner uses Testkit-owned
startup/cleanup and refuses a shared external server URL. Use a new absolute
`run` directory, one owner per target directory, and new output directories.

From the PR checkout, prepare two disposable worktrees and copy only the common
unpublished benchmark wiring to the old library:

```sh
run=/absolute/new-sdk-output-run
mkdir -p "$run"
git worktree add --detach "$run/baseline" c748379325266d7fa614fefee9d745eb4a88acca
git worktree add --detach "$run/candidate" HEAD
cp "$run/candidate/Cargo.lock" "$run/baseline/Cargo.lock"
cp "$run/candidate/crates/ic-wire-benchmarks/Cargo.toml" "$run/baseline/crates/ic-wire-benchmarks/Cargo.toml"
for fixture in caller_output caller_output_instructions caller_generation; do
  cp -R "$run/candidate/crates/ic-wire-benchmarks/examples/$fixture" "$run/baseline/crates/ic-wire-benchmarks/examples/"
done
```

Select the actual development toolchain binaries (the original host had direct
Cargo/rustc binaries ahead of its rustup proxies), then build both profiles:

```sh
task_rustc=$(rustup which --toolchain 1.99.1 rustc)
export PATH="${task_rustc%/*}:$PATH"
export RUSTC="$task_rustc"
export RUSTDOC="${task_rustc%/*}/rustdoc"
for variant in baseline candidate; do
  if [ "$variant" = baseline ]; then features=caller-output-baseline; else features=caller-output; fi
  for profile in evaluation benchmark-speed; do
    CARGO_TARGET_DIR="$run/target-$variant" cargo build --locked --offline       --manifest-path "$run/$variant/Cargo.toml" -p ic-wire-benchmarks       --example caller-output-wasm --features "$features"       --target wasm32-unknown-unknown --profile "$profile"
    cp "$run/target-$variant/wasm32-unknown-unknown/$profile/examples/caller_output_wasm.wasm" "$run/$variant-$profile.wasm"
  done
done
unset IC_TESTKIT_POCKET_IC_URL
for profile in evaluation benchmark-speed; do
  CARGO_TARGET_DIR="$run/target-runner" cargo run --locked --offline     -p ic-wire-benchmarks --example caller-output-instructions --features caller-output,ic --     --baseline "$run/baseline-$profile.wasm" --candidate "$run/candidate-$profile.wasm"     --output "$run/results-$profile" --samples 8
done
```

For the mixed dataset, copy `mixed-fixture.rs` over `caller_output/mod.rs` in both
worktrees before building, use fresh target/result directories, and retain new
input/artifact hashes. It is a separate fixture, not an old absolute-path build
receipt reused as fresh evidence.

For host generation, build `--example caller-generation --profile benchmark-speed`
in each worktree, using `caller-generation,caller-output-baseline` for baseline
and `caller-generation` for candidate, with the same locked dependencies. Copy
the two executables before running them. Alternate their order for eight blocks;
invoke each as `BINARY --output NEW-DIRECTORY --samples 4 --iterations 128`.
Compare `framed-1.ts` and `framed-128.ts` byte-for-byte. Hashing, metadata preparation
and source-file writes are outside timed intervals.

For every comparison, divide each counter by its recorded repetitions, take the
arithmetic mean of eight samples and calculate `100 * (after / before - 1)`.
Host means average the 32 recorded amortized nanosecond samples and divide by
1,000. The raw CSV/JSONL preserves all inputs to these calculations.

## Focused validation

Validation logs are compressed without changing their decompressed bytes.

[Logs](validation/runtime-tests.log.gz) cover 2 unit, 6 borrowed, 48 native and 15
runtime tests. [Doctests](validation/doctests.log.gz) cover 9 executable examples and
5 compile failures. Strict runtime/fixture Clippy passes for both benchmark
configurations; the maintained `make check-caller-output` compiles all new
qualification entry points. Rust 1.91.0 checks pass for the library with
TypeScript metadata on [native](validation/msrv-native.log.gz) and
[wasm32](validation/msrv-wasm32.log.gz).

[TypeScript execution](validation/typescript-checks.log.gz) includes the existing
165 conformance vectors, 35,228 differential mutations, exact Wire frames and
1 MiB byte buffers, plus new payload vectors, malformed/trailing input,
zero-byte unit, byte ownership and consumer scalar checks. Strict NodeNext
compilation passes; [simulated big-endian checks](validation/typescript-endian.log.gz)
cover 54 cases and do not claim a native big-endian engine.

Run focused native/borrowed/runtime tests with `--features typescript`, the Rust
doctests, `make check-caller-output`, `make check-typescript` and
`make check-doc-links` in the supported environment. Full workspace CI,
macOS, the full Candid matrix, SDK lifecycle/rollback behavior and external
applications were not qualified by this batch. Saved retry bytes and publication
authority remain consumer-owned.
