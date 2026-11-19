# Discovery claim comparison

All four selected discovery reports are present for the pinned candidate.

| Report | Result | Proposed material findings |
| --- | --- | --- |
| task-review-behavior.md | FAIL | BEH-002-1: print-only model visibility guidance contradicts existing outer code.run last-expression result. |
| task-review-invariants.md | PASS | None. |
| standards-review.md | PASS | None. Local Rust import/type-placement deviation explicitly classified non-blocking. |
| domain-review-security.md | PASS | None. |

One proposed cause/correction boundary: source documentation and shared example explanation of existing outer-output visibility, plus generated docs. No separate client or runtime correction is proposed. The other reports establish their scoped record/security/standards paths; their general documentation assessments do not trace or rebut the specific last-expression scenario. There is no conflicting runtime claim needing discovery arbitration. A unique finding alone is not a reason for a follow-up. No followup-rev selected; the mandatory fresh validator must independently test reachability, materiality, approved authority and the smallest correction of BEH-002-1, and independently check report/specialist coverage.

The proposed union above is not a validated ledger or intended verdict. No finding has been accepted by the orchestrator.
