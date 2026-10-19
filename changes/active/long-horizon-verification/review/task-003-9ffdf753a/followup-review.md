# TASK-003 focused follow-up

**Result: RESOLVED for the two assigned discovery conflicts.** The separate revision-13 approval versus revision-14/draft authority discrepancy remains **UNRESOLVED**. This report makes no overall task verdict and does not approve draft authority.

Subject: base `ef3a57b0776dd7bc451273b9176498fcca700550`, candidate `9ffdf753ab11ce9b638c906388d3690a6aede249`. Source, tests, configuration and HEAD remained immutable. Only this report was written.

## Scope and inspected evidence

The conflicting inputs were MAINT-001, B-001, INV-REV-001 versus the concurrency-domain lifecycle assessment; and B-002/INV-REV-002 versus that report's held-A progress assessment. They were treated as hypotheses, without an intended verdict. Read the subject, original task, relevant claims in the four reports, cumulative changed source/fixtures and governing authority. Navigation started with CodeGraph and expanded through source and caller searches.

Applicable authority inspected: supplied AGENTS instructions; `architecture/agent-rules.md`; spec-driven-development and maintainer-style references; original TASK-003 Owners/Scope, Scenarios 1–3 and acceptance criteria; unchanged `spec.md` REQ-006 and AC-005/J-005; Wyrd design invocation/submission/replay and outbox sections; Bifrost audit/outbox retry and loss boundary. The approved-revision discrepancy limits final acceptance authority, independently of the source conclusions below.

| Boundary | Source inspected |
|---|---|
| Admission and shutdown | Complete `Outbox::{new,stage,reserve,reservable,send,settle,shutdown}`, `Reservation::{stage,drop}`, `Admission::release`, `admit`, and `OutboxWriter::{run,dispatch,is_ready,next_retry,finish,release}` in `crates/shared/wyrd-runtime/src/outbox.rs`; base version of shutdown; reservation/retry/bound/drain tests. |
| Lifecycle callers | `crates/wyrd/wyrd-server/src/app/server.rs` shutdown sequence, especially 849–935; boot cleanup search; `crates/wyrd/wyrd-testing/src/server.rs` teardown at 900–950; Scribe and peer-audit shutdown tests; repository outbox-shutdown caller search. |
| Pressure and progress journeys | Complete `verification_staging_refuses_and_recovers` bodies in Rust (`sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:1220–1340`), Python (`sdks/wyrd-sdk-python/tests/integration/test_verify_in_real_time.py:456–526`), TypeScript (`sdks/wyrd-sdk-ts/wyrd/tests/integration/verify-in-real-time.test.ts:442–496`), with capacity/delay constants and affected fixture wiring. Supporting outbox tenant test and runtime claim/cap changes inspected. |
| Enqueue identity and acceptance | Complete `Target::{execute,enqueue,request}` in `crates/shared/wyrd-client/src/verify/mod.rs:409–468`; `VerificationControl::enqueue` in server service at 611–682; Scribe ACK handoff at `scribe_outbox.rs:940–952`; complete `SubmissionSink::write`; complete `RequestKey::{new,execution_id}`, `Submission::new`, `VerifierExecutionQueue::{accept_submissions,accept}` in `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs`. |
| Existing HTTP proof owner | Complete `post` and `enqueue` helpers at `pg_verification_routes.rs:159–200`, and `enqueued_executions_replay_conflict_and_read_back` including stable-key equality, conflict and durable queue-count assertions at 381 onward. |

## 1. Shutdown: supported shared-owner contract, without a demonstrated deployed race

The conflicting reports describe different boundaries. The domain report is correct about the production coordinator: `app/server.rs` awaits Scribe shutdown, then Bifrost shutdown, then submission shutdown. These are separate outbox instances, sequentially drained. The test-server fallback follows the Scribe-before-Bifrost sequence. I found no production caller that concurrently calls shutdown on the same outbox. Do not claim this interleaving currently occurs in ordinary server shutdown or requires redesigning that coordinator.

However, concurrent shutdown is not an invented new requirement. The **base** `outbox.rs` shutdown rustdoc already says: “When shutdown is called concurrently, exactly one caller reports the loss and the others report zero.” The candidate retains that statement at 331–333. The public method accepts `&self` on the shared `Arc<Outbox<S>>` returned by `new`; there is no single-caller restriction or exclusive lifecycle token. TASK-003 expressly says to keep existing “shutdown/crash-loss semantics,” preserve shutdown/loss counters, and reuse existing release/retry/shutdown ownership. Thus this is an explicit existing regression boundary of the materially changed owner, rather than a dormant concurrency feature requested merely because public methods could theoretically be called together. Removing the existing guarantee would change the preserved contract.

The producer and consumer trace confirms the reported mismatch:

1. `reserve` admits a reserved slot; `Reservation::stage` calls `send(..., true)`. Under the queue read fence, `send` increments both `pending` and `staged_reserved` before publishing the item.
2. A hanging/failed write retains the item and its reserved count. At deadline, shutdown cancels the writer and waits for its exit. Remaining unwritten counters can legitimately be `(pending=1, staged_reserved=1)`. New sends cannot enter the closed queue; normal writer settlement has ended.
3. On two runtime worker threads, caller A can take `pending.swap(0)` and obtain 1, then be preempted. Caller B takes `pending.swap(0)` and obtains 0, then takes `staged_reserved.swap(0)` and obtains 1.
4. B calls `Admission::release(0, 1)` at 350. `reserved.fetch_sub(1)` runs, then `count - reserved` at 441 underflows. Checked arithmetic panics; unchecked arithmetic performs modular accounting updates. A can subsequently take zero reserved items and release its one item against the wrong share. No await is needed between swaps for another OS/runtime worker thread to interleave; `AcqRel` does not make two distinct atomics one transaction.

The concrete blocking consequence is a panic in a supported concurrent shutdown path when arithmetic checks are enabled, violating the one-loss-reporter result. Do not describe the unchecked variant as necessarily permanently corrupting the final counters: if both callers finish, modular subtraction may cancel out; its intermediate accounting still fails the paired ownership invariant. Existing single-caller shutdown tests with unreserved work do not exercise this branch.

**Proposed local finding FOLLOW-001 — REGRESSION**, consolidating MAINT-001/B-001/INV-REV-001: `outbox.rs:348–350` splits the final abandoned-work pair across callers. Keep loss and reservation-share settlement with the existing Outbox lifecycle winner, preserving unspent reservations, late-stage loss, the queue-close fence and writer wait. A focused reserved-work concurrent shutdown regression should prove completion without panic, one summed loss report/count and final zero pending/share accounting. No new public concurrency contract, server coordinator, lifecycle service, saturating arithmetic or platform mechanism is needed. This proposal goes to independent validation; it is not a production-race claim.

**Conflict status: RESOLVED.** Sequential production shutdown and a violated existing generic-owner concurrency guarantee can both be true.

## 2. Progress and identity: useful evidence, with two specific missing seams

### Held A proves activity; it does not prove continued offered traffic

All three journey bodies have the same ordering:

`start A1 → observe its slow judge → submit B → await B completion → flush → start A2 → verify/submit refusals → finish A1/A2 → drain → submit recovery`.

With capacity 6, reservable capacity is floor(6×3/4)=4. A1 retains two slots while B is admitted and judged. Its unfinished handle is explicitly checked after B completes. This is credible evidence for the narrower AC-005/J-005 wording that B completes while A is still **active**, when resources permit. The domain report's claim is valid at that boundary. It is not evidence of strict fairness, nor does it need to be.

TASK-003 Scenario 3 separately requires B to complete “while A continues offering more calls”; its acceptance criterion says B progresses during A's burst. The tests offer no additional A calls while waiting for B, and enter the full/refusal phase only after B completes. The task's explicit continued-offering condition is therefore not covered by these three stories. Supporting sink-independence and finite pre-seeded SQL tenant-round tests exercise different boundaries and cannot establish that SDK producer-pressure seam. This is a missing requested proof, not evidence that the production claim loop starves B.

**Proposed local finding FOLLOW-002a — MISSING acceptance proof**, corresponding to the progress portion of B-002/INV-REV-002: extend the existing three SDK stories with finite controlled A requests overlapping the lifetime of admitted/runnable B work and its completion, including offered pressure, then stop A and drain accepted work. Observe accepted/refused calls and bounded retained work. Retain existing language projection assertions. Do not require B to complete while every resource is unavailable, admission after the bound is completely occupied, exact shares, a latency SLA, a new scheduler, synthetic host load, a benchmark or a replacement harness. The required observation is B progress while traffic is still offered and resources/dependencies permit it.

### Same Run is not the same enqueue identity

`Target::enqueue` explicitly mints `verification-submit-{Uuid::now_v7()}` at `verify/mod.rs:445` on **every invocation**. That key stays stable within the existing transport retries of one invocation. Recalling `.submit` after an exhausted refusal returns to `Target::enqueue` and mints a new key, even on the same Run. All three languages use this shared client behavior. Their recovered result's Run equality and A's three source rows prove refusal stages no source row, accepted work drains, and a later fresh-key submission works. They do not establish stable-key recovery/replay of the refused enqueue.

That distinction is required by AC-005/J-005 step 2: “Retry the same enqueue identity after drain: one acceptance/array element.” TASK-003 maps AC-005, asks for replay in Scenario 3, and preserves existing retry identity. The standalone task may defer **grouped array-element** evidence until TASK-001 integration, as the task expressly permits. It should not label fresh-key recovery proof as exact identity replay. There is no need to expose a caller key in a new SDK API to close this gap.

The existing server owner already provides the needed seam:

- `VerificationControl::enqueue` constructs a `Submission` using the optional supplied key before reserving either outbox; admission failure returns before source staging and before durable acceptance.
- `Submission::new` uses `RequestKey::execution_id`, derived from tenant, accepted attribution, key and canonical request digest. Repeating key/body/authority produces the same execution identity without a producer Postgres lookup.
- Acknowledged input hands the same submission to `SubmissionSink::write`; `VerifierExecutionQueue::accept` is the sole durable replay/conflict owner. Existing-ID replay returns without another insert. Key locking/digest checks preserve conflict behavior.
- The existing HTTP `enqueue` helper accepts an explicit key and the existing HTTP replay test checks equal returned IDs plus durable queue count. That test has no preceding capacity refusal or pressure drain. It does not close the composed recovery seam by itself.

**Proposed local finding FOLLOW-002b — MISSING acceptance proof**, corresponding to the identity portion of B-002: use the existing real-server HTTP enqueue/replay owner and explicit `Idempotency-Key` support to obtain a staging-full refusal for a fixed body/key/authority, drain pressure, retry that exact tuple successfully, then replay the accepted tuple and establish one durable queued execution. Prove the refused attempt stages no source row. Preserve delayed ACK/202 semantics and existing source-loss boundaries. Existing SDK stories remain the correct owners of each language's refusal/recovery projection; stable-key replay need not be duplicated through new language APIs. At this standalone boundary, durable execution cardinality is the smallest credible exact-key acceptance proof; grouped array-element cardinality stays at the already-deferred TASK-001 integration seam. No source defect in current key derivation or durable replay was demonstrated.

These two proof gaps may be packaged together under B-002 because the journeys are already being corrected, but they exercise distinct owners: continuing producer traffic in each existing SDK story, and exact-key recovery in the existing HTTP acceptance/replay path. Do not turn one into a prerequisite for an unrelated public API change.

**Conflict status: RESOLVED.** Preserve credit for held-A neighbor progress and fresh-key recovery, while requiring the task's additional continued-offering and exact-identity observations at their existing owners.

## Verification limits

This follow-up ran source/diff/caller inspections and `git rev-parse HEAD`; no behavioral test, build, SDK/Postgres/MCP rerun, benchmark or live infrastructure check was performed. The shutdown interleaving is a source-derived reachable schedule, not an observed production incident or a reproduced test failure. Prior reviewers' six passing outbox tests and implementation-reported green journeys remain their evidence, not newly reproduced results.

Skipped: source changes and behavioral reruns; this pass wrote only the assigned report.
Risk: proposed findings require independent validation, and the separate approved-spec artifact discrepancy remains unresolved.
