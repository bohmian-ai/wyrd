# TASK-011 r3 review verdict

**Verdict: FIX_REQUIRED**

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t005`, branch `vcc/task-005`.
- Base: `338f33235f81c30dfe3a570dc26934fe7bb77048`.
- Candidate: `c5c7a76cc1f45f5bdfad20de35a957b9f2f9ce57`.
- Authority: [approved specification revision 38](../../spec.md), [original TASK-011](../../tasks/TASK-011-conventional-psi-spc.md), both [r1](../TASK-011-r1/verdict.md) and [r2](../TASK-011-r2/verdict.md) verdicts and ledgers, and their remediation tasks.
- Scope: the full cumulative `338f3323..c5c7a76c` diff. HEAD stayed at the candidate through both review waves. The reviewers inspected source and recorded verification evidence without rerunning implementation tests.

## Acceptance matrix

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| REQ-153, exhaustive frozen PSI bins, `other`, smoothing and minimum sample | Vala fit/score and server SQL; mixed 100/99 feature fixture, duplicate-series fixture and SDK journeys. | PASS |
| REQ-154–155, fixed rational SPC groups, NIST X-bar/S math, typed evidence | Contract, Vala control limits and scorer, ordered server aggregates; independent formula and SDK evidence fixtures. | PASS |
| REQ-156, baseline/target completeness and direct/server agreement | Vala and server checks give whole-report inconclusive for typed nulls, partial groups and short windows. Direct `DataType::Null` columns instead return a type error (FIND-TASK-011-12). | FAIL |
| REQ-157, new fitted versions, old-fit refusal and historical reads | Fitted-format check and Rust/Python/TypeScript journeys. | PASS |
| INV-012, preserve tenant, query/audit, runtime, Custom and result/dispatch boundaries | One scoped SYSTEM-token query, existing Oracle cut and fold, existing result publication; query and security domain inspections. | PASS |
| AC-034, statistical fixtures and first-class SDK journeys | NIST/PSI fixtures and scheduled/legacy proof in all three SDKs; no direct `NullArray` fixture (FIND-TASK-011-12). | FAIL |
| Repository verification and task lifecycle | Recorded fmt/lint, Vala, Wyrd, Bifrost, OpenAPI, codegen, docs and exact named tests pass. Both implemented remediation task headers still say `ready` (FIND-TASK-011-13). | FAIL |
| Non-goals and prior findings | No extra chart, missing bin, migration, observation format or new query path. FIND-TASK-011-1 through -11 remain closed. | PASS |

## Independent results

| Wave | Report | Result |
|---|---|---|
| Task implementation | [task-review.md](task-review.md) | PASS |
| Repository standards | [standards-review.md](standards-review.md) | FAIL |
| Statistics | [domain-review-statistics.md](domain-review-statistics.md) | FAIL |
| Query and persistence | [domain-review-query-persistence.md](domain-review-query-persistence.md) | PASS |
| Security and tenancy | [domain-review-security.md](domain-review-security.md) | PASS |
| Independent Ponytail validation | [findings-validation.md](findings-validation.md) | Two confirmed findings; FIX_REQUIRED |

The independent validator resolved the task review's PASS against the direct Arrow null-type path and the implemented remediation task states. Both Wave 1 proposals were confirmed; none was rejected.

## Validated findings

| Finding | Status | Required closure |
|---|---|---|
| FIND-TASK-011-12 | CONFIRMED; INCORRECT | Treat a present direct PSI/SPC `DataType::Null` column as incomplete so the existing whole-report guard returns unscored; retain errors for non-null wrong types. |
| FIND-TASK-011-13 | CONFIRMED; VIOLATION | Set both implemented R1/R2 remediation task headers to `status: review`. |

The [TASK-011-R3 remediation task](TASK-011-R3-production-drift-closure.md) provides focused corrections and proof. Neither finding needs a new product, API, security, concurrency, or persistence decision.

## Verification limits

This was a static review. The recorded green lanes use typed nullable Arrow arrays and server SQL nulls; they do not exercise a direct `NullArray` feature. The single Oracle cut is established by one statement rather than an injected mid-query write. SDK JSON cannot carry NaN or infinity, so Vala and SQL tests own those cases. The 99-record duplicate case is proven at SQL/fold level, with the ordinary publication branch covered by existing journeys.
