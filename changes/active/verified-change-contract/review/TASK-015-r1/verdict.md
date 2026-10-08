# TASK-015 r1 verdict

## Immutable subject

- Candidate: `9b560d00569656ee7fdde19a9c76fa55c8d55fbb`.
- TASK-015 range: `3f8767a5f6a9b9c8605a53c424c7a7056a3b6786..9b560d00569656ee7fdde19a9c76fa55c8d55fbb`.
- Scoped merge resolution: `c5527627a50dd66a9f53d760d80f59bcb59609f9`, parents `ca2950856a37786a5cad25c73c1786c5fa7a1822` and `52e1144b5c186ccacd85d9c779a4e60c3cce5ac2`.
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 60, including REQ-077, REQ-108, and AC-014.
- Original task: `changes/active/verified-change-contract/tasks/TASK-015-eval-runs-in-the-batch-fence.md`.

The candidate identity was rechecked after discovery, follow-up, and structured validation. It remained unchanged. Merge `3f8767a5f` and previously passed TASK-013, TASK-014, audit-outbox, benchmark, and unchanged generic-outbox internals were not reopened.

## Reconciled acceptance matrix

| Obligation | Reconciled implementation and proof | Result |
|---|---|---|
| Reuse the generic outbox through `ObservationRunSink`; delete the hand-written queue and writer | `ObservationRunOutbox` is `Outbox<ObservationRunSink>`; the sink owns the tenant SQL write; the task-local queue, pending counter, writer, backoff, and stop machinery are deleted. | PASS |
| Stage one request per first-committed Eval record after Scribe acknowledgement without waiting for run creation | Gate invokes the synchronous hook only for `first_commit`; `ObservationEnqueue` stages one record carrying subject, record ID, and committed event time. | PASS |
| Use one retry-safe multi-row insert per tenant batch | The sink opens one tenant transaction and calls `VerifierRunQueue::enqueue_observation_batch`; the existing tenant/binding/record uniqueness fence and `ON CONFLICT DO NOTHING` make retries idempotent without consuming another ordinal. | PASS |
| REQ-077 outage retention, recovery, no count limit, graceful drain, and observable deadline loss | The unchanged generic outbox owns retention/retry/shutdown. The focused runtime test and integrated Gate/Scribe journey cover temporary refusal, pending retention, recovery, deduplication, graceful flush, and deadline residue. | PASS |
| REQ-108: an inactive occurrence creates no run and is not backfilled after later authentication | The staged request preserves no occurrence-time owner eligibility. A delayed flush lists matching bindings and evaluates mutable current activity; later qualifying authentication can therefore make a previously inactive committed observation eligible. Existing inactive-owner coverage checks only activity at SQL execution time. | FAIL — `FIND-TASK-015-1` |
| Keep the continuous Eval journey meaningful under revision 60 | Removing the permanently refused pre-phase deletes an assertion of superseded drop behavior. Required temporary-outage and recovery coverage remains in the dedicated integrated journey and focused runtime test. | PASS |
| Preserve sealed-replay proof | With the run table locked, Gate stages before acknowledgements return and the outbox pending count includes queued and in-flight work. Exact `pending() == 2` directly proves only original plus sentinel were staged; durable run-count and frozen-time assertions remain. | PASS |
| Use generic metrics and preserve shutdown wiring | The sink label is `eval_run_requests`; decode loss uses `outbox_events_lost_total`; Bifrost retains the outbox and server shutdown drains it after acknowledgement producers stop. | PASS |
| Preserve the scoped merge resolution | The resolution retains non-blocking decision staging through the shared `AuditOutbox`, removes reachable public audit-unavailable errors while reserving retired wire identity, and keeps SYSTEM-token issuance/acceptance removed while preserving credentialless SYSTEM attribution. | PASS |
| Meet repository documentation rules on materially changed async durable operations | `ObservationRunSink::write` documents returned errors and idempotent retry, but not deadline cancellation, partial progress, or an unknown commit outcome. | FAIL — `FIND-TASK-015-2` |
| Avoid other scoped regressions or prohibited expansion | No additional validated finding remains. The SDK integer conversion is behavior-preserving, generic-outbox internals are unchanged, and no Scribe/`vala-sql` cross-owner write was added. | PASS |

## Independent review results

| Report | Result | Proposed findings |
|---|---|---|
| `task-review-behavior.md` | PASS | None |
| `task-review-invariants.md` | PASS | None |
| `standards-review.md` | PASS | None |
| `maintainer-review.md` | FAIL | `MNT-015-001` |
| `system-review.md` | PASS | None |
| `domain-review-concurrency-durability.md` | FAIL | `DOMAIN-CONCURRENCY-1` |
| `domain-review-persistent-data-tenancy.md` | PASS | None |
| `domain-review-security-audit.md` | PASS | None |

## Follow-up decision

A focused follow-up was required because the concurrency reviewer identified a temporal REQ-108 path that the behavior, invariant, system, and persistent-data reviewers had passed, and because the maintainer and standards reviews disagreed about the async sink's documentation obligation.

`followup-review.md` resolved both conflicts:

- the committed Eval observation is the occurrence governed by REQ-108, and delayed evaluation of mutable current activity makes later-authentication backfill reachable;
- cancellation and partial-progress content is mandatory for this new async durable method even though the repository does not mandate a literal `# Cancellation` heading.

No additional discovery finding was added.

## Validated finding ledger

### `FIND-TASK-015-1` — REVISED / INCORRECT

- **Violated obligation:** REQ-108 requires an inactive occurrence to create no activation or run and forbids backfill after later authentication.
- **Location:** `crates/wyrd/wyrd-server/src/verification/observations.rs:105-123`; `crates/wyrd/wyrd-sql/src/queries/verifier_runs.rs:1640-1735`; mutable activity source in `crates/wyrd/wyrd-sql/src/queries/verification.rs:96-117,521-548`.
- **Evidence:** the staged record carries subject, record ID, and event time but no occurrence-time activity or admitted binding set. On a delayed retry, the batch insert reads the owner's current status and latest `last_authenticated_at`. A later qualifying authentication can therefore activate a historical observation. Matching by subject makes the path reachable when another active publisher emits for a shared subject while the exact binding owner is inactive.
- **Observable consequence:** a binding-driven Eval run can exist only because its owner authenticated after the observation committed.
- **Correction boundary:** a speculative event-time guard is unsafe because later renewal overwrites the only activity stamp and cannot distinguish a valid earlier occurrence from an inactive one. Synchronous binding/activity lookup in Gate would violate the non-I/O acknowledgement boundary. The specification must choose whether to redefine eligibility at successful flush, preserve activity history, or introduce an occurrence-bound eligibility snapshot through an approved asynchronous/durable boundary.
- **Closure proof after approval:** retain a request for an occurrence whose exact owner is inactive, authenticate before retry succeeds, prove the historical record never creates a run, prove the next eligible committed observation does, and prove ordinary renewal does not falsely reject an occurrence that was active when committed.
- **Specification revision required:** yes; this is a new concurrency and persistent-data decision.

### `FIND-TASK-015-2` — REVISED / VIOLATION

- **Violated obligation:** repository Rust documentation rules require applicable cancellation and partial-progress behavior on materially changed async durable methods.
- **Location:** `crates/wyrd/wyrd-server/src/verification/observations.rs:67-87`.
- **Evidence:** `ObservationRunSink::write` can be cancelled by the generic outbox at shutdown deadline while acquiring, inserting, or resolving commit, but its own rustdoc does not explain rollback versus unknown commit outcome or deadline-abandoned reporting.
- **Observable consequence:** a maintainer can incorrectly infer that cancellation proves rollback and change retry/shutdown handling unsafely.
- **Decision-complete correction:** extend the method's own rustdoc to state the deadline-cancellation and partial-progress contract, possible unknown commit outcome, loss reporting, and stable-key retry safety. Preserve implementation and generic outbox behavior.
- **Closure proof:** static source review plus the existing Rust documentation/lint lane; no new runtime test is required.
- **Specification revision required:** no.

## Explicitly rejected findings

- The two Eval journey edits follow revision 60 and do not weaken required proof.
- A permanently failing item blocking only its tenant is not a TASK-015 finding. REQ-077 defines slow/unavailable-Postgres retention and does not choose poison-item quarantine, splitting, dead-lettering, or discard semantics.
- The scoped merge resolution preserves both sides' required audit and SYSTEM-token intent.
- No other regression or unrelated scope expansion was independently validated.

## Prior-finding closure

This is the first TASK-015 review round. There are no prior `FIND-TASK-015-*` findings or remediation tasks to reassess.

## Verification limits

- Reviewers inspected the complete TASK-015 diff, the merge's combined conflict hunks, both merge-parent deltas, current owners/callers/consumers, and the named tests.
- `git show --remerge-diff c5527627a` could not create Git's temporary object directory in this checkout; the combined diff and explicit comparisons against both parents supplied the resolution evidence.
- No reviewer reran Cargo or `mise` commands in the shared checkout. The implementation record reports passing focused SQL and runtime tests, `mise run test:sql`, `mise run test:bifrost:integration:server`, the 31-test server journey lane, format, lints, workspace check, and diff check. `git show --check` was clean for both reviewed commits.
- Green tests do not close `FIND-TASK-015-1` because existing activity coverage observes only the later SQL-execution instant.

## Verdict

**SPEC_REVISION_REQUIRED**

TASK-015 cannot pass revision 60 while `FIND-TASK-015-1` remains. The approved behavior clearly forbids later-authentication backfill, but the current architecture does not decide how occurrence-time eligibility survives the asynchronous outbox boundary. Choosing that representation is a concurrency and persistent-data decision and cannot be invented in a remediation task. After the specification is revised and approved, the bounded documentation correction in `FIND-TASK-015-2` should be included in the resulting implementation work.
