# Vala architecture

Load this reference for broad Wyrd/Vala advice, ownership questions, or a
choice that crosses observations, evaluation, drift, and analytical storage.
It is the orientation slice; load narrower references for mechanics.

## Boundary

Wyrd owns durable cards, auth, policy, tenancy, audit, and the only network
serving surface. Vala owns observation storage, telemetry projections,
evaluation, drift computation, and the analytical data plane. Skald owns model,
prompt, tool, and agent-runtime primitives. A client SDK projects the wire
contract; it does not become a second registry or durable runtime.

The public seam is typed and language-agnostic:

```text
client / agent → wyrd-server → Vala engine → Bifrost tables
                                      ↘ Source (read-only external data)
```

`Source` is a Card that describes how Vala reads an external system. Wyrd does
not write external warehouses, metric backends, or object stores. Bifrost is
Wyrd-owned analytical storage for observations, traces, evaluation records,
drift records, and audit projections; it is not a Card kind.

## Durable invariants

- A Card is the declared subject. A Run is one client execution. An Observation
  carries the authenticated tenant, the asserted and authorized `card_ref`, an
  opaque `run_id`, and the propagated `Wyrd-Request-Id`.
- Physical analytical identity is `(organization, logical table)`. Never use a
  shared physical table and hope a later predicate provides isolation.
- Server-owned system columns are stamped at ingest and survive buffering,
  file conversion, compaction, and query. Tenant checks fail closed.
- `wyrd-spec` contains IO-free wire types. Server and Vala crates own behavior;
  Python, Rust, and TypeScript clients only add ergonomic projections.
- Query and ingest routes enforce permissions, bounded windows, payload
  sensitivity, and audit requirements at the server boundary.

## Design choices

Keep the engine/data-plane split because analytical dependencies are expensive
and serving concerns require one lifecycle owner. Keep Postgres for catalog and
control state while object storage holds analytical bytes. Keep Arrow as the
in-process interchange and Parquet/Iceberg as durable representation. These
choices trade a larger operational surface for column pruning, replayable
snapshots, and cross-language interoperability.

## Failure signals and anti-patterns

Treat tenant mismatch, schema fingerprint conflict, duplicate batch IDs,
unbounded query input, stale catalog epochs, and missing observation context as
fail-closed errors. Retry only idempotent admission or append operations, and
make recovery visible rather than silently dropping rows.

Reject HTTP or gRPC listeners in Vala engine crates, Python-side durable state,
external write adapters, stringly card/run identity, and a second warehouse
noun or compatibility route. A UI view may explain Vala state but cannot be the
only way to query, ingest, evaluate, or govern it.

## Stable Wyrd anchors

- Protocol ownership and Bifrost vocabulary: `architecture/wyrd-design.md`
  sections “Observation identity” and “Bifrost”.
- Public Vala contracts: `crates/wyrd-spec/src/vala/`.
- Vala engines and analytical clients: `crates/vala/` and `vala-sdk`.
- Single serving owner: `crates/wyrd/wyrd-server/`.

## Primary grounding

- [Apache Iceberg overview](https://iceberg.apache.org/docs/latest/)
- [Apache Arrow columnar format](https://arrow.apache.org/docs/format/Columnar.html)
- [Apache DataFusion features](https://datafusion.apache.org/user-guide/features.html)
- [OpenTelemetry signals](https://opentelemetry.io/docs/concepts/signals/)
- [NIST AI RMF 1.0](https://nvlpubs.nist.gov/nistpubs/ai/NIST.AI.100-1.pdf)
- Wyrd anchors: `architecture/wyrd-design.md` §Bifrost; `crates/vala/`;
  `crates/wyrd/wyrd-server/`.
