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

## Follow-up evidence

Candidate range `93a6d14a4..HEAD` on `oracle-distributed`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| AC-F1-001 | `crates/vala/vala-sql/migrations/20260619000001_olap_minimal.sql` keeps only `vala.bifrost_tables`; `20260820000000_olap_indexes_entity_bounds.sql` and `20260822000000_olap_projections.sql` deleted (`75ac55691`) | Fresh `db:migrate:all:inner` database: `pg_class` in schema `vala` matching the four removed names plus `bifrost_tables` returns exactly `bifrost_tables|r`, `bifrost_tables_data_tenant_id_fqn_key|i`, `bifrost_tables_fingerprint_idx|i`, `bifrost_tables_pkey|i`. `pg_proc` in `vala` returns only `audit_outbox_append_only`, `assert_maintenance_lease_fence`, `assert_scribe_publication_fence`, `oracle_epoch_protection_count`, `append_oracle_admission_recovery_audit` — no `check_index_state_transition` | PASS |
| AC-F1-002 | Same migration rewrite: the `refresh_epochs` policy, FK, and its half of the grant went with the table; no `DROP` was added | Same database: non-internal triggers in `vala` are `audit_outbox_append_only` only; `pg_policies` lists 17 `tenant_isolation` rows, none for a removed relation. `vala.bifrost_tables` retains all ten columns with `status` defaulting to `'active'` and grants `wyrd_app` SELECT/INSERT/UPDATE/DELETE plus `wyrd_platform_admin` SELECT | PASS |
| AC-F1-003 | `queries/olap_catalog.rs` keeps only `upsert_table`, `get_by_fqn`, `list_tables_for_tenant`; `row_types/olap_catalog.rs` keeps only `BifrostTableRow`; `EntityBoundsMapping`, the `BuiltinTableDefinition` field, the `DomainTable` default, the `definition::<T>` wiring, and both table overrides removed (`75ac55691`) | `git grep -n "EntityBoundsMapping\|entity_bounds_mapping" -- crates` returns nothing. `git grep` for `refresh_epochs`, `olap_indexes`, `entity_time_bounds`, `olap_projections`, `check_index_state_transition` across `crates python docs architecture scripts mise.toml` returns nothing | PASS |
| AC-F1-004 | No live catalog, pruning, cache, or storage path changed | `mise run test:bifrost` — `9/9 lanes passed`; integration:redux 972/972, integration:sql 107/107, integration:server 67/67, journey 6/6 capabilities (sdk 15, forge 13, scribe 20, oracle 28, server 4, mcp 7), journey:python and journey:typescript passed | PASS |
| AC-F1-005 | `crates/vala/vala-sql/src/lib.rs` asserts `a2df17d99f1ee29c1ead1fca2ea30c0062cd918d9d5b5e3f41f30ac6cdc259ef`, the SHA-256 of the rewritten baseline; no new migration file added | `mise run test:sql` — `forge_and_oracle_migrations_have_unique_forward_owners`, `migrations_embed_count_matches_files`, and `migration_filenames_match_timestamp_versions` all pass | PASS |
| AC-F1-006 | `git diff --stat 93a6d14a4..HEAD` touches only the expected write set plus the two defects recorded below | `git diff --check d3888ddae..HEAD` clean; no Oracle admission, pruning, cache, storage, or public-contract file appears in the range | PASS |

### Commands run

```
mise run fmt
mise run lints
mise run test:sql
mise run test:bifrost
git diff --check d3888ddae..HEAD
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && psql "$WYRD_TEST_DATABASE_ADMIN_URL" ...'
mise exec -- cargo nextest run --locked -p wyrd-sql --lib \
  -E 'test(=tests::transaction_discipline_is_documented)'
```

### Two defects found and fixed outside the expected write set

Both were pre-existing and both blocked verification of this task, so each is
its own commit, separate from the cleanup.

- `a6e28b549` — `crates/vala/vala-bifrost-redux/src/resources.rs`. The T01-R1
  instrumentation gate (`617155857`) also captured `OnceLock`, `AtomicU8`,
  `AtomicUsize`, and `Ordering`, which non-test governor code uses for the
  Oracle class split, the scratch namespace sequence, the reservation state
  machine, and every peak-byte counter. A default-feature
  `cargo check -p vala-bifrost-redux` failed with fourteen unresolved names;
  every `mise` lane hid it by compiling with tests or `test-support` enabled.
- `1ccfc7b10` — `crates/wyrd/wyrd-sql/src/lib.rs`.
  `transaction_discipline_is_documented` matched whole sentences against
  hard-wrapped Markdown and rustdoc, so it asserted where the wrap fell as much
  as what the prose said. It was already red at `d3888ddae`:
  `architecture/v1/00-foundations/sql-foundation.md:64` states the cross-crate
  invariant across a line break. Each source is now compared with doc markers
  stripped and whitespace collapsed. Falsified both ways: replacing the
  invariant's wording fails with the naming message, and re-wrapping the same
  sentence differently passes.

### Non-goals confirmed excluded

No replacement index catalog, refresh mechanism, projection matcher, entity
lookup, compatibility alias, dependency, forward cleanup migration, or
name-ban check was added. `PhysicalLayout::bloom_columns`, native Parquet
Bloom writing, Iceberg/provider statistics, hot-file descriptors, Oracle
pre-footer and row-group pruning, `BifrostStorage`, `ParquetMetadataCache`,
and DataFusion projection are untouched.

### Material limit

`vala.refresh_epochs` and the two deleted migrations are removed from the
baseline rather than dropped forward. Any database already migrated past
`20260822000000` keeps those relations and will fail sqlx's checksum
verification on `20260619000001`; the recorded repository fact is that no
release containing them has shipped, so the only affected databases are local
and are recreated per run by `scripts/postgres/with-test-postgres.sh`.
