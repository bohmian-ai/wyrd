# TASK-005 review verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t005`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `9cfe9b69c5990603e07458aa4402f6750131bfbc`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 35
- Original task: `changes/active/verified-change-contract/tasks/TASK-005-production-drift-verifier.md`
- Review attempt: `TASK-005-r1`

The candidate remained checked out at the requested commit throughout both review waves.

## Acceptance matrix

| Obligation | Result | Evidence |
|---|---|---|
| Approved PSI/SPC/Custom contract and registration validation | PASS | Typed contract validation and aggregate scorers preserve the approved public shapes. |
| Exact Parquet baseline resolution, durable fit state, and Card status | FAIL | Exact identity, PostgreSQL-clock claims, retry fencing, and status projection pass; shared admission and bounded decoded work fail under `FIND-TASK-005-8` and `FIND-TASK-005-9`. |
| Fixed tenant/subject/series/`[start,end)` Oracle aggregate plans | FAIL | Typed plan predicates and tenant source replacement pass, but the production reader uses forbidden synthetic SYSTEM authority under `FIND-TASK-005-6`. |
| PSI production scoring | FAIL | Core fitted-bin/count scoring exists, but mandatory boundary, categorical, zero-bin, and minimum-sample journey proof is incomplete under `FIND-TASK-005-2`. |
| SPC contract and bounded production scoring | FAIL | Public scorer semantics are preserved, but aggregate batches, subgroup means, and zones are retained for the full window under `FIND-TASK-005-5`; required production cases are also missing under `FIND-TASK-005-2`. |
| Custom weighted-mean and inconclusive semantics | FAIL | Aggregate scoring exists, but a never-created observation table errors instead of completing inconclusive under `FIND-TASK-005-1`; required equality/invalid/window journeys remain incomplete under `FIND-TASK-005-2`. |
| Generic result publication, settlement, and activation reuse | FAIL | Existing runtime/result owners are reused and scored details precede summary, but the required activation, delivery, retry, restart, and TypeScript production journeys are incomplete under `FIND-TASK-005-2`. |
| Tenant isolation and PostgreSQL coordination clocks | PASS | RLS, tenant-qualified keys, exact Card/artifact identities, PostgreSQL timestamps, token fencing, and cross-tenant refusals passed inspection. |
| Required exact focused verification evidence | FAIL | Named statistical, SQL, journey, CLI, and runtime tests lack recorded exact selectors under `FIND-TASK-005-4`. |
| Repository standards and scope discipline | FAIL | The candidate contains unrelated workflow-skill changes (`FIND-TASK-005-3`) and an undocumented new Rust test module (`FIND-TASK-005-7`). |
| Explicit non-goals | PASS | No client aggregation, raw-value download, user SQL, profile MemTable, Drift scheduler, Alert table, fabricated no-report result, compatibility surface, or SPC replacement was added. |

## Review waves

| Report | Result | Proposed findings |
|---|---|---|
| `task-review.md` | FAIL | `TASKREV-001` through `TASKREV-005` |
| `standards-review.md` | FAIL | `REPO-001` through `REPO-003` |
| `domain-review-statistics.md` | FAIL | `STAT-001`, `STAT-002` |
| `domain-review-data-durability.md` | FAIL | `DATA-001`, `DATA-002` |
| `domain-review-security-tenancy.md` | FAIL | `SEC-TEN-001`, `SEC-TEN-002` |
| `findings-validation.md` | SPEC_REVISION_REQUIRED | Nine retained, independently validated findings |

All required reviewers completed within the review cap. No sub-reviewer gap was recorded.

## Validated finding ledger

The decision-complete evidence, caller traces, corrections, and closure proofs are preserved in `findings-validation.md`.

| Finding | Status | Classification | Summary |
|---|---|---|---|
| `FIND-TASK-005-1` | CONFIRMED | INCORRECT | A never-created Custom observation table produces a terminal engine error instead of an inconclusive result. |
| `FIND-TASK-005-2` | REVISED | MISSING | The required Rust/Python/TypeScript production Drift method and activation journey matrix is incomplete. |
| `FIND-TASK-005-3` | CONFIRMED | DRIFT | Repository-wide implementation/review skill changes are unrelated to TASK-005. |
| `FIND-TASK-005-4` | CONFIRMED | VIOLATION | Specifically named new tests lack the required exact focused-run evidence. |
| `FIND-TASK-005-5` | REVISED | VIOLATION | Production SPC retains all aggregate batches, subgroup means, and zones instead of bounded rule state. |
| `FIND-TASK-005-6` | REVISED | VIOLATION / SPEC REVISION REQUIRED | Drift manufactures query authority for the result-writer-only SYSTEM principal; no approved internal read identity or capability exists. |
| `FIND-TASK-005-7` | CONFIRMED | VIOLATION | The new Drift test module lacks mandatory module rustdoc. |
| `FIND-TASK-005-8` | CONFIRMED | VIOLATION | Baseline fitting bypasses the shared Verifier/baseline global and per-tenant permit ceiling. |
| `FIND-TASK-005-9` | CONFIRMED | VIOLATION | Compressed-size checking does not bound decoded baseline work, timeout, or cancellation. |

No Wave 1 finding was rejected. Overlapping SPC, journey, and shared-permit findings were deduplicated by the independent Wave 2 reviewer.

## Prior-finding closure

This is the first review attempt for TASK-005. There are no prior `FIND-TASK-005-*` findings or remediation tasks to reassess.

## Verification limits

- Reviewers inspected the complete base-to-candidate diff and relevant source, callers, authorities, tests, manifests, schemas, and recorded implementation evidence.
- The implementation record reports the broad required lanes green. The standards reviewer additionally ran `check:skills-sync`, `py:format`, `py:lints`, and `py:typecheck`, all successfully.
- The review did not rerun the broad suites or missing focused selectors. Missing selector-level proof is itself `FIND-TASK-005-4`.
- No high-expansion Parquet denial-of-service fixture or long-window SPC benchmark was executed; the retained allocations and detached blocking path were established through full-body and caller inspection.
- `.codegraph/` is absent, so reviewers used repository text, Git diff, and direct caller tracing.

## Verdict

**SPEC_REVISION_REQUIRED**

Eight findings are bounded implementation, scope, documentation, journey, or evidence corrections. `FIND-TASK-005-6` cannot be safely packaged as an implementation remediation task: mandatory scheduled Drift reads need an approved tenant-bound internal identity or capability, its exact scope, construction or issuance path, and audit semantics. The current authorities reserve SYSTEM for result publication and provide no sanctioned replacement. Choosing one in this review would create a new security and architecture decision.

No remediation task is written. Revise and approve the specification and security authority for internal Drift observation reads, then derive remediation work that also closes the remaining validated findings.
