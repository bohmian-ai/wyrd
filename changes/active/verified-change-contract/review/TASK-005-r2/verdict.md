# TASK-005 review verdict, round 2

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t005`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `81d2221b4d3b1a9fb6bd36c591d421385cc8c67d`
- Authority: approved `changes/active/verified-change-contract/spec.md`, revision 36; original `tasks/TASK-005-production-drift-verifier.md`; prior verdict and findings in `review/TASK-005-r1`.
- Scope: complete cumulative base-to-candidate diff. The candidate remained checked out through both review waves. `.codegraph/` is absent.

## Acceptance matrix

| Obligation | Result | Evidence or gap |
|---|---|---|
| Typed Drift contract, registration, exact baseline identity/status, and canonical results (`REQ-110`, `REQ-072`–`074`, `REQ-085`, `REQ-134`) | PASS | Typed specs, SQL baseline state, fitter, Card projection, and three SDK journeys. |
| Fixed, scoped, audited observation reads through Gate and local or peer Oracle (`REQ-080`, revision 36, `INV-010`) | PASS | Scoped SYSTEM token, ordinary query service, fixed SQL, denial audit, and peer read journey. |
| PSI, SPC, and Custom scoring and inconclusive semantics (`AC-012`, `INV-004`, `INV-012`) | PASS | Existing Vala scorers, bounded SPC history, and Rust/Python/TypeScript method journeys. |
| Shared Trigger and manual binding activation through dispatch (`AC-013`) | FAIL | One-Service Drift journey and manual enqueue seam do not prove two-Service attribution or manual binding dispatch; `FIND-TASK-005-2`. |
| Bounded fit, shutdown cancellation, shared admission, durable claims (`REQ-073`, `REQ-146`, `AC-020`, `AC-033`) | FAIL | Permit, decoded budget, and fencing pass; fit after decode does not observe cancellation during potentially long scoring; `FIND-TASK-005-9`. |
| PostgreSQL clock, tenant isolation, schema and maintenance (`REQ-152`, `INV-015`, `AC-024`) | PASS | Tenant-qualified SQL, migration/Forge namespace guard, recorded SQL/Bifrost lanes. |
| Task and approved-spec alignment | FAIL | Ready task still names revision 35 and local typed plans; `FIND-TASK-005-10`. |
| Non-goals and scope | PASS | No client aggregation, raw scorer download, user SQL, private Drift scheduler, Alert table, or SPC replacement. Pre-existing uncalled `Oracle::query_plan` is outside this diff. |

## Review waves and validated findings

| Report | Result | Proposed findings |
|---|---|---|
| `task-review.md` | FAIL | `TASKREV2-001` |
| `standards-review.md` | FAIL | `REPO-R2-001` |
| `domain-review-security-tenancy.md` | PASS | None |
| `domain-review-data-durability.md` | FAIL | `DATA-R2-001` |
| `domain-review-statistics.md` | PASS | None |
| `findings-validation.md` | FIX_REQUIRED | Three retained findings |

| Stable ID | Validation | Classification | Required closure |
|---|---|---|---|
| `FIND-TASK-005-2` | CONFIRMED | MISSING | Complete the existing Drift SDK journey for two Services sharing a Trigger, separate results/dispatch, and manual binding dispatch. |
| `FIND-TASK-005-9` | REVISED | VIOLATION | Observe cancellation during expensive baseline fitting and finish blocking work before lease settlement. |
| `FIND-TASK-005-10` | CONFIRMED | VIOLATION | Align the active task's revision and displaced query-plan instructions with approved revision 36. |

The complete source locations, caller traces, consequences, and focused closure proofs are in `findings-validation.md`.

## Prior-finding closure and verification limits

Prior `FIND-TASK-005-1`, `-3` through `-8` are closed. `-2` and `-9` remain open only for the gaps above. Revision 36 supplies the security authority missing in prior `-6`; security and statistics reviewers found no new material issue.

The task packet records successful post-change format, lints, Vala/SQL/Wyrd, all nine Bifrost lanes including three SDK journeys, storage matrix, codegen, boundary checks, and named focused commands. Reviewers inspected source and recorded evidence; they did not rerun broad lanes. Existing tests do not prove cancellation after fitting starts or the missing binding journey. The task header and approach still contradict revision 36.

## Verdict

**FIX_REQUIRED.** All three retained findings have bounded corrections within approved behavior and existing owners. See `TASK-005-R2-production-drift-closure.md`.
