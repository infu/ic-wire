# IC Wire

Read TOKO-TOOLING.md, README.md and docs/DESIGN.md before implementation.
The installed Toko Tooling baseline is adopted; this overlay retains Wire's
product scope and stricter safety constraints.

- The replacement product crates have public releases. Qualify their current
  implementation independently; do not restore historical implementation
  dependencies or paired comparison lanes. Registry publication does not establish
  production deployment or authorize consumer migration.
- Reduce independent responsibilities and execution paths. Do not transplant the
  old implementation or its release system merely to obtain a working workspace.
  Reuse requires a destination contract, provenance and focused qualification.
- Preserve one production Serde/Postcard codec unless a separately justified
  design decision changes it. No private Postcard fork, parallel codec lanes or
  automatically rewritten consumer Serde representations.
- Keep codec code Wasm-safe and independent of SDKs, native tools and Candid.
  Endpoint execution, transport, authentication, retries and deployment belong
  downstream. Do not restore Wire Endpoint or an executor.
- Platform Types owns Principal/Account identities and Serde implementations.
  Wire owns optional schema integration; keep codec-only graphs small.
- Preserve pre-allocation admission and typed malformed-input errors. Account for
  trusted custom Serde work and consumer-owned collection normalization explicitly.
- The user authorized a hard cut: legacy API, wire, schema, acceptance and budget
  compatibility are not requirements. Keep one current implementation without
  versioned compatibility lanes. Record intentional contract changes and qualify
  safety and cross-language behavior. Actual deployment and persisted-state
  operations remain separately authorized actions.
- Avoid new abstraction layers and per-type codec specialization without measured
  justification. Prioritize total uncompressed Wasm at opt-z; report opt-3 losses,
  IC instructions, allocation and payload bytes separately. Do not optimize gzip.
- Keep product packages in a virtual root under crates/ and real qualification
  packages in a separate qualification/ workspace. Add no placeholder packages.
  Candid is an optional research comparator in private dev-only qualification.
  Use published Host, Build and Testkit for their owned native responsibilities.
- Tooling is installed through the authenticated exporter;
  `.toko-tooling.snapshot` records its source and owned bytes. Follow the installed
  baseline's immutable detailed references;
  do not follow a moving sibling baseline. The compiler is Rust 1.100.0.
  `make toko-tooling-check` admits the member declaration, Cargo.lock and snapshot;
  `make snapshot-check` verifies just the installed bytes.
- Keep lasting design decisions small and current; do not copy evidence trees,
  historical journals or broad orchestration before there is a real caller.
- Requested product releases follow the installed baseline and current package
  publication policy. Consumer migration, live persisted-state changes and deletion
  of the original repository require their own authorization.

## Dependency selection

Use ordinary Cargo-compatible requirements for registry libraries; each workspace
lockfile owns its qualified selection. Supplier updates require affected checks,
not automatic new benchmark claims. The runtime and derive crate have an exact
version coupling through their generated metadata API: update and qualify the pair
together. Both workspaces select the same exact authenticated Toko Tooling bundle;
read versions from their manifests, locks and snapshot rather than this overlay.

## Qualification layout

All qualification packages are immediate children of qualification/:
ic-wire-compatibility, ic-wire-consumer and ic-wire-instructions. They share one
virtual qualification/Cargo.toml and Cargo.lock. The maintainer's single-tier
layout correction supersedes the former separate instruction-workspace exception.
The initial consolidation preserved both previous graphs' selections. Subsequent
supplier updates are qualified independently: product thiserror is 2.0.21;
PocketIC requires 2.0.18 in qualification, where generic-array stays at 0.14.7.
No simulator dependency enters product Wire.

The parent qualification workspace denies unsafe code for the narrowly expected
Rust 2024 guest export attribute. The native instruction driver retains a local
forbid. Product packages continue to forbid unsafe code.

## Wire licence and contribution exception

For work after v0.31.0 the maintainer approved Apache-2.0 for Wire's first-party
product and an Apache-2.0 inbound CLA for external pull requests. Wire owns
LICENSE.md, CONTRIBUTING.md and CLA.md. Keep these files outside the tooling
snapshot during refreshes. Both workspaces inherit SPDX licence metadata and
the two public packages ship byte-identical copies of the root licence.
Vendored tooling and third-party components retain their suppliers' terms.
