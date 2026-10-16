# TASK-005 R3 verdict

**FIX_REQUIRED** — independently validated **FIND-TASK-005-7** and **FIND-TASK-005-14** remain. TASK-005 is not complete. Both are bounded implementation corrections within approved revision 9; no specification revision or migration is required.

## Immutable subject

Repository: `/home/thorrester/Documents/GitHub/wyrd-verifier-building-blocks`.
Base exclusive: `f2a726c90edbf8956c7b91c843379175ee20eac0`.
Candidate: `02848d737a621938ec0b5f817e1aa84d5f3df1fd`.
Prior candidate: `de69cb69b68eced72f7d760681d27253e013f8a1`.

All candidate source, authority and cumulative Git inspection used detached `/tmp/wyrd-task005-review-02848d737`. The recent fix range was a locator; the full cumulative task remained the acceptance subject. Main merge `1fa4526cc` supplies baseline integration context. The candidate stayed pinned and clean. Original-tree untracked reports and later edits are excluded; prior R2 reports were frozen without changing originals, with hashes in `prior-inputs.md`.

Authority: approved `changes/active/verifier-building-blocks/spec.md` r9, original `tasks/TASK-005-server-integration.md`, applicable repository rules, R1/R2 remediations and prior findings. Nothing has shipped: no upgrade/history/catalog migration or pending-work conversion applies. R9 already approves local evidence PUT and operation-level preparation; draft Agent-runtime-composition is excluded.

## Independent review and reconciliation

| Role / report | Discovery result | Reconciled result |
|---|---|---|
| Behavior — `task-review-behavior.md` | FAIL | Per-file admission proposal confirmed as FIND-14. |
| Invariants — `task-review-invariants.md` | FAIL | Same defect confirmed; residual-10 attribution revised to new FIND-14. |
| Repository — `standards-review.md` | FAIL | Mandatory panic documentation residual confirmed as FIND-7. |
| Maintainer — `maintainer-review.md` | FAIL | Admission proposal consolidated into FIND-14. |
| System — `system-review.md` | PASS | No additional material issue; Busy/result recovery closures supported. |
| Security/tenancy, data/durability, concurrency, evidence/trace, provider — five `domain-review-*.md` reports | PASS | No additional material proposals; domain PASS does not waive the distinct individual-cap gap. |
| Fresh focused follow-up — `followup-review.md` | RESOLVED | Pre-drive individual-cap obligation and new-ID distinction supported. |
| Fresh independent Ponytail — `findings-validation.md` | FIX_REQUIRED | Final deduplicated ledger: FIND-7 and FIND-14. |

The follow-up was required because repeated size remediation and conflicting stable-ID attribution left uncertainty about the shared admission source. It traced the narrow producer/consumer path; validation independently adjudicated all proposals and prior closures. No finding was accepted solely by reviewer agreement. Root orchestrated and packaged the validated ledger rather than acting as its own reviewer.

## Reconciled acceptance matrix

| Obligation family | Implementation and proof assessment | Result |
|---|---|---|
| REQ-001–003 / AC-001–002: declarative contract, graph, slots, missing/aggregation semantics | Existing contract/registration/engine owners and corrected slot validation; inspected tests and recorded focused proof | PASS, except independent deployment ceiling below |
| REQ-004–009 / AC-003–004: extraction, Agent/code, bounded tools, citations and Prompt/provider reuse | Existing owners; retained watch refusal cancels pending tools; Google schema replacement and typed signed history corrected | PASS within recorded proof |
| REQ-010 / Deployment limits / INV-006–007 / AC-006 | Tenant/integrity/slot and combined admission bounds correct; individual deployment file ceiling absent from pre-drive admission | **FAIL — FIND-14** |
| REQ-011–013 / AC-005–006: direct/queued/MCP, ACK-only submission, authority, leases/fencing/status | Existing shared control/engine, queue/outboxes and error mappings retained; oversized individual input still reaches a normal verdict | **FAIL only for FIND-14** |
| REQ-014 / AC-007: schedules, baselines, statistics and independent Operators | Explicit slot mapping and requirement baselines reach existing producers; frozen failed-only dispatch preserved | PASS |
| REQ-015: attributable selected results and features | Distinct produced-payload IDs, selected queue pointer, status and dispatch agree; rollback/reclaim proof inspected | PASS |
| REQ-016 / AC-008: matched examples/standalone apps | Recorded immutable examples and original deterministic real-server evidence; no fresh live claim | Supported within stated limits |
| REQ-017 / AC-009: public/SDK/CLI/UI/generated/docs cutover | Shared surfaces and capacity prose agree; per-file promise must be implemented | FAIL only for FIND-14 |
| INV-001–005/008 and non-goals | Correct owners, tenant/audit boundaries, retained scenarios; no new runtime/receipt/store or migration demanded | PASS |
| Mandatory changed-item rustdoc | Prior enumerated docs repaired; changed Google getter lacks required panic invariant | **FAIL — FIND-7** |
| AC-010 verification integrity | Supplied focused checks credible for exercised paths; no weakened gate retained; old aggregate predates remediation | Bounded evidence; no fresh candidate aggregate claimed |

## Validated ledger and prior closure

- **FIND-TASK-005-7 — VIOLATION, residual.** `crates/skald/skald-prompt/src/python/google.rs:577–588`, changed `parameters` getter: required item-level Panics contract omits the retained request/declaration-index invariant. No supported-input panic is alleged. Add the short accurate contract only.
- **FIND-TASK-005-14 — MISSING / INCORRECT, new.** `crates/wyrd/wyrd-server/src/verification/hosted/evidence.rs:333–379`: resolved original File/Artifact size goes directly to combined accounting without testing `file_max_bytes`. A conforming input above the individual ceiling and below the total can accompany passing no-read assertions. Enforce the existing limit once at this admission owner using the existing error; preserve read guards and all adjacent behavior.

Prior **1, 2, 3, 4, 5, 6, 8, 9, 10, 11, 12, 13 are CLOSED as diagnosed**. FIND-7's enumerated R2 inventory is repaired but its changed-item documentation cause remains. FIND-10 specifically concerned selected-slot conformity and FIND-11 combined accounting; neither is reopened for the distinct individual deployment predicate. Full source/closure table and every proposal disposition are in `findings-validation.md`.

## Proof limits and next action

Fresh review checks: pinned HEAD, clean detached source and cumulative diff whitespace passed. No tests/builds/generators/live calls were rerun. Committed R2 evidence records exact RED/GREEN, focused Vala/Skald/server checks, route/hosted 18/18 and hygiene checks. Original gate `03712af5e` predates remediation. No dedicated new MCP input test or combined queued Agent-refusal test is claimed; shared owner and separate seam proof were inspected. Uncommitted external builds are not immutable candidate proof.

Implement [TASK-005-R3-per-file-admission-and-rustdoc.md](TASK-005-R3-per-file-admission-and-rustdoc.md) through `$wyrd-implement`, then independently re-review the full cumulative candidate. Review made no source, task-status, commit, push, merge or deployment changes. Approved in-memory outbox loss, delayed result visibility and possible repeated execution remain unchanged operational limits.
