# TASK-005 independent finding validation

Result: **SPEC_REVISION_REQUIRED**. The independently validated ledger contains nine findings. FIND-TASK-005-6 requires an approved public-API decision; the other eight have bounded corrections within approved behavior. No implementation was changed.

## Subject, authority, and method

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verifier-building-blocks`.
- Base exclusive: `f2a726c90edbf8956c7b91c843379175ee20eac0`.
- Candidate: `99882cf824e70a601033aba4cfa5c5b27a8dfc7c` (HEAD verified unchanged).
- Approved authority: `changes/active/verifier-building-blocks/spec.md` r8, original `tasks/TASK-005-server-integration.md`, AGENTS.md, agent rules, applicable Wyrd/Bifrost/security design, reference router, spec-driven-development and maintainer-style guides. Relevant evaluation, drift-monitoring, telemetry and analytical-reliability rules were checked against source. Older packet mechanics yield to r8 and current design.
- Reviewed the cumulative diff/inventory and independently inspected the source paths underlying every proposed finding and correction, including owners, siblings, producers, consumers and the relevant full operation bodies. CodeGraph was used before source navigation. Read both task reports, standards, maintainer, system, all four domain reports, focused follow-up, claim comparison, supplied verification evidence, external source identities, and owner clarification. Reviewer agreement was not used as proof.
- Owner clarification controls: **nothing has shipped; no migrations are needed**. No upgrade/catalog migration, deployed-history migration, or legacy pending-delivery compatibility is demanded. This does not waive current executions' retry/reclaim correctness.
- This is fresh independent validation, not implementation or a rerun of broad checks. No prior TASK-005 FIND IDs existed; IDs below are assigned once to deduplicated causes.

## Disposition of every discovery proposal

| Discovery proposal(s) | Validation | Final identity / reason |
|---|---|---|
| BEH-01, I-01, SEC-01 | REVISED | FIND-TASK-005-1. Missing scope decision confirmed; reuse signed UID admission before staging. Reject unnecessary registry read or moving authorization/audit into asynchronous SQL acceptance. |
| BEH-02, I-02, SYS-002, CONC-001 | REVISED | FIND-TASK-005-2. One runtime-admission invariant includes Sandbox error erasure and workspace waiters. Preserve all real Python entry paths; no blanket execution permit or generic retry promotion. |
| I-03 | CONFIRMED | FIND-TASK-005-3. Optional omitted scheduled slot is reachable; evidence-domain happy-path PASS does not cover it. |
| I-05, SEC-02 | REVISED | FIND-TASK-005-4. Existing operation-level grant check supplies the minimal correction without a new evidence request field. |
| SYS-001 | CONFIRMED | FIND-TASK-005-5. Reclaim after staged-result/failed-commit aliases distinct attempts to one result ID. |
| BEH-03, I-04, SEC-03 | CONFIRMED | FIND-TASK-005-6. Authenticated externally callable upload route is a new public API; preserving File support prevents treating deletion alone as closure. |
| REP-001, MNT-001 | REVISED | FIND-TASK-005-7. Concrete changed-item rustdoc omissions are combined into one correction; no unrelated documentation sweep. |
| REP-002 | CONFIRMED | FIND-TASK-005-8. Current normative contracts disagree with approved current schemas, telemetry and activation. Update prose; do not restore retired machinery. |
| MNT-002 | CONFIRMED | FIND-TASK-005-9. Explicit deletion-only unearned-code criterion applies to copied trace-wait API whose only consumer is its own test. |
| DATA-001 | REJECTED | Assumes shipped/provisioned prior-version deployments. Owner explicitly excludes that migration/upgrade obligation. Do not add evolution machinery, compatibility aliases, or migration proof. |
| BEH-04 | REJECTED | Does not establish a current r8 acceptance failure. The cited older REQ-171/AC-040 workload depends on direct drift and judge APIs explicitly replaced/removed here. Current benchmark description, fixture and report honestly advertise assertion direct/queued traffic plus sample ingestion/query load. Current r8 requires statistical functional/journey proof, which is separately present; it does not require selecting a new scheduled statistical capacity qualification. Requiring a replacement mix/SLO would invent adjacent scope. Do not restore removed APIs or prescribe a benchmark redesign. |

Additional open concerns: equal-millisecond spans have the declared reproducible `(start_time, span_id)` order; no approved causal parent-first tie-break contract was established, so no finding is invented. The unrelated intermittent Oracle failure is not established as a candidate regression and was not diagnosed or fixed here. External uncommitted build outputs and unavailable live provider credentials remain evidence limits, not invented shipped-artifact or migration requirements.

## Independently validated ledger

### FIND-TASK-005-1 — Queued submission loses signed subject scope

**Status:** REVISED. **Sources:** BEH-01, I-01, SEC-01; focused follow-up. **Classification:** VIOLATION / REGRESSION. **Obligation:** INV-003/004, original manual admission boundary, TASK-005 accepted-authority scenarios.

**Locations and proof:** `components/verification/service.rs:553-615::enqueue` checks only exact Verifier invocation at :569, captures AcceptedAuthority and stages the requested subject. `wyrd-sql/src/queries/verifier_executions.rs:1105-1172::accept` only resolves target/readiness/replay; `verification/submissions.rs::write` commits it. `runner.rs:383-415` restores accepted authority and attributes the engine/result to `run.card_uid`. An assert-only JSON Verifier reaches this sink without gateway/tool reads. Thus a bound Service/Agent with V's exact run grant can produce retained judgment for unrelated same-tenant Card S.

Authority production was independently traced through `wyrd-auth/src/card_scope.rs:53-109` (registry-resolved member UIDs), auth issuance, `wyrd-auth-issue/src/lib.rs:277-310`, `wyrd-spec/src/reference.rs` scope/root merge and CardRef text serialization/parser, verified principal `seed_scope`, and `wyrd-runtime/src/request_context.rs:85-105`. UID correlations survive signed token and accepted-authority serialization. `Principal::card_attribution` and `CardRefScope::uids` define this exact invariant. `components/gateway/invocation.rs:773-790::attribute` already compares the requested UID against that signed mapping. Effective delegated subject remains authority; actor history does not expand it. Base start_run enforced scope before staging; its deletion creates the regression.

**Smallest correction:** restore the scope decision once in shared manual `VerificationControl::enqueue`, before input staging, using `CardAttribution::Scoped` and signed `scope.uids()` membership; preserve AnyRegistered callers. Use existing denied RBAC response and nonblocking audit stage so an out-of-scope refusal is accurately audited. No registry read, new scope resolver, asynchronous authorization status, request PG write, or consumer guard is necessary. Keep exact V invocation, accepted authority, async availability/replay/conflict decisions, pre-existing direct exception, and scheduled binding-owner rule.

**Focused closure:** existing authenticated real route/SDK fixture, bound caller with exact V grant, in-scope and unrelated active same-tenant subjects; unrelated subject is synchronously denied/audited with no execution input or queue work; scoped success and unbound success persist. Include delegated effective-subject narrowing. Exercise shared HTTP/MCP control projection at the narrow existing route boundary; no new harness or full journey aggregate.

### FIND-TASK-005-2 — Runtime capacity becomes terminal evidence and workspace waiters are unbounded

**Status:** REVISED. **Sources:** BEH-02, I-02, SYS-002, CONC-001. **Classification:** INCORRECT. **Obligation:** Deployment limits, INV-007, TASK-005 Scenario 6, Wyrd design runner resource-refusal contract.

**Locations and proof:** `runner.rs::new` composes `ClaimLoop` without a blanket execution permit, and `claims.rs` continues claim rounds/spawns. That is valid only with resource refusal returning to its existing queue. `hosted/sandbox.rs:153-224::exec` admits at most workers + configured waiters, then returns ordinary BlockError::Retryable when full or when checkout cannot obtain a worker by the deadline. `vala-eval/src/blocks/engine.rs:318-369::attempt` spends authored action retries and turns final error into failed step; :374-414::judge turns every code/Agent check error into inconclusive. `runner.rs:410-415` sees Ok(outcome) and completes/stages it. `hosted/tools.rs:280-303::Invocation::run` additionally stringifies Sandbox errors into ToolError; Agent code tools therefore lose the category even earlier. `hosted/workspace.rs:60-100::open` awaits an owned semaphore without bounded waiter admission, reached by `HostedTools::open` for filesystem/code tools. The unbounded claim loop can accumulate these futures until execution deadlines instead of retaining that pressure in the queue.

**Consequence:** ordinary transient process contention can consume valid queued work as completed inconclusive/failed-preparation or timed-out work. Releasing capacity cannot resume it. Existing scripted Deferred tests bypass the real broken mapping; standalone sandbox-cap tests stop before queue integration.

**Smallest correction:** retain a distinguishable resource-admission refusal at existing Sandbox/Workspaces owners and preserve it through all hosted consumers (code action/check and enabled Agent code.run) to queued execution handling. Route it through existing `EngineOutcome::Deferred`, `Transition::Defer`, fenced `VerifierExecutionQueue::release` with refunded attempt, and ClaimLoop pressure handling. Bound workspace waiting at its existing owner; no new queue/scheduler or whole-execution Python worker reservation. Do not promote all BlockError::Retryable/provider failures to queue admission or spend authored retries/Python budgets on process-capacity refusal. Direct in-process execution retains approved bounded behavior and check-local genuine infrastructure failures; preserve cancellation, deadlines, leases, shutdown, accepted authority, and non-Python/sibling progress. This implements already-approved capacity semantics, not a new concurrency policy.

**Focused closure:** tiny real configured pool, deliberately held operations rather than host stress. Submit another code-check execution through the real hosted runner, prove no terminal inconclusive result/attempt consumption while refused, then release and prove the same execution completes with actual code outcome. Narrow boundary coverage for code action and enabled Agent code.run propagation, workspace admission bound/cancellation, non-Python progress, and direct behavior. Scripted EngineOutcome::Deferred alone is insufficient; no full gate or full journey suites for bounded remediation.

### FIND-TASK-005-3 — Omitted optional scheduled slot is inferred from stored rows

**Status:** CONFIRMED. **Source:** I-03. **Classification:** INCORRECT. **Obligation:** REQ-001 optional null semantics, REQ-014 explicit sources, INV-006, AC-007.

**Locations and proof:** `wyrd-spec/src/card/verifier/validate.rs:878-911::validate_schedule` requires mappings only for required slots and validates selector sets only when supplied. A samples slot with `required: false` can legitimately be omitted while another required slot is mapped. SQL claim returns binding selectors; `ClaimedRow::into_claimed:1981-2000` puts them in ExecutionInput::Scheduled.slots. `runner.rs:363-369` discards that map. `drift.rs:686-702::checks` iterates every drift check and scores its configured metric/features for the subject/window, irrespective of whether check.on's slot was mapped. `blocks/engine.rs:544::check_bindings` deliberately bypasses normal slot binding for drift. `blocks/drift.rs:76-90::judge` takes a pre-scored report before its missing-samples branch. Retained rows for the omitted metric can therefore produce real pass/fail evidence.

**Correction:** in existing DriftEngine report production, use the claimed explicit scheduled slots and emit a scored report only for a check whose declared on slot is supplied. For an omitted slot, leave no pre-scored report so existing DriftChecks missing-evidence semantics produce inconclusive. Do not change registration to require optional slots, infer mappings, synthesize success, add a loader/source registry, or change statistical algorithms/baseline identity. The all-mapped path is already sound.

**Focused closure:** scheduled Verifier with mapped required slot and omitted optional samples slot, retaining rows for both names; omitted requirement stays inconclusive, has no fabricated scored detail, and cannot alone trigger a failed Operator. Explicitly mapping that slot enables its actual existing statistical judgment. Use existing drift/runtime integration fixture and narrow lane.

### FIND-TASK-005-4 — Exact Verifier grant cannot stage File evidence

**Status:** REVISED. **Sources:** I-05, SEC-02. **Classification:** INCORRECT. **Obligation:** REQ-010/011, INV-002/003; existing scoped verifier:run contract.

**Locations and proof:** shared `wyrd-client/src/verify/mod.rs::RunEvidence::pin` requests plans before execute/enqueue and uploads missing files via existing storage client. `VerificationControl::evidence:323-329` and `upload_local_evidence:403-414` request Permission::verifier_run_any (All), whereas execute/enqueue request exact UID. `PermissionSet::contains` uses covers, so exact V cannot cover All even for a deduplicated existing file because the check precedes storage lookup. All SDKs share this path. `mcp/verification.rs:137-142::may_run_verifiers` already admits an exact grant with `covers_operation(Resource::Verifier, Action::Run)`; the service then refuses its files.

**Correction:** reuse that existing operation-level grant mechanism for generic evidence planning/local evidence upload admission, recording its allow/deny through the existing audit stage. These staging operations select no Verifier and cannot invoke one; exact selected-Verifier authorization remains mandatory at execute/enqueue. Preserve tenant/digest/size/storage validation and result/Card read permissions. Do not add a verifier field to evidence requests or grant users All merely to stage bytes. This is coarse preparatory admission, not substituting coarse authorization for a selected object invocation; no new security model is required. Local endpoint retention remains separately subject to FIND-6 approval.

**Focused closure:** exact-V-only principal plus required ordinary read grants completes real local File verify and submit, including missing bytes; W still refused, no-Verifier-grant caller gets audited denial. Test remote-plan admission through existing backend fixtures without cloud credentials. Narrow route/shared-client proof; do not rerun every SDK journey for this shared correction.

### FIND-TASK-005-5 — Reclaimed attempts share a result ID and status can return an abandoned outcome

**Status:** CONFIRMED. **Source:** SYS-001. **Classification:** REGRESSION. **Obligation:** REQ-012 real result, REQ-013 fenced recovery/original identities, REQ-015 attributable evidence; TASK-005 Scenario 3.

**Locations and proof:** `results.rs:209-216::ResultPayloadBuilder::result_id` aliases result ID to execution UUID. `runner.rs:736-790::result` constructs a payload/Complete transition once; :438-509::settle stages that payload after fenced SQL complete but before commit. Its full body and documentation expressly permit staged result surviving a failed commit then lease loss. `verifier_executions.rs:221-259::CLAIM_SQL` reclaims that still-running execution under a fresh token; new attempt can produce a different stochastic model verdict (explicitly allowed). The second payload has the same result ID but different bytes/times. `ScribeSink::identify` hashes encoded rows, so it cannot deduplicate these distinct outcomes. SQL complete stores only that aliased result ID; `service.rs:251-296::read_result` selects WHERE result_id LIMIT 1, so A's abandoned result can satisfy B's completed handle. Drift detail uses the same ID and can join across attempts. This is a current fresh-install failure path, unrelated to upgrade history.

**Correction:** restore the existing base per-produced-payload VerificationResultId identity, minted once and retained across summary/features/payload and settlement retries. Carry it through the existing builder, queue completion, frozen Operator context and status lookup; execution_id remains the separate common execution correlation. A newly executed attempt gets a distinct result ID; transport/settlement retries of one payload keep its ID. Builder siblings are queued runner result, direct stage_direct, Scribe test support and payload tests; enqueue uses inputs only. Do not mint separately on every result_id accessor, sort queries to guess a winning row, add a result store/durable outbox, move the staging/commit boundary, or change approved loss guarantees. Existing queue already owns the selected result pointer; no persistent-schema or recovery-contract decision is needed.

**Focused closure:** force settlement rollback after A stages, reclaim under new lease, finish B with distinct verdict while retaining both payloads. Prove queue-selected ID and real execution read return B only, selected Operator context matches it, and feature projection joins do not combine attempts. Retried publication of A/B individually retains each payload identity. Existing fault-injection/runtime fixture and narrow owning SQL/runtime/result proof suffice.

### FIND-TASK-005-6 — Local evidence PUT exceeds approved public control boundary

**Status:** CONFIRMED. **Sources:** BEH-03, I-04, SEC-03. **Classification:** DRIFT / VIOLATION; **SPEC_REVISION_REQUIRED**. **Obligation:** TASK-005 material stop (no new public API), REQ-010/012.

**Locations and proof:** `routes.rs:38-49` conditionally mounts `local_evidence:153-200`, an authenticated externally callable `PUT /v1/verification/evidence/local/{sha256}`. Storage `service.rs:499-525::evidence_upload_plan` publishes it in UploadPlan::LocalFs; the shared storage client calls it. Its public digest/auth/error/backend contract is explicitly published in served OpenAPI. Conditional local mounting, reusing UploadPlan and no metadata row do not make a wire endpoint private. Existing `/cards/upload/local/{id}` consumes minted upload-session metadata/ID; it cannot fulfill metadata-free evidence namespace semantics by a simple private reuse.

**Required decision:** obtain approved specification revision for the local evidence upload/public boundary (including its authorization contract), or an owner-approved disposition using an already approved capability that truly fulfills REQ-010. Do not silently approve the route because tests pass, delete local File support, introduce a different endpoint/capability, or retrofit Card upload metadata. Necessity of local file transport exposes the material stop; it does not authorize resolving it in remediation. No decision about shipped migration is involved.

**Closure after approval:** served OpenAPI matches approved inventory and real local File upload/verify/submit preserves tenant, digest, size and permission rejection behavior. Remote single-PUT behavior remains intact. This is the only retained finding that requires new approved behavior rather than bounded implementation repair.

### FIND-TASK-005-7 — Changed Rust items miss mandatory substantive rustdoc

**Status:** REVISED. **Sources:** REP-001, MNT-001. **Classification:** VIOLATION. **Obligation:** AGENTS §16 / agent-rules hard gate, explicitly blocking under task-review skill.

Independently inspected changed bodies and diff membership. Concrete omissions are `wyrd-runtime/src/outbox.rs:107::Debug::fmt`; `verification/submissions.rs:59-61` associated Item/Error/NAME; Vala `tests/integration/orchestrator_parity.rs:52::drive`; `scenario.rs:452,494` changed scenario tests; `tasks/assertion.rs:297`, `tasks/agent.rs:148`, `tasks/trace.rs:279` changed executor factories; `executor.rs:939` changed full-spec test. New `scenario.rs:382::response_task`, `verification/engines.rs:102::tests::requirement`, and SQL `verifier_executions.rs:2197::tests::request` use fixture expect without required panic contract; request prose also incorrectly says fixed subject although it generates a UID per invocation. An impl/module comment is not item rustdoc.

**Correction:** document these changed items in place with role and actual fallible/panic conditions; no placeholder 'error on error' prose. Correct fresh-subject wording. Batch with bounded code corrections and preserve bodies/assertions/runtime behavior; no unrelated untouched-helper rewrite or style-only round. These are mandatory repository acceptance exceptions to the normal nonblocking style threshold.

**Closure:** source review against full item bodies plus formatting/owning documentation checks. No behavioral test or new documentation checker is needed. The existing gate does not statically prove private-item docs.

### FIND-TASK-005-8 — Normative descriptions retain retired schemas, labels and activation

**Status:** CONFIRMED. **Source:** REP-002. **Classification:** INCORRECT. **Obligation:** spec baseline/authority update clause, REQ-015/017, AC-009, AGENTS public-contract consistency.

**Locations and proof:** `architecture/bifrost-design.md:231-255` still promises top-level drift_report Struct and its nullability contract, absent from current ResultsTable's complete fields (`tables/verification/results.rs:51-69`); detailed drift is outcome Variant/requirement feature projection. Bifrost design :1006-1010 says execution_inputs partitions on wyrd_ingested_at, while its owner uses recorded_at for fields/layout (`execution_inputs.rs:62-67`), explicitly approved in REQ-015. Telemetry reference :65-77 retains kind labels; actual ExecutionTelemetry start/emit and app descriptors removed them. `wyrd-security-posture.md:125-130` retains observation-triggered activation and :208-209 old Drift/Eval paired read description; current boot/Gate/ACK composition and caller-authorized trace reads have replaced it.

**Correction:** update those existing authorities to the approved current schema/detail, recorded_at layout, actual bounded metric dimensions and schedule-or-submit acceptance/read authority. Do not restore old columns/kinds/activation or add compatibility. Keep existing nullable-Struct rules for actual remaining tables, accepted principal vs SYSTEM attribution, scheduled drift read boundary and loss semantics. The timestamp choice is already explicitly approved; no new layout or public decision is required.

**Closure:** compare prose against table fields/layout, telemetry emitters, and boot/ACK activation/read owners; run the existing documentation checks for touched prose. No journey expansion or migrations.

### FIND-TASK-005-9 — Copied trace-wait API has only its own test as caller

**Status:** CONFIRMED. **Source:** MNT-002. **Classification:** DRIFT (deletion-only). **Obligation:** INV-008 complete replacement, repository minimum earned machinery, task-review explicit allowance for deleting unearned code.

**Locations and proof:** SQL `verifier_executions.rs:375::AWAIT_TRACE_SQL`, :929::TraceWaitOutcome, :1644::await_trace implement an extra trace-specific refund/deadline/requeue lifecycle copied into the new queue. Full method reads timeout and invokes terminate when waiting expires. Repository caller search finds only the dedicated `pg_verifier_executions.rs:1834-1913` test/import. Actual direct/queued execution uses HostedBlocks → CallerTraces → VerificationEngine::trace once; source read errors become inconclusive. Runner has no trace-wait transition and old trace-poll settings were deleted. Standard release/retry/lease owners already cover active queue behavior.

**Correction:** delete just that SQL constant, enum, method and dedicated test/import. Do not wire it into production to justify it, change current trace readiness semantics, delete normal release/backpressure/fencing, or remove genuine active coverage. This does not allege a dormant runtime defect; it satisfies the explicit unearned-code deletion criterion.

**Closure:** no remaining source references; SQL crate compile/check and existing affected queue checks. No replacement harness/test for an API whose only caller was its own test.

## Reconciled acceptance and verification assessment

| Obligation family | Reconciled result |
|---|---|
| Direct engine, no queued activation/Operator, shared SDK/control composition | PASS, except File staging FIND-4 and shared admission obligations FIND-2 |
| ACK-only queue creation, no request PG write, stable keyed replay/conflict, tenant SQL capabilities | PASS; subject authorization independently FAIL FIND-1 |
| Fenced claims, shutdown, retries, independent Operator delivery | PASS for inspected existing paths; abandoned-vs-winning result identity FAIL FIND-5 |
| Status/result/dispatch/binding projection | PASS normal paths; recovery result selection FAIL FIND-5 |
| Statistical algorithms/readiness/baseline identities, mapped scheduled sources | PASS mapped cases; omitted optional source FAIL FIND-3 |
| Process startup limits and resource isolation | Startup configuration/packaging evidence credible; real runtime capacity behavior FAIL FIND-2 |
| Approved public HTTP/MCP and first-class File entry workflows | FAIL FIND-4; SPEC_REVISION_REQUIRED FIND-6 |
| Complete live contract cutover, unrelated scenario behavior | PASS with required deletion FIND-9 and documentation FIND-7/8 |
| Three-language functional journeys and external examples | Supplied deterministic evidence credible for exercised paths; live providers and ignored local dependency artifacts not proven |
| Migration/deployed history compatibility | Not required by owner; DATA-001 rejected |
| New scheduled statistical capacity benchmark qualification | Not a current task obligation; BEH-04 rejected; do not invent scope |
| Rustdoc and current normative authority alignment | FAIL FIND-7/8 |

The recorded full gate at `03712af5e` and final evidence-only candidate commit are useful evidence, not substitutes for these reachable source failures. No fresh exact tests were executed in validation, so no new runtime reproduction or performance claim is made. Narrow focused closure scenarios above deliberately avoid broad aggregates/full user-journey suites for bounded remediation; full change review owns final integrated qualification. Findings are grouped at their producer/owning invariant, avoiding repeated downstream guards. No proposed correction requires a new dependency, migration, result-recovery service, provider exactly-once claim, new worker reservation policy, or weakened trust boundary.

Skipped broad reruns, live providers, cloud storage and external build reproduction. Current blockers are the listed source-proven gaps and the unapproved public upload boundary.
