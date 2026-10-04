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

## Owners, Scope, Consumers, and Prohibited Changes

- `vala-sql` owns the tenant-scoped active table-read rows and the atomic cut
  acquisition/release operations. There is exactly one durable row for each
  active query/table pair. It carries the existing durable query identity,
  table identity, exact Oracle node fence, and PostgreSQL-computed abandonment
  time. It carries no snapshot ancestry, reader epoch, revision state machine,
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
  every analytical descendant have stopped. An unexpected drop performs no
  blocking cleanup and leaves the durable rows protective until the query's
  existing total expiration. Query class is derived after cut acquisition, so
  PostgreSQL uses the existing six-hour analytical total expiration as the
  conservative abandonment lifetime for either possible class and evaluates
  it from `statement_timestamp()`. Forge can discard the row only after the
  exact Oracle node fence is no longer live and that expiration has passed. A
  live owner's row never expires underneath it. This does not change either
  class's runtime; no Rust wall clock authorizes cleanup.
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
   PostgreSQL-time abandonment handling using the existing six-hour analytical
   expiration because class selection follows cut acquisition.
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
and evaluates the abandonment time from its own `statement_timestamp()` and
the existing analytical total expiration. An abandoned claim is removable
only when its exact Oracle fence is no longer live and the expiration has
passed. The caller retains transaction ownership.

**REFACTOR.** Delete epoch, frontier, ancestry, and reclamation code once the
focused SQL test remains green. Keep one concrete SQL owner and reuse existing
row/domain types where they fit.

### Scenario 2 — A usable cut cannot outlive or detach from its claim

**Behavior.** Oracle receives an opaque cut-and-claim owner. Every local scan,
analytical graph, returned stream, cancellation path, first-batch failure, and
terminal frame retains that owner until all descendants have settled. Normal
terminal completion releases its active rows exactly once. An unexpected drop
leaves the durable rows for PostgreSQL-time abandonment rather than risking an
early release.

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
after the existing local and distributed join points. Preserve a dropped or
crashed owner's rows until their database abandonment time.

**REFACTOR.** Collapse redundant terminal wrappers after the opaque owner is
the only way to retain a readable cut. Do not add an IO authorization layer or
an async `Drop` workaround.

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
  local and analytical readers stop. Unexpected loss remains protected until
  the owning Oracle fence is no longer live and the existing analytical total
  expiration has passed according to PostgreSQL. A live owner's row does not
  expire.
- Forge refuses expiration and object cleanup while a table has an active
  reader, then treats replaced snapshots as eligible without an age wait after
  the final reader releases. Every other real protection root remains.
- The Iceberg expiration commit acts only on explicit Forge-selected IDs.
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

- `changes/active/forge-concurrent-planning/spec.md` revision 10
- `AGENTS.md`
- `architecture/agent-rules.md`
- `architecture/wyrd-design.md`
- `architecture/wyrd-doctrine.mdx`
- `architecture/bifrost-design.md`
- `architecture/references/languages/spec-driven-development.md`
- `architecture/references/languages/implementation-execution.md`
- `architecture/references/languages/testing-workflows.md`
