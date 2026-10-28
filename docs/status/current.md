# Current handoff

## Released 0.7.21; pending 0.7.22 output coexistence follow-up

Baseline is `15c8b7ac881c008b2af0bd2c3d79502605a36d0d` / version 0.7.21.
All requested SDK producer APIs are already upstream: payload-only TypeScript,
caller slices, scoped scratch/fallback output, Serde crate-path support and
standalone schema validation. Issue #29 is closed. SDK method/transport wrappers
remain consumer-owned. The original feature proposal is superseded.

The pending compatible 0.7.22 fix shares existing bounded writing passes, keeping
preflight beside writing for ordinary callers and reusing its exact admitted
size for scoped fallback. Public APIs, bytes, admission, two Serialize visits,
errors and input-drop timing are unchanged. Own package versions remain 0.7.21;
there is no release, publication, installed SDK change or deployment.

The [new matched study](../evidence/output-coexistence-0722/README.md) rebuilds
old implementation, current head and candidate with the head dependency graph
and the identical PocketIC 16.1.0 executable. In a canister using all four output
APIs, opt-3 scoped ordinary-u8/262144 falls from 19,399,524 to 12,583,797
instructions (old control 12,583,791); opt-z still costs 34.41% above old.
Total/code bytes fall 58,183/55,290 to 55,303/52,420 at opt-3 and
52,350/49,075 to 47,935/44,670 at opt-z. All 160 distinct counter pairs and six
small regressions are retained. This is a partial performance fix.

Focused native tests (34), doctests (15), strict runtime/fixture Clippy and the
Rust 1.91 Wasm check pass. Four fresh standard/hotspot Wire artifact pairs
(both profiles) are byte-identical. Source review found no admission/lifetime issue.
These checks do not establish native timing, SDK Heavy16 behavior, mainnet
latency or complete elimination of regressions. No full CI gate was run.

## Preserved 0.7.21 handoff context

The following release-context observations and older studies retain their
original qualification scope; their hosted status was not rechecked here.

## Hosted CI groups

The workflow now runs `ci-core`, `ci-typescript` and `ci-benchmarks` independently
on each supported OS: nine validation jobs with separate checkouts/targets/tools
and unique failure artifacts. Node/npm preparation belongs to TypeScript only.
Existing `scaffold (OS)` status names remain lightweight final gates: all must
refuse unless every validation job succeeds. Public rulesets were empty at
inspection; classic branch-protection reads were forbidden, so existing status
names were preserved without changing remote rules.

One fixed Make roster owns the full gate list. Core filters out the two specialist
targets; both specialist groups admit tools before their owning target. The full
local/release `make ci` roster and order remain sequential and unchanged. No
shared snapshot runner was modified. Performance/allocation studies stay outside
ordinary CI. The TypeScript command now actually executes feature-enabled native
tests, including schema validation, alongside runtime/borrowed tests.

Actionlint and the disposable Make fixture pass actual group routing, complete
partition coverage, fail-fast/tool admission arguments, CLI roster-override
refusal, failure propagation and workflow final-gate refusal for failed/cancelled/
skipped results. The existing formatting/release-routing fixture also passes.
Logs: `target/parallel-ci-0721/`. Before this split, concurrent execution of the
three correctness binaries with eight threads each passed 75 tests (53 native,
16 runtime, six borrowed); logs: `target/parallel-suite-audit/`. Hosted execution
is pending delivery. No measured CI wall-time saving or full CI pass is claimed.

## Synthetic warning flood

The TypeScript host benchmark printed bulk advice on every iteration/control:
141,804 synthetic warnings per run. It now black-boxes the computed report;
all 36 normal warnings stay visible. The CI wrapper retains stderr in
`target/typescript-conformance/generation.log`, refuses synthetic Page warnings
before replay and preserves ordinary diagnostics/failure status.

The [warning study](../evidence/generator-warnings-0721/README.md) records all
settings/samples. All 229 generated files match; only host timings differ.
Seven-sample medians fall 60.90–66.69% from removing diagnostic output, not codec
or production generator work. Strict Clippy/TypeScript compilation, 165 vectors,
35,228 mutations, native/payload/admission controls and 54 simulated-endian cases
pass. Shell routing covers warning refusal/replay and generator exit 101.
Local source/logs: `target/warning-output-0721/`.

## Scoped payload output

`native::with_encoded(value, limits, scratch, consume)` selects scratch or owned
fallback once after preflight, using existing bounded Postcard writers. Owned
input is dropped before the synchronous callback, which never runs on encoding
failure. Borrow/future escape is rejected. Existing payload/generator/slice APIs
and framing are unchanged.

The [output study](../evidence/with-encoded-0721/README.md) retains raw settings,
samples and reproduction; local artifacts: `target/with-encoded-0721/`.
Existing owned Wasm is byte-identical to 0.7.20. Scoped u64 instructions:
opt-z 842 → 425 (-49.52%); opt-3 586 → 289 (-50.68%). Scratch-fitting output
allocates zero; large fallback allocates once. Scoped fixture total/code costs
+722/+704 bytes at opt-z, +2,490/+2,495 at opt-3. Regressions: opt-3 unit
+16 instructions (+6.27%); native unit +7.53%, native large vector +0.51%.
Query returns may reuse execution; native timings compare paths in one executable.
Four integration tests, three slice tests, one executable/two compile-fail
doctests, strict runtime/fixture Clippy and Rust 1.91 pass. Native allocation/heap
and focused IC studies pass; local server binding required sandbox escalation.

## Standalone schema validation

Host-only `typescript::validate_payload_schema` validates a root and complete
registry with existing generator checks, including unused definitions, alias
cycles and shared work/depth limits, without emitting a module. Three tests,
the existing payload control, strict Clippy, a doctest and Rust 1.91 pass.
The [validator study](../evidence/schema-validator-0721/README.md) records one/100-type
medians of 1,414/83,863 ns versus 5,178/158,205 ns for generating a disposable
module (-72.69%/-46.99%). All 229 generated files match. Transient generation
regressions and repeats remain in evidence; no IC/Wasm cost change applies.

## Dependency and consumer boundaries

The root now requires Build/Testkit 0.69.8 and Platform 0.7.4; the managed
release editor selects Build 0.69.8 with `cli`. The incoming lock already selected
these packages and host companions, plus AWS-LC updates; it is preserved exactly
against `target/ic-adoption-0698/incoming.Cargo.lock`. Shared Tooling stays 0.27.3
at `7ee5c83744aa822ecd070703dbf0a7d95076d5f6`; no snapshot files changed.

Published Tooling source is `cc9e544d1236c23c78d2732cfd04080e7e38052a`; Platform
source is `ca2ac1eb3b65a4e3adec2eb64f06bf942ca22af9`. Tooling changes retain public
APIs/recovery formats while simplifying custody and liveness handling. Platform's
new external-protocol capabilities stay disabled. Existing Candid/PocketIC/ic0
selections remain qualification-only; published Wire runtime has no SDK dependency.

`target/ic-adoption-0698/` records passing strict diagnostic/Testkit caller Clippy,
nine compiler-fail cases plus control, two standard Wire/Candid roundtrip tests,
Rust 1.91 Build/Testkit native and scoped-output Wasm checks, pin/boundary checks,
managed editor admission and the disposable real release-adapter fixture. Setup
reused a verified editor cache after an escalation for its managed-store lock.
Managed PocketIC startup/calls/stop/delete passes with the new host graph against
eight retained known guest modules. This tests host lifecycle integration, not
fresh guest builds or a new performance matrix. Retained IC replies/counters
match the prior study. Local binding required sandbox escalation.

Earlier performance studies above freeze Build/Testkit 0.69.6 and Platform 0.7.3;
their results were not relabelled or rerun for this adoption. No runtime/generator
implementation changed and no new speed/Wasm saving is claimed.
[#29](https://github.com/dragginzgame/ic-wire/issues/29) and
[#30](https://github.com/dragginzgame/ic-wire/issues/30) landed in 0.7.19;
registry-consumer evidence: `/tmp/ic-wire-0719-acceptance.pyWIBU/`. Native hosted
qualification was their remaining producer closure gate at the previous check;
issue/hosted status was not rechecked here. The [SDK recipe](../sdk-integration.md)
retains transport, copying, rollback, retries and consumer adoption ownership.
Allocation admission retains its eightfold ledger. Historical
[caller-storage](../evidence/caller-storage-0719/README.md),
[runtime](../evidence/runtime-0716/README.md) and
[encoder](../evidence/encoder-followup-0716/README.md) studies keep their frozen inputs.
No full Candid matrix, installed SDK or native macOS qualification ran locally.
