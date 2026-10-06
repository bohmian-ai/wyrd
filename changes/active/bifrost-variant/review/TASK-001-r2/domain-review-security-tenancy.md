# Security and Tenancy Domain Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `99c5871ec5ca664b9f54baa379b437ee09d66e95`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 10
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Prior remediation: `changes/active/bifrost-variant/review/TASK-001-r1/TASK-001-R1-close-variant-contract-gaps.md`
- Reviewed boundary: sensitivity refusal before provider or peer IO, raw Arrow
  Variant admission, tenant bindings and tripwires, audit hash-input
  preservation, and stable HTTP/gRPC problem identity.

The candidate remained the checked-out `HEAD` throughout this review.

## Authority and Source Coverage

| Boundary | Governing authority | Source and caller coverage | Result |
|---|---|---|---|
| Server-owned identity and object authorization | `AGENTS.md` §§2, 9; `architecture/wyrd-security-posture.md` §§Security principles, Authorization and policy | `oracle/planner.rs::acquire_and_materialize`; `oracle/mod.rs::{authorize_resolved_tables, plan_physical, authorize_payload_columns}`; real multi-peer Oracle journey | PASS |
| Sensitive Variant and Struct access | Spec INV-004 and AC-008; task scenario 2; `architecture/bifrost-design.md` §Public surface; `architecture/references/domain/olap-serving.md` | Table-owned classifications; gateway payload column list; optimized-plan projection/filter traversal including subqueries; interactive and analytical session journey | PASS |
| Raw Arrow trust boundary | Remediation `FIND-TASK-001-2`; spec Variant failure precedence and queue admission; `architecture/references/domain/olap-serving.md` §Admission and failure semantics | `tables/mod.rs::BuiltinTableDefinition::validate_variants`; recursive Struct/List traversal; `scribe/execution_lanes.rs::decode_rows`; native and projected callers through `scribe/{preprocess,ingress}.rs` | PASS |
| No ACK or WAL on malformed Variant | Remediation acceptance criterion 2; Scribe durable lifecycle authority | Native batches validate while `prepare_append` materializes slices; `prepare_and_dispatch` reaches the shard only after preprocessing; WAL append and ACK remain shard-owned after dispatch; server journey proves refusal and absence from persisted rows | PASS |
| Tenant isolation and physical identity | Spec INV-006; `architecture/wyrd-security-posture.md` §§Peer identity and distributed Oracle, Tenant and data isolation; `architecture/bifrost-design.md` tenant identity rules | Authenticated tenant to `TenantTableBinding`, catalog cut, footer/source, permission digest, and follower authority paths; audit range tenant validation; cumulative tenant paths and callers | PASS |
| Audit integrity and privacy | Task preserved behavior; `AGENTS.md` §2; `architecture/wyrd-security-posture.md` §Audit integrity and privacy | `tables/audit/{audit_log,projection}.rs`; `verification_runtime.rs::RetainedAuditRow`; staging-to-Variant projection and retained-row hash reconstruction | PASS |
| Stable remote problem details | Remediation `FIND-TASK-001-2` and `FIND-TASK-001-5`; stable-error rules in `AGENTS.md`; existing Wyrd gRPC error carrier | `gate/error.rs::{from_scribe,to_wyrd_error,into_status}`; `wyrd-client/src/error.rs::{from_grpc_status,from_problem_json}`; Oracle structured remote Variant carrier | PASS |
| Human decisions and drift constraint | Standing direction for this round | `RawValue` plus local `i128` lexical classification is used without workspace `arbitrary_precision`; Iceberg lineage uses the directed standard path. Neither introduces a security or tenancy mechanism. | PASS |

## Boundary Trace

### Sensitive query refusal

Oracle resolves the complete table set and authorizes its catalog-bound
`TableUid` identities in `oracle/planner.rs:224-264` before metadata,
manifest, or data-object reads. The optimized logical plan then passes
`authorize_payload_columns` at `oracle/mod.rs:3319` before
`create_physical_plan` at `oracle/mod.rs:3322`. The gate walks projections,
filters, and subqueries for the table-owned `vala.gateway.calls` sensitive
payload columns. A denial therefore creates no provider, physical plan,
admission charge, read-audit acceptance, or follower graph. The four-pod
journey at `wyrd-testing/tests/bifrost/oracle/published.rs:1110-1270` also
compares follower lease counters around the refusal.

Variant `->`, `->>`, and `to_json` remain expressions over the same authorized
root column. They add no leaf reader or alternate provider. Other sensitive
built-ins continue to require the same object-scoped table permission as their
whole columns; the gateway request/response payloads retain their established
additional tenant-wide `gateway_payload_read` decision.

### Raw Arrow admission and durable effects

`BuiltinTableDefinition::validate_variants` at `tables/mod.rs:215-239` checks
extension identity in declared-field order, then validates present values in
input-row and field order with the existing `EncodedVariant::from_bytes`.
Recursive Struct/List traversal keeps nested Variants under the top-level
field identity and skips null parents rather than interpreting their physical
placeholders. Failures retain their catalogued `BifrostError` in
`ScribeError::ContractViolation` and `IngestError::ContractViolation`.

Both projected and raw IPC inputs converge on `decode_rows` at
`scribe/execution_lanes.rs:533-581`. For native IPC,
`scribe/preprocess.rs::stamp_native_source` performs that validation while
`prepare_append` is still materializing the request. `prepare_and_dispatch`
does not call the shard mailbox until preprocessing and charging complete
(`scribe/ingress.rs:551-615`). A Variant refusal therefore drops local
reservations and prepared slices before the shard can append WAL, insert a
memtable row, or send durable ACK.

The ignored Postgres journey
`verification_runtime::builtin_variant_columns_are_refused_before_ack` covers
missing and foreign extension markers, invalid bytes, depth, size and
precedence for a predeclared built-in plus a Variant nested in a signal list.
It requires an exact typed refusal and then queries the tables to show only
accepted sentinel rows persisted.

### Tenant, audit, and error identity

No changed path derives tenant identity from SQL, Arrow metadata, Variant
content, object names, or remote problem details. Scribe continues to build
and validate `TenantTableBinding` from the authenticated tenant before shard
selection. Oracle binds the same authenticated tenant to catalog identities,
physical sources, permission digests, and distributed stage authority. Audit
publication rejects a staging row whose `data_tenant_id` differs from the
authenticated publication tenant before constructing a batch
(`tables/audit/projection.rs:87-151`).

Audit projection copies every existing hash input, encodes the staged canonical
JSON `detail` as Variant, and never recomputes or substitutes `entry_hash` or
`prev_hash`. The production journey decodes retained `detail`, canonicalizes
it again, and reconstructs the stored row's entry hash from all retained inputs
(`verification_runtime.rs:1046-1180,1778-1869`). Invalid Variant detail stops
the publication range rather than truncating or rewriting it.

The ingest error adapter attaches the existing Wyrd `wyrd-error-bin` canonical
problem document alongside `google.rpc.ErrorInfo`; the client prefers and
reconstructs that same problem document. This reuses the established Wyrd
transport carrier and exposes only catalogued Variant location/limit fields,
not input JSON, credentials, tenant paths, or provider diagnostics. Oracle's
distributed query path separately carries the existing tagged
`BifrostError` representation rather than parsing human `Display` prose.

## Drift Audit

No security- or tenancy-related `DRIFT` was found. The candidate reuses the
existing optimized-plan permission gate, authenticated tenant binding, Scribe
pre-dispatch validation point, catalogued Wyrd error model, canonical gRPC
problem carrier, and audit projection. It adds no policy engine, validation
service, permission cache, tenant selector, alternate audit sink, signing
layer, compatibility route, configuration option, repository check, or
bespoke lineage/security gate.

The round's human decisions are preserved: `serde_json` `arbitrary_precision`
is absent, and the lineage remediation adds no duplicate-ID scan or metrics
gate.

## Verification

This review ran:

```text
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  --features test-support \
  -E 'test(=gate::error::tests::every_ingest_error_has_one_transport_projection) | \
      test(=oracle::tests::gateway_payload_columns_require_payload_read_authority) | \
      test(~tables::audit::projection::tests::)'
```

Result: **10 passed, 0 failed**.

The remediation evidence also records the exact Postgres-backed
`builtin_variant_columns_are_refused_before_ack` and multi-peer
`variant_sql_registry_covers_every_session` journeys as passing on the
candidate, plus the original built-in/audit-hash journey. This independent
domain pass did not rerun those serialized Postgres journeys. That is a
verification limit only; their production paths and assertions were inspected
end to end, and no required source or domain report was unavailable.

## Proposed Findings

None.

## Overall Result

**PASS**
