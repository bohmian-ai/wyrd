# TASK-001 review verdict

**Verdict: FIX_REQUIRED.** Reviewed repository `/home/thorrester/Documents/GitHub/wyrd-vcc-t005`, immutable range `d1ec13200d332745af2fed8069a21d5b5c39cb47..f9115fbbf6b6f116cf5ec5fe5582a9543107955a`, against approved [spec revision 3](../../spec.md) and [original task](../../tasks/TASK-001-unified-scribe-live-query.md). Candidate HEAD remained `f9115fbbf6b6f116cf5ec5fe5582a9543107955a` throughout review. The user-approved review-skill commit `e15c610af` is separate from TASK-001 work. No prior task-review findings exist for this subject.

## Reconciled acceptance

| Obligation | Source and verification evidence | Result |
|---|---|---|
| AC-001 / REQ-001: one request and terminal contract | `wyrd-spec/src/vala/api.rs`, SDK/CLI/MCP projections, contract and SDK journeys, codegen | PASS |
| AC-002 / REQ-002: discover and select live owners | Oracle discovery and distributed routing journey select relevant owners; listing refusal loses fatal error identity | FAIL: FIND-TASK-001-1 |
| AC-003 / REQ-003: one distributed published-plus-live plan | Oracle planner/live source and `published_workers_and_live_scribes_share_one_plan` journey | PASS |
| AC-004 / REQ-005: bounded, query-owned streaming and lifetime | In-memory backpressure and >30-second journey pass; staged Parquet decode blocks an async Scribe poll | FAIL: FIND-TASK-001-3 |
| AC-005 / REQ-004: truthful failure terminals and LIMIT stop | Existing failure matrix covers ordinary availability, late loss, footer, capacity hook, and LIMIT; listing and production fragment faults can incorrectly degrade | FAIL: FIND-TASK-001-1, FIND-TASK-001-2 |
| AC-006 / REQ-007: Drift uses the same query and only terminal results | Scheduled caller and Drift live-row journey; no separate source mode. Failed-to-no-verdict is unit-tested but has no server journey | PASS with verification limit |
| AC-007: retire tail fence; retain authenticated listing | Source removal and Oracle journey show old acquire/page/release path gone | PASS |
| AC-008: required gates and repository standards | Task records green scoped and broad gates; new Rust source violates documentation/import/signature rules | FAIL: FIND-TASK-001-4 through FIND-TASK-001-6 |
| INV-001, INV-003, INV-004; REQ-006 and non-goals | Reviews found no changed ACK/publication authority, second planner, local cross-pod file access, exactness claim, new public mode, persisted owner index, or compatibility path | PASS |
| INV-002 and INV-005 | Peer listing and fragment fault classes can lose fail-closed meaning; staged blocking delays resource cleanup | FAIL: FIND-TASK-001-1 through FIND-TASK-001-3 |

## Independent review and reconciliation

Behavior, invariant, standards, security, concurrency, and durability reviewers each reported FAIL. [Behavior](task-review-behavior.md), [invariants](task-review-invariants.md), [standards](standards-review.md), [security](domain-review-security.md), [concurrency](domain-review-concurrency.md), and [durability](domain-review-durability.md) reports cover their assigned scopes. A [focused follow-up](followup-review.md) was required because staged synchronous IO was a unique reachable path and the fragment findings needed source-level consolidation; it resolved both questions. The fresh [Ponytail validation](findings-validation.md) independently retained six deduplicated findings; no proposal was wholly rejected. This verdict includes only that validated ledger.

| Finding | Validated source IDs | Required correction boundary |
|---|---|---|
| FIND-TASK-001-1 | BEH-001, INV-01, SEC-001, DUR-001 | Preserve private discovery failure classes; Oracle degrades only actual ready-source availability loss. |
| FIND-TASK-001-2 | INV-02, CONC-001 | Preserve Scribe producer/follower reason through peer status; resource, schema, integrity, and execution faults fail. |
| FIND-TASK-001-3 | CONC-002, FOLLOW-01 | Move staged blocking Parquet work off the async Scribe poll while retaining bounded demand and ownership. |
| FIND-TASK-001-4 | STAND-001 | Document errors on six new fallible Rust journey functions. |
| FIND-TASK-001-5 | STAND-002 | Move four added local imports to module import blocks. |
| FIND-TASK-001-6 | STAND-003 | Import and use bare types in four changed Rust signatures. |

The task records green `verify:bifrost`, 33/33 Oracle journeys, and the broad gate; this source review did not rerun them. Those checks do not exercise the validated listing/fragment error classes or staged blocking. The original Drift concern remains a verification limit rather than a separate defect: its scheduled consumer rejects Failed and the caller propagates the error without recording a verdict; only that decision point has injected-failure proof. The approved spec requires no revision. Implement the bounded corrections in [TASK-001-R1](TASK-001-R1-preserve-query-failure-and-resource-semantics.md), then review the full cumulative candidate again.
