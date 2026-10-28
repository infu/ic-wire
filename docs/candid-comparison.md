# Current Candid comparison

IC Wire has one production Serde/Postcard codec. Wire derives host layout
metadata; Serde owns serialization. Packed<T> is an explicit field representation.
Candid and PocketIC remain in the unpublished benchmark package.

The pending SDK integration APIs have a separate
[0.7.18 before/after qualification](evidence/sdk-standard-0719/README.md):
the complete existing matrix and hotspots produce identical IC instructions,
native operation allocation/heap metrics and all 88 Wasm artifact pairs. Native time
varies. That study uses PocketIC 16.1.0 and retains its query-execution reuse
limit; it does not replace the released historical comparisons below or qualify
the added entry points' costs.

The [0.7.10 full comparison](full-comparison-0710.md) is the current measured
baseline. It covers all 21 maintained cohorts plus the focused hotspot corpus,
with both optimization profiles, IC instructions, frame bytes, repeated native
timing, allocation/heap measurements and uncompressed Wasm/code-section sizes.
It also checks released 0.7.9 with matched fixtures, including a separate
normalization to isolate Wire code from dependency/version changes.
All 13,590 Wire IC rows match 0.7.9 exactly. Wire code-section sizes are unchanged;
total Wasm differs by at most two bytes. The normalized host-cleanup comparison
produces 88 byte-identical artifacts. Candid-control differences are retained
in the report rather than folded into a Wire improvement claim.

## Main results

At opt-3, the standard Wire fixture uses **573,482 total / 518,734 code bytes**
versus Candid's **1,386,033 / 1,306,893**. The 100 ordinary-type fixture uses
**398,739 / 364,245** versus **2,081,252 / 2,012,071**. These are complete benchmark
modules, not projected net savings in an application retaining external Candid
endpoints. See the full report for every cohort/profile and the regression verdict.

The 298-case standard corpus favors Wire on **233 encodes and 275 decodes** at
opt-3. A byte-oriented 1 MiB blob decodes in **1,049,508 instructions**, versus
Candid's **1,065,374**. Its Wire frame is two bytes larger. The earlier report's
owned-blob decoding disadvantage does not describe this current result.

Large ordinary vectors remain explicit weak cases: generic 1 MiB Vec<u8> encode
costs **37.23× Candid**, and 16,384 wide i16 values cost **32.82×**, at opt-3.
At opt-z, ordinary wide u16 encode reaches **119.67×**, and signed-byte lists
reach **13.39×**. Current Packed supports i8 as well as the wider words. These are current comparisons,
not evidence that 0.7.10 introduced those gaps.

Byte-oriented fields avoid generic byte-vector work. For the matched wide-i16
case, bounded packed Wire uses about **33,489 encode / 33,476 decode instructions**
versus Candid's **80,320 / 1,408,211**, with two more frame bytes. Packed fields
can expand small values relative to ordinary Wire varints. Select representations
for the contract and workload; coordinate existing schemas and preserve retry bytes.
See the [field selection guide](packed-fields.md). The 109-case generic Packed
cohort wins all 436 primary encode/decode instruction comparisons across both
profiles, including the signed-byte cases.

Malformed-input cost is separate: at opt-3, rejecting a truncated 1 MiB ordinary
Vec<u8> costs **42,472,926 instructions** versus Candid's **24,367**. The
byte-oriented blob counterpart costs **636** versus **24,861**. Ordinary sequences
can traverse a long valid prefix before discovering truncation; byte-oriented
fields can reject an unavailable byte range before copying it. The full report
keeps these rejection losses visible alongside successful-codec results.

## Measurement boundaries

The current run locks Candid 0.10.39, Postcard 1.1.3 and Platform 0.6.16, using
Rust 1.99.1 and an explicit PocketIC 16.0.0 server admitted by Testkit. Historical
matrices used different source, dependencies and server versions; their numbers
are not matched regression baselines for this run.

`make check-benchmarks` checks correctness, lint and compiler-floor support.
It does **not** execute a performance regression gate. Use an explicitly prepared
`POCKET_IC_BIN` with `make evaluate-codec-matrix`, plus the separate hotspot run,
for the full comparison. See [development](development.md) and the report's
[retained evidence](evidence/full-comparison-0710/README.md) for reproduction.

Native time, IC instructions, allocations/heap, frame bytes, total raw Wasm and
code-section bytes are separate measurements. Gzip is not an optimization goal.
Hash-array Candid fixtures retain their nested schema; flat byte-oriented manifest
controls are also measured. Resource policies and schema-evolution guarantees
differ. Borrowing/reuse lanes are labelled separately, and container roundtrips
do not qualify every Rust collection for generated TypeScript metadata.
This is codec qualification, not endpoint authentication, latency or deployment proof.

## Historical evidence

- [One-pass decoding cut](evidence/one-pass-070/README.md): the released change
  transferring collection/custom-Serde normalization to consumers.
- [Expanded 0.6.13 matrix](full-comparison-0613.md) and
  [hotspot follow-up](hotspot-encoding-0613.md): earlier decoder and encoder results.
- [Packed promotion](packed-promotion-0611.md), [design study](bulk-fields-0611.md)
  and [sharing study](packed-sharing-0611.md): field representation decisions.
- [Earlier comparisons](history/candid-comparison.md): retired finite-codec and
  rejected experiments, reproducible only at their recorded source revisions.
