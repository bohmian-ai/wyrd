# Expanded remediation proposal comparison

Subject and authority pins are in review-scope.md. Five selected discovery
reports are present. Claims are grouped by violated invariant rather than
line, and all remain proposals pending fresh independent validation.

| Cause | Sources | Material question / correction proposed |
|---|---|---|
| Python process-global setup can split publication between concurrent states | INV-001 in task-review-invariants.md; system-review.md has no finding for this path | Invariant proposes shared install synchronization; system questions supported consequence under required process restart. Focused followup-review.md resolves the interleaving as reachable but excludes harm to an already successful/app-owned provider and recommends rejecting the blocker under the approved failure recovery contract. Validator must independently decide. |
| New changed-item Rust docs omit tuple-field meaning / lifecycle-test panic conditions | REPO-R1-001 | Original FIND-TASK-003-2 locations are corrected. New expanded items fail the same explicit merge gate; validator determines stable ledger/closure treatment and minimum local docs. |
| Pure Python Trace runtime and stub both declare its class | MAINT-003-R1-1; standards notes rule deviation without retaining it | Maintainer identifies declaration-owner maintenance burden and re-export pattern; standards finds no supported typing/call regression or explicit merge gate. Validator resolves from local rule, concrete cost/materiality and smallest correction; agreement is not proof. |
| Rust refund setup omits explicitly named configure/state.flush calls | EV-1 in domain-review-evidence.md | Actual evidence completeness still passes through Telemetry::shutdown's force flush. Validator checks exact AC-016 wording against approved purpose and smallest correction; no new evidence protocol is proposed. |

Standards also records the absence of a committed TypeScript missing-optional-
package unit case under AC-016. Its reported proof-limit classification is
kept distinct from runtime failure; validator must assess the explicit evidence
obligation and any report addendum rather than silently treat source tracing
as executed unit proof. Dirty opt-out tests and direct-model followups are not
candidate contents.

One followup-rev was justified for the concrete concurrency/recovery conflict;
its exact report is followup-review.md. Other differences are resolvable from
source/approved obligation and materiality in the mandatory independent
validation pass, with no additional discovery team. No validator-origin
finding may be retained without separate source validation.
