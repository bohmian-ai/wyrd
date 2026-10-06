# Persistent-data and durability domain review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-forge`
- Base, excluded: `c1508b375`
- Immutable candidate: `7ac45dec99535c881b7a936c66623044f15d8823`
- Range: `c1508b375..7ac45dec9`
- Approved specification: `changes/active/forge-concurrent-planning/spec.md`, revision 11
- Tasks reviewed: TASK-001 through TASK-005-R1. TASK-006 has no persistent-data or analytical-data-path effect.
- Pinned comparison: RisingWave `e23ddf952c3e6ebc03cc254789e84d1179cfacae`

The candidate remained at the immutable commit above while this review was performed.

## Boundary and authority coverage

| Boundary | Authority and source inspected | Result |
|---|---|---|
| Leader and volatile scheduling state | Spec REQ-001/003/006/008/009; `forge/{leader,leadership,scheduler,worker}.rs`; `vala-sql/queries/{forge_leader,forge_tasks}.rs`; the pinned RisingWave `schedule.rs` pull/timeout path | PASS |
| Promotion publication durability | Spec REQ-002/006; `forge/scribe_promotion.rs`; `queries/file_list.rs`; `queries/forge_operations.rs`; promotion and production-route tests | PASS |
| Planning-demand retirement | Spec REQ-010 and deletion map; migration `20261003000100_drop_forge_planning_demands.sql`; all repository consumers of `forge_planning_demands`, scheduler `last_tenant_id`, and planning-demand APIs | PASS |
| Snapshot expiration and cleanup handoff | Spec REQ-007, INV-006, AC-006; `forge/{gc,expire,expiry_policy,orphan_gc,protection_roots,table_authority}.rs`; `queries/{forge_operations,forge_tasks}.rs`; migrations and Tier-2 Forge tests | **FAIL** (`DATA-DUR-001`) |
| Active-read row lifecycle and PostgreSQL time | Spec REQ-014, INV-005/009, AC-009; migration `20260910000025_oracle_reader_authority.sql`; `queries/oracle_reader_authority.rs`; `oracle/{planner,mod,query_stream}.rs`; SQL and Oracle journey tests | **FAIL** (`DATA-DUR-001`, `DATA-DUR-002`) |
| Terminal `file_list` retirement | Spec revision 10 and REQ-007/014; `ForgeTasks::settle_expired_cleanup_candidate`; expired-cleanup and held-query tests | PASS subject to `DATA-DUR-001`: deletion/confirmed absence and terminal-row retirement share one SQL transaction, and nonterminal rows do not match the deletion predicate |
| Never-published orphan cleanup | Spec REQ-007; `forge/orphan_gc.rs`; orphan and production-route tests | PASS for roots, replay identity, path binding, and the `AttemptGeneration`-only age floor; its active-reader ordering is part of `DATA-DUR-001` |
| Iceberg field IDs and filtering | Spec REQ-015/016; `catalog/bifrost_catalog.rs`; schema fingerprinting; Scribe Parquet writer; Oracle typed predicates/Bloom/page bounds; promoted/rewritten journey matrix | PASS |
| Crash, replay, and uncertain effects | Spec failure invariants; promotion generation identities, operation rows, snapshot-expiration claims, cleanup cursors/prepared candidates, Scribe claim recovery tests and evidence | PASS for the reviewed durable identities and idempotent settlement paths |
| RisingWave comparison | Pinned `gc.rs` maintenance order/expiry-then-cleanup and `schedule.rs` oldest-due/timeout selection compared with Wyrd owners and task evidence | PASS. The comparison is non-empty; observed deviations are the approved Wyrd hot-publication recovery, Oracle/hot-object deletion protection, and central-governor/spill ownership. `DATA-DUR-001` is a failure to implement the approved Oracle protection deviation, not an additional approved difference. |

## Material findings

### DATA-DUR-001 — INCORRECT: the maintenance-authority lock does not serialize a newly acquired cut with the destructive effect

**Violated obligation.** Spec REQ-014 and INV-005/009 require acquisition and destructive maintenance to have one durable ordering: Forge must refuse expiration and object cleanup while an active table read exists, and a reader that loses the ordering race must observe the later catalog pointer. TASK-005-R1 Scenario 1 says the destructive transaction either observes the committed reader and refuses, or completes first; Scenario 3 and its acceptance criteria require that Forge cannot prepare **or commit** snapshot expiration or cleanup while a read is active.

**Exact locations.**

- `crates/vala/vala-sql/migrations/20260910000025_oracle_reader_authority.sql:275-305` share-locks the authority row, reads the pointer, and inserts the active-read row.
- `crates/vala/vala-sql/src/queries/forge_operations.rs:908-958` exclusively locks the authority row and checks readers only in `prepare_snapshot_expiration`, then commits that preparation transaction.
- `crates/vala/vala-bifrost-redux/src/forge/expire.rs:195-197` explicitly documents that the authority row is released before the second catalog read; `expire.rs:240-257` reloads metadata and submits the expiry after that release; `expire.rs:674-724` performs the external Iceberg commit without another active-read serialization check.
- Expired cleanup has the same shape: `crates/vala/vala-sql/src/queries/forge_tasks.rs:1191-1252` commits the reader check and candidate preparation; `crates/vala/vala-bifrost-redux/src/forge/worker.rs:7842-7964` reloads protection, commits its SQL transaction, and only then stats and deletes the object.
- Orphan cleanup likewise commits its active-read check at `crates/vala/vala-bifrost-redux/src/forge/orphan_gc.rs:1041-1156` and later deletes from the cached protection snapshot at `orphan_gc.rs:1395-1453`.

**Evidence and reachability.** A snapshot-expiration worker can commit Prepared while no reader exists, release the exclusive authority lock, and pause before `commit_once`. Oracle can then run `oracle_acquire_table_cut`, take the now-free share lock, read the still-current pre-expiry pointer, commit its active-read row, and begin materialization. The already-prepared Forge worker never checks active reads again and commits the expiration while that query is active. Cleanup has an analogous post-check/pre-delete window. The table lease does not close this race because Oracle does not acquire it. Snapshot-expiration claims do not close it because `oracle_acquire_table_cut` does not read or wait on those claims.

The recorded `last_table_reader_controls_destructive_cleanup` test starts both readers before maintenance (`reader_expiry_ordering.rs:40-47`) and therefore proves only reader-before-prepare. It never exercises reader-after-prepare/before-effect. The held-query journey has the same ordering direction. Green lanes consequently do not falsify this path.

**Observable consequence.** Forge can commit snapshot expiration while an Oracle query owns a cut selected immediately beforehand, contrary to the approved lifetime and ordering contract. Physical files are likely retained by the later cleanup check, so this need not immediately lose rows, but the catalog mutation and reader claim coexist in a state the specification forbids. For cleanup, a read can also become active after the last check and before deletion, so the implementation does not satisfy the table-wide refusal promised by REQ-014/AC-009.

**Required testable correction.** Close the post-check/pre-effect window at the existing table-maintenance authority boundary. A cut acquisition that races an already-authorized destructive operation must not expose the old cut: it must wait/refuse-and-retry until that operation resolves and then select the resulting pointer, or the destructive owner must retain equivalent exclusive authority through its effect. Apply the same ordering to snapshot expiration and both object-cleanup paths without restoring reader epochs, ancestry frontiers, an IO gate, or a per-query connection. Add deterministic interleaving proofs that pause each destructive path after its current reader check but before its catalog/object effect, then acquire a cut and prove exactly the approved ordering. The snapshot case must assert which metadata pointer the reader receives; cleanup cases must assert no deletion occurs while a committed active row exists.

### DATA-DUR-002 — INCORRECT: bounded metadata reacquisition refreshes abandonment past the query's actual deadline

**Violated obligation.** Spec revision 11 and REQ-014 require each active row's `abandon_after` to be PostgreSQL `statement_timestamp()` plus the query's **remaining** deadline, so a crashed Oracle blocks cleanup only until that query's own deadline. Explicit deadlines remain uncapped, but they are not renewable extensions.

**Exact locations.**

- `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:1887-1919` computes one `duration` from the absolute wall deadline and stores that duration in `ActiveReadOwner` before planning.
- `crates/vala/vala-bifrost-redux/src/oracle/planner.rs:180-219` reuses the same copied owner for the permitted metadata-`NotFound` reacquisition after arbitrary metadata/manifest IO.
- `crates/vala/vala-sql/migrations/20260910000025_oracle_reader_authority.sql:293-305` sets `abandon_after = statement_timestamp() + p_deadline_ms` and overwrites the existing row on reacquisition.

**Evidence and reachability.** If the first acquisition succeeds, metadata materialization consumes time, and a selected metadata document returns `NotFound`, the second acquisition writes the original pre-materialization duration from a later PostgreSQL statement timestamp. Its `abandon_after` is therefore later than the immutable query deadline by the elapsed first-attempt time. If the process then crashes or the best-effort release fails, Forge retains the row beyond the only deadline the query was allowed to run.

The one-reacquire journey proves statement count and bounded retry, but it does not delay the first materialization and compare the refreshed `abandon_after` with the original absolute query deadline. The SQL test checks `abandon_after - acquired_at` for a single acquisition, which cannot expose the renewal.

**Observable consequence.** A crash on the reacquisition path can block snapshot expiration and cleanup longer than the query's own deadline. This violates the intended exact crash-recovery bound and can make the extra protection interval approach the original deadline again.

**Required testable correction.** Derive the duration passed to every acquisition from the same immutable absolute query deadline immediately before that acquisition. Reject when no positive duration remains. Add a deterministic reacquisition test that advances/holds the first metadata attempt, triggers the one allowed `NotFound` reacquire, and proves the refreshed PostgreSQL `abandon_after` remains at the original query deadline (within representation precision) rather than extending from the second statement time.

## Confirmed durable behavior

- The forward migration drops `vala.forge_planning_demands` and only the obsolete scheduler cursor column. The surviving `forge_worker_claim_state.last_tenant_id` and `forge_fair_claim.sql` implement worker task-claim fairness, not per-table planning demand; they remain real consumers.
- The active-read migration is unshipped and was edited in place as authorized. RLS is forced on the authority and active-read tables, the cut function is `SECURITY INVOKER`, and the narrow pointer function derives the physical namespace from `wyrd.current_tenant()`. No grant of `iceberg_catalog` to `wyrd_app` was found.
- Snapshot-expiration selection uses explicit IDs only; current snapshot and explicit refs are excluded before the fork action, and the fork call sets `explicit_ids_only(true)`.
- Expired-file cleanup persists one immutable ordered candidate vector, one prepared cursor, and advances only on confirmed deletion or confirmed absence. The same settlement transaction advances the cursor and deletes a matching terminal promoted `file_list` row.
- Promotion records Prepared before catalog mutation, carries operation identity into snapshot properties, and settles the operation projection and exact `file_list` rows in one tenant transaction. Reset generations use deterministic distinct operation IDs and retained catalog evidence prevents a definitely-landed operation from being reset.
- Iceberg owns registered physical field IDs; Scribe stamps the registered schema IDs, Forge validates file/table identity, and the Oracle journeys separately exercise hot, promoted, rewritten, binary Bloom/page-bound, and request-ID min/max behavior.
- The pinned RisingWave maintenance order is preserved: manifest rewrite before snapshot expiration, per-table failure isolation, and cleanup after the expiration commit. Wyrd's removal of age-based snapshot eligibility is the approved Oracle-protection deviation.

## Verification assessment and limits

The candidate records successful `mise run verify:bifrost`, `mise run test:principals:integration`, format, lint, and diff checks. Focused source and test inspection confirmed that those lanes cover the healthy reader-before-maintenance direction, crash/replay identities, terminal `file_list` retirement, field IDs, and filtering. I did not rerun the full packet-wide lanes because both findings are reachable orderings contradicted directly by the production source and neither has a test that exercises its required interleaving. No production source was modified.

The review did not independently rebuild the pinned Iceberg forks; it inspected the candidate's dependency identities, the explicit-ID call site, task-recorded fork evidence, and the production/journey consumers. That limit does not affect either finding.

## Overall result

**FAIL**

The persistent-data domain cannot pass while `DATA-DUR-001` permits a committed active cut to appear between Forge's last authority check and its destructive effect, and `DATA-DUR-002` can extend a crashed reader's durable protection beyond the query's actual deadline.
