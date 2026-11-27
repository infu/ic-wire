# Check Wire API upgrades

Keep the `ValidatedRegistry::api_manifests` output from each supported release in its own baseline directory. Put each method manifest in a separate `.json` file. Include `modes.json` with the deployed invocation mode for every method. Supply the baseline directories in release order, followed by the directory with newly generated manifests.

```sh
cargo run --locked --manifest-path qualification/Cargo.toml \
  -p ic-wire-consumer --features evolution-qualification \
  --example evolution-upgrade -- check BASELINE_DIR [BASELINE_DIR ...] CANDIDATE_DIR
```

```json
{"packet": "update", "status": "query", "scan": "composite_query"}
```

When you retire an extension field, remove it from the schema and add its ID and previous name to `retired.json` in the same release. Keep the entry in subsequent releases. The outer key is the deployed method name. The inner key is the named extension record in that method’s manifest.

```json
{"packet": {"PacketRow": [{"id": 7, "name": "note"}]}}
```

The checker compares open case IDs against the baselines you supply. Keep a reservation for removed open case IDs in the owning application after their last baseline leaves the set. `retired.json` reserves extension field IDs.

An appendable tuple has a fixed prefix followed by optional tail slots. Add new slots only at the end. Keep each published slot's name, position and compatible representation. A retired tuple slot stays in the manifest and writers can leave it absent. Tuple tail positions are permanent and cannot use `retired.json`. Diagnostic paths use zero-based positions, so ID `3` appears as `.2`.

The checker follows old requests into the candidate service and candidate replies into each supported client. It exits `0` for compatible manifests and lists paths where a reader can discard data. It exits `1` at the first incompatible path and `2` for unreadable input or a comparison it cannot complete within its complexity limit. Dynamic custom checks also require manual review. The checker compares declared limits and shapes. Runtime qualification covers instructions, allocation, and custom Serde behavior.

An optional record extension with a fresh ID can be added in either direction.
A newly required request extension fails if a supported client can omit it.
An existing explicit recovery boundary can allow the request with the
incomplete branch discarded. A new required reply extension can be skipped by
old clients because its ID is unfamiliar and its body has an extent.
These checks follow the field through containers and nested records.
When new clients also need to read old service replies, keep additions
optional while those older services remain in use.

To try it with the checked-in `a` baseline, run the example with `fixture B OUTPUT_DIR` to export B, then pass the path to `a` and the output directory to `check`. B adds two optional fields that old clients skip. `fixture C OUTPUT_DIR` demonstrates an incompatible required request field.
