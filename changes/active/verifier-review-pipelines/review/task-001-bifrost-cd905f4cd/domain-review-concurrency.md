# Concurrency boundary review

Result: **FAIL** — one material proposed finding, CONC-01.

Subject: full range `0e8a31b14d336fadf62e1d8525a7fa030ab6c4eb...cd905f4cd929785bda25696326d58b4a4ae88dee`. HEAD remained the candidate when checked. This is an independent source review, not acceptance of implementation summaries or another reviewer's conclusions.

## Boundary and authority coverage

Reviewed the changed execution-state lifecycle from memory mutation through lease confirmation, freezing, shared pending acceptance, reserved outbox staging, frame acceptance, coverage release, takeover and replay suppression. Also traced unchanged engine dispatch, runner cancellation, queue cancellation and shared outbox reservation/lane dependencies.

Authorities: root AGENTS; SQL nested AGENTS; spec-driven-development authority order; maintainer and Rust guides; TESTING; approved spec revision 18 REQ-052–059 and INV-017–019; approved durability supplement revision 2, especially its acknowledgement and stale-worker protection sections; original TASK-001 scenarios 1/3/4/5/6 and Revision 1 R1.1–R1.3; Bifrost design execution-checkpoint owner contract at lines 467–486.

CodeGraph was used before source search, for `RunState`, `StateScribe`, `Pending`, publication and terminal callers. Source examination then expanded into actual consumers and unchanged owners.

| Obligation / boundary | Implementation evidence | Verification evidence inspected | Result |
|---|---|---|---|
| Sibling memory transitions proceed during Scribe waits; no state mutex spans IO | `verification/state.rs:611–699`: mutation/ticket and frozen-clone bookkeeping have separate lock scopes from confirm/staging/ack | `siblings_record_progress_while_a_revision_is_held`; source assertions check both memory membership and separate accepted revisions | PASS |
| One immutable pending revision and exact mutation coverage | `Held`, `Pending`; `durable` freezes `covers = changes`; matching revision advances `covered`; later tickets loop into another revision | `held_changes_share_the_next_revision_across_cancelled_waiters`; `batched_checkpoints_recover_exactly_the_accepted_batch` | PASS |
| Dropped waiters leave other callers able to drive pending work | Shared boxed acceptance is stored in `Held`; its capture is `Arc<StateScribe>`, not `RunState`; no per-run background writer | Cancellation unit test drops first and second waiters; third drives both revisions | PASS |
| Same-call publication / terminal identity and success await accepted coverage | `select:476`, `publication_receipt:502`, `accept_publication:521`, `receipt_once_accepted:568`: shared memory authority; duplicate replies call `durable` | `duplicate_selections_share_one_accepted_result`; publication/recovery integration source | PASS for inspected identity/coverage races |
| Reserved staging, byte accounting and same-execution lane ordering reuse existing owner | `StateScribe::write:245–300`; shared runtime `outbox.rs:475–520`, `send_reserved:545`, `Reservation::stage:699`; sink lane:1775 | Outbox source confirms reservation queue cancellation cleanup, retained staged payload, reserved byte charge, shutdown sender drop; existing server unit selectors cover lanes | PASS |
| Own-frame ack does not await unrelated frames/group scheduling | `scribe_outbox.rs:1259` gives each state write its own keyed frame; `tally:1709` sends its ack; `write:1847` schedules groups later | `a_revision_is_acknowledged_at_its_own_frame` | PASS |
| Refused/uncertain state releases no success | `durable:683–698` records failure before returning it; `broken` checked before subsequent coverage release | `a_refused_revision_releases_nothing` | PASS |
| Replacement and expected-next acceptance order; rejection stays rejected on replay | `scribe_state_heads::claim/advance` serialize on head; batch `record` couples advance to fence; Scribe `commit_batch_control_fences`; replay suppresses unfenced state rows at `shards.rs:1882–1910` | `batched_checkpoints_recover_exactly_the_accepted_batch`; takeover/replay proof source | PASS for replacement ordering |
| Cancellation / expiry remain authoritative at acceptance, including already-frozen progress | Pre-freeze lease confirmation is disconnected from later head advance; queue cancellation never invalidates head | Existing cancellation test cancels before the next `begin`, not during its pending acceptance | **FAIL: CONC-01** |

The existing `RunState` and shared `Outbox<ScribeSink>` are the appropriate owners for memory batching and physical delivery. The revision extends those owners rather than introducing a competing writer. Whole-workflow comparison found no need for another queue, task or scheduler. The remaining defect is authority ordering across existing owners, not a reason to replace either owner.

## CONC-01 — A queued checkpoint can be accepted after cancellation commits

Classification: **INCORRECT**. Violated obligation: REQ-057 and the supplement's explicit rule that cancellation, lease validity and original deadline remain enforced at checkpoint acceptance, including when no replacement has run. Bifrost design lines 481–486 repeats that requirement. Revision 1 selected correction 7 preserves it.

Changed root locations: `crates/wyrd/wyrd-server/src/verification/state.rs:646–679` and `:245–300`; `crates/vala/vala-sql/src/queries/scribe_state_heads.rs:139–155`. Related producer/consumer: queue `CANCEL_SQL` at `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs:612–619`, runner `state` at `runner.rs:485–492`, runner cancellation selection at `runner.rs:1005–1019`, and engine external dispatch at `crates/vala/vala-verify/src/blocks/engine.rs:846–865`.

Reachable failing order:

1. A live queued writer changes memory for an invocation start, publication or terminal selection. `durable` successfully confirms the queue lease, freezes a revision and stages it through the outbox.
2. Hold the revision before Scribe accepts it, using the existing held-revision seam; an ordinary capacity wait, network delay or retry produces the same ordering in supported deployment.
3. A user cancellation commits. `CANCEL_SQL` changes the queue row to `cancelled` and clears `lease_expires_at`, but changes neither `scribe_state_heads.writer` nor any state acceptance authority.
4. Release the staged revision before its original execution deadline. `ScribeStateAdvance.not_after` is `run.deadline_at`, not current queue authority. `advance` checks only head writer, revision and that bound. The writer still matches and the next revision therefore commits and receives success.

The broken ordering requires neither lease expiry nor a replacement and can occur within milliseconds. Runner cancellation cannot retract the staged write: its lost token is observed through renewal/confirmation, and staging has transferred the immutable payload to the outbox. The checkpoint can therefore become recoverable after cancellation even if the engine waiter is dropped. If the start waiter remains active before the next renewal notices cancellation, the successful `begin` also releases external dispatch (`engine.rs:846–865`). Publication or terminal checkpoints can likewise add accepted content/selection after cancellation. This violates the mandatory stale-authority boundary; it is not a hypothetical retry policy change.

Sibling writers/consumers checked: `begin`, `forget`, `settle`, publication receipt/acceptance and terminal selection all share `durable`; queue explicit cancellation and principal cancellation both only update the queue table; periodic lease renewal / `HeldLease::confirm` cannot remove an already-staged write. Replacement claim does invalidate a writer through the state head, so this is specifically the missing invalidation/current-authority ordering before replacement. Original deadline checks alone do not close cancellation. Lease expiry before acceptance has the same conceptual gap when the original deadline permits it, but cancellation is the shortest independently sufficient proof.

Smallest correction boundary: preserve the existing queue as authority and existing Bifrost head/fence acceptance as the only state commit point; connect cancellation/lease validity to that commit ordering through the declared owner capabilities. A queue authority transition must prevent any subsequently accepted checkpoint from that token, including a checkpoint already frozen/staged, while a checkpoint accepted first remains retained. Reuse head/fence invalidation or an owner-provided authority fence at the acceptance boundary; do not add a second scheduler, copy the checkpoint into queue SQL, or import private cross-owner query modules. Another pre-stage/pre-ack application check cannot establish the required ordering and is insufficient. This is required owner-extension behavior already approved by REQ-057, not an additional product policy.

Focused proof: extend the existing `HeldRevisions` / real Scribe integration seam to freeze and stage a queued revision, commit `queue.cancel`, release the revision, and assert refusal, unchanged accepted head/state, no successful publication/terminal reply and no newly dispatched external invocation. Restart/replay the existing Scribe to assert the refused revision remains absent. Retain a converse acceptance-before-cancel assertion so cancellation does not erase already accepted custody. This needs no new harness or synthetic load.

## Verification limits

No Cargo commands, tests, benchmarks or source changes were run, as assigned. Named tests were inspected as source; historical green command claims are not independent execution evidence. The current cancellation test at `pg_durable_verifier_work.rs:1240` confirms cancellation *before* calling `begin`, so it cannot refute CONC-01. The held revision/reclaim test validates replacement ownership, which is a different ordering from cancellation without replacement.

No public API, SDK, migration/data-integrity or security-wide verdict is claimed beyond the traced concurrency boundary. Those are assigned separate reviewers. Snapshot encoding cost and full-history growth were not benchmarked. Risk: an already-staged checkpoint can currently outlive the queue authority that admitted it.
