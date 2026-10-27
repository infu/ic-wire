# IC Wire

A bounded Postcard binary protocol for controlled Rust endpoints on the Internet
Computer. IC Wire owns encoding, resource admission and typed
codec errors. Consumers own transport, authentication, endpoint integration,
application semantics and retry policy.

The production runtime uses ordinary Serde derives with one Postcard codec.
Decode validates primitives in one pass and requires only Deserialize; collection
and custom-visitor normalization is consumer-owned. Keep original bytes for retries.
Bounded byte/text/list and finite-float fields remain optional Serde types.
Runtime traversal supplies byte, allocation, node and depth admission; host-only
`Wire` metadata generates matching TypeScript without field IDs.
The former finite API is retired; `Wire` now generates metadata only. Alternative strategies
remain research rather than selectable production modes.

Start with the [runtime guide](docs/runtime.md), [design](docs/design.md),
[evaluation scope](docs/evaluation.md) and [current handoff](docs/status/current.md).
The runtime and derive crates were first published at 0.3.0; the private
`ic-wire-qualification` package remains unpublished.

For new contracts, default binary fields to `Bytes<N>`, `ic_wire::ByteBuf`
or the `serde_bytes` adapter, and large homogeneous integer/hash arrays to
`Packed<T>`. Keep ordinary collections for records and small values where compact
varints matter. These field types share the same Serde/Postcard codec; choosing
packed fields is explicit and requires coordinated consumers for existing layouts.
See the [field selection and adoption guide](docs/packed-fields.md).
For payload-only clients, caller-owned output and consumer scalar wrappers, see
[SDK integration without a codec fork](docs/sdk-integration.md).

## Development

The virtual root workspace owns package metadata, dependency selections and the
lockfile. See [local setup](docs/development.md) and [contributor instructions](AGENTS.md).
With the documented prerequisites prepared:

```sh
make install-tools
make tools-check
make install-hooks
make check
make fmt-check
make lint
make check-wasm
make check-msrv
```

Use focused targets for changed behavior. `make ci` is the complete scaffold gate
and requires an explicit local request. Release commands deliver Git versions
and tags; registry publication is a separate maintainer action.

## License

Licensed under the [MIT License](LICENSE).
