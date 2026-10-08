# Audit outbox r2 task-review verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-aad682fbca5074900`
- Base: `6714ae35d732814240fdcc42fc226c079b14d3f0`
- Candidate: `5a5542cbb965af99e92a3983a2ddf586412cea73`
- Remediation range: `6714ae35d..5a5542cbb`
- Approved specification: `changes/active/audit-outbox/spec.md`, revision 2
- Original tasks: `changes/active/audit-outbox/tasks/*.md`
- Prior review: `changes/active/audit-outbox/review/r1/`
- Remediation task: `review/r1/TASK-AUDIT-OUTBOX-R1-remediation.md`

The repository has no `.codegraph/` directory. `HEAD` remained the candidate
through discovery, follow-up, validation, and verdict writing. Only review
artifacts under `review/r2/` were created.

## Reconciled acceptance matrix

| Obligation | Reconciled implementation and proof | Result |
|---|---|---|
| REQ-003: requests do not await/refuse on audit; returned write errors retry at the tenant front while other tenants progress | The generic writer restores ordinary `Err` batches ahead of later same-tenant work, backs off from 50 ms to 5 s, and preserves cross-tenant dispatch. A spawned sink panic instead loses the owned batch while the process and writer continue. | **FAIL — `FIND-AUDIT-OUTBOX-12`** |
| REQ-003a: loss only on abrupt process stop or expired graceful-shutdown deadline | Deadline loss is counted, but the `JoinError` path counts and releases accepted work without either approved loss boundary. | **FAIL — `FIND-AUDIT-OUTBOX-12`** |
| REQ-007: shutdown stops intake and retries accepted work to the deadline | The deployed server drains current audit producers before final outbox shutdown. The generic owner itself does not close admission at invocation, so a concurrent `stage` can still enter the drain set. | **FAIL — `FIND-AUDIT-OUTBOX-13`** |
| REQ-008 / AC-008: one SQL-free generic owner with queueing, grouping, bounded tenant concurrency, retry, metrics, idle, and coherent shutdown | `wyrd-runtime::outbox` supplies the required generic owner and its five focused tests prove ordinary retry/order, tenant independence, no count limit, recovery, and deadline loss. Panic handling, admission fencing, and terminal pending/gauge settlement remain incorrect. | **FAIL — `FIND-AUDIT-OUTBOX-12`, `FIND-AUDIT-OUTBOX-13`** |
| REQ-009 / AC-009: unknown-outcome retry never duplicates audit | Event IDs are unique only while their rows remain in transient `vala.audit_staging`. A publisher can retain and retire an ambiguously committed row during retry backoff; the retry then allocates a new sequence for the same decision. The current test repeats before retirement and cannot exercise this path. | **FAIL — `FIND-AUDIT-OUTBOX-11`** |
| INV-002: committed sequence/hash chains stay gap-free and later tenant work does not overtake a retry | Chain-head locking and front-of-queue retry keep sequences gap-free and ordered. The retirement defect creates a second valid chain entry rather than a gap, so this invariant passes narrowly while exact-once audit fails. | PASS |
| AC-002: named surface families succeed under audit failure and recover exactly once | Recorded Gate, run-start, Oracle, Card, auth, admin, gateway, CLI, platform, OpenAPI, and direct-execution proofs cover definite failure and recovery. Exact-once recovery is not true for the reachable unknown-commit/retirement path. | **FAIL — `FIND-AUDIT-OUTBOX-11`** |
| AC-005 / r1 FIND-5: canonical capacity and two-replica scale-out | Explicitly deferred by the user and remediation task to `mise run bench:capacity` on the integrated branch. No substitute was required or accepted here. | DEFERRED TO INTEGRATION |
| AC-007: queued work drains before deadline and deadline remainder is counted | Generic and Postgres evidence covers recovery before deadline and exact remainder counting. The deployed audit shutdown order drains producers first. | PASS; generic owner lifecycle still fails REQ-007/REQ-008 under `FIND-AUDIT-OUTBOX-13` |
| R1 FIND-1 | Ordinary acquire/append/commit failures are no longer dropped and revision 2 supplied retry authority. The new retirement race is a distinct REQ-009 identity defect. | CLOSED; new `FIND-AUDIT-OUTBOX-11` |
| R1 FIND-2, FIND-4, FIND-6, FIND-9, FIND-10 | Admin allowance ordering, Oracle failure/recovery proof, RLS-only tenant filtering, task-ID cleanup, and `stage_*` naming are implemented and credibly evidenced. | CLOSED |
| R1 FIND-3 | Most live authority and contract prose was corrected, but the changed security posture still names an “Oracle audit commit failure” although the generic outbox owns commits. | **OPEN — `FIND-AUDIT-OUTBOX-3`** |
| R1 FIND-7 | Prior cited declarations were corrected, but new remediation declarations again use fully qualified `MutexGuard`/`Uuid` types instead of top-level imports and bare names. | **OPEN — `FIND-AUDIT-OUTBOX-7`** |
| R1 FIND-8 | The remediation record contains green TypeScript install, public/testing builds, N-API check, typecheck, unit, and 29/29 integration evidence. | CLOSED |
| `scripts/check_unwrap_audit.py` regression boundary | The current check passes, but it now skips every file named `tests.rs` without proving `#[cfg(test)]` ownership, allowing a production module with that legal basename to evade the gate. | **FAIL — `FIND-AUDIT-OUTBOX-14`** |
| `wyrd-runtime` dependency ownership and client-tier boundary | The shared SQL-free generic owner is required, the remediation explicitly selected `wyrd-runtime`, only installed lightweight runtime capabilities were added, no forbidden server/SQL/cloud/engine edge entered client tier, and `check:client-tier` passes. The proposed relocation finding was rejected. | PASS |

## Independent review results

| Review | Result | Material proposals |
|---|---|---|
| Behavior review | FAIL | Dedup retirement, sink panic loss, unwrap-check bypass |
| Invariant review | FAIL | Dedup retirement, sink panic loss, unwrap-check bypass |
| Repository standards review | FAIL | Six proposals; dependency relocation rejected in validation, remaining applicable claims retained or reconciled |
| Maintainer review | FAIL | Dedup retirement, terminal pending, shutdown admission, unwrap-check bypass |
| System-resilience review | FAIL | Dedup retirement; capacity proposal rejected here because integration deferral is explicit |
| Security/tenancy domain review | FAIL | Duplicate retained authorization decision after retirement |
| Concurrency/lifecycle domain review | FAIL | Dedup retirement, sink panic loss, shutdown admission |
| Durability/persistent-data domain review | FAIL | Dedup identity has no lifetime beyond staging; correction needs a material decision |
| Focused follow-up | RESOLVED | Narrowed shutdown, terminal pending, and dependency-ownership claims |
| Structured Ponytail validation | COMPLETE | Six retained findings; `SPEC_REVISION_REQUIRED` |

## Follow-up decision

A focused follow-up was required because discovery disagreed about shutdown
admission and raised unique pending-state and dependency-placement claims. It
established that the current server drains audit producers before shutdown,
but REQ-008 still assigns a synchronous admission fence and terminal pending
settlement to the generic owner. It also narrowed the dependency claim to a
new `metrics` edge; validation rejected relocation because the approved shared
runtime primitive uses installed dependencies, adds no forbidden client-tier
edge, and moving it would add an unearned package seam.

No follow-up was needed for AC-005: the user's explicit integration deferral
resolves that proposal.

## Validated finding ledger

Decision-complete evidence and corrections are preserved in
`findings-validation.md`.

| ID | Status | Classification | Required outcome |
|---|---|---|---|
| `FIND-AUDIT-OUTBOX-11` | CONFIRMED | INCORRECT | Choose, through an approved specification revision, the durable identity lifetime/owner or append-publication coordination that keeps unknown-outcome retries idempotent after staging retirement. |
| `FIND-AUDIT-OUTBOX-12` | CONFIRMED | VIOLATION | Preserve batch ownership across sink-task unwind and route it through the existing retry path rather than counting live-process loss. |
| `FIND-AUDIT-OUTBOX-13` | REVISED | INCORRECT | Make shutdown a handle-owned one-way admission transition and clear terminally abandoned work from pending/gauge while returning the exact loss count. |
| `FIND-AUDIT-OUTBOX-14` | CONFIRMED | REGRESSION / VIOLATION | Remove the basename-wide checker exemption and use a narrow proven cfg-test allowlist with regression fixtures. |
| `FIND-AUDIT-OUTBOX-7` | REVISED | VIOLATION | Import `MutexGuard` and `Uuid` at module scope and use bare names in the new declarations. |
| `FIND-AUDIT-OUTBOX-3` | REVISED | INCORRECT | Replace the remaining Oracle-specific audit-commit phrase with shared outbox write/publication ownership. |

## Verification limits

- Independently rerun: `mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(/^outbox::/)'` — 5/5 passed.
- Independently rerun: `mise run check:client-tier` — passed.
- Independently rerun with a writable temporary uv cache: `mise run check:unwrap-audit` — passed, but `FIND-AUDIT-OUTBOX-14` shows that the green result no longer proves the advertised production boundary.
- Independently rerun: `git diff --check 6714ae35d..HEAD` — passed.
- The Postgres SQL lane could not be rerun in this sandbox because access to the configured Docker socket was denied. The recorded remediation evidence reports 119/119 passing.
- A TypeScript rerun could not begin because pnpm could not open its configured store database in this sandbox. The immutable remediation record supplies the accepted green TypeScript evidence, including 29/29 integration tests.
- `mise run bench:capacity` and the aggregate gate remain integration-owned by explicit user direction.
- Green healthy/definite-failure tests do not exercise the validated unknown-commit publication race, sink panic, concurrent shutdown admission, terminal gauge state, or checker-bypass cases.

## Prior-finding closure

`FIND-AUDIT-OUTBOX-1`, `-2`, `-4`, `-6`, `-8`, `-9`, and `-10` are closed
according to their recorded dispositions. `FIND-AUDIT-OUTBOX-5` remains the
explicit integration-stage capacity obligation. `FIND-AUDIT-OUTBOX-3` and
`FIND-AUDIT-OUTBOX-7` remain narrowly open because the remediation reintroduced
their exact authority/style violations in changed locations. New findings
start at `FIND-AUDIT-OUTBOX-11`.

## Verdict

**SPEC_REVISION_REQUIRED**

`FIND-AUDIT-OUTBOX-11` proves that revision 2's chosen staging-only event-ID
fence cannot satisfy REQ-009 once the independently required publisher retires
transient staging. Correcting it requires a new persistent-data or concurrency
decision: where event identity survives, how long it survives, or how append
retry and retirement coordinate. The existing specification forbids a second
audit table/WAL/relay and does not authorize a retained-schema or publication
protocol change, so this cannot be prescribed as bounded implementation
remediation.

No r2 remediation task is written for this verdict. After revision 3 is
approved, a fresh remediation task may package `FIND-AUDIT-OUTBOX-11` together
with the five bounded retained findings.
