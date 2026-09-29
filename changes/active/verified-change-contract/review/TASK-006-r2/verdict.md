# TASK-006 R2 Verdict

## Verdict

**FIX_REQUIRED**

The cumulative candidate closes all ten findings from the first review, but two
bounded gaps remain: one repository-rule violation in the test fixture API and
one missing real-server/provider acceptance proof for negative media handling.
Neither correction changes approved behavior or requires a new architectural
decision.

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t006`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `3593bbc31273673f87159315fbf66a73562d3c99`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 36
- Original task: `changes/active/verified-change-contract/tasks/TASK-006-continuous-eval-verifier.md`
- Prior review and remediation: `changes/active/verified-change-contract/review/TASK-006-r1/`
- Current remediation: `changes/active/verified-change-contract/review/TASK-006-r2/TASK-006-R2-continuous-eval-closure.md`

The candidate remained `HEAD` and unchanged through both review waves. Review
artifacts were the only files created during this review.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-077/079, INV-004, AC-014: first committed observation alone activates best-effort enqueue with exact managed event time | Scribe propagates its existing first-commit disposition through Gate; observation activation remains post-ACK and asynchronous | Later-day sealed replay journey and Scribe evidence | PASS |
| REQ-083, INV-010, AC-016: sampling is stable across retries and restart | Per-binding observation ordinal is serialized and stored once on enqueue | SQL concurrency test and terminal/restart journey evidence | PASS |
| REQ-084: false assertions attest while executor/input/provider failures remain errors | Existing `vala-eval` fan-out propagates errors; common mapping consumes returned reports only | Six focused engine regressions rerun by task review | PASS |
| REQ-085, AC-020: canonical summary/items, terminal mapping, ACK ordering, and dispatch | Existing generic result publication and fenced settlement own the flow | Terminal-matrix, SQL, and server journey evidence | PASS |
| REQ-111/130: one existing Eval engine/judge path and complete trace evidence | `ScenarioScoring` and `SkaldJudgeInvoker` remain singular; trace projection reconstructs events/links/counts | Ordered/bounded trace journey and focused trace regression | PASS |
| REQ-083/152: deterministic bounded trace reads and PostgreSQL coordination clock | Closed time range, total SQL order, fixed sentinel ceiling; Postgres owns claims/retries/deadlines | Trace ceiling/order and runtime integration evidence | PASS |
| AC-014: input lookup uses frozen managed event day, not authored `created_at` | Record reader prunes by frozen `wyrd_event_time` | Cross-day SDK journey with mutation proof | PASS |
| REQ-131: authorized media bytes are effectively bounded and reach the provider natively | Tenant media resolution uses the existing bounded storage read before encoding/provider invocation | Bounded-storage, resolver, native provider, and emulator evidence | PASS |
| AC-027 negative media behavior is proved through the real server/provider seam | Production refusal paths exist, but the journey covers only the cross-tenant branch | Missing binding, unsupported kind/MIME, and oversized body remain lower-level-only | **FAIL — FIND-TASK-006-12** |
| REQ-086, INV-015: stable tenant System principal, narrow read authority, canonical Oracle audit | Stored tenant System identity and two table scopes use the normal query authorization path | Read-authority, principal, and tenant-isolation journeys | PASS |
| Stable secret-free public verification errors | Fixed public messages; raw causes remain protected diagnostics; media locators are redacted | Sentinel-based real-server and unit proofs | PASS |
| Repository SQL-handle rule | Production paths use approved handles, but `WyrdTestServer::superuser_pool` forwards raw `PgPool` through a new library signature | Caller/source inspection | **FAIL — FIND-TASK-006-11** |
| Explicit non-goals remain excluded | No outbox, second engine/judge, offline dataset path, provider file lifecycle, public auth surface, or new configuration | Complete cumulative diff inspection | PASS |
| Prior findings 1–10 close without reopening adjacent behavior | See prior-finding closure below | Wave 1 and Wave 2 source validation | PASS |

## Wave results

| Review | Result | Material outcome |
|---|---|---|
| Task implementation | PASS | No proposed acceptance finding; all ten prior findings closed |
| Repository standards | FAIL | `REPO-1`, raw `PgPool` forwarding API |
| Eval domain | FAIL | `EVAL-R2-001`, incomplete AC-027 real-seam negative matrix |
| Data/durability/concurrency domain | PASS | No material finding |
| Security/tenancy domain | PASS | No material finding |
| Structured Ponytail validation | COMPLETE | Retained both findings as `FIND-TASK-006-11` and `FIND-TASK-006-12` |

## Validated finding ledger

### FIND-TASK-006-11 — CONFIRMED / VIOLATION

`WyrdTestServer::superuser_pool` at
`crates/wyrd/wyrd-testing/src/server.rs:2676-2691` forwards a raw
`sqlx::PgPool` through a new library API, contrary to
`architecture/agent-rules.md`. Its three journey callers can use the already
exposed fixture owner directly. Delete the forwarding method and replace those
calls with `server.pg_fixture().superuser_pool().await?`; add no wrapper.

### FIND-TASK-006-12 — REVISED / MISSING

AC-027 and TASK-006 require the negative media contract through the real
server/provider seam. The current journey proves valid native media and the
cross-tenant branch, but not an unbound prompt media variable, an unsupported
valid kind/MIME pairing, or an effective oversized body. Extend the existing
terminal-matrix journey with exactly those three records and prove each settles
errored with no result, dispatch, or provider request. Reuse the existing
graph, object store, provider capture, and assertions; add no production code
or harness.

## Prior-finding closure

| Finding | Closure | Result |
|---|---|---|
| `FIND-TASK-006-1` | First-commit disposition suppresses replay activation and preserves committed event time | CLOSED |
| `FIND-TASK-006-2` | Immutable serialized observation ordinal fixes sampling across retry/restart | CLOSED |
| `FIND-TASK-006-3` | Effective body read retains at most ceiling plus one and refuses overflow | CLOSED |
| `FIND-TASK-006-4` | Trace projection reconstructs events, links, attributes, and dropped counts | CLOSED |
| `FIND-TASK-006-5` | Trace rows have a total timestamp/span-ID order | CLOSED |
| `FIND-TASK-006-6` | Real SDK journey separates client creation day from managed event day | CLOSED |
| `FIND-TASK-006-7` | Eval reads use the persisted tenant System principal and canonical Oracle audit | CLOSED |
| `FIND-TASK-006-8` | Trace reads have a closed time range and fixed overflow sentinel | CLOSED |
| `FIND-TASK-006-9` | Public errors are fixed and secret-free; protected diagnostics retain causes | CLOSED |
| `FIND-TASK-006-10` | Fan-out first-error, cancellation, and partial-progress behavior is documented | CLOSED |

## Verification limits

- No required reviewer exceeded the 20-minute ceiling; all required reports
  completed, so no sub-reviewer gap exists.
- Task review independently reran six focused `vala-eval` regressions, the
  bounded-storage test, and `git diff --check`; all passed.
- The review waves did not rerun broad Postgres, emulator, or full Bifrost
  lanes. They inspected their exact tests and the command/results recorded in
  TASK-006-R1. Those recorded lanes do not close the two retained findings.
- No live cloud store or external provider was used; repository-managed
  emulators and the local provider are the applicable task seams.
