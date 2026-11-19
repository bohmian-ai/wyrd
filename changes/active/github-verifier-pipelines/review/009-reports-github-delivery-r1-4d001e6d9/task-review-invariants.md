# TASK-009 invariant review

Result: **FAIL**.

Subject: base `d07c55158`, candidate `4d001e6d9ed3daf88bf81995510319373bd8c353`, full first-review range. Approved spec revision 10 and original `tasks/009-reports-and-github-delivery.md`. HEAD was checked against the candidate. No prior acceptance evidence carried forward. The working-tree task index and tutorial were excluded. Static review only; no Cargo commands or source edits.

## Scope and independent expansion

Started with `scope.md`, then expanded through CodeGraph and source to the actual producers of final `StepResult`, cancellation SQL and its callers, lease renewal, accepted-state revision fencing, inspection status projection, result assembly/readback, provider Check update bodies and the mock consumer. Read the task-review skill, root instructions, SQL directory instructions, spec-driven workflow, maintainer guide, and relevant Prompt/PR-delivery and Bifrost execution-event authority. This report covers the assigned shared lifecycle boundary; it does not claim independent full SDK, security or source-anchor parser coverage.

Navigation and ownership:

- Engine `blocks/engine.rs::member`, `aggregate`, `decide_speculation`, `reported` produce final logical outputs; `verification/results.rs::ResultPayloadBuilder::build_as` retains them unchanged; `InputResolver::result` reconstructs `OutcomeDetails`; `VerificationControl::presented` reads the accepted selected result; `select_report` consumes those steps.
- `RunState` owns accepted checkpoint revisions and `revision_fence`; SQL queue rows own status/claim fencing. `Retained::status` combines these authorities. Manual cancellation and principal cancellation can change SQL status without changing the accepted checkpoint revision.
- Runner settlement and startup recovery independently start `GithubDeliveries`. Its Bifrost fence owns the outcome for one execution/revision. `Write::send` owns Check lookup/update and review lookup/create. `allowed` briefly takes the existing PR advisory lock, but commits before lookup or write.
- Execution HTTP/MCP inspection calls the same `recorded` reader using its accepted revision. `InputResolver::checkpoint` now filters `event = 'checkpoint'`, preventing a delivery row at the same sequence from being interpreted as checkpoint data.

Reuse assessment: the implementation appropriately extends existing engine results, inspection, tenant query authority, Scribe fenced writes and GitHub transport rather than introducing another result store or verifier engine. Final logical-output selection belongs at the existing report contract and does not justify a second traversal of checkpoint invocations. The new background delivery owner is necessary provider presentation work. Its lifecycle must, however, respect the existing split between SQL cancellation and Bifrost revision, and all its writers must share coordination; a leaf-level completed-status lookup does not provide that coordination.

## Scoped acceptance matrix

| Obligation | Implementation evidence | Verification evidence inspected | Result |
|---|---|---|---|
| REPORT: zero/one/multiple/malformed envelopes; consumed envelope still counts; no verdict mutation | `report.rs:459` scans successful final steps, returns ambiguity before selecting; delivery consumes selected result separately from verdict | `report.rs::selection_counts_each_retained_final_output` asserts ordinary outputs ignored, fanout indices retained, consumed and fanout envelopes ambiguous, malformed invalid; PG selection journey asserts verdict remains passed with report errors | FAIL: report selection is suppressed without extraction, INV-009-3 |
| REPORT: only retained fanout items/final repeat output; failed attempts/discarded speculation excluded | `engine.rs:1522` retains `Member.last`; `aggregate:2574` emits final member outputs only on aggregate success; `decide_speculation:1194` removes output for discarded/blocked contributions; `reported:2490` restores only admitted successful state output | PG selection journey at lines 336–528 exercises actual engine fanout, repeat and rejected speculative approval; assertions at 521–526 name both fanout candidates, final repeat summary, retained speculative sibling and same Check id | PASS |
| DELIVERY: reads accepted result/source without recomputing or blocking verdict; uses canonical outcome custody | `runner.rs:1530` spawns after settlement; `presented:580` uses `RunState::accepted` and selected result; delivery writes through Scribe fenced execution_events; no report payload in SQL | PG restart journey checks Check/review counts remain one after restart; source shows no engine call from delivery | PASS within scoped healthy/restart path |
| DELIVERY: current outcome revision and queue status remain consistent through cancellation | `Retained::status:248` can change from waiting to cancelled solely from SQL; `deliver:295` skips any recorded checkpoint revision | No journey cancels an already-delivered waiting Check and checks final provider status, including restart | FAIL: INV-009-1 |
| RENDER: completed Check never becomes in_progress; one Check per execution except accepted lost-answer duplication | `Write::send:144–159` uses lookup then unconditional update/create; background spawn and recovery are independent writers | PG selection journey verifies sequential waiting→completed update at lines 525–526; lost-answer test verifies one POST after committed timeout. Neither interleaves two delivery writers | FAIL: INV-009-2 |
| DELIVERY: current generation checked; bounded retry and lookup after timeout | `attempt:446` rechecks `allowed`; `allowed:518` reads current PR generation under existing lock; `Write::send` performs fresh lookup each attempt; fixed ATTEMPTS | PG new-head/access-loss journeys assert later writes absent and skipped outcomes; cooldown/lost-answer journey asserts one Check/review and one Check POST | PASS for inspected generation/retry logic; access authorization audited by separate selected specialist |
| INV-004/SURFACES: fenced-but-invisible delivery is unavailable, absence is pending, independent of verdict | `recorded:337` checks committed batch before query; missing visible row after fence returns unavailable; `service.rs:301` exposes recorded outcome or pending; `inputs.rs:304` filters exact event/revision | PG inspection assertion compares HTTP and MCP at lines 317 onward; unit key test asserts stable per-execution/per-revision identities | PASS for read semantics; cancellation staleness remains INV-009-1 |
| Non-goals: no new engine, result mirror, SQL table/column or verdict calculation | Source/diff of delivery, runner, SQL and input reader; provider renderer consumes supplied verdict | Static diff and producer-to-consumer trace | PASS |

## Proposed findings

### INV-009-1 — INCORRECT: delivered waiting revision permanently masks later cancellation

Violated obligation: REQ-013 maps cancelled executions to a completed cancelled Check; REQ-014 keeps recoverable delivery and separately accurate inspection. Task RENDER requires the cancellation mapping, and delivery must present the execution's current state.

Changed location: `crates/wyrd/wyrd-server/src/components/github/delivery.rs:295–300`, its revision-only fence/key at `337–366` and `595–606`, and the only live spawn at `verification/runner.rs:1530–1537`.

Reachable producer-to-consumer path:

1. A PR execution parks for evidence/approval with accepted checkpoint revision N. Delivery writes an in_progress Check and records the delivered outcome for N. This is the actual supported waiting path exercised by the selection journey at lines 415–459.
2. The authorized execution cancellation endpoint calls `VerificationControl::cancel` (`service/inspection.rs:1015–1041`). `VerifierExecutionQueue::cancel` (`verifier_executions.rs:3175`) executes `CANCEL_SQL:607`: status becomes cancelled and lease is removed, while recorded state is retained. It neither advances Bifrost revision nor schedules GitHub delivery.
3. A parked execution has no running worker to settle it. A running worker loses its lease through `leases.rs:175–185`; runner `LeaseLost` settles as stale_lease and is excluded from the delivery spawn condition. Cancelled SQL rows are not runnable.
4. Even restarting or explicitly reaching `deliver` cannot repair the parked case: `Retained::status` now returns cancelled from SQL but `RunState::accepted` still returns N. `deliver` sees N's existing waiting-delivery fence and returns before rendering cancelled. Inspection likewise reads N's delivered result and reports delivered for the cancelled execution.

Sibling writers checked: manual cancellation, `cancel_principal` SQL (same preserved state), PR deactivation/supersession and runner terminal settlement. PR deactivation intentionally suppresses obsolete current-generation writes, so it is not evidence that manual cancellation is allowed to leave a current Check waiting. Ordinary runner completion/failure does persist a new state and is a different path.

Consequence/materiality: a person cancels a current PR review and receives cancelled status while GitHub remains in_progress indefinitely, including across restart, with delivery inspection claiming delivered. This violates an explicit user-visible mapping on a routine supported operation.

Smallest safe correction boundary: join cancellation's existing committed lifecycle to background delivery, and make the canonical delivery version distinguish a terminal SQL transition from an already-delivered waiting snapshot. Prefer the existing accepted-state/cancellation owners over a new progress ledger or result mirror; do not merely add a spawn call, because the revision-only dedup would still suppress it. Keep cancellation's lease fence and response latency semantics intact. If fulfilling the architecture's per-accepted-revision rule requires recording the cancellation at the existing state owner, do that through its existing protocol, rather than adding an unrelated SQL delivery column. The exact resolution of that authority seam should be checked by the selected data specialist before task packaging.

Focused proof: park a real admitted current PR execution, await its in_progress Check and delivered waiting outcome, cancel through the public endpoint, assert the same Check becomes completed/cancelled and inspection reflects that terminal delivery; restart and assert it stays cancelled with no duplicate Check. Also cover cancellation before any waiting outcome exists. No new harness is needed.

### INV-009-2 — INCORRECT: completed-status lookup is not monotonic across delivery writers

Violated obligation: TASK-009 RENDER and `architecture/wyrd-design.md:2540` explicitly require that a completed Check never returns to in_progress. REQ-013/014 require one recoverable Check per execution.

Changed location: `components/github/delivery.rs:144–159` and independently spawned writers at `191–199`, `230–232`; current-generation lock is released at `528` before provider lookup/write. `GithubApp::update_check_run` at `app.rs:735` unconditionally PATCHes the supplied status/body.

Reachable interleaving:

1. A waiting delivery W renders revision N. It looks up an existing in_progress Check and retains that result at line 150.
2. The decision/input arrives and the same execution completes, producing revision N+1 and an independent completion delivery C. C looks up the same Check and PATCHes completed.
3. W resumes its PATCH with the previously rendered in_progress body. The completed guard at line 154 tests W's old lookup response, so it does not prevent that write.
4. C already has a fenced delivered outcome for N+1. Restart sees that outcome and skips correction. GitHub can remain in_progress while inspection says completed/delivered.

W and C do not conflict in `record`: their batch keys deliberately differ by revision. Both pass `allowed` because this is the same active PR generation. Startup recovery can also overlap runner spawning on the same revision; two lookups can both observe absence and create duplicate Checks/reviews without any lost answer. The mock consumer at `wyrd-testing/src/github.rs:1357–1375` merges PATCH fields without a monotonic precondition, so the described stale update is directly observable on the repository's supported protocol transport.

Consequence/materiality: normal approval/evidence continuation can restore a pending Check after the verdict has been completed and delivered. This is an explicit lifecycle promise, not speculative input hardening. Serial test observation of waiting then completion does not prove concurrent monotonicity.

Smallest safe correction boundary: coordinate all provider delivery writers for the same execution through a shared owner that works across replicas, with a fresh current presentation/outcome read inside that ordering before lookup/write. Compare extension of the existing PR coordination owner before adding any new lock system or durable ledger. A second completed-status GET alone cannot close the check-then-write window. Preserve already-issued IO/supersession semantics and do not hold an engine permit during provider waits.

Focused proof: use the existing local protocol transport to pause an older waiting delivery after it has observed in_progress but before its PATCH, complete the execution and deliver the terminal Check, then release the older attempt. Assert the final remote Check remains completed and restart does not leave an incorrect provider state. Also overlap recovery and live delivery on an absent Check and assert no ordinary healthy-race duplication. This is a narrow delivery-concurrency proof, not a new verification journey suite.

### INV-009-3 — MISSING: report selection is silently discarded when extraction is absent

Violated obligation: REQ-012 requires selection from accepted successful outputs per execution, with ambiguity and malformed envelopes visible; TASK-009 explicitly forbids a mandatory preparation step. REQ-013 keeps unlocated findings in the Check summary.

Changed location: `components/github/delivery.rs:389–408`, specifically `extraction.as_ref().map(...)` at 405, plus `presentation.rs:202–209`, which calls report rendering only through that optional combined object.

A current PR execution may use normal code to emit a standard report without a pull_request extract step, or may conditionally skip extraction. Such a Verifier is accepted by existing authoring/engine rules; PR admission supplies source identity independently of having an extraction action. `select_report` correctly returns One, Invalid or Ambiguous from its accepted final outputs. Delivery then searches the final outputs for a deserializable extraction and, when none exists, passes `completed = None`. `Presentation::render` never examines the selection, produces an ordinary empty report section, and `deliver` records delivered with no report error. Thus unsupported/malformed or multiple envelopes disappear entirely; even a valid report with only unlocated findings loses its summary and findings. No anchor validation is needed for those unlocated findings.

Sibling consumers checked: the pure selection helper works independently of source, but `Completed` combines a mandatory extraction with the selection and has only this delivery producer; the PG verifier fixture at `pg_github_delivery.rs:32–35` always inserts extraction, so all current report-selection journeys bypass this gap.

Consequence/materiality: the documented optional report convention acquires an implicit extraction prerequisite, and required delivery errors become clean-looking success. This is an explicit report contract failure for normal code output, not a request to support invented source anchors.

Smallest safe correction: pass report selection independently of optional extraction to the existing renderer. Always show ambiguity/invalid-envelope errors and unlocated summary/findings. Keep retained extraction mandatory for accepting any supplied source anchor or suggestion; missing source must remain a visible refusal for anchored findings, never an invented location. This is a local change to the existing presentation owner and its single producer, without a new selector or report storage.

Focused proof: an admitted PR Verifier without extraction returns (a) an unlocated valid report, (b) a malformed envelope and (c) two envelopes. Assert report summary/findings or the respective explicit delivery error while verdict/conclusion remains authored. Add a renderer check showing an anchored report with absent retained source never becomes an inline comment.

## Verification limits and requested checks

No commands that build or execute project code were run, as assigned. Existing assertions above were inspected, not treated as new runtime proof. The orchestrator reports `verification.md` now records 4+7 unit checks, 29 PG tests, 5 CLI tests, and format/codegen/lints/Python typing/tenant gates passing. Those checks were run outside this reviewer; the missing interleavings and cancellation/no-extraction paths above still require focused closure proof. No broader Cargo, benchmark or SDK rerun is requested by this scoped static review.

Skipped full security, source-anchor parser, SDK parity and deployment resilience audits because separate selected specialists own those scopes. The material risks are stale pending Checks after cancellation/concurrent completion and silently omitted report errors without extraction.
