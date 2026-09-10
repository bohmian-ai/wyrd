---
id: ORACLE-LOCAL-T01-F1
title: Remove orphaned Bifrost OLAP machinery
kind: follow-up
status: ready
depends_on: [ORACLE-LOCAL-T01-R1]
skill: wyrd-implement
---

# Remove orphaned Bifrost OLAP machinery

Required execution skill: `$wyrd-implement`.

## Outcome

Fresh Wyrd databases no longer create the unused refresh-epoch, declared-index,
entity-time-bound, or projection-substitution schema. Their unreachable Rust SQL
APIs, row types, and Redux metadata are removed while the live Bifrost table
catalog and current Oracle pruning, caching, and storage paths remain unchanged.

This is follow-up maintenance discovered while reviewing Oracle admission. The
machinery became orphaned before the admission redesign and is not part of the
original Oracle behavior change.

## Confirmed repository facts

- The listed SQL APIs have no production, test, generated-contract, HTTP, SDK,
  CLI, or MCP callers in the current repository.
- `EntityBoundsMapping` is assigned into built-in definitions but never read or
  invoked.
- `vala-sql` is unpublished and has only workspace path dependents in the
  repository.
- No release containing these migrations has shipped, so there is no in-place
  migration compatibility obligation.
- Current Bifrost correctness uses `vala.bifrost_tables`, immutable pinned file
  facts, `PhysicalLayout`, Iceberg/provider statistics, bounded Parquet metadata
  caching, and DataFusion or hot-file row-group pruning.
- Entity-derived time-window lookup and LookupSet projection substitution are
  absent optimizations, not working behavior preserved by the dormant schema.
- Commit `228548cb0` removed the legacy refresh-epoch reader and projection
  matcher/rewriter. Entity-bound writes had already been removed by `a0a17e82f`.

## Selected cleanup

Rewrite the unshipped migration baseline rather than creating then dropping the
obsolete objects:

- Remove `vala.refresh_epochs` and only its associated policy, foreign key, and
  grant from `20260619000001_olap_minimal.sql`.
- Preserve every `vala.bifrost_tables` column, constraint, index, policy, and
  grant in that migration.
- Delete `20260820000000_olap_indexes_entity_bounds.sql` in full.
- Delete `20260822000000_olap_projections.sql` in full.
- Update the initial-migration SHA-256 assertion in `vala-sql/src/lib.rs` to the
  rewritten baseline.

## Rust cleanup

Remove the unreachable `vala-sql` operations and their now-unused imports:

- `delete_table`
- `bump_epoch`
- `current_epoch`
- `declare_index`
- `update_index_state`
- `list_indexes_for_table`
- `record_entity_bounds`
- `entity_bounds_for`
- `list_by_source`

Remove these unused row types:

- `DeclaredIndexRow`
- `EntityTimeBoundsRow`
- `RefreshEpochRow`
- `ProjectionCandidateRow`

Remove `EntityBoundsMapping`, `BuiltinTableDefinition.entity_bounds_mapping`,
`DomainTable::entity_bounds_mapping`, generic definition wiring, and the
overrides in `tables/dev/agent_traces.rs` and `tables/traces/spans.rs`.

Delete `delete_table` entirely. Wyrd currently exposes no supported table-delete
lifecycle; keeping an unreachable partial workflow is not a substitute for
designing that lifecycle when it is needed.

## Preserved behavior and constraints

- Preserve `vala.bifrost_tables`, `BifrostTableRow`, `upsert_table`,
  `get_by_fqn`, and `list_tables_for_tenant`.
- Preserve table registration and authority integration.
- Preserve `PhysicalLayout::bloom_columns`, native Parquet Bloom writing,
  Iceberg/provider statistics, immutable hot-file descriptors, Oracle
  pre-footer and row-group pruning, `BifrostStorage`, `ParquetMetadataCache`,
  and ordinary DataFusion projection.
- Classify generic uses of projection, index, Bloom, bounds, or rollup by their
  call path; do not delete current behavior by name.
- Add no replacement index catalog, refresh mechanism, projection matcher,
  entity lookup, compatibility alias, dependency, migration, or permanent
  name-ban check.
- Do not change Oracle admission, caching, pruning, storage, or query semantics.

## Acceptance criteria

- AC-F1-001: a fresh migrated database contains the unchanged live
  `vala.bifrost_tables` schema and does not contain `vala.refresh_epochs`,
  `vala.olap_indexes`, `vala.entity_time_bounds`, `vala.olap_projections`,
  `vala.check_index_state_transition()`, or the associated trigger.
- AC-F1-002: no RLS policy, grant, constraint, index, trigger, or function owned
  solely by the removed relations remains.
- AC-F1-003: all listed unused SQL methods, row types, imports, and
  `EntityBoundsMapping` wiring are absent, with no remaining production or test
  consumer.
- AC-F1-004: existing Bifrost catalog registration, lookup, listing, migration,
  pruning, cache, and storage tests continue to pass without behavioral changes.
- AC-F1-005: the initial migration checksum guard matches the rewritten file,
  and no forward cleanup migration is added.
- AC-F1-006: the cumulative diff contains no Oracle admission, current pruning,
  cache, storage, public contract, or unrelated cleanup change.

## Expected write set

- `crates/vala/vala-sql/migrations/20260619000001_olap_minimal.sql`
- `crates/vala/vala-sql/migrations/20260820000000_olap_indexes_entity_bounds.sql` — delete
- `crates/vala/vala-sql/migrations/20260822000000_olap_projections.sql` — delete
- `crates/vala/vala-sql/src/lib.rs`
- `crates/vala/vala-sql/src/queries/olap_catalog.rs`
- `crates/vala/vala-sql/src/row_types/olap_catalog.rs`
- `crates/vala/vala-bifrost-redux/src/tables/mod.rs`
- `crates/vala/vala-bifrost-redux/src/tables/dev/agent_traces.rs`
- `crates/vala/vala-bifrost-redux/src/tables/traces/spans.rs`
- Existing focused SQL migration or catalog tests only when necessary to prove
  the acceptance criteria.

## Verification

Use the repository-managed PostgreSQL wrapper through the owning `mise` tasks.
Run:

```bash
mise run fmt
mise run lints
mise run test:sql
mise run test:bifrost
git diff --check
```

Also inspect a freshly migrated database and record direct relation/function
absence evidence for AC-F1-001 and AC-F1-002. Record a final consumer search for
the removed Rust symbols and confirm the diff contains no unrelated Oracle,
cache, pruning, or storage changes.
