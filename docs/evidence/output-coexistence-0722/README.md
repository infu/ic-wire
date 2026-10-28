# Output coexistence follow-up on 0.7.21

This study checks a canister containing all four encoder APIs. The prior
caller-storage study builds separately selected output APIs; unchanged old-only
modules do not establish the cost of adding more generic serializer call sites
to the same module. These are codec-only wasm32 measurements, not an SDK/CDK,
canister lifecycle, native-time or mainnet comparison.

The producer feature audit found everything requested already present at
[`15c8b7a`](https://github.com/dragginzgame/ic-wire/commit/15c8b7ac881c008b2af0bd2c3d79502605a36d0d)
(Cargo version 0.7.21): caller-slice output and payload TypeScript from 0.7.19,
scoped output and standalone schema validation from 0.7.21, plus Serde crate-path
support. [Issue 29](https://github.com/dragginzgame/ic-wire/issues/29) is closed.
Upstream's named generic payload registry is sufficient; SDK method/transport
wrappers are consumer work. The earlier feature branch is superseded.

## Change and qualification identities

- **Old implementation:** restore only `crates/ic-wire` and `ic-wire-derive`
  from `c748379325266d7fa614fefee9d745eb4a88acca` into a detached 0.7.21 workspace.
  Keep head package metadata and dependency graph. This is an old-code control
  under the head graph, not the released 0.7.18 artifact.
- **Head:** unmodified runtime from `15c8b7a`, with the common benchmark wiring.
- **Fixed:** the same head plus [the native-only patch](native.patch).
  Ordinary callers retain sizing and writing in one private helper. Scoped
  fallback supplies its already-admitted size to that helper; slice and scoped
  scratch share a bounded slice pass. Both paths still serialize twice, use a
  fresh write budget and cannot write beyond the admitted length. API, bytes,
  acceptance, typed errors and input-drop-before-callback behavior are unchanged.
- Rust 1.99.1 (`58cf8f9c35b355199727627d07ae34a4e2846d67`), LLVM 23.1.1,
  wasm32-unknown-unknown; workspace `benchmark-speed` (opt-3) and `evaluation`
  (opt-z), LTO, one codegen unit, aborting panics, stripped symbols.
- **Guest** lock SHA-256:
  `9e9ffba99cb69b7bcc6e29f104c63bc7be7c9ba0e5a5f4a2e156e5701d16e3b1`.
  Guest fixture SHA-256:
  `b555ec974fb071361364847a1ca019a64ab21d2f7e02003e5bd0199a452e5965`.
- Exact PocketIC 16.1.0 binary SHA-256:
  `6a9a75ad9b4551d2b0f4a0d954ffb8bd5bbd0ebc44ac00290679b06ad0b2f8e6`.
  We do not assert a matching full IC source closure. Published upstream studies
  using PocketIC 16.0.0 are not numerical baselines for these counters.
- The retained host controller is built from the original feature experiment,
  source `6fe96d848493127b7638c36ca80261138a121c41`; executable SHA-256
  `15bab1be5452539f0ac46a768f98bbb6245ffd27d4f722c738c4bb25aa2eebfb`.
  Its embedded lock/fixture fields in raw `identity.json` describe the **host
  controller build**, not the freshly rebuilt guests. Their hashes are
  `3f659a3e3104aaa4b3d72a8484572f58ac553c5b3e3f82ef103c75cdb53bb9bb`
  and `a9411fe47767e89195eb760cedb8ff2cb60b33168bd6b6fe73e7a087d5dbaeba`.
  The new guest fixture only adds an immutable-reference coercion for upstream's
  mutable-slice return. The controller checks reply payloads, lengths and counts;
  it does not use its linked codec as a guest oracle.

## Method and results

Twenty inputs cover unit, three scalar u64s, four ordinary u8 vectors, four
ByteBufs, four ordinary u64 vectors and four record vectors. All four paths admit
the same borrowed root type. Limits: 2 MiB bytes, 64 MiB allocation ledger,
9 Mi nodes, depth 64; scoped scratch is 128 bytes. All four outputs coexist.
Old code uses reusable/fresh output as the slice/scoped alternatives.

Each paired study alternates variant order for eight physical update calls per
input, each warming its paths once before Counter0 windows. Small inputs use 16
operations/window; lengths over 4096 use four; over 65536 use one. Counters include
loop/call/output-disposal work without subtraction; setup, correctness checks
and IC transport are outside the windows. Updates avoid query-result caching.
All repeated counters are identical, payloads match, and head counters match
between the two independent paired studies. There are 1,280 physical updates
across both paired studies/profiles, and 160 distinct head/fixed counter pairs.

Instructions per operation, scoped path (old column is fresh-output control):

| Profile/input | Old implementation | Head | Fixed | Fixed vs head | Fixed vs old |
| --- | ---: | ---: | ---: | ---: | ---: |
| opt-3 u64 zero | 543.375 | 82.375 | 82.375 | 0% | -84.84% |
| opt-3 u8 / 262144 | 12,583,791 | 19,399,524 | 12,583,797 | -35.13% | +0.00005% |
| opt-3 records / 256 | 684,401.375 | 854,974.375 | 686,523.375 | -19.70% | +0.31% |
| opt-z u64 zero | 1,034.375 | 503.375 | 465.375 | -7.55% | -55.01% |
| opt-z u8 / 262144 | 24,381,020 | 33,031,789 | 32,769,695 | -0.79% | **+34.41%** |
| opt-z records / 256 | 1,206,218.375 | 1,371,358.375 | 1,362,450.375 | -0.65% | **+12.95%** |

The ordinary fresh/reused APIs share the opt-3 recovery. The caller-slice large
u8 path improves 15,467,163 → 6,160,892 instructions at opt-3. The fixed runtime
improves 141 pairs, ties 13 and regresses six relative to head. All six are:

| Profile/path/input | Head | Fixed | Change |
| --- | ---: | ---: | ---: |
| opt-3 slice u64 zero | 134.5 | 138.5 | +2.97% |
| opt-3 slice u64 seven | 134.5 | 138.5 | +2.97% |
| opt-3 slice u64 max | 449.5 | 450.5 | +0.22% |
| opt-z scoped ByteBuf / 4096 | 5,601.375 | 5,643.375 | +0.75% |
| opt-z scoped ByteBuf / 262144 | 263,913 | 263,955 | +0.016% |
| opt-z scoped u64 list / 16 | 19,240.375 | 19,274.375 | +0.18% |

The fix does not remove every regression relative to old code: opt-z ordinary
vectors, records and some small controls remain slower. See every absolute
result in [instructions.csv](instructions.csv) and residuals in
[summary.json](summary.json). No universal speedup or equivalence is claimed.

| Profile | Old total/code bytes | Head total/code bytes | Fixed total/code bytes |
| --- | ---: | ---: | ---: |
| opt-3 | 42,090 / 39,478 | 58,183 / 55,290 | 55,303 / 52,420 |
| opt-z | 36,189 / 33,303 | 52,350 / 49,075 | 47,935 / 44,670 |

These uncompressed whole-module sizes include the additional output APIs.
They do not show unused APIs adding code to every consumer. Artifact and
code-section hashes are retained in [artifacts.json](artifacts.json).
Head/fixed Wasm linear-memory capacities match for every sampled input; at the
large vector both finish at 2,490,368 bytes. This is high-water capacity after
fixture preparation and all operations, not live heap, allocation count or RSS.
Host round-trip milliseconds are retained but are not a matched latency study.

## Validation and cause

The previous compiler investigation found generic serializer call-site changes
altered inlining/loop optimization when the new APIs coexist. Sharing their
writing bodies recovers that optimization without skipping bounds or reducing
serialized work. Symbol-preserving diagnostic builds had the same code-section
hash as their stripped measurements. This is compiler attribution, not a claim
that a symbol name proves savings or that all compilers behave identically.

The existing standard and hotspot `codec-wasm` fixtures, Wire lane, are also
freshly built before/after under both profiles: all four artifact pairs are
byte-identical. This establishes unchanged code/size for these old-only
consumers, not universal behavior for every possible application. Their hashes
and artifacts are retained separately; no full matrix rerun is claimed.

Fresh head-based focused checks pass: 34 native tests including scoped/slice,
budget boundaries, inconsistent serializers and failure cleanup; 15 doctests
including borrow/future escape rejection; strict library/native-test and Wasm
fixture Clippy; Rust 1.91 Wasm compilation. Read-only source review found no
admission/lifetime issue. Check logs are retained. Full workspace/hosted CI,
macOS, native timings/allocations and SDK Heavy16 were not rerun here. Earlier
frozen evidence is unchanged and is not relabelled as qualification of this head.

## Reproduction and retained evidence

Use the pinned compiler and prepared wasm32 target. Keep one target owner per
checkout. A fresh directory is required for each instruction run. The committed
`caller-output-wasm` fixture and `caller-output-baseline` switch belong only to
qualification; production uses one codec.

For a head/fixed pair, create a detached worktree at `15c8b7a`. Copy only this
branch's benchmark manifest and `examples/caller_output/mod.rs` into it. Verify
both root manifests and locks match. In each checkout, select a distinct
`CARGO_TARGET_DIR` and build both profiles:

```sh
cargo build --locked --offline -p ic-wire-benchmarks \
  --example caller-output-wasm --features caller-output \
  --target wasm32-unknown-unknown --profile benchmark-speed
# Repeat with --profile evaluation. Retain the Wasm before any later build.
```

To reproduce the old-code control, start another detached worktree at the same
head, restore the two runtime crate directories from `c748379`, add the identical
benchmark wiring, and build with `--features caller-output-baseline`.
Do not replace head's root manifest/lock or compare a saved old PocketIC result.

Build the pinned host controller in a separate checkout of
[`6fe96d8`](https://github.com/infu/ic-wire/commit/6fe96d848493127b7638c36ca80261138a121c41):

```sh
cargo build --locked --offline --profile benchmark-speed \
  -p ic-wire-benchmarks --example caller-output-instructions \
  --features caller-output,ic
POCKET_IC_BIN=/path/to/verified/pocket-ic \
  target/benchmark-speed/examples/caller-output-instructions \
  --baseline /path/to/head.wasm --candidate /path/to/fixed.wasm \
  --output /fresh/results/fix-benchmark-speed --samples 8
```

Repeat with opt-z and the old/head pair. Raw baseline operation names always
say `slice-control-reused` / `scoped-control-fresh` because the controller was
written for old code; for the **head/fixed pair those slots are actual slice and
scoped APIs**. [summarize.mjs](summarize.mjs) uses counter slot order, not those
labels. Raw identity files are retained unchanged with their host-build meaning.

Extract [raw-results.tar.gz](raw-results.tar.gz) into a fresh directory, then run
`node summarize.mjs DIRECTORY`. Its retained layout includes all six guest
artifacts so the analyzer can verify counter populations, repeated head
observations, payloads and recompute code sizes. It emits `analysis/` CSV/JSON.
The Wasm files are evidence inputs, not deployment artifacts. [SHA256SUMS](SHA256SUMS)
binds the retained files; [source-identities.json](source-identities.json)
distinguishes controller/guest identities and local factory/SDK context.
