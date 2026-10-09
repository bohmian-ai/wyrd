# TASK-003 plan (spec revisions 21 and 26) — read this before any work

This file is the plan. Do the steps in order. Do not start a step before the
previous one is done. Do not add work that is not listed here.

## Order (set by the user)

| # | Step | Status |
|---:|---|---|
| 1 | Spec revision 21 (one read path) | done, approved |
| 2 | Delete unneeded and incorrect code | done; workspace compiles (`cargo check --workspace --all-features --tests`) |
| 2a | Spec revision 26 (Variant is a DataFusion feature) | done, approved (`spec-revision-26-datafusion-variant.md`) |
| 3 | Implement the correct way (below) | done |
| 4 | Run focused tests | done (see Step 4 evidence) |
| 5 | Run release benchmarks | done (see Step 5 evidence); review remediation `review/TASK-003-r1` in progress |

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

### Working in the fork: you are the DataFusion maintainer

Everything in the fork is written as production DataFusion code: the
`variant` feature is meant to ship in DataFusion itself. No pull request is
opened anywhere; the fork is published to `bohmian-ai` only (handoff step 4).

- Decide as a DataFusion maintainer would. Wyrd and Bifrost rules, names and
  behavior never apply inside the fork. Generic pieces know nothing about
  Variant; Variant pieces know nothing about Wyrd.
- No ad hoc changes: no workarounds, special cases, or test-only shortcuts
  to get a Wyrd journey or benchmark through. If DataFusion lacks an
  extension point, add the general one in its owning crate.
- Follow the fork's own standards: `AGENTS.md`, the contributor guide
  (`docs/source/contributor-guide/`, including `api-health.md` for public
  API changes), and `.ai/skills/` where relevant (`add-benchmarks` for
  benchmark changes). Use the pinned toolchain (`rust-toolchain.toml`,
  1.98.1: `cargo +1.98.1 ...`). Wyrd's `mise.toml` pins the same 1.98.1
  (user, 2026-10-08), so both repos build and lint on one compiler.
- Every fork commit passes, with output shown: `cargo fmt --all`,
  `cargo clippy --all-targets --all-features -- -D warnings`
  (`./dev/rust_lint.sh` runs the full lint suite), and the tests of every
  changed crate with and without the `variant` feature.
- Performance is a requirement, not a report: the latency table above
  holds for every phase. Non-Variant, Struct and unshredded Variant
  planning and execution unchanged; shredded key reads much faster than
  unshredded; whole-Variant reads of shredded files at most 2x unshredded
  (`parquet_variant_query`, mimalloc). Work that does not apply to a plan
  costs it nothing. Measure changed paths against the fork base before
  calling a phase done.
- Every change and implementation follows the owning repository's
  conventions and standards (user, 2026-10-08). Before writing code, read
  the nearest existing code in the owning crate and match its patterns.
  Tests come first and live in the owning crate: unit tests for each new
  piece before any integration test. Every tree rewrite is shown to
  terminate (a rewrite whose output can match its own pattern returns
  `TreeNodeRecursion::Jump`). Never run an integration test whose pieces
  have no passing unit tests.

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
  - V3 Lists status (2026-10-08): the first attempt was rejected by the
    user. Its lambda rebinding (`LambdaExpr::replace_parameter` via
    `transform_down`) re-entered its own `cast(x)` output and recursed
    forever, OOMing the machine (test process killed at 63, 72, 83 GB).
    It was written without unit tests and rerun without reading the kernel
    log. All lambda rebinding and lambda leaf tracking has been deleted.
    What remains, uncommitted and unverified: `l[i]['f']` leaf selection
    through Lists in `datasource-parquet`, the adapter's List cast
    narrowing below element access, and `core/tests/parquet/list_leaf_projection.rs`.
    An independent review decides REVISE or START_OVER for that remainder
    and an approach for `array_transform` (F2).
  - V3 F1 revision (2026-10-08, after review REVISE C-1..C-10, uncommitted):
    `list_leaf_projection.rs` deleted; byte evidence moved to
    `parquet_nested_schema_pruning.slt` per repo convention. Adapter unit
    tests and read-plan unit tests added first. The slt exposed a regression
    (`events[1]` from `narrow` read 312 B against the 172 B whole-column
    clip, because the List cast now sits below element access); fixed by
    clipping a cast of an element access with `clip_for_cast`, as for
    whole-column casts. Measured: `events[2]['x']` 87 B (narrow and
    full_schema, baselines 172 B / 312 B); `events[1]` 172 B. C-9
    (`physical_plan` projection rebased over the narrowed read) has its own
    test. Green: adapter and datasource-parquet unit tests with and without
    `variant`, the three nested slt files, `parquet_integration` Variant
    tests (default 244, `variant` 252, no warnings), fork clippy
    `-D warnings`. `parquet_struct_projection` (release-nonlto) against a
    bfe8d793c worktree: every struct and flat case within run-to-run noise
    (a first F1 run was an unexplained fast outlier; the rerun matched
    base). F2 widen-back was withdrawn: Variant `metadata` and shredded
    object fields are non-nullable, and `cast_column_to_field` turns any
    differing Variant type into a layout conversion.
  - V3 F2 (approved by the user 2026-10-08: rebinding). Read-plan analysis
    is read-only: `PushdownChecker` binds a list element lambda's parameter
    to (root, path + Element) and walks the body, falling back to the whole
    element when shadowed, used whole, or unresolvable.
    `HigherOrderFunctionExpr::rebind_list_element` (physical-expr) rebinds
    the parameter to the read element field with a `TreeNodeRewriter`: it
    jumps over shadowing lambdas and visits each node once in `f_up`, so a
    `bind` output containing the variable is never revisited (the crash
    cause). `rebase_onto_read` calls it with an identity bind; the adapter
    moves a generated List cast into the lambda with a cast bind.
    Done: `rebind_list_element` + 4 unit tests (narrowed list with nested
    lambda equals full-list result, bind-once, shadowing, no list element
    lambda), physical-expr clippy clean. Read-plan side done:
    `PushdownChecker` binds list element lambda parameters (shadow frames
    pushed per `LambdaExpr` in `f_down`, popped in `f_up`; a path through a
    Map reads below its last resolvable step); `rebase_onto_read` rebinds
    with an identity bind. Tests: `build_projection_read_plan_follows_list_element_lambdas`
    (body field, Struct-nested List, whole/unread element, capture,
    shadowing, nested lambdas, Map) and
    `rebased_list_element_lambdas_evaluate_over_narrowed_reads` (decoded
    narrowed read equals full-file evaluation, with empty and null Lists).
    Suites green with and without `variant`: datasource-parquet 284,
    physical-expr 1658, adapter 45; datasource-parquet clippy clean.
    Bench invalid (2026-10-08): base and fork shared one `CARGO_TARGET_DIR`
    and hash to the same bench binary, so the "current" run recompiled
    nothing and re-ran the base binary (0 `Compiling` lines). Its deltas
    (+14% to -70%) are base-vs-base noise, unpinned under `powersave`. The
    earlier F1 "equal to base" result was measured the same way and is
    unproven. Redo: separate target dirs, frozen binaries, `taskset` to one
    CCD, interleaved rounds, A/B delta judged against each case's A/A spread
    Redone with separate target dirs and frozen binaries (release-nonlto,
    F1+F2 incl. adapter vs bfe8d793c): full suites base-then-current flagged
    `*/unshredded/select_variant` +9-12%; alternating the same case alone
    (base, cur x3) shows no difference (narrow 1.42/1.47/1.42 vs
    1.43/1.44/1.43 ms; wide 7.73/7.27/8.13 vs 7.11/6.97/6.91; nested
    3.17/3.17/3.21 vs 3.20/3.11/3.11), and `perf` user-space profiles match
    within 0.35% with no new symbol. Planning cases unchanged or 1-3% faster.
    Struct bench: no reproducible regression.
    Adapter lambda cast pushdown done (`try_narrow_list_element_lambda`,
    `narrow_expr` split from `rewrite_expr`); adapter tests pass incl.
    `test_narrow_list_cast_into_lambda_body`. Slt lambda cases dropped: DF55
    SQL cannot bind lambda parameters over table columns (`No field named
    e`), so byte evidence goes in a core DataFrame-API integration test.
    Done: `shredded_paths_read_only_their_leaves` checks
    `array_transform(l, x -> x['v'] ->> 'a')[1]` values and reads under a
    tenth of the whole-Variant bytes (8/8 `variant::` tests pass). Committed
    F1+F2 as fork bb29c006c after fmt, clippy -D warnings (all features),
    datasource-parquet 284 + adapter 46 (with and without variant),
    physical-expr 1658 + expr 263, parquet_integration 244 / 252 with
    variant, nested slt files. Left uncommitted by decision: the
    datafusion-testing pointer, `file_pruner.rs`, the `assert!` hunk in
    `core/tests/parquet/variant.rs`. C-8 docs done (read plan docs name
    Lists; uncommitted, goes with V4).
  - V4 in progress (2026-10-08). The optimizer crate forbids function
    dependencies and name lookups ("use methods on the UDF traits"), so:
    new `HigherOrderUDFImpl::list_element_map` (result keeps the List's
    offsets and nulls, element i = lambda(element i)); `array_transform`
    declares it. `PushDownLeafProjections` threads `&dyn OptimizerConfig`
    (was the alias generator) and, at an `Unnest`, calls
    `try_push_through_unnest`: pairs on depth-1 unnested List elements that
    the recovery projection does not read whole become `map(l, x -> f(x))`
    below the `Unnest`, which unnests the result; pass-through pairs ride
    along; anything else keeps the barrier. Recovery columns, not
    `columns_needed` (inflated by pair refs), decide "read whole". Only
    runs when an extraction projection sits on an `Unnest`. Tests:
    3 optimizer unit tests (projection, filter, barrier without benefit /
    without a registered map), all 885 optimizer tests pass; slt
    `unnest(events)['x']` reads 87 B (= `events[2]['x']`) vs 172 B whole,
    same results, plan shows `array_transform` in the scan. Full slt found
    one changed plan (`unnest.slt:665`, column also selected whole): added
    `reads_mapped_lists_whole` (sources traced through the projection
    below the `Unnest`) so no-benefit cases keep the barrier; recovery
    columns are computed only on the `Unnest` path. Core
    `unnest(l)['v'] ->> 'a'` reads < 1/10 of whole. V4 committed as fork
    c165beeb4 (with C-8 docs) after fmt, clippy -D warnings (all
    features), optimizer 885 / expr 263 / functions-nested 132,
    datasource-parquet 284 with and without variant, parquet_integration
    244 / 252, core_integration unnest 15, full slt 520/520. Next: V5.
  - V5 done (2026-10-08). Placed in `PruningPredicate`, not the
    adapter, so row filters and projections evaluate nothing new: the
    rewrite only shapes statistics. New declarative
    `ScalarUDFImpl::stored_value` -> `StoredValue { arg_index, typed_path,
    residual_path }`: where the residual field is null the call returns
    the typed field cast to its return type. `ScalarFunctionExpr::
    stored_value` validates it. `RequiredColumns::rewrite_struct_fields`
    (already per file, against the physical schema) rewrites
    `[CAST(]f(..)[ AS t)] op lit` into `CAST(typed AS <f type>) op lit OR
    residual IS NOT NULL` over synthetic nested-field columns, so a row
    group is skipped only when its residual is all null and the typed
    bounds exclude the literal. Integer/Boolean typed fields compared
    through a string return as integer/Boolean `t` cast directly (exact
    round trip); string comparison of integers stays unprunable via the
    existing cast-order check. Marks `has_filter_semantics_only` (full-match
    inversion does not guard function nulls). `variant_get_text` declares
    it for a fully shredded literal key path whose last `typed_value` is a
    string, integer, or Boolean. Tests: pruning unit
    `stored_value_comparison_prunes_only_all_null_residuals`; core
    `typed_key_comparisons_prune_row_groups` (3 row groups, residual in
    one, string / integer / reversed / unprunable-text cases, results and
    `row_groups_pruned_statistics`). Shared `table()` helper in the core
    test replaces two copies of the write-and-register code.
    V5 committed as fork 603f257a5 after fmt, clippy -D warnings (all
    features), expr 263 / physical-expr 1658 / pruning 110,
    datasource-parquet 284 with and without variant, parquet_integration
    244 / 253, full slt 520/520. Crates are tested one per `cargo test`:
    testing expr, physical-expr, pruning and functions-variant together
    builds `functions-nested` without `sql` while `datafusion-expr/sql` is
    on, and `RawBinaryExpr.op`'s feature-dependent type then fails to
    compile (existing upstream feature setup).
  - V6 done (2026-10-08): fork b4ea3f642 removes the item-3 codec hook
    (codec trait methods and dispatch, encode/decode ctx methods, proto
    field 18, proto and physical-plan adapter deps, its round-trip test);
    return-field metadata and the by-name Variant round trip stay.
    `regen.sh` reproduces the proto models. d36fb87ed fixes clippy 1.98
    `unused_async_trait_impl` in the encryption-less Parquet opener
    (`std::future::Ready`, no `#[allow]`), which only the no-extra-features
    build hits. Evidence: fmt; clippy -D warnings for datasource-parquet
    without features and proto / proto-models / physical-plan /
    datasource / datasource-parquet with all features; proto 17 + 264 + 4,
    datasource 177 + 8. Next: publish (handoff step 4).
  - Wyrd 1.98.1 lint fallout fixed without `#[allow]`: `as_chunks` in OTLP
    decode, boxed `PublicationError::Write` source (136 B -> under 128 B),
    `unwrap_or` in the load harness, ready-future opendal test fakes, and
    `BackendSigner` cloud/non-cloud split matching `presign_part`. Both
    `mise run lints` steps pass; `wyrd-storage` 57/57.
  - Published (2026-10-08): new fork `bohmian-ai/arrow-rs`, branch
    `wyrd-unshred-presize` at c134488d6604; DataFusion fork commit 75b11dedd
    pins it by git and is pushed to `bohmian-ai/datafusion` (fast-forward
    38b7a7a0a..75b11dedd). Wyrd `[patch.crates-io]` points every
    datafusion-* crate (now including `datafusion-functions-variant`) at
    75b11dedd and every arrow-rs crate at c134488d6604 (uncommitted, lands
    with the switch).
  - Wyrd switch in progress (uncommitted):
    - `vala-bifrost-redux` enables `datafusion/variant` and drops
      `parquet-variant-json` (`parquet-variant`/`-compute` stay: the writer,
      residual, and table code use them).
    - Deleted `oracle/variant_sql.rs` and `oracle/leaf_paths.rs`; the session
      template moved to `oracle/session.rs` (`OracleSessionTemplate`, default
      features only). Analytical worker/planning sessions drop the install
      (the distributed worker builder already has default features).
      `with_expr_adapter` removed from `hot_parquet_scan`; `FileReadPlan`
      uses `DefaultPhysicalExprAdapterFactory`. `ORACLE_VARIANT_SQL_VERSION`
      removed from both digests.
    - Journeys (SDK Rust/Python/TS, MCP, peer `published.rs`): invalid
      `parse_json` now expects `WYRD_VALA_500_QUERY_EXECUTION_FAILED`;
      `variant_as_text(doc)` -> `to_json(doc)`. `bifrost-design.md` Variant
      SQL section and `docs/.../schema.svx` updated.
    - `oracle::` lib tests: 164/170. `exec::projected_leaf_union_preserves_predicate_columns`
      and `follower::scribe_live_sources_read_every_row_once_at_their_partition_count`
      fail on predicate/closure shape that step 2 removed (no
      `assignment.predicates`, no closure projection); they belong to plan
      item 6. Four `nested_pushdown` Variant tests fail: a filter pushed
      Exact into the hot scan returns every row when the file's `v` lacks
      the Variant extension annotation (the `write_groups_with_a` fixture);
      annotated files filter correctly.
    - Diagnosis (independent diagnostician agreed):
      - Symptom: `v -> 'o' ->> 'x' = '2'` returns [1, 2, 3], not [2]; the
        same holds for the CAST, absent-key, and typed-pruning cases.
      - Evidence:
        - the fixture field has no extension (`nested_pushdown.rs`
          `write_groups_with_a`);
        - the fork adapter keeps `CAST(v AS variant)` unless the *file*
          field is a Variant (`schema_rewriter.rs` `try_drop_layout_cast`);
        - `PushdownChecker` then sees the bare struct `v`, so no candidate
          is built (`row_filter.rs` `prebuild_row_filter_candidates`), and
          `RowFilterContext::try_new` installs none;
        - the plan already removed the `FilterExec` (`try_pushdown_filters`
          checked the table schema).
      - Cause: the test fixture writes an unannotated Variant. Production
        writers always annotate (`tables/fields.rs:261`).
      - Fix site: the fixture now annotates `v`, as `write_file` does. The
        Int8 `= 1000` case now expects row group [1]: the fork prunes the
        all-typed Int8 groups, which can never hold 1000; the old Wyrd
        adapter skipped this. The rustdoc was updated to match.
      - Latent fork defect, not fixed: a predicate claimed Exact at plan
        time is silently dropped when a file's adapted predicate is not
        row-filter-pushable (an unannotated Variant from an external writer,
        or a struct cast from schema evolution). The result is wrong rows,
        not slow rows.
  - Blocker (2026-10-09, user: "fix the issue at its root"): the first full
    `mise run test:bifrost` after the switch failed 37 tests. Every failure
    traced to one of four causes:
    - `datafusion-distributed` panic (fork defect):
      - Symptom: an empty table's query returns 500.
      - Evidence: trace shows a divide by zero at
        `file_scan_config.rs:68` (`i % ev.task_count`).
      - Cause: the default estimator desires 0 tasks for a scan with no
        bytes, and the planner accepted a zero count.
      - Fix site: fork `8647526`. `TaskCountAnnotation::limit` bounds every
        reconciled count to 1..=budget. Test
        `test_file_scan_without_bytes_runs_in_one_task` fails without it.
      - Also fork `0b81b12`: the TPC-H and TPC-DS plan snapshots were stale
        since the Arrow 60 move. They were refreshed; correctness suites
        pass (TPC-H 22, TPC-DS 95).
      - Pushed to `bohmian-ai/datafusion-distributed` `wyrd-arrow60`.
    - `parse_json` u64 (fork defect):
      - Symptom: `18446744073709551615` became `1.8446744073709552e19`.
      - Cause: arrow-rs `json_to_variant` fell back to f64 above
        `i64::MAX`.
      - Fix site: arrow-rs `0d0eb24` uses `From<u64>` (exact scale-0
        Decimal16, as ingest does) and enables upstream's ignored
        `decimal16_large_integer` test.
      - The DataFusion fork's `a4ff14550` pins it and adds
        `parse_json_keeps_unsigned_64_bit_integers_exact`.
    - Silently dropped filter (fork defect, the latent one below), fixed in
      DataFusion `220fc50b3`. A row-filter candidate whose only obstacle is
      a nested column used whole now decodes those roots instead of being
      dropped. Test `pushed_filter_applies_to_a_variant_without_its_extension`
      fails without it.
      - Fork evidence: fmt, clippy `-D warnings`; datasource-parquet 284
        with and without `variant`; `parquet_integration` 244 without and
        255 with; sqllogictests.
      - Pushed `75b11dedd..a4ff14550`.
    - Items 2, 4, 5, 6 and 7 unfinished: step 2 deleted Wyrd's analytical
      task-count, routing, leaf-split and placeholder scheme, and nothing
      has replaced it yet. This explains the "settled Interactive" and
      "graph did not settle" journeys, the off-by-one footer read
      (`persisted_input` prefetches hot footers before `FilePruner` runs),
      and the two out-of-date oracle unit tests. User (2026-10-09): do NOT
      bring back that code; rebuild analytical on native
      `datafusion-distributed` and the fork only.
    - Wyrd re-pinned: DataFusion `a4ff145`, arrow-rs `0d0eb24`,
      datafusion-distributed `0b81b12`.
  - Items 2+4+7 design (research agent, checked against the forks):
    - Item 2:
      - No Wyrd codec. The fork's `ParquetSource` proto carries predicate,
        projection, options and per-file statistics.
      - The planning session keeps `.with_distributed_planner()` defaults,
        `LiveUnionBoundary` and `children_isolator_unions(false)`.
      - Upstream's `file_scan_config_bytes_per_partition` becomes an
        optional Oracle runtime value. `None` means the upstream default
        (production); journeys set 1 so tiny fixtures distribute.
    - Item 4: an upstream `WorkerPlanRewriteHandler` on the worker session
      swaps each decoded Parquet `DataSourceExec`'s reader factory for the
      tenant footer-proof factory over this node's warehouse objects. The
      tenant comes from the verified stage identity. Any other file scan is
      refused.
    - Item 7: `OracleQueryScanStats::visit` counts files and partitions from
      the pre-execution plan by descending every `DistributedLeafExec`
      variant and every `Stage::Local` input. Bytes and pruning still come
      from `fold_distributed_scan_metrics`, because
      `rewrite_distributed_plan_with_metrics` wraps nodes in an opaque
      `MetricsWrapperExec`.
    - Item 5 follow-up: `persisted_input` caches footers for the published
      files only, before hot files join the scan.
    - Item 6: `LiveScribeExec::handle_child_pushdown_result` takes the
      non-dynamic parent filters in the Pre phase, reports them all
      unsupported (the leader keeps its `FilterExec`), and `fragment()`
      encodes `FilterExec(predicate, RemoteSourcePlaceholderExec)`. The
      follower runs `FilterPushdown` after decode, so the predicate reaches
      the staged `ParquetSource`. Dynamic filters are not shipped.
    - Item 8, second half: the staged-scan row-group pruning test was
      re-added in the first half; the live predicate peer journey is
      `peer_network::security::leaf_predicates_round_trip_and_execute`. Its
      Struct `IN` and Variant `->>` predicate is planned by a default
      session as a `FilterExec` over the placeholder, and a one-literal plan
      change under the signed fragment digest is refused.
    - Item 9: `distributed.rs` comments now describe per-task scans that
      carry the pushed predicate, with no follower assignment.
    - Journey class assertions that encoded the deleted rule change with
      the contract:
      - a scan and a GROUP BY on one table now get the same class;
      - an Interactive case uses a live-only or empty table;
      - no assertion depends on which follower runs a stage.
    - Fork gap, not Wyrd's: a single-task stage holding `LiveScribeExec`
      below a multi-task consumer can be routed to a worker and fails
      visibly (no encoding). If a journey reaches it, the fix is a
      coordinator-only leaf in `datafusion-distributed`.
    - Evidence after fixes (2026-10-08, fourth session):
      - `oracle::` unit tests 172/172, including
        `exec::tests::decoded_stage_scans_read_only_as_the_bound_tenant`
        and `live::tests::pushed_filter_ships_on_the_fragment_and_stays_on_the_leader`;
      - Scribe journeys 4/4 and `pg_bifrost_e2e
        builtin_variant_and_struct_payloads_are_queryable` pass against the
        published fork pins;
      - full `mise run test:bifrost`: see the next entry.
    - `wyrd-testing/tests/bifrost/oracle/published.rs` no longer asserts
      `bifrost_oracle_file_pruning_total`: Wyrd's pruner and its metric are
      gone, replaced by `DataFusion`'s `FilePruner`. The ranged-read claim
      that an excluded object's footer is never read stays.
    - Full `mise run test:bifrost` (fifth session): 8 of 9 lanes passed.
      In the Oracle journey phase, 42 of 53 passed. Two read-only
      diagnosticians traced the 11 failures:
      - Group A, 6 journeys (`stale_attempt`, `telemetry`,
        `pushdown_exchange_spill`, `single_planner`, `transport_drop`,
        `two_tenants`):
        - Symptom: an Analytical class was expected, Interactive was seen.
        - Evidence: no `DistributedExec` root for the 12-row,
          one-object tables.
        - Cause: one published object is one file group and a single
          partition. With an inexact row count and row estimates trusted,
          no repartition is added, so the plan has no stage boundary. Two
          objects do distribute.
        - Fix site: the fixtures. Analytical tables are published as two
          objects; Interactive cases read separate one-object tables.
      - Hot and Iceberg filtering, "files expected 1, saw 3":
        - Cause: files were counted from the plan before execution, but
          `FilePruner` skips files as it opens them.
        - Fix site: `OracleQueryScanStats::finalize` subtracts
          `files_ranges_pruned_statistics` (`PruningEvidence::files_pruned`,
          and `RemoteScanMetrics::record_pruned_files` for distributed
          folds).
        - Unit test: `exec::tests::scan_stats_exclude_files_distributed_tasks_pruned`.
      - `live_query_routes_only_relevant_scribes`, "saw [1,1,1]":
        - Cause: route event-time pruning was deleted along with
          `ScanPredicate`, but the spec keeps it.
        - Fix site: `LiveScribeExec::handle_child_pushdown_result` calls
          `LiveTableRoutes::selected_by`, which runs DataFusion's
          `PruningPredicate` over one `Statistics` per route, bounded by
          its time partition.
        - Unit test: `live::tests::event_time_predicate_selects_only_overlapping_routes`.
      - `published_workers_and_live_scribes_share_one_plan`, "leases on
        [1,2]": the assertion encoded the deleted single-destination rule.
        Task placement is now upstream's, so at least one frozen worker must
        lease.
      - Focused rerun of these 10, plus
        `pg_analytical_cancel_deadline_and_slow_consumer_leave_six_clean_nodes`
        and the new peer journey: 12/12 pass (`WYRD_LOG=info`).
      - MCP `agent_runs_three_table_analytical_query_through_mcp`
        (confirmed by a fresh read-only diagnostician):
        - Symptom: after the `max_rows: 1` refusal, the leader graph keeps
          `memory_bytes=1864`. The release times out, readiness goes false,
          and `await_cleanup_paused` times out (mcp.rs:173).
        - Evidence: the trace shows stage 0 `outcome="failed"`, then "leader
          graph was not confirmed drained". The plan is a leader-local
          `HashJoinExec` (CollectLeft) whose `OnceAsync` build side
          (`HashJoinInput` reservation) lives on the plan node.
        - Cause: `AnalyticalGraphLifecycle::settle` took the metric fold,
          which owns the executed root plan, only on success. On any other
          terminal the plan stayed in the graph entry, and the entry is only
          removed once the pool is idle, so the release waited on itself.
          `Drop` already dropped the fold.
        - Fix site: `settle` drops the unfolded fold after closing the
          exchanges and before the release. The forks are not involved:
          upstream caches the CollectLeft build on the plan node by design.
        - Evidence: `mise run test:bifrost:journey:oracle` 53/53 pass.

## Step 4: focused tests

```
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=distributed::iceberg_filtering_mechanisms_cover_all_table_kinds) | test(=published::struct_and_variant_share_physical_pushdown)'"
```

Plus the `vala-bifrost-redux` unit tests, and the fork tests for every
changed DataFusion crate with and without the `variant` feature.

Evidence (2026-10-09, Wyrd `029da306f`):
- the command above passes 2/2;
- `vala-bifrost-redux` `oracle::` unit tests pass 174/174;
- full `mise run test:bifrost` passes every lane (`exit=0`; Oracle journeys
  53/53);
- `mise run fmt`, `mise run lints` and `git diff --check` are clean;
- the fork tests are `scratchpad/verify6.sh`, finishing `ALL-OK` 38s before
  `a4ff14550` was committed. They cover fmt, clippy `--all-targets
  --all-features`, the `datafusion-datasource-parquet` lib with and without
  `datafusion-common/variant`, `datafusion-functions-variant`, the
  `datafusion` `parquet_integration` tests with and without `variant`, and
  sqllogictests.

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

### Step 5 evidence (2026-10-09, LTO `release`, `cargo +1.98.1`)

- Base is a fork export at `38b7a7a0a`. On is a clean export of the published
  `a4ff14550` with `variant`, so the excluded local edits are not included.
- Script: `scratchpad/bench/run.sh` and `resume.sh`. Results table:
  `scratchpad/bench/table.py`. The clean export links the base's ClickBench
  data, which `sql_planner` requires.
- p50 / p95, base → on:
  - `physical_plan_tpch_all`: 25.1 / 25.4 → 24.6 / 25.1 ms
  - `physical_select_all_from_1000`: 12.3 / 12.8 → 11.8 / 12.1 ms
  - `struct_access`: 2.65 / 2.74 → 2.60 / 2.69 ms
  - `filter_struct_field_eq`: 3.43 / 3.54 → 3.36 / 3.42 ms
  - `filter_top_level_with_struct_select`: 3.49 / 3.57 → 3.39 / 3.45 ms

  Non-Variant and Struct are unchanged.
- `parquet_variant_query`, p50 / p95, unshredded → shredded:
  - planning is equal in all 18 cases (e.g. narrow `sum_small_field`
    179 / 180 → 177 / 179 µs).
  - Whole-Variant reads, all under the 2x limit:

    | object | `select_variant` | `select_all` |
    |---|---|---|
    | narrow | 1.03 / 1.07 → 1.34 / 1.40 ms | 1.05 / 1.16 → 1.29 / 1.41 ms |
    | nested | 2.97 / 3.18 → 3.74 / 3.96 ms | 3.01 / 3.34 → 3.78 / 3.91 ms |
    | wide | 7.14 / 8.33 → 9.71 / 10.2 ms | 7.95 / 8.43 → 10.0 / 10.7 ms |

  - Key reads are lower in every case:
    - wide `select_small_field`: 5.79 / 6.09 ms → 272 / 298 µs
    - nested `select_inner_small_field`: 2.39 / 2.66 ms → 283 / 308 µs
    - narrow `select_small_field`: 981 µs / 1.02 ms → 267 / 283 µs
    - wide `select_two_string_fields`: 9.84 / 10.7 → 4.29 / 4.53 ms
- `bifrost_reads` (`release` profile: four codegen units, no LTO; mimalloc),
  DataFusion → Wyrd, p50 / p95. The task did not yet pass `--profile dist`,
  so these numbers are not `dist` builds:
  - Hot file:
    - `point_lookup`: 350 / 368 → 354 / 382 µs
    - `select_narrow`: 241 / 247 → 223 / 235 µs
    - `select_wide`: 9247 / 9908 → 10023 / 10787 µs
    - `count_star`: 185 / 204 → 198 / 210 µs
    - `group_by`: 416 / 427 → 410 / 420 µs
    - `time_range`: 386 / 392 → 387 / 400 µs
    - `struct_field`: 633 / 773 → 577 / 666 µs
    - `list_predicate`: 425 / 436 → 395 / 407 µs
  - Published:
    - `point_lookup`: 659 / 890 → 420 / 627 µs
    - `select_narrow`: 305 / 337 → 276 / 424 µs
    - `select_wide`: 9950 / 10939 → 10059 / 11578 µs
    - `count_star`: 10692 / 12726 → 621 / 911 µs
    - `group_by`: 1557 / 1700 → 600 / 764 µs
    - `time_range`: 690 / 807 → 460 / 643 µs
    - `struct_field`: 851 / 983 → 726 / 882 µs
    - `list_predicate`: 687 / 797 → 515 / 662 µs
  - Hot `select_wide` is about 0.8 ms above native at p50, the only case
    slower beyond noise. The gap predates this task and has the same size
    in earlier runs (`TASK-003-revision-query-latency.md`: 9488 → 10046 µs
    and 9325 → 10106 µs), so this work did not introduce it.

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

Done since this handoff: steps 1–5 (forks published and pinned: DataFusion
`a4ff14550`, arrow-rs `0d0eb24`, datafusion-distributed `0b81b12`) and the
step 6 code items. Remaining: a green `mise run test:bifrost`, fmt/lints,
the Wyrd commit, then steps 7 and 8.

Original list, in order:
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
