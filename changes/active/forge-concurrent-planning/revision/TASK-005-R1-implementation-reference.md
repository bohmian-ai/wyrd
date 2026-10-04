# TASK-005-R1 implementation reference

This file is the normative implementation reference for
[`TASK-005-R1`](../tasks/TASK-005-R1-active-table-reader-cut.md). It restores
the architecture and cross-owner implementation decisions needed to complete
TASK-005 Scenario 4. Private helper names and reversible local organization are
left to implementation; the boundaries, persisted facts, ordering, failure
semantics, fork behavior, and deletion rules below are not optional.

The approved revision-10 specification remains the behavior authority. This
reference may explain how to realize that behavior but may not weaken, replace,
or reinterpret it.

## Non-negotiable decisions

1. PostgreSQL is the only coordination clock. Rust wall clocks and Iceberg
   snapshot timestamps never authorize deletion.
2. One durable row represents one active query/table pair. There is no reader
   epoch, ancestry frontier, IO permit, capacity slot, or retained-snapshot
   preallocation.
3. The Oracle cut and its active-read ownership are one inseparable private
   Rust owner through planning, local and distributed execution, streaming,
   cancellation, and terminal settlement.
4. One successful attempt uses one tenant-scoped SQL `SELECT` for all requested
   tables. A metadata-document `NotFound` may cause one complete second attempt;
   no other retry loop exists.
5. `TenantConn` RLS is the tenant boundary for `vala.bifrost_tables`,
   `vala.file_list`, the maintenance-authority relation, and active reads.
   Ordinary acquisition SQL adds no duplicate tenant predicates.
6. `wyrd_app` receives no direct schema or table access to
   `iceberg_catalog`. Catalog pointer access is only through one narrowly
   scoped, tenant-derived `SECURITY DEFINER` function.
7. Forge expires exactly the snapshot IDs selected under Wyrd authority. The
   Iceberg action performs no independent snapshot aging, reference aging, or
   retain-last selection.
8. A table with any active read admits no snapshot-expiration preparation,
   expired-object cleanup, or orphan cleanup. Rewrite may proceed after
   promotion settles because the active read blocks reclamation of its old
   objects.
9. A prepared or otherwise unsettled Scribe promotion blocks rewrite and
   snapshot expiration using the existing Forge operation state and table
   maintenance authority. No new barrier relation is added.
10. Physical deletion eligibility is the sole authorization for deleting a
    matching terminal `file_list` row. Successful deletion or confirmed
    absence removes that row in the existing cleanup completion transaction.
11. The boot-resolved Oracle configuration is the only source of the default
    query deadline for both local and forwarded entry. There is no new maximum
    query duration.
12. Wyrd has not shipped. Edit the unshipped reader-authority migration in
    place and delete obsolete schema, wire, generated, configuration, test, and
    documentation surfaces without compatibility aliases or rollout logic.

## 1. PostgreSQL schema and privilege boundary

### 1.1 Replace reader epochs with active table reads

Edit the unshipped
`crates/vala/vala-sql/migrations/20260910000025_oracle_reader_authority.sql`
in place. Preserve `vala.bifrost_table_maintenance_authority` as the one
serialization row per tenant-qualified table and preserve the existing Forge
snapshot-expiration claim relation. Remove the epoch, protection-header, and
frontier-member schema that exists only for reader epochs and ancestry.

The replacement active-read relation stores only these facts:

- authenticated data tenant;
- durable query identity already used by Oracle;
- registered `table_uid` and its catalog/namespace/table identity;
- exact Oracle `node_id` and fencing token;
- acquisition time and abandonment time computed by PostgreSQL.

Its key makes one row the only representable state for a query/table pair.
The table identity references the existing maintenance-authority identity.
The Oracle fence is exact; a node ID without its fencing token is insufficient.
The relation uses forced RLS and the same `wyrd.current_tenant()` policy as the
other tenant-owned Bifrost control relations.

PostgreSQL sets abandonment time from `statement_timestamp()` plus the existing
six-hour analytical total expiration. Acquisition retries do not derive or
compare time in Rust. An active row remains protective regardless of its
timestamp while its exact Oracle fence is live. Abandoned-row deletion requires
both facts in one database decision:

- the exact `(node_id, fencing_token)` is no longer live; and
- PostgreSQL `statement_timestamp()` has passed the stored abandonment time.

Normal release deletes the query's active rows only after every local reader
and analytical descendant has stopped. Release is idempotent. Unexpected Rust
drop performs no blocking SQL and leaves the rows for the abandonment rule.

### 1.2 Give `TenantConn` narrow catalog pointer access

Keep the existing revocation of `wyrd_app` access to `iceberg_catalog`. Do not
grant `USAGE` on that schema and do not give Oracle a platform catalog pool.

Add one platform-owned `SECURITY DEFINER` catalog-pointer function with all of
these properties:

- `SET search_path = ''` and schema-qualified objects throughout;
- owner `wyrd_platform_admin`;
- `PUBLIC` has no privileges;
- `wyrd_app` may only execute the function;
- it takes the logical namespace and table name, never a caller-supplied tenant
  or arbitrary physical namespace;
- it reads the tenant exclusively from `wyrd.current_tenant()`;
- it accepts only the canonical logical namespace form already admitted by
  `TenantTableBinding` and derives the physical namespace exactly as
  `vala.tenants.<tenant UUID>.<logical segment>`;
- it fixes the catalog name to `wyrd-redux` and matches the SQL catalog's table
  record shape, including its existing record-type compatibility rule;
- it returns only the current `metadata_location`, never catalog rows or an
  enumerable result;
- missing or ambiguous catalog identity returns no pointer and is handled as
  the existing catalog-not-found error by the caller.

`iceberg_catalog.iceberg_tables` is created at runtime by
`iceberg-catalog-sql`, after migrations may run. The definer must therefore
defer resolution of that relation without accepting dynamic identifiers or
interpolating caller values. A constant schema-qualified statement with bound
values is acceptable. Moving Iceberg catalog table ownership into Wyrd's
migration, granting direct catalog access, or accepting a physical namespace
from the caller is not acceptable.

### 1.3 One security-invoker acquisition operation

The cut-acquisition database operation is `VOLATILE` because it records active
reads. It runs with caller privileges, not elevated catalog privileges, so the
`TenantConn` RLS policies remain load-bearing. It calls only the narrow pointer
definer for the catalog value.

The Rust caller invokes the operation through one SQL `SELECT`. The operation
receives one structured, ordered set of distinct canonical table references,
plus the durable query identity and exact Oracle node fence. It must not accept
separate arrays whose lengths can disagree.

Within that one statement it performs this durable order:

1. Resolve every requested registration and maintenance-authority identity
   through RLS.
2. Lock all requested maintenance-authority rows in deterministic table order.
   This is the serialization point against Forge destructive preparation.
3. Resolve each current catalog pointer through the narrow definer.
4. Read every unresolved `file_list` candidate for each table from the same
   database statement view. These are candidates; manifest reconciliation
   remains Oracle's later object-store work.
5. Insert the one active query/table row for every requested table using
   PostgreSQL time.
6. Return the ordered table identities, pointers, and zero or more hot
   candidates.

Any missing registration, authority row, or catalog pointer fails the whole
statement. No partial active-read set may commit. The caller owns the
`TenantConn` transaction and commits it before constructing a usable cut or
performing metadata, manifest, footer, or data-file IO.

This ordering gives the acquisition-versus-destruction race exactly two legal
outcomes: Forge completes destructive work before the authority lock and the
reader receives the later pointer, or the reader commits its active row first
and Forge refuses destructive preparation.

## 2. Typed SQL and Oracle ownership contract

### 2.1 Acquisition input and result

The `vala-sql` owner exposes one cohesive acquisition operation. Its contract
is:

- input table references are already canonical, distinct, and ordered by the
  Oracle planner;
- output contains exactly one registered authority identity and one non-null
  metadata pointer per requested table, in input order;
- each table contains zero or more unresolved `HotFileRow` values in durable
  file-list order;
- duplicate input cannot create duplicate active rows;
- no caller can combine pointers, identities, and hot rows obtained from
  separate operations.

The private flat SQL row represents every column from the left-joined hot side
as nullable. All hot columns null means an empty hot set. A partially null hot
row, conflicting identity for one ordinal, duplicate table result, missing
table result, malformed UID, or negative persisted size is an internal
invariant error. A missing registration or pointer maps to the existing typed
catalog-not-found behavior rather than a new public error family.

The grouped result is constructed only after validating the complete flat
result. It is not valid to silently drop a table with no hot rows or accept a
partial result.

### 2.2 The inseparable cut owner

Oracle constructs one private owner containing:

- all materialized table cuts for the query;
- the durable query identity; and
- committed active-read ownership for those exact tables.

Its fields do not expose a raw cut or detachable claim. Planning, physical plan
construction, local execution, analytical dispatch, follower work, returned
streams, cancellation, error framing, and terminal delivery retain this owner
or a lifetime-preserving descendant. The owner releases normally only after
all descendants have joined.

Delete the reader guard plus clonable IO-permit split, gated storage wrapper,
epoch authority, ancestry frontier, pointer-revalidation loop, and follower
reader-cut wire projection after all consumers use the new owner. Do not
replace them with another authorization wrapper or async `Drop` protocol.

### 2.3 Query attempt sequence

Each attempt follows this order:

1. Validate and authorize the query and its canonical table references before
   opening table object data.
2. Acquire and commit the complete active cut with one tenant-scoped SQL
   statement.
3. Read immutable metadata documents and manifests directly and concurrently
   across tables. Add no metadata cache, capacity, or singleflight owner.
4. Reconcile the returned unresolved hot candidates against the selected
   immutable metadata and promotion evidence using the existing exact
   hot/Iceberg rules.
5. Build and execute the query while retaining the inseparable owner.

If an exact selected metadata document returns object-store `NotFound` after a
catalog move, Oracle reacquires the complete cut once. The same query/table
ownership remains protective across that retry. A second metadata `NotFound`
or any other failure is terminal. Ordinary success executes one acquisition
statement; the documented race executes two, one per attempt.

## 3. Default query deadline

Server boot resolves one `OracleConfig` from `OracleRuntimeConfig`. That same
resolved value is supplied to the local Oracle and the public forwarding
planner. When `BifrostQueryRequest.deadline_ms` is absent, both paths use that
configured `default_deadline`. When it is present, both preserve it.

The forwarder must not call `OracleConfig::default()` on the request path and
must not keep a separately resolved default. This correction introduces no
maximum deadline and has no relationship to Forge retention. The six-hour
PostgreSQL abandonment lifetime protects dead-owner cleanup; it does not reject
or truncate a live query.

## 4. Forge destructive-maintenance contract

### 4.1 One active-read gate for every destructive path

Snapshot-expiration preparation, expired-object cleanup, and never-published
orphan cleanup take the existing per-table maintenance authority and read the
active table-read relation in the same PostgreSQL transaction. Any active row
blocks the operation for the whole table. Failure or contradictory evidence
also blocks; absence must be proven.

The check occurs before durable destructive preparation. Cleanup also refreshes
the same protection before each physical delete through its existing final
eligibility path. No path may authorize deletion from a previously cached
absence.

Promotion and non-destructive catalog commits may proceed while a reader is
active. Rewrite may proceed after promotion settlement. This is safe because
expiration and object cleanup remain blocked until the last active reader
releases.

### 4.2 Promotion barrier

Before rewrite or snapshot expiration, reuse the existing Forge operation
owner to check for an unsettled `ScribePromotion` operation while holding the
table maintenance authority. Prepared, commit-uncertain, or otherwise
unreconciled promotion state blocks both operations. Once the catalog commit
and `file_list` settlement agree, the ordinary maintenance path may proceed.

Do not add a barrier table, promotion lease, second state machine, or global
lock. Existing nonterminal `file_list` rows and open-operation output roots
continue protecting never-published and uncertain objects from orphan cleanup.

### 4.3 Exact snapshot selection

Forge selects explicit snapshot IDs after loading current metadata and all
authoritative roots. It must exclude:

- the current snapshot;
- every retained Iceberg branch or tag head and everything the ref contract
  retains;
- an active compaction's observed snapshot;
- unresolved publication and cleanup evidence;
- any open promotion barrier; and
- every other root already required by revision 10.

There is no snapshot-age cutoff, retain-last setting, or host-time comparison.
The prepared operation records the base metadata location, current snapshot,
retained refs, and sorted explicit selected IDs. Its identity and reconciliation
are derived from those facts, not from a cutoff timestamp. Before submission,
Forge reloads metadata and requires those facts and the selected IDs to remain
valid. Drift resets or reconciles through the existing operation lifecycle; it
never widens the set.

## 5. Required `iceberg-rust` fork change and pins

The current pinned fork at `1ccadbf5` does **not** provide the agreed behavior
through `expire_snapshot_ids` alone:

- its action always runs age-based snapshot selection unless given a sentinel
  cutoff; and
- even with an ancient cutoff, it independently ages non-main refs using its
  own `Utc::now()`.

Therefore Wyrd cannot satisfy revision 10 by calling
`expire_older_than_ms(i64::MIN)`. The fork must expose an explicit-ID-only
expiration mode with this contract:

- the committed removal set is limited to the caller's explicit IDs;
- unlisted snapshots are not selected by age;
- no branch or tag is removed by age;
- retain-last and table age properties do not add or subtract explicit IDs;
- the current snapshot and every retained ref head remain unremovable and an
  attempt to name one fails;
- unknown explicit IDs preserve the existing idempotent behavior;
- physical cleanup evidence contains only files made unreachable by removal of
  those explicit IDs.

The exact public method name is a reversible fork API choice. Wyrd's Forge
call site must use that explicit-only mode and must not also call
`expire_older_than_ms` or `retain_last`.

The fork adds focused tests proving exact IDs, preservation of an old unlisted
snapshot, preservation of an old non-main ref, rejection of current/ref heads,
and exact expired-file evidence.

After the fork commit:

1. Pin `iceberg`, `iceberg-catalog-sql`, `iceberg-datafusion`, and
   `iceberg-storage-opendal` in the workspace to the same immutable new SHA.
2. Update `bohmian-ai/iceberg-compaction` to that same Iceberg SHA and pin
   `iceberg-compaction-core` to its corresponding immutable commit.
3. Regenerate `Cargo.lock` and verify the graph contains one Iceberg/Arrow/
   Parquet universe.

Do not carry two Iceberg revisions, use a moving branch, or preserve the old
age call as a fallback.

## 6. Physical cleanup and terminal `file_list` deletion

The existing expired-cleanup handoff remains the only owner of physical files
made unreachable by snapshot expiration. For each exact candidate:

1. Take the existing table authority and refresh catalog, operation, active
   reader, tenant/table binding, and object evidence.
2. If physical deletion is not eligible, retain the candidate and its
   `file_list` row.
3. Delete the physical object, or accept object-store `NotFound` as already
   absent under the existing idempotent rule.
4. In the existing PostgreSQL transaction that advances/completes that exact
   cleanup candidate, delete a matching terminal `vala.file_list` row for the
   same tenant, logical table, and object path.

The metadata delete is conditional on terminal publication state:
`compacted = true` and `committed_snapshot_id IS NOT NULL`. No matching row is
normal for Forge rewrite outputs and is a no-op. A nonterminal row or a row
still required by unsettled promotion is never deleted.

Object storage and PostgreSQL are not one transaction, so object deletion is
first. If the database completion fails afterward, retry observes the object
absent and repeats step 4 before completing the candidate. Never delete the
metadata row first. Add no TTL, metadata archive, periodic scan, or second GC.

## 7. Exact hot-object proof

The Scenario 6 journey records the physical path of an object returned in the
initial hot cut and proves this sequence:

1. The query holds its active table read and has selected the exact hot path.
2. Promotion appends that same object unchanged and settles its `file_list`
   row terminal.
3. Rewrite publishes a distinct replacement object and snapshot.
4. Snapshot expiration, expired cleanup, and orphan cleanup all refuse while
   the reader is held.
5. The held query returns every expected row exactly once and the original
   path still exists.
6. Every local and analytical descendant settles and the active row releases.
7. The next maintenance pass expires the replaced snapshot and deletes the
   original path when it becomes unreachable.
8. The replacement path survives, and the original terminal `file_list` row
   is removed with physical cleanup.

Assertions based only on returned row values are insufficient because a later
snapshot could supply the same values. The test must assert the original and
replacement object identities directly.

## 8. Removal and consumer closure

Implementation is incomplete until all consumers move and the obsolete model
is deleted:

- reader epoch, heartbeat, activation, invalidation, frontier, ancestry, and
  recovery schema and SQL;
- Oracle reader authority, guards, IO permits, gated storage, and pointer
  revalidation;
- analytical follower reader-cut fields, codec handling, tonic/spec
  projections, generated schemas, and state inspection;
- retention-derived snapshot selection, cutoff evidence, retain-last plumbing,
  configuration, tests, and documentation;
- the forwarder's request-path `OracleConfig::default()` fallback;
- stale architecture and operator text describing epoch pins, age retention,
  or pointer revalidation.

Preserve the existing table maintenance authority, Forge operation recovery,
snapshot-expiration claims, exact hot/Iceberg reconciliation, active
compaction roots, unresolved publication/cleanup roots, authorization, audit
staging behavior, cancellation, and terminal framing.

## 9. Forbidden substitutions

The following are explicit deviations and must not be introduced:

- a retention window, grace period, clock-skew margin, or Rust-clock cleanup
  decision;
- rejection of explicit query deadlines above a newly invented maximum;
- a cache, LRU, singleflight coordinator, cache capacity, or metadata
  prefetch service;
- reader slots, reader capacity, snapshot preallocation, or retain-last used as
  reader safety;
- manual tenant predicates in ordinary `TenantConn` acquisition SQL;
- direct `wyrd_app` grants on `iceberg_catalog` or Oracle use of the platform
  catalog credential;
- an independently supplied tenant or physical namespace at the definer
  boundary;
- splitting claim creation from cut selection into a second SQL statement;
- a detachable raw cut, detachable claim, new IO gate, or async cleanup in
  `Drop`;
- a new promotion barrier table or state machine;
- age-based fallback inside `iceberg-rust`;
- a separate `file_list` metadata sweeper, archive, or deletion TTL;
- compatibility fields, aliases, dual reads, or mixed-version rollout work.

## 10. Verification ownership

The eight scenarios and exact focused commands in `TASK-005-R1` are the
required Wyrd Red/Green proofs. In addition:

- the fork's explicit-only expiration tests must run at the reviewed fork SHA;
- the implementation report records the new Iceberg and compaction SHAs and
  demonstrates one dependency universe;
- Scenario 1 proves the grants, RLS boundary, cross-tenant denial, atomic claim,
  empty-hot-set result, and PostgreSQL abandonment rule;
- Scenario 3 proves the explicit-ID-only fork behavior and every destructive
  gate;
- Scenario 6 proves the exact hot path transition and deletion ordering;
- Scenario 7 proves the one configured default on local and forwarded entry;
- Scenario 8 proves terminal metadata retention on refusal/failure and deletion
  with successful or already-absent physical cleanup.

Final verification remains the task's scoped repository-native lanes. No
elapsed-time threshold is evidence for the one-statement contract.

## Authority

- `changes/active/forge-concurrent-planning/spec.md` revision 10
- `changes/active/forge-concurrent-planning/revision/TASK-005-iceberg-filtering-across-tiers.md`
  Scenario 4 stop record
- `changes/active/forge-concurrent-planning/tasks/TASK-005-R1-active-table-reader-cut.md`
- `AGENTS.md`
- `architecture/agent-rules.md`
- `architecture/wyrd-design.md`
- `architecture/wyrd-doctrine.mdx`
- `architecture/bifrost-design.md`

