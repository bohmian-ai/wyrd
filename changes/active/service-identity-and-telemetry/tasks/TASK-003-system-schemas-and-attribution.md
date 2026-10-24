---
id: TASK-003
spec: SPEC-service-identity-and-telemetry
spec_revision: 16
status: approved
kind: implementation
depends_on: [TASK-009, TASK-004, TASK-005]
requirements: [REQ-054, REQ-055, REQ-056, REQ-049, REQ-050, REQ-051, REQ-052, REQ-053, REQ-021, REQ-022, REQ-023, REQ-024, REQ-025, REQ-026, REQ-027, REQ-028, REQ-031, REQ-032, REQ-034, REQ-035, REQ-036, REQ-039, REQ-040, REQ-042, REQ-044, REQ-048]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008]
acceptance: [AC-026, AC-025, AC-024, AC-007, AC-008, AC-009, AC-010, AC-012, AC-014, AC-016, AC-018, AC-021]
---

## Outcome and Value

Every Bifrost row keeps account, exact execution workload and observed Card
independently queryable under one public `bifrost` catalog. Complete typed
signals gain useful resource/URL/exception columns. A span about a shared Model
identifies that Model as subject, its executing Service release as workload,
and the stable account as publisher, with canonical Service and subject name/version dimensions.

## Two telemetry setup paths

Prove the same canonical ingest/projection for Wyrd-managed configured telemetry
and for application-owned providers using standard OTLP exporters and resource
Service/version/environment, with no WyrdState requirement on that path. Resource
environment is promoted when accepted request environment is absent, including
with validated workload context. Labels never establish ownership. Reuse the
existing OTLP/public-exporter proof below; no new exporter API is authorized.

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
- Implement every supplemental field/type/nullability: spans 65, logs 46,
  metrics 58, all unchanged nested data, precision/presence and typed Variant.
  Add six resource fields, span URL path/query and span/log exception booleans.
  Preserve integer counters, SpanKind, metric kinds, summaries and exemplars.
- Apply the exact nine-table partition/sort/Bloom ledger via TASK-004's engine:
  hourly signals/gateway, daily drift/verification/audit; ordered/null-aware
  sorts using common E/S/V/time keys; five shared attribution Blooms unioned
  with signal lookup keys, ten result defaults, span/log span_id opt-in. No hidden floor or claimed performance improvement.
- All nine system tables have the six common nullable Utf8 dimensions
  once: service_name/version, deployment_environment and subject_kind/name/version.
  The server resolves descriptors from validated UIDs without per-row registry SQL
  or new JWT descriptors. There is no workload_kind or Agent workload root.
  Configured Service/default subjects, component binding occurrences, gateway
  invocation context, queued inputs/results and drift all retain the accepted
  context. Dynamic tables keep the seven-field envelope only. Preserve raw OTel
  labels; canonical owned Service values cannot be overridden.
- Direct and queued verification capture one context
  {service_name, service_version, deployment_environment} at acceptance. Queued
  work stores it with the accepted execution; both paths write it unchanged to
  the result and inputs rows. Nothing else reads it. TASK-002/009 already add
  `deployment_environment` to verification `results` and `execution_inputs`
  and write it from direct and queued work; this task adds the other common
  dimensions there and the environment column on the remaining tables.
- Public forged managed/derived values fail before ACK. Internal mixed-origin
  single-tenant batches preserve each origin through forwarding/retry/recovery.
  Preserve atomicity, idempotency, bounded output, Variant validation and tenancy.

Completed TASK-002 plus supplemental TASK-009 supply validated Service-only
configured context/accepted authority; this task owns its row,
schema and consumer closure plus owning architecture amendments. TASK-004 owns
storage mechanics, TASK-005 editing/configuration, TASK-006 trace semantics,
TASK-007 optimization, TASK-008 docsite. Runtime examples/fixtures and SDK
projections change here. No new writer, graph owner, SDK durable behavior,
implicit row policy, cross-store SQL join or evidence/time-window narrowing.

## Gateway client/server closure

[The approved stock-provider SDK example and header contract](../supplements/auth-system.md#stock-provider-sdk-context-and-approved-examples)
are implemented client-side by TASK-009. This task closes the server path:
parse the existing workload/subject/Run headers plus environment; validate UID
and membership before dispatch; resolve canonical Service/subject descriptors;
freeze the accepted context on gateway work and retain it through streaming,
provider attempts, outbox and recovery. Both direct credential-slot mode and
state-bound adapter mode use the same admission/projection. Provider Batch HTTP
operations must no longer discard supplied subject context; do not invent batch
item tracking. Wyrd caller credentials/context do not reach upstream providers.
No readable identity headers, extra columns or physical-default changes.

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
have the complete envelope/domain schema and ledger defaults, including all
six shared dimensions on every attributable table, 65/46/58 signal counts and
exact E/S/V/time sorting and shared-plus-signal Blooms, without duplicate
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
| Wrong stable environment type; valid older key on unowned OTLP | Typed fallback selects older key; both raw attributes remain. |
| Owned resource labels disagree with registered Service | Registered Service columns win; raw supplied labels remain unchanged. |
| Same-name subjects of different kinds or versions | Server resolves all subject descriptors from UID; grouping/filtering remains kind/version aware. |
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
invalid request context refuses the request. Configured records outside component Runs use the Service as subject;
unconfigured generic rows may have workload without subject, or neither.

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
| Direct/queued/group inputs/results/drift features | Accepted authority, configured Service/version/environment, validated exact binding owner/subject and canonical descriptors, Verifier/execution/group independently. |
| Audit verified/unresolved | Original decision pair/sentinel/request, truthful nullable correlations, separate target; failed delivery does not fail the operation. |

**RED.** [Origin proof](#origin-proof) adds a producer matrix and extends audit
identity regression; distinct accounts and roots sharing a component expose worker restamping.

**GREEN.** Extend existing origin/build/outbox/stamp owners preserving accepted
provenance and binding content; refuse mixed tenants and rerun Scenarios 1–5.

**REFACTOR.** Consolidate stamping without another audit sink/writer/public trusted path.

### Scenario 6 — Lifecycle reads and evidence consumers remain exact

**Behavior.** Hot/published/recovered/compacted rows match completely. Direct
Service-name/version and kind/name/version subject aggregates stay independent of component,
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

### Scenario 7 — Gateway headers become canonical correlated rows

**Behavior.** OpenAI-compatible and Anthropic calls using either credential mode
record the same canonical Service/version/environment and subject kind/name/
version/UID, plus Run/trace/account/request context. A Service/default call and
Agent call share Service ownership and differ only in subject/Run. Unknown,
foreign, duplicate/malformed, mismatched signed-root and out-of-graph requests
retain existing refusals before any provider call. Readable label spoofing never
changes registered identity. Environment is per request, never a cache residue.

Buffered/streaming calls, verification-triggered calls and existing Batch HTTP
operations retain admitted origin; gateway cache, provider/model identity,
worker credentials, outbox batching/recovery cannot relabel it. Batch operations
record their HTTP caller context, with no new async item-result protocol. Only
operator provider credentials and allowed protocol headers reach the upstream.

**RED.** Add [gateway projection proof](#gateway-projection-proof). Current
subject-only capture, absent environment/readable projections and Batch
subject: None fail canonical readback and no-provider-on-refusal assertions.

**GREEN.** Extend existing ingress/request/admission/capture/outbox projection
owners to freeze and stamp the shared context. Use TASK-009's actual state-bound
provider adapters for SDK readback and raw explicit headers for server negative
cases. Rerun Scenarios 1–6 and the existing origin/read proofs.

**REFACTOR.** Reuse shared registry descriptor resolution and existing gateway
admission across routes; no duplicate gateway table schema, token graph labels,
per-row registry query or second capture pipeline.

## Acceptance Criteria

| Obligations | Proof and violation |
| --- | --- |
| AC-026; REQ-050–053 | Existing OTLP ingress proof covers Wyrd-managed context and application-managed standard resource values for all three signals, including resource environment with validated UID context but no request environment, typed fallback/null, raw preservation and permission/UID refusals. Neither WyrdState nor a new exporter API is required for application-managed export. |
| REQ-054–056; AC-025 | Gateway projection proof reads actual canonical rows for both provider SDKs/auth modes and verifies pre-dispatch refusals, no context leakage, cache separation, stream/recovery invariance and Batch HTTP context. Any copied worker/provider identity or accepted forged name fails. |
| REQ-021–024; AC-007; INV-008 | Projection/ingress covers every field/rejection; wrong promotion or lost precision fails. |
| REQ-049–053; AC-024 | Schema/ingress/origin/read proof exercises configured Service and Agent subjects, gateway-backed checks, unchanged optional environment tag readback, repeated/shared bindings and same-name kinds across all nine tables; missing/mislabelled dimensions or Agent workload roots fail. |
| REQ-022,028,034; AC-012; INV-001–005,007 | Schema/origin covers nine plus dynamic; duplicates, worker restamping, invented correlations or tenant crossover fail. |
| REQ-025,035–036,042,044,048; AC-016,018,021 | Exact ledger/describe/defaults; hidden floor or disagreement with the revised shared sort/Bloom ledger fails. Engine/config mechanisms belong to TASK-004/005. |
| REQ-026,039–040; AC-008,014; INV-003–004 | Catalog/read/peer regression; conflated identity or stale grants fail. TASK-007 measures pruning/performance. |
| REQ-027,031–032; AC-009–010; INV-006 | Fresh SDK/MCP/CLI/generated proof, runtime example and architecture diff; alias or SDK durable owner fails. Docsite belongs to TASK-008. |

## Implementation Reference

Inspected reuse evidence; paths guide discovery, not private code structure:

| Owner and callers/tests | Gap and selected extension |
| --- | --- |
| Redux `BifrostNamespace`, `TableRef::{fqn,parse_fqn,permission_scope}`; verification SQL, SDK query tests | Old names/scope fallback: change shared identity and all default, physical-binding, permission and operational consumers. |
| `tables/managed_columns.rs::MANAGED_COLUMNS`, `CorrelationPolicy::{appends,reserves,stamps}`, `DomainTable`; schema/layout tests | Existing managed owner and CorrelationPolicy::Observation cover eight tables; extend them with universal envelope plus six shared dimensions on all nine system tables; dynamic tables retain the universal envelope. Extend nine ledgers, reconcile three root fields and audit request/pair. |
| Signal ledgers and `signal.rs::{ResourceEnvelope,RecordCorrelation,promoted_string,promoted_int,last_attribute}`, `project_resource_*`, inline maximal tests | Existing typed Variant/final-key/presence/metric logic fits: extend columns, extraction, validation and accounting without another decoder. |
| Gate native/signal admission, `attribution::native_card_uids`, `gate/auth.rs`; Scribe `prepare_admitted_rows`/`preprocess` | Old subject names/insufficient context: extend current authorization, canonical-value and idempotent admission seams before ACK. |
| Server `scribe_outbox.rs` encode/attributed/`ScribeTable`, `audit/mod.rs`, `ResultPayloadBuilder`, gateway capture | Audit special-case/old owners: extend originating-context builders/stamping, preserve binding content and nonblocking audit. |
| Gateway ingress.rs::requested_subject, invocation.rs::{GatewayCallAttribution,GatewayCallSubject,admit_call}, capture.rs and batches.rs | Current parser/admitted call covers subject/Run/trace but capture lacks full shared context; batches constructs subject: None. Extend these existing boundaries and capture projection, retaining authorization before provider dispatch and settled-call behavior. |
| Existing wyrd-testing gateway target compatible.rs + harness.rs::Journey; Python gateway/test_client_http_auth.py and TS gateway-inference.test.ts | Existing real-server/local-provider and public query readback fixtures support cross-mode row/refusal proof. Add planned cases in these targets, no new binary or live provider. |
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

Existing redux exact-schema/layout tests and shared dimension cardinalities/defaults; planned public nine-system-plus-dynamic describe/write/read.

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

Existing OTLP full/partial tests extended across protobuf/JSON/gRPC; planned native refusal and Python/TS public stock-exporter-to-Bifrost journeys covering all three signals, promotions and origins. The same exact focused cases cover both setup paths: managed state context, and application-owned standard resources with no state, both rootless and optionally UID-attributed. With no accepted request environment, typed resource environment remains available even with validated workload ownership. Preserve accepted request environment precedence, typed fallback/null, all signal values and existing UID/permission refusals.

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

Planned full-value lifecycle/correlation and direct-name/version/environment aggregate comparison plus existing queued-peer and Rust SDK native-table regressions.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise run build:server && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=verification_runtime::canonical_signal_rows_survive_publication_recovery_and_compaction) | test(=verification_runtime::runner_without_local_scribe_publishes_through_a_peer_scribe)'"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-client --test integration -P journey --run-ignored=all -E 'test(=pg_bifrost_e2e::pg_tests::unified_client_registers_writes_swaps_and_reads_both_tables)'"
```

### Gateway projection proof

Planned Rust test in the existing wyrd-testing gateway target/module compatible;
its harness uses WyrdTestServer and a local provider. Planned Python readback
case uses the existing stock-client fixture and public Bifrost query. Confirm
actual selectors after addition and before RED. Server-row assertions belong
here, not in TASK-009's earlier adapter task.

```bash
mise exec -- env WYRD_TEST_GATEWAY_PROVIDER_KEY=sk-native-upstream scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E "test(=compatible::configured_gateway_headers_are_authorized_and_captured)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run py:setup && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/gateway/test_client_http_auth.py::test_configured_gateway_calls_have_canonical_dimensions'
```

Extend TS Task009's existing planned gateway story with canonical Bifrost
readback here, using its exact focused command in TASK-009 rather than adding
a duplicate broad lane. Prove both credential modes, OpenAI/Anthropic,
Service/Agent subjects, cache reuse across env/Runs, buffered/streaming and
outbox recovery, Batch HTTP operations and refusal with no upstream side effect.

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

- [Approved specification revision 16](../spec.md) and its normative
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

## Implementation Evidence

### Diagnoses

**Result Bloom floor.** Symptom:
`verification_runtime::result_layout_partitions_blooms_and_prunes_by_result`
failed with "bifrost.verification.results file … has no Bloom filter on
sub_principal_id". Evidence: the journey hard-coded
`[run_id, subject_card_uid, sub_principal_id, act_principal_id]` as the floor,
while `tables/mod.rs::ATTRIBUTION_BLOOMS` and `attributed_layout` declare the
revised ledger without principal Blooms (`bifrost-design.md`, "never a hidden
validation floor"). Cause: a stale test expectation of the pre-revision floor.
Fix site: the journey reads each table's `physical_layout().bloom_columns`. An
independent read-only diagnostician confirmed the cause and found no other
caller with the same hard-coded floor.

**Observation batch fingerprint.** Symptom:
`verification_runtime::two_bindings_share_one_client_observation` failed with
`FingerprintMismatch { table: "resolved ingress table" }` from Gate
`dispatch_native_frame` on `bifrost.drift.observations`. Evidence:
`ObservationsTable::arrow_fields()` now wraps its authored fields in
`with_shared_dimensions`, and `ConformPlan::build` refuses a registered column
that is neither sent nor reserved; the journey's hand-built schema omitted the
six dimensions. The SDK projection (`wyrd-client/src/observe/drift.rs`) already
sends them null. Cause: a stale hand-built schema. Fix site: the journey builds
its batch over `ObservationsTable::arrow_fields()`. The diagnostician found no
other hand-built observation schema.

**Lifecycle compaction.** The first run of the lifecycle journey matched the
hot, recovered and published reads, but Forge planned no rewrite: each
partition sealed one file. Cause: a single sealed file per partition leaves the
small-files strategy nothing to rewrite. Fix site: the journey seeds two
separately flushed datasets so each partition seals several files.

### Decisions

- Trusted outbox frames resolve the shared-dimension descriptors at delivery
  (`CardRegistry::row_dimensions`), with no migration: a UID's descriptor never
  changes, so a retried or recovered frame resolves exactly the context its
  producer accepted.
- A changed-content native batch replayed under the same `batch_id` is refused,
  but as `WYRD_VALA_500_BIFROST_INTERNAL`; a 409 would change the error
  catalog, so the refusal code is left as is.
- Read proof runs in the Oracle cluster binary, beside the existing Forge
  promotion/rewrite helpers:
  `oracle distributed::canonical_signal_rows_survive_publication_recovery_and_compaction`.

**MCP GenAI attributes.** Symptom:
`wyrd-mcp query::pg_tests::agent_reads_canonical_trace_genai_logs_and_metrics_through_sql`
expected the root span `attributes` Variant to hold only the two message keys;
it read all eight `gen_ai.*` keys. Evidence: `1aaae2e2d` gave the shared
canonical-signal fixture the source keys its promoted columns derive from, as
`refuse_forged_span_derivations` (`tables/traces/projection.rs`) requires;
REQ-023 and `bifrost-design.md` retain every source attribute. Cause: a stale
test expectation. Fix site: `canonical_signals::root_span_attributes` owns the
value and both MCP assertions read it. An independent read-only diagnostician
confirmed the cause and found `builtin_variant_and_struct_payloads_are_queryable`
as the only other affected caller.

### Evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Scenario 1, catalog resolution and grants agree (REQ-026,039–040; AC-008,014) | `e8483adba`, `250027567` | redux `catalog::table_ref::tests::canonical_catalog_has_no_alias`; server `query::canonical_catalog_queries_and_grants_agree`; MCP `query::pg_tests::agent_reads_canonical_trace_genai_logs_and_metrics_through_sql`; CLI `query_server_journey::{query_command_reads_seeded_table,query_command_denial_preserves_problem_without_read_decision}` | PASS |
| Scenario 2, exact schemas and defaults on nine tables plus dynamic (REQ-022,025,028,034–036,042,044,048; AC-012,016,018,021) | `2dfe39609`, `04ea7dd8d`, `f54833deb` | redux `tables::tests::{verification_tables_match_their_approved_schemas,physical_layout_contract_all_builtins,canonical_signal_schemas_round_trip_arrow_parquet_and_iceberg}`, `managed_columns::tests::observation_policy_appends_every_managed_column_in_order`; server `verification_runtime::system_and_dynamic_tables_share_attribution` | PASS |
| Scenario 3, promotions keep raw types (REQ-021–024; AC-007; INV-008) | `ffbe38247`, `7005f8f49` | redux `traces::tests::maximal_span_projection_is_lossless_and_identity_mapped`, `logs::tests::maximal_log_projection_preserves_body_context_and_presence`, `metrics::tests::all_pinned_metric_point_kinds_project_without_narrowing`, `tables::tests::resource_promotions_preserve_final_typed_values` | PASS |
| Scenario 4, public ingress cannot forge attribution (AC-026; REQ-050–053) | `66cf01453`, `1aaae2e2d`, `89be1bb8d`, `1624eb36c`, `2165ee81d` | OTLP `trace_export`/`logs_export`/`metrics_export`/`negative` focused cases (4); server `verification_runtime::native_signal_values_and_attribution_are_validated_before_ack`; Python `test_signal_promotions_and_attribution_read_back_through_bifrost`; TS `signal promotions and attribution read back through bifrost` | PASS |
| Scenario 5, trusted delivery keeps each origin (REQ-049–053; AC-024; INV-001–005,007) | `616c627a2`, `30efb5d90`, `18e725530`, `bifrost-design.md` stamping paragraph | server `verification_runtime::trusted_delivery_preserves_each_origin`, `audit_retention::subject_actor_decisions_preserve_trusted_identity`, `verification_runtime::runner_without_local_scribe_publishes_through_a_peer_scribe`; all 20 `verification_runtime::` journeys | PASS |
| Scenario 6, lifecycle reads stay exact | `879e9ccdc` | oracle `distributed::canonical_signal_rows_survive_publication_recovery_and_compaction` (RED with stamping disabled); client `pg_bifrost_e2e::pg_tests::unified_client_registers_writes_swaps_and_reads_both_tables` | PASS |
| Scenario 7, gateway headers become canonical rows (REQ-054–056; AC-025) | `5d69f2c3e` (`GatewayCallContext`, environment header, Batch HTTP context), `20b77dd2b`, `f786fc6d4`, `a779ac000` | gateway `compatible::configured_gateway_headers_are_authorized_and_captured` (RED with Batch `subject: None`); Python `test_client_http_auth.py::test_configured_gateway_calls_have_canonical_dimensions`; TS `state httpAuth supplies service and agent context`; gateway target 16/16 (vault through `test:gateway:vault:inner`); wyrd-server `gateway::` lib 61/61 | PASS |
| Static and contract closure (REQ-027,031–032; AC-009–010; INV-006) | regenerated `card-types.ts`; `bifrost-design.md` | `fmt`, `lints`, `codegen:check`, `check:deps`, `check:tenant-isolation`, `docs:check`, `py:format`, `py:lints` (stub parity), `py:typecheck`, `ts:format`, `ts:lints`, `ts:typecheck`, `ts:napi:check`, `git diff --check` | PASS |

Non-goals stayed excluded: no catalog alias or migration, no new writer,
exporter API, readable identity header, batch-item correlation protocol, trace
semantics, performance measurement or docsite change. No file outside the
task's write set changed. Material risk: a changed-content native batch replay
is refused as `WYRD_VALA_500_BIFROST_INTERNAL`, not a conflict code.
