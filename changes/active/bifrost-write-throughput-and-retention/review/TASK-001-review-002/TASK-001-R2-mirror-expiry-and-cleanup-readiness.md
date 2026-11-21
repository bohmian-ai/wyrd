---
id: TASK-001-R2
kind: remediation
status: proposed
spec: SPEC-bifrost-write-throughput-and-retention
spec_revision: 2
original_task: TASK-001
review_base: 66f44aee193077ef3565aa76140a942d8eb593f9
review_candidate: bf438a8d7985aadd29db3beb73ed0269c2b2f7ca
findings: [FIND-TASK-001-1, FIND-TASK-001-2]
---

# Expire cross-pod retry acknowledgements and keep cleanup off readiness

## Outcome and authority

A retry sent to a different running pod cannot receive an acknowledgement from
a receipt that has been deleted. A recovered server can serve retained rows while
initial receipt cleanup is blocked; fresh writes remain busy until cleanup succeeds.

This task corrects only settled REQ-002 and REQ-008–009 in
[spec revision 2](../../spec.md), using the existing owners in
[TASK-001](../../tasks/TASK-001-fence-lifecycle.md). The diagnosed paths and their
independent validation are recorded once in [findings-validation.md](findings-validation.md).
The review covers the cumulative identities above; R1 was maintainer-directed,
not a prior independent acceptance verdict.

The maintainer approved the R1 Bifrost-only/latest-readable behavior and requested
the spec correction and renewed review. Spec revision 2 and the owning architecture
record that decision. Preserve that behavior while correcting these two settled
receipt and availability defects. No application receipt reads are reintroduced.

## Corrections and connected scope

### FIND-TASK-001-1: another pod can retain a deleted receipt's acknowledgement

Pod A accepts and durably hands off a batch. Pod B handles a matching retry,
suppresses its rows after SQL arbitration, but still caches the acknowledgement.
A's cleaner deletes the receipt and forgets only A's mirror. B's discovery covers
B-owned streams, so the live B mirror continues acknowledging the deleted identity.

**Selected fix:** in `scribe/shards.rs::ShardOwner::record_committed`, skip
every batch in `state.spent_batch_ids` before calling `self.committed.record`.
Use the existing set populated by `append_and_sync_batch_commits`; it already
marks `AlreadyCommitted`, `AcceptedUnder`, and conflicted attempts. Keep the
existing version/conflict exclusions and acknowledgement handling. A duplicate
still gets its correct acknowledgement for the current attempt; it simply does
not create a new mirror entry from somebody else's acceptance.

Newly committed local batches continue entering the existing bounded mirror after
visibility. Their accepting node's cleaner remains responsible for forgetting
them before receipt deletion commits. Do not clear a valid existing local mirror
entry merely because a later duplicate is spent. Update the mirror population
rustdoc in `shards.rs` and `committed_batches.rs` so it describes locally committed
acceptances, rather than deliberately suppressed duplicates.

The complete retry path becomes: B misses its mirror → normal shared shard/WAL
and PostgreSQL arbitration → matching duplicate ACK while A's receipt exists,
or fresh acceptance after A's receipt has actually gone. B never gains a stale
mirror entry from the duplicate ACK. Mismatched content and accepted-version
resolution retain their current paths and errors. No change to ingress,
`CommittedBatches` storage/lookup/forget, cleanup SQL, or receipt schema is needed.

**Tradeoff:** repeated retries handled by a nonaccepting pod use the existing
durable duplicate path instead of its mirror fast path. Accepting-pod retries
retain the fast path while their original mirror entry remains cached. After
FIFO eviction, even an accepting pod's `AlreadyCommitted` retries no longer
repopulate that entry and use durable arbitration instead. Choose this cache omission rather
than cross-pod invalidation, expiry timestamps, new SQL probes or a new ledger.
It adds no new kind of wait or durability operation to the shared write path.

### FIND-TASK-001-2: blocked initial cleanup holds recovered service readiness

After custody recovery, `Scribe::replay_wal_async` awaits `sweep_and_report` before
setting recovery ready. Server boot awaits that call. A row lock on an old,
handed-off receipt can block DELETE or the sweep mutex indefinitely, coupling
maintenance progress to service startup rather than fresh-write admission.

**Selected fix:** remove the `if let Some(cleaner)` block that awaits
`cleaner.sweep_and_report()` from the successful result branch of
`ScribeImpl::replay_wal_async` in `scribe/mod.rs`. After successful custody
recovery, set `recovery_ready` and record replay timing immediately, as the
existing success branch already does after that block. Preserve every preceding
staging/WAL recovery step and the error path; failed recovery must remain unready.

Keep `ReceiptCleaner::start` in `ScribeImpl::build`. That existing single worker
already executes a sweep immediately before waiting for its first cadence or
notification. It is the sole production sweep driver; do not spawn another task,
move worker construction into replay, or add a readiness timeout. Update
`sweep_and_report` rustdoc to remove the claim of a synchronous startup caller;
retain the current sweep serialization and outcome publication.

Keep default `CleanupHealth` unverified and keep the existing local check in
`AdmissionController::try_reserve`. The background worker's first successful
whole sweep establishes cleanup health; a pending/failed sweep leaves new writes
busy, while successful custody recovery permits boot/role activation and retained
reads. If the initial worker sweep already finished successfully during recovery,
its still-current coverage remains valid. If recovery advances a frontier, the
existing frontier notification requests another sweep under the unchanged policy.
No new post-recovery health state or compulsory second initial sweep is introduced.

When a blocked sweep completes, its existing boundary-based health publication
automatically permits fresh writes subject to ordinary admission. Keep the
worker's existing startup, wake, retry, staleness and shutdown owners. Do not change
`is_ready`, boot orchestration, cleanup SQL, health deadlines, admission error
mapping, or recovery authority to compensate for this removed await.

## Connected write set

Production changes are confined to `scribe/shards.rs` (mirror population plus
its docs), `scribe/mod.rs` (remove the synchronous cleanup wait), and the related
module/method rustdoc in `committed_batches.rs` and `receipt_cleaner.rs`.
Existing ingress, SQL arbitration, mirror deletion, cleaner startup, admission,
boot, and retained-read consumers keep their algorithms. Tests belong in the
existing inline shard owner and Scribe lifecycle owners below. No configuration,
export, fixture package, dependency, schema or public API change is required.

## Observable closure scenarios

| Finding / requirement | Scenario and required observation | Narrowest credible proof |
|---|---|---|
| FIND-TASK-001-1 / REQ-002 | Accept/publish on A; retry on B using the same tenant/principal/table/version and prove B acknowledged the prior acceptance. Let A's real cleaner remove the safely handed-off aged receipt, keep B running, and retry on B. Assert authoritative reacceptance and the resulting receipt/rows rather than ACK success alone. No restart may clear B's mirror between deletion and retry. Also preserve matching/conflicting retries while authoritative. | Existing multiprocess Scribe lifecycle journey and focused mirror/arbitration tests. Internal expiry setup stays in the internal harness; introduce no public operational control. |
| FIND-TASK-001-2 / REQ-008–009 | Restart with independently durable accepted rows and eligible receipt debt. Block cleanup DELETE with a real transaction lock. Prove boot/recovery and retained reads finish while fresh writes return retryable busy. Release the lock; a current initial sweep completes and writes resume without restart. Preserve the existing unavailable-recovery-authority case. | Real-Postgres lifecycle/startup regression through the production-shaped server/cluster harness, plus focused cleaner/health checks where needed. |

### Required test actions and assertions

1. **Mirror population, inline shard owner test.** Use the existing owner/prepared
   append setup in `scribe/shards.rs`. Include a locally committed batch and an
   otherwise valid same-version `AlreadyCommitted` batch marked spent. Call the
   real mirror population method. Assert only the locally committed identity is
   known and exactly matched; populating a spent retry never creates an entry.
   Cover an already cached local identity remaining intact when its later attempt
   is spent. This test should fail before the new exclusion. Do not replace the
   production population method with a test-only model of the predicate.
2. **Live second-pod journey, `scribe/lifecycle.rs`.** Reuse the existing two-mixed
   cluster and same-credential setup of
   `a_losing_duplicate_restores_nothing_after_the_winning_receipt_is_deleted`,
   but add a separate case that keeps B alive. Publish on A, retry on B and
   assert the table still contains one copy. After the persisted frontier covers
   A's receipt, age it with the existing internal PostgreSQL fixture and wait
   boundedly for A's real cleaner to delete it. Retry on B without restarting B.
   Assert a new receipt is owned by B and the exact original rows occur twice
   after delivery/publication. Successful ACK alone is insufficient. Preserve
   the existing losing-duplicate restart case and matching/conflicting retry
   tests; do not rewrite their expectations.
3. **Blocked initial sweep journey, `scribe/lifecycle.rs`.** Publish accepted
   rows and establish a persisted frontier covering their receipt. Stop that
   node while the receipt is still recent. With the node stopped, age the
   receipt through the existing internal fixture, then hold a real transaction
   row lock on that eligible receipt before restarting on its retained roots.
   This ordering prevents the old cleaner deleting it before the test holds
   the lock. Restart must complete within the bounded test deadline while the
   lock remains held; verify the new cleaner has reached the blocked DELETE
   using the internal database boundary rather than assuming it from elapsed
   time. Query exact retained rows and assert a fresh append returns
   `WYRD_VALA_429_INGEST_BUSY`. Release the lock, observe committed receipt
   removal and retry a fresh append until admitted within the existing cleanup
   deadline; assert the appended rows. A direct `sweep_and_report` unit call
   does not substitute for this boot/recovery path. No public failure control,
   production timing change or broad test-harness redesign is required.

All three tests are test-first for these corrections. A bounded restart timeout
is an expected behavioral RED only when the trace shows custody recovery completed
and cleanup DELETE is waiting on the held lock; setup/compilation failure is not RED.

Use the current Scribe lifecycle journey target under the repository PostgreSQL
wrapper. Confirm each new test's actual nextest name and run its exact selector
before RED and after correction; do not invent names in this packet. The existing
runner shape is:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all --test-threads=1 -E "test(/^lifecycle::/)"'
```

For the new inline test, use the actual registered name with
`mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=<actual-name>)'`.
For each new journey, use the PostgreSQL wrapper above with its exact registered
`test(=<actual-name>)` selector; add `--features test-support` only where the owning
target requires it. Preserve the focused cleaner-health and losing-duplicate
regressions, then run `mise run test:bifrost:integration:redux` and
`mise run test:bifrost:journey:scribe` once after closure. Finish `mise run fmt`,
`mise run lints`, `mise run check:tenant-isolation`, and
`mise exec -- git diff --check`. SQL receipt integration is unnecessary when the
unchanged SQL owner is preserved. Keep Cargo commands serial.

The existing task's writes/hot/Parquet checkpoint policy still applies. Record
the nonaccepting-pod duplicate-path tradeoff separately from ordinary ingest;
do not claim unchanged ordinary-ingest throughput proves cross-pod retry cost.
No new benchmark framework or full CPU qualification belongs to this remediation.

No CPU throughput implementation, extra worker, public test control, migration,
receipt quota, domain contract redesign, full journey aggregate, commit, or push
is part of this remediation task. Preserve recorded TDD violations; new
closure tests follow current test-first discipline without claiming to repair history.
