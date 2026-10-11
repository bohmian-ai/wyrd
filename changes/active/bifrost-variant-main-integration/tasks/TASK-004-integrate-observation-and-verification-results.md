---
id: TASK-004
kind: implementation
status: proposed
spec: SPEC-bifrost-variant-main-integration
spec_revision: 2
requirements: [REQ-010, REQ-012, REQ-013, REQ-014, REQ-015, REQ-016, REQ-017, REQ-023, REQ-027]
primary_requirements: [REQ-014, REQ-015, REQ-016]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008, INV-009, INV-010, INV-011, INV-012]
acceptance: [AC-010, AC-012, AC-013, AC-014, AC-015, AC-016, AC-017, AC-023, AC-027]
scenarios: [S13, S14, S15]
depends_on: [TASK-003]
---

# Verification composition

## Outcome and Value

Acknowledged source observations activate the correct main writer bindings once, and the retained verification runtime produces attributable typed Drift/Eval/Task evidence without coupling run settlement to result delivery.

Primary closure ownership: REQ-014 / AC-014, REQ-015 / AC-015, REQ-016 / AC-016. Supporting clause contributions: REQ-010, REQ-012, REQ-013, REQ-017, REQ-023, REQ-027. All INV-001–INV-012 apply. The task-specific criteria below are scoped contributions; integrated AC closure also includes the contributing tasks in [decomposition](../decomposition.md).

This task requires [shared implementation contract](../implementation-contract.md), [approved spec revision 2](../spec.md), canonical evidence and reconciliation. Current user direction explicitly authorizes this six-task decomposition without changing the approved behaviors. Use `$wyrd-implement`; implementation self-report is not independent task completion.

This is task 4 of 6 for merging the bifrost variant worktree into the current main logic (this worktree)

## Owners, Scope, Consumers, and Prohibited Changes

Own ACK-to-ObservationRunOutbox/bulk queue/replay linkage, retained main claim/cache/baseline/lease/schedule/gateway authority, and typed result authoring/readback/application-Run lineage. Preserve source timestamp/Variant readers and result_id joins across detail days. Main judging authority, non-blocking result delivery and Task AssertionResult are approved exceptions; no new Task type/table, direct provider bypass or minted SYSTEM reader credential.

Depends on accepted TASK-003 because its actual owners/contracts are prerequisites for this outcome. Start from that integrated checkpoint and preserve its proof. No parallel overlapping implementation or Cargo-backed verification.

All source-fidelity, SQL trust, fresh-only, no-deployment/no-push/no-main-merge, no-compatibility and no-gate-weakening prohibitions in the shared contract apply. Paths/symbols are inspected guidance, not a private implementation allowlist. Caller/test/fixture/export/generated/doc closure is part of this task, not work postponed to TASK-006.

### Source-backed reuse

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification | Proof |
| --- | --- | --- | --- | --- | --- | --- |
| REQ-010 / AC-010 — Shared outbox | M ScribeOutbox/ScribeSink/ScribeBatch; AuditStage; AppState | P11/P12/P13; audit/capture/results | S restores independent old publishers/staging | Retain M one process outbox; change row/frame projection | Existing outbox already owns batching/retry/loss | S10/S11 |
| REQ-012 / AC-012 — Trusted internal writes | M ScribeCapturePeerGrpc::batch; ScribeIngressFrame attribution | P13; six destinations and public Gate refusals | S older two-destination path loses result/audit attribution | Retain six-destination mTLS and exact trusted UID scope | Existing private route; no minting publisher | S10/S11 |
| REQ-013 / AC-013 — UID observation scope | M CardAttributionRegistry::attributed_cards; AuthContext; S decode/conform | P14/P19/P23; OTLP/native/client correlation | S older CardRef correlation drops unbound UID authorization | Port M trusted UID resolution into S admission/decode | Existing registry/signed scope; no new lookup/cache in CPU decoder | S12 |
| REQ-014 / AC-014 — Observation activation | M ObservationRunOutbox/ObservationRunSink; enqueue_observation_batch | P15/P16; ACK decoder created_at and writer scope | S best-effort task and subject-only activation regress M | Retain M retry/bulk/writer/replay; adapt ACK fields | Existing outbox/SQL; no semaphore task replacement | S13 |
| REQ-015 / AC-015 — Verification runtime | M VerifierRunQueue::renew/tick_schedule; EvalEngine::execute; writer_caller; gateway caller | P15/P16; runner/cache/leases, direct routes | S removes renew/hydration/delay and direct-calls providers | Retain M runtime; port only S read/type projections | Existing verification owner; no second runner/provider path | S14 |
| REQ-016 / AC-016 — Result evidence | M ResultPayloadBuilder::build/summary; ResultRun; S typed result schemas | P15/P17; direct Task/continuous Eval/Drift results | S typed Drift/Eval summaries omit M Task and app Run meaning | S typed structs + existing Task AssertionResult as assertion_result Variant | Existing AssertionResult; no new Task type/table | S15 |
| REQ-017 / AC-017 — Declared time axes | S TimeAxis::of_wire; canonical tables; event_time bound conversion; M BifrostReader | P01/P15/P17/P23; all ten table producers/readers | M event-time predicates/types and same-day joins obsolete | Use spec matrix, source precision/conformance; result_id joins | Existing table owners/type mapper; no synthetic time alias | S01/S13/S15/S16 |
| REQ-023 / AC-023 — Product lifecycle | M WorkflowRuns::drain; RunTools; AppState; BoundServer | P09/P20/P21/P24; workflow/MCP/gateway/Operator routers | Whole-source state selection can disconnect M-only owners | Retain connected M owners and ordered drain using S Bifrost | Existing trackers/owners; no general orchestrator | S19/S21 |
| REQ-027 / AC-027 — Proof/artifact closure | M consolidated targets/fixture stories/CI selection; source Variant Arrow fixtures | P01-P31; static I27; served OpenAPI/runtime MCP | Source tests/stubs/lanes may disappear or omit new cases | Rehome coverage, regenerate source-owned outputs, docs agree | Reuse M harness; no extra checker/duplicate TS server | All scenarios/I27 |

## Approved Requirement Detail

The following requirement bodies are copied verbatim from revision 2 for packet-local implementation. Primary requirements are fully owned here; supporting requirements contribute only the scope above and remain mapped to their closure owner. Do not reinterpret their full acceptance obligations.

### REQ-010 — One shared non-blocking Scribe outbox

Retain main's process outbox for authorization audit, gateway captures, and
Verifier results. Use destination-grouped bounded Arrow frames and
content-derived batch IDs; retry identical slices, consume/count terminal
rejections, and count observable shutdown loss. Permission completes before
operation/refusal and audit delivery never delays or fails it. No separate
publisher, SQL staging, relay, audit WAL, or public AuditUnavailable error.

### REQ-012 — Six-destination trusted internal writes

Retain main's private mTLS audit/two-capture/three-result write path. Only result
destinations carry exact Verifier attribution; the reserved system owner is
admitted only for audit. Public writes to gateway calls and result tables remain
refused for every principal. Preserve validated attributed UIDs in source
ingress without trusting client-supplied internal scopes.

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

### REQ-023 — Retained product owners and ordered shutdown

Keep main Workflow/MCP/gateway/Operator/Trigger/verification/identity/principal
owners, routers, configuration, and tracking connected in state and boot. Drain
accepted Workflows while their dependencies remain available; query tools settle
before run terminal state. Account for tracked calls, drain shared Scribe outbox
while Bifrost accepts, then settle roles/storage and observation queue within
the existing process deadline. Preserve errors and lifecycle settlement rather
than merely restoring a shutdown call.

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

1. Preserve main ACK-independent retry/bulk writer-aware enqueue, replay fence and lock order, using admitted original created_at rather than receipt time.
2. Retain main queue hydration, parsed/baseline caches, PostgreSQL leases/schedule clock, cancellation and direct invoke permission boundaries.
3. Route judging through the governed gateway with the correct writer/judging authority and preserve application Run lineage.
4. Compose source nullable typed Drift/Eval summaries and existing Task assertion_result Variant with result/outbox producers/readers and result_id joins.
5. Prove outage/next-day replay, denied/allowed judge calls, all terminal implementation payloads and split-day summary/details; rerun UID/audit/durable ACK regressions.

## Ordered Implementation Scenarios

Proof class: mixed. Changed executable seams require actual Behavior/RED/GREEN/REFACTOR; unchanged source behavior may use documented verification-only/no-op proof. Static/generated/configuration work does not require manufactured RED. Original scenario blocks below are retained verbatim, including mapped obligations, expected failures, required extensions and earlier-regression references. Their order is task-local; the shared contract explains cross-task regression references.

### S13 — ACK-derived Eval activation survives outage and next-day replay

**Behavior.** Observation ACK remains independent of enqueue availability, queue retries retain work, matching writer bindings activate once, and later-day replay uses original created_at to read the same subject record. Mapped obligations: REQ-014 / AC-014, REQ-017 / AC-017.

**RED.** Run P15/P16 and ACK decoder in P12. Extend existing replay/outage assertions with source UID/Variant created_at and different receipt day. Expected RED is all-subject activation, lost enqueue, duplicate run, or record lookup under the receipt day.

**GREEN.** Retain main outbox/bulk writer-aware queue and replay fence; adapt admitted ACK fields/time. Rerun S04/S12 with changed frame identities.

**REFACTOR.** No best-effort semaphore task replacement or per-binding SQL loop; preserve lock order and batching.

### S14 — Verification keeps leases, schedule clock and gateway authority

**Behavior.** The database owns claim/renew/schedule timing; expired/stale leases cannot revive and cancel their work; hydrated/cache baselines remain correct. Judge calls pass through gateway under the right authority and denial sends no provider request. Mapped obligations: REQ-015 / AC-015.

**RED.** Run P15/P16. Extend existing terminal/direct-judge matrix with explicit denied/allowed gateway assertions and retained writer/Run fields. Expected RED is direct provider bypass, missing renewal/cancel, missed delayed occurrence, or wrong attribution.

**GREEN.** Keep main queue/runtime/authority/cache/leases and gateway invocation; port source Variant/timestamp read decoding only. Rerun S13 and S11 decision/lineage proof.

**REFACTOR.** Keep existing verification owners; no duplicate runner, minted SYSTEM reader token or repeated per-run baseline lookup.

### S15 — Every implementation retains its result and application lineage

**Behavior.** Drift/Eval use typed source summaries, Task uses existing AssertionResult in nullable assertion_result Variant, and exactly the matching implementation payload is populated. Application run_id stays distinct from verifier execution identity; detail joins survive a different ingestion day. Mapped obligations: REQ-016 / AC-016, REQ-017 / AC-017.

**RED.** Run P17 and result/Task selectors in P15. Extend existing result-layout journey for ended_at versus detail ingestion midnight. Expected RED is lost Task payload/fields, text decode, duplicated summaries, verifier UUID as app Run, or pruned detail rows.

**GREEN.** Compose main result/outbox and source schema/projectors; use result_id joins and source nullable semantics. Rerun S11/S14 under combined schemas.

**REFACTOR.** Use existing AssertionResult and result contracts; no new Task struct/table or synthetic shared event time.

## Acceptance Criteria

- **TASK-004-C01.** ACK never waits for derived enqueue; outage/retry retains work; writer-matched bindings activate once on original created_at even when replay receipt occurs another day.
- **TASK-004-C02.** Main lease/cache/baseline/schedule/hydration behavior survives; stale/expired leases cannot revive and cancellation is owned.
- **TASK-004-C03.** Denied judging performs zero provider requests; allowed calls use governed gateway under exact authority, not direct ProviderRegistry.
- **TASK-004-C04.** Drift/Eval typed summaries and Task-only nullable AssertionResult Variant retain complete payload semantics; app run_id differs from Verifier execution UUID.
- **TASK-004-C05.** Summary/detail joins work across midnight ingestion using result_id; delivery remains shared-outbox/non-blocking and settlement retains separate exact result/Verifier identities.
- **TASK-004-EVIDENCE.** Every assigned scenario records required assertions and actual exact-command RED/GREEN or justified preserved/no-op results, plus affected regressions; each relevant REQ/AC has source/diff evidence. Update per-path and historical dispositions for this write set and preserve all invariants.
- **TASK-004-REVIEW.** Fresh independent task review evaluates the immutable cumulative task, its necessary consumer closure and source-authority exceptions before accepting the checkpoint. Missing proof and unexplained source divergence are not PASS.

## Expected Write Set and Consumer Closure

verification observations/outbox/ACK decoding; wyrd-sql verifier queues and binding/writer identity; verification runner/cache/leases/scheduler/authority/eval/drift/gateway callers; result builder and Vala typed summaries/detail schema/reader/query windows; real Eval/Task/Drift/direct-judge/lease/schedule/result-layout tests, affected generated/docs surfaces.

Trace whole workflows through callers, producers/readers, schemas, exports, fixtures, generated outputs, tests, command registration and owning docs. Tests requiring Python/Node lifetimes run in those runtimes. Rehome source assertions by body and critical behavior rather than function-name matching; keep current main consolidated targets. Update documentation and generators with actual contract changes in this task.

## Verification and Evidence

Read the shared recipe/execution contract. Selected historical recipe groups: P12, P13, P15, P16, P17, P18. Run only exact selectors supporting the active scenario plus changed earlier regressions; multi-selector recipes may be narrowed without dropping required assertions. Correct relocated names/features/setup through actual discovery and record non-weaker proof. New scenarios extend listed owners; exact new selectors are chosen/discovered and executed before GREEN, rather than predetermined fictitious names.

Affected Rust/static lanes: `mise run fmt:check`, `mise run lints`, `mise run check:deps`, `mise run check:tenant-isolation`, `mise run codegen:check`; `git diff --check`. Apply Python format/lint/typecheck and TS format/lint/typecheck/native-declaration checks only when those surfaces change; use owning generators and `codegen:check` for changed contracts. Run docs-site checks when docs-site content changes. Add only a narrow affected owner lane whose property is not proven by exact selectors, recording why. No task-local full language/journey suite or broad aggregate.

Record cross-task regression triggers now; later modifications must rerun affected accepted proof. Task-local acceptance never waives final specification-owned gate/geometry/parity.

### Non-TDD proof

Relevant I18/I24/I26/I27 contributions (removal/history/graph/registration/artifact/docs metadata) use static or regression proof; changed runtime scenarios still require the complete TDD cycle.

### Exact Selected Recipes

#### P12 — Native UID/ACK/audit projection and actual Gate adapter

```bash
WYRD_LOG=info mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=tables::audit::projection::tests::projects_canonical_content_with_the_decision_time) | test(=tables::eval::observations::tests::acknowledged_keys_mirror_scribe_stamping) | test(=scribe::execution_lanes::tests::card_scope_rejects_before_writer_admission) | test(=scribe::execution_lanes::tests::optional_and_scoped_card_correlations_stamp_signed_uids)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=gate::tests::gate_enforces_bifrost_record_write)'
```

#### P13 — Real retained audit and six-destination private validator

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=audit_retention::retained_history_carries_both_credential_shapes) | test(=audit_retention::system_owner_security_rejections_retain_once)"'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=grpc::capture_peer::tests::requests_are_confined_to_tenant_capture_destinations)'
```

#### P15 — Evaluation/Task/trace/result layout real journeys

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_LOG=info,vala_bifrost_redux=debug mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=eval_verification::eval_runs_follow_the_writing_owner) | test(=eval_verification::integrated_enqueue_outage_preserves_ack_and_recovers) | test(=eval_verification::sealed_replay_on_a_later_day_activates_once) | test(=eval_verification::continuous_eval_runs_the_terminal_matrix) | test(=eval_verification::direct_task_judgment_records_one_correlated_result) | test(=eval_verification::continuous_eval_reads_ordered_bounded_trace_evidence) | test(=eval_verification::continuous_eval_refuses_a_trace_over_the_span_ceiling) | test(=verification_runtime::result_layout_partitions_blooms_and_prunes_by_result)"'
```

#### P16 — Verification queue, schedule, lease and judging dependency proof

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_LOG=info,vala_bifrost_redux=debug mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_verification_runtime::observation_outbox_retains_through_an_outage_and_flushes_at_shutdown) | test(=pg_verification_runtime::schedulers_create_one_run_per_occurrence_across_ticks_and_restart) | test(=pg_verification_runtime::renewal_keeps_a_long_run_and_a_taken_token_cancels_it) | test(=pg_verification_routes::direct_execution_calls_the_judge_provider)"'

scripts/postgres/with-test-postgres.sh -- bash -lc 'WYRD_LOG=info mise exec -- cargo nextest run --locked -p wyrd-sql --test integration --test-threads=1 -E "test(=pg_verifier_runs::observation_runs_follow_the_writer) | test(=pg_verifier_runs::concurrent_schedulers_create_one_run_per_occurrence) | test(=pg_verifier_runs::scheduler_skips_inactive_unready_and_missed_occurrences) | test(=pg_verifier_runs::leases_renew_once_a_third_has_passed_and_never_revive) | test(=pg_verifier_runs::reclaimed_lease_fences_the_stale_token)"'
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
