---
id: TASK-005
kind: implementation
status: ready
spec: SPEC-forge-concurrent-planning
spec_revision: 8
requirements: [REQ-002, REQ-014, REQ-015, REQ-016, INV-005, INV-008, INV-009, INV-010, AC-002, AC-006, AC-009, AC-010, AC-011]
depends_on: [TASK-001]
---

# Prove filtering from Scribe hot objects through Iceberg

## Outcome and Value

For a user-configured custom dataset and the built-in spans, points, and
records tables, the same selective queries return exact rows from Scribe hot
objects and after Forge promotion and compaction. Tests prove the declared
sort order, Bloom filters, file and row-group min/max, and page indexes affect
physical reads in both tiers. A trace-ID query is one case in this broader
contract, not its sole proof (REQ-002, INV-005, AC-002).
In particular, `traces.spans.trace_id` is a declared Bloom column: the hot,
promoted, and rewritten reads must prove that its Bloom filter is written and
used for point lookups.
`wyrd_request_id` lookups remain exact and demonstrate row-group min/max
exclusion when the physical ranges are disjoint. Every Parquet field ID comes
from the registered Iceberg table, including fresh signal tables (REQ-015,
REQ-016, INV-010, AC-010, AC-011).
Oracle also acquires the tenant's Iceberg and Scribe cut with one SQL
`SELECT`, while retaining exact reader and hot-object protection through
concurrent Forge publication and cleanup (REQ-014, INV-005, INV-008,
INV-009, AC-006, AC-009).

## Owners, Scope, Consumers, and Prohibited Changes

- The registered Iceberg table assigns physical field IDs. Scribe stamps those
  IDs into files; Oracle reads them from the table; Forge validates the match.
  Scribe and Forge own the physical files and their registered layouts. Oracle
  owns hot and Iceberg scan behavior. The pinned `bohmian-ai/iceberg-rust` fork
  owns the Iceberg Bloom reader and binary page-bound handling.
- `wyrd-spec` owns the lossless binary literal in the closed `ScanLiteral`
  assignment contract, and Oracle owns its classification and evaluation on
  leader and follower paths.
- The custom dataset declares `partition_granularity`, ordered `sort_keys`,
  and `bloom_columns` through a public `TableConfig` registration. Assert the
  server-resolved layout before writing; a built-in layout or test-only
  catalog override cannot stand in for the user's configuration path.
- Configure and exercise the readers Oracle actually executes:
  `HotParquetExec` for Scribe hot files and the Iceberg `ArrowReader` for
  promoted files. Enabling DataFusion session options is not proof that either
  custom reader consumes Bloom filters or page indexes.
- A SQL result alone does not prove pruning. Each physical claim needs a
  selective-versus-unfiltered comparison or direct reader evidence that
  isolates that mechanism. In particular, Bloom proof must use a predicate
  whose min/max overlaps the searched value; the existing test that observes
  a Bloom filter and then prunes by min/max does not prove Bloom consumption.
- `wyrd_request_id` is a UTF-8 managed column, not a declared Bloom column or
  default sort key. Its row-group min/max assertion requires deliberately
  disjoint ranges and cannot imply universal point-lookup pruning.
- Query predicates remain exact after conservative pruning. Missing, malformed,
  or unsupported physical metadata must retain candidate rows, never discard
  matches or fail solely because an optional pruning aid is unavailable.
- Oracle's catalog view is tenant scoped. Forge may use the internal platform
  credential for publication and compaction; an Oracle request uses its
  tenant-bound SQL authority and cannot read another tenant's catalog pointer.
  One returned cut must establish durable, bounded protection for both its
  snapshot and hot objects before exposing their identities. A plain joined
  read without that protection does not meet this outcome.
- No migration, legacy-table repair, new public query contract, blanket
  query-wide pruning switch, or Bloom declaration on a column that lacks an
  approved physical-layout reason. No latency target or benchmark result is
  part of this task. Nothing has shipped; use fresh tables.

## Approach

1. Make fresh registered Iceberg tables the source of physical field IDs for
   signal, managed, and custom fields. Prove Scribe output and Forge promotion
   agree with those IDs without rewriting Iceberg metadata or including IDs
   in the canonical signal fingerprint.
2. Extend the existing Oracle production journeys with deterministic batches
   for the publicly registered custom dataset, spans, points, and records.
   Write enough distinct files, row groups, and pages to make each level
   observable; assert the physical sort order and emitted min/max, Bloom,
   and page metadata.
3. Keep Forge promotion paused while Scribe seals hot files. Assert the cut is
   hot-only, then run an unfiltered baseline and the selective query matrix.
   Disabling compaction alone is insufficient: promotion is independent.
4. Allow exact Forge promotion, verify the snapshot and hot-to-Iceberg
   authority transition, then use test-scoped small compaction settings to
   produce a rewrite. Repeat the same queries on promoted and rewritten cuts.
5. Add the missing lossless binary predicate path and Bloom consumption in
   the executed hot and Iceberg readers. Correct the fork's binary page-bound
   handling and remove Oracle's query-wide `page_index_evaluable` gate once
   the pinned fork passes the focused proof.
6. Exercise tenant-scoped cut acquisition across hot, promoted, and rewritten
   data, including concurrent publication and cleanup. Prove that one SQL
   `SELECT` establishes the protected cut for every table in the query and
   reduces serial database steps, without using elapsed time as a proxy.
7. Run the existing Rust, Python, TypeScript, and MCP canonical journeys and
   the scoped Bifrost gate.

## Ordered Implementation Scenarios

### Scenario 0 — Iceberg assigns field IDs before Scribe writes

**Behavior.** A fresh spans, points, records, or custom table takes every
physical field ID, including nested and Bifrost-managed fields, from its
registered Iceberg schema. Scribe writes those IDs; hot Oracle reads and
Forge promotion accept the same table identity. The canonical signal
fingerprint covers ordered names, physical types, nullability, nesting, and
semantic sensitivity metadata, but no numeric field IDs. Forge still refuses
a file whose IDs disagree with the table. No built-in or managed column
declares an ID and table creation does not rewrite Iceberg's metadata JSON.

**RED.** Add `distributed::iceberg_assigned_field_ids_promote_fresh_signal_tables`
to the Oracle journey. It checks the registered IDs, emitted Parquet IDs,
fresh signal promotion, and mismatch refusal. The current declared-ID
workaround fails the ownership assertion; removing that workaround before
Scribe adopts the table IDs makes fresh signal promotion fail. Run:
`scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=distributed::iceberg_assigned_field_ids_promote_fresh_signal_tables)"'`.

**GREEN.** Use the registered Iceberg schema as the field-ID authority across
table creation, Scribe writing, Oracle reading, and Forge's existing
validation. Remove the declared IDs and metadata-rewrite workaround. Rerun
the focused journey.

**REFACTOR.** Remove only ID-specific declarations, fingerprint encoding,
tests, and conversion branches made obsolete by the single authority. Retain
the canonical field order, types, nullability, nesting, and sensitivity proof.

### Scenario 1 — Hot files use their declared filtering layout

**Behavior.** On a proven hot-only cut, all four table kinds return exact
matches and nonmatches for declared Bloom-key equality, event-time range,
mixed key/time, and applicable null filters. The test demonstrates physical
sort order and positive file, row-group, page, and Bloom exclusions using
fixtures that make each mechanism distinguishable from the others. A Bloom
false positive may retain a group; it must never exclude a match. The custom
dataset's physical evidence matches its caller-declared layout. For
`traces.spans.trace_id`, present-ID lookups return the exact spans, and a
deterministic absent-ID lookup excludes a row group through its Bloom filter
even though that ID lies within the group's min/max bounds. The test verifies
that the probe is Bloom-negative, avoiding a flaky false-positive assertion.
For `wyrd_request_id`, exact present and absent lookups demonstrate row-group
min/max exclusion on disjoint ranges, without claiming Bloom use.

**RED.** Add `distributed::hot_filtering_mechanisms_cover_all_table_kinds`
to the existing Oracle journey. The current hot reader does not show Bloom
consumption, and the closed `ScanLiteral` has no bytes variant, so its binary
Bloom-exclusion assertion must fail even when
residual filtering returns the right rows. The named test must include the
isolated `trace_id` Bloom assertion. Run:
`scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=distributed::hot_filtering_mechanisms_cover_all_table_kinds)"'`.

**GREEN.** Extend the closed `ScanLiteral` contract with lossless bytes and
carry binary equality to the executed hot reader. Add actual Bloom-filter
consumption there, with conservative handling of absent or unsupported
metadata; correct any writer or reader behavior the test proves absent or
wrong. Regenerate contracts and run `mise run codegen:check`. Rerun the exact
test after each correction.

**REFACTOR.** Reuse the existing Oracle cut, telemetry, and Forge promotion
controls; remove duplicate test setup without weakening the separate
mechanism assertions. Delete or rename
`dictionary_bloom_row_group_pruning_contract` because it currently proves
min/max pruning, not Bloom consumption; retain its legitimate min/max proof.
Run the existing test before that correction with
`mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::exec::tests::dictionary_bloom_row_group_pruning_contract)'`.

### Scenario 2 — Binary page bounds cannot break Iceberg reads

**Behavior.** Fixed-size and binary ID predicates preserve exact results and
page selection in the fork, including a conjunction with a selective time
bound. Invalid UTF-8 is treated as bytes; an unsupported physical/logical
pair keeps pages rather than failing or guessing. A custom table sorted by a
binary key also demonstrates binary page selection.
This scenario addresses binary page-index evaluation; the `trace_id` Bloom
lookup is proved separately in Scenarios 1 and 3.

**RED.** Add fork test
`expr::visitors::page_index_evaluator::tests::binary_bounds_preserve_mixed_pruning`.
The pinned evaluator currently errors on fixed-length bounds and can panic on
non-UTF-8 byte bounds. Run from `../iceberg-rust`:
`cargo test -p iceberg --lib expr::visitors::page_index_evaluator::tests::binary_bounds_preserve_mixed_pruning -- --exact`.
Add `distributed::binary_sort_key_page_pruning` to the Oracle journey for a
custom table whose binary sort key makes page exclusion observable. The
current query-wide gate prevents that exclusion. Run:
`scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=distributed::binary_sort_key_page_pruning)"'`.

**GREEN.** Correct the fork reader, pin the tested revision in Wyrd, and
remove Oracle's query-wide page-selection gate. Rerun both focused tests
and Scenario 1.

**REFACTOR.** Delete the obsolete Oracle type-list test and gate commentary;
retain the fork's conservative behavior for types it cannot decode.

### Scenario 3 — Promoted and compacted files keep the same filtering contract

**Behavior.** After actual promotion, and again after an actual Forge rewrite,
the four tables return the same exact query results as the hot-only phase.
Snapshot and file-list evidence prove which tier served each read. Physical
sort order, declared Bloom filters, min/max pruning, and page pruning are
verified on the Iceberg data, including mixed binary-ID/time filters. Local
file pruning and follower row-group pruning are distinguished: follower file
assignments cannot be silently pruned after their signed cut is fixed.
For both promoted and rewritten spans, repeat the present and deterministic
absent `trace_id` probes from Scenario 1 and attribute the absent row-group
exclusion specifically to Bloom consumption.
`wyrd_request_id` lookups repeat the disjoint-range row-group min/max proof
on both promoted and rewritten cuts.

**RED.** Add `distributed::iceberg_filtering_mechanisms_cover_all_table_kinds`
to the same journey owner. The current Iceberg reader does not show Bloom
consumption, and Oracle's binary-predicate gate disables page selection, so
those physical assertions must fail even if exact rows return. The test also
refuses failed promotion or a rewrite that drops the layout or the `trace_id`
Bloom filter. Run:
`scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=distributed::iceberg_filtering_mechanisms_cover_all_table_kinds)"'`.

**GREEN.** Add Bloom-filter reading to the pinned Iceberg fork and pin its
reviewed commit. Correct the smallest owning writer, catalog, promotion,
compaction, or reader behavior shown defective by the test. Keep Forge's
footer/evidence check and Oracle's signed-assignment validation intact.
Rerun Scenarios 0–3.

**REFACTOR.** Share the query expectations across hot, promoted, and rewritten
phases while retaining independent physical evidence for each phase and
mechanism.

### Scenario 4 — One tenant-scoped SQL selection protects the Oracle cut

**Behavior.** On both first and repeated reads, including a query referencing
multiple tables, the request's tenant-scoped authority obtains every
registered table identity, current Iceberg metadata pointer, and unresolved
Scribe hot-file candidate through one SQL `SELECT` for the complete query.
That operation establishes bounded reader protection before returning the
cut. Oracle performs no separate catalog-pointer read, post-protection
pointer recheck, or protection SQL statement. Its serial database steps are
fewer than the current pointer, recheck, and hot-list path; transaction setup
and commit are counted separately from the `SELECT`. Forge continues to
publish and compact with its platform authority. Another tenant's table or
pointer is inaccessible. Promotion, compaction, expiration, cleanup, query
cancellation, node failure, or retry cannot duplicate or lose rows, delete an
object still required by the cut, or leave permanent protection. The Iceberg
metadata document and manifests may still require object-store reads; the
one-`SELECT` claim concerns SQL only.

**RED.** Add `distributed::tenant_scoped_single_select_cut_preserves_hot_and_iceberg_rows`
to the Oracle journey. Assert the exact Oracle SQL statement count and ordered
database steps for first and repeated single- and multi-table reads, plus
tenant denial and deterministically interleaved Forge publication and cleanup
while a cut is active. The current path issues two catalog-pointer `SELECT`s
and a separate hot-list `SELECT`, so the step-count assertion must fail even
if query results remain exact. Run:
`scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=distributed::tenant_scoped_single_select_cut_preserves_hot_and_iceberg_rows)"'`.

**GREEN.** Make the tenant's Oracle cut acquisition one SQL selection with
protection that covers both Iceberg and hot objects until the query releases
it. Keep Forge's internal publication authority and exact hot-to-Iceberg
reconciliation. Rerun this scenario and Scenarios 1 and 3.

**REFACTOR.** Remove Oracle's obsolete global-catalog and second-pointer
query steps only after the concurrency proof establishes that the protected
one-selection cut is sufficient. Keep the production read path and test
instrumentation small and attributable.

## Acceptance Criteria

- All four table kinds have a hot-only cut, a promoted Iceberg cut, and a
  rewritten Iceberg cut established by actual authority and snapshot evidence.
  The same queries return the expected row identities in every cut.
- The custom dataset is registered through a public client with explicit
  partition granularity, sort keys, and Bloom columns. Describe returns the
  resolved layout, and hot, promoted, and rewritten files follow it.
- For both hot and Iceberg tiers, the test proves the physical order of the
  declared sort keys and a positive exclusion attributable to each promised
  pruning mechanism: file-level min/max, row-group min/max, Bloom, and page
  index. Partition exclusion is checked separately and cannot stand in for
  file-level min/max. A positive result from one mechanism never substitutes
  for proof of another. Bloom probes overlap min/max so statistics alone
  cannot satisfy the Bloom assertion.
- `traces.spans.trace_id` Bloom filters are present and consumed for hot,
  promoted, and rewritten files. Present-ID lookups stay exact; a verified
  Bloom-negative absent ID within the relevant min/max bounds excludes a row
  group at each cut. Correct results alone, or exclusion by min/max, file
  pruning, or page indexes, cannot satisfy this criterion.
- Binary IDs, numeric and string keys, time ranges, mixed predicates, misses,
  and applicable null cases remain exact. Unavailable index evidence is
  conservative. Every file's IDs match its registered Iceberg table's IDs;
  no built-in or managed column declares its own ID.
- `wyrd_request_id` present and absent lookups return exact rows on hot,
  promoted, and rewritten cuts and demonstrate row-group min/max pruning
  where ranges are disjoint. No Bloom filter is added for that column.
- Oracle uses one tenant-scoped SQL `SELECT` per query cut, including the first
  read and multi-table queries, with no additional pointer or protection
  statement. Evidence counts SQL statements and transaction setup/commit
  separately and shows fewer serial database steps than the current path. It
  does not assert a millisecond latency target or claim one network round trip.
- The selection protects its returned snapshots and hot objects before they
  can be reclaimed. Cross-tenant pointer access is denied; concurrent Forge
  publication, compaction, expiration, cleanup, cancellation, node failure,
  and retries preserve exact results and release protection safely.
- No migration or legacy compatibility path is added. Existing language and
  MCP journeys remain green.

## Expected Write Set and Consumer Closure

Likely owners are the existing `wyrd-testing` Oracle journeys and Bifrost
test support; `wyrd-spec` assignment literals, the Scribe Parquet writer,
`tables/*` canonical and managed declarations, Forge promotion/rewrite,
Oracle hot and Iceberg scan, `catalog/bifrost_catalog.rs` table creation,
tenant-scoped catalog and reader protection, and the pinned Iceberg fork.
`Cargo.toml` and
`Cargo.lock` carry any tested fork pin. First-class SDKs and MCP are
regression consumers, not duplicate filtering implementations.

## Verification and Evidence

- Run each exact RED/GREEN command above. The new journeys must report the
  actual hot, promoted, and rewritten cut; expected and returned row IDs;
  physical sort evidence; proof that each executed reader consumed its index;
  and per-mechanism pruning evidence. Counters without tier attribution or a
  fixture that another mechanism can satisfy do not close an assertion.
- The one-selection journey must record Oracle SQL statement count, ordered
  database steps, tenant role, protected cut, and Forge cleanup outcome.
  Keep transaction setup/commit and object-store reads visible but distinct;
  defer elapsed-time measurement to the optimization benchmark change.
- Run the fork's relevant reader tests, `mise run fmt`, `mise run lints`, and
  `mise run codegen:check`, and `mise run verify:bifrost` for Wyrd. Run the separately defined
  `mise run test:bifrost:journey:forge:production-geometry` because it is
  outside `verify:bifrost` and was reported red.
- Record the reviewed fork commit and Wyrd pin. The existing writer and
  catalog unit tests remain supporting evidence; only the real journeys
  establish end-to-end behavior.

## Material Stop Conditions

- If a pruning claim cannot be attributed to its mechanism with the existing
  telemetry, use focused reader evidence in the owning test. Do not weaken
  the claim to "the query returned the right rows" or relabel another
  mechanism's counter.
- If Scribe cannot obtain the registered Iceberg table's IDs before writing,
  stop and report that evidence. Do not restore declared IDs, rewrite
  Iceberg's metadata, or relax Forge's validation.
- If one SQL selection cannot establish safe retention for both the returned
  Iceberg and hot objects, do not delete revalidation or weaken cleanup.
  Return for specification authority with the failing race evidence.

## Authority Links

- [Approved revision 8 spec](../spec.md): REQ-002, REQ-014–016,
  INV-005, INV-008–010, AC-002, AC-006, AC-009–011.
- [AGENTS.md](../../../../AGENTS.md),
  [agent rules](../../../../architecture/agent-rules.md),
  [Bifrost design](../../../../architecture/bifrost-design.md), and
  [Iceberg reference](../../../../architecture/references/domain/iceberg.md).
