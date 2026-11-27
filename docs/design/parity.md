# Current feature scope

The current implementation is qualified against its own contracts. Historical
API or representation parity is not a requirement, and no older Wire package
or runtime is retained as a comparison oracle.

| Area | Current contract |
| --- | --- |
| Portable codec | Sparse message roots through typed and explicit projection APIs, with optional request/response framing |
| Admission | Byte, allocation-charge, node and depth allowances; typed errors |
| Fields | Explicit bytes, text, lists, borrowed views and generic packed words |
| Domain adapters | Bool, finite floats, nonzero-i64 and heap-u64 Serde adapters |
| Host tooling | Validated graph, strict derive, named collection and rooted advice |
| Clients | Generated payload/method modules and one bundled TypeScript runtime |
| Identities | Optional host integration with published Platform Types |
| Execution and deployment | Owned downstream; no Wire endpoint or executor |

[Architecture](../DESIGN.md) describes ownership and current API decisions.
[Qualification](../../qualification/README.md) lists independent byte, custody,
allocation, malformed-input and cross-language checks.
[Performance](performance.md) defines measurement scope without implying speed
from correctness. [Release preparation](../release.md) covers archive and browser
checks, with publication and consumer rollout separately authorized.
