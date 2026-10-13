**Diagnosis: the test’s completion premise is wrong.** `compact(geometry)` proves the geometry table has Forge outputs and drains existing durable tasks. It does **not** prove the neighbour’s pending compaction has been dispatched, admitted, or completed. The exact dispatch history in the two failing runs remains unproven because those logs omit it.

**Symptom**

At `production_closeout.rs:1367`, the neighbour’s current snapshot equals its latest promoted Append snapshot:

- `geom-fail-gate6.txt:94–97`: both IDs are `7132536473241758141`.
- `geom-fail-gate4.txt:23–26`: both IDs are `8054122723315370989`.

The geometry assertion immediately before it passed. Thus geometry completed, but the neighbour had no observable replacement head when inspected. This was an early assertion failure, not a compaction timeout.

**Evidence**

The helpers establish three different boundaries:

1. [`scheduler_pass()`](/home/thorrester/Documents/GitHub/wyrd/crates/wyrd/wyrd-testing/tests/bifrost/forge/production_closeout.rs:843) waits for a completed scheduler pass. That pass only sweeps **promotion debt**: [`scheduler.rs:383–455`](/home/thorrester/Documents/GitHub/wyrd/crates/vala/vala-bifrost-redux/src/forge/scheduler.rs:383). It does not pull ordinary compaction or wait for workers.
2. [`compact()`](/home/thorrester/Documents/GitHub/wyrd/crates/wyrd/wyrd-testing/tests/bifrost/forge/production_closeout.rs:306) waits until the **supplied binding’s** current head contains only Forge files, then calls `drain_tasks()`.
3. [`drain_tasks()`](/home/thorrester/Documents/GitHub/wyrd/crates/wyrd/wyrd-testing/tests/bifrost/forge/production_closeout.rs:865) exits when SQL contains zero non-terminal task rows and the observer has caught up with recorded attempts. Neither condition examines the leader’s pending compactions.

Production deliberately separates pending demand from durable task creation:

- Promotion increments the tenant-qualified leader track’s pending count in [`leader.rs:456–484`](/home/thorrester/Documents/GitHub/wyrd/crates/vala/vala-bifrost-redux/src/forge/leader.rs:456).
- A worker independently pulls at most four tables per turn, with a default **five-second pull interval**: [`worker.rs:66`](/home/thorrester/Documents/GitHub/wyrd/crates/vala/vala-bifrost-redux/src/forge/worker.rs:66), `2532–2605`.
- The leader dispatches identities from its in-memory schedule; **dispatch creates no SQL task row**: [`leader.rs:555–600`](/home/thorrester/Documents/GitHub/wyrd/crates/vala/vala-bifrost-redux/src/forge/leader.rs:555).
- The worker subsequently loads metadata and inserts the claimed task: [`worker.rs:2718–2774`](/home/thorrester/Documents/GitHub/wyrd/crates/vala/vala-bifrost-redux/src/forge/worker.rs:2718).
- If insertion encounters another active or queued attempt, the worker reports `NotStarted` without starting a task. The leader preserves the commits and makes the table due again: `worker.rs:2753–2758`, [`leader.rs:603–640`](/home/thorrester/Documents/GitHub/wyrd/crates/vala/vala-bifrost-redux/src/forge/leader.rs:603).

Consequently, SQL can be fully drained while a table still awaits a pull or re-dispatch.

**Normal sequence in the passing trace**

All line numbers below refer to `geom-pass-traced.log`.

| Step | Trace evidence |
|---|---|
| Leader starts with an empty schedule | Line 550, `00:08:42.777589` |
| Primary geometry promotion records its commit | Lines 551–553, snapshot `8359034157327086941` |
| Neighbour geometry promotion records its commit | Lines 558–560, snapshot `8613303024833261709` |
| Both requested scheduler passes return | Lines 564–565 |
| One pull dispatches both geometry tables and both audit tables | Lines 566–571, `00:08:43.450–43.491`, `returned=4`, `still_due=false` |
| Worker admits both geometry attempts | Lines 572 and 574 |
| Both geometry plans start on worker `…0002` | Lines 576–577 |
| Neighbour rewrite settles and reports success | Lines 583–585, `00:08:43.740–43.742` |
| Primary geometry rewrite settles and reports success | Lines 590–592, `00:08:43.838–43.840` |

The neighbour mapping follows the tenant ordering of the promotion sweep (`forge_tasks.rs:144`) and the trace’s tenant identities; the Forge messages themselves log only the shared table name. Its dispatched task ends in `001ea76f0f1a`.

The successful admission messages imply durable insertion already succeeded, because `admit_dispatch()` inserts the row before calling `start_claim()`. There is no separate insertion log.

**Cause**

The test equates **“all currently recorded attempts settled”** with **“all promoted tables finished maintenance.”** That equivalence is false.

A source-supported ordering is:

1. Geometry becomes due and a worker pulls it.
2. Neighbour promotion becomes due after that pull, or its dispatch is refused while its promotion still owns the table.
3. Geometry finishes before the worker’s next eligible pull.
4. `drain_tasks()` observes only terminal rows and returns.
5. The test immediately inspects the neighbour, which still has its Append head.

The passing trace avoids this ordering by pulling both tables together after both promotions settled. Under the alternative ordering, the missing step at observation is **neighbour compaction admission/task creation**, potentially preceded by a missing pull or a `NotStarted` dispatch.

**Unproven:** the failing logs cannot distinguish these paths, establish the neighbour’s leader-track state, or exclude a neighbour attempt that completed without rewriting. They contain no dispatch/admission/report evidence. Therefore the exact incident-level ordering is unresolved. No production scheduler, leader, worker, or Scribe defect is demonstrated.

The production contract explicitly describes this independent pull model in [`bifrost-design.md:1003–1023`](/home/thorrester/Documents/GitHub/wyrd/architecture/bifrost-design.md:1003).

**Fix site**

Correct the test’s synchronization in [`production_closeout.rs:1353–1367`](/home/thorrester/Documents/GitHub/wyrd/crates/wyrd/wyrd-testing/tests/bifrost/forge/production_closeout.rs:1353):

- Explicitly await the neighbour’s required rewrite state before asserting its changed snapshot.
- Where the test requires complete settlement, also await the named tables’ leader demand/in-flight dispatches, then drain durable attempts.
- Apply that stronger boundary before clock advances and before the “unchanged second pass” baseline.

Keep `drain_tasks()`’s existing meaning—its documentation already says “already-created tasks.” The same file’s [`settle_owed_compaction()`](/home/thorrester/Documents/GitHub/wyrd/crates/wyrd/wyrd-testing/tests/bifrost/forge/production_closeout.rs:739) already recognizes the missing leader-debt boundary. A normal-error variant should retain strict worker-error checks and require a live leader.

History supports this distinction:

- **Q17, `a21f335ac`:** correctly removed the geometry entry-head race; it did not add neighbour completion.
- **Q18, `4066e46c3`:** waits for concurrent **Scribe claim publication** before flush returns. It provides no Forge compaction barrier.

**Sibling callers/tests affected**

The shared helper exposure is confined to these three tests in `production_closeout.rs`; only the geometry failure is observed:

- `compaction_geometry_exact_rows_and_non_destructive_second_pass`: direct `compact`, boot drain, clock advance, neighbour assertion, and second-pass drain.
- `lazy_old_reader_survives_cross_pod_expiration_and_physical_cleanup`: `ReaderCleanupJourney::start`, `protect_during_cleanup`, and `finish` call `compact_small_table`; setup and held-reader passes call `drain_tasks`; cleanup uses `collect_exact`.
- `failed_never_published_output_is_collected_after_terminal_age`: `NeverPublishedJourney::start` calls `compact_small_table`, scheduler/drain pairs, and `prove_commit_window_retention`; `strand_one_generation` uses the analogous refusal-tolerant drain **plus** `settle_owed_compaction`, already accounting for later re-dispatch. Cleanup also uses `collect_exact`.

These siblings share the synchronization limitation; their failure is not established. The separate `LeaderJourney` helpers do not call these drains.

No files changed, and no cargo commands or tests ran.