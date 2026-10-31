---
id: TASK-002-R2
kind: remediation
status: ready
spec: SPEC-storage-upload-consolidation
spec_revision: 2
parent_task: TASK-002
remediates: [FIND-TASK-002-3]
---

# Finish required recovery documentation

Route directly to `$wyrd-implement`. The upload isolation and File sweeper fixes are accepted. Correct only the remaining mandatory documentation within FIND-TASK-002-3; no runtime change is required.

## Acceptance context

- Approved spec: `changes/active/storage-upload-consolidation/spec.md`, revision 2.
- Original task: `changes/active/storage-upload-consolidation/tasks/TASK-002-shared-file-uploads.md`.
- Original base: `15f939fc917b22efa5e5e4d12fd1f97be807a0a7`.
- Prior reviewed candidate: `5ef2981e66068e8ee5f96d3be15000eea9cbd94f`.
- Current candidate / correction baseline: `a80afd68e47d992e35f20931984d96524b2e7dbf`.
- Prior remediation: `../TASK-002-R1/TASK-002-R1-file-session-lifecycle.md`; retain its original scope and implementation evidence.
- Current acceptance evidence: this directory's `verdict.md`, `findings-validation.md`, `followup-review.md`, `verification.md`; prior R1 reports remain context.

## Diagnosis and selected correction

Root AGENTS requires meaningful rustdoc on changed Rust items, including applicable Panics. The Rust guide also requires accurate cancellation, partial-progress and retry contracts. The prior remediation supplied most of the missing inventory, but some new recovery claims contradict the actual committed states. This is incomplete closure of the existing mandatory gate, not a request for more runtime behavior or optional style work.

**File planning precedes transfer.** `crates/shared/wyrd-client/src/verify/mod.rs:484–489`, `RunEvidence::pin`, says transfer failure leaves later Files unplanned and the interrupted session open. In fact the method sends all missing File declarations in one request at `:532–536` before serial transfer at `:538–555`. Server `VerificationControl::evidence` commits each required session before returning that plan. If A completes and B fails, C already has an open session. A planning cancellation can leave a committed prefix without returning its plans. The interrupted session can remain open, become terminal Failed, or already be Completed when its response is lost. The current paragraph therefore gives a maintainer the wrong partial-state model.

Replace that paragraph with concise phase-accurate effects: all missing Files are planned before serial transfer; earlier acknowledged completions survive; untransferred plans remain open for their existing expiry reclamation; the interrupted session can be open or terminal; a lost plan response may leave initialized sessions. A fresh invocation consults completed inventory and initializes independent sessions for remaining work. Do not promise immediate inventory visibility or rollback on error. Keep the existing plan/transfer/complete owners and behavior unchanged.

**Terminal Failed is not sweep eligible.** `crates/wyrd/wyrd-storage/src/service.rs:185–190`, `upload_init`, promises sweep reclamation for an initiating row “or a row marked failed.” `file_upload_init` at `:232–235` similarly assigns sessions left after any post-initiating failure to sweep reclamation. But `drive_backend_init_or_mark_failed:1221–1252` can commit Failed, while SQL `admin/multipart_uploads::expired_uploads_batch:46–88` selects only pending/initiating. Later cancellation can also happen after a pending transition or terminal outcome; initiating commit alone does not establish the remaining status.

Correct both public initializer descriptions and align `init_upload`'s shared internal explanation: only open Initiating/Pending rows are eligible for the existing grace/expiry sweep; persisted Failed is terminal and is not promised reclamation. Preserve the existing disclosure that an unrecorded provider handle may rely on provider expiry. Keep this a prose correction: do not add Failed to discovery, delete terminal rows, change cleanup policy, or invent guarantees to match the inaccurate prose.

**One changed test is still undocumented.** `crates/wyrd/wyrd-storage/tests/integration/pg_sweeper.rs:14`, `tick_sweeps_expired_uploads_and_reaps_idempotency_rows`, changes its owner-fixture calls but has no purpose rustdoc or Panics. Root AGENTS explicitly gates every changed Rust item, even though these calls are mechanical adaptations. Add a concise statement covering leader-only expired/orphan upload cleanup and idempotency expiry, with Panics for fixture/query/tick/assertion failure. Preserve its body; no new test is needed.

These edits are the smallest correction: update the existing item docs and add the omitted test docs. No abstraction, new checker, dependency or broader documentation pass is warranted. Exact independent producer/consumer traces and rule validation are preserved in `findings-validation.md` and `followup-review.md`.

## Constraints and preserved acceptance

Keep FIND-TASK-002-1 and FIND-TASK-002-2 closed: independent session-qualified final locations, strict tenant/owner validation, Card replacement, completion replay/inventory semantics and the existing sweeper abort/status/race behavior remain unchanged. Do not change code, tests, provider behavior, authorization, retention, queueing or public declarations to make the prose true. Do not pull TASK-003 SQL elimination/metadata/readback work forward. No new cloud cleanup contract, runtime regression test or full journey suite is required.

## Acceptance and focused proof

For FIND-TASK-002-3, verify the final item documentation against these already inspected scenarios:

- A returned plan covers A/B/C; after A completes and B is interrupted, C is already planned. Interrupted planning can leave a committed prefix without a response.
- The interrupted session is not promised to remain open: failed completion or a lost completion response can leave a terminal state.
- Card/File initialization docs distinguish sweep-eligible open states from terminal Failed and preserve actual provider-expiry limits.
- The changed sweeper test has purpose/Panics documentation matching its existing fixture, tick, query and assertion behavior.

This is documentation-only, so source comparison is the credible proof; do not manufacture RED or add a documentation checker. Run `mise run fmt:check`, the required owning `mise run lints`, and `git diff --check`. Record their results and the bounded source checks. No Cargo runtime tests, backend emulators, cloud credentials, codegen or full user-journey suites are needed unless implementation scope changes. A subsequent review checks only this remaining closure and changed documentation against the carried-forward acceptance.

## Implementation evidence

Commit: `48a9bc98b` (documentation only; no code or test body changed).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| A/B/C plan returned before transfer; interrupted planning can leave a committed prefix | `wyrd-client/src/verify/mod.rs` `RunEvidence::pin` Cancellation: one planning request opens every missing session before serial transfer; planning failure can leave unreturned sessions | Source check: single `POST /v1/verification/evidence` precedes the transfer loop; `VerificationControl::evidence` commits each `file_upload_init` | PASS |
| Interrupted session not promised open | Same paragraph: open, failed, or completed with a lost response; no rollback; fresh call replans against completed inventory | Source check: `finish_upload` marks failed on verification failure; completion commits before response | PASS |
| Init docs separate sweep-eligible open states from terminal Failed; provider-expiry limit kept | `wyrd-storage/src/service.rs` `upload_init`, `file_upload_init`, `init_upload` Cancellation | Source check: `drive_backend_init_or_mark_failed` commits Failed; `expired_uploads_batch` selects only pending/initiating | PASS |
| Changed sweeper test documented | `pg_sweeper::pg_tests::tick_sweeps_expired_uploads_and_reaps_idempotency_rows` purpose and Panics | Source check against its body | PASS |

Checks: `mise run fmt:check`, `mise run lints`, `git diff --check` — all pass. No runtime tests run (documentation only, per task).

Non-goals confirmed: no runtime, test-body, SQL, sweeper policy, provider, authorization or public-contract change; FIND-TASK-002-1/2 untouched; no TASK-003 work.
