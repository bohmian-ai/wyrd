---
id: TASK-003
spec: SPEC-service-identity-and-telemetry
spec_revision: 12
status: proposed
kind: implementation
depends_on: [TASK-002, TASK-004]
requirements: [REQ-021, REQ-022, REQ-023, REQ-024, REQ-025, REQ-026, REQ-027, REQ-028, REQ-031, REQ-032, REQ-034, REQ-035, REQ-036, REQ-039, REQ-040, REQ-042, REQ-044, REQ-048]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008]
acceptance: [AC-007, AC-008, AC-009, AC-010, AC-012, AC-014, AC-016, AC-018, AC-021]
---

## Outcome and Value

Every Bifrost row keeps account, exact execution workload and observed Card
independently queryable under one public `bifrost` catalog. Complete typed
signals gain useful resource/URL/exception columns. A span about a shared Model
identifies that Model as subject, its executing Service release as workload,
and the stable account as publisher, independently of service labels.

## What Changes and What Must Stay True

- Only `bifrost.*` resolves for built-ins, datasets and operational namespaces,
  through parsing/rendering, defaults, bindings, grants, SQL, SDKs, HTTP/gRPC,
  MCP, CLI and operational output. No alias, migration or destructive reset;
  internal Vala crate/domain names are outside the catalog rename.
- All nine built-ins and dynamic tables share seven managed fields: nullable
  workload/subject Card UID and Run; required account subject/actor, request,
  and microsecond-UTC PostgreSQL admission time. Tenant stays a physical binding.
  Stored subject is `subject_card_uid`; OTLP attribute stays `wyrd.card_uid`.
  Reconcile three explicit owner fields into one `workload_card_uid`.
- Audit uses `wyrd_request_id`, original decision pair/unresolved sentinel and
  truthful nullable correlations. Keep `audit_card_ref` a separate target;
  neither audit nor binding-owned data adopts its delivery worker's identity.
- Implement every supplemental field/type/nullability: spans 62, logs 43,
  metrics 55, all unchanged nested data, precision/presence and typed Variant.
  Add six resource fields, span URL path/query and span/log exception booleans.
  Preserve integer counters, SpanKind, metric kinds, summaries and exemplars.
- Apply the exact nine-table partition/sort/Bloom ledger via TASK-004's engine:
  hourly signals/gateway, daily drift/verification/audit; ordered/null-aware
  sorts, complete per-table Bloom sets, eight result defaults, span/log span_id
  opt-in. No hidden floor or claimed performance improvement.
- Public forged managed/derived values fail before ACK. Internal mixed-origin
  single-tenant batches preserve each origin through forwarding/retry/recovery.
  Preserve atomicity, idempotency, bounded output, Variant validation and tenancy.

TASK-002 supplies validated context/accepted authority; this task owns its row,
schema and consumer closure plus owning architecture amendments. TASK-004 owns
storage mechanics, TASK-005 editing/configuration, TASK-006 trace semantics,
TASK-007 optimization, TASK-008 docsite. Runtime examples/fixtures and SDK
projections change here. No new writer, graph owner, SDK durable behavior,
implicit row policy, cross-store SQL join or evidence/time-window narrowing.

## Approach

1. Cut over shared catalog identities and lookup/grant/query consumers.
2. Extend managed/domain ledgers and apply defaults using the shared engine.
3. Extend projections/native validation and trusted producer origin propagation.
4. Close public projections/architecture and prove exact rows and refusals.

## Ordered Implementation Scenarios

### Scenario 1 — Catalog resolution and grants agree

**Behavior.** Built-ins/datasets describe and query under `bifrost.*` on every
public surface; old names, ungranted and foreign-tenant reads fail without fallback.

**RED.** [Catalog proof](#catalog-proof) adds planned parser/scope and public
catalog tests and extends MCP/CLI regressions; current old catalog literals fail.

**GREEN.** Change existing namespace/lookup/scope owners and every consumer
together, passing exact positive and denial assertions.

**REFACTOR.** Reuse TableRef/namespace rendering rather than duplicate catalog literals.

### Scenario 2 — All tables describe exact schemas and defaults

**Behavior.** Nine freshly provisioned empty built-ins plus a dynamic table
have the complete envelope/domain schema and ledger defaults, without duplicate
root or old stored Card/request fields; default application preserves explicit tenant choices.

**RED.** [Schema proof](#schema-proof) extends exact ledger/layout assertions
and adds all-table describe/write/read; current six-field envelope and audit exception fail.

**GREEN.** Extend managed declarations/domain ledgers, reconcile binding-owned
roots/audit and apply exact defaults through TASK-004. Rerun Scenario 1.

**REFACTOR.** Derive schemas/reservations/describe from existing owners while
preserving sensitivity metadata and registered IDs.

### Scenario 3 — Promotions retain raw types and signal meaning

**Behavior.** OTLP and native canonical batches agree on every supplemental field:

| Input variation | Required result |
| --- | --- |
| Resource/scope absent or present-empty; wrapper URLs | Preserve presence, typed empty values and exact URLs. |
| Duplicate/missing/null/wrong-type ordinary attributes | Final occurrence controls raw/promoted value; wrong type gives SQL null, retaining raw value. No parsing/stringification or envelope crossover. |
| Wrong stable environment type; valid older key | Typed fallback selects older key; both raw attributes remain. |
| Wrong final GenAI type | Reject affected complete span under existing strict rule. |
| Last exact-name exception event lacks earlier strings | Span boolean true, strings only from last event; no event means false. |
| Empty log exception string; severity-only error | Empty string means true; severity alone false. |
| Nested data/all metric kinds/>2^53 counters/nanoseconds | Exact order/type/presence; overflow and invalid shapes refuse rather than coerce. |

**RED.** [Projection proof](#projection-proof) extends maximal tests and adds
shared resource edges; missing columns and false native promotions fail initially.

**GREEN.** Extend existing resource/signal projectors, native validation and
output-byte accounting, independently asserting raw and derived values; rerun Scenarios 1–3.

**REFACTOR.** Reuse final-key, typed-promotion and Variant owners; retain strict GenAI handling.

### Scenario 4 — Public ingress cannot forge attribution or filter values

**Behavior.** HTTP protobuf/JSON, gRPC, SDK preparation and native Arrow agree.
Forged managed/derived values, invalid correlation and graph disagreement preserve
exact OTLP partial-success counts or native whole-batch refusal before ACK;
invalid request context refuses the request. Generic rows may have workload without subject, or neither.

**RED.** [Ingress proof](#ingress-proof) extends transports and adds native/language
cases: valid/refused siblings, shared-envelope defects, no forged rows, changed-owner batch replay.

**GREEN.** Close Gate/Scribe and thin SDK preparation against shared validation;
one bound client may batch subjects/Runs, distinct root/auth contexts cannot
share buffered attribution. Preserve batch identity and rerun Scenarios 1–4.

**REFACTOR.** Remove transport-specific duplicate validation; keep durable decisions server-owned.

### Scenario 5 — Trusted delivery preserves the original action

**Behavior.** Forwarding, mixed-origin single-tenant batching, restart and retry
retain these origins independently of delivery credentials:

| Producer | Retained identity |
| --- | --- |
| Gateway/internal capture | Accepted originating call account pair/context. |
| Direct/queued/group inputs/results/drift features | Accepted authority, validated binding owner/subject, Verifier/execution/group independently. |
| Audit verified/unresolved | Original decision pair/sentinel/request, truthful nullable correlations, separate target; failed delivery does not fail the operation. |

**RED.** [Origin proof](#origin-proof) adds a producer matrix and extends audit
identity regression; distinct accounts and roots sharing a component expose worker restamping.

**GREEN.** Extend existing origin/build/outbox/stamp owners preserving accepted
provenance and binding content; refuse mixed tenants and rerun Scenarios 1–5.

**REFACTOR.** Consolidate stamping without another audit sink/writer/public trusted path.

### Scenario 6 — Lifecycle reads and evidence consumers remain exact

**Behavior.** Hot/published/recovered/compacted rows match completely. Cards-resolved
Service-family/exact-release workload filters stay independent of component,
account, Run/request and resource filters. Result/input/drift/group/trace readers
use canonical columns/grants without changing evidence meaning.

**RED.** [Read proof](#read-proof) compares full nested values, two roots sharing
a subject and dynamic correlation, extending queued-peer/native-SDK regressions;
stale names or attribution loss fail.

**GREEN.** Close recovery/publication/compaction, specialized SQL and result
decoding against the ledgers and existing table permissions. Rerun all scenarios;
trace semantic changes and measurements remain TASK-006/007.

**REFACTOR.** Reuse result/query owners and lifecycle controls; add no implicit
read bounds, evidence filters or registry SQL join.

## Acceptance Criteria

| Obligations | Proof and violation |
| --- | --- |
| REQ-021–024; AC-007; INV-008 | Projection/ingress covers every field/rejection; wrong promotion or lost precision fails. |
| REQ-022,028,034; AC-012; INV-001–005,007 | Schema/origin covers nine plus dynamic; duplicates, worker restamping, invented correlations or tenant crossover fail. |
| REQ-025,035–036,042,044,048; AC-016,018,021 | Exact ledger/describe/defaults; hidden floor or altered partition/sort fails. Engine/config mechanisms belong to TASK-004/005. |
| REQ-026,039–040; AC-008,014; INV-003–004 | Catalog/read/peer regression; conflated identity or stale grants fail. TASK-007 measures pruning/performance. |
| REQ-027,031–032; AC-009–010; INV-006 | Fresh SDK/MCP/CLI/generated proof, runtime example and architecture diff; alias or SDK durable owner fails. Docsite belongs to TASK-008. |

## Implementation Reference

Inspected reuse evidence; paths guide discovery, not private code structure:

| Owner and callers/tests | Gap and selected extension |
| --- | --- |
| Redux `BifrostNamespace`, `TableRef::{fqn,parse_fqn,permission_scope}`; verification SQL, SDK query tests | Old names/scope fallback: change shared identity and all default, physical-binding, permission and operational consumers. |
| `tables/managed_columns.rs::MANAGED_COLUMNS`, `CorrelationPolicy::{appends,reserves,stamps}`, `DomainTable`; schema/layout tests | Six fields/audit exception: extend one envelope/reservation owner and nine ledgers, reconcile three root fields and audit request/pair. |
| Signal ledgers and `signal.rs::{ResourceEnvelope,RecordCorrelation,promoted_string,promoted_int,last_attribute}`, `project_resource_*`, inline maximal tests | Existing typed Variant/final-key/presence/metric logic fits: extend columns, extraction, validation and accounting without another decoder. |
| Gate native/signal admission, `attribution::native_card_uids`, `gate/auth.rs`; Scribe `prepare_admitted_rows`/`preprocess` | Old subject names/insufficient context: extend current authorization, canonical-value and idempotent admission seams before ACK. |
| Server `scribe_outbox.rs` encode/attributed/`ScribeTable`, `audit/mod.rs`, `ResultPayloadBuilder`, gateway capture | Audit special-case/old owners: extend originating-context builders/stamping, preserve binding content and nonblocking audit. |
| Verification drift/group/hosted trace readers and result service | Rename SQL/payload/read-grant consumers preserving frozen authority/predicates; TASK-006 changes trace semantics later. |
| Client `bifrost::{handle,facade,table,observe,query}`, queue/preparation; spec vala managed/API; SDK bindings/exports/stubs | Existing single Bifrost client exposes old names: close all wire/native/public declarations and regenerate through owners. |
| Testing `OtlpJourney`, server verification/audit; client pg_bifrost_e2e; Python/TS OTel; MCP/CLI query modules | Existing real-server/Postgres targets support focused tests; pure extraction stays inline, with no new test binary/fixture owner. |

Close HTTP/gRPC codecs, CLI/MCP help/runtime schemas, SDK types, fixtures,
runtime examples/embedded SQL and operational output. Amend owning
`architecture/bifrost-design.md` row/signal/catalog/default sections, coordinating
shared edits with TASK-002/004/005. Final docsite embeds proved sources in TASK-008.

## Verification and Evidence

Current manifests/module declarations, TESTING.md and mise tasks establish these
packages, targets, runners and Postgres/setup wrappers. **Planned** tests below
must have their actual selector confirmed after addition and before RED; zero
selected tests is not proof. Existing tests are extended, not replaced. Run
Cargo commands sequentially. Record RED, actual selected counts, green results,
full value/origin comparisons and described/layout declarations.

### Catalog proof

Planned redux parser/scope and server public catalog tests; existing MCP/CLI query regressions.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=catalog::table_ref::tests::canonical_catalog_has_no_alias)'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise run build:server && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=query::canonical_catalog_queries_and_grants_agree)'"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(=query::pg_tests::agent_reads_canonical_trace_genai_logs_and_metrics_through_sql)'"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-cli --test cli -P journey --run-ignored=all -E 'test(=query_server_journey::query_command_reads_seeded_table) | test(=query_server_journey::query_command_denial_preserves_problem_without_read_decision)'"
```

### Schema proof

Existing redux exact-schema/layout tests; planned public nine-system-plus-dynamic describe/write/read.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=tables::tests::verification_tables_match_their_approved_schemas) | test(=tables::tests::physical_layout_contract_all_builtins) | test(=tables::tests::canonical_signal_schemas_round_trip_arrow_parquet_and_iceberg) | test(=tables::managed_columns::tests::observation_policy_appends_every_managed_column_in_order)'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise run build:server && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=verification_runtime::system_and_dynamic_tables_share_attribution)'"
```

### Projection proof

Existing maximal signal projections plus planned shared-resource edge test in redux lib.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=tables::traces::tests::maximal_span_projection_is_lossless_and_identity_mapped) | test(=tables::logs::tests::maximal_log_projection_preserves_body_context_and_presence) | test(=tables::metrics::tests::all_pinned_metric_point_kinds_project_without_narrowing) | test(=tables::tests::resource_promotions_preserve_final_typed_values)'
```

### Ingress proof

Existing OTLP full/partial tests extended across protobuf/JSON/gRPC; planned native refusal and Python/TS public stock-exporter-to-Bifrost journeys covering all three signals, promotions and origins.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test otlp -P journey --run-ignored=all -E 'test(=trace_export::pg_tests::otlp_grpc_maximal_trace_round_trips_every_field) | test(=logs_export::pg_tests::otlp_log_routes_round_trip_body_context_and_redaction) | test(=metrics_export::pg_tests::otlp_metric_routes_round_trip_every_supported_point_kind) | test(=negative::pg_tests::mixed_otlp_requests_commit_only_complete_siblings_and_exact_partial_success)'"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise run build:server && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=verification_runtime::native_signal_values_and_attribution_are_validated_before_ack)'"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run py:setup && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/otel/test_otel_signals.py::test_signal_promotions_and_attribution_read_back_through_bifrost'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run ts:build && mise run ts:build:testing && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/otel-signals.test.ts -t "^signal promotions and attribution read back through bifrost$"'
```

### Origin proof

Planned producer matrix plus existing audit original-decision regression, using current server/outbox/peer controls.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise run build:server && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=verification_runtime::trusted_delivery_preserves_each_origin) | test(=audit_retention::subject_actor_decisions_preserve_trusted_identity)'"
```

### Read proof

Planned full-value lifecycle/correlation comparison plus existing queued-peer and Rust SDK native-table regressions.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise run build:server && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=verification_runtime::canonical_signal_rows_survive_publication_recovery_and_compaction) | test(=verification_runtime::runner_without_local_scribe_publishes_through_a_peer_scribe)'"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-client --test integration -P journey --run-ignored=all -E 'test(=pg_bifrost_e2e::pg_tests::unified_client_registers_writes_swaps_and_reads_both_tables)'"
```

### Static and contract closure

Prose/generated/mechanical changes need static proof, not manufactured RED.
Record architecture/runtime-example diffs, generation sources/outputs and
Python stub/export parity. Inspect executable old-catalog/old-column references;
only deliberate rejection tests and historical change records may retain them.

```bash
mise run fmt
mise run lints
mise run codegen:check
mise run check:deps
mise run check:tenant-isolation
mise run py:format
mise run py:lints
mise run py:typecheck
mise run ts:format
mise run ts:lints
mise run ts:typecheck
mise run ts:napi:check
```

Engine/revision/configuration proof belongs to TASK-004/005; trace semantics
and measurements belong to TASK-006/007. Full capability/language suites run
once on the integrated candidate; focused shipped-surface proofs above run here.

## Material Stop Conditions

Stop if correctness requires a second catalog/alias, historical conversion,
SDK-owned durable validation, weakening tenant/managed-field boundaries,
inventing ownership from labels or delivery credentials, changing accepted
batch identity, or changing an approved field/type/default. Route a conflicting
storage API through TASK-004/005 coordination rather than add a parallel owner.
Do not change trace evidence scope/retry behavior or add implicit read bounds
while fixing renamed consumers. An unavailable test environment is verification
work to resolve using repository tooling, not permission to weaken proof.

## Authority Links

- [Approved specification revision 12](../spec.md) and its normative
  [telemetry](../supplements/telemetry-schemas.md),
  [storage/correlation](../supplements/table-storage-and-correlation.md), and
  [authentication](../supplements/auth-system.md) supplements.
- [Bifrost authority](../../../../architecture/bifrost-design.md#table-and-row-identity),
  [Wyrd design](../../../../architecture/wyrd-design.md), and
  [security posture](../../../../architecture/wyrd-security-posture.md).
- [Spec-driven development](../../../spec-driven-development.md),
  [implementation execution](../../../implementation-execution.md),
  [testing](../../../../TESTING.md), [agent guides](../../../../agent_docs/index.md),
  and the Python/TypeScript SDK directory rules.
