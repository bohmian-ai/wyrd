---
id: BIFROST-OTEL-T02
title: Converge OTLP and canonical Arrow before Scribe WAL preparation
kind: implementation
mode: DECOMPOSE
status: proposed
spec: SPEC-bifrost-canonical-otel-signals
spec_revision: 7
depends_on: [BIFROST-OTEL-T01]
requirements: [REQ-002, REQ-003, REQ-004, REQ-005, REQ-020]
acceptance: [AC-002, AC-006, AC-007, AC-008, AC-011]
---

# Gate projection and signal-neutral Scribe persistence

## Outcome and value

OTLP protobuf/JSON and public canonical Arrow writes reach Scribe as the same
validated user-column `RecordBatch` shape. Gate owns accepted-subset ordering;
Scribe owns managed stamping and the unchanged WAL/fence/ACK boundary. Mixed
OTLP requests durably commit only complete valid siblings and reuse the current
standard partial-success responses.

Required execution skill: `$wyrd-implement`.

## Owners, scope, consumers, and prohibited changes

- The existing server codecs remain the bounded protobuf/JSON decode owners.
- A write-enabled `Gate` owns the same concrete `Arc<BifrostCatalog>` injected
  into Scribe at boot. It resolves the authoritative table schema/layout and
  calls the three built-in projectors or generic native validator before
  Scribe. Query-only `Gate::without_scribe` has neither dependency.
- Gate owns the accepted subset and creates a checked `RowOrdinalRange { start:
  0, len }` in request order.
- `ScribeIngressFrame` carries only validated canonical batches, the move-only
  decode/material reservation owner, trusted principal/tenant/request/batch
  context, and that ordinal range. Scribe stamps the range before WAL encoding.
- Retain `IngestOutcome`, `MetricsOutcome`, `LogsOutcome` and their HTTP/gRPC
  encoders unchanged. Table projection returns `(Option<RecordBatch>, outcome)`;
  do not create a replacement outcome enum.
- Do not alter ACK timing, shard selection, WAL/fence formats, replay identity,
  topology, admission fairness, or public error codes.

## Selected implementation architecture

Replace `IngressPayload::{ProjectedArrow,OtlpTraces,OtlpMetrics,OtlpLogs}` and
the optional frame fingerprint with one invariant-bearing
`ValidatedCanonicalFrame { table, resolved_schema_identity, batches,
row_ordinal_range, correlations, memory_owner }`. `correlations` has exactly one
typed entry per row: nullable `run_id`, nullable Gate-resolved `card_uid`, and
nullable validated `wyrd_event_time` candidate. Retain `ArrowIpc` only at
Gate's public native boundary, decode it under the existing bounded owner, then
convert it to this frame. Remove decoded OTLP types from Scribe.

`resolved_schema_identity` contains the catalog's existing schema fingerprint
for every table and, only for the three canonical built-ins, the table-owned
`CanonicalPhysicalFingerprint`. Scribe re-resolves and compares the same
identity before stamping. Dynamic tables retain current catalog fingerprint
and auto-ID behavior.

For native Arrow, Gate resolves the table through `BifrostCatalog`, obtains the
resolved identity/physical layout, maps canonical built-ins by stable ID/name
and dynamic tables through their current catalog validation, then extracts
`card_ref`, `run_id`, and the optional event-time candidate, authorizes every
distinct `card_ref`, resolves it to `card_uid`, and strips all correlation and
managed columns from the user batch. OTLP supplies the authenticated
principal's resolved card identity and nullable run identity through the same
sidecar. The three canonical table validators and the generic user-table
validator both produce the same required frame, so moving resolution from
Scribe does not create a built-in-only shadow path.

The table owner derives the writable schema from the physical schema. For the
three canonical built-ins, physical user fields retain their ledger IDs,
required-but-nullable writable `run_id` retains ID `1000`, and optional-column
writable `wyrd_event_time` retains ID `1004`. Dynamic descriptions copy the
actual managed-field IDs/metadata from their stored physical schema; they do
not use the canonical fixed IDs. Required non-null `card_ref: Utf8` is ID-less
for every table with metadata
`wyrd:input_class=gate_correlation`. It never enters Iceberg or the physical
fingerprint. Gate accepts reordered input only when stable IDs/names make each
field unambiguous, refuses duplicate or unknown reserved fields, and projects
to ledger order. Scribe replaces `card_ref` with resolved `card_uid`: canonical
built-ins use ID `1001`; dynamic tables use the actual stored physical
`card_uid` field and metadata from `resolved_schema_identity`.

Gate processes each OTLP resource/scope container in wire order. A shared
resource/scope defect contributes one rejection for each affected descendant.
The table projector appends only complete accepted records, so the resulting
batch order is the accepted relative order. Gate constructs the ordinal range
only after projection. Empty input and all-invalid nonempty input return the
current signal outcome immediately with no Scribe call; mixed input calls
Scribe once, awaits its durable ACK, then merges the existing outcome into the
transport response. Request-wide decode, auth, size, schema, admission, and
durability errors remain whole-request errors and never become partial success.

Transfer the existing decode reservation into the canonical batch owner and
resize it with checked Arrow array memory plus exact IPC bytes before Scribe
admission; releasing the frame releases both. Extend the existing
`FixedIpcPlan::count`/`encode` recursive walk to Arrow `List` and `Struct`, field
metadata, and their canonical child buffers. Remove `count_schema` and
`FixedIpcColumnPlan` once no pre-materialized OTLP projector uses them. Keep the
generic exact-capacity encoder because Scribe WAL and tail RPC already share
it; do not replace it with a second growable encoder. The recursive plan must
retain current row/byte/depth caps and exact post-materialization comparison.

`Scribe::decode_rows` becomes a signal-neutral managed-envelope materializer:
it compares the resolved schema identity with its catalog authority, validates the
sidecar cardinality/range and event-time window, stamps resolved `card_uid`,
`run_id`, the trusted ordinal range, and remaining system columns, and proceeds
through the existing prepared-slice path. It never trusts raw `card_ref`.
Multiple Arrow IPC record batches carry one request-wide running ordinal; no
batch resets it.

## Ordered implementation scenarios

### Scenario 1 — Gate projects accepted records and owns one contiguous range

**Behavior.** Mixed trace/log/metric requests preserve valid sibling order,
count every affected invalid descendant, and send one canonical batch with
ordinals `0..accepted`. Empty/all-invalid input performs no Scribe work. Maps
REQ-002, REQ-005, INV-002, INV-008–INV-010, AC-006.

**RED.** Add
`gate::tests::mixed_otlp_projection_assigns_only_accepted_contiguous_ordinals`
and
`gate::tests::all_invalid_otlp_returns_existing_outcome_without_scribe`.
Use the existing fake Scribe dispatch seam to assert the exact canonical rows,
one dispatch only after projection, transferred memory owner, range, outcome,
and zero dispatch/fence for all-invalid input. The current Gate forwards typed
OTLP and cannot pass.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=gate::tests::mixed_otlp_projection_assigns_only_accepted_contiguous_ordinals)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=gate::tests::all_invalid_otlp_returns_existing_outcome_without_scribe)'
```

**GREEN.** Move table projection into the three Gate entry points, build the
single canonical frame/range, and return the current signal outcome only after
Scribe ACK. Preserve current auth-before-decode and route response wiring.

**REFACTOR.** The three entry points may share one private canonical dispatch
method, but no generic mapper trait or fourth outcome type.

### Scenario 2 — Canonical Arrow and OTLP share managed stamping and WAL bytes

**Behavior.** Equivalent accepted user columns enter the same managed-envelope
and prepared-slice code; only trusted managed identities differ. Multi-batch
Arrow ordinals do not reset. Maps REQ-003–REQ-004, INV-003, INV-007–INV-009,
AC-002, and AC-008.

**RED.** Extend the existing Scribe persistence test owner with
`scribe::tests::scribe_persistence_path::canonical_nested_batches_share_one_managed_wal_path`.
Feed equivalent OTLP-projected and public-Arrow-validated maximal batches,
including a two-batch Arrow stream. Assert identical user-column logical
digest, recursively encoded nested values, exact managed values, request-wide
ordinals, one fence, and no signal branch below `Scribe::ingest`.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::tests::scribe_persistence_path::canonical_nested_batches_share_one_managed_wal_path)'
```

**GREEN.** Collapse Scribe ingress/preprocess/execution onto canonical rows,
materialize managed columns from trusted context/range, and extend the existing
exact-capacity IPC walk recursively. Preserve partition splitting and the
prepared-slice/WAL/fence implementation.

**REFACTOR.** Scribe may inspect table physical layout and managed-column
policy only; it cannot import generated OTLP signal types or table field names.

### Scenario 3 — Recovery replays the accepted nested subset exactly

**Behavior.** Timeout, duplicate retry, rotation, and restart reconstruct the
same accepted nested rows and fence digest once. Maps REQ-003, REQ-005,
INV-001, INV-008–INV-009, AC-006–AC-007.

**RED.** Add
`scribe::replay::tests::nested_accepted_subset_replays_one_fence_and_digest`.
Persist a mixed-request accepted subset with nested arrays, replay it twice,
and assert exact row values/order/ordinals, one authoritative batch identity,
one digest, and duplicate suppression. Corrupt one nested IPC child and assert
fail-closed recovery with no partial authority.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::replay::tests::nested_accepted_subset_replays_one_fence_and_digest)'
```

**GREEN.** Adapt the existing replay and logical-data digest paths only where
nested IPC exposes a real assumption. Keep WAL versions and fence identity
unchanged when the existing format already stores generic IPC bytes.

**REFACTOR.** One recursive Arrow/IPC validator is shared by live write and
replay; no signal-aware recovery code.

### Scenario 4 — Superseded Scribe mapping authority is deleted

**Behavior.** No Scribe module accepts decoded OTLP or owns signal columns,
while existing outcome and response behavior remains. Maps REQ-020, INV-010,
AC-011.

**RED.** Add the narrow static unit assertion
`scribe::tests::scribe_boundary_contains_no_otlp_signal_payload` over the
closed `IngressPayload` API and compile-time module surface; it initially fails
while OTLP variants and direct modules exist.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::tests::scribe_boundary_contains_no_otlp_signal_payload)'
```

**GREEN.** Delete `direct_traces.rs`, `direct_logs.rs`, `direct_metrics.rs`,
`otlp_managed.rs`, OTLP-only preprocess types, and
`scribe/test_projection_oracle/` after their retained validation/tests have
moved to T01. Remove module declarations and dead branches. Retain the generic
IPC, admission, prepared-slice, outcome, and durability code.

**REFACTOR.** Do not add a name-ban repository script; Rust type/module removal
and focused behavior tests make the old boundary unreachable.

## Expected write set and consumer closure

- `crates/vala/vala-bifrost-redux/src/{contracts,otlp_contract}.rs`
- `crates/vala/vala-bifrost-redux/src/gate/`
- `crates/vala/vala-bifrost-redux/src/scribe/` including deletion of the
  signal-specific modules and projection oracle
- Existing server OTLP HTTP/gRPC adapters only where the Gate return shape
  requires direct wiring; decoding behavior remains in place

No query DTO, generated SDK artifact, physical-table removal, migration,
dependency, feature, new test target, or new harness belongs here.

## Verification and evidence

Run every named command, then:

```bash
mise run test:bifrost:journey:scribe
mise run fmt
mise run lints
git diff --check
```

Record exact accepted/rejected counts and reason, zero-work all-invalid facts,
the accepted-subset fence/digest, recursive IPC capacity, replay/duplicate
results, and a source inspection showing no decoded OTLP type below Gate.

## Material stop conditions

- The current WAL record cannot carry generic nested Arrow IPC without a format
  change.
- Existing admission cannot transfer/resize the decode owner before another
  input-sized allocation.
- Preserving the public partial-success response requires changing the existing
  outcome or response encoder contract.

Return the first two to planning and the last to `$wyrd-spec`; do not keep a
shadow OTLP Scribe path.

## Authority links

- `AGENTS.md`
- `architecture/agent-rules.md`
- `architecture/wyrd-design.md`
- `architecture/bifrost-design.md`
- `architecture/references/domain/telemetry-observations.md`
- `architecture/references/domain/arrow-analytical-interop.md`
- `architecture/references/domain/analytical-operations-reliability.md`
- `architecture/references/languages/implementation-execution.md`
- `architecture/references/languages/testing-workflows.md`
- `changes/active/bifrost-canonical-otel-signals/spec.md`
