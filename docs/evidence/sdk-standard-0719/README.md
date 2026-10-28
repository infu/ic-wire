# Existing benchmark suite qualification for the SDK API additions

The measured source is the prepared SDK integration change at
`5d535c0a7e7514885c4f7253e229586b5f8b137f`, compared with released 0.7.18 at
`c748379325266d7fa614fefee9d745eb4a88acca`. This study uses the repository's
existing benchmark fixtures and runners. The separate
[caller-output study](../sdk-output-0719/README.md) measures the added APIs.
Existing benchmarks exercise their original entry points; passing them cannot
establish the performance of a consumer that switches to the new entry points.

The complete comparison finished on 2026-10-28: **22 cohorts and 1,208 overlapping
case entries**, with both optimization profiles. Existing IC instruction costs,
frame bytes, native operation allocations/heap and Wasm artifacts are unchanged.
Native wall-clock measurements vary and are retained separately. Both revisions
also pass `make check-benchmarks`.

## Results

| Paired measurement | Comparisons | Result, 0.7.18 → SDK API change |
| --- | ---: | --- |
| Wire IC instruction rows | 13,590 | All equal; 0% change |
| Candid IC control rows | 12,080 | All equal; 0% change |
| Native allocation metric rows | 124,248 | All equal; zero absolute change |
| Native heap metric rows | 103,540 | All equal; zero absolute change |
| Input and encoded-frame identities | Every IC/native pair | Identical hashes and frame sizes |
| Wasm artifact pairs | 88 | All byte-identical, including code and total size |

Metric rows are individual metrics, not independent operations. Cohorts reuse
some cases; 1,208 is the sum of cohort case counts, not 1,208 unique schemas.
The [summary](summary.json), [instruction changes](instruction-changes.csv),
[allocation changes](allocation-changes.csv.gz), [heap changes](heap-changes.csv.gz)
and [artifact identities](artifacts.csv) retain the complete comparisons.

This establishes no IC instruction, native operation-allocation/heap or Wasm regression for the
existing benchmark entry points. Unused new entry points are removed from these
Wasm fixtures. It does not supersede the separate added-API study: that study's
ordinary-byte-vector instruction regressions, fixture growth and generation
costs still apply to its measured callers. No consumer SDK migration is qualified
by either codec-only study.

### Wire and Candid on the existing standard corpus

These counts are unchanged between the two revisions:

| Profile | Standard cases | Wire encode wins | Wire decode wins |
| --- | ---: | ---: | ---: |
| opt-z | 298 | 228 | 241 |
| opt-3 | 298 | 233 | 275 |

The remaining comparisons favor Candid; there are no ties. Every cohort's
primary-lane counts are in [Wire/Candid wins](wire-candid-wins.csv). Counts are
unweighted descriptions of this corpus, not a workload speed ratio.

Selected opt-3 instructions per operation, identical before and after:

| Standard input / operation | Wire native | Candid |
| --- | ---: | ---: |
| Unit, fresh encode | 385.25 | 5,957.75 |
| Unit, owned decode | 207.50 | 6,309.00 |
| Byte-oriented 1 MiB blob, fresh encode | 1,049,704 | 2,115,234 |
| Byte-oriented 1 MiB blob, owned decode | 1,049,508 | 1,065,374 |
| Ordinary 1 MiB byte vector, fresh encode | 78,644,279 | 2,112,114 |
| Ordinary 1 MiB byte vector, owned decode | 42,472,921 | 119,553,417 |
| Ordinary 16,384 wide i16 values, fresh encode | 2,638,675.25 | 80,397.25 |
| Ordinary 1 MiB byte vector, reject truncated | 42,472,926 | 24,367 |
| Byte-oriented 1 MiB blob, reject truncated | 636 | 24,861 |

Both 1 MiB Wire examples have 1,048,590-byte frames, versus Candid's 1,048,588.
Ordinary-vector encoding and late malformed-input detection remain explicit
weak cases. Byte-oriented fields and explicit `Packed<T>` have different
representation costs; adoption belongs to the consumer's contract.

Standard fixture sizes below are uncompressed bytes, unchanged before and after:

| Profile / codec | Total Wasm | Code section |
| --- | ---: | ---: |
| opt-z Wire | 377,601 | 337,789 |
| opt-z Candid | 933,474 | 869,313 |
| opt-3 Wire | 572,970 | 518,516 |
| opt-3 Candid | 1,386,289 | 1,306,893 |

These are whole benchmark modules, not the marginal size of an application
retaining external Candid endpoints.

### Native timing observations

The [native comparison](native-changes.csv.gz) retains all **41,416 paired
per-run medians**, their eleven-sample extrema and absolute times. Across both
codecs, 22,115 medians increase, 19,300 decrease and one is equal. Observed changes
range from −44.98% to +77.59%; this is not a causal attribution to library code.
The two runs are retained independently rather than pooled into a speed claim.

For example, opt-3 standard Wire unit fresh encode moves from 15.54 to 15.94 ns
(+2.59%) in run 1 and 15.10 to 14.90 ns (−1.32%) in run 2. A large repeated
slowdown also remains visible: opt-z `derive-10/record_000`, bounded Wire
bad-header rejection, moves from 11.77 to 18.91 ns (+60.72%) and 11.74 to
18.32 ns (+56.07%). Native executables have different hashes; only the Wasm
artifacts are byte-identical. These serial, non-interleaved measurements on a
shared host do not establish native performance equivalence or endpoint latency.

## Inputs and method

The original `evaluate-codec-matrix.sh` runs all 21 maintained cohorts, followed
by the separately selected hotspot corpus from the same frozen source. Every
cohort uses both `evaluation` (opt-z) and `benchmark-speed` (opt-3). IC artifacts
are wasm32 only. Native timings run on x86_64 Linux; neither native time nor IC
counters are endpoint latency measurements.

The old library receives the candidate's benchmark manifest, lockfile and new
unused benchmark example files. Existing fixture, native runner, IC driver,
Wasm driver and evaluation-script bytes are identical between variants. This
matches the dependency graph without importing the candidate's library code.
Cargo package versions remain 0.7.18; this is not a release or publication.

Both variants use Rust 1.99.1, the locked dependencies and the same prepared
PocketIC 16.1.0 executable, SHA-256
`6a9a75ad9b4551d2b0f4a0d954ffb8bd5bbd0ebc44ac00290679b06ad0b2f8e6`.
The executable identifies the measured runtime. A complete matching IC source
closure is not asserted. The factory and SDK revisions recorded with the
experiment are context only; neither is a benchmark build dependency.

`make check-benchmarks` runs separately on both variants. Its correctness,
Clippy and Rust 1.91 checks are distinct from the performance matrix. The Cargo
launcher selects actual compiler/tool binaries explicitly and puts minimum-
toolchain checks in separate targets; no host wrapper is assumed to honor
`RUSTUP_TOOLCHAIN` by itself.

Native timing uses the original defaults: two runs, eleven samples per run,
and at least two milliseconds per calibrated batch (or the repetition cap).
Setup, semantic equality and reporting occur outside timed intervals. Warmed
codec calls, loop/dynamic-call overhead and result destruction remain inside.
The native before/after files retain each run separately. Shared host load and
case ordering prevent small wall-clock differences from establishing a speedup.

Allocation and heap modes are separately instrumented binaries. Each records
one warmed operation per case/path, not eleven allocation or heap observations.
Heap instrumentation starts after input preparation and warmup: it describes
operation allocations, not total live process memory. The fixture output
capacity column belongs to precomputed `lane.bytes`; it is not a measurement
of every timed operation's buffer capacity.

IC Counter0 windows use four repetitions for frames at most 65,536 bytes and
one above that size. Each operation is warmed first. Six paths are retained:
fresh encode, owned or explicitly labelled borrowed decode, available buffer
reuse, bad-header rejection, truncated-input rejection and loop control. The
control is reported without subtraction. Setup, correctness checks and reply
transport are excluded. Every response frame matches its native oracle exactly.

Wire policy is 2 MiB frame bytes, 256 MiB allocation, 8 Mi nodes and depth 64.
Candid uses 2 MiB reply bytes, 128 Mi decoding and skipping quotas, 4,096 type
entries and 64 KiB headers. Those policies have different semantics. They are
identical across variants, but are not interchangeable resource guarantees.
Earlier SDK/raw-payload benchmarks used different adapters and policies and are
not pooled with this matrix.

## Query reuse and sample counts

The maintained IC driver sends three identical anonymous queries for each
unique profile/case/lane. A companion probe on the exact frozen binary and
baseline standard artifact observed one Wasm execution for those three returns.
A distinct sender caused a new execution with identical complete response bytes.
The proof identifies observed execution reuse; it does not identify an internal
cache layer or replace a complete replica-source closure.

Consequently, the report retains three returned counter samples without
claiming three independent Wasm executions per query input. Timed repetitions
inside the executed query are separate from these repeated returns. The original
benchmark driver is unchanged. Probe receipts and traces are retained with the
result so cleanup cannot leave the conclusion supported only by old temporary
paths.

Each variant has **4,962 unique query inputs and 14,886 returned queries**.
The [probe](query-reuse-probe.json), [traces](query-reuse-traces/) and
[owned-server receipt](query-reuse-server-receipt.json) support the reuse limit;
they are not a second three-execution performance sample set.

## Analysis and reproduction

The analyzer checks complete paired operation key sets, case counts, compiler,
source/runner/dependency identities, resource policies, exact input/frame hashes,
native sample medians and extrema, allocation/heap metrics and actual Wasm hashes.
Every case and regression remains visible; heterogeneous costs are not combined
into a traffic-weighted average. Code-section size and total uncompressed Wasm
are separate metrics. Compression is used only for evidence storage.

The [raw index](raw/index.json) binds every compressed cohort record to its
decompressed SHA-256. Those records retain IC samples, native samples,
allocation/heap metrics and executable/source identities. The
[source receipts](source-receipts/), [baseline source archive](baseline-source.tar.gz)
and [candidate source archive](candidate-source.tar.gz) preserve the measured
source, including the baseline normalization. The derived portable archive
checksums are in [sources.sha256](sources.sha256); original checksum receipts
remain under `execution/` with their recorded paths.

[Execution receipts](execution/) retain exact local commands, compiler selection,
completion markers and logs. Their old absolute paths describe this completed
run; copying or relocating them is not a new build. The two
[check receipts](execution/check-results.json) and compressed
[baseline](checks/check-benchmarks-baseline.log.gz) and
[candidate](checks/check-benchmarks-candidate.log.gz) logs record the passing
benchmark correctness, lint and compiler-floor checks. Hosted CI and macOS were
not run locally. This is not SDK Heavy16 or endpoint/runtime qualification.

Use the prepared compiler/target, locked dependency cache and explicit PocketIC
binary. Select a fresh run directory and one owner per Cargo target. Create two
detached worktrees at the source identities above. Copy only the candidate's
`Cargo.lock` and `crates/ic-wire-benchmarks/` into the old worktree. Preserve the
old library and evaluation scripts. Record the source manifests before running.

Run each variant serially with `RUNS=2 SAMPLES=11 BATCH_MS=2`, separate fresh
Cargo targets and fresh output paths:

```sh
make check-benchmarks
bash scripts/ci/evaluate-codec-matrix.sh /absolute/run/results-VARIANT
```

Then execute the hotspot cohort from that variant's frozen matrix source,
keeping output adjacent to the `source` directory:

```sh
export GIT_DIR="$(git rev-parse --absolute-git-dir)"
export GIT_WORK_TREE="$PWD"
cd /absolute/run/results-VARIANT/source
CODEC_SOURCE_SNAPSHOT="$PWD" CORPUS_FEATURE=hotspots \
  bash scripts/ci/evaluate-codecs.sh /absolute/run/results-VARIANT/hotspots
```

The benchmark scripts never install PocketIC implicitly. Set `POCKET_IC_BIN`
to the prepared executable and leave `IC_TESTKIT_POCKET_IC_URL` unset so Testkit
owns each server/instance lifecycle. Keep builds and native timing serial.
Source relocation needs fresh metadata/build checks and new receipts.

After both variants finish, preserve `receipts/identity.json` and the cache
probe receipts/traces under the run root, then analyze into a new directory:

```sh
node docs/evidence/sdk-standard-0719/analyze.mjs /absolute/run /absolute/new-analysis
```

Percent changes are `100 * (after / before - 1)`. Native CSVs compare the
recorded per-operation medians in each of the two runs separately. Allocation
and heap files retain absolute metric changes, including zero baselines.
The analyzer emits plain CSVs and source patches; this retained study
gzip-compresses native, allocation and heap CSVs and source patches after
analysis without changing their decompressed contents.
