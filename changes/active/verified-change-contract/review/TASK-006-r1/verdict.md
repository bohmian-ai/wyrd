# TASK-006 Review Verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t006`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `55c5bff84fbfcc8f43967a8ad3aeac6057f10c3f`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 35
- Original task: `changes/active/verified-change-contract/tasks/TASK-006-continuous-eval-verifier.md`
- Review attempt: `TASK-006-r1`

## Verdict

**FIX_REQUIRED**

The review began with `HEAD` exactly at the requested candidate. During Wave 1,
the shared checkout advanced to `01146cf87d22147b87d0c9224aa2bdf67decad92`.
That descendant changes repository skill documentation; every reviewer
inspected the explicit base and candidate commit objects. The repository owner
resolved the two prior blockers with these decisions, recorded verbatim:

> (1) Checkout instability: 'is not a blocker. i approve the commit' -- commit 01146cf8 (skill docs only) is accepted; the named base f8811ac5 / candidate 55c5bff8 review stands.

> (2) FIND-TASK-006-7: 'That is an anti-pattern why are you creating a new user. If this is a server runtime/machinary that is concstantly running and by the nature of its design, doesnt have a direct principal, then it needs to use a system principal (per tenant)'. Approved decision: continuous Eval's internal Bifrost reads use the existing per-tenant PrincipalKind::System principal (stable id, credentialless, no public lifecycle), extended with a narrow server-minted read scope for continuous Eval inputs; never a fabricated User principal.

These decisions leave the immutable reviewed source unchanged and make all ten
validated findings bounded remediation. The current approved specification
and security posture still describe the System principal as write-only; their
text must be aligned with the owner's approved narrow read capability in the
remediation. TASK-006 is not accepted until the cumulative corrected candidate
passes another review.

## Acceptance matrix

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| REQ-077, REQ-079, INV-004, AC-014: post-ACK enqueue freezes the committed row's exact managed event time and replay converges | Post-ACK hook and SQL uniqueness exist; fail-open journey exists. Exact Scribe replay can activate with a new attempt-local receipt time. | **FAIL** — `FIND-TASK-006-1` |
| REQ-083, INV-010, AC-016: sampling and retry/restart behavior are deterministic | Sampling precedes execution and terminal cases are covered. `every_nth` ordinal is recomputed from mutable MVCC visibility on each attempt. | **FAIL** — `FIND-TASK-006-2` |
| REQ-084: execution/input failures remain errors rather than false assertions | Existing executor paths propagate failures; focused and journey cases cover error settlement. | PASS |
| REQ-085, AC-020: completed Eval persists the canonical summary and every Ran/Skipped item; non-result states persist none | Result projection and terminal-matrix journey cover the required rows and exclusions. | PASS |
| REQ-111, REQ-130, INV-012: continuous Eval reuses the existing Vala engine and preserves trace semantics | One existing engine path is used, but the new trace decoder fabricates empty event/link evidence. | **FAIL** — `FIND-TASK-006-4` |
| REQ-083 reproducibility: repeated trace reads preserve stable positional evidence | The trace query has no deterministic ordering. | **FAIL** — `FIND-TASK-006-5` |
| AC-014 Scenario 5: real journey proves managed-event-day lookup when client `created_at` differs | Query uses frozen managed time, but the required adversarial cross-day journey is absent. | **FAIL** — `FIND-TASK-006-6` |
| REQ-131, AC-027: media is tenant-authorized, bounded, and delivered natively without private URI text | Native provider content and static refusal cases exist. Metadata and body reads are separate, and the effective body is unbounded. | **FAIL** — `FIND-TASK-006-3` |
| Security authority: internal Oracle reads use authenticated, authorized, attributable identity | Tenant confinement holds, but the Eval reader fabricates a new self-authorized user principal for every read. The owner approved the existing per-tenant System principal with a narrow read scope. | **FAIL** — `FIND-TASK-006-7` |
| Bifrost resource rules: analytical reads have bounded time and result volume | Eval trace reads have only a lower time bound and no row ceiling before full collection. | **FAIL** — `FIND-TASK-006-8` |
| Stable, secret-free public verification errors | Raw SQL, Bifrost, registry, storage, locator, and provider text can enter persisted/public errors. | **FAIL** — `FIND-TASK-006-9` |
| Repository Rust documentation rules | Materially changed async fallible fan-out helper lacks required intent, error, and cancellation documentation. | **FAIL** — `FIND-TASK-006-10` |
| INV-015: tenant isolation across enqueue, reads, media, and result publication | TenantConn/RLS, signed scope, Oracle tenant context, path validation, and SYSTEM result-table controls were traced; no tenant escape was found. | PASS |
| Prohibited changes and non-goals | No outbox, atomic Scribe/run transaction, Bifrost queue poller, synthetic false assertion, second Eval engine, offline execution, or provider file lifecycle was added. | PASS |
| AC-033: recorded verification evidence | Task records focused Eval tests plus `test:vala`, `test:sql`, `test:wyrd`, `test:bifrost`, `test:wyrdstate:journey`, format, lints, and diff check as passing. | PASS as supplied evidence; broad lanes were not independently rerun |

## Wave results

| Review | Result | Material output |
|---|---|---|
| Task implementation review | FAIL | `TASKREV-006-001` through `TASKREV-006-003` |
| Repository standards review | FAIL | `RS-001` through `RS-006` |
| Eval domain review | FAIL | `EVAL-DOM-001` through `EVAL-DOM-005` proposed |
| Data/durability/concurrency/tenancy review | FAIL | `DATA-001` |
| Security/privacy review | FAIL | `SEC-T006-01`, `SEC-T006-02` |
| Structured Ponytail validation | Ten validated findings; original attempt marked BLOCKED | `EVAL-DOM-001` rejected as unreachable/speculative; owner's later decisions resolved both blockers without changing the candidate |

All required reviewers returned reports within the caller's 20-minute ceiling.
No sub-reviewer timeout gap applies.

## Validated finding ledger

The decision-complete ledger is preserved in `findings-validation.md`.

| Finding | Status | Classification | Required outcome |
|---|---|---|---|
| `FIND-TASK-006-1` | REVISED | INCORRECT | Reuse Scribe's existing commit/replay disposition so only the commit that inserted rows invokes activation. |
| `FIND-TASK-006-2` | CONFIRMED | INCORRECT | Assign and persist one immutable per-binding observation ordinal at enqueue. |
| `FIND-TASK-006-3` | REVISED | INCORRECT | Bound the effective storage body read to `limit + 1` and reject overflow before encoding/provider invocation. |
| `FIND-TASK-006-4` | CONFIRMED | INCORRECT | Reconstruct canonical persisted span events, links, and dropped counts in the existing trace reader. |
| `FIND-TASK-006-5` | CONFIRMED | INCORRECT | Give the existing trace query a total order by start time and span ID. |
| `FIND-TASK-006-6` | CONFIRMED | MISSING | Add the required different-day `created_at` real-server journey using the existing harness. |
| `FIND-TASK-006-7` | REVISED | VIOLATION | Reuse the stable per-tenant System principal for narrowly scoped Eval reads; align the spec and security posture; prove Oracle audit attribution and fail-closed scope/tenant checks. |
| `FIND-TASK-006-8` | REVISED | VIOLATION | Close the Eval trace time interval and impose a fixed server-owned span ceiling with overflow rejection. |
| `FIND-TASK-006-9` | REVISED | VIOLATION | Preserve raw causes only in protected diagnostics and construct public errors from stable safe text. |
| `FIND-TASK-006-10` | CONFIRMED | VIOLATION | Document the changed fan-out helper's role, error propagation, and cancellation behavior. |

## Prior-finding closure

This is the first review attempt for TASK-006. There are no prior stable
`FIND-TASK-006-*` findings to close or preserve from an earlier remediation.

## Verification limits

- The shared checkout changed during Wave 1; the owner accepted the intervening
  skill-documentation commit and affirmed the named base/candidate review.
- Reviewers statically inspected the complete named range and relevant callers.
  Broad `mise` lanes recorded in the task were not independently rerun during
  this time-bounded review.
- The security reviewer independently reran the focused media-resolution test;
  it passed but does not exercise body mutation after metadata lookup or public
  error redaction.
- No cross-tenant enqueue journey was added or rerun. Static tracing found the
  path tenant-bound through signed Card scope and `TenantConn`/RLS.
- CodeGraph was unavailable because the repository has no `.codegraph/` index;
  reviewers used Git objects and direct source/caller inspection.

## Remediation disposition

The single self-contained remediation task is
`TASK-006-R1-continuous-eval-closure.md` in this directory. It carries
`FIND-TASK-006-1` through `FIND-TASK-006-10`, including the approved
System-principal decision and required authority-text updates. A later review
reassesses the complete cumulative original base-to-corrected-candidate range.
