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

## Acceptance Evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| A frozen `1..100` survives row `101`; no competing or restarted publisher widens it | `vala-sql/migrations/20260802000000_vala_audit_staging.sql` (`publishing_seq_hi` + CHECK), `vala-sql/src/queries/audit_staging.rs::freeze_publication_range` | `pg_audit_staging::frozen_range_survives_tail_growth_competition_and_stale_settlement` | PASS |
| Crash after Scribe acceptance, before watermark advance, replays the identical tenant and bounds into the batch fence | `wyrd-server/src/audit/publication.rs::publish_range`, `vala-bifrost-redux/src/tables/audit/projection.rs::derive_batch_id` | `server::audit_publication::frozen_audit_range_replays_once_while_its_tail_waits` (two `publish_range` calls on one frozen range, retained count still 3) | PASS |
| A stale completion neither clears nor advances past a newer in-flight bound; rows above the bound stay staged | `vala-sql/src/queries/audit_staging.rs::settle_publication` (`GREATEST` + `CASE WHEN publishing_seq_hi = $1`) | `pg_audit_staging::frozen_range_survives_tail_growth_competition_and_stale_settlement` (asserts `published_seq = 3`, `publishing_seq_hi = Some(4)` after the stale settle) | PASS |
| Watermark advance, bound clearance, and deletion commit atomically; an idle tenant drains to zero | `settle_publication` (one tenant transaction), `publication.rs::settle` | `pg_audit_staging::settled_tenant_drains_to_zero_and_owes_nothing`, `pg_audit_staging::publication_batch_is_bounded_and_settlement_is_idempotent`, journey `await_drained` | PASS |
| Concurrent tenants publish independently; slow publication holds no audit-chain append lock | `publication.rs::freeze` / `read_range` / `settle` each own a short transaction; no lock spans `ingest_frame` | `pg_audit_staging::settlement_is_tenant_scoped`; the server journey appends a new decision after the fence commits and the background worker carries it | PASS |
| AuditPublisher starts only where a local Scribe exists and calls it directly; no retained path calls Gate or audits itself | `publication.rs::from_state` (`state.bifrost_ingest()?.scribe()`), `publish_range` calls `ScribeImpl::ingest_frame` | `server::audit_publication::audited_transitions_retire_only_into_retained_history` (background worker publishes and drains); `grep` shows no `publish_audit_projection` caller remains | PASS |
| Gate's audit-public retained-publication bypass is deleted without a replacement trait, adapter, service, or public API | `vala-bifrost-redux/src/gate/mod.rs` — `publish_audit_projection` and `audit_batch_id` removed; version/variant stamping folded into `derive_batch_id` | `mise run lints` (exit 0, `--all-features --all-targets`) | PASS |
| Architecture and replay documentation describes the frozen in-flight range | `architecture/bifrost-design.md`, `architecture/wyrd-security-posture.md`, `AGENTS.md` §2 | Text review: the "watermark is the only progress state" claim is replaced by watermark + one frozen bound | PASS |
| Exactly one new canonical-ingest concurrency scenario, in an existing harness and test binary | `wyrd-testing/tests/bifrost/scribe/horizontal_ingest.rs::one_sealed_batch_submitted_to_every_pod_is_visible_once` (reuses `WyrdTestCluster`, `endpoint_clients`, `append_values`, `read_rows`) | `scribe::horizontal_ingest::one_sealed_batch_submitted_to_every_pod_is_visible_once` | PASS |

Non-goals held: no lease, owner token, claim table, scheduler, service, global
lock, migration, compatibility path, backfill, new test file, or new harness was
added. `list_publication_batch` survives only as the staging-inspection read the
journeys and SQL tier already used.

### Commands

```
mise run fmt                           # clean
mise run lints                         # exit 0
mise run check:unwrap-audit            # unwrap/expect audit passed
mise run test:sql                      # 112/112 + 2/2
mise run test:bifrost:journey:server   # 7/7
mise run test:bifrost:journey:scribe   # 18/21 (3 pre-existing failures, below)
git diff --check                       # clean

# Focused, through the repository Postgres wrapper:
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner \
  && mise exec -- cargo nextest run --locked -p vala-sql --test pg_audit_staging \
     -E 'test(=pg_tests::audit_staging::frozen_range_survives_tail_growth_competition_and_stale_settlement) \
       + test(=pg_tests::audit_staging::publication_batch_is_bounded_and_settlement_is_idempotent) \
       + test(=pg_tests::audit_staging::settled_tenant_drains_to_zero_and_owes_nothing) \
       + test(=pg_tests::audit_staging::settlement_is_tenant_scoped)' \
  && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all \
     -E 'test(=audit_publication::frozen_audit_range_replays_once_while_its_tail_waits) \
       + test(=audit_publication::audited_transitions_retire_only_into_retained_history)' \
  && mise exec -- cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all \
     -E 'test(=horizontal_ingest::one_sealed_batch_submitted_to_every_pod_is_visible_once)'"
# 4/4, 2/2, 1/1 PASS
```

### Material limits

- Three `test:bifrost:journey:scribe` cases encoded assumptions this task
  falsified and were corrected rather than reported as pre-existing:
  - `source_boundary_recovery::scribe_failure_retry_replay_remain_atomic`
    counted `vala.audit_staging` rows for
    `operation = 'bifrost.scribe.visibility.publish'`, a name that exists only as
    a tracing span — Scribe publication is lineage, not audit, so the count was
    structurally zero. It now counts the published generations the commit
    produced, which is the lineage authority for the property it claimed. The
    redundant second assertion and the dead
    `WyrdTestServer::scribe_publication_audit_count_for_test` probe were deleted.
  - `qualification::scribe_512_mib_physical_object_qualifies` and both
    bucket-ownership assertions in
    `sustained::scribe_sustained_ingest_oracle_hot_read_journey` asserted that
    Scribe owns no writable bucket at all. This task gave the server its own
    retained-audit writer on a fixed interval, so `vala.system.audit_log` buckets
    appear on the publisher's schedule and no global count can be stable. The
    three sites now read through `support::journey_buckets`, which excludes the
    `Audit` namespace and returns every other bucket so a failure names the
    surviving seal keys.
- `mise run codegen:check` was not run: no wire type, schema, OpenAPI surface, or
  stub changed. `AuditProjection::batch_id` moved from `[u8; 16]` to `Uuid`, and
  that type is engine-internal with no generated projection.
