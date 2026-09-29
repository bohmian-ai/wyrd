# TASK-001 cumulative re-review verdict

**Verdict: FIX_REQUIRED.** Repository `/home/thorrester/Documents/GitHub/wyrd-vcc-t005`; immutable base `d1ec13200d332745af2fed8069a21d5b5c39cb47`, candidate `f1f1d5ebd264e8f9ac861ec79c6340da44a7a1a8`. Reviewed approved [spec revision 3](../../spec.md), [original TASK-001](../../tasks/TASK-001-unified-scribe-live-query.md), the [prior verdict](../task-001-codex-20260926/verdict.md), [prior finding ledger](../task-001-codex-20260926/findings-validation.md), [R1 remediation](../task-001-codex-20260926/TASK-001-R1-preserve-query-failure-and-resource-semantics.md), and the full cumulative diff. The approved review-skill commit `e15c610af` is separate from task implementation. Candidate HEAD stayed unchanged.

## Reconciled acceptance

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| REQ-001 / AC-001: one request and valid three-way terminal | `wyrd-spec/src/vala/api.rs`, shared client, CLI/MCP/Python/TypeScript projections; contract, codegen and client journeys | PASS |
| REQ-002 / AC-002: discover and select bound live owners | `oracle/mod.rs::discover_live_routes`, authenticated listing and routing journey; R1 typed listing errors. Proposed wrong-node path is unreachable through the real ticket-bound Scribe listing response. | PASS |
| REQ-003 / AC-003 / INV-003: one plan, distributed published work, local live scans | Oracle planner, live source and follower; published-worker/multiple-Scribe journey | PASS |
| REQ-004 / AC-005 / INV-002: truthful failures and early LIMIT stop | R1 typed fragment failures; real ticket, capacity, availability, late-loss and footer journeys; LIMIT journey | PASS, with unit-level staged schema/decode proof limit |
| REQ-005 / AC-004 / INV-005: bounded query-owned streaming and cancellation | `StagedRunWindows` moves IO to Tokio blocking pool and retains the file lease; current empty-window filter can decode a whole run in one step, and an aborted stream releases Scribe admission while a step remains active. | FAIL: FIND-TASK-001-3, FIND-TASK-001-7 |
| REQ-006 / INV-001: approved publication overlap and unchanged ACK/order | Pinned cut and staged authority source; architecture and journeys; no changed ACK path | PASS |
| REQ-007 / AC-006 / INV-004: verification uses same terminal-safe query | `ScheduledQueryCaller` and Drift live-row journey; Failed-to-no-verdict has unit proof only | PASS with recorded journey limit |
| AC-007: old tail fence gone, authenticated listing retained | Deleted acquire/page/release path, source inspection, Oracle journey | PASS |
| AC-008 and repository rules | Recorded `verify:bifrost` 9/9, full gate 48/48, format/lint/codegen/docs; changed Rust still has local imports, qualified discovery signatures, and wrong error rustdoc. | FAIL: FIND-TASK-001-5, -6, -8 |
| Non-goals | No new mode, tail lease, planner, persistent owner index, Scribe worker role, or ACK change; approved skill edit is separately authorized. | PASS |

## Independent reviews and finding ledger

[Behavior](task-review-behavior.md) PASS; [invariants](task-review-invariants.md) FAIL; [standards](standards-review.md) FAIL; [security](domain-review-security.md) PASS; [concurrency](domain-review-concurrency.md) PASS; [durability](domain-review-durability.md) FAIL. A [focused follow-up](followup-review.md) resolved the staged-resource conflict and empty-window path. The fresh [Ponytail validation](findings-validation.md) checked all proposals against production source, rejected the unreachable wrong-node proposal, and retained only these findings:

| Finding | Classification and required boundary |
|---|---|
| FIND-TASK-001-3 | INCORRECT / VIOLATION: skip empty staged windows after each awaited blocking step, so cancellation stops further decoding. |
| FIND-TASK-001-5 | VIOLATION: move two changed function-local imports to module import blocks. |
| FIND-TASK-001-6 | VIOLATION: use imported bare types in the new discovery fields and signatures. |
| FIND-TASK-001-7 | INCORRECT / VIOLATION: retain the existing Scribe follower grant through an already-started blocking window after stream cancellation. |
| FIND-TASK-001-8 | VIOLATION: document `TailReadError` rather than an unrelated `BifrostError` on discovery. |

Prior FIND-1, FIND-2, and FIND-4 are closed. FIND-3 is partly closed by moving staged IO off the async runtime, but its per-pull bound remains open. FIND-5 and FIND-6 remain open at additional changed symbols. FIND-7 and FIND-8 are new. The existing query-derived grant can cover only the already-started protected step under the approved bounded-resource invariant; this requires no second deadline or new resource policy. The reported green gates were not rerun in this read-only review and do not exercise the retained paths or enforce the cited source rules. Implement [TASK-001-R2](TASK-001-R2-bound-staged-cancellation-and-close-source-rules.md), then review the full base-to-new-candidate range again.
