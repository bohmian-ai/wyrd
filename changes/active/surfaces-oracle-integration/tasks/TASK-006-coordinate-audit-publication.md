---
id: TASK-006
kind: implementation
status: ready
spec: SPEC-surfaces-oracle-integration
spec_revision: 6
requirements: [REQ-027, REQ-028, REQ-029, INV-008, INV-008C, AC-005]
depends_on: [TASK-005]
parent_task: TASK-001
remediates: []
---

## Objective

Make retained audit publication safe across crashes and multiple Scribe
replicas, and route retained audit batches through their owning local Scribe
without passing through Gate. One logical staging row reaches
`vala.system.audit_log` once even when the staging tail grows during replay.

Implement this task immediately after TASK-005 when the paused, in-progress
TASK-001 resumes. TASK-001 cannot close before both tasks complete.

## Constraints

- Retained publication runs only on targets with a local Scribe. Oracle-only
  and Forge-worker-only processes never run it.
- Multiple Scribe replicas remain supported; correctness cannot depend on a
  singleton deployment or process-local coordination.
- Persist only one nullable in-flight upper sequence bound beside the existing
  per-tenant published watermark. Do not add a lease, owner token, claim table,
  scheduler, service, or global lock.
- Establish and settle the in-flight bound in short tenant transactions. Never
  hold the audit-chain append lock or a database transaction across Scribe IO.
- AuditPublisher calls the existing local Scribe ingestion capability directly.
  It never calls Gate, evaluates permission, or emits another audit event.
- Preserve bounded polling, tenant isolation, deterministic batch identity,
  Scribe's durable dedup fence, fail-closed publication, and idle drain-to-zero.
- Do not add a test case or test file. Extend the existing retained-publication
  replay coverage with this race and use the existing verification lanes.

## Relevant Surface

- Retained audit publisher lifecycle in `crates/wyrd/wyrd-server`
- Scribe-bearing role composition and existing Scribe handle in server state
- Tenant audit staging, chain-head publication state, and Scribe batch fence in
  `crates/vala/vala-sql`
- Audit projection batch identity and Scribe ingress in
  `crates/vala/vala-bifrost-redux`
- Existing retained audit publication journey under
  `crates/wyrd/wyrd-testing/tests/bifrost/server`
- Watermark and replay authority in `AGENTS.md`,
  `architecture/bifrost-design.md`, and
  `architecture/wyrd-security-posture.md`

Paths are ownership guidance, not a private implementation allowlist.

## Approach

1. Extend the tenant publication state with one nullable upper bound for the
   batch currently owed to retained history.
2. Under tenant serialization, reuse an existing in-flight bound or establish
   one from the current bounded staging prefix, then release the transaction.
3. Read and publish exactly that frozen sequence range through the local Scribe
   handle, deriving the same batch identity on every replica and crash replay.
4. After Scribe durably accepts the range, conditionally advance the watermark,
   clear only the matching in-flight bound, and garbage-collect through the
   watermark in one tenant transaction.
5. Remove Gate's retained-audit publication method and compose AuditPublisher
   directly from the Scribe handle it already requires at startup.
6. Correct the architecture text that claims watermark-only replay is
   sufficient, and revise the existing replay coverage to exercise a growing
   tail and competing publication attempts.

## Acceptance Criteria

- When one publisher freezes `1..100` and row `101` arrives, every competing or
  restarted publisher continues to use `1..100` until that batch settles; none
  publishes an overlapping `1..101` batch.
- A crash after Scribe accepts a batch but before watermark advancement replays
  the identical tenant, lower bound, and upper bound, so Scribe's existing batch
  fence absorbs it without duplicate retained rows.
- A stale completion cannot clear or advance past a newer in-flight bound.
  Rows arriving above the frozen bound remain staged for the next batch.
- Watermark advancement, matching-bound clearance, and deletion through the
  watermark commit atomically after durable Scribe acceptance. Failure before
  that commit loses no row, and an idle tenant eventually drains to zero.
- Concurrent tenants can publish independently, and slow Scribe publication
  does not hold the audit-chain append lock or block new authorization records.
- AuditPublisher starts only on `All`, `Server`, and `Scribe` targets and calls
  local Scribe directly. No retained-audit path calls Gate or creates another
  authorization audit event.
- Gate's audit-public retained-publication bypass is deleted without adding a
  replacement trait, adapter, service, or public API.
- Existing architecture and replay documentation describes the frozen
  in-flight range rather than claiming that the watermark alone makes a
  changing tail replay-safe.
- No new test case or test file is added.

## Verification

Use the existing retained-publication scenario and repository verification lanes:

```bash
mise run fmt
mise run lints
mise run test:sql
mise run test:bifrost:journey:server
mise run verify:bifrost
git diff --check
```

The existing replay scenario must demonstrate tail growth, competing publisher
attempts, crash-before-watermark replay, no duplicate retained sequence, and
final staging drain. Missing proof blocks completion rather than authorizing a
new test case or file.
