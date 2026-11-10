# System resilience review — TASK-001

Result: **FAIL**. Three material proposed findings; independent validation remains required.

Subject: base `f4e79ea00523af5cd9212fdacde64f16c97ad554`, candidate `365bfc94bfef1a153f213462f7ef104afa823741`. First review, complete cumulative range. HEAD remained the recorded candidate at the end of source inspection. Only this report was written. No other reviewer report or implementation summary was used as proof; the task's recorded command outcomes were treated as reported verification, not independently executed results.

## Scope and authority

Reviewed admitted queued work, runner crash/restart, dependency failure, cancellation/revocation, exact-version availability, immutable publication acceptance/replay, live reads and historical event recovery. Read the review skill and navigation, task, relevant specification sections REQ-013/014/046–049 and output-publication supplement, root instructions, SQL directory instructions, spec workflow, maintainer guide, operations README, and owning Verifier/Bifrost architecture sections. Used CodeGraph before source navigation. This is a system specialist review; security/data/concurrency details outside these failure paths belong to their selected reviewers.

## Deployed topology and propagation

- Public server admission stages the execution input on the process Scribe outbox. Local Scribe or an authenticated Scribe peer acknowledges durable input; `SubmissionSink` then batches SQL admission and answers its `AdmissionReply` only after commit (`verification/submissions.rs::write`, service.rs:770–786). Losing the request reply after commit leaves the keyed execution recoverable.
- A composed `VerificationRuntime` supervises Scheduler, Runner, Fitter and OperatorWorker separately (`verification/mod.rs:308–405`). The Runner owns claim work and joins lease renewal and event projection. Claims, checkpoints and cancellation use tenant transactions; hosted blocks share process capacity but build per-execution authority/workspaces. A capability crash degrades its health slot and restarts that capability, rather than exiting the shared server. A dropped runtime aborts work and leaves durable leases for expiration/reclaim.
- The runner resolves the exact Verifier/subject, reads acknowledged input through Oracle, loads SQL invocation settlements, and supplies the same engine with `ExecutionCheckpoints` (`runner.rs:450–575`). PostgreSQL fixes the deadline at first claim. Unsettled action attempts count toward step retry budgets, and committed action/check settlements bypass dispatch on recovery.
- Publications freeze scratch under Workspace's writer exclusion, complete Files through existing storage and inventory, then accept one SQL manifest and event under the execution fence (`hosted/publications.rs`). Object/inventory failure is scoped to this publication; no transaction spans storage IO. Unaccepted bytes are not a named publication. Publication/event metadata is independently inspectable through tenant SQL pages while historical projection lags.
- `EventProjector` reads SQL-assigned batches, stages them on the existing Scribe outbox, and marks ACK only after Scribe acknowledgement (`history.rs:67–155`, SQL events.rs). Stop/crash before SQL ACK preserves batch/event identities for replay. Scribe batch dedup is reused. This durable replay applies to execution events, not final Result writes (SYS-001).
- Cancellation commits status and fences future invocation/publication/final SQL writes. Principal suspension and affected execution cancellation share the revocation transaction (`wyrd-auth/src/revoke.rs`, `inspection.rs:560–585`, SQL cancel methods). Renewal discovers removed tokens and drops in-flight futures. Late action responses cannot settle accepted work. Completed executions are unchanged. No process-wide fail-closed action is needed or justified.

## Failure-path and proof matrix

| Failure / required outcome | Source assessment | Proof assessment | Result |
|---|---|---|---|
| Admission DB failure or response loss; no ACK before recoverable row | Input ACK → batched SQL commit → AdmissionReply; failures leave reply unanswered | Rust `queued_acceptance_survives_response_loss` covers delayed admission, keyed replay and restart | PASS |
| Crash after action commit, before requirement settlement | SQL invocation begin/settle; engine `with_checkpoints`, `restored`, `recorded`; fixed `deadline_at` | Rust `action_checkpoint_survives_unfinished_requirements` counts review once/judge twice; SQL checkpoint/reclaim test adds direct fence proof | PASS for action/checkpoint recovery |
| Crash after final SQL commit before Scribe result ACK | Final result only staged in process memory; completed rows are never reclaimed | Journey explicitly flushes Bifrost before final restart; misses vulnerable interval | FAIL: SYS-001 |
| SQL/Oracle/model/storage outage during ordinary work | Input reads defer/retry; checkpoint failure propagates through engine; lease renewal cancels on loss; storage acceptance fails before manifest | Recorded targeted proofs cover representative failure/fence paths; no claim of exhaustive outage testing | PASS for inspected component failure containment |
| Lost publication reply; same-call replay after scratch loss or dependency outage | `path` freezes and completes content before SQL replay lookup | Replay proof repeats with same intact workspace/storage; does not exercise original receipt recovery elsewhere | FAIL: SYS-002 |
| Upload interrupted before manifest acceptance | Manifest acceptance occurs only after complete File/inventory writes; current lease still required | `publication_replay_preserves_attempt_identity` injects local upload failure; cancelled/stale lease proof verifies no late accepted entry | PASS for all-or-nothing visibility |
| Cancel/revoke while running, or late completion | SQL status/token fences; shared revocation commit; renewal cancels work; accepted records preserved | `cancel_and_revocation_fence_unfinished_work` exercises active/pending revocation and late completion; backend cancellation test checks checkpoint/publication writes | PASS |
| Exact referenced Agent/Prompt deleted before unfinished check | Registry refuses deleted Card; engine converts refusal to an ordinary inconclusive judgment and completes | No deletion/recovery proof in the cited cancellation journey | FAIL: SYS-003 |
| Rolling worker compatibility | `worker_format` filters due and expired reclaim candidates | Backend proof checks a newer-format running row is not returned by claim; no broad rolling-upgrade claim made here | PASS for explicit no-claim behavior |
| Event projection paused, ACK lost, then projector recovery | Stable batch assignment and event IDs survive outside memory; SQL pages do not require projected events | `events_replay_without_hiding_live_progress` uses separated roles, pending live SQL reads and ambiguous-ACK dedup | PASS |
| Shared server impact | Errors remain execution/publication/projector scoped; runtime supervision restarts failed capability; ordinary API/other capability tasks remain running | Source topology plus existing supervisor semantics; no process-kill recommendation | PASS |

## Proposed findings

### SYS-001 — MISSING: final completion is not recoverably durable

Obligation: approved spec REQ-013 explicitly strengthens existing settlement semantics: final completion means the result is durable, and response loss after commit remains recoverable. TASK-001 scenarios 2/8 require final committed readback and durable completion.

Locations: `crates/wyrd/wyrd-server/src/verification/runner.rs:674–754` (single completion), `verification/completions.rs:198–226` (batched completion), `scribe_outbox.rs:5–32` (memory-only staging), `components/verification/service.rs:260–280` (result read).

Reachable failure: complete a queued execution while Scribe is slow/unavailable. Both settlement paths call `queue.complete[_many]`, put `ScribeWrite::Result` into a reserved in-memory outbox slot, and commit SQL without waiting for durable result acknowledgement. Kill the process after that SQL commit and before Scribe accepts the result. The completed row and selected result ID survive; the only final payload disappears. Claim SQL only recovers pending/retrying/expired-running work. The event projector retains status metadata, explicitly not results. Public `get_execution` reads only the selected Bifrost result and therefore keeps returning completed with no result; SDK wait eventually times out. This is permanent evidence loss, not projection lag, and can also leave an Operator dispatch referencing unavailable evidence.

Shared-owner check: both the group completion flusher and runner fallback share `completions::stage`; neither has an ACK or durable recovery handoff. The new SQL event recovery owner cannot reconstruct the missing final payload. Adding a consumer retry cannot recover vanished bytes. The healthy recovery test flushes Bifrost before its final restart (Rust durable work journey:197–204), so its PASS does not cover this path.

Authority discrepancy: `architecture/wyrd-design.md:1837–1839` still says completion never waits for Scribe and may lack retained results. That is the prior behavior REQ-013 expressly strengthens, and the specification explicitly requires amending owning architecture before implementation. The candidate retains the old limitation rather than delivering that approved strengthening. Reconcile this owner text as part of the correction; do not silently treat it as permission to discard the approved outcome. If the orchestrator considers the unreconciled owner controlling, this row requires authority correction rather than an implementation PASS.

Smallest correction boundary: existing runner/completion/Scribe handoff, preserving batching, exact result identity, lease fences, non-blocking audit, and unrelated server availability. Make the selected result recoverably durable before completed can commit, reusing Scribe acknowledgement or the existing committed recovery ownership rather than introducing a second ingestion service. Failure must leave this execution recoverable rather than kill the shared server. A final-result consumer guard is insufficient.

Focused proof: hold result ingress before durable Scribe ACK, allow action/check settlements, interrupt at the completion boundary and restart over the same Postgres/store; require either no premature completed state or recovery of exactly the selected result with no repeated committed model work. Do not flush or drain the vulnerable write before interruption.

### SYS-002 — INCORRECT: acknowledged file publication replay depends on scratch and fresh uploads

Obligation: output-publication.md:45–65 and TASK-001 scenario 5 promise the original same-call/arguments/attempt receipt after reply loss, including retry elsewhere; private scratch is mutable and receipt identity is durable.

Location: `crates/wyrd/wyrd-server/src/verification/hosted/publications.rs:115–149` (`path`), `:205–253` (`accept`), SQL `verifier_executions.rs:684–719` and `:3448–3495`.

Reachable failure: a trusted hosted caller successfully commits a path publication but loses its acknowledgement. Retry the same PublishCall/producer/name/path under the current valid lease from another workspace, or after the original private file has become unavailable. `path` calls `workspace.freeze(path)?` before consulting SQL, so it refuses rather than returning the recorded receipt. Even with intact scratch, storage/inventory outage causes a fresh `complete` to fail before the durable replay identity is checked. On successful changed scratch, it unnecessarily uploads the new bytes before discovering replay. If file/directory kind changed, `accept` returns the newly computed kind rather than the original accepted kind. The durable manifest itself remains immutable, but the required recoverable receipt is not delivered.

Shared-owner check: JSON and path calls meet in `accept`; SQL already retains the original call ID, canonical argument digest, kind and manifest. Reuse that identity at the publication owner. Existing replay proof repeats the path twice with the same intact workspace and available storage (`pg_durable_verifier_work.rs:523–554`); it proves no duplicate named row, not recovery independent of scratch/upload.

Materiality: reply loss/retry elsewhere is an explicit primary recovery scenario, and an already accepted publication can be reported failed solely because unrelated transient upload dependencies are down. This is not a request to preserve uncommitted scratch across crashes.

Smallest correction: validate current fence and look up same-call/name/argument replay at the existing SQL publication owner before freezing/uploading; return the original retained receipt/kind. Preserve same-name and changed-arguments conflicts, stale-lease refusal and the current upload-before-new-acceptance order. No new queue or storage layer is needed.

Focused proof: accept a file/tree, discard its acknowledgement, remove its private workspace or supply a fresh workspace and make storage unavailable, then replay the exact call under a valid lease. Require its original kind and exactly one accepted manifest without a new upload. Keep fresh-attempt and stale-lease cases distinct.

### SYS-003 — MISSING: unavailable referenced capability becomes a completed judgment

Obligation: REQ-014 and AC-025, TASK-001 scenario 7: deleting the exact Agent or Prompt before unfinished work starts must end the execution errored, preserve checkpoints, and never substitute versions.

Locations: `crates/wyrd/wyrd-server/src/verification/runner.rs:351–377` (claim checks Verifier/subject), `verification/hosted/registry.rs:47–58,102–110` (resolver error mapping), `crates/vala/vala-verify/src/blocks/engine.rs:745–780,951–957` (unfinished requirement dispatch/conversion).

Producer-to-consumer trace: SQL Card `SELECT_BY_REF` excludes deleted rows (`wyrd-sql/src/queries/cards/get.rs:20–27`); `components/cards/service.rs:163–184` and routes `get_card_spec_by_ref_for` propagate not found. TenantRegistry turns that 404 into `BlockError::Terminal`. `AgentExecutor::resolve` propagates it. For an unfinished Agent requirement, `judge` treats every non-Busy executor error as `Judged::inconclusive`; the new checkpoint code commits that requirement, and runner accepts the resulting engine outcome as Completed. Action invocation similarly turns terminal block errors into ordinary failed step results. No recovery preflight for required referenced Agents/Prompts exists in runner; exact Verifier resolution does not cover these references.

Reachable scenario: run a Verifier whose action committed, interrupt before its Agent check, delete the check's referenced Agent or Prompt, and restart. Committed work restores correctly, but the deleted capability produces a completed execution with an inconclusive requirement instead of an errored execution identifying unavailability. Depending on the ordinary decision rule, sibling successful requirements may even satisfy a pass-rate threshold. The same missing boundary affects normal queued dispatch after deletion while waiting. No version substitution is needed to demonstrate the failure.

Smallest correction: preserve the existing ordinary model/code failure judgment semantics, but distinguish exact required-capability unavailability at the existing hosted resolution/queued engine boundary and terminate this execution errored with retained checkpoints. Reuse the same availability path during normal execution and recovery; do not add a separate recovery executor, general process shutdown, or broadly turn every model failure into an execution error.

Focused proof: reuse the action-checkpoint journey, delete the exact unfinished check's Agent and separately its Prompt before restart, and assert errored/unavailable, no provider call, unchanged committed action/checkpoint/publications. Include an action reference case and verify ordinary provider/check failure still follows its existing inconclusive/failed-step behavior.

## Limits

Static source/diff review only; no Cargo jobs, fault injections or platform runs were executed by this reviewer. The task records targeted test and gate results; source inspection identifies missing fault boundaries those tests do not cover. No source edits or new runtime machinery were proposed outside the existing owners. Cross-tenant authorization, SQL concurrency proof and full public-surface parity remain with their selected specialists.
