# Persistent-data and durability domain review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-forge`
- Original implementation base, excluded: `c1508b375ba21a517f03ed6dd4d680dab4c3d12c`
- Remediation base, excluded from the remediation delta: `e8d3cca13ccb799ec6dc5c69d09da3de40bffba9`
- Immutable candidate: `7fcb45fc15ef2a43e8249af2a3dc7721fb55d517`
- Cumulative range: `c1508b375..7fcb45fc1`
- Remediation delta: `e8d3cca13..7fcb45fc1`
- Approved specification: `changes/active/forge-concurrent-planning/spec.md`, revision 11

The checkout had advanced beyond the candidate, so every source claim below was
checked with commit-scoped `git show`, `git grep`, and `git diff`; current
worktree source was not treated as the review subject.

## Boundary and authority coverage

| Boundary | Candidate source and proof inspected | Result |
|---|---|---|
| Active-cut acquisition and PostgreSQL time | `vala-sql` migration `20260910000025_oracle_reader_authority.sql:197-326`; `queries/oracle_reader_authority.rs:378-457`; `catalog/bifrost_catalog.rs:344-429`; `oracle/planner.rs:206-276`; SQL and `reader_expiry_ordering` tests | PASS. One invoker statement share-locks every table, reads the pointer and unresolved hot rows, and inserts/refreshes the active rows. PostgreSQL stamps `statement_timestamp() + remaining`; every retry derives `remaining` from the original `Instant` immediately before acquisition, and Forge alone discards rows at `abandon_after <= statement_timestamp()`. |
| Tenant and connection ownership | `BifrostTableMaintenanceAuthority`, `OracleActiveTableReads`, `TableAuthority`, `ForgeOperations`, and `ForgeTasks` call sites | PASS. Request work uses `TenantConn`; cross-tenant fenced maintenance uses `OperatorPool` with an explicit tenant binding. The narrow catalog-pointer definer remains the only request-role catalog access. |
| Continuous destructive authority | `queries/oracle_reader_authority.rs:56-286`; `forge/table_authority.rs`; `forge/expire.rs:267-323,711-790,1097-1137`; `forge/worker.rs:7999-8055,8106-8299`; `forge/orphan_gc.rs:1398-1591`; reader-race integration tests | **FAIL** only for the unbounded snapshot-expiry hold in `DATA-DUR-REREVIEW-001`. The type/borrow shape otherwise prevents a checked-then-dropped capability from authorizing an effect, and every catalog expiration or object delete takes the exact table capability. |
| Expired-object uncertain outcome and exact replay | `forge/worker.rs:7936-8300`; `queries/forge_tasks.rs:1160-1352`; `row_types/forge_tasks.rs` outcome semantics; expired-cleanup integration tests | PASS. Refused and uncertain outcomes retain `Prepared` at the same candidate index; only confirmed deletion or confirmed absence advances the cursor. Generic failure settlement no longer consumes the prepared owner. |
| Orphan cleanup durability | `forge/orphan_gc.rs` candidate scan, prepared batch, protection refresh, bounded deletion, cursor checkpoint, and recovery; orphan cleanup tests | PASS. The prepared operation and attempt remain the replay owner, the scan cursor advances only after a settled batch, and only `AttemptGeneration` objects use the orphan-age floor. |
| Hung object-store effect at the TTL | `forge/worker.rs:8190-8224`; `forge/orphan_gc.rs:1503-1586`; `hung_cleanup_delete_surrenders_table_authority_at_the_lease_bound`; `hung_orphan_delete_surrenders_table_authority_at_the_lease_bound` | PASS. Timeout surrenders table authority and retains exact uncertain/Prepared evidence. A reader admitted afterward cannot name the possibly late-deleted object: the exclusive proof excluded every older reader, and the refreshed reachability proof established that the candidate was absent from every current catalog, hot-file, ref, and unresolved-operation root. This is the explicit architecture exception for a remote effect still running at the bound, not a check/effect gap. |
| Terminal `file_list` settlement | `queries/forge_tasks.rs:1259-1352`; file-list candidate identity; terminal-row integration coverage | PASS. Confirmed deletion/absence deletes the matching compacted terminal row in the same transaction that advances the candidate; refusal, uncertainty, nonterminal rows, and unsettled promotion evidence retain it. |
| Snapshot-expiry claims and reconciliation | migration claim tables/indexes; `forge/expire.rs`; `queries/forge_operations.rs`; snapshot-expiry and reader-ordering tests | PASS except `DATA-DUR-REREVIEW-001`. Selection is explicit, claims survive uncertain acceptance, definite drift/reset removes them, and reconciliation reuses the same operation identity. |
| Planning-demand retirement and migration history | migration `20261003000100_drop_forge_planning_demands.sql`; repository consumers of the removed relation and scheduler cursor | PASS. The forward migration drops the table and scheduler cursor after production callers were removed; historical creation/alter migrations remain correctly immutable. The surviving `forge_worker_claim_state.last_tenant_id` belongs to fair retryable task claiming, not the retired planner. |
| Promotion and published-object durability | `scribe_promotion.rs`, `file_list.rs`, operation evidence, hot-object reconciliation, protection roots, and Forge journeys | PASS. Hot publication debt remains durable; ambiguous publication retains its operation identity and exact object evidence; cleanup treats unresolved or unpromoted hot objects as roots. |
| Pinned compaction fork and noncommitting seam | candidate `Cargo.toml:230-243`, `Cargo.lock:4745-4748`; task's exact `74bdc45`/`6773e19`/`ef97aea`/`380a4d0` comparison; local fork objects at `380a4d0717e1786b95c4aa9f257579af496b3c8c` | PASS. The shipped pin is literal and present locally. The narrowing deletes the unused identity planner, dependency-universe test, and unused telemetry machinery; retained noncommitting, governed-context, cancellation/drain, and selection-report code has named Forge consumers. No additional durability deviation beyond the approved Wyrd recovery/deletion-protection/governed-spill differences was found. |

## Review Findings

### Critical

None.

### Important

- **DATA-DUR-REREVIEW-001 — INCORRECT — `crates/vala/vala-bifrost-redux/src/forge/expire.rs:267-296,743-784,1119-1137`: snapshot expiry does not actually bound the lifetime of its exclusive table authority by `lease_ttl`.** The first path acquires `ExclusiveTableAuthority`, then awaits a separate SQL preparation, a corroborating catalog load, the Iceberg commit, and the authority transaction commit; recovery repeats the same pattern. The catalog load and catalog commit have individual shorter timeouts, but the complete authority-owned scope has no lease-TTL deadline, and the SQL preparation/fence/commit awaits have no applicable statement or lock timeout. A PostgreSQL stall or lock wait can therefore keep the authority row locked after the Forge lease TTL, making every Oracle cut for that table wait until its own query deadline indefinitely across successive queries, contrary to `architecture/bifrost-design.md:875-881` and the remediation's capped-authority requirement. Bound the existing snapshot-expiry authority-owned scope with one deadline derived when the authority is acquired, release/rollback the authority when that deadline is reached, and retain the existing Prepared/uncertain operation identity whenever submission may have occurred; do not add a second lock or coordination protocol. Prove closure with a short-lease test that pauses preparation or a post-preparation await while the exclusive authority is live and shows the authority is surrendered by the lease bound while the exact prepared operation remains safely replayable.

### Suggestions

None.

## Prior-finding closure

| Finding | Data/durability disposition |
|---|---|
| `FIND-TASK-003-1` | CLOSED. `CleanupRetained` bypasses generic retry/terminal settlement and returns the same prepared task/attempt/candidate to reconciliation. |
| `FIND-TASK-005-R1-2` | CLOSED. Initial acquisition and reacquisition derive a shrinking duration from one immutable deadline immediately before the PostgreSQL statement. |
| `FIND-TASK-005-R1-3` | PARTIALLY CLOSED. The owned capability closes the reader-check/effect gap for all three destructive paths, and the object-delete timeout exception is safe; snapshot expiry still fails the required lease-TTL bound (`DATA-DUR-REREVIEW-001`). |
| `FIND-TASK-005-R1-5` | CLOSED. Production Redux no longer embeds the registry SQL; `TableAuthority` delegates to the typed `vala-sql` catalog owner. |

## Verification limits

- I did not rerun the recorded full gate or database journeys; the review used their committed evidence and independently inspected the immutable candidate source and focused tests.
- The hung-delete tests exercise timeout, retained evidence, and authority release. They do not emulate a remote service completing after its Rust future is dropped; candidate reachability and root ownership nevertheless establish why such a late completion cannot delete an object selected by a newly admitted cut.
- No live cloud object store was used. The remaining finding is source-reachable without one: an unbounded PostgreSQL await occurs after snapshot expiry has acquired the authority row.

## Overall result

**FAIL** — the persistent-data protocols close the prior prepared-cleanup,
deadline, continuous-authority, SQL-ownership, migration, file-list, and fork
findings, but snapshot expiry still lacks the required lease-TTL bound around
the complete exclusive-authority lifetime.
