# TASK-001-R1 cumulative verdict — FIX_REQUIRED

The five original findings are **closed**. The approved guided-mode addition
passes task-stage behavioral acceptance. One new missing-field-rustdoc finding
remains under the repository's explicit documentation merge gate; no runtime
defect is alleged for it.

## Immutable subject and authority

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/guided-setup`.
Cumulative base: `ebd75dc224854413d17cb06b70da8181b78cde6b`.
Prior implementation: `32fbc264adccaf1160894769d5501fbe441259a5`.
Remediation start: `00b6148919b9ccd5f958f3b621ab6a89c6372001`.
Candidate: `796377a1d02f5a0a80fd905cc18fbbbb2d49bb4a`.

Reviewed the complete original base-to-candidate range. The latest fix range
located changed owners and prior closure but did not reduce acceptance scope.
Authority is [approved spec revision 6](../../spec.md), the
[original task](../../tasks/TASK-001-guided-setup-and-asgi.md),
[prior verdict](../TASK-001-2026-10-21/verdict.md),
[prior final findings](../TASK-001-2026-10-21/findings-validation.md), and
[R1 task/evidence](../TASK-001-2026-10-21/TASK-001-R1-close-guided-setup-gaps.md),
with the repository authorities listed in [subject.md](subject.md).
Guided mode is a separately approved human-directed addition, not a retroactive
review finding. TASK-002 and combined change-review checks retain their scope.
Initial tree was clean; HEAD and all tracked source stayed unchanged.

## Independent review results

| Role | Report | Result |
| --- | --- | --- |
| behavior-rev | [Behavior](task-review-behavior.md) | PASS |
| invariant-rev | [Invariants](task-review-invariants.md) | PASS |
| repo-rev | [Standards](standards-review.md) | PASS; exact field exception reconciled below |
| maintainer-rev | [Maintainer](maintainer-review.md) | FAIL: MAINT-R1-001 |
| system-rev | [System resilience](system-review.md) | PASS |
| domain-rev ASGI lifecycle/concurrency | [ASGI](domain-review-asgi.md) | PASS |
| domain-rev process/terminal | [Process](domain-review-process.md) | PASS |
| domain-rev protocol/manual readback | [Protocol](domain-review-protocol.md) | PASS |
| followup-rev | [Focused follow-up](followup-review.md) | RESOLVED |
| ponytail-rev | [Independent validation](findings-validation.md) | One confirmed new finding; prior five closed |

All required reports are complete. [Claim comparison](claim-comparison.md)
identified a material disagreement between broad standards PASS and the
maintainer's precise tuple-field omission. A fresh focused reviewer traced the
explicit fields rule and actual saved-terminal owner; a separate fresh validator
then independently adjudicated that claim and the entire cumulative acceptance.
No additional exploratory pass was needed.

The type and methods have meaningful docs, but the new tuple field does not.
The Rust guide explicitly includes fields regardless of visibility; the skill
retains absent rustdoc as blocking. This is the explicit documentation exception,
not a preference about representation or prose. The correction requires no
runtime change, new test, public contract or specification revision.

## Reconciled acceptance and prior closure

| Obligation | Reconciled source/proof | Result |
| --- | --- | --- |
| REQ-001–004, AC-001/002: rename, passive handoff, offline/JSON/persistence | Existing bootstrap retained; parser/dispatcher and all-host handoff updated; C1/C2 source and recorded proof | PASS |
| REQ-005–008, INV-007: shared release prompt, authenticated MCP/proxy, safe setup instructions | Shared Markdown/formatter and static authenticated handler; C2/C3 parity and language/file/version rules | PASS |
| REQ-021–023, AC-014: ASGI startup/diagnostics/final drain | Existing native lifecycle reused, guarded abnormal-exit drain and safe generated projection; C5/C7 plus actual task-cancel probes | PASS |
| REQ-024–026, AC-012/013, INV-009: Run isolation, OTel, workers/portability | Existing Run/context/provider owners; C6/C7/C8 and 68 observation tests; Linux-only runtime limit retained | PASS within limits |
| REQ-027/028/030/031, AC-003/015: Manual and integration documentation | Published get/query/SQL/Run readback, three language runners, three audience paths and tested FastAPI source | PASS for task portion |
| REQ-032, AC-016 task rename portion | Public help/docs/generated projections/wrappers renamed | PASS |
| REQ-037/038, AC-020, INV-008: explicit guided mode | One selected host, readiness/connection before literal argv, existing child/terminal supervision; controlled C2 launch/exit/failure/interrupt proof | PASS |
| INV-005 and non-goals | Offline deployed loading; no second bootstrap/writer/provider/transport or auth/TASK-002 expansion | PASS |
| Changed Rust item/field rustdoc | Prior three sites fixed; new SavedTerminal tuple field at terminal.rs107 undocumented | FAIL: FIND-TASK-001-6 |

The validator's [full acceptance matrix](findings-validation.md) provides the
complete requirement/evidence reconciliation and all proposal dispositions.

| Prior stable finding | Closure evidence | Status |
| --- | --- | --- |
| FIND-TASK-001-1 | Established abnormal lifespan reaches the single guarded existing drain; original error/cancellation preserved | CLOSED |
| FIND-TASK-001-2 | Shared generated diagnostic producer withholds unexpected bodies and preserves canonical code/known install remedy | CLOSED |
| FIND-TASK-001-3 | Manual links published get/query/SQL completion and matching Run interpretation; C4 executes it across languages | CLOSED |
| FIND-TASK-001-4 | One seven-host invocation owner projects both direct argv and literal displayed command; ordinary init remains passive | CLOSED |
| FIND-TASK-001-5 | Changed schema method, MCP tests module and walk helper now have required item/panic docs | CLOSED |

Prior rejected pre-receive lifespan refusal and application-owned message
rewriting remain excluded. No new evidence justifies changing those boundaries.

## Open validated ledger

**FIND-TASK-001-6 — CONFIRMED / VIOLATION**, discovery MAINT-R1-001.
Location: `crates/wyrd/wyrd-cli/src/terminal.rs:107`, the new `Termios` tuple
field in `SavedTerminal`.

The snapshot is captured before the owned agent starts and restored through
the existing normal-exit/settlement owner. Type docs convey that role but do
not document the separately governed field. The candidate therefore misses
the explicit changed-field documentation gate. Add field-level rustdoc to the
existing field; keep representation, methods and behavior unchanged. Static
source inspection and formatting/whitespace checks are sufficient closure.
The [independent ledger](findings-validation.md) records full producer/caller
evidence and why deletion/refactoring would be an unjustified correction.

## Verification limits and disposition

Fresh discovery checks include actual Task.cancel/off-loop probes, 68 observation
tests, a PTY reproduction of the fake-agent ordering race, generated-text/source
equality, docs commands/links, authored-stub and whitespace checks. The offline
built-CLI comparison was not a fresh build. These are distinguished from supplied
C1/C2/C4/C5/C7 and original journey/build evidence; no Cargo/Postgres/worker/full
aggregate rerun was performed by the validator.

The original C2 diagnostician conclusion is summarized in the packet without a
separate original trace/report. Independent source tracing and a system-review
PTY probe support the cause and test-only correction; assertions/timeouts remain
intact. Raised cancellation in submitted C5 is supplemented by fresh actual
task-cancel probes, without inferring repeated-cancellation/startup-thread or
hard-kill delivery guarantees. Direct-child ownership does not expand to
pre-existing/detached sessions or arbitrary agent descendants.

Runtime evidence remains Linux x86_64 only. macOS arm64/Linux arm64, paid-host
runtime/version compatibility and combined identity/docs/SDK/bootstrap gates
remain unexercised here. Combined gates run at change review as approved.
No implementation, commit, merge, push or deployment occurred.

Verdict: **FIX_REQUIRED**, solely **FIND-TASK-001-6**. Route
[TASK-001-R2-document-saved-terminal-field.md](TASK-001-R2-document-saved-terminal-field.md)
directly to `$wyrd-implement`. The documentation-only correction calls for no
runtime reruns. A fresh comprehension reader found no ambiguity in that packet.
