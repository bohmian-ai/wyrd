# Audit outbox task-review verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-aad682fbca5074900`
- Base: `ce5c09ef3b559c35e65d6a3e73beebe0a69ae5bd`
- Candidate: `f451d52be01acba66bae003b8509c95086fc8558`
- Reviewed range: `base..candidate` only
- Approved specification: `changes/active/audit-outbox/spec.md`, revision 1
- Original tasks: `01-publication-progress.md`, `02-one-outbox.md`, and
  `03-remove-audit-unavailable.md`
- Supplemental approved authority: high-throughput acknowledgement means
  receipt rather than guaranteed durability; derived audit work is eventually
  consistent, retries dependency slowness through a batched server-owned
  outbox, flushes on graceful shutdown, and may lose counted unflushed work on
  hard process termination.

The repository has no `.codegraph/` directory. The candidate remained at the
same commit throughout discovery, follow-up, validation, and verdict writing.
Only review artifacts under this directory were written.

## Reconciled acceptance matrix

| Obligation | Reconciled implementation and proof | Result |
|---|---|---|
| REQ-001 / AC-001 / INV-004: one process outbox and one production append path | Boot constructs and shares one `AuditOutbox`; the canonical append is crate-private and its only production caller is the outbox writer. | PASS |
| REQ-002: batched per-tenant commits with bounded cross-tenant concurrency | The writer groups by tenant, commits at most four tenants concurrently, and permits one in-flight batch per tenant. The proposed requirement for per-tenant queue reservation was rejected as outside revision 1. | PASS |
| REQ-003: audit never delays or refuses a request | Request paths enqueue without awaiting commits, but accepted batches are terminally discarded after transient Postgres failures. The supplemental approved retry principle conflicts directly with revision 1's required permanent-loss behavior. Live docs also still advertise audit-caused failures. | FAIL — `FIND-AUDIT-OUTBOX-1`, `FIND-AUDIT-OUTBOX-3` |
| REQ-004 / AC-006: audit-unavailable contracts and doctrine removed | Error variants and stable codes were removed, with historical decode values and the reserved proto identifier correctly retained. Live authority, OpenAPI descriptions, schema prose, and Rust/client docs still describe the removed transactional, fail-closed, or Oracle-WAL model. | FAIL — `FIND-AUDIT-OUTBOX-3` |
| REQ-005 / INV-001: decision before effect; stage the known verdict before proceeding or refusing | Most surfaces comply. Six tenant-admin allowance paths perform fallible work, including outbound issuer discovery, before staging the already-known verdict. | FAIL — `FIND-AUDIT-OUTBOX-2` |
| REQ-006 / AC-004: publication does not contend with append | Publication state is on the tenant-scoped `vala.audit_publication` row; freeze does not lock the chain head; settlement advances the watermark and garbage-collects atomically. Focused SQL and server journeys cover contention, replay, and stale settlement. | PASS |
| REQ-007 / AC-007: graceful shutdown drains queued work to the deadline and reports remainder | Healthy queued work drains and remainder is reported. A transiently failed accepted batch is removed from ownership before shutdown can retry it; the required retry/deadline semantics need specification revision. | FAIL — `FIND-AUDIT-OUTBOX-1` |
| INV-002 / AC-003: committed tenant chains remain gap-free and ordered across replicas | The canonical append locks one tenant chain head and commits a batch atomically; the two-outbox Postgres test proves sequence and hash continuity. | PASS |
| INV-003: tenant isolation | Each batch opens a tenant-bound connection and the new publication table has forced RLS. The canonical append nevertheless violates the repository rule against redundant manual tenant filters on a `TenantConn` path. | FAIL (repository rule) — `FIND-AUDIT-OUTBOX-6` |
| AC-002: named surface families succeed and report injected audit persistence failure | Gate, run start, Cards, auth, and admin have recorded failure-path coverage. The cited Oracle test is healthy-path only and does not inject failure or assert the `bifrost` counter. | FAIL — `FIND-AUDIT-OUTBOX-4` |
| AC-005: canonical capacity and two-replica scale-out pass | No full passing canonical capacity artifact exists for the cumulative candidate. Correctness lanes cannot substitute for the required throughput verdict. | FAIL — `FIND-AUDIT-OUTBOX-5` |
| Repository structure, documentation, and verification rules | The changed signatures include forbidden fully qualified types; permanent test rustdoc embeds task IDs; staging-only traits retain misleading `append_*` names; and the broad/TypeScript verification required for this range is absent. | FAIL — `FIND-AUDIT-OUTBOX-7` through `FIND-AUDIT-OUTBOX-10` |
| Non-goals | No second audit writer or publisher was added; hash-chain content and retained-history ownership remain intact; accepted eventual-visibility and hard-kill loss windows were not treated as defects. | PASS |

## Independent review results

| Review | Result | Proposed findings |
|---|---|---|
| Behavior review | FAIL | `BEH-001`–`BEH-004` |
| Invariant review | FAIL | `INV-REV-001`–`INV-REV-004` |
| Repository standards review | FAIL | `STD-001`–`STD-005` |
| Maintainer review | FAIL | `MAINT-01`–`MAINT-03` |
| System-resilience review | FAIL | `SYS-001`–`SYS-003` |
| Security and tenancy domain review | FAIL | `SEC-TEN-001`, `SEC-TEN-002` |
| Concurrency and lifecycle domain review | FAIL | `CONC-001` |
| Durability and persistent-data domain review | FAIL | `DOMAIN-DUR-001` |
| Focused follow-up | RESOLVED | Authority conflict and disputed paths resolved in `followup-review.md` |
| Structured Ponytail validation | COMPLETE | Ten retained findings; noisy-tenant admission proposal rejected |

## Follow-up decision

A follow-up was required because reviewers found a material conflict between
revision 1 and the supplemental approved retry principle, and because the
admin-ordering, noisy-tenant, Oracle-proof, capacity-proof, and documentation
claims needed caller-level resolution.

The follow-up established that transient dependency retry is controlling
behavior but contradicts revision 1's REQ-003, expensive-to-reverse durability
decision, AC-002 proof, and task evidence. It also confirmed the six admin
pre-stage exits, missing Oracle failure proof, missing full capacity result,
and stale live contracts. The validator rejected the noisy-tenant proposal:
revision 1 bounds cross-tenant commit concurrency but explicitly permits loss
when the single process queue is full, and it does not authorize per-tenant
admission quotas.

## Validated finding ledger

The decision-complete evidence and corrections are preserved in
`findings-validation.md`.

| ID | Status | Classification | Violated obligation and required outcome |
|---|---|---|---|
| `FIND-AUDIT-OUTBOX-1` | CONFIRMED | VIOLATION | Accepted batches are discarded after transient dependency failure. Revision 1 must first define retryable versus terminal failures, bounded retry/backoff, ordering, metrics, and shutdown-deadline behavior; implementation then retains failed batches in the existing writer. |
| `FIND-AUDIT-OUTBOX-2` | REVISED | INCORRECT | Six admin allowances can escape before staging. Move each existing stage call to the earliest point after the route-specific event is complete and before fallible work. |
| `FIND-AUDIT-OUTBOX-3` | REVISED | INCORRECT | Live authority, OpenAPI, schema prose, and Rust/client docs retain the removed transactional/WAL/fail-closed model. Delete obsolete promises, correct owning source prose, and regenerate while preserving historical decode and proto reservations. |
| `FIND-AUDIT-OUTBOX-4` | CONFIRMED | MISSING | AC-002 lacks an Oracle injected commit-failure proof using the existing server harness and failure metrics. |
| `FIND-AUDIT-OUTBOX-5` | REVISED | MISSING | AC-005 lacks a full passing canonical capacity and two-replica scale-out artifact. |
| `FIND-AUDIT-OUTBOX-6` | CONFIRMED | VIOLATION | The canonical `TenantConn` append repeats tenant filters and documents an impossible bypass-RLS caller. Rely on RLS for selection/update and retain the tenant value only where insertion requires it. |
| `FIND-AUDIT-OUTBOX-7` | CONFIRMED | VIOLATION | Candidate-added signatures use fully qualified types. Import them at module scope and use bare names. |
| `FIND-AUDIT-OUTBOX-8` | CONFIRMED | MISSING | The broad change lacks the required aggregate and TypeScript proof. Record a green canonical aggregate after corrections. |
| `FIND-AUDIT-OUTBOX-9` | CONFIRMED | VIOLATION | Permanent test rustdoc embeds disposable requirement IDs. Remove only the IDs and preserve behavior-focused prose. |
| `FIND-AUDIT-OUTBOX-10` | CONFIRMED | INCORRECT | Gate, Oracle, and peer staging collaborators use misleading `append_*` names. Rename them and their callers/test doubles to `stage_*`; keep the real SQL append name. |

## Verification limits

- The review did not rerun the large recorded test suite. It inspected the
  command evidence in the task packets, the exact candidate source and diff,
  callers, sibling consumers, tests, `mise.toml`, and generated artifacts.
- `git diff --check` was clean, and static source searches confirmed the sole
  production staging append as well as the stale live-contract locations.
- Recorded evidence includes format, lints, Rust/SQL/server/Bifrost lanes,
  Python checks, code generation, proto drift, docs, and OpenAPI integration.
  It does not include the required full capacity result, aggregate gate, or
  complete TypeScript proof.
- A green code-generation check proves generated output matches its source; it
  does not make false source rustdoc semantically correct.

## Prior-finding closure

This is the first review attempt for revision 1. There are no prior stable
`FIND-*` findings or remediation tasks to close. All discovery IDs were
deduplicated into the ledger above.

## Verdict

**SPEC_REVISION_REQUIRED**

`FIND-AUDIT-OUTBOX-1` cannot be remediated under approved revision 1 without
changing its explicit durability and concurrency behavior and inverting its
acceptance proof. Revise and approve the specification first. No remediation
task is written for this verdict. The remaining nine findings are bounded
corrections or missing proof, but they do not make implementation under the
contradictory current specification permissible.
