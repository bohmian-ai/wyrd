# TASK-001 recovery and custody review

Subject: base `66f44aee193077ef3565aa76140a942d8eb593f9`, candidate
`bf438a8d7985aadd29db3beb73ed0269c2b2f7ca`. HEAD was unchanged at the end
of source inspection. This is an independent specialist report, not the final
task verdict.

Scope: REQ-004–006 in approved spec revision 1, original TASK-001 scenario 2;
accepted-only replay, whole-batch custody, persisted frontier, stage retirement,
and torn-tail safety. SQL eligibility and cleaner lifecycle are assigned to
the other specialist. No production edits or test executions were made.

Authority inspected: repository AGENTS instructions; change workflow and task
packet; maintainer style; testing policy; `architecture/bifrost-design.md`,
“Live tail, recovery, and shutdown” and “Receipt handoff and cleanup.”
There is no narrower AGENTS file in the affected Bifrost source tree.

## Source and assertion coverage

| Promise | Trace and evidence | Assessment |
|---|---|---|
| REQ-004: restore accepted canonical attempts only | `shards.rs:1915` resolves each reconstructed COMMIT through tenant SQL before adopting the generation. Both `Unfenced` and `Suppress` remove that attempt's rows; SQL errors propagate. Existing canonical-coordinate resolution remains authoritative. `lifecycle.rs:595` refuses receipt INSERT, abruptly restarts, and asserts only accepted rows; `shards.rs:8961` tests the real uncommitted group-fence transaction and same-ID retry. | PASS on source and recorded execution |
| REQ-004: losing duplicate stays absent after winner receipt expires | `lifecycle.rs:792` sends one identity through two pods under the same credential, removes the winner's receipt only after its frontier covers it, abruptly restarts the loser, flushes, and asserts the exact original rows once. Missing authority is suppressed by the same accepted-only branch. | PASS on source and recorded execution |
| REQ-005: crossing batches and gaps cannot advance a partial frontier | `shards.rs:4209` gives every partition every segment of its durability group. Selective retirement (`:2658`) filters against all active/pending/retained/cohort references (`:2702`); automatic rotation retains the complete union (`:3918`). `wal.rs:3049` advances only from the current safe prefix through consecutive released segment sequences. The crossing-partition test (`shards.rs:10370`) asserts equal complete segment sets for both partitions after an actual roll; the WAL test (`wal.rs:4967`) releases sequence 1 before 0 and asserts no deletion or frontier until the gap closes. | PASS on source and recorded execution |
| REQ-005: durable checkpoint precedes file disposal | `manifest.rs:200` uses the existing guarded write/fsync/rename/fsync-parent mechanism. `wal.rs:3112` persists first, then changes the in-memory safe value; only returned covered paths can be recycled/deleted. A failed manifest write leaves the frontier unchanged and paths deferred. `wal.rs:5024` blocks the temp-file path and asserts failure with the segment retained. | PASS on source and recorded execution |
| REQ-005: load checkpoint before interpreting missing receipts | `WalReader::open_paths` (`wal.rs:3742`) reads each shard manifest and deletes/skips covered segments before replay assembles SLICE/COMMIT sets. `replay.rs:2133` simulates only the first covered segment being deleted and asserts successful empty replay and removal of the remaining covered segment. | PASS on source and recorded execution |
| REQ-005–006: connected retained-data recovery and torn tail | `lifecycle.rs:682` drives real accepted input → publication → observed persisted frontier → real tenant deletion → abrupt restart → exact rows. It keeps another WAL-dependent receipt, tears that final WAL tail, and verifies rows 0–47 with no unaccepted rows. `lifecycle.rs:870` performs the equivalent handoff using staged, unpublished rows and then a second restart after publication. | PASS on source and recorded execution |
| REQ-005–006: replay and stage retirement preserve custody | Replay handoff prepares all states and assigns union retirement authority before persistence (`shards.rs:1581–1685`); union cohorts absorb overlapping paths and reader pins protect unread segments. `execution_lanes.rs:2627` persists a per-key watermark before releasing replay references. Unchanged `hot_stage.rs:578` folds each staged member into its shard manifest using the same manifest guard, preserving the safe field. Stage records remain independent custody until publication retirement. The staged-row journey asserts exact rows on both sides of this retirement. | PASS on source and recorded execution |

The new group-wide references are deliberately conservative. They reuse the
existing owning maps and cohort retirement; no new per-batch ledger or recovery
authority is introduced. During replay, per-slice segment sets do not independently
retire: `assign_replay_segment_authority`, merged cohorts, and reader pins protect
the union until its dependent members complete.

## Execution evidence and process history

The task records real Postgres/redux and multiprocess Scribe journey execution:
479 redux / 51 Scribe journeys at scenario-2 close, later 483 / 52 at scenario-3
close, with the exact setup/runner in the task. The assertions above were inspected,
not inferred from their names. These are carried implementation records; this
review did not independently rerun the commands and does not attest new execution.
Root validation must reconcile their candidate applicability with later R1 changes.

The task records expected behavioral RED for accepted-only replay, contiguous
frontier, failed persistence, replay frontier skipping, and crossing partitions.
It explicitly records that the connected restart, losing-duplicate, staged-retirement,
and torn-tail journeys were added after production behavior. Preserve that TDD
violation independently of their useful behavioral proof; later mutation runs do
not establish test-first history.

## Findings and limits

No blocking implementation finding is proposed within this assigned boundary.
The SQL current-coordinate predicate, mirror invalidation, admission-health scope,
domain consumers, benchmark qualification, and full task acceptance belong to the
other selected reviewers and validator.

Skipped: fresh Cargo/Postgres runs and performance measurements. Risk remaining
for acceptance: this report relies on recorded execution evidence and must be
reconciled with the immutable candidate by the validator.
