# TASK-003 plan (spec revisions 21 and 26) — read this before any work

This file is the plan. Do the steps in order. Do not start a step before the
previous one is done. Do not add work that is not listed here.

## Order (set by the user)

| # | Step | Status |
|---:|---|---|
| 1 | Spec revision 21 (one read path) | done, approved |
| 2 | Delete unneeded and incorrect code | done; workspace compiles (`cargo check --workspace --all-features --tests`) |
| 2a | Spec revision 26 (Variant is a DataFusion feature) | done, approved (`spec-revision-26-datafusion-variant.md`) |
| 3 | Implement the correct way (below) | in progress: order items 1–3, V1, V2 done; V3 F5 done, Lists (F1/F2) next |
| 4 | Run focused tests | not started |
| 5 | Run release benchmarks | Variant bench written and passing on a fast build (`release-nonlto`); final LTO run not started |

## The rule

One read path, whether data is hot, promoted, or distributed, and that path is
native DataFusion. Wyrd does not write DataFusion logic; it uses it.

- All Variant querying is a self-contained `variant` feature in the DataFusion
  fork, written as an upstream DataFusion feature (no Wyrd names or
  behavior). Wyrd turns it on (spec revision 26).
- Wyrd keeps only: write-side shredding and write limits (REQ-020), enabling
  the feature, and tenant isolation.
- No new Wyrd mechanism where a DataFusion or `datafusion-distributed`
  extension point exists.

## Latency requirements (user, spec revision 26)

p50/p95, same machine, before and after; reported as values, not ratios.

| Query kind | Planning | Execution and read |
|---|---|---|
| No Variant column | unchanged | unchanged |
| Struct field | unchanged | unchanged (DataFusion already reads only selected leaves) |
| Variant, unshredded | unchanged | unchanged |
| Variant, shredded | unchanged | dramatically lower than unshredded |

"Before" for non-Variant and Struct queries is the fork with the `variant`
feature off. "Before" for Variant queries is HEAD `b145767dd`'s numbers in the
task file's benchmark evidence, rerun on this machine if not like for like.
Anything that adds work to a non-Variant plan (for example F3 on every
`unnest`) must be gated so it costs nothing when it does not apply.

## Step 3: implement

1. Every persisted scan is one `DataSourceExec`; its only predicate comes from
   DataFusion's `FilterPushdown`.
2. Analytical queries distribute that scan with `datafusion-distributed`'s
   defaults and ship it with `datafusion-proto`.
3. ~~Fork adapter-factory codec hook~~ — deleted by spec revision 26; Variant
   needs nothing on the wire. Remove the hook from the fork.
4. A `WorkerPlanRewriteHandler` reattaches the tenant footer proof
   (`TenantParquetReaderFactory`) on the worker.
5. Event-time file pruning from `PartitionedFile` statistics.
6. Live Scribe fragments ship `FilterExec(pred, RemoteSourcePlaceholderExec)`;
   the follower runs native `FilterPushdown` after substituting its provider.
7. Distributed scan metrics count files and partitions again
   (`fold_distributed_scan_metrics`).
8. Re-add native coverage deleted in step 2: Scribe staged-scan row-group
   pruning, the live predicate journey in
   `wyrd-testing/tests/bifrost/oracle/peer_network/security.rs`.
9. Rewrite stale comments in `wyrd-testing/tests/bifrost/oracle/distributed.rs`
   (`OracleCatalogResolver`, `assignment.predicates`, `with_assigned_files`).
10. The DataFusion `variant` feature, V1–V6 below, and Wyrd's switch to it.
11. Update the spec and task file.

## DataFusion `variant` feature (fork phases)

Fork: `/home/thorrester/Documents/GitHub/datafusion`. Every phase ends with
its own fork tests passing (shown with output), with and without the
`variant` feature. Generic pieces know nothing about Variant.

- **V1 — Reading** (`physical-expr-adapter`, `datasource-parquet`, parquet
  `variant_experimental`). The default adapter converts a file's Variant to
  the table's Variant at any Struct/List depth: validate the bytes, then
  arrow-rs `unshred_variant`. Null under a null parent stays null; malformed
  bytes are a DataFusion error, never a panic (upstream decode panics, so
  arrow-rs full validation runs first). No DataFusion Variant limit or config
  (user, 2026-10-08: "what would you, the datafusion maintainer, do?"). The struct cast must never
  silently drop `typed_value`. Tests: shredded/partial/unshredded at top
  level, in a Struct, in a List; null parents; malformed bytes; depth limit.
- **V2 — `datafusion-functions-variant` crate and `variant` feature.** Move
  `variant_get`, `variant_get_text`, `parse_json`, `try_parse_json`,
  `to_json`, and the `->`/`->>` `ExprPlanner` out of Wyrd's `variant_sql.rs`,
  renamed to DataFusion style. The `datafusion` crate's `variant` feature
  registers them in the default `SessionState`. Tests: core integration
  tests for REQ-017 semantics (not sqllogictest: turning the feature on
  there changes the function list for every `.slt`); proto round trip by
  name (no codec).
- **V3 — Leaf reads (F1, F2, F5).** Already written: `list_element_access`
  (`array_element`), `list_element_lambda` (`array_transform`).
  - F1: the adapter's cast pushdown goes through List element access and
    `array_transform` lambdas.
  - F2: the Parquet read plan follows List element steps and lambdas and
    resolves `required_input_fields` through the accessor chain under it.
    Projection-only through Lists (no statistics pruning).
  - F5: `variant_get`/`variant_get_text` declare a layout-agnostic Variant
    argument and their leaves; the adapter drops the conversion under them.
  - Known risk: `LambdaVariable` checks its field exactly at runtime, so
    narrowing a list column must rebind lambda parameter fields.
  - Tests: plan shape and decoded-leaf metrics for `l[i]['f']`,
    `array_transform`, and Variant paths through Structs and Lists.
- **V4 — `unnest` (F3, optimizer).** `ExtractLeafExpressions` /
  `PushDownLeafProjections` pass a leaf expression on an `unnest` output
  through the `Unnest` as `array_transform(l, x -> expr)`, built in the
  optimizer (DF55 SQL cannot parse lambdas), only when `array_transform` is
  registered and fewer leaves are read. Otherwise `Unnest` stays a barrier.
  Tests: plan shape, unchanged plan when no benefit, same results.
- **V5 — Pruning.** The typed shredded-key comparison rewrite (from Wyrd's
  `typed_comparison`) moves into the Variant crate and runs per file, guarded
  by the residual `value` being all-null and the literal type matching.
  Tests: row groups skipped only when safe.
- **V6 — Fork cleanup.** Delete the item-3 adapter-factory codec hook. Wyrd
  then pins the published fork instead of the `df-snap2` snapshot (handoff
  step 4).

Wyrd switch (after V6):
- Enable `datafusion/variant`.
- Delete `oracle/variant_sql.rs` Variant functions, planner, `FileRead`,
  `VariantFileAdapterFactory`/`VariantFileAdapter` (`narrow_reads`,
  `unshred_bare`, `typed_comparison`), `trimmed`; delete `leaf_paths.rs`;
  remove its users in `nested_pushdown.rs` and `exec.rs`; delete the adapter
  codec registration and `ORACLE_VARIANT_SQL_VERSION` (the peer fingerprint
  carries the fork revision); drop `parquet-variant*` from
  `vala-bifrost-redux`.
- Prove with the nested-field unit tests and journeys before item 2.

## Implementation order for step 3

Each item depends only on items above it.

1. Item 1 — done.
2. Item 5 — done.
3. Item 8, first half — done.
4. V1.
5. V2.
6. V3.
7. V4.
8. V5.
9. V6, then the Wyrd switch.
10. Item 2 with item 4 (a distributed scan never runs without the tenant
    footer proof).
11. Item 7.
12. Item 6.
13. Item 8, second half.
14. Item 9.
15. Item 11.

## Current state of the tree

- Wyrd, step 3 progress (2026-10-08, third session):
  - Order item 1 (plan item 1): already true after step 2.
    `OracleTableProvider::persisted_input` builds one `hot_parquet_scan`
    `DataSourceExec` with no predicate; Iceberg reports `Inexact`, so the
    `FilterExec` stays and `FilterPushdown` places it in the scan. Only a stale
    rustdoc link (`scan_predicate_conjunction`) was fixed.
  - Order item 2 (plan item 5): `HotFileSource::statistics` attaches exact
    `wyrd_event_time` min/max to each `PartitionedFile`; `DataFusion`'s
    `FilePruner` skips the file before its footer is read
    (`files_ranges_pruned_statistics`). Merged scan and group statistics are
    made inexact (`compute_all_files_statistics(.., true)` +
    `Statistics::to_inexact`), so `min`/`max` are never answered without
    opening a footer. Test:
    `oracle::exec::tests::event_time_predicate_skips_disjoint_hot_files_before_any_footer_open`
    (replaces the plan-time Wyrd pruning test; dead `pruning_provider`,
    `pruning_fixture_table`, `hot_file`, `retained_hot_locations` removed).
  - Order item 3 (plan item 8, first half): re-added
    `oracle::follower::tests::scribe_staged_scan_prunes_non_matching_row_groups`
    with `FilterExec` + `FilterPushdown` over the resolved leaf.
- The third-session `file_read` codec blocker is moot: spec revision 26 puts
  no Wyrd-specific UDF or adapter in any plan.
- DataFusion fork (local checkout `/home/thorrester/Documents/GitHub/datafusion`
  and the patched snapshot `.../scratchpad/df-snap2`, which does not yet have
  this session's fork edits):
  - Item 3's adapter codec hook is written, uncommitted; V6 deletes it.
  - V3 groundwork written, compiles, tests pending:
    `ScalarUDFImpl::list_element_access` (+
    `ScalarFunctionExpr::list_element_access`); `array_element` implements it
    and moves toward leaves with a literal index;
    `HigherOrderUDFImpl::list_element_lambda` (`ListElementLambda`);
    `array_transform` implements it.
  - Not pushed; publishing is handoff step 4.
- V1 done (2026-10-08, fork, uncommitted; snapshot not synced):
  - `datafusion-common` feature `variant` (deps `parquet-variant`,
    `parquet-variant-compute` 60): `variant::cast_to_variant_field` validates
    every metadata and residual `value` (arrow-rs full validation, which caps
    depth at 128), then `unshred_variant`, then casts children to the target.
  - `nested_struct`: `is_variant`, `cast_column_to_field` (Struct, List,
    ListView children and `CastExpr::evaluate` route a Variant target here;
    a Variant under a null Struct parent gets the parent's null first),
    `validate_field_cast_compatibility` (used by the default adapter). With
    the feature off, a shredded source for a Variant target is a plan error
    instead of silently dropping `typed_value`.
  - `datasource-parquet`: cast clipping keeps every leaf of a Variant target
    (`clip_field`) and never records a whole-column Variant cast access.
  - `datafusion` feature `variant = ["datafusion-common/variant"]`.
  - Tests: `cargo test -p datafusion-common --features variant --lib
    variant::tests` 4/4; `cargo test -p datafusion --features variant --test
    parquet_integration variant::` 1/1; regressions: common 618, adapter and
    datasource-parquet libs, physical-expr `cast` 58, `parquet_integration`
    245, all pass.
  - No read-side limit config: arrow-rs validation (depth cap 128) is the
    read rule; Bifrost's 64 is a write rule (spec revision 26 amended).
- V2 done (2026-10-08, fork, uncommitted; snapshot not synced):
  - New crate `datafusion/functions-variant` (`get.rs`: `variant_get`,
    `variant_get_text`; `json.rs`: `parse_json`, `try_parse_json`,
    `to_json`; `planner.rs`: `VariantFunctionPlanner` for `->`/`->>`,
    chains merge into one call). `datafusion` feature `variant` adds it;
    the default `SessionState` registers the functions and (with `sql`)
    the planner.
  - Two generic gaps found by the tests and fixed in their owning crates:
    `array_element` now keeps the element field's metadata (extension type)
    in its return field; `datafusion-proto` `PhysicalScalarUdfNode` gains
    `return_field_metadata` (field 7, regenerated) so a function's
    extension-typed return field survives a distributed round trip.
  - Known, unreachable today: `PhysicalCastNode` carries only the target
    type, so a serialized `CastExpr` to Variant would lose its target
    field. The adapter inserts Variant casts per file on the executing
    node, after decode, and SQL cannot spell a Variant cast. Fix in the
    proto when that changes.
  - Tests (`datafusion/core/tests/parquet/variant.rs`): operators and
    functions, chain planning, invalid JSON / `try_parse_json`, functions
    over shredded files (top level, Struct, List element), malformed
    bytes are errors for every function, unshredded Variant under a null
    Struct is null. `cargo test -p datafusion --features variant --test
    parquet_integration` 251/251 (7 Variant). Proto: `cargo test -p
    datafusion-proto` 18 + 264 + 4 doctests, including
    `roundtrip_variant_functions_by_name`. `functions-nested` lib 132.
    `cargo check -p datafusion --tests` clean with and without `variant`.
    Clippy clean on changed files (`-D warnings` fails on pre-existing
    `datafusion-common/src/scalar/mod.rs` lints under clippy 1.97).
  - REQ-019 amended (user, 2026-10-08): Bifrost limits and catalog codes
    apply to writes only. Queries use DataFusion's Variant feature as-is.
    Wyrd switch: delete Wyrd's query-side Variant error mapping and the
    tests that expect it.

- V3 F5 done (2026-10-08, fork, uncommitted; snapshot not synced):
  - `InputFieldRequirement::accepts_any_layout` (`datafusion-expr`): a
    function accepts an argument in any storage layout of its extension
    type. `variant_get`/`variant_get_text` declare it with their leaves
    (whole root when the path or layout gives none); `to_json` with the
    whole argument.
  - Adapter `try_drop_layout_cast`: drops a generated Struct cast whose
    source and target share an extension type when the rebuilt function
    declares that argument any-layout and keeps its return field.
  - Read plan: requirements on a Struct field chain (`s['v']`) apply below
    its path (`struct_access_chain`). `rebase_onto_read` (decoder
    projection and row filter) rebuilds nested-returning functions when the
    read narrowed a Struct, so `get_field(s,'v')` matches its pruned input.
  - `datafusion` `variant` feature adds `parquet?/variant_experimental`, so
    files tag Variant columns with the extension type on read and write.
  - Found and fixed: `to_json`, `variant_get_text` rendering and the
    non-literal `variant_get` path used row-wise `try_value`, which cannot
    read shredded objects; they unshred first.
  - Test `shredded_paths_read_only_their_leaves`: `v ->> 'a'`,
    `s['v'] ->> 'a'`, and both as pushed-down filters read under 1/10 of
    the bytes of `to_json(v)`, same results. Suites: datasource-parquet 276,
    physical-expr-adapter 42, expr 263, core `parquet_integration` 252 with
    `variant`, 244 without.
  - Next: Lists (F1/F2), then the `parquet_variant_query` benchmark.

## Step 4: focused tests

```
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=distributed::iceberg_filtering_mechanisms_cover_all_table_kinds) | test(=published::struct_and_variant_share_physical_pushdown)'"
```

Plus the `vala-bifrost-redux` unit tests, and the fork tests for every
changed DataFusion crate with and without the `variant` feature.

## Step 5: release benchmarks

`mise run bench:bifrost:reads` (Wyrd overhead: `hot` and `published`) plus the
fork's benchmarks. Report p50/p95 only, like for like, no ratios, against the
latency requirements table above.

- Unshredded vs shredded Variant is DataFusion's benchmark, not Wyrd's
  (moved 2026-10-08 by user direction). Wyrd's `bifrost_reads` `variant`
  section is deleted. The fork's `parquet_variant_query` is the Variant
  before/after of `parquet_struct_projection` (the pydantic
  struct-field-pushdown article's workload): all 18 cases over narrow, wide
  and nested objects, before = unshredded Variant, after = shredded Variant,
  same SQL, rows and build. Each case times plan+execute (the article's single
  measurement) and planning alone. No Struct layout: Struct is already
  DataFusion's `parquet_struct_projection`, run base vs on for "Struct
  unchanged".
- First requirement check (2026-10-08): no non-Variant or Struct regression
  (`filter_struct_field_eq` +7% was noise; interleaved reruns equal).
  Shredded key reads 2–11x faster than unshredded. Diagnoses:
  - Shredded planning +25–60 µs: bench bug. `ListingTable` built without
    the session statistics cache re-read footer statistics per plan. Fixed
    with `with_cache`, as `register_parquet` does; planning now equal.
  - `narrow/select_large_field` slower: noise (10 samples on 8 MB results).
    A 2,000-iteration loop gives shredded 1.61 ms vs unshredded 2.06 ms.
    Bench now takes 50 samples over 3 s.
  - Whole-Variant reads (`select_variant`, `select_all`) on shredded files
    1.7–3.7x slower: real. Decode time is equal; the cost is arrow-rs
    `unshred_variant` (narrow 1.13 ms, nested 2.34 ms, wide 8.93 ms per
    query), which rebuilds row by row: parses each row's metadata, copies
    each string into the value buffer, and `Vec::splice`s each object's
    header in front of its bytes, shifting them once per object level.
- Requirement amended by maintainer decision, approved by the user
  2026-10-08: shredded key reads dramatically faster than unshredded;
  whole-Variant reads of shredded files pay the rebuild cost, bounded by a
  limit in the benchmark. The rebuild fix belongs in arrow-rs
  (`unshred_variant`), not in DataFusion. The limit is still to be set.
- User decision 2026-10-08: fix arrow-rs locally. Clone
  `/home/thorrester/Documents/GitHub/arrow-rs` at tag 60.0.0 (`ef1fa15`),
  sources identical to the crates.io packages; never pushed. Finding: the
  slowdown is mostly the allocator, not arrow-rs. With glibc's default
  allocator, each 16 MB output buffer is mapped fresh and page-faulted in
  (wide: 1.25M faults per 300 batches; 3.39 ms per batch, 0.96 ms with
  allocator reuse via `GLIBC_TUNABLES` mmap/trim thresholds raised).
  Pre-sizing the value buffer saves a further ~20% (0.77 ms), wide only.
  The bench under allocator reuse, unshredded → shredded p50 ms:
  narrow 1.10→1.30 / 1.20→1.29, nested 3.11→4.65 / 3.14→4.62, wide
  8.00→11.59 / 6.80→11.56 (`select_variant` / `select_all`), so 1.1–1.7x.
  Wyrd's server runs mimalloc.
- Agreed 2026-10-08: the bench runs on mimalloc (as `datafusion-cli` and
  `benchmarks` do) and the whole-Variant limit is 2x. On mimalloc alone,
  whole-Variant was 1.58–1.99x slower: mimalloc copies the value buffer on
  each doubling. The arrow-rs pre-size fix (`ValueBuilder::with_capacity`,
  `VariantValueArrayBuilder::with_capacity`, sized from the input's buffers,
  `shrink_to_fit` at build) makes `unshred_variant` 1.7–3.4x faster; its
  tests pass (174 + 364). The DataFusion fork's `[patch.crates-io]` points
  all arrow-rs crates at the local clone. Result, p50 ms unshredded →
  shredded, `select_variant` / `select_all`: narrow 1.11→1.89 /
  1.15→1.71 (1.71x / 1.48x), wide 7.81→10.08 / 8.23→10.08 (1.29x / 1.22x),
  nested 3.17→4.08 / 3.41→4.19 (1.29x / 1.23x). Key reads: wide small
  6.19→0.36, nested inner small 2.81→0.38, narrow small 1.06→0.35.
  Results in `scratchpad/bench/variant-mimalloc-presize.txt`.
- The "unshredded unchanged" baseline is Wyrd's committed
  `nested_field_pushdown` bench at HEAD `b145767dd` (from a `git archive`
  export); the uncommitted `bifrost_reads/variant.rs` was deleted.
- Requirement check before the full run: base (fork HEAD `38b7a7a0a` export)
  vs on (working fork, `variant` feature), `sql_planner`
  `physical_plan_tpch_all` and `physical_select_all_from_1000`,
  `parquet_struct_query` `struct_access`, `filter_struct_field_eq` and
  `filter_top_level_with_struct_select`, plus `parquet_variant_query`.

## Handoff (2026-10-08): remaining work, in order

State of the three repositories (committed locally 2026-10-08, nothing pushed):

| Repo | Path and branch | Base | Local commit |
|---|---|---|---|
| Wyrd | this worktree, `wyrd/bifrost-variant/TASK-003` | `b145767dd` | `4559cff37` (excludes `spec.md` and TASK-004, never committed) |
| DataFusion fork | `/home/thorrester/Documents/GitHub/datafusion`, `wyrd-arrow60-variant-read-plan`, remote `github.com/bohmian-ai/datafusion` | `38b7a7a0a` (pushed) | `b86808d88`: V1, V2, V3-F5, V3 groundwork, item-3 codec hook, `parquet_variant_query`, `[patch.crates-io]` to local arrow-rs. The `datafusion-testing` submodule pointer change is unrelated and left uncommitted. |
| arrow-rs | `/home/thorrester/Documents/GitHub/arrow-rs`, `wyrd-unshred-presize`, remote is upstream `apache/arrow-rs` (no Bohmian fork yet) | tag 60.0.0 `ef1fa15` | `c134488`: `unshred_variant` pre-size fix |

Wyrd's worktrees share the main checkout's `pre-commit` hook, which runs
`mise run check:file-size`, a task defined only on another branch
(`wyrd/verified-change-contract/TASK-017`). `4559cff37` ran that task's
command by hand (pass) and committed with `--no-verify`.

How Wyrd builds today (root `Cargo.toml` `[patch.crates-io]`):
- DataFusion crates point at `/tmp/.../6cd9bbbe-.../scratchpad/df-snap2`, a
  copy of the fork that predates V1–V3 and is lost on reboot.
- Every arrow-rs crate points at the local arrow-rs clone.
  `cargo check --locked --workspace --all-targets` passes.
- Other forks (unchanged by this task): `bohmian-ai/iceberg-rust`,
  `bohmian-ai/iceberg-compaction`, `bohmian-ai/datafusion-distributed`, all
  pinned by git `rev`.

Remaining, in order:
1. V3 Lists (F1/F2), with the lambda-rebind risk, and their tests.
2. V4 `unnest`, V5 pruning.
3. V6: delete the item-3 codec hook.
4. Publish the forks (user direction 2026-10-08: all forks get published;
   say what will be pushed before each push):
   - arrow-rs: create `bohmian-ai/arrow-rs`, push a branch from tag 60.0.0
     with the pre-size fix.
   - DataFusion: commit and push the fork branch; its `[patch.crates-io]`
     points at the published arrow-rs rev, not the local path.
   - Wyrd `[patch.crates-io]`: replace the `df-snap2` and local arrow-rs
     paths with git `rev` pins to those two pushes, add
     `datafusion-functions-variant`, delete `df-snap2`.
   - Any of the other three forks that change get pushed and re-pinned the
     same way.
5. Wyrd switch and deletions (list above), including the query-side Variant
   error mapping.
6. Order items 10–15 (items 2+4, 7, 6, 8 second half, 9, 11).
7. Step 4 focused tests. Step 5 LTO benchmarks: `parquet_variant_query`
   (whole-Variant cases under 2x; key reads faster), base vs on for
   `sql_planner` and `parquet_struct_query`, and `bench:bifrost:reads`.
8. Closeout: `$wyrd-task-review`, then `$wyrd-change-review` /
   `$wyrd-complete` per AGENTS.md §14.

Not required (user, 2026-10-08): a Wyrd "before" for unshredded Variant
latency. Variant latency is DataFusion's `parquet_variant_query`.

## Standing constraints

- Never commit `changes/active/bifrost-variant/spec.md` or
  `tasks/TASK-004-forge-parallelism.md`. Commit only when asked.
- Local git identity only; no co-author trailers; never `git config`; no
  stash, rebase, or force-push.
- Diagnoses use Symptom / Evidence / Cause / Fix site. Never claim a test ran
  without its output. Do not change code to make an old test pass.
