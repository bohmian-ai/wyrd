# TASK-005 R1 acceptance verdict

**Verdict: FIX_REQUIRED.** Immutable cumulative subject: base `05d7d741304af3b0b4e667e7e18f93dec16b897b`, candidate `1fc68f3b78c4dbf82a8f1c518bbc40343c484d65` on `vcc/task-004`. The candidate remained fixed throughout this review. No implementation source was edited, merged, pushed, or deployed.

The [user-designated original task](/home/thorrester/Documents/GitHub/wyrd-pr-95/changes/active/bifrost-scribe-live-reads/tasks/TASK-005-simplify-bifrost-telemetry.md) is revision 14 (SHA-256 `0e62758cd6790451c2fa7497c06f6eba7d57400ed15ea38b50695c30b0177814`); the approved `changes/active/bifrost-scribe-live-reads/spec.md` is revision 24. This repeat review includes the [prior verdict](../task-005-review-20261001/verdict.md), [prior validated findings](../task-005-review-20261001/findings-validation.md), [R1 remediation](../task-005-review-20261001/TASK-005-R1-close-telemetry-proof.md), and the complete base-to-candidate diff. The user explicitly deferred `mise run gate` to another branch, so its absence is not a finding here and the review makes no claim that it passed.

## Reconciled acceptance matrix

| Original obligation or boundary | Implementation and verification evidence | Result |
| --- | --- | --- |
| Readable, correlated write, local/remote query, and Forge traces with true terminal lifetime and one failure reason | Original focused trace assertions and R1 retained production captures; all 147 Bifrost journey tests passed | PASS |
| Owner-derived Scribe staging measurements remain current after concurrent mutation and restore | R1 publishes each transition's backlog under the existing assembler lock; four-thread owner test, fresh-recorder restore test, and real-server restart journey pass | PASS; prior FIND-1 closed |
| Abruptly restarted server exposes restored backlog, serves acknowledged rows, and settles after publication | Named `staged_backlog_survives_abrupt_restart` journey scrapes matching owner facts before/after kill and zero after publication; fresh-recorder restore proof remains | PASS; prior FIND-2 closed |
| Six dashboard questions have production samples, independent facts, trace parentage, and family inventory | Original task's R1 evidence has numeric transitions, trace IDs, 105-family inventory, and 18 removed families; raw journey logs retained | PASS; prior FIND-3 closed |
| Standard candidate benchmark remains valid and meets approved selective latency target | Candidate standard run exits 1: one-client selective p50/p95/p99 `9.3/12.6/15.5 ms` against `<7/<10/<10 ms`; older `6.8 ms` run is from an earlier ancestor and host-contention attribution is unproved | FAIL; FIND-TASK-005-4 |
| Changed declarations follow module-top import and bare-type rule | R1 fixed original cited sites, but additional changed Oracle stream/tracer, Scribe persistence, and test-inspection declarations retain qualified types | FAIL; FIND-TASK-005-5 |
| Candidate contains only TASK-005 work | Cumulative range includes `1f1cbcf5f`, adding a separate 224-line draft dashboard spec that the initial review expressly excluded | FAIL; FIND-TASK-005-6 |
| Preserve ACK, WAL, publication, admission, terminal, tenant, Forge settlement, SDK, and recovery boundaries | Focused/whole journeys passed; concurrency, durability, security/tenancy, and system reviewers found no validated runtime regression; SQL release-build fix only boxes the existing begin future | PASS on reviewed source |

## Independent review results

| Report | Result and claims |
| --- | --- |
| [Behavior](task-review-behavior.md) | FAIL; B-R1-001 benchmark, B-R1-002 dashboard drift |
| [Invariants](task-review-invariants.md) | FAIL; INV-R1-1 benchmark, INV-R1-2 dashboard drift |
| [Standards](standards-review.md) | FAIL; STD-R1-001 benchmark, STD-R1-002 lint evidence |
| [Maintainer](maintainer-review.md) | FAIL; M-1 changed qualified declarations |
| [System resilience](system-review.md) | PASS; no source-local finding |
| [Concurrency](domain-review-concurrency.md) | PASS; no finding |
| [Durability](domain-review-durability.md) | PASS; no finding |
| [Security and tenancy](domain-review-security.md) | PASS; no finding |
| [Performance](domain-review-performance.md) | FAIL; PERF-1 benchmark |
| [Focused follow-up](followup-review.md) | RESOLVED; maintainer's import claim is supported by changed declarations, despite standards' broader import-rule PASS |
| [Structured Ponytail validation](findings-validation.md) | Retained three deduplicated findings; rejected missing-lint claim after [reviewer verification](verification.md) |

The follow-up was required by the conflicting import-rule assessments. Independent validation traced the disputed declarations and their callers, preserved prior finding IDs, and rejected only STD-R1-002 because this review ran final-tree `mise run lints` successfully. The benchmark reports and raw samples establish a missed target; they do not prove its cause. A benchmark rerun was not performed at the user's direction.

## Validated finding ledger and prior closure

| ID | State | Closure needed |
| --- | --- | --- |
| FIND-TASK-005-1 | Closed | Concurrent stage gauges now publish under the owner lock. |
| FIND-TASK-005-2 | Closed | Production abrupt-restart journey passes. |
| FIND-TASK-005-3 | Closed | Six-question production evidence and inventory are recorded. |
| FIND-TASK-005-4 | **Open — VIOLATION** | A valid standard candidate benchmark must meet the approved selective target; investigate the actual cause if a quiet run still misses. |
| FIND-TASK-005-5 | **Open — VIOLATION** | Correct the remaining changed qualified declarations under the existing import rule. |
| FIND-TASK-005-6 | **New — DRIFT** | Present an immutable TASK-005 candidate excluding the unrelated dashboard draft. |

The [validated ledger](findings-validation.md) gives source evidence and correction boundaries; [TASK-005-R2-close-capacity-and-scope.md](TASK-005-R2-close-capacity-and-scope.md) packages the bounded remaining work. The 147 passing journeys, final-tree lints, and release-build/SQL evidence do not override the red benchmark target. The broad gate remains deliberately unrun under the user's override.
