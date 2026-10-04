# TASK-004 R2 Review Verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 13
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Prior remediation: `changes/active/skald-workflow-runtime/review/TASK-004-r1/TASK-004-R1-close-accepted-job-gaps.md`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `e86831e5ac784028f8022cc3faeee1c22b12c665`
- Scope: complete cumulative base-to-candidate range

The candidate identity remained unchanged throughout discovery, focused
follow-up, and independent validation. The review was strictly read-only with
respect to the candidate: no build, test, Cargo, or mise command was run.

## Independent review results

| Review | Result | Material outcome |
|---|---|---|
| Behavior | FAIL | Revision 13's three canonical Prompt examples still use the removed untagged request form. |
| Invariants | PASS | Accepted-job, authority, query, terminal, and provider-tagging invariants pass; prior findings remain closed. |
| Repository standards | FAIL | Qualified types violate the explicit bare-interface rule, and active TASK-004 authority still names Revision 12. |
| Maintainer | PASS | No additional material maintainability finding. |
| System resilience | PASS | Preparation, shutdown, query, provider, and recovery paths satisfy the approved boundaries. |
| Concurrency/lifecycle | PASS | Reservation tracking, blocking-work ownership, waiters, terminalization, and shutdown close the prior gaps. |
| Security/tenancy | PASS | Real second-tenant evidence, captured authority, fresh request authorization, tool isolation, and live gateway admission pass. |
| Query settlement | FAIL | Runtime behavior passes, but the Analytical lifecycle owner still documents a nonexistent follower release-ack protocol. |
| Provider contract | FAIL | Runtime, persistence, schema, SDK, and Vertex paths pass; the same stale spec examples remain. |
| Focused follow-up | RESOLVED | Confirmed the bare-type and stale Oracle-doc findings and narrowed the Revision 13 metadata correction without rewriting immutable r1 reports. |
| Ponytail validation | COMPLETE | Four new findings retained; all conflicts resolved and prior findings remain closed. |

## Reconciled acceptance matrix

| Obligation group | Result | Validated findings |
|---|---|---|
| Preparation is tracked before visibility and blocking work drains under the Workflow owner | PASS | Prior `FIND-TASK-004-1`, `-2` closed |
| Query opening, cancellation, owner loss, terminal integrity, and post-loss serviceability | PASS | Prior `FIND-TASK-004-3`, `-4` closed |
| Tenant isolation and captured accepted-run authority with fresh later-request authorization | PASS | Prior `FIND-TASK-004-5`, `-8`, `-14` closed |
| Exact built-in tool declarations and required negative journeys | PASS | Prior `FIND-TASK-004-6`, `-7`, `-10` closed |
| Idempotency, lifecycle races, gateway dialects/fallback, graph/run bounds, and sibling serviceability | PASS | Prior `FIND-TASK-004-9`, `-11`, `-12`, `-13` closed |
| Revision 13 tagged `ProviderRequest`, mismatch refusal, generated contracts, persistence, and internal Vertex projection | PASS | Runtime/source and recorded evidence align |
| Revision 13 examples use the accepted tagged request form | FAIL | `FIND-TASK-004-15` |
| Changed Rust interfaces satisfy the mandatory module-import/bare-type rule | FAIL | `FIND-TASK-004-16` |
| Active TASK-004 authority identifies the approved Revision 13 while preserving historical r1 evidence | FAIL | `FIND-TASK-004-17` |
| Oracle lifecycle documentation matches grant-stream-close release with no leader acknowledgement | FAIL | `FIND-TASK-004-18` |
| Rejected bespoke Oracle polling/refusal machinery remains absent; no new check, setting, option, dependency, compatibility path, or lifecycle protocol is introduced | PASS | None |

## Validated finding ledger

The detailed authoritative ledger is `findings-validation.md`.

- `FIND-TASK-004-15` — canonical Prompt examples use the removed untagged request form.
- `FIND-TASK-004-16` — changed Rust interfaces use qualified type paths contrary to the explicit repository rule.
- `FIND-TASK-004-17` — active TASK-004 authority still identifies Revision 12 despite Revision 13 being assigned and implemented there.
- `FIND-TASK-004-18` — the Analytical lifecycle owner documents a nonexistent follower release-acknowledgement protocol.

None requires a specification revision or a new material product, public API,
architecture, security, compatibility, cross-service, concurrency,
resource-ownership, or persistent-data decision.

## Follow-up decision

A focused follow-up was required because the standards report conflicted with
the maintainer review on qualified Rust interfaces, and the query-domain report
conflicted with otherwise passing standards/system/maintainer reviews on the
Oracle owner documentation. The follow-up confirmed both source-visible
violations. It also confirmed that current TASK-004 and remediation authority
must identify Revision 13, while the r1 verdict, ledger, discovery reports, and
the remediation's clearly historical immutable-input line remain Revision 12
records. No disagreement remains unresolved.

## Prior-finding closure

`FIND-TASK-004-1` through `FIND-TASK-004-14` remain closed. The candidate places
their corrections at the diagnosed owners and the recorded focused and broad
evidence corresponds to the required paths. The fixed human decisions remain
preserved: deleted Oracle graph-drain polling and supervisor idle refusal stay
deleted; follower release is grant-stream close with no leader acknowledgement;
tests wait for follower cleanup; and foreign-tenant journeys do not require a
model step because the harness provisions gateway credentials only for the
fixture tenant.

## Verification limits

- No build, test, Cargo, or mise command was run during this review.
- Candidate-recorded results were treated as evidence claims and checked against
  the named source and assertions.
- The four retained findings are directly visible in source or active change
  metadata and are not converted into verification limits.
- Every required discovery, follow-up, and validation report is present; no
  reviewer, authority, diff, source, or report was unavailable.

## Verdict

**FIX_REQUIRED**

Remediation task: `TASK-004-R2-align-revision-and-source-contracts.md`.
