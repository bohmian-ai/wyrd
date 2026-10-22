# TASK-001 — FIX_REQUIRED

## Immutable subject

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/guided-setup`.
Base: `ebd75dc224854413d17cb06b70da8181b78cde6b`.
Candidate: `32fbc264adccaf1160894769d5501fbe441259a5`.
Reviewed the entire cumulative base-to-candidate range, beginning with `4188c05f3`, against approved [spec revision 5](../../spec.md) and [original TASK-001](../../tasks/TASK-001-guided-setup-and-asgi.md). Initial source tree was clean; source and candidate remained unchanged during review. Only review artifacts were written. No previous task-review findings exist.

## Independent results and reconciliation

| Role | Report | Discovery result |
| --- | --- | --- |
| behavior-rev | [Task behavior](task-review-behavior.md) | FAIL |
| invariant-rev | [Task invariants](task-review-invariants.md) | FAIL |
| repo-rev | [Repository standards](standards-review.md) | FAIL |
| maintainer-rev | [Maintainer](maintainer-review.md) | FAIL |
| system-rev | [System resilience](system-review.md) | FAIL |
| domain-rev: concurrency/resource lifecycle | [Concurrency](domain-review-concurrency.md) | FAIL |
| domain-rev: protocol/schema/prompt boundary | [Protocol](domain-review-protocol.md) | FAIL |
| followup-rev | [Focused follow-up](followup-review.md) | RESOLVED |
| ponytail-rev | [Independent source validation](findings-validation.md) | Five confirmed/revised findings |

All required reports are complete. [Claim comparison](claim-comparison.md) grouped overlapping failures at their producer. A focused follow-up was required because reports disagreed on diagnostic ownership and early lifespan exceptions. It narrowed scrubbing to Wyrd-created diagnostics and identified the ASGI unsupported-lifespan compatibility boundary. Fresh validation then independently checked every proposal, all sibling host branches and correction targets. No additional discovery pass was needed.

INVREV-001 is rejected: an early arbitrary exception cannot distinguish broken initialization from the standard unsupported-lifespan behavior. Universal failure conversion would change compatibility without establishing a bounded implementation defect. Application-owned explicit failure messages remain unchanged; only diagnostics constructed by Wyrd are corrected. Cancellation correction is limited to a cleanup attempt for an established lifecycle, preserving the existing timeout/hard-kill limits. No retained finding needs a new material decision or specification revision.

## Reconciled acceptance matrix

| Obligation | Implementation and verification evidence | Result |
| --- | --- | --- |
| REQ-001, INV-008, AC-001: rename, preserved bootstrap, no agent launch | Existing bootstrap moved; parser/TS dispatcher updated; C1/C2 and inspected regression bodies | PASS |
| REQ-002: supported host launch commands | Existing literal quoting and print-only handoff work; five supported host branches return no command | FAIL: FIND-TASK-001-4 |
| REQ-003/004: offline prompt and JSON readiness | Exclusive flag and early return; one readiness object; C1/C2 assertions | PASS |
| REQ-005/006, INV-007: authenticated prompt, one release-labelled source | Shared Markdown/formatter, authenticated MCP/proxy, docs embed; C2/C3 parity/refusals | PASS |
| REQ-007/008: agent setup and safety | Prompt steps 1–7, exact release/language/file safety, verification/query; C4 API proof and static prompt inspection | PASS for prompt |
| REQ-021: public pure-ASGI wrapping | Exports/stub, callable adapter and tested framework example | PASS |
| REQ-022, AC-014: startup and safe diagnostics | Lifecycle/provider reuse works; unexpected exception bodies enter generated diagnostics | FAIL: FIND-TASK-001-2 |
| REQ-023, AC-014: final drain and failure preservation | Normal/explicit-message paths work; established lifecycle raw exception bypasses state shutdown | FAIL: FIND-TASK-001-1 |
| REQ-024/025, AC-012: request Runs and instrumentation ownership | Existing Run context, execution-local correlation, C6/C7 and 68 observation tests | PASS |
| REQ-026, INV-009, AC-013: worker-local portable resources | Startup in worker lifespan; C8 actual two workers, enabled lifespan/limits documented | PASS with platform/proof limits below |
| REQ-027, AC-003/015: equivalent published Manual path | Manual links Quickstart; durable readback/Run identification occurs only in test helper | FAIL: FIND-TASK-001-3 |
| REQ-028/030/031: audience paths and integration guide | Exactly three ordered paths, Integration shape, tested embedded FastAPI example | PASS for TASK-001 portion |
| REQ-032, AC-016: public rename | Published source/generated projections/help use init | PASS for task portion |
| INV-005: deployment loading stays separate | Adapter accepts loaded state; FastAPI uses from_path, no registration | PASS |
| Changed Rust item rustdoc | Schema method, tests module, filesystem helper missing required item/panic docs | FAIL: FIND-TASK-001-5 |
| Non-goals and workflow reuse | Existing bootstrap, transport, state/run/provider owners retained; no extra engine/auth/framework discovery | PASS |
| TASK-002 and combined obligations | Deployment identity, platform release content and cross-task checks explicitly deferred by original task | Outside this task gate |

The validator's [full reconciled matrix and source evidence](findings-validation.md#reconciled-acceptance-matrix) supply the detailed acceptance mapping. Green recorded checks do not close the demonstrated missing paths.

## Validated finding ledger

| Stable ID | Validation | Classification | Consequence and correction boundary |
| --- | --- | --- | --- |
| FIND-TASK-001-1 | REVISED; BREV-001, INVREV-002, SYSTEM-001, CONC-001 | INCORRECT | `_middleware.py:143–147` bypasses final drain after established application lifespan error. Reuse existing off-loop state shutdown, preserve primary failure and drain only once. |
| FIND-TASK-001-2 | REVISED; BREV-002, INVREV-003 | VIOLATION | `_middleware.py:25–29` publishes arbitrary exception bodies in Wyrd-generated output. Use safe diagnostic projection, retain Wyrd codes and established missing-extra remedy. |
| FIND-TASK-001-3 | CONFIRMED; BREV-003 | MISSING | Setup Manual/Quickstart omits published durable confirmation and Run identification. Publish existing read/query completion and exercise it in C4. |
| FIND-TASK-001-4 | REVISED; PROTOCOL-001 | MISSING | `mcp/hosts.rs:93–101` omits prompt commands for Copilot, Pi, VS Code, Cursor and Hermes. Extend existing quoted match and focused handoff proof. |
| FIND-TASK-001-5 | CONFIRMED; REPO-001, MAINT-001, INVREV-004 | VIOLATION | Three changed Rust items lack required docs. Batch accurate item/panic rustdoc with behavioral corrections. |

The [validation ledger](findings-validation.md#final-deduplicated-ledger) records exact locations, producer/caller/sibling evidence, consequences, minimal correction decisions and closure proof for every retained finding. Rejected proposals are not remediation requirements. Prior-finding closure: N/A, first review.

## Verification limits and disposition

Independent review checks include cumulative whitespace, Python lint/stub, docs command and read-only generated-text equality checks; 68 existing observation tests passed. Read-only public middleware/Uvicorn probes reproduced skipped drain, and public middleware probes reproduced unsafe generated diagnostics. These probes establish adapter behavior, not real-server persistence. Full C1–C8/Postgres/worker journeys were inspected as recorded implementation evidence, not rerun by this review. Runtime evidence remains Linux x86_64 only; macOS arm64/Linux arm64 and installed host-version compatibility were not exercised. C8 disables telemetry; C7 supplies single-process managed-provider proof.

Full docs, installed bootstrap regression, all-SDK and cross-task ASGI identity checks remain at integrated change review as the original task prescribes. A fresh read-only comprehension reader received only the spec and remediation packet and found no blocking ambiguity, missing explanation or contradiction; it performed no implementation review. No merge, push, commit, implementation or deployment occurred.

Verdict: **FIX_REQUIRED**. Route [TASK-001-R1-close-guided-setup-gaps.md](TASK-001-R1-close-guided-setup-gaps.md) directly to `$wyrd-implement`, then review the complete cumulative candidate against the original task again.
