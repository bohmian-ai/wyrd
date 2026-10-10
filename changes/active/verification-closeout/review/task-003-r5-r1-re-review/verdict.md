# TASK-003 R1 re-review verdict

**Verdict: FIX_REQUIRED**

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-verification-closeout-task3`.
- Base: `7f79fb3417db651adedac194ada8908f0a0372d7`.
- Candidate: `9a8f9f7eef95f70d356c037a192b7d7b90a37f31`.
- Approved spec: `changes/active/verification-closeout/spec.md` revision 3.
- Original task: `changes/active/verification-closeout/tasks/TASK-003-r4-canonical-support-desk-closeout.md`.
- Prior review and remediation: `review/task-003-r4-canonical-closeout/verdict.md`, `findings-validation.md`, and `TASK-003-R1-canonical-closeout.md`.
- Reviewed the complete base-to-candidate diff; `f6c841d57..9a8f9f7ee` located the seven-finding remediation. The candidate HEAD did not change during review.

## Reconciled acceptance matrix

| Obligation | Source and verification evidence | Result |
|---|---|---|
| Declared Service tables, optional activation, direct and continuous verification results, shared non-blocking Scribe outbox | Cumulative Card, verification, result, outbox, and SDK implementation; original green gate and focused journeys recorded in task | PASS |
| Signed Card UID or tenant-registry gateway authorization before dispatch; captured UID, no CardRef alias | Cumulative gateway ingress/invocation/capture and client contract; security review found no R1 regression | PASS |
| Exact Prompt gateway deployment and actionable refusal in each SDK example | Three deploy functions compare `openai/gpt-4o`; each real-server journey rejects an `anthropic/gpt-4o` collision before accepting the correct pair | PASS; prior FIND-1 closed |
| Late-Scribe delivery after an audit is already staged | Updated journey holds Scribe unbooted, stages the audit, boots Scribe, drains Oracle, and reads retained history; exact focused run recorded green | PASS; prior FIND-6 closed |
| Oracle shutdown waits for all Oracle-owned query memory, excluding sibling-role memory | New predicate excludes sibling governed memory, but its counter omits Oracle infallible headroom retained by a child pool | FAIL; FIND-2 remains |
| Forge unexpired self-reclaim only after predecessor plan and heartbeat work stops | Event-loop guard covers normal and event-loop panic paths; startup recovery can spawn detached work before the guard and quiescence marker are installed | FAIL; FIND-7 remains |
| Container `psql` role rerun propagates SQL errors | Second invocation has `ON_ERROR_STOP=1`; roles lane and deliberate SQL-error check recorded green | PASS; prior FIND-5 closed |
| Active Card UID correlation guidance and required Rust documentation | Observation passages now use `card_uid`; prior fixture and `main` docs are present, but a new Forge tuple field has no rustdoc | FAIL; prior FIND-3 and -4 closed, new FIND-8 |
| No extra alias, result sink, memory governor, persistent Forge fence, or unrelated behavior | Cumulative and remediation diff inspection; no validated drift finding | PASS |

## Independent review results

| Report | Result | Reconciliation |
|---|---|---|
| `task-review-behavior.md` | PASS | Closure claim narrowed by the follow-up and validation for Oracle headroom and Forge startup. |
| `task-review-invariants.md` | FAIL | Forge startup path retained as FIND-7. |
| `standards-review.md` | FAIL | Missing tuple-field rustdoc retained as FIND-8; placement notes are non-blocking. |
| `maintainer-review.md` | PASS | No material maintainer finding. |
| `system-review.md` | FAIL | Oracle and Forge recovery paths retained as FIND-2 and FIND-7. |
| `domain-review-data.md` | FAIL | Independently raised the same two runtime paths. |
| `domain-review-security.md` | PASS | No material security or tenancy finding. |
| `followup-review.md` | RESOLVED | Traced both disputed runtime paths through producers and consumers. |
| `findings-validation.md` | Complete | Fresh Ponytail reviewer independently validated every proposal and produced the deduplicated ledger. |

The follow-up was required because behavior review reported closure while invariant, system, and data reviewers found reachable residual states. The final validation confirmed both states from source. The import-placement and Python explanatory-string notes have no demonstrated behavior or public-contract consequence and are non-blocking.

## Validated findings and closure

| ID | Status | Required correction boundary |
|---|---|---|
| FIND-TASK-003-1 | Closed | Exact gateway provider/model comparison and refusal are present in all three examples and journeys. |
| FIND-TASK-003-2 | Open, INCORRECT | Attribute Oracle infallible headroom within the existing governor and include it in Oracle shutdown's predicate and residual report, without counting Forge or Scribe bytes. |
| FIND-TASK-003-3 | Closed | The two previously missing item docs are present. |
| FIND-TASK-003-4 | Closed | Active observation-correlation guidance uses Card UID. |
| FIND-TASK-003-5 | Closed | The role rerun stops on SQL error. |
| FIND-TASK-003-6 | Closed | The journey stages the audit before Scribe boots. |
| FIND-TASK-003-7 | Open, REGRESSION | Cover startup recovery with existing quiescence and cancellation ownership before it can spawn work; keep the predecessor state available for the initial reclaim decision. |
| FIND-TASK-003-8 | New, VIOLATION | Add required rustdoc to the new Forge quiescence tuple field. |

Full producer-to-consumer evidence, exact source locations, dispositions, and proof boundaries are in `findings-validation.md`. The bounded remediation task is `TASK-003-R2-oracle-and-forge-recovery.md` in this directory. These corrections use existing owners and stay within the approved behavior.

## Verification limits

This read-only review did not rerun the recorded test lanes. The R1 focused tests exercise governed Oracle child memory and Forge panic after the quiescence guard is installed; they do not exercise headroom-only Oracle child memory or interrupted Forge startup recovery. The task's original `mise run -c gate` and R1 focused results remain recorded evidence for their stated paths, not proof of these residual states.
