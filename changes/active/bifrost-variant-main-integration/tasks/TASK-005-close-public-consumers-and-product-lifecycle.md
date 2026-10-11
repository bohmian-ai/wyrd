---
id: TASK-005
kind: implementation
status: proposed
spec: SPEC-bifrost-variant-main-integration
spec_revision: 2
requirements: [REQ-001, REQ-002, REQ-008, REQ-010, REQ-011, REQ-013, REQ-014, REQ-015, REQ-016, REQ-017, REQ-018, REQ-019, REQ-020, REQ-021, REQ-022, REQ-023, REQ-024, REQ-025, REQ-026, REQ-027]
primary_requirements: [REQ-017, REQ-018, REQ-019, REQ-020, REQ-021, REQ-022, REQ-023]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008, INV-009, INV-010, INV-011, INV-012]
acceptance: [AC-001, AC-002, AC-008, AC-010, AC-011, AC-013, AC-014, AC-015, AC-016, AC-017, AC-018, AC-019, AC-020, AC-021, AC-022, AC-023, AC-024, AC-025, AC-026, AC-027]
scenarios: [S01, S16, S17, S18, S19, S21]
depends_on: [TASK-004]
---

# Public consumer closure

## Outcome and Value

Real Rust/Python/TypeScript SDK and agent callers register, emit, query, verify and shut down through one coherent source-authoritative contract while main Service, Workflow, MCP, gateway, identity and local bootstrap behavior survives.

Primary closure ownership: REQ-017 / AC-017, REQ-018 / AC-018, REQ-019 / AC-019, REQ-020 / AC-020, REQ-021 / AC-021, REQ-022 / AC-022, REQ-023 / AC-023. Supporting clause contributions: REQ-001, REQ-002, REQ-008, REQ-010, REQ-011, REQ-013, REQ-014, REQ-015, REQ-016, REQ-024, REQ-025, REQ-026, REQ-027. All INV-001–INV-012 apply. The task-specific criteria below are scoped contributions; integrated AC closure also includes the contributing tasks in [decomposition](../decomposition.md).

This task requires [shared implementation contract](../implementation-contract.md), [approved spec revision 2](../spec.md), canonical evidence and reconciliation. Current user direction explicitly authorizes this six-task decomposition without changing the approved behaviors. Use `$wyrd-implement`; implementation self-report is not independent task completion.

This is task 5 of 6 for merging the bifrost variant worktree into the current main logic (this worktree)

## Owners, Scope, Consumers, and Prohibited Changes

Own full public consumer closure and the atomic terminal/parameter protocol across spec/protobuf/conversions/server/forwarder/shared client/native bindings/SDKs/CLI/MCP. Reuse TASK-001's already-consistent foundation, adding required actual cross-surface behavior/assertions rather than temporary compatibility. Preserve before-Card-transaction Service ensure and lock-decided Created/AlreadyExists; all-ten eager schemas/payload restrictions and main installed bootstrap. Do not introduce a second client/transport, remove newer main errors, revive broad exports or overwrite main state/boot wholesale.

Depends on accepted TASK-004 because its actual owners/contracts are prerequisites for this outcome. Start from that integrated checkpoint and preserve its proof. No parallel overlapping implementation or Cargo-backed verification.

All source-fidelity, SQL trust, fresh-only, no-deployment/no-push/no-main-merge, no-compatibility and no-gate-weakening prohibitions in the shared contract apply. Paths/symbols are inspected guidance, not a private implementation allowlist. Caller/test/fixture/export/generated/doc closure is part of this task, not work postponed to TASK-006.

### Source-backed reuse

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification | Proof |
| --- | --- | --- | --- | --- | --- | --- |
| REQ-001 / AC-001 — Source analytical engine | S BifrostCatalog, OraclePlanner, ScribeIngress, ForgeRewriteCommit; catalog/oracle/scribe/forge/storage owners | S published/distributed journeys; P01/P06/P07/P22 | M consumers still assume replaced providers/schema/storage | Import cohesive S engine; adapt consumers, not old owners | S rewrite owns real native execution/storage state; no additional engine justified | S01/S03/S04/S06/S07/S20 |
| REQ-002 / AC-002 — Snapshot retention/deadlines | S OraclePlanner::request_deadline/pin_cut; OracleTableCuts::read; Forge expiry protections | P03/P06 source retention and non-blocking-cut proofs | M active-reader SQL and unlimited budgets conflict | Use S retention/clamp and delete superseded reader workflow | Existing S retention is replacement authority; no reader-pin layer | S02/S06 |
| REQ-008 / AC-008 — Membership lifetime | M ClusterTask; ClusterRegistry; BifrostComposition.membership_poller | P08/P09; Oracle/Scribe teardown and boot | S process token stops discovery before drain | Retain M final role tasks + one process poller | Existing concrete owners retain task lifetime | S08 |
| REQ-010 / AC-010 — Shared outbox | M ScribeOutbox/ScribeSink/ScribeBatch; AuditStage; AppState | P11/P12/P13; audit/capture/results | S restores independent old publishers/staging | Retain M one process outbox; change row/frame projection | Existing outbox already owns batching/retry/loss | S10/S11 |
| REQ-011 / AC-011 — Audit schema/attribution | M project_audit_event; GateAudit blanket impl; Oracle query_audit_event; S AuditLogTable types | P12/P13; actual keyed Gate/Oracle -> retained history | S brings chain/staging; M builders omit credential id | M event content + S created_at/Variant; carry credential | Existing event/sink/projection; no chain/hash authority | S11 |
| REQ-013 / AC-013 — UID observation scope | M CardAttributionRegistry::attributed_cards; AuthContext; S decode/conform | P14/P19/P23; OTLP/native/client correlation | S older CardRef correlation drops unbound UID authorization | Port M trusted UID resolution into S admission/decode | Existing registry/signed scope; no new lookup/cache in CPU decoder | S12 |
| REQ-014 / AC-014 — Observation activation | M ObservationRunOutbox/ObservationRunSink; enqueue_observation_batch | P15/P16; ACK decoder created_at and writer scope | S best-effort task and subject-only activation regress M | Retain M retry/bulk/writer/replay; adapt ACK fields | Existing outbox/SQL; no semaphore task replacement | S13 |
| REQ-015 / AC-015 — Verification runtime | M VerifierRunQueue::renew/tick_schedule; EvalEngine::execute; writer_caller; gateway caller | P15/P16; runner/cache/leases, direct routes | S removes renew/hydration/delay and direct-calls providers | Retain M runtime; port only S read/type projections | Existing verification owner; no second runner/provider path | S14 |
| REQ-016 / AC-016 — Result evidence | M ResultPayloadBuilder::build/summary; ResultRun; S typed result schemas | P15/P17; direct Task/continuous Eval/Drift results | S typed Drift/Eval summaries omit M Task and app Run meaning | S typed structs + existing Task AssertionResult as assertion_result Variant | Existing AssertionResult; no new Task type/table | S15 |
| REQ-017 / AC-017 — Declared time axes | S TimeAxis::of_wire; canonical tables; event_time bound conversion; M BifrostReader | P01/P15/P17/P23; all ten table producers/readers | M event-time predicates/types and same-day joins obsolete | Use spec matrix, source precision/conformance; result_id joins | Existing table owners/type mapper; no synthetic time alias | S01/S13/S15/S16 |
| REQ-018 / AC-018 — Remove dev table | M canonical built-in inventory/provisioning; S dev/CodeAxis sole owner | Static I18; source dev tests -> supported/dynamic property proof | S approved dev inclusion conflicts with latest user override | Keep ten, no dev/CodeAxis; transfer still-reachable assertions | No test-only inventory or extra name-ban check | I18/S16/S20 |
| REQ-019 / AC-019 — Typed SQL parameters | M Bifrost::sql/sql_as/stream; QueryParam; S OraclePlanner | P18/P19/P23; HTTP/grpc/SDK query bodies | S unchanged-base signatures omit newer params | Bind M typed ordered values in S plan and keep public argument order | Existing planner/transport; no interpolation/second client | S17 |
| REQ-020 / AC-020 — Terminal problems | S WyrdProblem; tonic query_conversion; M from_problem/error catalog | P19/P22/P23; grpc/http/client/CLI/MCP collectors | Wire terminal changes must agree and preserve M other errors | Atomic S problem field with M params/security | Existing typed catalog/conversions; no compatibility parser | S18 |
| REQ-021 / AC-021 — Shared types/client boundaries | S wyrd-types schema mapper/Variant/timestamps; M Bifrost handle and Service declared_tables | P01/P19/P23; cards/service.rs preflight and SDK exports | Queue mapper removed; broad source exports regress M internals | Use S shared mapper and M narrow facade/preflight | Source shared type owner replaces duplicate mapper, no new transport | S01/S16/S19 |
| REQ-022 / AC-022 — Identity/bootstrap | M ServerAuth/ExchangeApiKey/AuthContext::from_verified; DevServer; ResolvedDsns | P14/P24/P25/P26; issued credentials, OTLP, saved local config | S Gate/state/boot lack newer entrance/wiring | Retain M authorities/verified entrance/DSNs/endpoints | Existing auth/runtime/SQL owners; no raw pools or new credential model | S12/S19/S21 |
| REQ-023 / AC-023 — Product lifecycle | M WorkflowRuns::drain; RunTools; AppState; BoundServer | P09/P20/P21/P24; workflow/MCP/gateway/Operator routers | Whole-source state selection can disconnect M-only owners | Retain connected M owners and ordered drain using S Bifrost | Existing trackers/owners; no general orchestrator | S19/S21 |
| REQ-024 / AC-024 — Migration history | M MIGRATOR/verify_migrations; S three forward migrations | P27/P28; serving/operator readiness and role RLS | Historical checksum/source old grants cannot be transplanted | M bytes + S forward DDL with current tenancy | Existing migrators; no reset or checksum bypass | S22/I24 |
| REQ-025 / AC-025 — Fresh v3 contract | S create_physical_table/physical_table_matches; spec no data migration | P06/P28; dynamic/fresh built-ins and format refusal extension | Old tables/objects are not schema-compatible | Fresh isolated Bifrost; reject old schema/version without mutation | No conversion/dual reader; old-state deployment excluded | S20/S22 |
| REQ-026 / AC-026 — Native graph/tooling | S native Cargo pins/ObjectStore factory helpers; M deps.sh/Hakari/profiles | Static I26; scoped dependency/Clippy/codegen checks | Locks/lints/test scripts/signing drift between snapshots | Resolve combined graph; retain M deleted tooling and artifact storage | Combined compatible compaction pin + required analytical helpers; source workload modes in existing capacity target; no unrelated signer rewrite | I26/S23 |
| REQ-027 / AC-027 — Proof/artifact closure | M consolidated targets/fixture stories/CI selection; source Variant Arrow fixtures | P01-P31; static I27; served OpenAPI/runtime MCP | Source tests/stubs/lanes may disappear or omit new cases | Rehome coverage, regenerate source-owned outputs, docs agree | Reuse M harness; no extra checker/duplicate TS server | All scenarios/I27 |

## Approved Requirement Detail

The following requirement bodies are copied verbatim from revision 2 for packet-local implementation. Primary requirements are fully owned here; supporting requirements contribute only the scope above and remain mapped to their closure owner. Do not reinterpret their full acceptance obligations.

### REQ-001 — One source-authoritative analytical engine

Integrate the source native scans, Variant/shredding, declared timestamp types,
ObjectStore storage, Iceberg v3 publication, cut cache, Scribe recovery, and
distributed executor together. Do not revive replaced analytical scan, pruning,
fixed-IPC, payload-cache, or duplicate storage owners to satisfy old imports.
Rehome compatible newer main behavior in the actual source owner.

### REQ-002 — Source snapshot retention and deadlines

Retain source replacement-time retention, supersession roots, read-cut function,
configured default/max query lifetime, and request deadline clamp. Remove the
superseded per-query SQL reader-pin workflow. Do not introduce main's unbounded
deadline policy or an alternative over-limit rejection policy. Propagate the
source behavior consistently through server and public clients/documentation.

### REQ-008 — Role heartbeat and one membership poller

Retain main's final role-owned ClusterTask lifetime and one process membership
poller. Readiness withdrawal never re-advertises readiness, but heartbeat and
discovery continue while outbox/role drain needs them. Teardown joins/stops
them; failed construction and abort terminate their tasks. Do not restore
source's premature process-token cancellation or per-role poller duplication.

### REQ-010 — One shared non-blocking Scribe outbox

Retain main's process outbox for authorization audit, gateway captures, and
Verifier results. Use destination-grouped bounded Arrow frames and
content-derived batch IDs; retry identical slices, consume/count terminal
rejections, and count observable shutdown loss. Permission completes before
operation/refusal and audit delivery never delays or fails it. No separate
publisher, SQL staging, relay, audit WAL, or public AuditUnavailable error.

### REQ-011 — Combined audit content and credential attribution

Retain main's 11 decision-content fields and direct AuditEvent projection; add
explicit decision `created_at` and source Variant `detail` (12 content fields
before the physical envelope). Preserve nullable credential attribution through
actual Gate/Oracle event builders, not just prepopulated events. No seq/hash
chain columns or sequence ordering. Preserve trusted system-owner rules and
audit Card identity content; `audit_card_ref` is not observation correlation.

### REQ-013 — UID-only observation correlation

Use card_uid in native correlation, OTLP attributes, shared contracts, client
encoding, Gate, ingress, ACK decoding, private peer frames, and SDKs. Bound
principals use trusted signed scope; unbound principals resolve at most 32
distinct UIDs in their tenant and require observation-target Cards. Missing
correlation is valid/null; foreign, missing, wrong-kind, malformed, or forbidden
UIDs refuse the complete frame. Authentication stamps publisher/request identity.
Keep legitimate CardRef contracts elsewhere; no global rename or alias.

### REQ-014 — Acknowledged observations activate the correct writer's bindings

Keep main ObservationRunOutbox retries, bulk enqueue, subject/writer matching,
durable replay fence, and lock ordering. Acknowledged observations do not wait
for derived queue writes. Freeze admitted row created_at as input_event_time,
not receipt time; next-day replay reads the same row and activates once. Do
not replace this with source best-effort tasks or activate every subject binding.

### REQ-015 — Main verification authority and runtime lifecycle

Keep main claim hydration, cache/baseline ownership, lease renewal, delayed
schedule clock, queue SQL, direct invocation permissions, and trusted tokenless
reads. LLM judges use the governed gateway under the correct judging authority;
no direct ProviderRegistry bypass. Preserve authenticated writer and application
Run lineage while adapting Variant/timestamp readers. Result delivery remains
on the shared outbox and never delays durable run settlement.

### REQ-016 — Typed Drift/Eval summaries and complete Task evidence

Use source typed drift_report/eval_summary. Preserve nullable assertion_result
Variant for Task results only, using the existing AssertionResult
contract; no new Task payload type/table or duplicate summaries. Preserve
nullable/source summary semantics and sensitivity metadata. Drift/Eval summaries
remain typed Structs; open features inside them may be Variant. Sensitivity is
descriptive except main's specific gateway payload-read permission; do not add
a general elevated-read requirement for verification summaries. Result run_id is
the application Run being judged, not the Verifier execution UUID; result and
Verifier-run linkage remain separately attributable.

### REQ-017 — Declared time axes for all retained tables

Remove managed wyrd_event_time and use source types, precision, bounds, and
layout semantics consistently across ingestion, WAL, hot/sealed reads, Forge,
and consumers. card_uid/run_id are correlation-only; server-owned columns stay
unwritable. Dynamic tables default to wyrd_ingested_at or use a valid declared
non-null source-supported timestamp axis. Parent/detail joins use result_id
and cannot assume both rows share a day partition.

| Retained built-in | Declared time axis | Required adaptation |
|---|---|---|
| vala.traces.spans | start_time_unix_nano | Timestamp(ns,+00:00) OTLP instants; preserve exact ordering/bounds. |
| vala.metrics.points | time_unix_nano | Typed point/start/exemplar instants. |
| vala.logs.records | time_unix_nano | Typed event/observed instants. |
| vala.drift.observations | created_at | Timestamp(us,+00:00); retain main window/aggregation semantics. |
| vala.eval.observations | created_at | Variant context/media; preserve writer/Run inputs. |
| vala.verification.results | ended_at | Source typed summaries plus Task assertion_result Variant. |
| vala.drift.result_features | wyrd_ingested_at | result_id join; measured window stays domain time. |
| vala.eval.result_items | wyrd_ingested_at | Variant actual/expected; result_id join. |
| vala.gateway.calls | started_at | request_payload/response_payload Variant, preserving payload-read authorization. |
| vala.system.audit_log | created_at | Actual decision timestamp and Variant detail, no chain. |

### REQ-018 — No development agent trace table

Keep vala.dev.agent_traces removed and remove its sole-owner CodeAxis behavior,
registration/docs/test dependencies. Maintain ten canonical built-ins. Preserve
generic Variant/admission proof using supported or dynamic tables; do not
restore a test-only capability. Main eager provisioning/startup checks operate
over the ten source-updated schemas; source idempotent ensure remains available.

### REQ-019 — Preserve main typed query parameters

Keep ordered QueryParam across shared Rust facade, HTTP/gRPC, forwarding,
native bindings, Python, and TypeScript. Bind values in source planning rather
than SQL interpolation; preserve main public argument positions. This source
omission is unchanged ancestry, not intentional API removal. Parameter support
does not change source deadline clamping or source plan/result semantics.

### REQ-020 — Atomic terminal problem protocol

Take source WyrdProblem terminal representation and protobuf error_problem_json
field 9 with old error field 6 reserved. Keep main request params and other
newer error identities. Spec, protobuf, conversions, server streams, client
reconstruction, SDKs, CLI, and MCP must agree. Partial frames followed by failure
cannot become success. Keep revision/body/tenant validation and no compatibility
shim. Python QueryTerminal.error_code returns the stable catalog code, and error_detail
comes from the problem detail. Descriptor generation follows current main build ownership.

### REQ-021 — Shared types and narrow first-class client projections

Adopt source wyrd-types, schema mapper, timestamp/Variant conversions, row
preflight, and logical JSON rendering. Keep main internal feature gates and
renewable credential pooling identity. Adapt Service declared-table preflight
and registration to the new mapper without dropping its before-Card-transaction
validation/ensure semantics. Registration Created/AlreadyExists is the source
outcome decided under its registration lock; callers must not infer it by a
describe-before-create race. A propertyless top-level table model stays invalid;
open column values can be Variant. Clients do not duplicate server/transport owners.

### REQ-022 — Main identity, credentials, SQL trust, and bootstrap

Retain main auth/credential/role/grant/revocation and human/platform separation,
API-key OTLP authentication-before-decode, verified-principal Gate entrance,
external Postgres credentials, and default local storage. New Bifrost SQL obeys
TenantConn/OperatorPool and main role/RLS conventions; no naked pool/connection
API. The narrow SECURITY DEFINER newest-WAL-fence function follows main's
operator raise/restore and bounded search_path/privilege conventions. Keep selected local HTTP/gRPC endpoints and safe persisted client config.
Do not revive obsolete workload/bootstrap grants or Argon2 credential behavior.

### REQ-023 — Retained product owners and ordered shutdown

Keep main Workflow/MCP/gateway/Operator/Trigger/verification/identity/principal
owners, routers, configuration, and tracking connected in state and boot. Drain
accepted Workflows while their dependencies remain available; query tools settle
before run terminal state. Account for tracked calls, drain shared Scribe outbox
while Bifrost accepts, then settle roles/storage and observation queue within
the existing process deadline. Preserve errors and lifecycle settlement rather
than merely restoring a shutdown call.

### REQ-024 — Main migration history plus source forward DDL

Preserve main applied historical migration bytes/checksums and retired audit
tables' drop history. Import only source's three genuinely new retention,
cut-notification, and WAL-fence migrations, adapting role/RLS/search_path/grants
to main. Put genuinely required historical DDL differences in uniquely versioned
forward migrations; comment changes do not justify checksum changes. Keep old
workload/grant deletions: they are unchanged source/base, not new source changes.
Fresh replay and forward migration from main with no old Bifrost objects must
produce equivalent target SQL while preserving identities/Cards/credentials.

### REQ-025 — Fresh-only Bifrost, no implicit destructive transition

Honor source fresh Iceberg-v3-only and no mixed-version/data-migration contract.
Do not rewrite stored fingerprints, relax schema/format refusals, add historical
read conversion, or delete state to make startup succeed. Use isolated fresh
test Bifrost state. Retained old Bifrost deployment/export/reset is outside this
task; main-owned control-plane data is never collateral cleanup.

### REQ-026 — Coherent native graph and retained main simplifications

Take source Arrow/Parquet 60, DataFusion 55.1, object_store 0.14.2, Variant crates,
and analytical forks as a coherent graph. The compaction pin must be one reachable
immutable commit combining source compatibility with main's no-REST/Ring
narrowing. The supplied commit is
`22cd178d14009468bc951a14d7acf63237af808a` on the fork’s `wyrd/main`,
already pinned by integration commit `914495674`; preserve it when importing
source manifests. Neither old source 6965606 nor main 4e9f325 is sufficient. No
Ring waiver is allowed. Preserve unrelated locked versions during directed graph
resolution rather than performing a blanket dependency update; preserve main unrelated deps,
features, profiles, pedantic rules, tier boundaries, and removed Ring/unearned
features. Regenerate combined lock/Hakari output. Carry only shared-storage
helpers needed by source Bifrost; retain main artifact signing/storage workflow.
Preserve source query/write/ingest-client benchmark workloads, inputs, assertions,
metrics, report meaning and usable mise entrypoints, but compile them into main's
existing consolidated capacity target. Preserve main mixed-capacity defaults;
retain source Host versus main scoped placement and experiment lifetime semantics.
Do not restore per-workload source binaries, create target directories per case,
or change the reference variant worktree. Do not restore unrelated SignerFuture churn, retired checks/family scripts,
checked-in proto descriptor, or source review-skill/.claude changes.

### REQ-027 — Consumer-complete tests, artifacts, and documentation

Port useful source journeys/Arrow fixtures into main consolidated targets and
current public fixture stories. Reuse main isolated TS server fixture. Cover all
three SDKs and agent surfaces, registering new Variant/time/timestamp cases in
focused lanes and CI selection without silent omission. Regenerate schemas,
stubs, native TS declarations, errors, and proto outputs from source owners;
remove managed_candidates consistently. Update served OpenAPI, runtime MCP,
examples, and owning architecture where their actual contracts change. No
weakened assertions, manufactured tests for static work, or restored obsolete
test binaries/harnesses.

## Approach

1. Complete declared schema/time/UID and Service preflight through existing shared mapper and registration outcome under lock; prove nested/time/Variant combined Service boundary.
2. Close all ten built-in producers/readers, payload-read security and removed dev/CodeAxis assertions across public surfaces.
3. Preserve ordered parameters and public argument positions in source planning/shared transport/bindings without SQL interpolation or changed deadline policy.
4. Prove terminal WyrdProblem propagation atomically across all surfaces, including failure after an initial batch, Python stable error_code/detail and protobuf reserved old field.
5. Retain connected main Workflow/MCP/gateway/Operator/Trigger/identity/verification owners and ordered shutdown; prove installed bootstrap selected HTTP/grpc config in each owning language.
6. Port source assertions and fixtures into main consolidated targets, regenerate outputs and update served OpenAPI/runtime MCP/examples/owning docs as contracts change.

## Ordered Implementation Scenarios

Proof class: mixed. Changed executable seams require actual Behavior/RED/GREEN/REFACTOR; unchanged source behavior may use documented verification-only/no-op proof. Static/generated/configuration work does not require manufactured RED. Original scenario blocks below are retained verbatim, including mapped obligations, expected failures, required extensions and earlier-regression references. Their order is task-local; the shared contract explains cross-task regression references.

### S01 — Fresh registration and lossless typed rows

**Behavior.** A fresh user/model/Service table describes the registered schema and time axis, rejects undeclared/type-invalid rows before admission, and returns nested Struct/List/Variant and explicit timestamp values through the source contract. Mapped obligations: REQ-001 / AC-001, REQ-017 / AC-017, REQ-021 / AC-021.

**RED.** Use P01 and the source Python/TS file recipes P23; extend the registration/round-trip assertions at the combined Service and UID boundary. Expected RED is mismatched schema/time/Variant decoding or acceptance of a forbidden row, not an absent test module.

**GREEN.** Import source type mapping, preflight, declared layout, and projection in current owners; adapt main Service table preflight to the new mapper. Rerun the specific registering/writing/reading cases that changed.

**REFACTOR.** Remove duplicate mapper/conversion paths; preserve per-column types, strict validation and narrow client exports.

### S16 — Ten built-ins and protected payloads use declared schemas

**Behavior.** All ten built-ins are eagerly available with source time/types; dev table/CodeAxis are absent. Gateway Variant payload projection/predicate/subquery access still requires payload-read authority; metadata remains ordinarily readable. Mapped obligations: REQ-017 / AC-017, REQ-018 / AC-018, REQ-021 / AC-021.

**RED.** Extend current query journey P19 with all-ten description/empty reads and removed-table absence, then run P18 for renamed payload protections. Expected RED is an old schema, revived dev table, missing eager table, or payload authority bypass. Static I18 complements rather than replaces real reads.

**GREEN.** Update canonical table registrations, all consumers/time predicates and protected payload names. Transfer generic removed-table Variant assertions to supported/dynamic owners. Rerun S01/S12/S15.

**REFACTOR.** Remove sole-owner dev/CodeAxis definitions/docs/tests; do not add name-ban machinery or a test-only inventory.

### S17 — Bound SQL stays data across first-class clients

**Behavior.** Ordered parameters bind in source plan and forward unchanged through HTTP/gRPC/Rust/Python/TS, preserving argument positions and separating hostile text from SQL. Source deadline behavior is unchanged. Mapped obligations: REQ-019 / AC-019.

**RED.** Run P19/P23 and shared-client bind proof P18. Expected RED is dropped/reordered values, interpolation, wrong TS rows/params position, or changed result projection.

**GREEN.** Carry main request/facade/native parameter projection into source planning and update imported callers. Rerun S01/S02/S16 typed reads.

**REFACTOR.** Reuse shared transport/planner and existing typed QueryParam conversion; no alternate query client or parameter parser.

### S18 — Failure after rows remains a typed failed terminal

**Behavior.** Server/grpc/forwarder/client/SDK/CLI/MCP reconstruct the source WyrdProblem exactly; prior rows do not become complete success, old protobuf field remains reserved, and main newer non-Bifrost errors remain available. Mapped obligations: REQ-020 / AC-020.

**RED.** Extend existing streaming/error owners in P19/P22/P23 with an induced failure after an initial batch and assert typed problem/terminal behavior. Expected RED is missing problem fields, legacy terminal decoding, partial success, or widened worker context. Run P29 for actual MCP/CLI and HTTP framing; extend its compiled CLI/MCP journeys for late failure because the framing unit alone is insufficient.

**GREEN.** Integrate terminal contract and consumers atomically with main params/security; Python error_code is the catalog code and error_detail is the problem detail. Rerun S09/S17 frame and request proof.

**REFACTOR.** Use derive-backed catalog reconstruction; no manual parallel code map or old-terminal compatibility shim.

### S19 — Main app owners and Service registration stay connected

**Behavior.** Service declared tables validate/ensure through source mapper before Card transaction, main Workflow/MCP/gateway/Operator/Trigger/auth owners retain mounts/config/authority, and shared outbox is boot-composed once. Mapped obligations: REQ-021 / AC-021, REQ-022 / AC-022, REQ-023 / AC-023.

**RED.** Run Service preflight and composed OpenAPI P30, changed-boot P24, and Workflow P20/P21; extend these existing proof owners for the Variant table and connected main owner seams. Expected RED is table preflight divergence, missing route/owner, duplicate outbox, or wrong credential/tenant composition. Record exact newly selected owner commands before GREEN.

**GREEN.** Adapt main state/boot and Service mapper use around source role/resource/storage construction; consume source registration outcome under its lock, never infer Created/AlreadyExists by a prior describe. Rerun S10/S12/S14/S16 after constructors change.

**REFACTOR.** Retain existing app services/handles; no new umbrella owner, graph layer or generic bootstrap wrapper.

### S21 — Shutdown and installed local bootstrap preserve real callers

**Behavior.** Accepted Workflow tools settle before terminal run state; admission/reservations and abandoned blocking work remain tracked. Installed CLI provisions empty state, saves selected endpoints/credentials, and subsequent Rust/Python/TS/MCP clients work without developer credential leakage. Mapped obligations: REQ-022 / AC-022, REQ-023 / AC-023.

**RED.** Run P20/P21/P25/P26. Expected RED is a detached query, forgotten preparation, removed app owner, stale saved gRPC endpoint, altered credential state, or inherited local config winning. Bootstrap starts unmigrated; do not pre-migrate that fixture.

**GREEN.** Retain main product drain/config/endpoint persistence around source Bifrost lifetimes. Rerun S07-S11 where teardown/routing changes.

**REFACTOR.** Use existing trackers, installed harness and isolated SDK fixture; no duplicate test server or new config surface.

## Acceptance Criteria

- **TASK-005-C01.** S01 proves the combined Service-declared table/UID/nested/time/Variant workflow and strict refusal; no describe-before-create race changes registration outcome.
- **TASK-005-C02.** S16 proves all ten eager schemas and supported empty reads, removed dev/CodeAxis and gateway payload protections without assuming audit history stays empty after authorized calls.
- **TASK-005-C03.** S17 proves typed bind values and original public argument positions through all first-class runtimes with hostile text remaining data; source deadlines remain clamped.
- **TASK-005-C04.** S18 proves full typed problem and no complete success after delivered rows in shared SDK/HTTP/grpc/forwarding/CLI/MCP; preserve stronger source assertions and the reserved old terminal field.
- **TASK-005-C05.** S19/S21 prove Service-before-Card ensure, connected main owners, tracked Workflow/query settlement and dependency-preserving ordered shutdown.
- **TASK-005-C06.** Installed CLI starts from unmigrated isolated state, preserves credentials/control plane after repeat/failure/interruption and saves selected endpoints consumed by real Rust/Python/TS/MCP callers.
- **TASK-005-C07.** Source assertions, runtime catalogs, served OpenAPI, generated schemas/stubs/declarations, examples and architecture agree; no user-visible negative flow is replaced by a unit-only substitute.
- **TASK-005-EVIDENCE.** Every assigned scenario records required assertions and actual exact-command RED/GREEN or justified preserved/no-op results, plus affected regressions; each relevant REQ/AC has source/diff evidence. Update per-path and historical dispositions for this write set and preserve all invariants.
- **TASK-005-REVIEW.** Fresh independent task review evaluates the immutable cumulative task, its necessary consumer closure and source-authority exceptions before accepting the checkpoint. Missing proof and unexplained source divergence are not PASS.

## Expected Write Set and Consumer Closure

spec/API/errors/schema and tonic/proto converters; Oracle/forwarding/collectors; wyrd-client facade/query/transport/schema helpers and credential pooling; Rust/Python/TS SDK boundary/exports/stubs/declarations; Service/cards preflight; main state/boot/routes/workflow/query tracker/shutdown; CLI/MCP and installed harness; focused source/client/product journeys, examples/docs/CI registration.

Trace whole workflows through callers, producers/readers, schemas, exports, fixtures, generated outputs, tests, command registration and owning docs. Tests requiring Python/Node lifetimes run in those runtimes. Rehome source assertions by body and critical behavior rather than function-name matching; keep current main consolidated targets. Update documentation and generators with actual contract changes in this task.

## Verification and Evidence

Read the shared recipe/execution contract. Selected historical recipe groups: P01, P03, P09, P12, P14, P15, P17, P18, P19, P20, P21, P22, P23, P24, P25, P26, P29, P30. Run only exact selectors supporting the active scenario plus changed earlier regressions; multi-selector recipes may be narrowed without dropping required assertions. Correct relocated names/features/setup through actual discovery and record non-weaker proof. New scenarios extend listed owners; exact new selectors are chosen/discovered and executed before GREEN, rather than predetermined fictitious names.

Affected Rust/static lanes: `mise run fmt:check`, `mise run lints`, `mise run check:deps`, `mise run check:tenant-isolation`, `mise run codegen:check`; `git diff --check`. Apply Python format/lint/typecheck and TS format/lint/typecheck/native-declaration checks only when those surfaces change; use owning generators and `codegen:check` for changed contracts. Run docs-site checks when docs-site content changes. Add only a narrow affected owner lane whose property is not proven by exact selectors, recording why. No task-local full language/journey suite or broad aggregate.

Record cross-task regression triggers now; later modifications must rerun affected accepted proof. Task-local acceptance never waives final specification-owned gate/geometry/parity.

### Non-TDD proof

Relevant I18/I24/I26/I27 contributions (removal/history/graph/registration/artifact/docs metadata) use static or regression proof; changed runtime scenarios still require the complete TDD cycle.

### Exact Selected Recipes

#### P01 — Declared schema/time conformance (source lib owners)

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=catalog::layout::tests::time_column_drives_partition_and_sort)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=catalog::layout::tests::invalid_time_column_is_refused_naming_the_field)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::execution_lanes::tests::same_iceberg_type_spellings_cast_to_the_registered_type)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::execution_lanes::tests::a_naive_timestamp_is_refused_for_a_zoned_column)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::parquet_writer::tests::row_group_bounds_read_the_time_column)'
```

#### P03 — Source deadline projection and retention configuration

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::tests::request_deadline_projects_unusable_budgets_to_the_default)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib --features test-support -E 'test(=config::tests::max_query_deadline_sits_between_default_deadline_and_retention)'
```

#### P09 — Real membership lifetime and audit drain

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=owner_inspection::unready_roles_heartbeat_until_teardown_ends_membership_tasks) | test(=query::oracle_only_pod_retains_audit_staged_before_its_drain)"'
```

#### P12 — Native UID/ACK/audit projection and actual Gate adapter

```bash
WYRD_LOG=info mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=tables::audit::projection::tests::projects_canonical_content_with_the_decision_time) | test(=tables::eval::observations::tests::acknowledged_keys_mirror_scribe_stamping) | test(=scribe::execution_lanes::tests::card_scope_rejects_before_writer_admission) | test(=scribe::execution_lanes::tests::optional_and_scoped_card_correlations_stamp_signed_uids)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=gate::tests::gate_enforces_bifrost_record_write)'
```

#### P14 — OTLP API-key and UID runtime entrances

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey --run-ignored=all -E "test(=trace_export::pg_tests::stock_exporter_authenticates_with_api_key_after_token_lifetime)"'

scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_LOG=info,vala_bifrost_redux=debug mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey --run-ignored=all -E "test(=negative::pg_tests::unbound_writer_attributes_spans_to_registered_cards_only) | test(=negative::pg_tests::identical_exports_from_two_principals_each_keep_their_own_attribution)"'
```

#### P15 — Evaluation/Task/trace/result layout real journeys

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_LOG=info,vala_bifrost_redux=debug mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=eval_verification::eval_runs_follow_the_writing_owner) | test(=eval_verification::integrated_enqueue_outage_preserves_ack_and_recovers) | test(=eval_verification::sealed_replay_on_a_later_day_activates_once) | test(=eval_verification::continuous_eval_runs_the_terminal_matrix) | test(=eval_verification::direct_task_judgment_records_one_correlated_result) | test(=eval_verification::continuous_eval_reads_ordered_bounded_trace_evidence) | test(=eval_verification::continuous_eval_refuses_a_trace_over_the_span_ceiling) | test(=verification_runtime::result_layout_partitions_blooms_and_prunes_by_result)"'
```

#### P17 — Result authoring and schema-by-name

```bash
WYRD_LOG=info mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --lib -E 'test(=verification::results::tests::direct_task_writes_one_summary_row) | test(=verification::results::tests::eval_result_writes_ran_and_skipped_items_then_summary) | test(=verification::results::tests::sampled_out_eval_writes_zero_count_summary_only) | test(=verification::results::tests::reordered_table_fields_keep_values_bound_to_names)'
```

#### P18 — SQL bind forwarding and protected gateway payloads

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E "test(=query_bifrost::parameterized_sql_returns_the_callers_rows) | test(=query_bifrost::bound_sql_text_is_treated_as_data) | test(=query_bifrost::stream_yields_arrow_batches_and_a_terminal) | test(=query_bifrost::caller_without_bifrost_read_is_refused)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E "test(=observe_a_run::run_observations_read_back_by_run_id) | test(=observe_a_run::card_scoped_key_cannot_write_another_cards_observations) | test(=observe_a_run::unsealable_byte_budget_is_refused_at_connect)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E "test(=scheduled_drift_alerts_operator::failed_schedule_alerts_its_operator_on_the_connection_origin) | test(=verify_in_real_time::exact_verifier_grant_verifies_only_that_verifier) | test(=verify_in_real_time::another_tenant_cannot_verify_the_assistant)"'

mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=bifrost::query::tests::sql_forwards_bind_values_without_interpolation)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::tests::gateway_payload_columns_require_payload_read_authority)'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --lib -E "test(=components::gateway::pg_invocation_tests::gateway_payload_objects_are_authorized_convergent_and_stable_when_expired)"'
```

#### P19 — Focused first-class Rust query/observation/verification stories

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E "test(=query_bifrost::parameterized_sql_returns_the_callers_rows) | test(=query_bifrost::bound_sql_text_is_treated_as_data) | test(=query_bifrost::stream_yields_arrow_batches_and_a_terminal) | test(=query_bifrost::caller_without_bifrost_read_is_refused)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E "test(=observe_a_run::run_observations_read_back_by_run_id) | test(=observe_a_run::card_scoped_key_cannot_write_another_cards_observations) | test(=observe_a_run::unsealable_byte_budget_is_refused_at_connect)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E "test(=scheduled_drift_alerts_operator::failed_schedule_alerts_its_operator_on_the_connection_origin) | test(=verify_in_real_time::exact_verifier_grant_verifies_only_that_verifier) | test(=verify_in_real_time::another_tenant_cannot_verify_the_assistant)"'

mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=bifrost::query::tests::sql_forwards_bind_values_without_interpolation)'
```

#### P20 — Accepted Workflow owner accounting

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib --features test-support -E 'test(=components::workflow::runs::tests::drain_waits_for_a_reservation_before_its_task_exists) | test(=components::workflow::runs::tests::drain_waits_for_blocking_work_its_caller_abandoned)'
```

#### P21 — Real Workflow shutdown and forwarded query settlement

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration -P journey -E "test(=pg_workflow_runs::lifecycle_races_retention_and_shutdown)" && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=workflow::workflow_forwarded_query_settles_before_the_run_ends)"'
```

#### P22 — Source logical Variant reads and recovered publication

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=published::variant_sql_registry_covers_every_session)'"

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=published::struct_and_variant_share_physical_pushdown)'"

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=distributed::pg_hot_variant_filter_on_an_unshredded_key_reads_null)'"

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E 'test(=recovery::variant_staging_restores_and_publishes_once)'"

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::parquet_writer::tests::recovery_runs_are_unshredded_and_the_claim_sample_shreds)'
```

#### P23 — Focused Python/TypeScript public stories and imported source capability files

```bash
# Runtime prerequisites once, before the focused tests below.
mise run py:setup
mise run ts:build
mise run ts:build:testing

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_query_bifrost.py::test_bound_sql_text_is_treated_as_data tests/integration/test_query_bifrost.py::test_stream_yields_arrow_batches_and_a_terminal tests/integration/test_observe_a_run.py::test_run_observations_read_back_by_run_id tests/integration/test_observe_a_run.py::test_card_scoped_key_cannot_write_another_cards_observations tests/integration/test_scheduled_drift_alerts_operator.py::test_failed_schedule_alerts_its_operator_on_the_connection_origin tests/integration/test_verify_in_real_time.py::test_exact_verifier_grant_verifies_only_that_verifier'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/query-bifrost.test.ts tests/integration/observe-a-run.test.ts tests/integration/scheduled-drift-alerts-operator.test.ts'

# Source files become available after import; retain their inspected names.
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/bifrost/test_variant_tables.py tests/integration/bifrost/test_every_iceberg_column_type.py tests/integration/bifrost/test_register_a_table_from_a_model.py tests/integration/bifrost/test_three_timestamp_types.py tests/integration/bifrost/test_table_time_column.py'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/variant-tables.test.ts tests/integration/every-iceberg-column-type.test.ts tests/integration/register-a-table-from-a-model.test.ts tests/integration/three-timestamp-types.test.ts tests/integration/table-time-column.test.ts'

# Rehomed Rust client cases: discover actual final symbols in existing target.
mise exec -- cargo nextest list --locked -p wyrd-client --features test-support --test integration
```

#### P24 — Changed boot, credentials, signing boundary and platform initialization

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=boot::auth::tests::build_auth_handles_mints_tokens_the_request_verifier_accepts) | test(=boot::auth::tests::build_auth_handles_rejects_an_invalid_signing_key)'
mise exec -- cargo nextest run --locked -p wyrd-sql --lib -E 'test(=dsn::tests::explicit_serving_dsns_stay_separate_and_derive_the_catalog) | test(=dsn::tests::platform_url_falls_back_to_database_url)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=config::tests::the_signing_key_file_requires_a_restrictive_regular_bounded_file)'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E "test(=platform_admin_e2e::an_operator_initializes_the_deployment_through_the_shipped_binary) | test(=platform_admin_e2e::operator_provisions_a_usable_tenant_without_touching_the_database) | test(=platform_admin_e2e::revoking_a_platform_credential_ends_its_live_sessions)"'
```

#### P25 — Installed CLI bootstrap (unmigrated fixture)

```bash
mise exec -- cargo build --locked -p wyrd-cli -p wyrd-server
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=pg_tests::server_dev_first_run_is_client_ready) | test(=pg_tests::server_dev_repeat_and_partial_failure_preserve_state) | test(=pg_tests::server_dev_selected_mcp_host_reads)'
```

#### P26 — Installed local callers in owning language runtimes

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration 'tests/integration/test_local_development.py::test_installed_cli_bootstrap[rust]'
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration 'tests/integration/test_local_development.py::test_installed_cli_bootstrap[python]'
mise exec -- scripts/postgres/with-test-postgres.sh -- node scripts/check_ts_package.mjs mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration 'tests/integration/test_local_development.py::test_installed_cli_bootstrap[typescript]'
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration 'tests/integration/test_local_development.py::test_installed_cli_bootstrap[interrupt]' 'tests/integration/test_local_development.py::test_installed_cli_bootstrap[exit]'
```

#### P29 — Mounted MCP, compiled CLI and late HTTP terminal

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E "test(=query::pg_tests::agent_investigates_from_multiple_evidence_pivots) | test(=query::pg_tests::bifrost_query_errors_are_actionable_before_row_execution) | test(=query::pg_tests::delegated_agent_query_is_authorized_as_the_subject)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-cli --test cli -P journey -E "test(=query_server_journey::query_command_reads_seeded_table) | test(=query_server_journey::query_command_denial_preserves_problem_without_read_decision)"'

mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=query::routes::tests::http_query_stream_preserves_late_failed_terminal)'
```

#### P30 — Service before-activation preflight and served OpenAPI

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration -P journey -E "test(=pg_card_registration_route::service_declared_tables_are_ensured_before_activation)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration -P journey -E "test(=pg_openapi_contract::bifrost_operations_publish_typed_problem_refusals) | test(=pg_openapi_contract::every_problem_response_declares_its_media_type_and_stable_code) | test(=pg_openapi_contract::no_operation_documents_an_audit_caused_refusal)"'
```

## Material Stop Conditions

- A required product/wire/persisted/security/lifecycle outcome contradicts
  approved revision 2 or needs a different decision. Finish unaffected work,
  cite the exact conflict and request a semantic revision; do not guess.
- Supplied compaction commit `22cd178d14009468bc951a14d7acf63237af808a`
  becomes unreachable or fails source Arrow60 compatibility or main no-REST/Ring
  narrowing in the combined graph. Report the concrete failure; do not replace
  the pin silently, waive checks, or publish externally without authorization.
- Work would reset/migrate retained old Bifrost objects or damage main control
  plane. Fresh-only scope does not authorize data deletion.
- A proposed fix requires a new public client/service, dependency family,
  cross-tenant/raw SQL boundary, scope widening, or optional feature not earned
  by the approved analytical graph/foreign-runtime boundary.
- User/main/source input pins change materially; record the delta and resolve
  its authority before extending this packet's scope.

Compiler, lint, setup, current test-name, private plumbing or sanctioned fixture
failures are ordinary implementation diagnosis, not material stop conditions.
A required red check blocks completion; no authorship hunt, older-code baseline
checkout, disabling assertion, extra sleep, or unsupported no-op substitute.

An unexplained substantive S divergence outside approved M exceptions is a material authority conflict, not permission for a local redesign. Preserve previous task checkpoints; do not edit their acceptance outcome to hide a regression.

## Authority Links

- [Spec revision 2](../spec.md)
- [Six-task traceability and dependency map](../decomposition.md)
- [Shared implementation contract](../implementation-contract.md)
- [Complete original task, verbatim historical source](../source/TASK-001-integrate-bifrost-variant-with-main.md)
- [Complete proof recipes and inspected selectors](../proof-recipes.md)
- [Canonical evidence](../evidence.md)
- [Reconciliation history](../reconciliation.md)
- [Repository standards](../../../../AGENTS.md)
- [Agent rules](../../../../architecture/agent-rules.md)
- [Wyrd design](../../../../architecture/wyrd-design.md)
- [Bifrost authority](../../../../architecture/bifrost-design.md)
- [Spec-driven development](../../../../architecture/references/languages/spec-driven-development.md)
- [Implementation execution](../../../../architecture/references/languages/implementation-execution.md)
- [Testing workflows](../../../../architecture/references/languages/testing-workflows.md)
