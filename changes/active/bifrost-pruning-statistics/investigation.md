# Investigation: statistics ownership and DataFusion pruning warnings

Investigated 2026-11-03 against the current TASK-004 worktree. This is a
diagnosis, not implementation or specification approval.

## Findings

The initial explanation was incomplete. Wyrd does not need to replace
Iceberg's file statistics or DataFusion's Parquet pruning. It does need an
adapter for its combined published and live sources. That adapter supplies
statistics for objects that are not ordinary Iceberg files: live batches and
live time-partition routes.

Two independent defects are present:

1. **An empty live-route selection reaches DataFusion's statistics conversion.**
   This reproduces the paired warnings for every reported predicate column,
   without any missing file statistics. The result is an empty selection; the
   warning itself does not mean this path scans extra files.
2. **Mixed exact and unavailable bounds fail conversion in the pinned fork.**
   This is a real lost-pruning bug. It was fixed, with a regression test, on
   October 29 and removed with a broader rollback later that day. It is not
   evidence that nobody previously caught the bug.

The current persisted span file and its Iceberg manifest both contain lower
and upper bounds for all four reported columns. No change to statistics
collection is justified by this incident's available evidence.

## Who owns each part

| Stage | Existing owner and actual behavior |
|---|---|
| Writing Parquet | Wyrd configures the existing Parquet writer with page and column-chunk statistics enabled. The library computes the values. |
| Building Iceberg file metrics | Wyrd calls Iceberg's footer-to-DataFile conversion, retaining its column sizes, counts, and bounds. It does not implement its own file min/max calculation. |
| Selecting published files | Wyrd calls Iceberg's file planner with the pinned snapshot, current schema, and supported predicates. Iceberg evaluates manifest-entry metrics before returning file tasks. |
| Reading persisted files | Wyrd constructs a DataFusion Parquet scan over Iceberg-selected files and unpublished hot objects. DataFusion owns decoding, row-group pruning, and the underlying scan machinery. Wyrd installs its tenant-proof reader and caches. |
| Reading in-memory batches | Scribe records primitive bounds using DataFusion min/max accumulators. Its custom memory source uses DataFusion's pruning predicate to avoid scanning disjoint batches. |
| Selecting live routes | Wyrd supplies each route's time-partition interval, then asks DataFusion to select potentially matching routes. These are routing facts, not file statistics. |

This division is consistent with the owning architecture: one query combines a
pinned published cut and authenticated Scribe live sources; every opened file
must prove its tenant; persisted sources share a native DataFusion read plan.
The pinned Iceberg DataFusion leaf normally executes its own `to_arrow()`
reader. It exposes no DataFusion Parquet reader-factory hook on that leaf.
Replacing Wyrd's scan with that leaf is therefore not a drop-in correction:
the tenant-proof, shared-reader, live-source, and source-accounting obligations
would still need implementation.

The built-in DataFusion memory statistics helper computes row counts, bytes,
and null counts, but not min/max bounds. Thus live batch bounds are an explicit
optimization supplied by Wyrd, not automatic Parquet statistics that were
forgotten. Scribe currently leaves string batch bounds unknown. That does not
imply that persisted string statistics are missing.

## Published statistics are used before the adapter handoff

`OracleTableProvider::planned_files` obtains an Iceberg scan, calls
`published_files`, appends unpublished hot objects, performs conservative
pre-footer time pruning, and obtains cached footers. `persisted_input` builds
the native Parquet scan from the planned files.

Iceberg's `plan_files()` applies partition and inclusive file-metric predicates
before emitting `FileScanTask`s. Those tasks do not carry general per-column
lower/upper-bound maps. Wyrd maps the tasks to paths, sizes, and row counts,
with unavailable event-time bounds at that second stage. Consequently not all
manifest bounds are supplied again as DataFusion file statistics, but it would
be incorrect to say they were never used or that file pruning was disabled.

DataFusion subsequently loads Parquet metadata and invokes its row-group
statistics pruner. Wyrd deliberately marks merged scan statistics inexact so
an aggregate cannot answer from metadata alone and bypass the required file
tenant proof. That security boundary is intentional; restoring exact aggregate
statistics indiscriminately would be an unsafe fix.

No additional metadata-forwarding defect was established. There may be further
optimization opportunities, but they must demonstrate avoided work beyond the
existing Iceberg and Parquet pruning before adding another metadata path.

## The empty-route causal chain

1. A nonempty live route list creates a `LiveScribeExec` with the session's
   target output partition count. The route count need not equal that count.
2. `LiveScribeExec::resolved` deals routes to output partition P with
   `skip(P).step_by(partition_count)`. A partition can legitimately receive
   nothing. Static filter pushdown can also remove every route before execution.
3. When a predicate exists, `resolved` still calls
   `LiveTableRoutes::selected_by` for an empty share.
4. `selected_by` builds a `PrunableStatistics` with zero containers and calls
   DataFusion's pruning predicate. It has no empty-input guard.
5. The dependency attempts `ScalarValue::iter_to_array` on an empty iterator,
   which returns an error. Its warning says "min" for both minimum and maximum
   requests, then returns unavailable statistics.

An isolated probe linked against the current pinned library used a nanosecond
timestamp, the actual numeric `kind` shape, and the two string columns. A real
DataFusion pruning predicate over zero containers produced eight warnings:
two each for `start_time_unix_nano`, `service_name`, `kind`, and
`gen_ai_operation_name`. Dividing one route across four output partitions by
the production dealing rule produced three empty shares and 24 warnings.

This is an executable reproduction of a reachable production path and explains
the reported column pattern. It does not retroactively identify the route
count and output partition of each original request. The original process's
stderr was a user terminal, so its historical log stream was not captured or
intercepted during this investigation.

The other Wyrd `PrunableStatistics` call is `BatchPruning::keep`, which already
returns immediately for zero batches. DataFusion's inspected file pruner uses
one container per file, so it cannot produce an empty-container list at that
call site. This narrows the empty-input problem to the unguarded route adapter
among the inspected production callers.

## The mixed-bound regression and its history

In the current fork, `get_exact_column_statistics` substitutes untyped
`ScalarValue::Null` for unavailable exact bounds. Combining that with a typed
exact bound fails `iter_to_array`, so the requested bounds become unavailable.
Inexact bounds must remain unknown for this pruner; they must not be promoted
to exact to work around the failure.

The probe observed:

| Input | Observed conversion warnings |
|---|---:|
| One or two all-unknown containers | 0 |
| One time-bounded route, other columns unknown | 0 |
| Exact timestamp followed by unknown timestamp bounds | 2 |
| Unknown timestamp bounds followed by exact timestamp | 2 |
| Zero containers, four predicate columns | 8 |

Earlier probes also showed that typed unknown entries successfully combine
with integer, timestamp, and string scalars in either order.

Git history supplies the missing explanation of prior coverage:

- Wyrd commit `3b7f9d5af2251fa98403b798a44aaac7f221a2a9`, October 29 at
  17:36 -0400, pinned DataFusion to
  `8d6d6bf58c3a64242d2193a0cff0b8ad3a050599` and added
  `nanosecond_pruning_preserves_unknown_container_bounds`.
- That fork revision differs from current pin
  `19796a13ed46877cf998013886947b28a6f774b3` only in
  `datafusion/common/src/pruning.rs`: typed unknowns and a mixed timestamp
  regression test, 47 additions and one deletion.
- Wyrd commit `07a157cb9195c39f280cd851b683703ed53557ac`, October 29 at
  20:05 -0400, rolled back the broader TASK-008 attempt, including the pin
  and Wyrd regression test. Retained research explains the broader attempt's
  performance and verification failures; those do not independently establish
  that the isolated statistics fix was wrong.
- The current inspected tests do not preserve that mixed-bound regression.
  DataFusion's empty-input test checks unavailable results without checking
  diagnostic silence; Wyrd's route tests cover populated selections without
  an empty-output-partition warning assertion.

Restoring the existing fork correction alone cannot fix empty-route warnings:
its changed helper still attempts conversion for zero containers.

## Runtime and storage checks

The already-running development server on `127.0.0.1:8080` was queried through
the existing CLI credential chain. No credentials were printed, no service was
restarted, and no workloads were generated.

- A span count returned 19,423 with a successful terminal.
- A valid bounded timestamp query with service, numeric kind, and GenAI
  operation predicates returned the expected zero matches and a successful
  terminal. The terminal's empty `warnings` field is a protocol result, not
  evidence that the server's log stream was silent.
- The current span Parquet artifact had 19,423 rows and one row group. Every
  reported column had a present, exact minimum and maximum.
- The inspected current Iceberg span manifest had one live entry. Every
  reported column had lower bounds, upper bounds, and null counts.

These are observations of the current local data, not proof that every
historical object or another deployment has complete bounds.

The temporary executable probes and source are in
`/tmp/wyrd-pruning-investigation.Y5d0c4/`: `statistics.rs`, `predicate.rs`, and
`storage.rs`. They were compiled through `mise exec -- rustc` against existing
build artifacts. Their assertions passed. They are diagnostic probes, not
committed regression coverage; the source-owner regression tests must still be
restored or added during implementation.

## Smallest complete recommendation

1. Correct empty-input behavior in DataFusion. Its API permits zero containers;
   Wyrd's empty-route call is a valid use, not an invalid-input contract breach.
   The dependency must return the ordinary empty selection without conversion
   warnings. Add dependency coverage and Wyrd integration coverage for a globally
   pruned route set and an empty execution partition. No new Wyrd guard or
   pruning wrapper is required. This ownership recommendation supersedes the
   earlier suggestion to compensate inside the route adapter.
2. Restore the isolated, existing typed-unknown dependency correction and its
   Wyrd regression coverage, after normal compatibility verification. Retain
   mixed exact/absent/inexact bounds and both minimum and maximum cases.
3. Keep normal no-evidence dependency inputs quiet and make actual conversion
   diagnostics identify bounds accurately without exposing scalar values.
   A max failure is currently mislabeled min.
4. Verify conservative sizing and full-match behavior, real SDK query results,
   required source proofs, and the targeted Bifrost gates. Confirm no conversion
   warnings for the reproduced empty-route cases.

No file rewrite, new string-statistics collector, broad dependency upgrade,
query rewrite, logger suppression, or replacement Iceberg reader is warranted.
The dependency-owned empty-input correction removes unnecessary conversion and logging; it does
not restore a lost file exclusion because that route share contains no data.
The typed-unknown correction can restore real pruning across mixed bounds.

## Evidence locations

- [Parquet statistics configuration](../../../crates/vala/vala-bifrost-redux/src/parquet/writer_properties.rs): `recipe_builder`.
- [Iceberg footer conversion](../../../crates/vala/vala-bifrost-redux/src/scribe/parquet_writer.rs): `derive_data_file_metrics`.
- [Oracle file planning and native reader](../../../crates/vala/vala-bifrost-redux/src/oracle/exec.rs): `published_files`, `planned_files`, `persisted_input`, `hot_parquet_scan`, `verify_scanned_footer_tenant`.
- [Live route selection and execution](../../../crates/vala/vala-bifrost-redux/src/oracle/live.rs): `selected_by`, `resolved`, `handle_child_pushdown_result`.
- [Batch pruning guard](../../../crates/vala/vala-bifrost-redux/src/scan_sizing.rs): `BatchPruning::keep`.
- [Scribe batch statistics](../../../crates/vala/vala-bifrost-redux/src/scribe/memtable.rs): `batch_statistics`.
- [Memory source](../../../crates/vala/vala-bifrost-redux/src/oracle/memtable_source.rs): owner rationale and pruning consumer.
- [Actual span column types](../../../crates/vala/vala-bifrost-redux/src/tables/traces/spans.rs): `kind` is Int32; `service_name` and `gen_ai_operation_name` are Utf8.
- [Retained rollback research](../benchmark-scaling/research.md): hot shredding history and rollback.
- Pinned DataFusion: `common/src/pruning.rs`, `physical-plan/src/common.rs`,
  `pruning/src/file_pruner.rs`, and `datasource-parquet/src/opener/mod.rs`.
- Pinned Iceberg: `iceberg/src/scan/mod.rs` (inclusive metrics evaluation),
  `iceberg/src/scan/task.rs` (task fields), and
  `integrations/datafusion/src/physical_plan/scan.rs` (normal execution).
- [Bifrost architecture](../../../architecture/bifrost-design.md): Oracle source
  authority, nested reads, and source/resource invariants.

## Ownership clarification after maintainer feedback

The empty warning is a dependency defect exposed by a reachable Wyrd path.
`PruningStatistics::num_containers` has no positive-count requirement;
`PrunableStatistics` explicitly tests empty inputs; `PruningPredicate::prune`
starts with one result per container, naturally zero for zero input. Thus the
missing Wyrd guard explains why this adapter exposes the warning, but is not
itself evidence that Wyrd violated the dependency API. Both generic corrections
belong in DataFusion. Wyrd retains source facts and integration tests. The
existing `BatchPruning` wrapper also contains generic predicate logic; relocating
all such code is a separate scope, not a prerequisite or implicit authorization
for this bug correction.
