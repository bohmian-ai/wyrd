# TASK-005 independent behavior review

Overall: **FAIL**.

Subject: base `f2a726c90edbf8956c7b91c843379175ee20eac0`, candidate `99882cf824e70a601033aba4cfa5c5b27a8dfc7c`, approved `changes/active/verifier-building-blocks/spec.md` r8 and original `tasks/TASK-005-server-integration.md`. HEAD remained the candidate. This report owns no implementation edits. Discovery did not read peer findings.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| One hosted engine and existing control workflow; focused dependencies | `VerificationControl`, `HostedBlocks::engine`, `VerifierRunner::dispatch` compose Vala blocks, existing AgentExecutor, gateway, registry, sandbox and evidence owners | Engine/module selections and integrated gate supplied in task Proof | PASS |
| Direct Run execution returns breakdown, records inputs/result without activation; no Operator | `VerificationControl::execute`, `stage_direct`; `ScribeWrite::ExecutionInputs` has no queued submission | `pg_verification_routes::direct_execution_judges_inline_without_enqueueing`; supplied SDK immediate pass/fail journeys | PASS |
| No request-path Postgres writes; ACK then batched SQL admission | `enqueue` only constructs Submission and stages Scribe; `ScribeSink` ACK hands to `SubmissionOutbox`; `SubmissionSink::write` owns tenant transaction | Real route/runtime and SQL queue suites in supplied gate | PASS |
| Replay/conflict stable identity and no duplicate logical work | `Submission::new`, scoped advisory lock and existing key/hash check in `VerifierExecutionQueue::accept`, errored conflict insertion | `enqueued_executions_replay_conflict_and_read_back`, SQL concurrent acceptance tests | PASS |
| Preserve signed subject Card authority when queueing/retrying | Removed base `subject_in_scope`; current enqueue only checks Verifier permission, SQL accept and runner never apply principal Card scope | Existing tests prove tenant/RBAC but do not test Card-bound foreign subject rejection | **FAIL BEH-01** |
| Dispatch state readable independently; latest binding follows admission | `execution_status`, `get_execution`, execution-keyed dispatch FK and existing worker; SQL admission updates latest identity | `pg_operator_delivery`, binding/latest SQL tests and reconnect journeys supplied | PASS |
| Frozen Operator context, execution identity, bounded summary; no guessed multi-check drift counts | `engines::{summary,drift_counts}`, execution settlement and existing `operators.rs`; counts absent unless exactly one drift detail | scheduled-drift Operator tests and `failure_context_counts_requirements_and_the_one_drift_check` | PASS for newly admitted executions; historical pending deliveries need explicit follow-up noted below |
| Published input/result fields, nullable binding and application Run | `ResultPayloadBuilder`, `ExecutionInputsTable`, `ResultsTable`, mapped by column name | Rust/TS/Python persisted result journeys supplied | PASS |
| Read and wait reconnect to real server and tolerate delayed visibility | `get_execution` reads durable queue then ordinary Bifrost result; shared client Execution polls | three-language `verify_in_real_time` reconnect and cross-tenant tests supplied | PASS |
| Scheduled drift reuses scheduler, fitter, exact requirement state and failed-only Operators | `scheduler.rs`, `drift.rs`, `fitter.rs`, baseline requirement key, existing queue completion/dispatch owner | all-method building-block and scheduled-drift SDK journeys supplied; no live provider needed | PASS |
| All built-in tools resolve under separate invoke/domain authority and accepted delegation | `HostedBlocks`, `HostedTools`, existing `ToolRegistry`/AgentExecutor, accepted Caller; per-statement request IDs | real hosted-tool grant/denial/delegation journeys supplied across SDKs | PASS |
| Startup parses limits once; background Python work stays queued under saturation | config parser and shared pool exist, but runner starts unlimited executions; saturated code checks become completed inconclusive outcomes | startup validation present; scripted claim tests do not exercise real Monty saturation | **FAIL BEH-02** |
| All eight action/check rows, extraction formats, negative cases across SDKs | Rust building-block file and Python/TS mirrors drive real server, fixtures include formats and source citations | supplied full gate and matrix inventory | PASS subject to BEH-02 capacity gap |
| Remove live old contracts, preserve unrelated scenarios and historical evidence | old eval/direct/observations owners and judge implementation deleted; scenario assertion/trace/agent executors retained; additive rename migration retains rows | codegen/docs/SDK/SQL gates supplied; actual cumulative diff reviewed | PASS for live cutover; historical pending reaction caveat below |
| HTTP/MCP five operations, no old aliases, no new public API | exact MCP tools migrated; router additionally publishes local evidence PUT | served OpenAPI test explicitly expects sixth local route | **FAIL BEH-03** |
| Affected semantic lineage closes through existing visitors/relationships | existing Card reference visitors and registration/read consumers; no new graph store | three-language register/hydrate semantic assertions supplied | PASS |
| Standalone examples own Cards/evidence and use shared SDK conventions | immutable external heads TS `0e45152b5ad0bf33f6d6518c305f5d8868eb7e71`, Python `a9a6327b9027223a128d53f64360d1d9bc7c0555`, Rust `f7647cf3fc6f63b102695a08d22ee73aef9c0b04`; respective source uses Run/transport hook, SDK telemetry, verify/submit/reconnect | supplied deterministic tests 5/5, 5/5, 1/1; README live provider execution not claimed | PASS |
| AC-001–010 integrated proof under repository policy | supplied gate on `03712af5e`; later commits record proof; complete source range reviewed | task Proof records gate exit 0; no independent rerun here | FAIL overall because source-proven gaps remain despite supplied green aggregate |

## Proposed findings

### BEH-01 — VIOLATION / REGRESSION: queued verification discards the signed subject scope

**Obligation:** INV-003 preserves signed Card scope; task scenario 1/2 preserves exact subject and accepted authority. `wyrd-runtime/src/principal.rs:215–230` explicitly defines Card attribution for both observations **and verifications**. Base `VerificationControl::start_run` checked `subject_in_scope` before admission.

**Location:** `crates/wyrd/wyrd-server/src/components/verification/service.rs:553–588` (`enqueue`); `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs:1149–1171` (acceptance); the deleted `subject_in_scope` in base control.

**Producer → consumer:** a Card-bound Service with `verifier:run` for Verifier V sends inline JSON about unrelated same-tenant subject S. `authorize_run` checks only V, Submission retains the principal but no scope decision, SQL resolves only V/S presence/readiness and inserts pending, runner's `Caller::accepted` retains that unchecked authority, then executes and persists `card_uid=S`. An assert-only Verifier never calls the gateway, so the gateway's own Card attribution check cannot close this path. The Scribe internal writer does not enforce the original principal's signed scope.

**Sibling paths/owners:** HTTP and MCP share this `enqueue`; SDK submit shares HTTP. Direct execute also lacks subject-scope enforcement, but that direct gap already existed in the base, so it is not independently classified as a new direct regression here. Scheduled work retains binding-owner authority and is not a replacement check for manual work. Existing `Principal::{card_attribution,authorizes_card}` and the removed control helper are the relevant mechanisms; no new policy service is needed.

**Consequence:** a workload can attribute a durable judgment and supporting evidence to Cards outside its declared signed graph.

**Smallest correction:** restore original subject-scope authorization in the shared manual submission/control owner using existing principal attribution semantics and exact registered Card identity. A read is compatible with the no-Postgres-**write** constraint. Preserve unbound human/automation RBAC behavior, accepted authority, non-blocking audit and batching. Do not rely on model calls or Scribe SYSTEM credentials to police subject attribution.

**Closure proof:** real HTTP/SDK (and shared MCP projection) submission by a Card-bound principal with V grant but no S scope is refused/audited and creates no execution/result; an in-scope subject succeeds; unbound caller behavior remains valid. Exact-focused tests and the affected verification route lane suffice.

### BEH-02 — INCORRECT: background Python saturation is converted into a terminal inconclusive judgment

**Obligation:** spec Deployment limits: background executions remain in the existing queue until runtime capacity is available; Scenario 6 expressly requires real saturation proof. INV-007 requires bounded in-process waiting.

**Location:** `crates/wyrd/wyrd-server/src/verification/runner.rs:192` (`ClaimLoop::new(..., None, ...)`), `runner.rs:309–337`, `runner.rs:358–416`; `hosted/sandbox.rs:159–175`; `crates/vala/vala-eval/src/blocks/engine.rs:407–409`.

**Producer → consumer:** runner claims and starts every due execution with no capacity reservation. A code check enters Sandbox; first operations occupy workers and configured waiters, later `try_acquire` returns retryable capacity. `VerificationEngine::judge` turns **every** check executor error, including this retryable capacity error, into `Judged::inconclusive`. The engine returns `Ok(outcome)`, runner calls `result`, settles completed and publishes an inconclusive verdict. Even admitted operations wait inside the process instead of remaining queued. A code action similarly consumes its authored step retry budget and may finish failed/inconclusive due solely to process capacity.

**Sibling paths/owners:** direct code, background code and Agent-action `code.run` share Sandbox; direct bounded rejection/waiting and authored action retry semantics must remain intact. Existing ClaimLoop/VerifierExecutionQueue defer/fencing and Sandbox capacity owners already exist. `EngineOutcome::Deferred` currently handles runtime read backpressure, but this capacity state never reaches it. Scripted `verifier_runs_execute_beyond_the_former_permit_ceilings` is deliberately not proof of real Python saturation.

**Consequence:** valid submitted work is consumed as completed inconclusive during ordinary contention and will not run when a worker frees. This is observable loss of the promised queue behavior, not a discretionary throughput improvement.

**Required correction outcome:** coordinate queued Python capacity through existing runner/queue and sandbox owners so resource shortage preserves/requeues the same execution without consuming authored retries or manufacturing a judgment, and bound waiting. Do not cap Agent/model-only work by reserving Python workers for the whole invocation; do not add a second queue or scheduler. The precise resource handoff must preserve the spec's operation-only worker ownership.

**Closure proof:** real worker pool configured to one usable worker and minimal waiters; hold one operation, enqueue additional simple code-check executions, prove they remain/revert pending without published inconclusive results, then free capacity and prove each completes with its real result and stable identity. Also prove non-Python verification progresses and direct admission stays bounded.

### BEH-03 — DRIFT / VIOLATION: a new public upload API crosses the task's material stop

**Obligation:** task Material Stop Conditions explicitly prohibits a new public API; REQ-012 lists the approved control routes. REQ-010 requires existing single-PUT upload plans and storage client; it does not approve a sixth verification endpoint.

**Location:** `crates/wyrd/wyrd-server/src/components/verification/routes.rs:38–46,153–200`; `crates/wyrd/wyrd-storage/src/service.rs` `evidence_upload_plan` emits `/v1/verification/evidence/local/{sha256}`; control `upload_local_evidence`; OpenAPI contract test `verification_contract_publishes_five_typed_operations_and_the_local_upload`.

**Evidence/consequence:** this is an authenticated externally callable PUT route, conditionally published in served OpenAPI and required by the local SDK upload plan. Backend specificity does not make a wire endpoint private. It is not merely a private storage helper. It establishes a new trust, authentication, error and deployment contract beyond approved route inventory. The existing Cards local upload endpoint is a precedent but not authority for adding another public resource.

**Correction boundary:** if local evidence upload cannot be fulfilled by an already-approved storage capability without introducing a new public contract or metadata row, obtain a spec revision explicitly approving this endpoint and its backend semantics. Deleting it alone would break required local file journeys; therefore do not prescribe deletion as a bounded implementation fix. This finding calls for `SPEC_REVISION_REQUIRED`, not inventing a parallel upload mechanism.

## Existing workflow reuse assessment

- HostedBlocks is justified process dependency ownership, composes existing Vala engine/Skald Agent executor/ToolRegistry/gateway/evidence/sandbox rather than another execution algorithm.
- SubmissionSink replaces observation enqueue activation using existing Outbox and queue transactional acceptance; it does not add durable receipt/reservation state. Replay acceptance belongs there. The missing Card-scope gate is not solved by copying authorization into every consumer.
- CallerTraces replaces the old hosted Eval trace reader with the existing ScheduledQueryCaller and shared span projection. Ordering by start time and span ID is reproducible. The spec does not promise parent-first ordering for timestamp ties; no ordering finding is proposed solely from the TS fixture's explicit time separation. A seven-day lookback can make old traces unavailable but produces inconclusive rather than fabricated passes; extending the public trace identity/time contract is outside this task.
- ResultPayloadBuilder extends existing canonical result/input writers, preserving named column mapping and original identities; no second result store was found.
- Existing Operator queue/worker/renderer remain owners. A separate reachable migration uncertainty should be checked in reconciliation: migration changes pending historical `failure_context` field `run_id` to `execution_id`, but frozen old Operator templates still use `{{run_id}}`; current renderer rejects that field. I did not elevate it without completing historical Card decoding/caller proof. This is not a confirmed finding in this report.

## Verification and limits

This is static independent discovery. No broad gate or live-provider test was rerun. Supplied `mise run gate` evidence is credible for its recorded candidate and scenarios, but does not prove the absent Card-scope and actual Monty-saturation cases. The external immutable identities/source inventory were available; ignored candidate builds are proof dependencies, not source identities. No concern is raised merely because commits are unpushed. Live provider credentials are not required by AC-008's deterministic automated proof.

No optional refactoring or ordering redesign is requested. Risk remains the unchecked subject attribution, saturation-induced inconclusive completion, and unapproved public route described above.

## Additional capacity-benchmark acceptance audit

Reviewed cumulative fixture/load/report/step/evidence/main changes and `mise.toml::bench:capacity` after the initial runtime pass. The TASK-001 handoff explicitly requires deleting the LLM judge everywhere, including capacity `Kind::Judge` and judge setup. The deletion search for `JudgeError|JudgeInvoker|JudgeTaskExecutor|LlmJudge|llm_judge|judge_mock|judge-mock|JudgeOutcome|TimedJudge|EvalEngine|TenantMedia` returned no remaining source hits outside change records. This part passes; restoring old judge or immediate Drift inputs would violate approved r8.

### BEH-04 — DRIFT / MISSING: benchmark migration deletes remaining statistical verification coverage and changes qualification meaning without approved replacement

**Current obligation:** TASK-005 permits minimum shared-contract switches and final old-type deletion while preserving unrelated existing capabilities. Its judge deletion map authorizes removing the judge capacity case. The current approved r8 specification preserves existing PSI/SPC/Custom algorithms and requires their integrated proof; it does not approve redefining the capacity qualification workload as assertion-only. The older still-active `changes/active/verified-change-contract/spec.md` REQ-171 and AC-040 explain the baseline benchmark meaning, but their immediate-drift and LLM-judge mechanics are superseded and cannot govern a correction literally.

**Locations:** `capacity/fixture.rs:1–110,125` removes fitted PSI/SPC baselines and all statistical execution fixtures; `capacity/load.rs::mix` selects only assertion direct/enqueue requests; `capacity/report.rs:190–208` replaces separate per-kind overhead/sample cells with one assertion cell; `capacity/step.rs::Deployment::run` collects that reduced evidence; `mise.toml:516` advertises the reduced production mix under the same REQ-171/AC-040 identities. The change is much larger than the expressly required deletion of `Kind::Judge`.

**Reachability/consequence:** running the existing `bench:capacity` now judges a system under assertions plus ingested samples; its drift Verifier intentionally has no schedule and never executes (`fixture.rs` module docs). PSI/SPC/Custom verification contributes no engine, fitted-baseline, scheduler or result pressure to the measured server. The PASS report can therefore qualify a system that was never subjected to those preserved runtime workloads. No live benchmark run is required to establish that the workload paths were removed.

**Existing alternatives:** the new scheduler/fitter/Window verification workflow is the current owner for statistical checks; do not resurrect removed direct drift scoring, old kind labels, or legacy judge mock. Existing benchmark orchestration and report owners can host an approved replacement. However selecting its schedule/rates, interpretation of queued-vs-scheduled execution, and replacement overhead SLO changes qualification semantics; r8 contains no such decision.

**Disposition/recommendation:** `SPEC_REVISION_REQUIRED` for the benchmark replacement meaning or explicit approval to narrow its qualification to assertions. Do not hand an implementer a task that silently chooses a new statistical workload or treats the obsolete parallel spec's direct API as valid. Once approved, focus benchmark fixture/mix/report tests on the chosen coverage and honest labels, retaining the existing benchmark owner. This is separate from BEH-02 runtime admission correctness.

Acceptance addition: capacity consumer compiles and obsolete judge deletion is complete (**PASS**); preserving an honest approved capacity qualification after removing obsolete mechanics is **FAIL BEH-04**, pending semantic authority.
