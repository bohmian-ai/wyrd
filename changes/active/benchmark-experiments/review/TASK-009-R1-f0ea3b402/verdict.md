# TASK-009 verdict

**FIX_REQUIRED**

## Immutable subject and scope

- Original task: `changes/active/benchmark-experiments/tasks/TASK-009-unified-queue-and-closeout.md`, revision 2.
- Approved specification: `changes/active/benchmark-experiments/spec.md`, revision 24.
- Base: `6f38db16c524417301ffc4a5dc0b478daf997ed1`.
- Candidate: `f0ea3b4023e544d0f95e9b5e38d398105dbe2e33`.
- First review of this replacement task: complete `git diff 6f38db16c..f0ea3b402`, all 35 changed files and each of the 15 commits. Base is merge-base, so the skill's three-dot range is identical. No earlier task verdict or acceptance evidence was carried forward.
- HEAD and tracked source/index remained unchanged through final review. The existing untracked `tasks/unified-verification-flow.html` was excluded and preserved. Only this review directory was written.

[Scope and navigation](scope.md) records owners, callers, authorities and reviewer selection. [Verification](verification.md) records checks and external evidence provenance. Reviewers independently expanded the common map; the implementer's summary was not acceptance evidence.

## Independent review and reconciliation

| Role and selected boundary | Report | Result |
|---|---|---|
| Comprehensive behavior/acceptance and every commit | [Behavior](task-review-behavior.md) | FAIL: B-01, B-02 |
| Persistent state, concurrency, dedupe, migration and tenancy | [Persistence](domain-review-persistence.md) | FAIL: DP-001; no additional runtime defect established |
| Failure propagation, cancellation, process loss, polling and shared outbox | [System](system-review.md) | PASS, empty material ledger |
| All changed tiers and mandatory repository rules | [Standards](standards-review.md) | FAIL: REPO-001 |
| Ownership, whole-workflow reuse and maintainability | [Maintainer](maintainer-review.md) | PASS, empty material ledger |
| Fresh independent source/materiality/correction validation | [Validated ledger](findings-validation.md) | FIX_REQUIRED, four retained findings |

Persistence/system specialists cover the complex shared-state/lifecycle boundaries; no separate invariant reviewer was selected. No sensitive permission protocol was changed that required a separate RBAC reviewer. All selected reports are complete. [Claim comparison](claim-comparison.md) found no material discovery conflict requiring follow-up. The validator independently traced each unique proposal, validated empty specialist ledgers and found no new blocker needing another source validator. No convergence assessment applies on this first round.

The standards report's architecture PASS concerns the text added by the patch; it does not negate the persistence review's source-proven contradictory existing owner paragraphs. The independent validator reconciles this as FIND-TASK-009-3. Test-local imports and positional decoding observations are not retained findings and create no remediation work.

## Reconciled acceptance matrix

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| REQ-035 / AC-015: own-input ACK and one common handoff; direct independent | Private `Acknowledged`, Scribe successful input filtering, shared `SubmissionSink`, closed typed admission; V1/V4/V18 source assertions and earlier execution evidence | Implementation supported; required public-authoring proof incomplete under FIND-TASK-009-2 |
| REQ-036 / AC-016: atomic execution/source acceptance, frozen metadata, replay and newer activity | TenantConn prepare/accept split, expected cursor, non-reused group generation and conditional consumption; SQL rollback/content/occurrence/tenant tests, V4 fixed metadata | PASS within reviewed source/evidence; no fresh database execution claimed |
| AC-017: bounded asynchronous preparation, per-tenant mixed-origin commits and distributed discovery | Existing outboxes/reservations, retained RAII claim, common writer and queue/lease implementation; V3–V6/V17 assertions | PASS within reviewed source/evidence |
| REQ-037 / AC-018: Bifrost inputs, existing engine, stale fence, three payload owners and direct costs | Existing runner/InputResolver/result settlement reused; no PG input path; V7–V11 and shared-clock regression | PASS within reviewed source/evidence |
| REQ-038: architecture reconciliation, retirement and exact counts | No prequeue store/hold/alternate runner; protected one scheduled/four mixed attempts and exactly 14 group queries unchanged; normative owner ordering remains contradictory | FAIL, FIND-TASK-009-3 |
| Mandatory changed-item rustdoc | New `InFlight::drop` has no rustdoc | FAIL, FIND-TASK-009-4 |
| REQ-038 / AC-019 / V13: final integrated candidate proof | Genuine earlier green gate identifies `e9e3d95cb`; executable client/upstream changes follow it | FAIL, FIND-TASK-009-1 |
| REQ-039 / AC-019 / V14–V16: same-final-SHA measurements and delivery | Five accepted older L1 records on `19f4190a8` in separate history commit `8c066985a`; later executable candidate differs; PR explicitly unopened | PENDING original downstream closeout; not an invented admission-code defect |
| INV-001/002/003/006/007/008 and non-goals | Existing RLS, authority, audit, payload ownership, retention and resource boundaries preserved; no new framework/store/public receipt, stronger durability, L2, merge or release | PASS within inspected scope |

## Validated findings

The complete source evidence, materiality, producer/consumer traces, correction burden and closure proof are in [findings-validation.md](findings-validation.md).

| Stable ID | Discovery / validation | Classification and smallest correction |
|---|---|---|
| FIND-TASK-009-1 | B-01, REVISED | MISSING: final-source V13 proof. Preserve honest SHA/log attribution, keep the original integrated gate pending until same-source proof exists; do not add an aggregate to per-correction remediation. |
| FIND-TASK-009-2 | B-02, REVISED | MISSING: existing V18 bypasses explicitly planned Run/Window authoring. Extend that one journey through existing public Run/Observe APIs while retaining held-ACK and fixed-readback assertions. |
| FIND-TASK-009-3 | DP-001, CONFIRMED | INCORRECT: Wyrd group and Bifrost permission/capture paragraphs still prescribe early admission. Correct those owner paragraphs to ACK-first, generation-scoped acceptance. |
| FIND-TASK-009-4 | REPO-001, CONFIRMED | VIOLATION: missing mandatory rustdoc on new scheduler `InFlight::drop`; add concise lifecycle documentation without behavior changes. |

No source-proven material runtime admission, persistence, tenancy, recovery or ownership defect was established. The proof and mandatory documentation findings still prevent acceptance. No correction requires a specification revision.

## Verification limits and next scope

The exact outbox lost-wakeup nextest selector passed on the candidate (one test); cumulative diff whitespace check passed. No full gate, full benchmarks or database/SDK suites were rerun in this review. Static assertions and earlier logs are identified as such. The outbox diagnosis proves the source race; it does not prove causal attribution of the particular captured integration timeout.

The user's maintainer acceptance of the five older L1 records is honored; driver-budget validity is not reopened. Those records continue to identify their original SHA. V13 orders immutable correctness review before final measurement/PR delivery, so this review does not demand publication before correctness review. Whole-task completion still requires original final-source integrated proof and V14–V16 closeout, or an explicit change to those obligations. No benchmark, commit, push, PR, merge or release was performed or authorized by this review.

Route [TASK-009-R1-complete-proof-and-authority.md](TASK-009-R1-complete-proof-and-authority.md) directly to `wyrd-implement`. A later review retains this original task/range, verdict, ledger and candidate, then checks bounded closure and affected paths rather than restarting full discovery. Final gate/measurement/delivery remain original integrated closeout obligations, not a new remediation-wide suite.

**FIX_REQUIRED**
