---
id: TASK-005
kind: implementation
status: review
spec: SPEC-verifier-building-blocks
spec_revision: 8
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-010, REQ-011, REQ-012, REQ-013, REQ-014, REQ-015, REQ-016, REQ-017, INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008, AC-001, AC-003, AC-005, AC-006, AC-007, AC-008, AC-009, AC-010]
depends_on: [TASK-002, TASK-003, TASK-004]
---

# Compose one hosted verification workflow and finish the cutover

## Outcome and Value

The separately tested contracts, Vala engine, hosted checks, and clients form
one real verification product: direct requests and batched submissions execute,
persist attributable evidence, expose status/results, and react only through
the existing background Operator path. The integrated candidate then earns
all deferred integration, three-language journey, and working-demo evidence.

## Owners, Scope, Consumers, and Prohibited Changes

Own final server admission and evidence planning, existing control owner and
HTTP/MCP projection, registration and bindings, Scribe input/result publication,
acknowledgment-to-request batching, queue/SQL migrations, scheduling/baselines,
runner settlement, and affected CLI/UI consumers. Compose tasks 2/3 through
the shared task 1 boundary; consume task 4's client and examples unchanged in
meaning. Own the standalone developer example repositories (r8 REQ-016):
fully refactor `wyrd-example-ts` and create `wyrd-example-py` and
`wyrd-example-rs`; discard Task 4's uncommitted partial `wyrd-example-ts`
update rather than building on it. Keep modules independently callable with explicit dependencies; do
not build one AppState-coupled god object that cannot be exercised in isolation.

No per-request Postgres write: r7 preserves approved batching. Replay/conflict
acceptance happens in the queue writer, and a conflicting submission becomes
an errored execution through the existing status/error surface. No separate
reservation, receipt, transport, scheduler, result store, or durable outbox.

Preserve historical evidence and unchanged unrelated observation/scenario
capabilities. Remove obsolete live types/routes only as their final consumers
move; no compatibility aliases or permanent dual authored contracts.

## Source-backed Reuse Map

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
| --- | --- | --- | --- | --- | --- |
| One public control owner | `components/verification/service.rs::VerificationControl`, `routes.rs::verification_router`; MCP adapters | HTTP `pg_verification_routes` and served `pg_openapi_contract`; current MCP projection | Direct kind-specific inputs and manual Drift run control | Extend the existing control workflow for evidence/execute/enqueue/status, preserving auth/audit/error ownership | No parallel service just to reuse leaf calls |
| Queue, authority, replay | `wyrd-sql::VerifierRunQueue::enqueue_manual`, `RequestKey`, `RunAuthority`; existing tenant connection | VerificationControl start_run; runner claim/settle; pg runtime tests | Current replay reservation is synchronous SQL; submissions require batched async acceptance | Reuse key/hash/transactional uniqueness in the background writer; persist accepted/refused execution outcomes there | Extend existing queue rows and key checks, not a reservation service; preserve no-PG hot path |
| Post-ACK batching | `ObservationEnqueue::acknowledged`, `ObservationRunSink::write`, generic `Outbox`; Gate `ObservationAck` | Existing Eval capture acknowledgment and runner queue consumers | Binding fan-out and owner re-resolution do not fit exact caller submissions | Retarget to execution inputs/requests, submit-only marker, stable identity and accepted authority | Existing outbox/ACK workflow already owns this handoff; no second queue |
| Result persistence | `ResultPayloadBuilder`, `ScribeWrite`, `ScribeTable`; canonical ResultsTable and existing detail projections | `VerifierRunner::settle`; direct execute; pg runtime fault/publication coverage | Kind-specific payloads and no generic input/outcome reconstruction | Extend existing payload/table writers and reads for the generic result and execution input contracts | Approved fixed input table and canonical outcome only; no speculative requirement-results store |
| Scheduled drift and reactions | `VerificationScheduler::pass`, `DriftEngine::verify`, `FittedBaselines`, baseline queue, runner dispatch/settle | Existing runtime and scheduled-drift SDK journeys; `components/cards/resolve.rs::check_activation` | Explicit multi-slot evidence mapping and requirement-specific fitted state | Extend existing binding/window/baseline resolution and failed-only Operator settlement | Existing schedule/Operator owners remain; no workflow host or new statistical algorithm |
| Final consumer cutover | Existing server registration/ref visitors, CLI verification, UI Verifier/result consumers | Current wire contracts, docsite examples and three-language journeys | Consumers assume removed kinds, `/runs`, and Eval observation activation | Move remaining consumers to the integrated generic workflow and delete replaced code | Necessary closure, not a general UI redesign or unrelated migration |
| Dispatch reads and templates | `VerifierRunQueue::run_status`, `OperatorDispatchQueue`, `OperatorFailureContext::{field, render}` | `VerificationControl::get_run`, HTTP/MCP status, `verification/operators.rs` worker and SQL claim/settlement | New execution state omits dispatches; queue/context retain verification-run identity | Adapt the existing workflow and frozen context to REQ-012/014 | No second queue, renderer, delivery worker or dispatch read endpoint |
| Affected Card lineage | Reference visitors and `queries/cards/relationships.rs` | Binding registration/resolution and Card relationship reads | Generic `outbound` rows do not establish semantic kinds or `via` | Close affected Verifier/Trigger/Operator/baseline projection through existing owners | No second lineage store or unrelated lineage expansion |

## Approach

The r7 handoff below is part of this task's scope and proof, not a new task.

1. Start after tasks 2, 3, and 4 are integrated into the change branch; revalidate
   the shared contract and each module's isolated proof and composition notes.
2. Extend existing registration/control/evidence workflow, preserving exact
   tenant, subject, invocation permission, and accepted caller authority.
3. Retarget Scribe input acknowledgment and batched queue writes with stable
   request identities, key/hash acceptance, terminal conflict reporting,
   submit-only activation, and no request-path Postgres writes.
4. Compose the shared engine and hosted capabilities; extend scheduled window,
   baseline, settlement/result, status, and failed-only reaction behavior.
5. Complete wire/CLI/UI/generated consumer migration and remove obsolete live
   kinds/routes/observation paths without deleting historical evidence.
6. Add focused seam proof and complete the authored real-server/journey tests.
   Once this task too is merged, run the single integrated proof pass from
   README; repair failures and rerun only affected proof as needed.

## Ordered Implementation Scenarios

### Scenario 1 — Direct verification runs once under authorized evidence

**Behavior.** The typed request resolves exact Cards/evidence, calls the same
engine with hosted capabilities, stages inputs/results without queue activation,
and returns a breakdown or structured execution error. No Operator fires.
Digest/format/permission/tenant refusals happen at their owning boundaries.

**RED.** Add focused control/composition tests against injected existing
capabilities. Use them for TDD before a full server exists; separately author
real route and SDK assertions for the integrated pass. Confirm missing
composition rather than a setup failure; record actual exact selections.

**GREEN.** Extend VerificationControl and evidence planning, reuse tenant media
and shared storage, and compose the engine. Keep no-PG request semantics.

**REFACTOR.** Remove duplicated direct/kind dispatch; do not replace the
existing control owner or move durable rules into an SDK.

### Scenario 2 — Acknowledged submissions enter one batched queue

**Behavior.** Submit stages bounded pinned inputs plus accepted authority and
returns 202 before durable queue acceptance. Only queue_execution=true inputs
activate. Identical keyed requests retain one identity; conflicting content
gets its own observable errored submission with the existing conflict code,
leaving the original untouched. No conflicting/replayed logical work dispatches
another Operator (REQ-012/013, INV-003/004).

**RED.** Add focused input decoding/routing, identity, request hash, authority-
context round-trip, and writer-decision tests in the existing owners. Author
real Postgres/ACK races, restart, and conflict/status assertions now; run the
full environment-owning integration suites only after this task is merged.
Do not fake SQL atomicity in a unit test and claim the database behavior proven.

**GREEN.** Extend existing ACK/outbox/queue key checks and outcome mapping.
Commit replay acceptance and conflict refusal in the existing background
transaction. Preserve caller authority rather than resolving binding-owner
permissions for manually submitted work. Rerun scenario 1's local proof.

**REFACTOR.** Delete writer-Card Eval fan-out and remove duplicate enqueue
paths, not batching. No new per-request PG call or reservation subsystem.

### Scenario 3 — Results, statuses, and loss remain distinct

**Behavior.** The runner reads the pinned inputs, uses the same engine,
renews/fences claims, stages exact result/provenance, and settles independently
of result Scribe acknowledgment. Status may precede result visibility; missing
or lost records can time out; terminal conflicts/errors are not subject
failures. Direct recording cannot queue another execution (REQ-013/015).
Status also exposes dispatches independently of execution completion; binding
latest identity follows admitted work and published SQL uses REQ-015 fields.

**RED.** Add focused result mapping, status assembly, transition/fence,
loss/error-path and cancellation tests in existing module owners. Author real
reconnect status/wait, delivery-status visibility, binding admission ordering,
SQL field, and denied/cross-tenant dispatch-read assertions. Author real
outbox/lease/visibility tests using the existing runtime fault controls for the
merged run. Do not initialize Node or Python lifetimes in Rust tests.

**GREEN.** Extend existing ResultPayloadBuilder, queue settlement, outbox/table
admission and Bifrost reads; preserve subject/actor attribution, immutable
result identity, and historical rows. Rerun earlier local scenarios.

**REFACTOR.** Keep result persistence, lifecycle transitions, and execution
as cohesive separate capabilities. Remove obsolete result union publication
and unused types once no live consumer remains.

### Scenario 4 — Scheduled windows and reactions preserve their owners

**Behavior.** Schedule bindings fill explicit sample slots for the right
subject/window, use exact requirement-specific baselines, preserve readiness
and all drift methods, and dispatch only failed background verdicts. No
Trigger recursion or writes from checks (REQ-014). Extend the existing
scheduled drift journeys identified in README through the complete Service with declared
drift → async baseline fit → `observe.submit` → scheduled failed result →
actual Operator delivery lifecycle. A Model component is optional. Preserve
the real background owners and
assert no dispatch for passing/inconclusive outcomes or duplicate logical
dispatch on replay.
Templates use execution identity and a bounded generic summary through the
existing renderer/context; independent delivery failure preserves the verdict.

**RED.** Add focused mapping/readiness/decision tests and author the real
window, baseline, authority, replay and Operator integration cases in existing
owners. Keep their full execution in the merged proof pass.
Include immutable retry context, removed/inapplicable template fields, and
failed delivery visible on a completed execution.

**GREEN.** Extend existing registration, scheduler/fitter, window reads, and
runner dispatch composition. Preserve fixed boundaries and existing delivery
retry semantics; rerun earlier local cases.

**REFACTOR.** Remove implementation-kind activation checks and stale bindings
without adding a second scheduler or statistical implementation.

### Scenario 5 — All real surfaces tell the same story

**Behavior.** HTTP/MCP/SDKs/CLI/UI consume the new identity/result contract;
all three language examples and the three standalone example repositories
execute against the built server; no old `/runs`/kind/observation workflow remains. Integrated negative
and edge behavior satisfies every shared acceptance obligation. Execute every
row of the README action/check coverage matrix through all three real SDKs;
reused executors must be exercised through the new contracts too.

**RED.** Complete the real route, served OpenAPI, MCP, three-language journeys,
and runnable-demo assertions alongside the owning changes. Their first full
execution is on the candidate with all five tasks merged. Record expected
missing/mismatched behavior when encountered there; never call a local double
an end-to-end RED.

**GREEN.** Finish live composition and consumer cutover, merge through the
caller-owned workflow, then run the coordinated final evidence pass. Fix
integration failures in their actual owner and rerun the failing exact tests
and affected checks, not unrelated full batteries.

**REFACTOR.** Remove temporary migration scaffolding and stale references while
all isolated and required integrated behavior remains green. Do not weaken
checks or add compatibility aliases to pass.

### Scenario 6 — Startup configuration governs runtime capacity

**Behavior.** Read every Deployment limits environment variable once at startup,
apply approved defaults, reject invalid/inconsistent settings, and compose one
pool per server process. Existing queued background work waits for capacity;
in-process waiting remains bounded. Configuration changes require restart.

**RED.** Add isolated configuration tests for defaults, overrides, malformed,
zero/overflowing, and inconsistent values. Author real startup/admission
integration proof that the configured pool limits are used and background work
remains queued under saturation; execute it in the final merged pass.

**GREEN.** Extend existing server startup configuration and runtime admission;
pass typed configuration into task 3's runtime owner without per-call environment
reads. Preserve existing queue leases/fencing and shutdown behavior.

**REFACTOR.** Remove duplicate defaults and environment parsing; use one owning
configuration surface and existing capacity/error behavior.

## Contract Handoff From TASK-001

The LLM judge is not kept anywhere, including offline `wyrd eval run`
(owner decision). Delete it with the old Eval/task Verifier cutover; keep no
alias, flag, or compatibility path. Its replacement is an `agent` check
executed through `vala_eval::blocks::CheckExecutor` (TASK-003) returning
`BlockError`. Removal map at staging time (`git grep -nE
"JudgeError|JudgeInvoker|JudgeTaskExecutor|LlmJudge|llm_judge|judge_mock|judge-mock|JudgeOutcome|TimedJudge|EvalEngine|TenantMedia"`
must return nothing outside `changes/` when done):

| Area | Delete |
| --- | --- |
| Contracts (`wyrd-spec`) | `vala/eval/llm_judge.rs` (`LlmJudgeTask`), `EvalTask::LlmJudge` in `vala/eval/task.rs` and its `mod.rs`/`spec.rs`/`result.rs` uses; `VerifierTask::LlmJudge` in `card/verifier.rs`; judge `VerifierKind` variants in `verification.rs`; judge handling in `refs/mod.rs`, `graph/composition.rs`, `reference.rs` tests; fixture `tests/fixtures/verifier-task-llm-judge.yaml` |
| Engine (`vala-eval`) | `judge.rs` (`JudgeError`, `JudgeInvoker`, `MockJudgeInvoker`); `tasks/judge.rs` (`JudgeTaskExecutor`); `orchestrator/judge.rs` (`SkaldJudgeInvoker`, after TASK-003 moved any reused helper); `Executors::judge` and `EvalTaskKind` judge kind (`executor.rs`, `store.rs`); judge variants in `error.rs`; judge media binding in `tasks/media.rs`; judge parameters of `orchestrator/scoring.rs::ScenarioScoring`; judge cases in `scenario.rs`, `context.rs`, `tasks/{assertion,trace,agent}.rs` tests; integration module `tests/integration/orchestrator_judge_skald.rs` and judge cases in `orchestrator_parity.rs`/`orchestrator_support` |
| Server | The whole of `wyrd-server/src/verification/eval.rs` (`EvalEngine`, `TimedJudge`, `BifrostReader` over the deleted `vala.eval.observations` input, `TenantMedia` and its `supported`/media limit, and the Eval-only helpers) and its uses in `verification/{direct,mod}.rs`; judge cases in `tests/integration/{pg_verification_routes,pg_verification_runtime,pg_card_registration_route}.rs`. `TenantRegistry` is the one server Agent/Prompt Card resolver the `agent` action and check need: TASK-003 moves it to `verification/hosted/registry.rs` and authorizes every lookup as the accepted caller's audited `card:read`; TASK-005 composes it into `AgentExecutor`. No `eval` module, type, or file name remains in `verification/`. |
| CLI | `--judge-mock`, `judge_for_spec`, `has_llm_judge` in `wyrd-cli/src/eval/run.rs`; `WyrdCliError::JudgeMockRequired`; judge plumbing in `eval/local_records.rs`; judge fixtures/support in `tests/eval_support`, `tests/eval_local_records.rs`, `tests/fixtures/loader/end_to_end/agent1/eval.yaml` |
| Client/testing | `LlmJudgeTask` test in `wyrd-client/src/state.rs`; `Kind::Judge` and `eval_llm_judge` cells in `wyrd-testing/src/bin/capacity/{fixture,report}.rs`; `wyrd-testing/tests/bifrost/server/eval_verification.rs` judge cases |
| SDK/docs/fixtures | Owned by TASK-004 (listed there); coordinate so generated SDK types drop judge kinds when the contracts do |

Offline `wyrd eval run`/`ScenarioScoring` keeps assertion, trace, and agent
scenario tasks only. `architecture/wyrd-design.md` mentions the judge in six
places; update it in the same change.

The remaining staged switch points are listed in TASK-001's "Staged exports
and switch points" table.

## Acceptance Criteria

- Execution status exposes existing dispatch states under tenant authorization;
  delivery retries/failure remain readable on a completed execution without
  changing its verdict. Binding `last_execution_id` follows admitted work.
- MCP exposes the r7 verification tools and removes old start/get-run tools
  without aliases. Served OpenAPI, SDKs, CLI/UI and docs agree on the shapes.
- Existing Operator queue, worker, renderer and frozen context are reused with
  execution identity and bounded generic summaries. Obsolete verification
  `run_id` and `eval.*` template fields are rejected; applicable drift counts
  retain their meaning, with no guessed multi-check aggregate.
- Real Bifrost reads prove published `execution_id`, nullable `binding_id` and
  application `run_id`, result `started_at`/`ended_at`, and input `recorded_at`.
  Affected lineage reads prove design semantic kinds, direction and `via`;
  generic `outbound` persistence alone is insufficient evidence.
- Reconnected handles exercise both `status()` and bounded `wait()` against
  the real server. Server dependence is a journey requirement, not a reason
  to omit coverage or substitute a unit test.

- All composed modules retain explicit dependencies and focused isolated tests.
- Startup supplies the built-in Agent tools and hosted Invoker resolution. Real
  tool calls enforce invoke permission using Wyrd's existing global/scoped
  permission rules, then independent
  Card/Bifrost resource permissions, original-subject/delegated authority, tenant
  isolation, and audit; queued retry/restart preserves it. Interleaved calls
  cannot reuse another invocation's authority/workspace. No SYSTEM privilege
  fallback, client registration, or loopback MCP is needed. Exercise absent
  tool grant, absent domain grant, exact-name mismatch, scoped multi-table
  denial, code/filesystem refusal, and narrowed delegation with real audit
  decisions. Extend existing grant issuance/delegation/forwarding consumers so
  tool scopes survive acceptance/retry/restart without widening.
- No execute/enqueue request writes Postgres; queue insertions remain batched.
- Stable-key replay and delayed conflict outcomes work across concurrent
  requests/server replicas and do not overwrite original evidence or authority.
- Direct recording never activates the runner; background failed-only Operator
  behavior and lifecycle cancellation/leases remain correct.
- Generic input/results and all affected consumers replace old live contracts
  without historical data deletion, extra storage systems, or a Workflow host.
- The full README scheduled drift lifecycle passes in all three SDKs by
  extending the existing journeys; fitted state is not seeded and the scheduler
  is not bypassed by direct verification.
- All eight README action/check rows have passing real journey evidence in
  Rust, Python, and TypeScript, including the required negative/edge cases,
  all extraction formats, and all supported drift methods.
- `wyrd-example-ts`, `wyrd-example-py`, and `wyrd-example-rs` each own their
  Cards and sample evidence, follow the shared layout and REQ-016 conventions
  (transport hook in TypeScript/Python, Wyrd Agent Run in Rust, SDK OTel
  exporters, `verify`/`submit`/`verify.Execution`), contain no hand-built
  credential headers or exporter wiring, work from their README quickstart,
  and pass their tests against the merged candidate with a deterministic
  provider.
- The merged candidate closes AC-001–010 with the shared evidence inventory;
  local-only proof is not reported as journey/integration completion.

## Expected Write Set and Consumer Closure

Server verification control/routes, MCP, evidence/runtime composition,
registration/binding/reference integration, runner/scheduler/baseline/result/
Scribe/outbox owners; `wyrd-sql` queue/binding/baseline migration/query owners;
Bifrost fixed-table/admission/ACK and canonical query projection; existing CLI
and UI verification consumers; minimum shared-contract switches and final old-
type deletion; actual generated artifacts from their owners. Existing managed
Postgres/runtime fixtures and server/SDK/MCP journey targets receive necessary
closure changes. External `../wyrd-example-ts`, `../wyrd-example-py`, and
`../wyrd-example-rs` source, `service/` Cards, scripts, tests, and
package/build setup. Coordinate with task 4 on doc/schema/SDK fixes instead of
copying their logic into server helpers.

## Verification and Evidence

### Contract handoff from TASK-004 (approved r7)

- Implement `verification.execute`, `verification.enqueue`,
  `verification.evidence`, `verification.get_execution`, and retained
  `verification.get_binding`. Remove `verification.start_run` and
  `verification.get_run`; update runtime catalog descriptions/tests.
- Extend `VerificationExecutionState` using existing `OperatorDispatchState`
  and replace binding `last_run_id` with `last_execution_id`. Close SDK
  wrappers/stubs/generated types, CLI/UI, examples and documentation.
- Reuse `VerificationControl::get_run`/`VerifierRunQueue::run_status` status
  assembly, `OperatorDispatchQueue` and `verification/operators.rs` delivery,
  and `OperatorFailureContext::{field, render}`. Their queue foreign keys,
  claim joins, deduplication, and frozen context need execution identity;
  response-only renaming is insufficient. No new dispatch endpoint, queue,
  renderer or delivery loop. Preserve leases, retries and sibling isolation.
- Extend Scenario 3's status/result tests with dispatch visibility, admission
  ordering for binding latest identity, published SQL fields, reconnect
  status/wait, and denied/cross-tenant dispatch reads. Extend Scenario 4's
  reaction tests with immutable template retries, removed/inapplicable field
  rejection, and a completed execution whose delivery fails independently.
- Extend Scenario 5's real catalog and registration/read proof with exact MCP
  names and affected Verifier/Trigger/Operator/baseline lineage semantics.
  Existing reference visitors and `queries/cards/relationships.rs` remain
  owners; generic `outbound` rows do not prove semantic kinds or `via`.
  Do not introduce a second lineage store or expand unrelated lineage work.
- Publish REQ-015's correlation/time fields and update SQL examples. Preserve
  historical evidence without relabelling it as generic results.
- Task 004's drift refusal uses `WYRD_VERIFICATION_422_INPUT_UNSUPPORTED`.
  Correct stale derive-backed error remediation advertising direct Drift or
  LLM-judge execution and regenerate dependent artifacts from their source.
  The worked example is corrected with this handoff.

### Proof execution

Use TDD for local decoding, policy, mapping, state transitions and composition.
Real SQL atomicity, tenancy, worker-server, SDK and OpenAPI/MCP behavior require
integrated proof, not mocked substitutions. During implementation use only
focused isolated selections, including:

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/^verification::/) and not test(/::pg_tests::/)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/^components::verification::/) and not test(/::pg_tests::/)'
mise exec -- cargo clippy --locked -p wyrd-server -p wyrd-sql --all-features --lib -- -D warnings
mise run fmt
mise run check:deps
mise run check:tenant-isolation
```

Collect actual test names and record each exact focused RED/GREEN command.
Compile owning integration/journey targets when changing them without running
full suites prematurely. After all five task changes are merged, execute the
shared once-integrated proof in README, including this task's real route,
queue, loss, cross-tenant, and replay cases. Do not copy those full suites into
each task or count this task complete before their required evidence passes.

## Material Stop Conditions

No switch to per-request Postgres writes, synchronous replay acknowledgment,
new reservation/receipt resource, durable result outbox, weaker authority or
isolation, new public API, destructive history rewrite, or new Card kind is
allowed. If existing queue/outbox owners cannot satisfy r6's async contract,
return evidence for a spec revision; do not silently change acceptance or hide
failure behind retries. Private row/column/query organization stays within the
approved queue/input/result boundaries, except the published fields fixed by
REQ-015. If multiple drift checks make template counts ambiguous, obtain an
approved semantic revision before changing their meaning; do not silently sum
counts, select one check, or fabricate values.

## Authority Links

- [Approved specification r7](../spec.md), [supporting proposal](../proposal.md),
  [worked examples](../example.md), and [flow diagram](../architecture/verification-flow.html).
- [AGENTS.md](../../../../AGENTS.md), [agent rules](../../../../architecture/agent-rules.md),
  [Wyrd design](../../../../architecture/wyrd-design.md), and
  [Bifrost design](../../../../architecture/bifrost-design.md).
- [Execution and final proof coordination](README.md). Task status is proposed;
  this packet does not grant a readiness verdict or implementation approval.

## Implementation Evidence

Commits `f2a726c90..HEAD` on `change/verifier-building-blocks-task-005`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Execution status exposes dispatch states; delivery failure keeps the verdict; binding `last_execution_id` follows admitted work | `c01c9739a`, `142ea3d46` (queue on `execution_id`, `VerificationExecutionState` with dispatch, binding `last_execution_id`) | `pg_operator_delivery` 17/17 (incl. `verifiers_progress_while_operator_deliveries_are_capped`); drift lane 40/40 | PASS |
| MCP r7 tools; old start/get-run removed with no aliases; OpenAPI/SDK/CLI/UI/docs agree | `142ea3d46`, `093a77ba7`, `ef98b0b8f`, `05a7370b3`, `b1dc71cb2` | `pg_openapi_contract::verification_contract_publishes_five_typed_operations_and_the_local_upload`; `codegen:check`; `docs:check`; MCP runtime tests in `093a77ba7` | PASS |
| Operator queue/worker/renderer reused with execution identity; obsolete `run_id`/`eval.*` fields rejected | `c01c9739a`, `a36c01870` (template `{{execution_id}}`/`{{summary}}`) | `scheduled_drift_alerts_operator::*` 5/5 in drift lane | PASS |
| Bifrost reads prove `execution_id`, nullable `binding_id`, application `run_id`, times; lineage semantic kinds | `f210fef96`, `142ea3d46` | `verify_in_real_time::*`, `verifier_building_blocks::*` (retained result reads); `register_and_hydrate` 13/13 | PASS |
| Reconnected handles exercise `status()` and bounded `wait()` against the real server | `142ea3d46` | `verify_in_real_time::submitted_answer_is_judged_in_the_background_and_resumes_by_id`, `another_tenant_cannot_resume_an_execution` | PASS |
| Built-in Agent tools and hosted Invoker resolution under real authority, delegation, isolation and audit | `5690c6452`, `b5be79e2b`, `c4a31e8bd`, `b3316b1d5` | `verifier_building_blocks::hosted_tools_need_the_tool_and_domain_grants`, `delegated_tools_keep_the_narrower_authority`, `forbidden_code_access_cannot_pass_dependent_work` | PASS |
| No execute/enqueue request writes Postgres; queue insertions batched | `142ea3d46` (evidence at `evidence/sha256/<digest>`, no row; enqueue through batched queue) | `wyrd-server --lib verification::` selections; `check:tenant-isolation` | PASS |
| Stable-key replay and delayed conflict | `142ea3d46` | `093a77ba7` server integration replay/conflict cases | PASS |
| Direct recording never activates the runner; failed-only Operator; leases | `c01c9739a` | `pg_operator_delivery` 17/17; `passing_window_alerts_no_one`, `empty_window_is_inconclusive_and_alerts_no_one` | PASS |
| Generic input/results replace old live contracts without history deletion | `f210fef96` (additive migration), `ac9539734` | `test:sql`; `pg_migration::migrations_apply_and_are_idempotent` | PASS |
| README scheduled drift lifecycle in all three SDKs, unseeded, through the scheduler | `a36c01870`, `b3316b1d5`, `50e4ed235` | Rust `scheduled_drift_alerts_operator::*` and `verify_in_real_time::unfitted_baseline_is_skipped_on_schedule`; Python/TS mirrors in the gate (Proof) | PASS |
| Eight README action/check rows in Rust, Python and TypeScript | `5690c6452`, `a36c01870` | Rust `verifier_building_blocks::*` 20/20; Python/TS mirrors in the gate (Proof) | PASS |
| External examples own Cards, follow REQ-016, pass against the candidate | `wyrd-example-rs` `f7647cf`, `669d130`; `wyrd-example-py` `5b2e1e9`, `a9a6327`, `1e27f68`; `wyrd-example-ts` branch `verifier-building-blocks` `79ea206`, `0e45152`, `5c79080`. Each checks answers with three Verifiers: Service-bound `no-refund-promise` (assert), agent-bound `ticket-complete` (code) and `follows-policy` (agent step + judge, `gpt-6-luna`, submitted) | `cargo test -- --ignored` 1/1; `pytest` 6/6; `npm run typecheck && npm test` 6/6 | PASS (README quickstarts not run live) |
| Merged candidate closes AC-001–010 | — | `mise run gate` exit 0 (Proof) | PASS |

### Diagnoses

- **Local evidence plan was unusable.** Symptom: hosted journeys failed to
  upload evidence on the local backend. Cause: `presign_single_put` returned a
  `file://` URL that no client can PUT to. Fix site:
  `wyrd_storage::service::evidence_upload_plan` returns the existing
  `UploadPlan::LocalFs` shape (same pattern as `/v1/cards/upload/local/{id}`),
  served by `PUT /v1/verification/evidence/local/{sha256}`, which verifies the
  digest before writing.
- **Denied tool-call error lacked its code.** Symptom: drift run 2,
  `hosted_tools_need_the_tool_and_domain_grants` expected
  `WYRD_PERMISSION_403`. Cause: `RecordedTool::invoke` recorded only the
  message. Fix: prefix with the `WyrdError` code (`b5be79e2b`).
- **Tool names and `code.run` arguments.** Cause: scripted provider turns
  used dotted names and `code`, but providers see `provider_tool_name`
  (underscores) and the tool schema names `source`. Fix: journey fixtures in
  all three SDKs (`5690c6452`, `a36c01870`).
- **Agent loop spans were missing from the tenant trace.** Cause: the
  behavior-span catalog in `wyrd-telemetry::export::in_catalog` omitted
  `invoke_agent`/`chat`/`execute_tool`. Fix: the catalog and design doc (`dd755c4fd`).
- **RunningQueryConflict (diag-drift-2, cause A).** Cause: every hosted
  Bifrost statement in one execution reused the caller's `request_id`, and
  Oracle keys running queries by (tenant, request_id). Fix:
  `hosted::statement_caller` mints a fresh request id per statement (`c4a31e8bd`).
- **No-domain-grant case could not resolve its Agent (cause B).** Cause: the
  fixture omitted `cards:read`, so the failure was Card resolution rather than
  the domain-grant denial under test. Fix: the fixture grants
  `tool:invoke` + `cards:read` in all three SDKs (`b3316b1d5`).
- **Unfitted baseline (cause C).** Cause: the scheduler skips a binding
  whose baseline is not ready (existing readiness rule, retained by the spec)
  and advances its cursor without an execution. The old test expected an
  execution. Fix: the journey asserts the skip through binding status
  (`b3316b1d5`). Its helper first sent `Authorization`. The Wyrd API reads
  `x-wyrd-access-token`, so the request got a 401. Fixed in all three
  SDKs (`50e4ed235`).
- **Python session cooldown leak (diag-py-vbb).** Symptom: three Python VBB
  tests fail (`answer-facts` skipped, `investigate` failed) only in the full
  file. Cause: PROVIDER-DOWN's 500 makes `DeploymentHealth` mark the
  (tenant, `openai`) deployment unhealthy for 30s (`wyrd-gateway`
  `engine.rs:633`, `health.rs:16`). Python shares one session server and
  tenant, while Rust and TS use one server per test. The OFF-SCHEMA and
  SLOW-READER cases were also passing for the wrong reason. Fix: the failed-reader
  test gets its own server, with PROVIDER-DOWN last (`a38d479cb`).
- **Delegated Python token audience.** Cause: `on_behalf_of` defaults to
  the `bifrost` audience, and the server rejects that audience on
  `/v1/verification/execute`. Fix: `audience="wyrd"`, matching Rust and TS (`a38d479cb`).
- **TS unfitted journey missing dependencies.** Cause: the test requested only
  the lazy `server` fixture, so `latency-baseline` and `latency-model` were never
  registered. Fix: register them first, as the Python fixture does (`a38d479cb`).
- **TS span start-time tie.** Cause: the OTel JS default start time has
  millisecond precision, and the server orders spans by
  `start_time_unix_nano, span_id` (`hosted/traces.rs:83`). Parent and child can
  therefore swap. Fix: explicit start times 1ms apart in `recordTrace` (`a38d479cb`).
- **Hosted `bifrost.query` result too large.** Cause: `max_rows` fails rather
  than truncates (`query/collect.rs` tool schema). The scripted query had no
  LIMIT and broke once the session retained more than one result. Fix:
  `LIMIT 1` in all three SDKs (`670ad206e`; confirmed by diag-py-vbb, which
  also notes a latent Rust/TS flake if a flush lands between investigations).
- **Missing `bifrost.enqueue` PRODUCER span from `observe.submit` (diag-gate-py,
  diag-gate-ts).** Symptom: the Python and TS action timelines held one
  producer span; the test expects two. Cause: the Window sample write ran
  inside the async submit, outside the binding's `traced` producer scope, so
  `transport::trace::producer()` reused the caller's span. Fix site:
  `wyrd-client` `Observe::submit` is split into a synchronous
  `stage` and an async `enqueue_staged`. Python and TS stage inside `traced`
  on the calling thread. Rust's signature is unchanged (`854a2f1cd`).
- **TS scheduled custom drift timed out (diag-gate-ts).** Cause: vitest shares
  file-scoped fixtures by name, so the runtime-enabled `observedService` reused
  the first test's server, which had no scheduler. Fix: the file uses one
  runtime-enabled server (`12cd91605`).
- **TS empty window timed out.** Cause: `BINDING_ACTIVITY_SQL` requires the
  owner to have authenticated recently. The TS test never acted as the Service,
  so the scheduler skipped the binding as `Inactive`. Fix: submit an empty window
  first, mirroring Rust `a36c01870` (`12cd91605`).
- **TS gateway round trip.** Cause: `ProviderDeployment.access` is always
  serialized (`deployment.rs:310`). Fix: expect `access: "open"`, as Python
  does (`12cd91605`).
- **`observe_ingest` x3 (diag-gate-rust).** Cause: the tests submitted
  measurements that the shared `observed-drift` samples slot does not declare.
  The client refusal (REQ-014) is correct. Fix: each test registers a copy of
  the fixture that declares its measurements. The shared fixture is
  unchanged (`12cd91605`).
- **`builtin_variant_columns_are_refused_before_ack`.** Cause: the test was
  repointed at `vala.verification.execution_inputs`, which the gate reserves for
  the server writer (REQ-015). Every frame was therefore refused before any
  Variant check. Fix: the raw frames target the client-writable
  `vala.logs.records` (`attributes`/`body`). On a signal table, a non-Variant
  type change is refused as `UnsupportedType` (`signal.rs:1020`), so that
  expectation changed with the table. The Scribe-ingress panic backtraces in
  this test come from `EncodedVariant::validate`'s intentional `catch_unwind`
  (`12cd91605`).
- **`delegated_input_reads_keep_history` (server bug).** Cause:
  `oracle_context` dropped `Caller::executed_by`, so a queued Verifier's
  trace-read decision recorded the delegating Service as actor instead of
  SYSTEM. Fix site: `AuthorizedQueryContext.executed_by`, an attribution-only
  field that is serde-optional, so forwarded peers keep it. It is applied in
  `query_audit_event`, set by `oracle_context`, and carried by
  `record_object_denial` (`d8a7db603`).
- **Rust custom drift `Inconclusive` in the gate (diag-custom-drift).**
  Symptom: 6 of 13 isolated runs failed in clusters. Evidence: the row's
  `created_at` was 6.6 ms past the window end, and Oracle file pruning opened
  zero files. Cause: `make_binding_due` set the exclusive window end to the
  Postgres VM clock. Observations carry producer `created_at` from the host
  clock, which ran about 50-60 ms ahead (colima). Fix site: the shared fixture
  `VerificationFixture::make_binding_due` sets the cursor to
  `GREATEST(statement_timestamp(), harness now)`. Dueness stays Postgres's
  decision, and the change covers every Rust, Python and TS caller. Afterward,
  6 of 6 runs passed.
- **Oracle `agent_runs_three_table_analytical_query_through_mcp`
  (intermittent).** A follower pod held 604 reserved bytes after teardown. The
  test passed when rerun alone. The cause is in the Oracle follower-stage
  memory release (`resources.rs:2797-2830`), which this change does not touch.
  The Monty prewarm load may widen the window. Not fixed here; it passed in
  the final gate.
- **CLI canonical journey count (11 vs 13).** Cause: the installed
  server did not ship the Monty worker. Fix: `2c55785e1`. Rerun passes.

### Non-goals

No per-request Postgres writes, new Card kind, new queue/renderer/delivery
loop, durable result outbox, synchronous replay ack, or history rewrite was
added. Files outside the write set: `wyrd-telemetry` span catalog and
`architecture/wyrd-design.md` (required for the trace-copy criterion).

### Risks for review

- `PUT /v1/verification/evidence/local/{sha256}` is a sixth verification route
  that exists only on the local backend. It serves the existing `UploadPlan::LocalFs`
  contract and follows the Card upload precedent. Reviewers should confirm it does
  not count as a new public API under the material stop.
- Hosted query audit rows carry per-statement request ids, not the verify request id.
- Recorded tool errors are formatted `CODE: message`.
- Enqueue skips the Card-scope check. A foreign tenant gets 202, and the execution
  later reads as not found.
- The enqueue inputs row has an empty `verifier_version`. Result queries have no
  time pruning. The execution view has no principal fields. The trace lookback is
  7 days, and trace anchors are approximate.
- The `kind` metric label was removed. The capacity YAML and `bench:capacity`
  (Linux-only opt-in) were not run.
- `wyrd-example-rs` copies the workspace DataFusion/Arrow patch pins.
- The external examples' live README quickstarts were not run. The
  `wyrd-example-ts` commits are not pushed.
- The TS `startTelemetry` docs list four OTel peers, but the SDK imports seven
  (pre-existing).
- The Agents SDK SIGINT handling can exit before `WyrdState.shutdown`.
- Trace documents order spans by `start_time_unix_nano, span_id`. A JS client that
  starts a parent and child in the same millisecond gets an arbitrary `spans[0]`.
  Breaking ties parent-before-child would be a server or spec decision.

### Proof

Candidate `03712af5e`. Postgres suites ran sequentially under one `flock`.

| Command | Result |
|---|---|
| `mise run gate` | exit 0, 2550 s. Includes wyrd-testing server suite 35/35, TS integration 31/31 files, Python integration 278 passed / 2 skipped, Rust SDK journeys, and `codegen:check`, `docs:check`, `check:deps`, `check:tenant-isolation` |
| Rust `scheduled_drift_alerts_operator::*` (custom drift), repeated after `03712af5e` | 6/6 (6 of 13 failed before the fixture fix) |
| `mise run examples:{rust,python,typescript}:support-desk` | Live mains exit `NoCredentials` with no `WYRD_SERVER_URL`/`WYRD_API_KEY`/gpt-4o deployment. Not counted as proof. The same code paths pass in the gate's SDK support-desk journeys with a deterministic wiremock provider |
| `npm --prefix ../wyrd-example-ts run typecheck` / `run test` (vendored candidate build) | OK / 5/5 |
| `scripts/postgres/with-test-postgres.sh -- uv run --directory ../wyrd-example-py pytest` (rebuilt wheel) | 5 passed |
| `scripts/postgres/with-test-postgres.sh -- mise exec -- cargo test --manifest-path ../wyrd-example-rs/Cargo.toml -- --ignored` | 1/1 (`answers_are_verified_now_and_in_the_background`) |

Limit: the README quickstarts were not run against a live provider. The
vendored candidate builds in the external repos are local and not committed.
