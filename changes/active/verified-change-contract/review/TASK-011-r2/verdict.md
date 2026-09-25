# TASK-011 r2 review verdict

**Verdict: FIX_REQUIRED**

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t005`, branch `vcc/task-005`.
- Base: `338f33235f81c30dfe3a570dc26934fe7bb77048`.
- Candidate: `3c6fc1880692cc29c4c1ff7fc5fcb72a850bb9d6`.
- Authority: approved [specification revision 38](../../spec.md), [TASK-011](../../tasks/TASK-011-conventional-psi-spc.md), the [r1 verdict](../TASK-011-r1/verdict.md), and [TASK-011-R1](../TASK-011-r1/TASK-011-R1-production-drift-closure.md).
- Scope: the complete cumulative `338f3323..3c6fc188` diff. HEAD stayed at the candidate through both review waves. Reviewers inspected source and recorded evidence; they did not rerun test lanes.

## Acceptance matrix

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| REQ-153, exhaustive PSI bins, reserved `other`, frozen proportions and minimum sample | Vala PSI fit/score and server SQL; Vala and SDK fixtures. One short feature can be outranked by a drifting sibling (FIND-TASK-011-8). | FAIL |
| REQ-154–155, fixed rational SPC subgroups, NIST X-bar/S limits, typed evidence | Contract validation, Vala control limits/scorer, ordered server aggregate; independent formula fixtures and all three SDK journeys. | PASS |
| REQ-156, selected-row completeness and wholly unscored insufficient targets | Direct selected-row check, server completeness query, one-cut fold; null-only and partial SPC tests. PSI duplicate series can inflate one feature count (FIND-TASK-011-8). | FAIL |
| REQ-157, version/refit boundary and historical results | Fitted-format refusal and retained result reads; Rust, Python and TypeScript journeys. | PASS |
| INV-012, tenant, auth/audit, query, runtime, Custom and publication boundaries | Scoped SYSTEM token, query service/Oracle, one statement, existing publication; domain inspection and recorded Bifrost lanes. | PASS |
| AC-034, statistical and first-class SDK journeys | NIST/PSI fixtures, scheduled Operator dispatch and stored-legacy refusal in Rust/Python/TypeScript; no proof for mixed-count PSI (FIND-TASK-011-8). | FAIL |
| Public contract and repository standards | Schema/OpenAPI gates recorded; changed Rust items miss required rustdoc/import style (FIND-TASK-011-9/10); changed `wyrd-spec` tests lack execution evidence (FIND-TASK-011-11). | FAIL |
| Non-goals and prior r1 findings | No new chart, missing bin, migration, public route, or scorer; FIND-TASK-011-1 through -7 are closed by source and recorded proof. | PASS |

## Independent results

| Wave | Report | Result |
|---|---|---|
| Task implementation | [task-review.md](task-review.md) | PASS |
| Repository standards | [standards-review.md](standards-review.md) | FAIL |
| Statistics | [domain-review-statistics.md](domain-review-statistics.md) | FAIL |
| Query and persistence | [domain-review-query-persistence.md](domain-review-query-persistence.md) | PASS |
| Security and tenancy | [domain-review-security.md](domain-review-security.md) | PASS |
| Independent Ponytail validation | [findings-validation.md](findings-validation.md) | Four retained findings; FIX_REQUIRED |

The independent validator resolved the task review's PASS against the reachable mixed-count PSI path and repository standards failures. Its [ledger](findings-validation.md) confirms or revises every Wave 1 proposal; no proposal was rejected.

## Validated findings and disposition

| Finding | Status | Required closure |
|---|---|---|
| FIND-TASK-011-8 | REVISED; INCORRECT | Unscore the whole PSI report before math when any feature has fewer than 100 samples; reject repeated configured-series rows in the existing completeness query. |
| FIND-TASK-011-9 | CONFIRMED; VIOLATION | Add required rustdoc, `# Errors`, and `# Panics` to the identified changed items and audit the remaining changed Rust items. |
| FIND-TASK-011-10 | REVISED; VIOLATION | Import and use bare type names in the identified new/materially changed signatures. |
| FIND-TASK-011-11 | CONFIRMED; VIOLATION | Execute and record owning `wyrd-spec` validation tests with exact commands. |

The single [TASK-011-R2 remediation task](TASK-011-R2-production-drift-closure.md) gives the correction and proof for all four. None needs a new product, public API, security, concurrency, or persistence decision.

## Verification limits

Recorded fmt, lint, Vala, server, OpenAPI, codegen, docs and Rust/Python/TypeScript journey lanes passed at the candidate. Static review did not rerun them. One Oracle statement establishes the common query cut structurally; there is no mid-query ingest injection. JSON SDK observations cannot carry NaN or infinity, so Vala and server SQL tests cover those inputs. The passing lanes do not exercise mixed PSI feature counts or the changed `wyrd-spec` test bodies.
