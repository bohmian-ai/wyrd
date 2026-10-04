# TASK-004 R3 Review Verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 13
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Prior remediations:
  `changes/active/skald-workflow-runtime/review/TASK-004-r1/TASK-004-R1-close-accepted-job-gaps.md`
  and
  `changes/active/skald-workflow-runtime/review/TASK-004-r2/TASK-004-R2-align-revision-and-source-contracts.md`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `f17726fb25df1fa513875dca8d92f0073340ee0a`
- Scope: complete cumulative base-to-candidate range

The candidate identity remained unchanged throughout discovery, focused
follow-up, and independent validation. The review was strictly read-only with
respect to the candidate: no build, test, Cargo, or mise command was run.
`.codegraph/` is absent, so reviewers used immutable Git objects and direct
source inspection.

## Independent review results

| Review | Result | Material outcome |
|---|---|---|
| Behavior | PASS | The accepted server-job journeys and Revision 13 provider behavior satisfy the task; no proposal. |
| Invariants | PASS | The reviewer found prior findings closed, but did not identify the pre-poll step-attempt path later confirmed by follow-up and validation. |
| Repository standards | FAIL | Four materially changed Rust interfaces or dependency uses violate the mandatory module-import rules. |
| Maintainer | PASS | No independent maintainability proposal. |
| System resilience | PASS | Preparation, shutdown, query, provider, pod-loss, and recovery paths otherwise satisfy the approved boundaries. |
| Security/tenancy | PASS | Captured authority, fresh request authorization, audit coupling, tenant isolation, and the approved fixture limitation pass. |
| Concurrency/lifecycle | FAIL | Published `Running` steps can have zero attempts and regress to `Unstarted`; it also proposed stale supervisor wording. |
| Query settlement | PASS | Cancellation, original-deadline settlement, grant-stream-close release, pod loss, and later serviceability pass. |
| Provider contract | PASS | Tagged request authoring, persistence, schemas, SDK projection, and Vertex dispatch pass. |
| Focused follow-up | RESOLVED | Confirmed the reachable step-attempt defect and import violations; confirmed the supervisor wording is inaccurate. |
| Ponytail validation | COMPLETE | Retained two findings and rejected the unchanged supervisor wording as out-of-scope pre-existing debt. |

## Reconciled acceptance matrix

| Obligation group | Result | Validated findings |
|---|---|---|
| Preparation is owned before visibility; idempotent admission, capacity, retention, blocking work, and shutdown are bounded | PASS | Prior `FIND-TASK-004-1`, `-2`, `-9`, `-11`, `-13` remain closed |
| Query opening, cancellation, terminal proof, owner loss, capacity recovery, and post-loss serviceability | PASS | Prior `FIND-TASK-004-3`, `-4`, `-10` remain closed |
| Tenant isolation, authorization, audit, and accepted authority independent of bearer lifetime | PASS | Prior `FIND-TASK-004-5`, `-8`, `-14` remain closed |
| Built-in tool declarations, exact bounds, and negative journeys | PASS | Prior `FIND-TASK-004-6`, `-7` remain closed |
| Gateway dialects, fallback, deadline, cancellation, provider tagging, persistence, and Vertex projection | PASS | Prior `FIND-TASK-004-12`, `-15` remain closed |
| Active Revision 13 authority and Oracle lifecycle owner documentation | PASS | Prior `FIND-TASK-004-17`, `-18` remain closed |
| Every observable running step has a begun attempt; interrupted active work remains cancelled with timestamps and never regresses to unstarted | **FAIL** | `FIND-TASK-004-19` |
| Materially changed Rust interfaces and dependency uses follow module-import and bare-type rules | **FAIL** | `FIND-TASK-004-20` |
| Rejected bespoke Oracle polling/refusal/release-ack machinery remains absent; no new check, setting, option, compatibility path, or foreign-tenant credential harness is introduced | PASS | None |

## Validated finding ledger

The authoritative validation and correction details are in
`findings-validation.md`.

- `FIND-TASK-004-19` — a published `Running` step has zero attempts and a
  pre-poll abort can regress it to `Unstarted`, erasing its start timestamp.
- `FIND-TASK-004-20` — four materially changed Rust interfaces or dependency
  uses bypass their module import blocks contrary to repository rules.

Neither correction requires a new product, public API, architecture, security,
compatibility, cross-service, concurrency-semantics, resource-ownership, or
persistent-data decision.

## Follow-up decision

A focused follow-up was required because the concurrency report conflicted
with otherwise passing behavior, invariant, maintainer, and system reviews,
and because the standards report identified R2 import-cleanup omissions those
reviews had not addressed. It traced all three uncertainties.

The independent validator confirmed the step-attempt and import claims. It
rejected the proposed `analytical_supervisor.rs` documentation finding: the
wording is factually stale, but it predates the base unchanged, while the
candidate runtime, governing architecture, and R2-owned lifecycle
documentation already use grant-stream-close release without a leader
acknowledgement. Requiring that edit would turn unrelated pre-existing debt
into TASK-004 scope.

## Prior-finding closure

`FIND-TASK-004-1` through `FIND-TASK-004-18` remain closed. The R1 runtime,
security, lifecycle, and journey corrections remain at their diagnosed owners.
The R2 tagged examples, bare-interface locations it named, Revision 13
authority chain, and `AnalyticalGraphLifecycle` documentation corrections are
present.

The fixed human decisions remain preserved: deleted Oracle graph-drain polling
and supervisor idle refusal stay deleted; follower release is grant-stream
close and the leader does not await a follower acknowledgement; foreign-tenant
journeys do not require a model step because the harness provisions gateway
credentials only for the fixture tenant.

## Verification limits

- No build, test, Cargo, or mise command was run during this review.
- Candidate-recorded results were treated as evidence claims and checked
  against current source and the named assertions.
- The recorded lanes credibly support the passing boundaries and prior-finding
  closures, but they do not close `FIND-TASK-004-19`: the transition test omits
  the running-attempt assertion and the pre-poll-abort test expects the
  contradicted regression.
- Recorded format and lint results do not enforce or waive the explicit import
  rule in `FIND-TASK-004-20`.
- Every required discovery, follow-up, and validation report is present; no
  required reviewer, authority, diff, source, or caller trace was unavailable.

## Verdict

**FIX_REQUIRED**

Remediation task: `TASK-004-R3-close-step-attempt-and-import-gaps.md`.
