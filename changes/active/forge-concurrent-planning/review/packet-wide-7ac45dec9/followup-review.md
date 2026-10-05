# Focused follow-up review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-forge`
- Base, excluded: `c1508b375`
- Candidate: `7ac45dec99535c881b7a936c66623044f15d8823`
- Range: `c1508b375..7ac45dec9`
- Approved authority: `changes/active/forge-concurrent-planning/spec.md`, revision 11
- Pinned RisingWave checkout: `e23ddf952c3e6ebc03cc254789e84d1179cfacae`

The candidate remained at the named commit throughout this pass. This review
investigated only the four conflicts assigned by the orchestrator.

## Paths and authority inspected

- Review inputs: `navigation-map.md`, both task-review reports,
  `standards-review.md`, `system-review.md`, and
  `domain-review-data-durability.md` in this review directory.
- Active-read ownership: `oracle/planner.rs`, `oracle/mod.rs`,
  `oracle/query_stream.rs`, `oracle/admission.rs`, `oracle/analytical.rs`,
  `oracle/analytical_supervisor.rs`, and the dropped-caller journey in
  `wyrd-testing/tests/bifrost/oracle/distributed.rs`.
- Destructive authority: migration
  `20260910000025_oracle_reader_authority.sql`,
  `vala-sql/src/queries/{forge_operations,forge_tasks}.rs`, and
  `forge/{expire,worker,orphan_gc,table_authority,gc}.rs`.
- TASK-002 comparison: `tasks/README.md`,
  `tasks/TASK-002-pull-and-worker-results.md`,
  `evidence/TASK-002-fork-review.md`, workspace `Cargo.toml`/`Cargo.lock`, the
  local `iceberg-compaction` checkout, and the pinned RisingWave checkout.
- Verification authority: `AGENTS.md` sections 11-12,
  `architecture/references/languages/testing-workflows.md`, the task packet's
  verification sections, `mise.toml`, and the exact candidate diff check.

## Conflict 1 — dropped active-read claim versus analytical descendant drain

**Resolution: RESOLVED. `INV-REV-001` is confirmed; the system review's safe-drop
assessment is rejected.**

Normal terminal settlement is correctly ordered. `query_stream.rs:625-657`
joins distributed children, explicitly settles the Analytical graph, and only
then awaits `ActiveReadClaim::release`. The conflict is the abandoned-stream
path, which does not run that sequence.

On caller drop, `ActiveReadClaim::drop` independently spawns the SQL deletion
(`oracle/planner.rs:138-152`). The Analytical caller-side owner independently
signals cancellation from `AnalyticalGraphSignals::drop`
(`oracle/analytical.rs:2474-2483`). That signal merely wakes the supervisor-owned
lifecycle task. The task still must run `AnalyticalGraphLifecycle::settle`,
join the retained attempt and all its drivers, close exchanges, drop follower
grants, drain envelope children, and release the graph
(`oracle/analytical.rs:2693-2789`). No ownership edge makes the spawned
active-read deletion wait for that lifecycle settlement.

The lower-level drop path reinforces the same distinction:
`AnalyticalAttemptGuard::finish` awaits every retained driver
(`analytical_supervisor.rs:1200-1247`), while its `Drop` path calls
`abandon_attempt`, which cancels and aborts without joining
(`analytical_supervisor.rs:1342-1370,1549-1556`). Cancellation or abort is not
proof that a task has already stopped; the lifecycle's explicit joins are the
repository's own proof boundary.

The journey does not close the race. In
`distributed.rs:4740-4757`, the dropped-caller case releases the paused
follower before waiting for ownership and active rows to disappear. It never
holds a descendant stopped-but-not-joined while asserting that the active row
remains.

This is reachable and violates TASK-005-R1's material stop condition at lines
446-447: no reader may perform metadata or file IO after the cut-and-claim
owner releases. Revision 11 requires drop to remain non-blocking; it does not
require the durable claim itself to be deleted before the existing background
Analytical settlement has drained. The smallest correction boundary is to
transfer the claim to the existing supervisor-owned background settlement on
abandonment, so that cancellation and joins complete before row release. The
interactive/no-descendant case may retain its direct non-blocking drop release.

### Follow-up finding FUP-001 — INCORRECT

- **Violated obligation:** REQ-014, INV-005/009, TASK-005-R1 Scenario 2 and its
  explicit no-IO-after-release stop condition.
- **Observable consequence:** Forge can observe no active row and destructively
  expire or delete a selected object while an abandoned Analytical descendant
  is still stopping.
- **Closure proof:** pause a follower after the leader stream is dropped; keep
  it paused while proving the active row remains and all destructive paths
  refuse; then allow/join follower shutdown and observe row release.

## Conflict 2 — authority transaction lifetime versus external destructive effect

**Resolution: RESOLVED. `DATA-DUR-001` is confirmed; the reviews that accepted
the active-read gate established only pre-effect checks, not the required
ordering through the effect.**

Oracle acquisition takes the maintenance-authority rows `FOR SHARE`, reads the
catalog pointers, and inserts active rows in the same statement/transaction
(`20260910000025_oracle_reader_authority.sql:275-305`). Forge takes the same
row `FOR UPDATE` and refuses active readers (`forge_operations.rs:1519-1574`).
That mechanism gives one ordering only while the Forge transaction remains
open.

Every destructive path closes that transaction before the external effect:

- Snapshot expiration commits preparation and releases the authority lock,
  then reloads the table and submits the Iceberg commit. The source documents
  the gap at `forge/expire.rs:195-197`; the prepare/reload/commit sequence is
  `expire.rs:240-257`, and the external commit is submitted from
  `complete_expiry` at `expire.rs:674-724`.
- Expired cleanup commits its candidate preparation after the active-reader
  check (`forge_tasks.rs:1191-1252`). Its later refreshed protection load also
  commits its tenant transaction before returning. `worker.rs:7842-7915`
  then stats and validates the candidate without an authority lock, and
  `worker.rs:7934-7964` submits the delete.
- Orphan cleanup loads active-reader protection and commits that transaction
  (`forge/orphan_gc.rs:1123-1158`), then uses the cached protection snapshot
  while statting and deleting candidates (`orphan_gc.rs:1395-1453`, called
  from `delete_gc_batch` at `:1508-1521`). The Forge table lease does not close
  this race because Oracle acquisition does not participate in that lease.

Therefore a destructive worker can pass its final active-reader check, release
the exclusive authority lock, and pause. Oracle can then acquire the shared
lock, commit a cut and active row, and begin materialization before the older
worker performs its catalog mutation or object deletion. The existing tests
exercise reader-before-prepare/refusal, not reader-after-check/before-effect.

The approved task is explicit: Scenario 1 requires that a destructive
transaction either observe the committed reader and refuse or complete first,
after which the reader observes the later pointer; Scenario 3 says Forge cannot
prepare **or commit** expiration or cleanup while a reader is active. A short
SQL lock cannot be held across object IO under the existing architecture, so
the correction must use the existing maintenance-authority owner to make a
cut racing already-authorized destructive work wait/refuse and reacquire the
post-effect pointer, or otherwise retain equivalent exclusive authority through
effect completion. A new reader epoch, IO gate, or per-query connection is not
authorized.

### Follow-up finding FUP-002 — INCORRECT

- **Violated obligation:** REQ-007/014, INV-005/006/009, AC-006/009, and
  TASK-005-R1 Scenarios 1 and 3.
- **Observable consequence:** snapshot expiration can commit while a new reader
  owns the pre-expiry cut; expired and orphan cleanup can delete after a new
  active row committed in the post-check window.
- **Closure proof:** deterministically pause each destructive path after its
  last active-reader check and before the external effect, race cut acquisition,
  and prove one approved ordering: either the reader wins and destruction
  refuses, or destruction wins and the reader receives the resulting pointer.

## Conflict 3 — TASK-002 pinned fork comparison

**Resolution: RESOLVED. `BEH-002` is confirmed; TASK-002's runtime behavior may
be substantially tested, but its mandatory comparison acceptance artifact is
incomplete.**

`tasks/README.md:45-55` requires every task comparison row to contain the exact
RisingWave link, implemented Forge source, focused executed test/result, or an
approved Wyrd difference; an empty row blocks completion. TASK-002 strengthens
that requirement at lines 118-145 for six specific nimtable/fork seams: Full,
SmallFiles/FilesWithDelete, Auto, noncommitting execution, governor/spill, and
cancellation/loose outputs. It also requires the final disposition and live
consumer/failure test for every retained fork-only module.

The implementation report at `TASK-002-pull-and-worker-results.md:340-367`
contains a six-row **scheduler/worker** RisingWave comparison, not the mandatory
six-row nimtable/fork comparison. The mandatory table at lines 129-136 still
contains only the original source-comparison instructions in its right-hand
column.

`evidence/TASK-002-fork-review.md` is a useful pre-implementation analysis of
`74bdc45` versus `6773e19`, but it is not the required completed report: it
contains recommendations and explicit outstanding statements such as “TASK-002
still needs the real-rewrite test,” does not supply a focused executed result
for every seam, and does not reconcile the final retained/deleted fork modules.
The reviewed workspace now pins `iceberg-compaction-core` at
`ef97aea028aac502f50914ac37e74edc836cfbcc` (`Cargo.toml:243`,
`Cargo.lock:4745-4748`). That revision descends from `6773e19` but adds later
dependency-pin commits; the report never names it as the selected final
implementation revision.

The smallest correction is evidence-only if source validation shows the code
already satisfies every row: complete the mandatory table with the exact
`74bdc45`, `6773e19`, final `ef97aea`, Forge consumers, executed focused tests,
and retained/deleted module disposition. If any row lacks its required test or
live consumer, the implementation must close that specific row rather than
assert parity from the general worker journey.

### Follow-up finding FUP-003 — MISSING

- **Violated obligation:** TASK-002 mandatory pinned comparison and the packet
  index's empty-row completion block.
- **Observable consequence:** review cannot establish that the final fork keeps
  only approved Wyrd differences or that every retained fork-only behavior has
  a live, failure-tested consumer.
- **Closure proof:** a completed non-empty row for each mandatory seam, checked
  against the three exact revisions and backed by the named executed test
  result.

## Conflict 4 — diff check and aggregate verification authority

**Resolution: RESOLVED. `STD-003` and `STD-004` are confirmed.**

The exact command claimed green by the implementer was rerun:

```text
git diff --check c1508b375..7ac45dec9
```

It exits `2`, not `0`, and reports:

```text
changes/active/forge-concurrent-planning/revision/TASK-005-R1-implementation-reference.md:464: new blank line at EOF.
changes/active/forge-concurrent-planning/tasks/TASK-003-maintenance-and-removal.md:307: new blank line at EOF.
```

Those blank lines are present in the immutable candidate blobs. The user-supplied
green claim is therefore stale or was produced by a different command/tree;
the standards review's direct candidate result is reproducible.

`AGENTS.md:507-510` requires `mise run gate` when a change is intentionally
broad, changes shared CI/build/test infrastructure, prepares a release, or
crosses several ownership boundaries without a complete capability gate. This
candidate is intentionally broad (approximately 286 files across SQL, Vala,
server, contracts, three SDK surfaces, docs, tests, and generated artifacts)
and changes shared test infrastructure in `mise.toml` by adding the Forge
capacity benchmark task. Either independent condition selects the aggregate.
`verify:bifrost` is the complete scoped Bifrost gate, but it does not override
the separate intentionally-broad/shared-infrastructure clauses. The packet's
instruction that TASK-003 runs `verify:bifrost` once avoids redundant component
reruns; it does not waive the repository-wide aggregate required by the higher
repository authority.

No `mise run gate` result is recorded in the packet or supplied verification.
Per `AGENTS.md`, it should be run once as the final aggregate on the corrected
immutable candidate, without redundantly rerunning its component lanes; only a
specialized lane proven outside `gate` should remain separate.

## New proposed findings

| ID | Classification | Discovery relationship | Result |
|---|---|---|---|
| `FUP-001` | `INCORRECT` | Confirms and narrows `INV-REV-001`; rejects the contrary safe-drop assessment | Retain |
| `FUP-002` | `INCORRECT` | Confirms `DATA-DUR-001`; rejects acceptance based only on pre-effect gating | Retain |
| `FUP-003` | `MISSING` | Confirms `BEH-002`; explains why the separate analysis file does not close the mandatory report | Retain |

`STD-003` and `STD-004` need no new follow-up IDs; their original standards
findings are directly confirmed.

## Final follow-up result

**RESOLVED.** All four assigned uncertainties resolve from approved authority
and current source. No remaining disagreement requires another discovery pass.
