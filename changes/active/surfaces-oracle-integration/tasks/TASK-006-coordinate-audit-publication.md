---
id: TASK-006
kind: implementation
status: ready
spec: SPEC-surfaces-oracle-integration
spec_revision: 7
requirements: [REQ-027, REQ-027C, REQ-028, REQ-029, INV-008, INV-008C, INV-008D, AC-005]
depends_on: [TASK-005]
parent_task: TASK-001
remediates: []
---

## Objective

Make retained audit publication safe across crashes and multiple Scribe
replicas, and route retained audit batches through their owning local Scribe
without passing through Gate. One logical staging row reaches
`vala.system.audit_log` once even when the staging tail grows during replay.

Implement this task immediately after TASK-005 against TASK-001's completed
integration result. TASK-001 remains implemented; only its closeout is blocked
until both child tasks complete.

## Constraints

- Retained publication runs only on targets with a local Scribe. Oracle-only
  and Forge-worker-only processes never run it.
- Multiple Scribe replicas remain supported; correctness cannot depend on a
  singleton deployment or process-local coordination.
- Persist only one nullable in-flight upper sequence bound beside the existing
  per-tenant published watermark. Do not add a lease, owner token, claim table,
  scheduler, service, or global lock.
- Audit state is unshipped. Edit the existing schema definition in place; do
  not add a migration, compatibility path, or backfill.
- Establish and settle the in-flight bound in short tenant transactions. Never
  hold the audit-chain append lock or a database transaction across Scribe IO.
- AuditPublisher calls the existing local Scribe ingestion capability directly.
  It never calls Gate, evaluates permission, or emits another audit event.
- Preserve bounded polling, tenant isolation, deterministic batch identity,
  Scribe's durable dedup fence, fail-closed publication, and idle drain-to-zero.
- Update the existing retained-publication replay coverage for the audit race.
  Add one Tier-1 concurrency scenario proving the canonical Gate-to-Scribe path
  deduplicates one sealed batch submitted simultaneously to multiple Scribe
  replicas. Both scenarios must reuse existing harnesses and test binaries; do
  not add a harness or test file.

## Relevant Surface

- Retained audit publisher lifecycle in `crates/wyrd/wyrd-server`
- Scribe-bearing role composition and existing Scribe handle in server state
- Tenant audit staging, chain-head publication state, and Scribe batch fence in
  `crates/vala/vala-sql`
- Audit projection batch identity and Scribe ingress in
  `crates/vala/vala-bifrost-redux`
- Existing retained audit publication journey under
  `crates/wyrd/wyrd-testing/tests/bifrost/server`
- Existing multi-pod Scribe journey and cluster harness under
  `crates/wyrd/wyrd-testing/tests/bifrost/scribe` and
  `crates/wyrd/wyrd-testing/src/bifrost`
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
7. Add the single canonical-ingest control scenario using the repository's
   existing multi-pod journey infrastructure, without changing production
   behavior unless the test exposes a real defect.

## Test Scenarios

1. Freeze an audit range, append beyond its upper bound, race another
   publisher, and prove both attempts use the same frozen range until it
   settles.
2. Replay an accepted audit range after a crash before watermark advancement;
   prove Scribe absorbs the identical batch and retained rows remain unique.
3. Submit one immutable canonical Bifrost batch concurrently through distinct
   server endpoints backed by distinct Scribe replicas; prove the public read
   returns each `(batch_id, row_ordinal)` exactly once.
4. Let the audit publisher become idle and prove staging drains completely.

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
- Exactly one new canonical-ingest concurrency scenario is added using an
  existing multi-pod journey harness and test binary. No new test file or
  harness is added.

## Verification

Use the existing retained-publication scenario and repository verification lanes:

exercise only these tests + the retained-audit replay scenario and the added canonical-ingest scenario

```bash
mise run fmt
mise run lints
mise run test:sql
mise run test:bifrost:journey:server
mise run test:bifrost:journey:scribe
git diff --check
```

The retained-audit replay scenario must demonstrate tail growth, competing
publisher attempts, crash-before-watermark replay, no duplicate retained
sequence, and final staging drain. The one added canonical-ingest scenario must
demonstrate simultaneous delivery of an identical sealed batch to distinct
Scribe replicas and exact-once public visibility. The implementer must inspect
the current harness and record the exact focused command after choosing the
existing test location; missing proof blocks completion.
