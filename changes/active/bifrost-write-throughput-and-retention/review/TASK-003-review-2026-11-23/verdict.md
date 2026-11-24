# TASK-003 review: FIX_REQUIRED

The implementation is not ready for pre-benchmark acceptance. Independent
discovery and validation retain two scheduling defects, one authority wording
correction and two executable-proof gaps. The required multi-tenant/table test
is included in [one remediation task](TASK-003-R1-pipeline-scheduling-and-proof.md).
No implementation or benchmark was run automatically by review.

## Immutable subject and authority

- Base: `74034459d468fd0adf277e3b51c32b1a676e8e55`.
- Candidate: `c0900bff523e83d3666658440daa81355067cb34`.
- Reviewed range: `git diff 74034459d...c0900bff5`.
- Approved contract: [SPEC-bifrost-write-throughput-and-retention revision 4](../../spec.md).
- Original task: [TASK-003](../../tasks/TASK-003-bounded-write-pipeline.md).
- Packet hashes, excluded dirty work and owner navigation: [subject](subject.md).

Candidate HEAD and packet hashes remained unchanged throughout review. This is
the first TASK-003 review; prior TASK-001/002 reports do not establish closure
for the pipeline. The intentionally uncommitted packet was supplied by the
maintainer and was not edited. Unrelated benchmark/capacity work was excluded.

Governing references: root and affected SDK AGENTS, spec-driven development,
maintainer style, TESTING, Bifrost Ingest: Scribe and live recovery/receipt-custody
sections, and hosted-capacity authority for the collateral Sandbox changes.
The maintainer explicitly confirmed during this review that healthy interleaving
must be tested with multiple tenants and tables through the current write logic.

## Independent coverage and findings

| Reviewer | Assigned boundary | Report |
|---|---|---|
| behavior_rev | Requirements, scenarios, overlap/ACK/collection, repository rules and maintainability | [Behavior](task-review-behavior.md) |
| invariant_rev | Tenant acceptance, ambiguity/recovery, retained IO/resources, lifecycle and cleanup custody | [Invariants](task-review-invariants.md) |
| repo_rev | Collateral hosted admission, Rust/Python/TypeScript testing consumers and Forge accounting assertion | [Collateral](task-review-repo.md) |
| ponytail_rev | Fresh independent source validation of every finding, proof strength, complete coverage and specialist applicability | [Validation](findings-validation.md) |

These assignments cover concurrency/persistent-data, system recovery and owner
maintainability directly. Separate domain/system/maintainer reviewers would
duplicate covered boundaries. No selected report is missing; the orchestrator
did not discover or source-validate findings.

The **reconciled requirement/scenario matrix** in
[findings-validation.md](findings-validation.md#reconciled-coverage-matrix) is
incorporated here as the verdict's acceptance record. It connects requirements,
production owners, actual assertions and execution evidence without redefining
the approved contract. Retained findings are recorded once in that same file:

| Finding | Classification | Required closure |
|---|---|---|
| FIND-TASK-003-1 | INCORRECT | Service ready head completion before next preparation; bound mailbox service. |
| FIND-TASK-003-2 | INCORRECT | Excluded in-flight identities cannot satisfy the eligible collection target. |
| FIND-TASK-003-3 | INCORRECT documentation | Describe 32 as a soft waiting target; retain ready draining to existing limits. |
| FIND-TASK-003-4 | EVIDENCE_GAP | Prove healthy repeated interleaving across multiple tenants/tables through the real shard/WAL/Postgres pipeline. |
| FIND-TASK-003-5 | EVIDENCE_GAP | Bind executable proof to ambiguous live COMMIT followed by unavailable reconciliation and retained younger ownership. |

The two evidence gaps do not allege production defects. Supply missing tests or
valid existing proof; change production only if a test exposes incorrect behavior.
The cleanup concern was not retained as a blocker: independent tracing checked
segment rollover, retained references and persisted contiguous frontier custody,
with unchanged downstream journey evidence. Collateral fixes have no retained
finding. No new blocker arose during validation, so no additional validation
round is needed.

## Verification and process limits

Independent review execution, all successful at the unchanged candidate:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::shards::tests::pg_tests::later_groups_sync_while_earlier_groups_fence_and_ack_independently)'
mise run ts:typecheck
mise run ts:napi:check
mise exec -- git diff --check 74034459d...c0900bff5
mise exec -- git diff --check
```

The focused overlap run passed 1/1, nextest run
`22116042-a9f1-4d86-8ff0-b5b737c8bdd5`; 1009 tests were not selected.
TypeScript checks close the missing recorded finishing-check evidence identified
by the collateral reviewer. Generated builds left no additional tracked changes.

Reviewers independently inspected assertions and connected production paths.
The final lib-suite 1010/1010 three times, scoped journeys/integration, other SDK
and lint results remain implementation-record evidence; review did not rerun
those full suites. Passing existing tests does not establish the five missing
closures. No source, spec or original task changes were made by this review.

Behavioral acceptance: **FIX_REQUIRED**. TDD compliance: **VIOLATED**, preserving
the task's disclosed code-before-test history. Later tests cannot repair that
history; remediation requires no fabricated historical RED or production churn.

Performance qualification: **PENDING**, not a code defect. REQ-020 deliberately
remains unresolved under the maintainer's instruction to pass code review first.
After remediation/review and required correctness checks, the maintainer runs
`mise run benchmark:l1:bifrost-writes` independently as the final required execution,
comparing only against `design/l1-baseline-74034459d.md`. The task remains open;
this review neither requests an early benchmark nor grants completion.

Return [TASK-003-R1](TASK-003-R1-pipeline-scheduling-and-proof.md) for maintainer-owned
execution. Review does not start implementation, merge, push or deploy.

The independent validator also checked the remediation packet for comprehension
and alignment. Its one requested clarification was applied: failed reconciliation
must leave both groups' waiters unanswered, not merely suppress success ACKs.
