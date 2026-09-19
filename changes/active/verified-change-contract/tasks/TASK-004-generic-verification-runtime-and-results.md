---
id: TASK-004
kind: implementation
status: proposed
spec: SPEC-verified-change-contract
spec_revision: 32
requirements: [REQ-061, REQ-062, REQ-063, REQ-078, REQ-079, REQ-081, REQ-082, REQ-085, REQ-086, REQ-087, REQ-096, REQ-097, REQ-100, REQ-115, REQ-119, REQ-121, REQ-122, REQ-135, REQ-136, REQ-137, REQ-145, REQ-146, INV-004, INV-007, INV-010, INV-011, AC-013, AC-015, AC-020, AC-023, AC-024, AC-028, AC-030]
depends_on: [TASK-002, TASK-003]
---

## Outcome and Value

One supervised, durable VerificationRuntime admits scheduled or manual work,
claims and retries exact runs, publishes immutable result batches remotely
through Bifrost under a tenant SYSTEM writer, settles only after required ACKs,
and exposes binding/run status through one HTTP/SDK/MCP contract. Drift and Eval
plug into this closed runner without owning claims, results, dispatch, or
process-local state.

## Owners, Scope, Consumers, and Prohibited Changes

`wyrd-sql` owns run/dispatch control persistence; `wyrd-server` owns the
runtime, HTTP handlers, audit boundaries, supervision, limits, health, and
telemetry. Existing auth crates own the internal `system` principal/token.
`wyrd-client::Bifrost`, Gate, and Scribe own remote result admission/durability;
Oracle remains the read path. Shared client plus SDK/MCP surfaces project the
typed status/manual API.

Do not add a broker, direct/local Scribe write, result HTTP endpoint,
process-local run registry, new RBAC permission, cross-table recovery protocol,
or claim atomic analytical visibility. Internal mechanics do not emit auth
audit rows.

## Approach

1. Add durable run/dispatch claim, lease, retry, settlement, cursor, and status
   operations with token fencing and UUIDv7 identities.
2. Provision and verify one credentialless UUIDv7 SYSTEM principal per tenant;
   enforce Gate's closed table matrix.
3. Supervise one bounded runtime with scheduler, generic runner, and later
   Operator worker capability slots, health, telemetry, and drain behavior.
4. Publish reusable result/detail batches through the existing Bifrost facade
   and preserve sealed payload identity for unacknowledged retries.
5. Expose the three Verification operations and their shared-client/SDK/MCP
   projections with normal permissions, idempotency, tenancy, and audit.

## Ordered Implementation Scenarios

### Scenario 1 — Provisioned SYSTEM can write only result tables

**Behavior.** Tenant provisioning creates one stable credentialless UUIDv7
`system` principal. Per publication, the runner mints a short-lived token with
no root Card/roles and exactly one UID-bearing Verifier scope. Verification
rejects every malformed/public issuance path. Gate permits only that principal
on the three result tables and denies it everywhere else; every other kind,
including wildcard admin, is denied those tables.

**RED.** Add provisioning, token verification, issuance/refresh/delegation
refusal, Gate matrix, scope-forgery, and tenant-forgery tests. The current
principal model and Gate lack this path.

**GREEN.** Extend the existing principal/token/store/Gate owners and reuse the
existing `bifrost_record:write`, signed scope, and canonical Gate audit.

**REFACTOR.** Delete any special transport credential or table permission;
SYSTEM remains an intrinsic internal principal path.

### Scenario 2 — Manual requests durably enqueue and preserve requester identity

**Behavior.** Authenticated `POST /v1/verification/runs` accepts one bounded
Drift window and binding/direct target, audits `evals:run` plus exact scope,
uses normal Idempotency-Key semantics, returns `202 {run_id}`, and freezes the
caller separately from nullable binding owner/ID. Invalid windows, targets,
tenants, scope, or readiness fail before enqueue.

**RED.** Add handler/SQL and real HTTP cases for binding/direct success,
request replay/conflict, identity columns, and every refusal.

**GREEN.** Compose existing caller, audit, idempotency, Card/binding resolution,
and run insert in the owning transaction.

**REFACTOR.** Share one enqueue path with scheduler/Eval origins while keeping
origin-specific required fields exhaustive.

### Scenario 3 — Scheduler creates one exact window without catch-up

**Behavior.** One scheduler task claims a due active/ready binding briefly,
creates at most one unique run for the stored half-open window, and advances
the cursor in the same transaction. Inactive/unready/missed occurrences create
no run and are never backfilled; the next future boundary becomes the cursor.
Analysis runs after the lock is released.

**RED.** Add controllable-clock concurrency tests for duplicate ticks,
inactive/unready/missed periods, restart, cursor movement, and fixed windows.

**GREEN.** Reuse the existing skip-locked/fenced SQL patterns and approved
activity/readiness queries.

**REFACTOR.** Keep cron calculation synchronous and IO only in the owner that
claims/persists work.

### Scenario 4 — Claims, retries, and terminal states survive restart

**Behavior.** The runner acquires global/per-tenant permits before claim,
settles only with its lease token, reclaims expiry, retries engine failures with
the same run/input, and distinguishes completed verdicts from cancelled,
timed_out, and errored execution. Restart never duplicates a run or loses
visible state.

**RED.** Add lease expiry, stale settlement, retry exhaustion, cancellation,
permit fairness, and restart cases against Postgres.

**GREEN.** Implement bounded claims on the durable store and one typed closed
dispatch to the two real engines.

**REFACTOR.** Centralize lifecycle transitions on the runtime owner; engines
return typed outcomes and do not mutate run rows.

### Scenario 5 — Result publication requires every non-empty ACK

**Behavior.** Every non-empty detail batch precedes the summary; all rows share
result event time and exact run/Verifier/subject/owner/binding identities. Zero
details sends no empty batch. Only all required Scribe ACKs permit completed
settlement and failed-binding dispatch insertion. Identical unacknowledged
sealed payload retry deduplicates; fresh writes do not. Partial rows may remain
visible but never authorize dispatch.

**RED.** Add multi-server gRPC/Scribe tests for success, zero-detail, duplicate
sealed replay, fresh batch, detail-success/summary-failure, crash, and no-local-
Scribe worker.

**GREEN.** Use `wyrd_client::Bifrost` and preserve sealed batch/table/bytes
inside the bounded attempt until ACK or terminal failure.

**REFACTOR.** Keep analytical payload construction separate from transport
mechanics and add no repair coordinator.

### Scenario 6 — Status is one typed control-plane projection

**Behavior.** Binding GET reports exact identities, active/readiness reasons,
schedule and last activation/run. Run GET reports execution/requester/result
pointer/error and independent dispatch statuses without copying verdict/detail
data from Bifrost. Rust/Python/TypeScript and MCP call the same shared client;
Cards and Bifrost retain baseline/result reads.

**RED.** Add HTTP and first-class client/MCP journeys for polling, permissions,
cross-tenant denial, and direct versus binding delivery state.

**GREEN.** Add typed `wyrd-spec` wire shapes, handlers, shared Verification
client capability, thin language projections, and MCP tools.

**REFACTOR.** Remove any duplicate result/status transport or language-owned
state machine.

### Scenario 7 — Runtime limits, audit, health, and shutdown are enforced

**Behavior.** Global 16/per-tenant 4 Verifier capacity prevents one tenant
starving another. Shutdown stops claims, drains 30 seconds, then makes durable
work reclaimable; a crashed required task restarts and health is degraded until
present. Metrics/traces expose bounded queue/work/attempt/failure/latency.
Only public/Gate permission decisions audit; claims, retries, commits, and
worker mechanics do not.

**RED.** Add deterministic permit, shutdown/restart, supervisor, telemetry,
and audit-cardinality tests.

**GREEN.** Compose existing server supervision, bounded permit, clock,
telemetry, and audit owners around the durable workers.

**REFACTOR.** Keep one runtime owner and no process-local work registry.

## Acceptance Criteria

- Status/verdict independence and nullable direct-run ownership match the spec.
- Remote result writes obey SYSTEM/Gate/Scribe identity and ACK semantics.
- Manual/scheduled work, claims, retries, concurrency, shutdown, and audit
  satisfy `AC-015`, `AC-023`, `AC-028`, and runtime portions of `AC-030`.
- Drift/Eval/Operator tasks can consume this runtime without a second lifecycle.

## Expected Write Set and Consumer Closure

Likely owners: `wyrd-spec` verification/auth/error contracts, `wyrd-runtime`
principal/permissions, auth issue/verify/provisioning, `wyrd-sql` migrations and
queries, `wyrd-server` routes/runtime/state/health, `vala-bifrost-redux` Gate,
shared Bifrost/Verification clients, SDK bindings, MCP tools, and production-
shaped server/Bifrost tests.

## Verification and Evidence

```bash
mise run test:principals:unit
mise run test:principals:integration
mise run test:sql
mise run test:shared
mise run test:wyrd
mise run test:vala
mise run test:bifrost:integration:server
mise run test:bifrost:journey:sdk
mise run test:bifrost:journey:server
mise run test:bifrost:journey:mcp
mise run test:e2e
mise run py:test:integration
mise run py:typecheck
mise run ts:test:integration
mise run ts:typecheck
mise run codegen:check
mise run check:tenant-isolation
mise run check:client-tier
mise run check:unwrap-audit
mise run fmt
mise run lints
git diff --check
```

New named tests require exact focused commands after their targets and names
exist; do not invent selectors during implementation.

## Material Stop Conditions

Stop for changed result schemas, stronger atomicity/recovery promises, new
permissions, another run/status API, direct Scribe access, a public SYSTEM
credential, different concurrency limits, or changed audit cardinality.

## Authority Links

- `changes/active/verified-change-contract/spec.md`
- `changes/active/verified-change-contract/architecture/verification-control-flow.html`
- `changes/active/verified-change-contract/architecture/logic/table_schema.md`
- `architecture/wyrd-security-posture.md`
- `architecture/bifrost-design.md`
- `architecture/references/languages/implementation-execution.md`
- `AGENTS.md`
