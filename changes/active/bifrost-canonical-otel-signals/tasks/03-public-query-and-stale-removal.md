---
id: BIFROST-OTEL-T03
title: Serve complete trace and GenAI queries from canonical spans and remove stale tables
kind: implementation
mode: DECOMPOSE
status: proposed
spec: SPEC-bifrost-canonical-otel-signals
spec_revision: 7
depends_on: [BIFROST-OTEL-T01, BIFROST-OTEL-T02]
requirements: [REQ-001, REQ-004, REQ-009, REQ-013, REQ-014, REQ-015, REQ-016, REQ-017, REQ-018, REQ-019, REQ-021]
acceptance: [AC-001, AC-002, AC-003, AC-004, AC-005, AC-008, AC-011]
---

# Canonical typed reads and pre-release stale-surface deletion

## Outcome and value

Trace detail returns real nested content from `vala.traces.spans`, and
`QueryGenAi` searches generation spans through promoted columns while reading
structured messages only for authorized callers. The seven unwritten
trace-child/GenAI physical tables and every public/internal assumption about
them disappear without aliases or migration.

Required execution skill: `$wyrd-implement`.

## Owners, scope, consumers, and prohibited changes

- `wyrd-spec::vala::api` owns public Rust/HTTP response types; it stays IO- and
  PyO3-free.
- `wyrd-server::vala_query` owns authorization-aware DataFusion plans and
  Arrow-to-public projection. `vala-sdk`, tonic, Python, TypeScript, MCP,
  OpenAPI, schemas, descriptors, and stubs project that same contract.
- The table registry/catalog is the only physical table inventory.
- Do not add query aliases, compatibility readers, dual-table scans, cost
  inference, derived metrics, payload caches, or a migration/backfill.

## Selected public contract and query design

Extend the existing describe contract rather than creating a canonical-schema
endpoint. Change `DataTypeSpec::List` to hold `Box<FieldSpec>` so
the list child retains name, nullability, metadata, and field ID; add the stable
field ID to `FieldSpec.metadata` using the existing Arrow
`PARQUET:field_id` key. Preserve all metadata recursively in
`wyrd-server::bifrost::convert`, `vala-bifrost-redux::catalog::wire`, and the
existing `wyrd_queue::{fieldspec_to_arrow,arrow_schema_to_fieldspec}` pair.
Replace `BifrostTableDescription.fields` with `user_fields`,
`correlation_fields`, and `managed_candidates`. `correlation_fields` is exactly
required ID-less non-null `card_ref: Utf8` tagged
`wyrd:input_class=gate_correlation` plus required nullable `run_id: Utf8`.
`managed_candidates` contains the optional-column, non-null
`wyrd_event_time: Timestamp(Microsecond, UTC)`. For the three canonical
built-ins these fields carry IDs 1000 and 1004; dynamic descriptions copy their
actual IDs/metadata from the stored physical schema.
`entry.fingerprint` retains its existing catalog/user-schema meaning. The three
canonical descriptions additionally expose:

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub canonical_physical_fingerprint: Option<String>
```

The value is exactly 64 lowercase hexadecimal characters from T01's digest for
canonical built-ins and `None`/omitted JSON for dynamic tables.

Add `QueryClient::describe_table(namespace: &str, name: &str) ->
Result<BifrostTableDescription, ValaSdkError>` over GET
`/v1/bifrost/tables/{namespace}/{name}`. In `wyrd-queue`, extend the current
recursive converter and add `writable_schema(description,
include_event_time) -> Result<Schema, WyrdQueueError>` and
`BatchBuilder::from_description(description)`. The latter consumes only
`user_fields` and the declared correlation fields, appends correlation exactly
once, and omits event time because its JSON row API has no timestamp argument.
Direct Arrow callers use `writable_schema(..., true)` when supplying it.

PyO3 adds `_NativeBifrostQueryClient.describe_table(namespace, name)` through
the existing JSON-to-Python projection and
`writable_schema_ipc(description, include_event_time)` returning schema-only
IPC produced by Rust's `writable_schema`. Public Python exposes async
`describe_table` and `writable_schema(...) -> pyarrow.Schema`, decoding that
bounded IPC with installed PyArrow. Batches built on it use the existing write
path.

The same native class retains a cloned `WyrdClient` and adds
`insert_batch(table, batch_id_bytes, arrow_ipc)`, validating a 16-byte UUIDv7
and delegating to the existing `BifrostGrpcTransport::insert_batch` exactly as
the N-API owner does. Public Python adds async
`insert_batch(table: str, batch_id: uuid.UUID, batch: pyarrow.RecordBatch) ->
uuid.UUID`; it writes one schema-plus-batch IPC stream with PyArrow, calls the
native method through `asyncio.to_thread`, and requires the echoed durable ACK
identity to match. It does not enter the buffered JSON `Bifrost.insert` path or
rebuild the schema.

N-API adds `NativeBifrostQueryClient.describeTable(namespace, name)` as its
existing structured result and `writableSchemaIpc(description,
includeEventTime) -> Buffer`, backed by the same Rust description/converter.
TypeScript `BifrostClient.describeTable` and `.writableSchema` decode the
schema-only IPC with installed `apache-arrow`; resulting batches feed existing
`insertBatch`. Generated description types come from `wyrd-spec`. No client
imports table code or implements another field/type conversion switch.

Make `SpanEventRow` include `time_unix_nano`, name, decoded JSON attributes when
authorized, and dropped count. Make `SpanLinkRow` include IDs, trace state, raw
flags, authorized attributes, and dropped count. Expand `SpanRow` with trace
state, raw flags/kind, exact start/end/duration nanos, status code/message,
dropped counts, resource/scope fields, and `events`/`links` nested on their
owning span. Remove top-level `TraceWaterfall.events` and `.links`; the
pre-release stub shape has no compatibility requirement. Payload fields are
`Option` and omitted without `BifrostTracePayload:Read`; metadata and counts
remain visible under the existing trace read permission.

Replace `GetTraceRequest.window` with `trace_id` plus optional `since` and
`until`; remove `limit` and `page_token` from this complete-detail contract and
its protobuf projection. Validate `since <= until`. HTTP carries the bounds as
query parameters and gRPC carries the same fields. Trace detail returns one
complete authorized cut and has no continuation token.

Change `GenAiRow` to nullable promoted conversation/model/provider, exact start
nanos, token counts, and optional `input_messages`/`output_messages:
serde_json::Value`. Remove `prompt`, `completion`, and `cost_usd` everywhere.
Decode only canonical `AnyValue` array/object/scalar/null values under
`gen_ai.input.messages` and `gen_ai.output.messages`; reject a stored canonical
value that cannot satisfy the public JSON contract rather than stringifying it.

`build_query_genai_plan` reads `vala.traces.spans`, filters
`gen_ai_operation_name IN ('chat','generate_content','text_completion')`, and
applies conversation/model/provider predicates to the exact promoted columns.
Its base projection contains only metadata/promotions. Add the canonical
attribute payload only when `BifrostGenAiPayload:Read` is present, so an
unauthorized plan never scans it. Trace detail similarly selects sensitive
attributes/events/links/resource/scope payload columns only after
`BifrostTracePayload:Read`. Logs retain their existing body/attribute
projection gate, expanded to any new sensitive resource/scope columns.

Delete `tables/traces/{events,links}.rs`, `tables/genai/`, their seven registry
entries, documentation, generated projections, fixtures, query strings,
maintenance expectations, and tests. Recreate dev/test built-ins from the new
registry; add no SQL migration because revision 7 declares them unshipped.

Add concrete first-class reads on the existing `vala_sdk::QueryClient`:
`get_trace(&GetTraceRequest) -> Result<GetTraceResponse, ValaSdkError>` sends
GET to the existing `/v1/traces/{trace_id}` route with `since`/`until` query
parameters, and
`query_genai(&QueryGenAiRequest) -> Result<QueryGenAiResponse, ValaSdkError>`
posts to the existing `/v1/genai/query` route through its shared
authenticated `WyrdClient`, reusing `request_json` and current problem/error
conversion. The Python `BifrostQueryClient` exposes async `get_trace` and
`query_genai` through `PyBifrostQueryClient` and the shared Wyrd runtime bridge.
TypeScript `BifrostQueryClient.getTrace` and `.queryGenAi` use its existing
authenticated native/HTTP owner and generated DTOs. gRPC remains the server
contract exercised directly by Rust journeys; SDKs do not gain a second
transport selector. Server permission and GenAI pagination behavior remains
authoritative; clients pass requests and project responses only.

## Ordered implementation scenarios

### Scenario 1 — Public DTOs express complete nested traces and structured GenAI

**Behavior.** Rust/JSON/schema/protobuf contracts expose complete canonical
trace detail and ordered structured GenAI messages with no obsolete fields.
Maps REQ-009, REQ-017, INV-002, INV-004–INV-006, AC-001, AC-005, AC-008.

**RED.** Replace the existing Vala API round-trip fixtures with
`vala::api::tests::canonical_trace_and_genai_rows_round_trip_without_stubs`.
Assert exact nanos/raw values, nested order and payload omission, structured
JSON arrays/objects/scalars/null, and absence of top-level children,
prompt/completion/cost in serialized JSON and generated schemas.

```bash
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=vala::api::tests::canonical_trace_and_genai_rows_round_trip_without_stubs)'
```

**GREEN.** Update source DTOs, tonic proto/conversions, vala-sdk conversions,
and generated client projections. Regenerate artifacts from owners only.

**REFACTOR.** Keep one public DTO definition per language projection; no manual
parallel schema or handwritten generated file.

### Scenario 2 — Query plans filter promotions and gate payload before IO

**Behavior.** GenAI generation search uses only exact approved operation names
and promoted predicates; unauthorized trace/GenAI/log queries never project
sensitive payload columns. Maps REQ-009, REQ-013–REQ-018, INV-001,
INV-005–INV-007, AC-003, AC-005.

**RED.** Extend the existing `pg_router_smoke` integration target with
`canonical_trace_and_genai_queries_filter_promotions_before_payload_projection`.
Register the new built-ins, insert representative generation and excluded
embedding/agent/tool spans, then inspect/execute authorized and unauthorized
plans. Assert exact three-operation membership, promoted predicates, complete
nested trace output, structured messages only when authorized, and no
attributes/events/links/messages column in unauthorized physical projection.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test pg_router_smoke -E 'test(=canonical_trace_and_genai_queries_filter_promotions_before_payload_projection)' --test-threads=1"
```

**GREEN.** Change the server plan builders and row extractors to the selected
canonical columns. Decode canonical protobuf values only after authorization
and bounded Oracle projection. Update HTTP and gRPC handlers without changing
tenant, audit, page-token, terminal, or query admission owners.

**REFACTOR.** Classification and predicates never decode attributes; payload
decoding is a response-edge conversion, not a DataFusion UDF or service.

### Scenario 3 — Describe and first-class clients project one recursive schema

**Behavior.** Describe returns exact nested field identity/metadata and the
three client languages build the same Arrow schema and call typed trace/GenAI
reads without local durable logic. Maps REQ-001, REQ-004, REQ-009, REQ-017,
INV-003, AC-002, AC-005, AC-008, and AC-011.

**RED.** Add
`bifrost::convert::tests::nested_field_description_preserves_identity_and_metadata`,
`schema::schema_tests::fieldspec_to_arrow_preserves_recursive_field_contract`,
`query::tests::describe_builds_writable_schema_without_duplicate_correlation`,
`query::tests::typed_trace_and_genai_methods_project_http_contracts`,
Python `test_typed_trace_and_genai_client_contracts` and
`test_canonical_arrow_insert_uses_described_schema`, and TypeScript `typed
trace and GenAI client contracts`. Assert recursive child name/nullability/ID,
the catalog fingerprint, exact canonical 64-character physical fingerprint and
dynamic omission, three describe field classes with canonical and stored
dynamic IDs, exact method DTOs,
absence of trace pagination, GenAI pagination, structured messages, schema-only
IPC, one set of correlation fields, compatibility with existing insert, and
existing error conversion.

Add the integrated Rust proof
`pg_tests::described_canonical_and_dynamic_schemas_reach_exact_physical_schema`
to the existing `pg_bifrost_e2e` target. For one canonical built-in and one
registered dynamic table, describe, build through
`BatchBuilder::from_description`, send via the existing public insert, and
assert Gate/Scribe stamp each table's actual physical IDs/fingerprint and
resolved correlation values without duplicates.

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=bifrost::convert::tests::nested_field_description_preserves_identity_and_metadata)'
mise exec -- cargo nextest run --locked -p wyrd-queue --lib -E 'test(=schema::schema_tests::fieldspec_to_arrow_preserves_recursive_field_contract)'
mise exec -- cargo nextest run --locked -p vala-sdk --lib -E 'test(=query::tests::describe_builds_writable_schema_without_duplicate_correlation)'
mise exec -- cargo nextest run --locked -p vala-sdk --lib -E 'test(=query::tests::typed_trace_and_genai_methods_project_http_contracts)'
mise exec -- bash -lc "cd python/py-wyrd && mise run py:setup:testing && uv run pytest -q tests/bifrost/test_query.py::test_typed_trace_and_genai_client_contracts"
mise exec -- bash -lc "cd python/py-wyrd && mise run py:setup:testing && uv run pytest -q tests/bifrost/test_query.py::test_canonical_arrow_insert_uses_described_schema"
mise exec -- bash -lc "cd typescript/wyrd && mise run ts:build && mise run ts:build:testing && pnpm exec vitest run tests/unit/bifrost-query.test.ts -t 'typed trace and GenAI client contracts'"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p vala-sdk --test pg_bifrost_e2e -P journey -E 'test(=pg_tests::described_canonical_and_dynamic_schemas_reach_exact_physical_schema)' --run-ignored=all"
```

**GREEN.** Evolve `DataTypeSpec`/`FieldSpec`, existing conversions, describe
response, `BatchBuilder`, PyO3/N-API projections, and the named client methods
exactly as selected. Regenerate all source-owned artifacts.

**REFACTOR.** One recursive wire field and the existing converters serve
registration, description, and Arrow construction; no canonical-signal SDK
schema builder.

### Scenario 4 — Removed physical names are unreachable

**Behavior.** Only three OTel signal tables remain; stale physical names cannot
be resolved, queried, maintained, or generated. Maps REQ-019, REQ-021,
INV-001, INV-010, AC-005, AC-011.

**RED.** Replace current registry count/GenAI assertions with
`tables::tests::canonical_otel_registry_has_no_child_or_genai_tables`. Assert
the three canonical names resolve and every removed name returns the existing
unknown-table failure. Assert catalog bootstrap creates none of the removed
names. The current 14-table registry fails.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=tables::tests::canonical_otel_registry_has_no_child_or_genai_tables)'
```

**GREEN.** Delete the definitions and all direct consumers/goldens/docs, then
regenerate artifacts. Remove exclusive configuration/control state only where
it actually exists; do not add a tombstone or ban script.

**REFACTOR.** Absence is enforced by the closed registry and generated-source
checks, not a permanent grep gate for old words in historical specs.

## Expected write set and consumer closure

- `crates/wyrd-spec/src/vala/api.rs` and source-owned generated schemas/OpenAPI
- `crates/wyrd/wyrd-tonic/proto/wyrd.v1.proto` and generated descriptor owners
- `crates/wyrd/wyrd-server/src/vala_query/{service,routes,grpc}.rs`
- `crates/vala/vala-sdk/` public Rust/Python conversion owners
- `python/py-wyrd/` exports/tests/stub generator inputs; generated stubs only by
  regeneration
- `typescript/wyrd/` N-API/public types/tests and generated outputs
- `crates/wyrd/wyrd-mcp/` typed query projections and existing tests
- `crates/vala/vala-bifrost-redux/src/tables/` registry and deleted stale files
- Current docs/goldens that name the removed tables or obsolete DTO fields

No migration, compatibility surface, new dependency/feature, new query engine,
or new test target belongs here.

## Verification and evidence

Run every named command, then:

```bash
mise run codegen:check
mise run check:client-tier
mise run check:pyo3-scope
mise run py:format
mise run py:lints
mise run py:typecheck
mise run ts:napi:check
mise run ts:typecheck
mise run docs:check
mise run fmt
mise run lints
git diff --check
```

Record DTO/schema/proto/stub regeneration, authorized versus unauthorized
physical projections, exact GenAI membership, structured payload round-trip,
and closed-registry refusal for all seven removed names.

## Material stop conditions

- An existing public consumer outside the repository is shown to require the
  pre-release stub or removed table names.
- The current permission model cannot distinguish metadata planning from
  sensitive payload projection before IO.
- Generated TypeScript/Python projections cannot express the structured JSON
  and nested trace contract without a new durable contract.

These require spec/architecture resolution; do not add compatibility aliases
or read sensitive columns and redact afterward.

## Authority links

- `AGENTS.md`
- `architecture/agent-rules.md`
- `architecture/wyrd-design.md`
- `architecture/wyrd-doctrine.mdx`
- `architecture/bifrost-design.md`
- `architecture/wyrd-security-posture.md`
- `architecture/references/domain/telemetry-observations.md`
- `architecture/references/domain/datafusion.md`
- `architecture/references/domain/arrow-analytical-interop.md`
- `architecture/references/languages/agent-harness.md`
- `architecture/references/languages/python-api-and-stubs.md`
- `architecture/references/languages/typescript-guide.md`
- `changes/active/bifrost-canonical-otel-signals/spec.md`

## Execution evidence

### Scenario 1 — Public DTOs express complete nested traces and structured GenAI

- **RED.** `wyrd-spec` `vala::api::tests::trace_and_genai_dtos_carry_nested_and_structured_content`
  failed on the flat `SpanRow`/`GenAiRow` shape: no nested `events`/`links` on a
  span, no structured message parts, and the always-absent `cost_usd` still
  declared.
- **GREEN.** `GetTraceResponse` now nests `SpanEventRow`/`SpanLinkRow` under
  their owning span; `GenAiRow` carries structured `input_messages`/
  `output_messages` and drops `cost_usd`. Commit `05f49af3c`.

### Scenario 2 — Query plans filter promotions and gate payload before IO

- **RED.** `wyrd-server` `tests/pg_router_smoke.rs` failed because the GenAI plan
  scanned canonical `attributes` and the bounded caller's plan still projected
  sensitive columns.
- **GREEN.** Both plans are built from the promoted `gen_ai_*` columns and the
  payload gate is applied to the projection list before any scan is issued;
  `resource_entity_refs` is projected by no plan. Commit `464af919b`.
- **Bounded correction.** Byte-layout normalization and the `struct_child`
  projection helper are shared by the trace and GenAI plans rather than
  duplicated per query; `run_id` is synthesized as NULL for a table whose
  correlation policy appends none, so one projection shape serves both.

### Scenario 3 — Describe and first-class clients project one recursive schema

- **RED.** `wyrd-server` `bifrost::convert::tests::nested_field_description_preserves_identity_and_metadata`
  failed: the described field DTO was flat and dropped nested children,
  `PARQUET:field_id`, and `wyrd:sensitive`.
- **GREEN.** One recursive `BifrostFieldSpec` carries children plus identity
  metadata across HTTP, the Python `wyrd.bifrost` wrapper, and the TypeScript
  `BifrostClient`. `wyrd_queue::schema::writable_schema` is the single owner of
  the described-to-writable Arrow projection, and every SDK decodes the same
  schema-only IPC instead of rebuilding fields locally. Commits `22dea17dd`,
  `8fe6b0d15`, `6cb6207ea`, `fd27e17d5`, `1c27f7a52`.
- **Material finding — canonical describe must not use the Iceberg round trip.**
  The journey showed the catalog renumbers top-level field ids sequentially at
  table creation (declared `dropped_events_count` 22 stored as 17, `links`
  23→18, `run_id` 1000→38, `wyrd_event_time` 1004→42), while
  `scribe::execution_lanes::enforce_canonical_physical_identity` enforces the
  *ledger* ids on every stamped canonical batch. Describing the stored ids would
  hand a writer a schema its own batches fail against, so
  `BifrostCatalog::describe_table` describes a canonical built-in from
  `(definition.schema)()` and keeps the Iceberg round trip for dynamic tables
  only — matching this task's own statement that canonical correlation fields
  carry ids 1000/1004 while dynamic descriptions copy their stored ids.
- **Reuse.** The journey compares physical types through the existing
  `vala_bifrost_redux::tables::arrow_type_shape_matches`, which already owns
  Binary/LargeBinary and Utf8/LargeUtf8 round-trip equivalence, rather than
  adding a second comparison rule.
- **Limitation.** The two `vala-sdk` unit tests
  (`query::tests::describe_builds_writable_schema_without_duplicate_correlation`
  and `query::tests::typed_trace_and_genai_methods_project_http_contracts`) were
  authored alongside their methods, so a separate RED run was not observed for
  them; the journey test above supplied the failing-first proof for the surface.

### Scenario 4 — Removed physical names are unreachable

- **RED.** `vala-bifrost-redux`
  `tables::tests::canonical_otel_registry_has_no_child_or_genai_tables` failed
  against the 14-table registry.
- **GREEN.** Deleted `tables/traces/events.rs`, `tables/traces/links.rs`, and the
  whole `tables/genai/` module, their registry entries, the `GenAi` namespace
  variant, and the `vala.genai` Forge admission control state (the two
  `namespace_name` CHECK lists and `ForgeTaskTableIdentity::NAMESPACES`).
  Documentation naming the removed tables was rewritten: `docs/.../schema.svx`
  now documents the canonical span ledger including nested events, links, and
  the promoted `gen_ai_*` columns; `docs/.../architecture.svx` and
  `architecture/bifrost-design.md` drop their rows and namespace entries. No
  tombstone, alias, or ban script was added — the closed registry is the
  enforcement, and `builtin_fqns()` is exactly the bootstrap input, so asserting
  it also asserts that bootstrap creates none of the removed names.
- **Bounded correction.** The task text says "seven registry entries" and
  "registry 14 → 7", but REQ-019 enumerates six physical names
  (`vala.traces.events`, `vala.traces.links`, `vala.genai.{messages,embeddings,
  tool_calls,memory}`). The enumerated names are authoritative: the registry is
  now eight built-ins.
- **Consumer closure.** `crates/wyrd/wyrd-mcp/` names no trace or GenAI query
  surface, so it required no change despite appearing in the write set.
  No SQL data migration was added (REQ-021, pre-release).

### Cross-task consumer repairs found by verification

Splitting the describe response broke three consumers that the scenario tests
did not reach; each was fixed at its owner, not worked around:

- `wyrd-server::bifrost::service::assert_registered_layout_matches` rebuilt a
  schema from the describe response to canonicalize a re-registered layout.
  Describe reports only the managed column a writer may supply, so the rebuilt
  schema lost the server-stamped columns that decide the managed Bloom floor
  and an identical re-registration became `PhysicalLayoutMismatch`. The handler
  now resolves against `BifrostCatalog::assignment_schema`, the provider's own
  schema, which is where that floor is actually determined
  (`bifrost::service::pg_tests::bifrost_tables_register_is_idempotent`).
- The MCP journeys (`wyrd-mcp` discovery and query, `wyrd-testing` oracle mcp)
  read `described["fields"]`; they now read the class that owns each column.
  The discovery journey's `wyrd:column_class == "correlation"` assertion was
  replaced by membership in `correlation_fields` plus non-empty identity
  metadata: splitting the response made the per-field marker redundant, and the
  server no longer emits it.

### Verification

Passing: `codegen:check`, `check:client-tier`, `check:pyo3-scope`, `fmt`,
`lints`, `py:format`, `py:lints`, `py:typecheck`, `py:test:unit` (441 passed),
`ts:napi:check`, `ts:typecheck`, `ts:test:unit` (14 passed), `docs:check`,
`git diff --check`.

`mise run test:bifrost`: 8/10 lanes passed. `unit:rust` and `journey` failed
only on the pre-existing items below; every lane-member command those two lanes
abort before reaching was then run directly and passed —
`wyrd-server --lib --features test-support` (136 tests), `vala-sdk --lib`,
`wyrd-mcp --lib`, and the `mcp` and `oracle` journey lanes after the consumer
repairs above.

Focused: `tables::tests::canonical_otel_registry_has_no_child_or_genai_tables`
PASS via
`mise exec -- cargo nextest run --offline -p vala-bifrost-redux --all-features --lib -E 'test(=tables::tests::canonical_otel_registry_has_no_child_or_genai_tables)'`.
The task's command omits `--all-features`; `vala-bifrost-redux` does not compile
its test targets without them, so the feature flag is the recorded correction.

Pre-existing baseline failures, unrelated to this task and reproduced at the
pre-task commit `8c107b344`:

- `wyrd-spec query::tests::display_parse_roundtrip_for_small_queries` (proptest).
- `wyrd-queue producer::tests::flush_timeout_retains_batch_until_later_ack`.
- `mise run check:unwrap-audit` fails on 77 lines across files this task does
  not touch (no overlap with `git diff --name-only 8c107b344..HEAD`).
- Six `wyrd-testing` lib tests — `bifrost::scribe_workload::tests::scribe_workload_read_boundaries_may_not_reuse_an_earlier_read`
  and the five `bifrost::forge_harness::worker_lifecycle_tests::*` cases, which
  panic on a fixture built with no Scribe. All six fail identically in a
  worktree at `8c107b344`. This is what aborts the `unit:rust` lane before its
  remaining packages.
- The `otlp` journey lane fails with `no tests to run`: every file under
  `crates/wyrd/wyrd-testing/tests/bifrost/otlp/` is a 0-byte archived target at
  `8c107b344` as well.
- `wyrd-mcp::mcp query::pg_tests::delegated_agent_query_is_authorized_as_the_effective_principal`
  fails with `RowNotFound` when the seven-test MCP journey lane runs in
  parallel and passes when run alone — the audit relay race. It reproduces
  identically in the baseline worktree at `8c107b344`.
