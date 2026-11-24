# TASK-001-R2 correction decisions: independently checked

Checked the revised R2 against runtime commit
`bf438a8d7985aadd29db3beb73ed0269c2b2f7ca`, approved spec revision 2, the two
retained findings, actual producers/consumers, and the existing cleaner lifecycle.
CodeGraph preceded source inspection. This validates the proposed correction
decisions and proof plan; it does not claim implementation or behavioral closure.

## Mirror population decision

Adding `spent_batch_ids` to `ShardOwner::record_committed`'s exclusions closes
the identified cache population path. `append_and_sync_batch_commits` marks
matching `AlreadyCommitted` attempts spent, so another pod's duplicate cannot
newly acquire mirror authority. Rejected version/conflict paths remain excluded.
New local acceptances and same-coordinate ambiguous-COMMIT reconciliation remain
`Committed`, not spent, and keep the current mirror path after visibility.

The correction does not remove an already valid local mirror entry, change its
identity checks, alter acknowledgement results, or modify forget-before-deletion.
A locally owned receipt encountered through the durable duplicate path after
mirror eviction also stays uncached; this loses an optimization rather than
correctness. Such retries use the existing shared WAL/PostgreSQL arbitration.
The task should state this alongside the nonaccepting-pod retry tradeoff.

No TTL, owner-coordinate cache field, cross-pod invalidation, ledger, new SQL
probe, schema change or ingress rewrite is required to close this finding.

## Recovery/cleanup decision

Removing the synchronous `sweep_and_report` block from successful
`replay_wal_async` closes the identified startup coupling. Existing preceding
custody recovery and its failure path remain authoritative for readiness.
`ScribeImpl::build` already starts one cleaner; `ReceiptCleaner::start` sweeps
immediately before sleeping, and `CleanupHealth::default` initially holds no
completed coverage. Existing admission refuses until that worker completes a
current whole sweep. The fix therefore needs neither a replacement worker nor
a new readiness timeout or health mechanism.

A first sweep completing during recovery is valid for its captured boundary.
Subsequent recovery frontier advancement triggers the existing notification and
is handled by later sweeps. Requiring another synchronous sweep after recovery
would recreate the defect. Process readiness can precede healthy write admission;
that is the required separation rather than a weakened startup cleanup rule.

## Connected scope and proof

The revised write set is complete for these decisions: the shard population
predicate, deletion of the replay await, and corresponding mirror/cleaner
rustdoc. Existing ingress, SQL arbitration, mirror forgetting, worker startup,
health/admission and boot algorithms are reused.

The three required test flows distinguish the defect from mere ACK success:

- A real population-method assertion checks new local caching, spent omission
  and preservation of an already valid local entry.
- The two-pod flow keeps the duplicate pod alive while the accepting pod's real
  cleaner expires the receipt, then requires a new canonical receipt and exact
  duplicate row population after reacceptance.
- The restart flow ages and locks a safely handed-off receipt while the old pod
  is stopped, avoiding deletion races in setup. It requires evidence of the
  pending cleanup DELETE, successful recovered serving, exact busy refusal,
  and automatic admission resumption after lock release.

These assertions need the stated real dependency/journey tier; isolated models
cannot establish cross-pod lifetime or boot availability. Bounded test deadlines
and exact registered nextest selectors are appropriate. Existing owner lanes
cover the affected implementation without adding a full shipped-surface gate.

No material behavior or owner choice remains unresolved for implementing R2.
Private test setup and local helper choices remain with the implementer. The
findings remain open until the actual corrections and required executions exist.

Skipped: implementation, test execution, lints and performance measurements.
Risk: repeated durable retries lose mirror acceleration; the task preserves this
explicit tradeoff and requires performance evidence without claiming closure now.
