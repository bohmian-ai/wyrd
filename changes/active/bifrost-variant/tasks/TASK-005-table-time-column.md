---
id: TASK-005
kind: implementation
status: proposed
spec: SPEC-bifrost-variant
spec_revision: 23
requirements: [REQ-036, REQ-037, REQ-038, REQ-039, REQ-040, INV-012, AC-016]
depends_on: [TASK-003]
parent_task:
remediates: []
---

## Outcome and Value

Every Bifrost table partitions, sorts, bounds, and prunes on one time column of
its own content. The managed `wyrd_event_time` column, its acceptance window,
its exemption flag, and `WYRD_VALA_400_EVENT_TIME_OUT_OF_RANGE` no longer
exist. A user who filters `start_time_unix_nano`, `time_unix_nano`,
`created_at`, or their own declared column gets partition and file pruning on
the hot and published tiers without knowing any managed column. A table that
declares no time column partitions on `wyrd_ingested_at`.

By human direction this is one task: the change is one contract (the time
axis) that every tier, wire, SDK, and document must agree on at once, and no
intermediate state is shippable.

## Owners, Scope, Consumers, and Prohibited Changes

- **Execution base.** Begin from the current TASK-003 branch head; the
  telemetry query-capacity fixture
  (`crates/wyrd/wyrd-testing/src/bin/bifrost_query_capacity/telemetry.rs`)
  belongs to TASK-003 and is consumed here for AC-016's last sentence.
- **Contract owner.** `crates/wyrd-spec`: `PhysicalLayoutWire` gains optional
  `time_column` (REQ-038); `TimeGranularityWire`, `RegisterTableRequest`, and
  `BifrostTableDescription` docs and `managed_candidates` lose
  `wyrd_event_time`; the `EventTimeOutOfRange` catalogue entry, the
  `WYRD_EVENT_TIME` constant, and every schema mention go. Generated schemas
  regenerate; none is hand-edited.
- **Layout owner.** `vala-bifrost-redux` `catalog/layout.rs`: the resolved
  time partition carries its source column; the Iceberg partition field, the
  default first sort key, physical-table validation, and `describe` derive
  from it. Built-in layouts (`tables/**`) declare REQ-037's column; every
  built-in's other sort keys, Bloom columns, and granularity stay as they are.
- **Built-in schema owners.** `tables/traces`, `tables/logs`,
  `tables/metrics`: OTLP instant fields become `Timestamp(ns, "+00:00")`
  (Iceberg `timestamptz_ns`), names unchanged, durations unchanged; the logs
  projection stores observed time when `time_unix_nano` is 0. `tables/audit`:
  new non-null `created_at` content column carrying the PostgreSQL decision
  time; audit hash inputs unchanged. Verification detail tables partition on
  `wyrd_ingested_at`.
- **Write path.** Scribe admission, stamping, `seal_key` time-partition
  planning, memtable, WAL slices, Parquet row-group time bounds, Parquet
  encoding overrides, `file_list` min/max, staging, and promotion key on the
  time column at either precision and either timestamp type. The 32-partition
  per-request bound stays and refuses the whole request (REQ-039). No managed
  column is writer-supplied; any `wyrd_*` input column is refused as
  reserved.
- **Read path.** Oracle hot-file statistics, published-file bounds from the
  manifest (both `timestamp`/`timestamptz`, µs and ns), live Scribe route
  pruning, and the `COUNT(*)` column closure use the time column. Forge
  promotion bound checks and rewrite grouping use the table's partition
  source column.
- **Postgres.** `vala.file_list` time-bound columns and the functions that
  return them keep their role under neutral names; Bifrost is unshipped, so
  migrations are edited in place only where the repository's migration rules
  allow it, otherwise added.
- **Server consumers.** `wyrd-server` verification (results, Drift and Eval
  windows, Eval span reads), audit projection, gateway capture, and
  `wyrd-sql` verifier-run input partition selection use the table's time
  column.
- **Clients.** `wyrd-client` (`TableConfig` layout, reserved-column checks),
  `wyrd-types` (`writable_schema` without event time), `wyrd-queue`, Python
  and TypeScript SDK layout surfaces, stubs, and declarations expose
  `time_column` and drop every `wyrd_event_time` mention.
- **Docs.** `architecture/bifrost-design.md`, `architecture/wyrd-design.md`,
  `architecture/references/domain/iceberg.md`,
  `telemetry-observations.md`, and the `docs/` Bifrost pages describe the
  time column; the stale `Day(event_time)` claims go.
- **Prohibited.** No compatibility alias, shim, or fallback reader for
  `wyrd_event_time`; no acceptance window on the time column; no second
  partition field; no new managed column; no change to Variant, shredding,
  Bloom sizing, or retention behavior; no migration of existing data.

## Approach

1. Make the contract change in `wyrd-spec` and the resolved layout: one time
   column per table, carried by the layout, defaulting to `wyrd_ingested_at`.
2. Move every built-in to its REQ-037 column, with the OTLP nanosecond
   timestamp types, the log observed-time substitution, and audit
   `created_at`.
3. Re-key the Scribe write path and hot file list on the time column; delete
   the window, exemption, error, and managed column.
4. Re-key Oracle and Forge bounds, pruning, and validation; extend manifest
   bound decoding to every timestamp form.
5. Move server, client, SDK, and docs consumers; regenerate contracts.
6. Prove pruning end to end on each tier and SDK, then rerun the telemetry
   fixture.

## Ordered Implementation Scenarios

### Scenario 1 — The layout carries the table's time column

**Behavior.** A resolved layout names its time column; the Iceberg partition
field is `hour`/`day` of that column; the default first sort key is that
column descending, nulls last; a layout without `time_column` resolves to
`wyrd_ingested_at`. An absent, nullable, non-timestamp, or `TIMESTAMP_TZ`
column is refused with `WYRD_VALA_400_BIFROST_INVALID_PHYSICAL_LAYOUT` naming
the field. A repeat registration with a different time column conflicts.
(REQ-036, REQ-038)

**RED.** Unit tests in `catalog::layout::tests` fail because the layout has no
time column and the partition spec is hard-wired to `wyrd_event_time`:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=catalog::layout::tests::time_column_drives_partition_and_sort)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=catalog::layout::tests::invalid_time_column_is_refused_naming_the_field)'
```

**GREEN.** Add `time_column` to `PhysicalLayoutWire` and the resolved layout;
derive the partition spec, sort default, and validation from it; regenerate
schemas.

**REFACTOR.** Delete `WYRD_EVENT_TIME`-specific layout helpers once nothing
names them.

### Scenario 2 — Built-ins declare their own time columns

**Behavior.** Each built-in's Iceberg partition source column and first sort
key is its REQ-037 column; OTLP instant fields are nanosecond UTC timestamps;
the audit log has `created_at`; no physical schema contains
`wyrd_event_time`. (REQ-037, INV-012)

**RED.** A table-driven unit test over `BUILTIN_TABLES` fails because every
built-in partitions on `wyrd_event_time`:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=tables::tests::every_builtin_partitions_and_sorts_on_its_time_column)'
```

**GREEN.** Change the built-in layouts, OTLP field types and projections, and
audit schema and projection; remove the managed column declaration.

**REFACTOR.** Remove `past_event_time_exempt` and its plumbing.

### Scenario 3 — Logs with no event time use the observed time

**Behavior.** An OTLP log record with `time_unix_nano = 0` stores its observed
time as `time_unix_nano` and keeps `observed_time_unix_nano`. (REQ-037)

**RED.**

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=tables::logs::projection::tests::unset_time_takes_the_observed_time)'
```

fails because the projection copies 0.

**GREEN.** Substitute in the logs projection.

**REFACTOR.** None expected.

### Scenario 4 — Scribe accepts any instant and bounds fan-out

**Behavior.** A write whose time column is years in the past or future is
accepted into its own partition; a request spanning 33 partitions is refused
whole and writes nothing; a caller batch with any `wyrd_*` column is refused as
reserved; hot files and `file_list` record the time column's min and max at
µs and ns precision. (REQ-039, REQ-040)

**RED.** Existing window tests in `scribe::execution_lanes::tests` are replaced
by tests that fail while the window and the µs-only `wyrd_event_time` reader
remain:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=scribe::execution_lanes::tests::any_time_column_instant_is_accepted)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=scribe::seal_key::tests::thirty_three_partitions_refuse_the_request)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=scribe::parquet_writer::tests::row_group_bounds_read_the_time_column)'
```

**GREEN.** Key stamping, partition planning, row-group bounds, encoding
overrides, staging, and promotion on the resolved time column; delete the
window, exemption, and error.

**REFACTOR.** Collapse the stamping path now that only `wyrd_ingested_at` is
stamped.

### Scenario 5 — A time-range query prunes on every tier

**Behavior.** A range filter on the time column of a span table, a log table,
and a user table skips out-of-range partitions and files on the hot and
published tiers and returns exactly the rows of an unpruned scan; published
bounds decode from `timestamp`, `timestamptz`, `timestamp_ns`, and
`timestamptz_ns` manifests; `COUNT(*)` reads the time column. (REQ-036,
REQ-040, INV-012, AC-016)

**RED.** Journeys fail because pruning keys on `wyrd_event_time`:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && \
  mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey \
  --run-ignored=all -E "test(=oracle::published::time_column_range_prunes_published_files)"'
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && \
  mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey \
  --run-ignored=all -E "test(=otlp::trace_export::span_start_range_prunes_hot_and_published)"'
```

Confirm the final module paths with `mise exec -- cargo nextest list` before
recording them.

**GREEN.** Re-key Oracle hot statistics, manifest bound decoding, live route
pruning, the `COUNT(*)` closure, and Forge promotion checks.

**REFACTOR.** Remove event-time-specific statistics types that the general
time-column path subsumes.

### Scenario 6 — Each SDK declares a time column and reads a pruned range

**Behavior.** The Rust, Python, and TypeScript SDKs register a user table with
`time_column`, write rows across several partitions, read back a pruned time
range, and see `time_column` in `describe`; a table registered without one
reports `wyrd_ingested_at`; an invalid `time_column` is refused naming the
field. (REQ-038, AC-016)

**RED.**

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && \
  mise exec -- cargo nextest run --locked -p wyrd-client --test pg_bifrost_e2e -P journey \
  --run-ignored=all -E "test(=user_table_time_column_prunes_a_time_range)"'
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && \
  cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration \
  tests/integration/bifrost/test_table_time_column.py'
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && \
  cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/table-time-column.test.ts'
```

(`mise run ts:build ts:build:testing` first for the TypeScript journey.)

**GREEN.** Expose `time_column` on each SDK's layout surface, stubs, and
declarations through `wyrd-client`; drop event-time mentions.

**REFACTOR.** None expected.

### Scenario 7 — Server consumers read the table's time column

**Behavior.** Verification results, Drift and Eval observation windows, Eval
span reads, verifier-run input partition selection, audit projection, and
gateway capture behave as before with the time column in place of
`wyrd_event_time`. (REQ-037)

**RED.** The existing server journeys fail to compile or query once the
column is gone:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && \
  mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey \
  --run-ignored=all'
```

**GREEN.** Move each consumer to its table's time column.

**REFACTOR.** None expected.

## Acceptance Criteria

- AC-016 holds in full.
- No tracked source, schema, document, stub, or SDK names `wyrd_event_time`,
  `EventTimeWindow`, `event_time_past_window_secs`,
  `event_time_future_window_secs`, `past_event_time_exempt`, or
  `WYRD_VALA_400_EVENT_TIME_OUT_OF_RANGE` (`git grep` returns nothing outside
  `changes/`).
- Each REQ-037 built-in's Iceberg partition source and first sort key is its
  listed column; its other sort keys, Bloom columns, and granularity are
  unchanged.
- Existing Bifrost, OTLP, verification, audit, and gateway journeys pass with
  unchanged results.
- The telemetry query-capacity fixture runs on both tiers with every exact
  answer passing, and a span-time window query prunes partitions (fewer files
  or row groups scanned than an unfiltered scan, read from the report's scan
  counters).

## Expected Write Set and Consumer Closure

- `crates/wyrd-spec/src/vala/{api.rs,error.rs,managed_columns.rs,mod.rs,
  assignment_authority.rs,trace/trace_summary.rs,logs/record.rs}`,
  `crates/wyrd-spec/schemas/**` and `tests/schemas/**` (regenerated),
  `crates/wyrd/wyrd-tonic/proto/wyrd.v1.proto` where it documents the column.
- `crates/vala/vala-bifrost-redux/src/{catalog,tables,scribe,oracle,forge,
  gate,parquet}/**`, its `tests/` and `benches/bifrost_reads`.
- `crates/vala/vala-sql/{migrations,src}/**` (file list bounds),
  `crates/wyrd/wyrd-sql/{migrations,src}/**` (verifier-run input time).
- `crates/wyrd/wyrd-server/src/{verification,gateway,config.rs,boot,http/otlp.rs}`.
- `crates/shared/{wyrd-client,wyrd-types,wyrd-queue}/**`,
  `sdks/wyrd-sdk-python/**`, `sdks/wyrd-sdk-ts/**`.
- `crates/wyrd/wyrd-testing/{src,tests}/**`, including the query-capacity
  bench.
- `architecture/{bifrost-design.md,wyrd-design.md}`,
  `architecture/references/domain/{iceberg.md,telemetry-observations.md}`,
  `docs/src/content/docs/bifrost/**`.

## Verification and Evidence

- Red-Green: the focused commands in each scenario.
- Final aggregate: `mise run gate` (the change crosses contract, server,
  storage, SDK, and docs boundaries; it includes `check`, `test:rust`,
  `test:bifrost:gate`, `py:test:integration`, `ts:test:integration`,
  `docs:check`, and `codegen:check`).
- Outside the gate: `mise run bench:bifrost:query-capacity -- --fixture
  telemetry` (both tiers), recorded with the pruning evidence above.

## Material Stop Conditions

- The pinned iceberg-rust cannot create, plan, or prune an `hour`/`day`
  transform on a `timestamp_ns`/`timestamptz_ns` or `timestamp` source column
  (`SPEC_REVISION_REQUIRED` if it changes the column types of REQ-037).
- A built-in has a consumer that needs a time column other than its REQ-037
  one, or a verification detail table needs partition alignment with its
  parent result.
- Audit hash inputs would have to change to carry `created_at`.
- A migration cannot be edited in place under the repository's migration
  rules and the added migration would need a data backfill.

## Authority Links

- `changes/active/bifrost-variant/spec.md` revision 23 (REQ-036 to REQ-040,
  INV-012, AC-016)
- `AGENTS.md`
- `architecture/bifrost-design.md`
- `architecture/references/domain/iceberg.md`
- `architecture/references/languages/testing-workflows.md`
