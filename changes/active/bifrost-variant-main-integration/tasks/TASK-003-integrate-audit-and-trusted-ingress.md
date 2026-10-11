---
id: TASK-003
kind: implementation
status: proposed
spec: SPEC-bifrost-variant-main-integration
spec_revision: 2
requirements: [REQ-004, REQ-008, REQ-009, REQ-010, REQ-011, REQ-012, REQ-013, REQ-017, REQ-022, REQ-023, REQ-027]
primary_requirements: [REQ-010, REQ-011, REQ-012, REQ-013]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008, INV-009, INV-010, INV-011, INV-012]
acceptance: [AC-004, AC-008, AC-009, AC-010, AC-011, AC-012, AC-013, AC-017, AC-022, AC-023, AC-027]
scenarios: [S10, S11, S12]
depends_on: [TASK-002]
---

# Audit and ingress composition

## Outcome and Value

Authorization audit, gateway captures and Verifier result ingress use one non-blocking process outbox and the trusted six-destination peer path, with exact credentials, UID scopes and source time/Variant rows.

Primary closure ownership: REQ-010 / AC-010, REQ-011 / AC-011, REQ-012 / AC-012, REQ-013 / AC-013. Supporting clause contributions: REQ-004, REQ-008, REQ-009, REQ-017, REQ-022, REQ-023, REQ-027. All INV-001–INV-012 apply. The task-specific criteria below are scoped contributions; integrated AC closure also includes the contributing tasks in [decomposition](../decomposition.md).

This task requires [shared implementation contract](../implementation-contract.md), [approved spec revision 2](../spec.md), canonical evidence and reconciliation. Current user direction explicitly authorizes this six-task decomposition without changing the approved behaviors. Use `$wyrd-implement`; implementation self-report is not independent task completion.

This is task 3 of 6 for merging the bifrost variant worktree into the current main logic (this worktree)

## Owners, Scope, Consumers, and Prohibited Changes

Own the existing shared outbox projection/batching/retry/loss contract, actual Gate/Oracle credential builders, private destination validation and whole-frame UID admission. Preserve main verified auth/API-key-before-decode boundaries. Source and main tests expressing partial sibling acceptance for forbidden correlation must be adapted to the approved complete-frame refusal; inherited behavior is not permission to weaken REQ-013. Do not create an audit publisher, staging table, relay, audit WAL, chain, public internal scope or general API-key route.

Depends on accepted TASK-002 because its actual owners/contracts are prerequisites for this outcome. Start from that integrated checkpoint and preserve its proof. No parallel overlapping implementation or Cargo-backed verification.

All source-fidelity, SQL trust, fresh-only, no-deployment/no-push/no-main-merge, no-compatibility and no-gate-weakening prohibitions in the shared contract apply. Paths/symbols are inspected guidance, not a private implementation allowlist. Caller/test/fixture/export/generated/doc closure is part of this task, not work postponed to TASK-006.

### Source-backed reuse

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification | Proof |
| --- | --- | --- | --- | --- | --- | --- |
| REQ-004 / AC-004 — Scribe ACK/recovery | S shard group commit; NewestFence/FenceLookup; ScribePersistence recovery | P04/P22; SQL newest acknowledged fence | M lacks S compressed/group/fenced recovery changes | Import S WAL/replay/SQL as one durable protocol | Existing shard/recovery owners suffice | S04/S20 |
| REQ-008 / AC-008 — Membership lifetime | M ClusterTask; ClusterRegistry; BifrostComposition.membership_poller | P08/P09; Oracle/Scribe teardown and boot | S process token stops discovery before drain | Retain M final role tasks + one process poller | Existing concrete owners retain task lifetime | S08 |
| REQ-009 / AC-009 — Independent peers | M ScribeRoute::select/ScribePeers::channel; S ScribeFragmentExecutor | P10/P11; private grpc mounts and ingress | S capture requires local Oracle for remote Scribe TLS | Keep M routing + S assignment/fencing and limits | No additional peer service/transport | S09 |
| REQ-010 / AC-010 — Shared outbox | M ScribeOutbox/ScribeSink/ScribeBatch; AuditStage; AppState | P11/P12/P13; audit/capture/results | S restores independent old publishers/staging | Retain M one process outbox; change row/frame projection | Existing outbox already owns batching/retry/loss | S10/S11 |
| REQ-011 / AC-011 — Audit schema/attribution | M project_audit_event; GateAudit blanket impl; Oracle query_audit_event; S AuditLogTable types | P12/P13; actual keyed Gate/Oracle -> retained history | S brings chain/staging; M builders omit credential id | M event content + S created_at/Variant; carry credential | Existing event/sink/projection; no chain/hash authority | S11 |
| REQ-012 / AC-012 — Trusted internal writes | M ScribeCapturePeerGrpc::batch; ScribeIngressFrame attribution | P13; six destinations and public Gate refusals | S older two-destination path loses result/audit attribution | Retain six-destination mTLS and exact trusted UID scope | Existing private route; no minting publisher | S10/S11 |
| REQ-013 / AC-013 — UID observation scope | M CardAttributionRegistry::attributed_cards; AuthContext; S decode/conform | P14/P19/P23; OTLP/native/client correlation | S older CardRef correlation drops unbound UID authorization | Port M trusted UID resolution into S admission/decode | Existing registry/signed scope; no new lookup/cache in CPU decoder | S12 |
| REQ-017 / AC-017 — Declared time axes | S TimeAxis::of_wire; canonical tables; event_time bound conversion; M BifrostReader | P01/P15/P17/P23; all ten table producers/readers | M event-time predicates/types and same-day joins obsolete | Use spec matrix, source precision/conformance; result_id joins | Existing table owners/type mapper; no synthetic time alias | S01/S13/S15/S16 |
| REQ-022 / AC-022 — Identity/bootstrap | M ServerAuth/ExchangeApiKey/AuthContext::from_verified; DevServer; ResolvedDsns | P14/P24/P25/P26; issued credentials, OTLP, saved local config | S Gate/state/boot lack newer entrance/wiring | Retain M authorities/verified entrance/DSNs/endpoints | Existing auth/runtime/SQL owners; no raw pools or new credential model | S12/S19/S21 |
| REQ-023 / AC-023 — Product lifecycle | M WorkflowRuns::drain; RunTools; AppState; BoundServer | P09/P20/P21/P24; workflow/MCP/gateway/Operator routers | Whole-source state selection can disconnect M-only owners | Retain connected M owners and ordered drain using S Bifrost | Existing trackers/owners; no general orchestrator | S19/S21 |
| REQ-027 / AC-027 — Proof/artifact closure | M consolidated targets/fixture stories/CI selection; source Variant Arrow fixtures | P01-P31; static I27; served OpenAPI/runtime MCP | Source tests/stubs/lanes may disappear or omit new cases | Rehome coverage, regenerate source-owned outputs, docs agree | Reuse M harness; no extra checker/duplicate TS server | All scenarios/I27 |

## Approved Requirement Detail

The following requirement bodies are copied verbatim from revision 2 for packet-local implementation. Primary requirements are fully owned here; supporting requirements contribute only the scope above and remain mapped to their closure owner. Do not reinterpret their full acceptance obligations.

### REQ-004 — Source Scribe durability and recovery

Keep group-fsynced acknowledgements, compressed IPC, durable batch fences,
fenced final-record corruption handling, and publication-aware recovery. No ACK
precedes its durable authority, replay does not duplicate writes, corruption
covered by an acknowledged fence fails closed, and published recovery does not
reupload objects. Wire, WAL, ingress, SQL, and crash proof must agree.

### REQ-008 — Role heartbeat and one membership poller

Retain main's final role-owned ClusterTask lifetime and one process membership
poller. Readiness withdrawal never re-advertises readiness, but heartbeat and
discovery continue while outbox/role drain needs them. Teardown joins/stops
them; failed construction and abort terminate their tasks. Do not restore
source's premature process-token cancellation or per-role poller duplication.

### REQ-009 — Scribe capability and routing independent of Oracle

Preserve Scribe-only fragment execution, role-specific readiness/fences,
tenant/body authority, and source assignment shape. Internal writes choose
local or ready remote Scribes from boot-resolved TLS and membership without
requiring a local/ready Oracle. Keep configured decode ceilings, per-peer
deadlines, retry rotation, and stale endpoint eviction. Do not invent a second
peer protocol solely to match remembered ScribePeer/OraclePeer names.

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

1. Preserve the main one-process outbox and audited permission boundary while updating source Arrow frame/time/Variant projections.
2. Carry actual credential_id and explicit decision created_at through Gate/Oracle builders and canonical retained audit content without seq/hash chaining.
3. Retain private mTLS audit/two-capture/three-result destination rules and exact trusted result UID attribution; prove public-write refusals.
4. Integrate signed bound scopes and unbound at-most-32 tenant-registry UID checks across native/OTLP/decode/ACK/private frames/client boundaries, refusing the entire frame on forbidden correlation.
5. Extend actual keyed allowed/denied audit, six-destination ACK/readback, trace/log/metric API-key and revoked-key journeys; rerun role-discovery drain and durable retry proofs.

## Ordered Implementation Scenarios

Proof class: mixed. Changed executable seams require actual Behavior/RED/GREEN/REFACTOR; unchanged source behavior may use documented verification-only/no-op proof. Static/generated/configuration work does not require manufactured RED. Original scenario blocks below are retained verbatim, including mapped obligations, expected failures, required extensions and earlier-regression references. Their order is task-local; the shared contract explains cross-task regression references.

### S10 — One outbox delivers bounded identical private writes

**Behavior.** Audit/capture/results share one process writer, bounded destination frames and stable retry IDs. Terminal refusal consumes/counts its slice and later work progresses; private destinations/Verifier/system-owner rules and all public result-write refusals remain enforced. Mapped obligations: REQ-010 / AC-010, REQ-012 / AC-012.

**RED.** Run P11 and private validator in P13. Extend existing private/public journey owner to observe ACK/readback of all destination categories under source schema. Expected RED is lost category, changed retry frame, forbidden public write, or permissive internal attribution.

**GREEN.** Use main shared outbox and six-destination mTLS ingress with source frame/projection. Rerun S04/S09 exact retry and routing cases.

**REFACTOR.** Delete old independent publishers/staging owners; keep existing sink and permission surfaces.

### S11 — Audit records real credentials and never changes authorization

**Behavior.** Allowed/denied keyed Gate and Oracle requests retain credential or null exactly, decision-created_at and logical Variant detail, with no sequence/hash columns; delivery failure never changes the request decision. Trusted system rejection records once. Mapped obligations: REQ-011 / AC-011, REQ-010 / AC-010, REQ-012 / AC-012.

**RED.** Run P12/P13. Extend existing retention journey to originate actual credential-bearing ingress/query decisions rather than only stage fabricated events, and extend production blanket adapter proof. Expected RED includes current-main omitted credential_id, text/time mismatch, restored chain, or audit IO affecting authorization.

**GREEN.** Keep main event/stage/outbox authority, source explicit time/detail representation, and carry credential through actual builders. Rerun S08/S10 retention and private destination proof.

**REFACTOR.** Remove old hash/staging helpers and AuditUnavailable cases; do not change canonical CardRef content or batch dedup hashing.

### S12 — UID and API-key scope are preserved at native and OTLP entrances

**Behavior.** Bound signed scope and unbound tenant registry admit valid optional subject UIDs; missing correlation stays null; forbidden/malformed/wrong-kind/foreign UID rejects the entire frame. Stock API-key OTLP authenticates before decode and works across token TTL without becoming a global API-key route. Mapped obligations: REQ-013 / AC-013, REQ-022 / AC-022.

**RED.** Run P14 and native scope selectors in P12; extend the API-key journey to trace/log/metric entrances and revoked/unknown key refusals where those branches changed. Expected RED is CardRef-only behavior, unbound denial/allow-all, attribution drift, or missing verified Gate entrance.

**GREEN.** Port main authorization/registry/verified entrance into source admit/conform/ACK and SDK/OTLP input. Rerun S01/S10/S11 for scope and retained attribution.

**REFACTOR.** Keep authentication and registry IO at the trusted boundary; no lookup in CPU decode or global CardRef rename.

## Acceptance Criteria

- **TASK-003-C01.** One bounded destination-grouped outbox retries identical content-derived batch IDs, progresses after terminal refusals and counts observable shutdown loss; audit delivery never changes authorization.
- **TASK-003-C02.** Real keyed/null Gate and Oracle decisions retain all 12 content fields, decision time and logical Variant detail through production adapters.
- **TASK-003-C03.** All six trusted destination categories are ACKed/read back with their exact system-owner/Verifier scopes; public gateway/result writes always refuse.
- **TASK-003-C04.** Native and OTLP valid optional UIDs are admitted/null as specified; malformed/missing/foreign/wrong-kind/forbidden IDs reject the whole frame before persistence, with no valid sibling prefix accepted.
- **TASK-003-C05.** API-key trace/log/metric requests authenticate before decode, work across token TTL and refuse unknown/revoked keys; other routes never gain API-key authentication.
- **TASK-003-EVIDENCE.** Every assigned scenario records required assertions and actual exact-command RED/GREEN or justified preserved/no-op results, plus affected regressions; each relevant REQ/AC has source/diff evidence. Update per-path and historical dispositions for this write set and preserve all invariants.
- **TASK-003-REVIEW.** Fresh independent task review evaluates the immutable cumulative task, its necessary consumer closure and source-authority exceptions before accepting the checkpoint. Missing proof and unexplained source divergence are not PASS.

## Expected Write Set and Consumer Closure

ScribeOutbox/sink/batch and AuditStage; Gate/Oracle decision builders; audit table/projection; capture_peer/private grpc; verified auth and CardRegistry boundary; ingest conform/decode/frame and ACK correlation; immediately affected SDK encodings, real audit/OTLP/native/private destination tests, schemas and docs.

Trace whole workflows through callers, producers/readers, schemas, exports, fixtures, generated outputs, tests, command registration and owning docs. Tests requiring Python/Node lifetimes run in those runtimes. Rehome source assertions by body and critical behavior rather than function-name matching; keep current main consolidated targets. Update documentation and generators with actual contract changes in this task.

## Verification and Evidence

Read the shared recipe/execution contract. Selected historical recipe groups: P09, P10, P11, P12, P13, P14, P19. Run only exact selectors supporting the active scenario plus changed earlier regressions; multi-selector recipes may be narrowed without dropping required assertions. Correct relocated names/features/setup through actual discovery and record non-weaker proof. New scenarios extend listed owners; exact new selectors are chosen/discovered and executed before GREEN, rather than predetermined fictitious names.

Affected Rust/static lanes: `mise run fmt:check`, `mise run lints`, `mise run check:deps`, `mise run check:tenant-isolation`, `mise run codegen:check`; `git diff --check`. Apply Python format/lint/typecheck and TS format/lint/typecheck/native-declaration checks only when those surfaces change; use owning generators and `codegen:check` for changed contracts. Run docs-site checks when docs-site content changes. Add only a narrow affected owner lane whose property is not proven by exact selectors, recording why. No task-local full language/journey suite or broad aggregate.

Record cross-task regression triggers now; later modifications must rerun affected accepted proof. Task-local acceptance never waives final specification-owned gate/geometry/parity.

### Non-TDD proof

Relevant I18/I24/I26/I27 contributions (removal/history/graph/registration/artifact/docs metadata) use static or regression proof; changed runtime scenarios still require the complete TDD cycle.

### Exact Selected Recipes

#### P09 — Real membership lifetime and audit drain

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=owner_inspection::unready_roles_heartbeat_until_teardown_ends_membership_tasks) | test(=query::oracle_only_pod_retains_audit_staged_before_its_drain)"'
```

#### P10 — SQL role fences and private peer trust

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p vala-sql --test integration -E "test(=pg_oracle_membership::same_node_roles_have_independent_fences) | test(=pg_oracle_membership::stale_role_mutation_is_rejected) | test(=pg_oracle_membership::live_discovery_excludes_expired_heartbeat)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=peer_network::listener::peer_listener_is_isolated_mtls_and_role_complete) | test(=peer_network::security::peer_context_refusals) | test(=peer_network::transport::peer_transport_uses_immutable_fenced_destinations)"'
```

#### P11 — Shared outbox no-Oracle routing, bounded retry and refusal

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib --features test-support -E 'test(=scribe_outbox::tests::a_peer_route_needs_no_oracle) | test(=scribe_outbox::tests::a_retry_resubmits_identical_frames) | test(=scribe_outbox::tests::a_terminal_rejection_never_blocks_later_writes) | test(=scribe_outbox::tests::an_oversized_detail_reaches_scribe_in_bounded_frames_that_replay_identically) | test(=scribe_outbox::tests::an_unsplittable_row_is_submitted_whole_and_terminal_refusal_consumes_it) | test(=scribe_outbox::tests::a_silent_peer_times_out_and_the_slice_retries_on_the_next_scribe)'
```

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

#### P14 — OTLP API-key and UID runtime entrances

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey --run-ignored=all -E "test(=trace_export::pg_tests::stock_exporter_authenticates_with_api_key_after_token_lifetime)"'

scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_LOG=info,vala_bifrost_redux=debug mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey --run-ignored=all -E "test(=negative::pg_tests::unbound_writer_attributes_spans_to_registered_cards_only) | test(=negative::pg_tests::identical_exports_from_two_principals_each_keep_their_own_attribution)"'
```

#### P19 — Focused first-class Rust query/observation/verification stories

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E "test(=query_bifrost::parameterized_sql_returns_the_callers_rows) | test(=query_bifrost::bound_sql_text_is_treated_as_data) | test(=query_bifrost::stream_yields_arrow_batches_and_a_terminal) | test(=query_bifrost::caller_without_bifrost_read_is_refused)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E "test(=observe_a_run::run_observations_read_back_by_run_id) | test(=observe_a_run::card_scoped_key_cannot_write_another_cards_observations) | test(=observe_a_run::unsealable_byte_budget_is_refused_at_connect)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E "test(=scheduled_drift_alerts_operator::failed_schedule_alerts_its_operator_on_the_connection_origin) | test(=verify_in_real_time::exact_verifier_grant_verifies_only_that_verifier) | test(=verify_in_real_time::another_tenant_cannot_verify_the_assistant)"'

mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=bifrost::query::tests::sql_forwards_bind_values_without_interpolation)'
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
