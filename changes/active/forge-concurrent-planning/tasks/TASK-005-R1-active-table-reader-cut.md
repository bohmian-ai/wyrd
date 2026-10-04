---
id: TASK-005-R1
kind: implementation
status: ready
spec: SPEC-forge-concurrent-planning
spec_revision: 10
requirements: [REQ-002, REQ-007, REQ-014, INV-005, INV-006, INV-008, INV-009, AC-002, AC-006, AC-009]
depends_on: [TASK-001]
parent_task: TASK-005
completes: TASK-005 Scenario 4
supersedes: TASK-005 Scenario 4 SPEC_REVISION_REQUIRED stop record
---

# Acquire one protected Oracle cut and expire snapshots after the last reader

## Outcome and Value

Oracle acquires every referenced table's tenant-scoped catalog pointer, hot
file candidates, and active table-read ownership in one SQL statement per
attempt. The returned cut cannot exist without that ownership. Forge expires a
replaced snapshot at the next maintenance opportunity after the table's final
reader releases, subject only to current snapshots, explicit refs, active
compaction inputs, unresolved publication state, and unsettled promotions.

This removes reader epochs, ancestry frontiers, IO gates, snapshot-age waiting,
retention-derived query limits, and reader-capacity preallocation while keeping
hot and Iceberg reads safe across local and analytical execution.

TASK-005 Scenarios 0–3 are already implemented and evidenced. This task is the
complete replacement for its stopped Scenario 4; TASK-005 closes only after
this task passes.

The normative cross-owner implementation decisions are restored in
[`TASK-005-R1-implementation-reference.md`](../revision/TASK-005-R1-implementation-reference.md).
Implementation must satisfy that reference; it is not optional guidance.

## Owners, Scope, Consumers, and Prohibited Changes

- `vala-sql` owns the tenant-scoped active table-read rows and the atomic cut
  acquisition/release operations. There is exactly one durable row for each
  active query/table pair. It carries the existing durable query identity,
  table identity, exact Oracle node fence, and PostgreSQL-computed expiry at
  the query's own deadline. It carries no snapshot ancestry, reader epoch, revision state machine,
  capacity slot, or preallocated frontier.
- `TenantConn` RLS is the tenant boundary for Bifrost registration, hot rows,
  and active reads. Do not duplicate tenant predicates on those tables. The
  narrow security-definer catalog-pointer lookup retains its explicit physical
  namespace restriction because `iceberg_catalog` has no RLS.
- Acquisition consumes the planner's existing distinct canonical table
  references and preserves their order. Its grouped result reuses the existing
  table-authority identity and hot-file projection: exactly one identity and
  pointer per requested table plus zero or more hot rows. The private SQL row
  represents every left-joined hot column as nullable; all-null means an empty
  hot set, a partially-null hot row or conflicting identity is an internal
  invariant error, and a missing registration or pointer uses the existing
  catalog-not-found error. Callers cannot supply independently sized SQL
  arrays.
- Oracle owns one opaque value containing both the materialized table cuts and
  their committed active-read ownership. Its fields are private. Planning,
  physical construction, local execution, analytical execution, streaming,
  cancellation, and terminal settlement may move the owner but cannot extract
  a usable cut from it or detach its claim.
- Normal terminal settlement releases active reads only after local work and
  every analytical descendant have stopped. A dropped owner spawns a
  non-blocking release of its rows (spec revision 11). A crashed owner's rows
  stay protective until the query's own deadline: Oracle binds the remaining
  deadline duration at acquisition and PostgreSQL derives the expiry from
  `statement_timestamp()`. Forge discards a row once PostgreSQL time passes it;
  no fence liveness participates. This does not change either class's
  runtime; no Rust wall clock authorizes cleanup.
- Forge owns destructive maintenance. Snapshot expiration and object cleanup
  take the existing table maintenance authority and refuse while an active
  read exists. Promotion and non-destructive catalog commits may proceed.
  Rewrite and expiration also refuse while a promotion operation is unsettled.
- The server resolves the Oracle runtime configuration once. Local Oracle
  entry and public forwarding consume its configured default query deadline
  when the request omits one. Neither path reconstructs a library default, and
  no maximum deadline is added.
- Physical deletion eligibility is also the sole authorization to delete the
  matching terminal `file_list` row. After expired-object cleanup deletes the
  object or confirms it already absent, the existing completion transaction
  deletes that row and completes the cleanup candidate. Nonterminal and
  unsettled-promotion rows remain. There is no separate metadata check, row
  TTL, archive, or sweeper.
- Expiration names explicit snapshot IDs. The Iceberg fork's independent
  age-based selector is disabled for the commit. The current snapshot and
  every other authoritative root remain protected.
- The common successful Oracle path performs one tenant-scoped acquisition
  statement for all referenced tables. One complete retry is allowed only
  when the selected immutable metadata document returns `NotFound` after a
  catalog move. No metadata cache, cache capacity, singleflight owner, new
  timeout, query-duration cap, retention/configuration agreement, clock-skew
  margin, barrier table, or compatibility field is in scope.
- Wyrd has not shipped. Edit the unshipped reader-authority migration directly
  and remove obsolete wire fields and generated projections without aliases.
- Preserve authorization, audit staging, exact hot/Iceberg reconciliation,
  active compaction inputs, unresolved publication roots, tenant isolation,
  cancellation, and terminal framing.

## Approach

1. Replace the reader epoch, table frontier, and frontier-member schema with
   the single active query/table read relation. Keep the existing per-table
   maintenance authority and snapshot-expiration claim relation.
2. Make the tenant-scoped cut acquisition statement serialize with destructive
   maintenance, read all pointers and hot candidates, and commit active reads
   before returning any cut identity. Add idempotent terminal release and
   PostgreSQL-time expiry at the query's own deadline.
3. Carry the cut and its active-read ownership as one private Oracle value from
   acquisition through complete local or analytical settlement. Remove the IO
   permit, gated storage wrapper, pointer revalidation loop, epoch lifecycle,
   and follower reader-cut wire projection.
4. Change Forge selection to expire replaced, otherwise-unreferenced snapshots
   without an age wait. Block expiration and cleanup under active table reads,
   and reuse the existing operation state plus table maintenance authority to
   block rewrite and expiration during an unsettled promotion.
5. Keep metadata reads direct and concurrent across tables. Reacquire the
   entire cut once only for metadata `NotFound`; propagate the second failure.
6. Use the single boot-resolved Oracle default deadline in local and forwarded
   entry, and remove request-path reconstruction of a library default.
7. Delete matching terminal `file_list` metadata in the existing physical
   cleanup completion transaction, then remove the obsolete retention,
   reader-epoch, ancestry, permit, wire, tests, and documentation while
   retaining real cleanup roots and the one-statement evidence.

## Ordered Implementation Scenarios

### Scenario 1 — Cut acquisition commits active reads atomically

**Behavior.** One tenant-scoped statement locks every requested table's
maintenance authority, returns its registered identity, catalog pointer, and
unresolved hot candidates, and records one active read for the query/table and
exact Oracle node fence before exposing the result. A competing destructive
transaction has one ordering: it either observes the committed reader and
refuses, or completes first and the reader observes the later catalog pointer.
Empty hot sets remain valid, duplicate requested tables create one claim,
missing registrations or pointers fail with the existing catalog error, and
another tenant's pointer or claim is never visible.

**RED.** Add
`active_table_read_claim_is_atomic_tenant_scoped_and_postgres_expired` to the
existing `pg_oracle_membership` target. It fails because the current operation
publishes epoch/frontier state separately from catalog and hot-row selection.

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p vala-sql --test pg_oracle_membership --test-threads=1 -E "test(=active_table_read_claim_is_atomic_tenant_scoped_and_postgres_expired)"'
```

**GREEN.** Replace the unshipped reader schema and SQL owner with the single
active-read relation and atomic acquisition/release behavior. PostgreSQL sets
and evaluates the expiry from its own `statement_timestamp()` plus the
query's bound remaining deadline. A claim is removable once PostgreSQL time
passes that expiry, whatever its Oracle fence state. The caller retains
transaction ownership.

**REFACTOR.** Delete epoch, frontier, ancestry, and reclamation code once the
focused SQL test remains green. Keep one concrete SQL owner and reuse existing
row/domain types where they fit.

### Scenario 2 — A usable cut cannot outlive or detach from its claim

**Behavior.** Oracle receives an opaque cut-and-claim owner. Every local scan,
analytical graph, returned stream, cancellation path, first-batch failure, and
terminal frame retains that owner until all descendants have settled. Normal
terminal completion releases its active rows exactly once. A dropped owner
releases its rows without blocking the drop.

**RED.** Add
`distributed::held_cut_owns_active_reads_until_all_descendants_settle` to the
Oracle journey. It fails because the current protected cut can be separated
into raw cuts, a reader guard, and an IO permit, and because the replacement
active-read relation does not exist.

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=distributed::held_cut_owns_active_reads_until_all_descendants_settle)"'
```

**GREEN.** Make the private Oracle ownership graph carry one inseparable
cut-and-claim value through execution and terminal settlement. Release only
after the existing local and distributed join points. A dropped owner spawns
its release; only a crashed owner's rows wait for their deadline expiry.

**REFACTOR.** Collapse redundant terminal wrappers after the opaque owner is
the only way to retain a readable cut. Do not add an IO authorization layer.

### Scenario 3 — Forge deletes after the last reader, without an age wait

**Behavior.** While any active table-read row exists, Forge cannot prepare or
commit snapshot expiration, expired-object cleanup, or orphan cleanup for that
table. After the final reader releases, the next maintenance pass may expire
every replaced snapshot that has no other authoritative root, regardless of
snapshot age. Shared files remain until unreachable; current snapshots,
explicit refs, active compaction inputs, unresolved publications, and hot
objects remain protected. The fork expires only Forge's explicit IDs.

**RED.** Add
`forge::reader_expiry_ordering::last_table_reader_controls_destructive_cleanup`
to the existing Redux integration target. It fails because current selection
uses age plus epoch/frontier protection.

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p vala-bifrost-redux --features test-support --test integration --test-threads=1 -E "test(=forge::reader_expiry_ordering::last_table_reader_controls_destructive_cleanup)"'
```

**GREEN.** Serialize Forge's reader check and destructive preparation with the
same table authority used by acquisition. Remove snapshot-age and retain-last
eligibility from the Wyrd selector, retain the real roots, and commit only the
explicit selected IDs.

**REFACTOR.** Remove cutoff arithmetic, reader watermarks, and age-only due
logic once selection and cleanup share the active-read decision.

### Scenario 4 — An unsettled promotion blocks conflicting maintenance

**Behavior.** A prepared `ScribePromotion` operation blocks rewrite and
snapshot expiration until its catalog commit and `file_list` settlement agree.
After settlement, maintenance proceeds through its ordinary path. No new
barrier table or state machine is introduced.

**RED.** Add
`forge::promotion::unsettled_promotion_blocks_rewrite_and_expiration` to the
existing Redux integration target. It fails because current reconciliation
only treats open Iceberg rewrites as replacement barriers.

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p vala-bifrost-redux --features test-support --test integration --test-threads=1 -E "test(=forge::promotion::unsettled_promotion_blocks_rewrite_and_expiration)"'
```

**GREEN.** Reuse the existing operation owner and table maintenance authority
to include open promotions in the replacement barrier before rewrite or
expiration begins.

**REFACTOR.** Share the existing open-operation decision. Do not create a
second barrier abstraction.

### Scenario 5 — The optimized cut remains exact under catalog movement

**Behavior.** Cold and warm one-table and multi-table queries each use one
acquisition statement on the successful path. Pointer and hot-row reads are
not repeated per table. Metadata documents are read directly and concurrently.
If the selected document is missing after a catalog move, Oracle reacquires the
complete cut once; a second `NotFound` is terminal. Authorization happens
before table object IO. Cross-tenant lookup fails without exposing identity.

**RED.** Add
`distributed::tenant_scoped_active_cut_is_one_statement_and_one_bounded_retry`
to the Oracle journey. It fails because the current planner performs separate
pointer, protection, revalidation, and hot-row statements.

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=distributed::tenant_scoped_active_cut_is_one_statement_and_one_bounded_retry)"'
```

**GREEN.** Route every table through the atomic acquisition result, authorize
the resolved tables, materialize metadata and manifests concurrently, and
perform the single permitted reacquisition only for metadata `NotFound`.

**REFACTOR.** Delete the old pointer-read, revalidation, and per-table hot-read
paths. Keep direct metadata IO; add no cache or concurrency coordinator.

### Scenario 6 — Held queries remain exact through publication and rewrite

**Behavior.** A query first selects an exact Scribe hot object path, then stays
held while that same object is promoted unchanged, its `file_list` row becomes
terminal, and rewrite publishes a replacement object and snapshot. The journey
proves the originally selected path still exists and the query returns its
rows exactly once while the active table read is held, even though the hot-row
root has transitioned to catalog ownership. Expiration and both cleanup paths
refuse during the hold. After every local and analytical reader drains, the
claim releases; the next maintenance pass may expire the replaced snapshot,
delete that exact original path once it is unreachable, and preserve the
replacement. Cancellation and execution failure follow the same ordering.

**RED.** Add
`distributed::held_query_blocks_cleanup_then_releases_replaced_snapshot` to
the Oracle journey. It fails because the current journey and maintenance path
do not prove the single active-table ownership contract.

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=distributed::held_query_blocks_cleanup_then_releases_replaced_snapshot)"'
```

**GREEN.** Extend the existing preparation hold and Forge maintenance owners
to observe the active row before release and its absence after complete
settlement. Record and assert the original hot object path, its promotion and
terminal `file_list` transition, the distinct rewrite replacement, refusal of
every destructive path while held, exact query rows, deletion of only the
original after release, and survival of the replacement. Do not advance an
independent Forge clock or wait for an artificial retention period.

**REFACTOR.** Keep one journey covering success, cancellation, and destructive
release ordering. Reuse existing server and maintenance controls.

### Scenario 7 — Local and forwarded queries share one configured default deadline

**Behavior.** When a request omits an explicit deadline, both local Oracle
entry and the public forwarder use the same non-default deadline resolved from
the server's Oracle configuration. An explicit request deadline is preserved.
Neither path substitutes `OracleConfig::default()` or rejects a deadline for
exceeding a new maximum.

**RED.** Add
`query::configured_default_deadline_is_shared_by_local_and_forwarded_queries`
to the server journey. It fails because the public forwarder currently falls
back to `OracleConfig::default()` rather than the composed configuration.

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=query::configured_default_deadline_is_shared_by_local_and_forwarded_queries)"'
```

**GREEN.** Make the boot-resolved Oracle configuration the single default
deadline source consumed by both entry paths. Rerun the held-query scenario to
show that this configuration correction does not alter active-read safety.

**REFACTOR.** Remove the request-path default reconstruction and any duplicate
stored default once both entries consume the same composed authority.

### Scenario 8 — Physical object cleanup also removes its terminal file-list row

**Behavior.** A promoted object's terminal `file_list` row survives promotion,
rewrite, expiration preparation, active-reader refusal, delete failure, and
cleanup uncertainty. The same eligibility decision authorizes physical and
metadata deletion: after the object is deleted or confirmed already absent,
the cleanup completion transaction removes the matching terminal row and
completes the candidate. Retrying after object deletion but before SQL
completion converges by observing absence. Nonterminal rows and rows belonging
to unsettled promotions are untouched.

**RED.** Add
`forge::expired_cleanup::terminal_file_list_row_is_removed_only_after_object_cleanup`
to the existing Redux integration target. It fails because expired-object
cleanup currently completes without pruning terminal `file_list` metadata.

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p vala-bifrost-redux --features test-support --test integration --test-threads=1 -E "test(=forge::expired_cleanup::terminal_file_list_row_is_removed_only_after_object_cleanup)"'
```

**GREEN.** Extend the existing cleanup completion owner so successful deletion
or confirmed absence removes the matching terminal row in the same database
transaction that completes the candidate. Preserve every row that is
nonterminal or still required by an open promotion.

**REFACTOR.** Keep metadata retirement inside the existing cleanup lifecycle.
Add no TTL, archive, periodic scan, or second garbage collector.

## Acceptance Criteria

- One tenant-scoped acquisition statement per ordinary query attempt returns
  all requested table identities, pointers, and hot candidates and commits one
  active query/table row before returning them.
- Acquisition and destructive maintenance serialize through the existing
  table authority; no pointer-read/delete race can expose an expired cut.
- A readable cut and its active-read ownership are one private Rust value from
  construction through terminal settlement. No production API returns or
  retains the cut independently.
- Normal success, cancellation, timeout, and failure release claims after all
  local and analytical readers stop. A dropped owner releases its claims.
  A crashed owner's claims expire once PostgreSQL time passes the query's own
  deadline (spec revision 11).
- Forge refuses expiration and object cleanup while a table has an active
  reader, then treats replaced snapshots as eligible without an age wait after
  the final reader releases. Every other real protection root remains.
- The Iceberg expiration commit acts only on explicit Forge-selected IDs. The
  fork performs no independent snapshot aging, reference aging, or retain-last
  selection, and every unlisted snapshot and ref remains unchanged.
- An unsettled promotion blocks rewrite and expiration using existing
  operation state and authority.
- RLS and the narrow catalog definer prevent cross-tenant identity, pointer,
  hot-row, and active-read exposure.
- Metadata `NotFound` permits one complete reacquisition; ordinary success uses
  one statement and repeated failure is terminal.
- Local and forwarded queries consume one boot-resolved configured default
  deadline when the request omits one; explicit deadlines remain uncapped.
- The held-query journey tracks the exact hot object through promotion and
  rewrite, proves it survives while held, and proves only that original path is
  deleted after release while the replacement survives.
- Successful expired-object deletion or confirmed absence removes the
  matching terminal `file_list` row in the cleanup completion transaction.
  Failures retain it; nonterminal and unsettled-promotion rows are never
  pruned.
- No reader epoch, frontier, ancestry path, IO gate, follower reader-cut wire
  field, metadata cache, retention-derived deadline, query cap, reader capacity
  preallocation, or compatibility alias remains.
- Architecture and operator documentation describe exact active readers and
  immediate post-reader eligibility rather than age or epoch protection.

## Expected Write Set and Consumer Closure

- `crates/vala/vala-sql`: unshipped reader-authority migration, active-read row
  type and tenant-scoped queries, catalog-pointer definer, terminal file-list
  retirement, SQL integration coverage, and tenant-isolation inventory.
- `crates/vala/vala-bifrost-redux`: Oracle planner/catalog/stream ownership,
  analytical and follower consumers, direct metadata materialization, Forge
  selection and cleanup, promotion reconciliation, storage wrappers, and
  Redux integration tests.
- `bohmian-ai/iceberg-rust`, `bohmian-ai/iceberg-compaction`, workspace
  `Cargo.toml`, and `Cargo.lock`: explicit-ID-only expiration, focused fork
  proof, and immutable pins that keep one Iceberg dependency universe.
- `crates/wyrd/wyrd-server` and `crates/wyrd/wyrd-testing`: Oracle composition,
  forwarding from the single configured default, state inspection, exact-path
  held-query journeys, and removed epoch setup.
- `crates/wyrd-spec` and `crates/wyrd/wyrd-tonic`: remove follower reader-cut
  projections and regenerate affected contracts. Add no replacement wire
  field.
- `architecture/bifrost-design.md`, `architecture/agent-rules.md`, Iceberg and
  analytical reliability references, and public Forge documentation: replace
  epoch/frontier/retention claims with the approved active-read lifecycle.
- Existing reader epoch, frontier, permit, revalidation, and obsolete test
  surfaces are deletion candidates only after their consumers move to the new
  owner.

## Verification and Evidence

Run each exact scenario command during RED/GREEN execution. Final verification:

```bash
mise run fmt
mise run lints
mise run codegen:check
mise run test:principals:integration
mise run verify:bifrost
```

`verify:bifrost` owns tenant-isolation, SQL, Redux integration, and Bifrost
journey coverage. `test:principals:integration` separately proves the served
OpenAPI after the follower wire contraction.

The completion report must include:

- acquisition statement counts for one and multiple tables;
- active rows observed while held and absent after settlement;
- PostgreSQL-time abandoned-row behavior;
- expiration refusal while held and successful expiration after release;
- the exact selected hot path surviving its ownership transition and the
  distinct rewrite replacement surviving cleanup;
- the configured non-default deadline observed through local and forwarded
  entry;
- terminal `file_list` metadata retained through refusal and removed as part
  of successful physical cleanup;
- explicit-ID-only Iceberg expiration;
- exact hot and Iceberg results through promotion and rewrite; and
- deletion evidence for the obsolete epoch, frontier, permit, and wire paths.

## Material Stop Conditions

- Stop if any reader can perform file or metadata IO after the opaque
  cut-and-claim owner has released. That is an ownership-contract defect, not
  a reason to add a time-based retention fallback.
- Stop if any destructive cleanup path can bypass the table maintenance
  authority or active-read check. Bring that path under the same owner before
  deleting the old protection.
- Stop if an analytical descendant can outlive the query expiration recorded
  for its active rows. Correct the existing query lifecycle contract rather
  than adding a second expiration.
- Stop if a required historical/time-travel snapshot ref exists outside the
  enumerated roots. Route it through specification authority before changing
  deletion semantics.
- Stop if a durable consumer still requires a terminal `file_list` row after
  the exact object is confirmed unreachable and deleted. Move that consumer to
  its proper lineage owner before metadata retirement is implemented.
- Stop if the single acquisition statement cannot commit claims and return the
  consistent pointer/hot view under `TenantConn` RLS. Do not split protection
  into a later statement.

## Authority Links

- `changes/active/forge-concurrent-planning/spec.md` revision 11
- `changes/active/forge-concurrent-planning/revision/TASK-005-R1-implementation-reference.md`
- `AGENTS.md`
- `architecture/agent-rules.md`
- `architecture/wyrd-design.md`
- `architecture/wyrd-doctrine.mdx`
- `architecture/bifrost-design.md`
- `architecture/references/languages/spec-driven-development.md`
- `architecture/references/languages/implementation-execution.md`
- `architecture/references/languages/testing-workflows.md`


## Implementation Evidence

Commits: `41968879e`, `17c864196`, `3f69e10a5`, `fa2ad497b`, `191fd217e`
(S3), `362d7e758` (S4), `6acd54371` (S8), `62de8d84f` (S7), `7dde4299c` (S5),
`e4aecabc5` (S2), `3a51a24d5` (S6 and expired-cleanup age fix), `6160a35e9`
(lint and sync cleanup), `d87086302` (documentation), `a678b5916`
(fence-liveness definer, removed by revision 11), `05cceaf35` and
`484c3b4f6` (gate fixes), `0924e52cf` (revision 11: drop release and
deadline expiry).

Scenario commands use the exact forms listed per scenario above
(`scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked ... -E "test(=...)"'`).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| One tenant-scoped acquisition statement commits one active row per query/table | `vala-sql` migration `20260910000025_oracle_reader_authority.sql` (`oracle_acquire_table_cut`, SECURITY INVOKER under TenantConn RLS) | S1 `active_table_read_claim_is_atomic_tenant_scoped_and_postgres_expired` (`pg_oracle_membership`); S5 journey | PASS |
| Acquisition and destructive maintenance serialize through table authority | Acquisition takes the maintenance authority `FOR SHARE`; Forge destruction takes it exclusively and checks `table_is_read` | S3 `forge::reader_expiry_ordering::last_table_reader_controls_destructive_cleanup` | PASS |
| Cut and ownership are one private value through settlement | `ActiveReadClaim` owned with the materialized cut in `oracle/mod.rs`; released in `settle_and_finish_stream` | S2 `distributed::held_cut_owns_active_reads_until_all_descendants_settle` (active=1 while a follower is held; 0 before the terminal frame) | PASS |
| Release after success, cancellation, timeout and failure; a dropped owner releases; a crashed owner's rows expire at the query deadline in PostgreSQL time | `planner.rs` `ActiveReadClaim::release` plus `Drop` spawning the same release; `ActiveReadOwner::deadline` bound as `p_deadline_ms`, `abandon_after = statement_timestamp() + p_deadline_ms ms`; `active_table_reads_exist` deletes on `abandon_after <= statement_timestamp()` only | S2 dropped caller and dropped pin both reach 0 rows (RED with drop release disabled: "a dropped caller: 1 active reads remain"); S1 `abandon_after - acquired_at` equals the bound 3600 s, a future row survives, a past row is discarded | PASS |
| Forge refuses while read; immediate eligibility after last reader; other roots stay | `forge/expire.rs` no age/depth retention; `forge/orphan_gc.rs` age floor only for `AttemptGeneration` | S3; S6 `distributed::held_query_blocks_cleanup_then_releases_replaced_snapshot`; unit `forge_expired_cleanup_eligibility_matrix` | PASS |
| Iceberg expiration acts only on explicit IDs | Fork pins `iceberg-rust@97c32f63`, `iceberg-compaction-core@ef97aea0` (`17c864196`) | Fork proof in the pinned fork; Tier-2 `forge::snapshot_expiration` suite | PASS |
| Unsettled promotion blocks rewrite and expiration | Existing operation owner and table authority (`362d7e758`) | S4 `forge::promotion::promotion_barrier::unsettled_promotion_blocks_rewrite_and_expiration` | PASS |
| RLS plus narrow definer prevent cross-tenant exposure | `oracle_catalog_metadata_location` SECURITY DEFINER pointer lookup; no `wyrd_app` grant on `iceberg_catalog` | S1 cross-tenant cases; S5 cross-tenant lookup; tenant-isolation check in `verify:bifrost` | PASS |
| One-statement success, one reacquire on `NotFound`, second terminal | Oracle acquisition loop in `catalog/bifrost_catalog.rs` / `oracle/mod.rs` | S5 `distributed::tenant_scoped_active_cut_is_one_statement_and_one_bounded_retry` | PASS |
| Local and forwarded queries share one boot-resolved default deadline | `62de8d84f` (server state forwarding) | S7 `query::configured_default_deadline_is_shared_by_local_and_forwarded_queries` | PASS |
| Held-query journey tracks exact hot path through promotion and rewrite | S6 journey in `wyrd-testing/tests/bifrost/oracle/distributed.rs` | S6: originals survive while held with terminal rows; after release only originals deleted, replacement survives, promoted snapshot expired | PASS |
| Successful cleanup removes the terminal `file_list` row; failures retain it | Cleanup completion transaction (`6acd54371`) | S8 `forge::expired_cleanup::terminal_file_list_row_is_removed_only_after_object_cleanup`; S6 rows `None` after cleanup | PASS |
| No epoch, frontier, ancestry, IO gate, reader-cut field, cache, retention deadline, cap, preallocation, or alias | Deletions in `41968879e`, `3f69e10a5`, `fa2ad497b` | `git grep -i -E "reader_epoch|ReaderEpoch|ancestry_frontier|reader_cut|IoGate|io_permit|reader_protection"` finds only protobuf `reserved "reader_cut"`; `codegen:check` | PASS |
| Docs describe exact active readers and immediate eligibility | `architecture/bifrost-design.md`, `references/domain/iceberg.md`, `references/domain/analytical-operations-reliability.md`, `docs/.../bifrost/forge.svx` | `mise run docs:check` | PASS |

### Diagnoses

- **S3 — Symptom:** later passes recorded no expiry after a refusal while a
  read was held. **Evidence:** the refused attempt remained active in
  `forge_tasks`. **Cause:** expiry was claimed before the read check, so a
  refusal left an active attempt. **Fix site:** the leader's `table_is_read`
  pre-check before claiming destructive work.
- **S2 — Symptom:** the journey hung. **Evidence:** the trace logs
  "analytical attempt admitted" but never "Oracle leader opened one query
  stream". **Cause:** `query_sql` awaits the first batch, which needs the
  paused follower; awaiting it before the join deadlocked the test. **Fix
  site:** the test only (run the query inside `try_join!`/`select!`).
- **S6 — Symptom:** no rewrite replaced the promoted object. **Evidence:**
  small-files task `progressed=false` with one input. **Cause:** small-files
  needs at least two inputs. **Fix site:** the test writes two objects.
- **S6 — Symptom:** expired cleanup refused with `TooYoung`, then "retry
  failure lost attempt ownership". **Evidence:** "refreshed protection refused
  a prepared expired-cleanup candidate: TooYoung". **Cause:** shared
  `orphan_gc.rs::eligibility` applied the orphan TTL to expired-object
  cleanup. This was hidden because Tier-2 S3 advanced the clock 48h.
  **Fix site:** `eligibility` applies the age floor only to
  `MaintenanceScope::AttemptGeneration`. Both callers were checked; Tier-2
  `expired_cleanup`, `reader_expiry_ordering`, `snapshot_expiration`,
  `promotion` and `orphan_cleanup` pass (21/21).
- **S6 — Symptom:** originals not deleted after release. **Cause:** the
  head's `lineage_snapshot_id` root (a legitimate pre-existing root) retained
  the rewrite base. **Fix site:** the test only. It appends ordinarily with
  compaction disabled so the head moves, and production keeps the root.

- **verify:bifrost — Symptom:** `check:tenant-isolation` rejected public
  `refuse_active_table_reads` and `active_table_reads_exist` (raw
  `Transaction`). The S1 test was moved to the public owner
  (`BifrostTableMaintenanceAuthority::has_active_reads` on `TenantConn`,
  which the Forge leader's `table_is_read` uses), and it then failed at
  `pg_oracle_membership.rs:1001`: a dead fence alone discarded a read.
  **Evidence:** `vala.cluster_nodes` RLS is `data_tenant_id =
  wyrd.current_tenant()` and Oracle fences are SYSTEM_OWNER rows. **Cause:**
  under a tenant-bound connection the abandonment's `NOT EXISTS (live
  fence)` saw no row, so a live owner's read past `abandon_after` was
  discarded. Only the operator-pool path (BYPASS) was correct. **Fix site:**
  the shared rule `active_table_reads_exist` now calls a narrow SECURITY
  DEFINER `vala.oracle_fence_is_live(node_id, fencing_token, liveness_secs)`
  added to unshipped migration 025. Both helpers are `pub(crate)`. Callers
  checked: `forge_tasks.rs` expiration preparation (operator),
  `BifrostTableMaintenanceAuthority::has_active_reads` (TenantConn: Forge
  `gc.rs`/`orphan_gc.rs`). Superseded by revision 11: abandonment no longer
  reads fence liveness, so the definer was removed (`0924e52cf`).
- **verify:bifrost — Symptom:** `forge::snapshot_expiration` compared a
  manual Forge clock with host `Utc::now()`. **Cause:** the test read the
  wrong clock. **Fix site:** the test reads `forge.clock_for_test()`.
- **verify:bifrost — Symptom:** `forge::production_routes` failed on a
  `snapshot_expiry`/`expired_cleanup` sibling route. **Cause:** a stale ban
  from before immediate eligibility; those routes are now expected. **Fix
  site:** the ban was removed. Disabling expiration instead stopped orphan
  scheduling and was reverted.
- **verify:bifrost — Symptom:** `forge::production_closeout` reader case
  expected expiration that immediate eligibility now performs only after
  release. **Fix site:** the test holds the reader through
  `HELD_READER_PASSES`, asserts the objects survive, then collects them
  exactly after `wait_for_reader_release`.
- **verify:bifrost — Symptom:** `server::eval_verification` and
  `oracle::published` bracketed durable timestamps with host time. **Cause:**
  PostgreSQL owns those timestamps; host skew put them out of range. **Fix
  site:** both tests read `clock_timestamp()` from PostgreSQL.

### Non-goals

No metadata cache, query cap, reader-capacity preallocation, retention-derived
deadline, replacement wire field, compatibility alias, or `iceberg_catalog`
grant to `wyrd_app` was added. No unrelated files changed.

### Risks

- A crashed Oracle's rows block destructive cleanup on their tables until
  each query's deadline passes in PostgreSQL time. This is by design. The
  deadline is computed from host time and bound as a duration, so host/PG skew
  does not shift it; acquisition latency only lengthens it.
- A dropped owner's release is spawned, not awaited. Outside a Tokio runtime,
  or if the release statement fails, the row waits for its deadline.
- After a refused cleanup candidate, failure settlement can log "retry failure
  lost attempt ownership". This was not observed once the age floor was
  fixed, and was not addressed here.
- S5 counts acquisition calls, not wire statements. Audit staging and the
  release `DELETE` are separate statements.
- S5 production code predates its test commit, so it has no recorded RED.
- In S6, orphan refusal is not discriminating because Scribe objects are
  outside orphan scope. Tier-2 `orphan_cleanup` covers that refusal.

### Final Verification

- `mise run fmt`, `mise run lints`, `git diff --check`: clean.
- `mise run verify:bifrost`: exit 0. Every lane passed, including
  `check:tenant-isolation`, the Bifrost unit and integration suites, and the
  forge (21), scribe (25), oracle (49), drift and SDK journeys.
- `mise run test:principals:integration`: exit 0.
- S1 and S2 were rerun after revision 11 with their exact commands. S2 RED
  was demonstrated with the drop release disabled.
- No contract, schema, stub, or docs change was made in revision 11, so
  `codegen:check` and `docs:check` were not rerun for it.

## Final Status

IMPLEMENTED
