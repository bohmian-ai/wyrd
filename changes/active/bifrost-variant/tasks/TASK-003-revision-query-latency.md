# TASK-003 revision: query latency, scan simplification, and close-out

Status: approved by the user on 2026-10-07 ("Approve"), together with spec
revision 25.

This revision applies to `TASK-003-shredding-and-leaf-pruning.md`. It lists
every remaining TASK-003 item: the code changes (Part A) and the close-out
(Part B). TASK-003 is complete when both parts are done.

## Handoff, 2026-10-08 (read this first)

### State

- Branch `wyrd/bifrost-variant/TASK-003`, HEAD `b145767dd`. Everything since
  is **uncommitted** (84 paths: direction changes 2–4 and the fixes below).
  Never commit `changes/active/bifrost-variant/spec.md` or
  `tasks/TASK-004-forge-parallelism.md`. Commit only when the user asks.
- DataFusion fork `bohmian-ai/datafusion`, branch
  `wyrd-arrow60-variant-read-plan`, pinned at `38b7a7a0` in `Cargo.toml`.
- Last verification, on the current tree: `mise run test:bifrost` 9/9 lanes,
  9/9 journey capabilities (redux 909/909, Oracle journey 53/53, Python 106,
  TypeScript 90/90); `fmt`, `lints`, `check:client-tier`, `py:lints`,
  `git diff --check` pass. The script exits 0 even on failure: read the
  `=== bifrost summary` line.

### Done (details in the sections below)

1. Direction changes 1–3: hot and published reads use DataFusion's
   `DataSourceExec` + `ParquetSource`; one scan, as DataFusion builds it.
2. Direction change 4: one Bifrost `object_store` (`LimitStore`); OpenDAL
   removed. The storage gap is closed (large reads within 0–1.5%).
3. `test:bifrost` failures after direction change 4, all fixed: tenant bypass
   via exact row counts (`Inexact` now), Bloom cast (fork `38b7a7a0`), tenant
   code lost across peers, late-failure ordering (4 tests), memory accounting
   (2 tests). Planner flag
   `execution.use_row_number_estimates_to_optimize_partitioning = true`
   (stock option) stops inexact counts from splitting every scan.
4. Variant/Struct journeys failing after that flag: fixed (see "Variant/Struct
   reads failed after the partitioning flag" under Status). Two changes:
   `LeafPathPushdown::leaf` leaves expression scan outputs untrimmed;
   `unshred_bare` exempts only literal-key `variant_get`.

### Benchmark now (`mise run bench:bifrost:nested-field-pushdown`, µs)

The approved labels are the five the benchmark prints.

| Case | `datafusion_struct` | `datafusion_struct_pushdown` | `wyrd_struct` | `wyrd_variant_unshredded` | `wyrd_variant_shredded` |
|---|---|---|---|---|---|
| wide/select_small_field | 286 | 290 | 316 | 15737 | 533 |
| wide/sum_small_field | 454 | 456 | 480 | 16129 | 669 |
| wide/select_one_string_field | 4842 | 4884 | 4841 | 21824 | 6328 |
| wide/filter_selective | 4918 | 3323 | 724 | 19061 | 5854 |
| wide/filter_broad | 6134 | 5911 | 5097 | 22896 | 10426 |
| nested/select_extra_string | 3705 | 3693 | 3741 | 9477 | 5202 |
| nested/select_inner_small_field | 301 | 315 | 332 | 7691 | 537 |
| nested/sum_inner_small_field | 459 | 456 | 484 | 7888 | 668 |
| narrow/select_small_field | 281 | 276 | 304 | 4028 | 516 |
| narrow/select_id_and_small_field | 305 | 318 | 333 | 4044 | 551 |

Against "Done when" and the user's target (±5% of DataFusion or better):

- `wyrd_variant_shredded` < `wyrd_variant_unshredded`: **met** on every case.
- `wyrd_struct` ≤ `datafusion_struct`: **met** on large reads and filters;
  **missed** on small reads by a fixed 25–30 µs per query (+5% to +10%).
- `wyrd_variant_shredded` ≤ `datafusion_struct`: **missed** on every case
  (+19% to +87%).

### Open drift: custom code that duplicates DataFusion

`oracle/leaf_paths.rs` (`LeafPathPushdown`, ~700 lines, a custom physical
optimizer rule) trims **Struct, List and Variant** paths. The rule above
allows only the Variant part. Struct field pruning is native DataFusion
(Parquet projection of `get_field`). Measured on 2026-10-08: when DataFusion
moves a path expression into the scan, native Struct pruning plus the
adapter's direct shredded read beat this rule's trim (running the rule
earlier so it trimmed those too: `wyrd_variant_shredded` 516 → 797 µs,
`wyrd_struct` 304 → 338 µs on narrow/select_small_field; reverted).

### Next, in order

Items 1–2 are done and item 3 is profiled; see "Progress, 2026-10-08,
second session" below.

1. **Cut `LeafPathPushdown` to Variant only, or delete it.** Remove its
   Struct and List path handling. Keep only what reads shredded Variant paths
   that DataFusion cannot. If `VariantFileAdapter` already covers those
   everywhere (in-scan expressions, filters, `unnest`, distributed followers
   via `RemoteSourcePlaceholderExec::with_leaf_paths` and
   `LeafPathPushdown::assign` in `codec.rs` / `analytical_scan.rs`), delete
   the rule. Prove with L1–L6 and record the L6 row.
2. **Profile `wyrd_struct` vs `datafusion_struct` on
   `narrow/select_small_field`** (the fixed 25–30 µs). The default bench
   binary has no debug info, so `perf` cannot unwind. Build a profiling copy
   without touching repo files:
   `CARGO_TARGET_DIR=<scratch>/target-prof CARGO_PROFILE_RELEASE_DEBUG=line-tables-only RUSTFLAGS="-C force-frame-pointers=yes" mise exec -- cargo bench --locked -p vala-bifrost-redux --features test-support,bench-support --bench nested_field_pushdown --no-run`,
   then `perf record -F 4999 -g --call-graph fp -- <binary> --bench --profile-time 10 '^narrow/select_small_field_wyrd_struct$'`
   and the same for `_datafusion_struct`. Setup (file writes, ZSTD)
   dominates a flat profile; compare inclusive time under the timed closure
   (plan vs execute). Unverified lead from a profile without call stacks:
   `hashbrown` clone of the `ScalarUDF` map (~1%) appears only in
   `wyrd_struct` — possibly a per-query session-state clone.
3. **Profile `wyrd_variant_shredded`** the same way. Unverified leads from
   the same stackless profile: `VariantMetadata::try_new_with_shallow_validation`
   3.3%, `VariantAsText::invoke_with_args` 2.1%,
   `EncodedVariant::validate` 1.7%, `Variant::new*` ~2.5%,
   `unshred_variant` 1.0%. Repeated validation per value is the first thing
   to check (Variant shredding code is allowed custom code; validation that
   arrow already did is not).
4. Fix only at the root cause found, with the Ponytail ladder; record
   Symptom / Evidence / Cause / Fix site here; rerun L1–L6.
5. **Part B close-out** (B1–B8 below), including B2.
6. **Query capacity under load:** `mise run bench:bifrost:query-capacity`;
   show published-data query latency improved against the last recorded
   capacity run.

### Progress, 2026-10-08, second session

Steps 1 and 2 done; step 3 profiled, fix waits on a user decision.

Verification on this tree: L1 `fmt` and L2 clippy pass; L3 836/836; L4
`test:bifrost:integration:redux` 909/909; L5 `test:bifrost:journey:oracle`
53/53; L6 recorded in the Benchmark log. L7 and L8 not yet run.

1. **`LeafPathPushdown` cut to Variant.** `LeafPathPushdown::paths` keeps a
   column only when it holds a Variant (`tables::holds_variant`, now
   `pub(crate)`). A column with no Variant is never trimmed; DataFusion's
   Parquet projection prunes it. The `get_field` / `array_element` steps stay
   in the walk because they are the only way to reach a Variant inside a
   Struct or List (spec revision 23). Not deleted: the adapter does not cover
   paths that stay above the scan (filters DataFusion keeps above it, `unnest`
   elements, follower placeholders); without the rule those read the whole
   column and unshred it.
2. **`wyrd_struct` small-read gap (fixed).** Diagnosis:
   - Symptom: `narrow/select_small_field_wyrd_struct` 299 µs vs 274 µs.
   - Evidence: frame-pointer `perf` of both forms, inclusive µs per query
     (main thread 296 vs 268 µs, matching Criterion). Wyrd-only frames:
     `LeafPathPushdown` `push`/`leaf` ~6 µs; `iceberg::util::available_parallelism`
     (reads cgroup quota files) under `TableScanBuilder::new` in
     `published_files` ~4.4 µs; rest spread over DataFusion internals
     (field-id metadata in schema projection ~3 µs, `Arc<dyn ObjectStore>`
     clone in `create_reader` in both forms). The `ScalarUDF` map clone lead
     was not found.
   - Cause: (a) `LeafPathPushdown::leaf` rebuilt every hot scan
     (`try_pushdown_projection`, `compute_properties`) even when it trimmed
     nothing; (b) Oracle built an Iceberg `TableScan` for a table with no
     snapshot, which plans nothing but probes host parallelism each time.
   - Fix site: (a) `LeafPathPushdown::leaf` returns the scan unchanged when no
     column is trimmed; (b) `published_files` returns no file for a leader
     scan of a table with no snapshot (A6 item). The follower path
     (`assigned` set) is unchanged, so drift is still refused.
   - Result: 299 → 260 µs; every case now meets `wyrd_struct` ≤
     `datafusion_struct` (Benchmark log, last row).
   - Not fixed, upstream: with a snapshot, every published scan still pays
     `available_parallelism` in iceberg-rust's `TableScanBuilder::new`
     (uncached cgroup read). Fix belongs in the `bohmian-ai/iceberg-rust`
     fork (cache it once); needs a fork push, so the user decides.
3. **`wyrd_variant_shredded` profile** (`narrow/select_small_field`, 493 µs
   vs 276 µs, gap ~217 µs per query):
   - Execution of `CAST(s ->> 'small_int' AS INT)`, ~140 µs:
     `VariantAsText` re-gates `VariantGet`'s output through
     `mask_placeholders` (~50 µs) and re-decodes each row with `try_value`
     (~12–18 µs); `VariantGet`'s `canonical_storage` unshreds its own result
     (~20 µs) plus casts (~7 µs); the final Utf8 → Int32 cast ~4 µs.
   - Planning, ~65 µs over Struct: logical optimize +24, expression-name
     formatting (`fmt`) +23, SQL to plan +10, physical optimize +8; the file
     adapter's `rewrite` +21 at execution.
   - Finding against A7's premise: `mask_placeholders` validates only the
     top-level `value` of a root. In a shredded file, residual `value` bytes
     nested under `typed_value` are first validated by `VariantAsText`'s gate
     on `VariantGet`'s output. Dropping that gate, as A7 says, would let those
     bytes reach rendering unvalidated. A7 needs the gate moved, not removed.
   - Finding against the target: even with zero UDF cost, planning alone
     leaves shredded small reads at ~350 µs, above `datafusion_struct`.
     `wyrd_variant_shredded` ≤ `datafusion_struct` on small reads needs
     DataFusion planning changes, outside Wyrd.

### Direction change 5, 2026-10-08 (user-approved): three like-for-like read benches

The user's standard: a benchmark stores the same data and compares like for
like. `datafusion_struct` vs a Variant form was not like for like, so the
five-form `nested_field_pushdown` bench is replaced by one bench target,
`benches/bifrost_reads/`, run by `mise run bench:bifrost:reads`
(`-- variant|hot|published` runs one section). It replaces spec revision 24's
five-form layout and the Criterion medians.

| Section | Left | Right | Data |
|---|---|---|---|
| `variant` | Wyrd unshredded Variant | Wyrd shredded Variant | the fork workload's 3 shapes, 10 cases, hot files |
| `hot` | native DataFusion `register_parquet` | Wyrd Oracle hot read | `events`, 8 cases, one hot file |
| `published` | native `IcebergStaticTableProvider` | Wyrd Oracle published read | the same file committed as an Iceberg snapshot |

- `events`: 4096 rows, 4 row groups; `id`, `service`, 4 KiB `body`,
  `latency_ms`, `attrs` Struct, `tags` List, and Bifrost's managed
  `wyrd_event_time` / `wyrd_ingested_at` (Oracle's empty projection reads
  `wyrd_event_time`, so every Bifrost table has it). Cases: `point_lookup`,
  `select_narrow`, `select_wide`, `count_star`, `group_by`, `time_range`,
  `struct_field`, `list_predicate`.
- Both sides use `bench_session_config()` (`OracleSessionShape`); the native
  session has no Wyrd provider, rule or function.
- Each pair must return identical rows before timing; then 3 warm-up and 30
  timed runs per side, alternating sides each run. A run is one query end to
  end (plan, execute, collect). Reported: p50 and p95 per side.
- Published fixture: `OracleBenchTables::publish` commits the file through an
  iceberg-rust `MemoryCatalog` over Bifrost storage with
  `parquet_to_data_file_builder` + `fast_append`. Native reads the same
  snapshot through iceberg's local `FileIO`.
- `OracleHotTables` is renamed `OracleBenchTables`; Criterion is no longer a
  dev-dependency of this crate.

First run (`mise run bench:bifrost:reads`, exit 0), µs:

| variant case | unshredded p50 | unshredded p95 | shredded p50 | shredded p95 |
|---|---:|---:|---:|---:|
| wide/select_small_field | 18972 | 19573 | 802 | 936 |
| wide/sum_small_field | 19576 | 23659 | 1086 | 1557 |
| wide/select_one_string_field | 21743 | 22818 | 9758 | 10365 |
| wide/filter_selective | 23899 | 26414 | 10182 | 11394 |
| wide/filter_broad | 24399 | 26870 | 10940 | 12363 |
| nested/select_extra_string | 9250 | 9631 | 5427 | 5798 |
| nested/select_inner_small_field | 7519 | 7708 | 589 | 658 |
| nested/sum_inner_small_field | 7789 | 8247 | 732 | 970 |
| narrow/select_small_field | 4021 | 4149 | 537 | 548 |
| narrow/select_id_and_small_field | 4063 | 4136 | 567 | 597 |

| hot case | datafusion p50 | datafusion p95 | wyrd p50 | wyrd p95 |
|---|---:|---:|---:|---:|
| point_lookup | 443 | 450 | 478 | 487 |
| select_narrow | 301 | 311 | 285 | 299 |
| select_wide | 9488 | 10423 | 10046 | 11368 |
| count_star | 241 | 276 | 264 | 314 |
| group_by | 517 | 546 | 509 | 549 |
| time_range | 467 | 478 | 519 | 533 |
| struct_field | 850 | 860 | 1650 | 1705 |
| list_predicate | 526 | 546 | 489 | 509 |

| published case | datafusion p50 | datafusion p95 | wyrd p50 | wyrd p95 |
|---|---:|---:|---:|---:|
| point_lookup | 683 | 867 | 562 | 604 |
| select_narrow | 365 | 371 | 348 | 358 |
| select_wide | 10029 | 10509 | 10103 | 10711 |
| count_star | 11148 | 13090 | 845 | 1052 |
| group_by | 1520 | 1895 | 630 | 675 |
| time_range | 704 | 770 | 631 | 702 |
| struct_field | 942 | 1251 | 1734 | 1892 |
| list_predicate | 707 | 955 | 630 | 845 |

Read of the first run:

- Shredded beats unshredded on every case.
- Wyrd's worst case is `struct_field` (filter on `attrs['region']`, project
  `attrs['code']`): hot p50 1650 vs 850 µs, published 1734 vs 942 µs.
- Hot point lookup (478 vs 443), count (264 vs 241) and time range (519 vs
  467 µs p50) are over native.
- Published: Wyrd is at or below native iceberg-datafusion except
  `struct_field`.
- Shredded small reads time higher here (537–802 µs) than under Criterion
  (493–505 µs). The loop alternates each shredded run with a 4–19 ms
  unshredded run; cause not yet measured.

### Diagnoses after direction change 5, 2026-10-08

Both were measured with a throwaway bench target (deleted) that timed each
phase separately (SQL → logical plan, logical → physical plan, execution;
median of 50–60 runs) and with frame-pointer `perf`.

**`struct_field` planning (fixed).**

- Symptom: `hot/struct_field` (`SELECT id, attrs['code'] FROM events WHERE
  attrs['region'] = 'eu'`) p50 1650 µs vs native 850 µs; published 1734 vs
  942 µs.
- Evidence: `EXPLAIN ANALYZE` shows Wyrd's `DataSourceExec` is not slower
  (opening 120 vs 319 µs, processing 354 vs 588 µs). Phases: logical 78 vs
  76 µs, execute 396 vs 359 µs, physical planning 1200 vs 384 µs. `perf` of
  physical planning: 85% in `ScalarFunctionExpr::eq`, 71% in
  `sorted_config_entries`. Wyrd's scan predicate was
  `get_field(attrs, region) = eu AND get_field(attrs, region) = eu`.
- Cause: `scan_predicate_physical_expr` built the provider's scan predicate
  with `ExecutionProps::new()`, so each `get_field` carried a fresh default
  `ConfigOptions`. DataFusion compares two function expressions by
  `Arc::ptr_eq` on that config, otherwise by every sorted config entry.
  Every comparison between the planner's copy of the filter and Wyrd's took
  the slow path, and the two never matched.
- Fix site: `oracle/exec.rs` `scan_predicate_physical_expr` /
  `scan_predicate_conjunction` / `hot_parquet_scan` now take the session's
  `&Arc<ConfigOptions>` and build with it. Callers: the leader scan
  (`state.config().options()`), the follower hot and published leaves and the
  Scribe live filter (`session.config().options()`); test fixtures pass their
  own options.
- Result: physical planning 1200 → 377 µs (native 381 µs). Still open, smaller:
  `ParquetSource` does not deduplicate conjuncts, so the predicate is still
  applied twice (provider `Inexact` + DataFusion's own pushdown); execution
  374 vs 362 µs.

**Shredded Variant read (diagnosed, not fixed).** `narrow` shape, shredded
file, per query:

| query | logical | physical | execute |
|---|---:|---:|---:|
| `SELECT id` | 24 µs | 51 µs | 60 µs |
| `s -> 'small_int'` | 46 µs | 145 µs | 149 µs |
| `s ->> 'small_int'` | 51 µs | 160 µs | 234 µs |
| `CAST(s ->> 'small_int' AS INT)` | 60 µs | 179 µs | 259 µs |

Arrow alone on the same file: read `metadata` + the `small_int` subtree
38.5 µs; `variant_get` returning `Int32` 0.5 µs; `variant_get` returning
Variant 24 µs; `VariantArray::try_new` 0.2 µs.

`perf` of the scan (where the expression runs), for the `CAST` query:

| share | where |
|---:|---|
| 38.5% | Parquet decode |
| 26.4% | `VariantAsText` → `decode_rows` → `mask_placeholders` → `EncodedVariant::validate`: re-validates every value `VariantGet` just built |
| 16.4% | arrow `variant_get` as Variant: re-encodes each typed `Int32` as Variant bytes |
| 10.6% | `VariantAsText` per row `try_value` + `to_json_value().to_string()` |
| 3.8% | `VariantGet` `canonical_storage` |
| 2.4% | `VariantGet` `mask_placeholders` on the root |
| 1.9% | final `Utf8` → `Int32` cast |

- Cause 1: validation sits on the wrong side. `mask_placeholders` checks only
  the root's top-level `value`. For a shredded root, the residual bytes under
  `typed_value` are not checked there, so `VariantAsText` validates
  `VariantGet`'s whole output again. That output is mostly bytes arrow just
  built from typed columns.
- Cause 2: `CAST(s ->> 'k' AS INT)` goes typed `Int32` → Variant bytes → JSON
  text → `Int32`. Arrow returns the typed column directly in 0.5 µs.
- Physical planning, +94 µs for `variant_get` over `SELECT id`, is
  DataFusion's own rules (`EnsureRequirements` 35%, `ProjectionPushdown`
  18%, `FilterPushdown` 10%; `LeafPathPushdown` 0.9%) on a plan with one
  more projection. It is not Wyrd code.

Fix options, which wait on the user (they change where untrusted bytes are
validated, or the results of a query):

1. Validate residual bytes once where they are read: extend the file-read
   gate to the residual `value` under `typed_value` that a path reads, and
   have `VariantAsText` skip re-validating `VariantGet` output. This removes
   ~26% of scan time with no change to results.
2. When a batch's leaf is fully typed (residual all null), `->>` renders the
   typed column with arrow's cast instead of building Variant bytes and JSON.
   Results are unchanged only for types whose arrow text equals the Variant
   JSON text (strings, integers, booleans); floats, decimals and timestamps
   render differently and would keep the current path.
3. A7 cast folding (`CAST(s ->> 'k' AS T)` → typed `variant_get`) changes
   results: a Variant string `"7"` returns `7` today and null after.

**Decision (user, 2026-10-08): option 1 now; option 3 waits on a spec
revision; option 2 dropped (per-type rendering is custom code).**

- Upstream check. DataFusion core has no Variant query (apache/datafusion
  #16116 open). `datafusion-contrib/datafusion-variant` @ `45fa2d75` is SQL
  glue over the same arrow-rs `variant_get` we call, on DataFusion 54 / arrow
  59, and TASK-001 excludes it by name. Its `VariantGetRewrite` is option 3,
  and it has the same result change: its Spark test for `"1"` → `1` is
  skipped (TODO #71). Arrow 60's typed kernel (`parquet-variant-compute`
  `type_conversion.rs` `as_num`) does not parse strings, casts booleans to
  1/0 and truncates floats, where today's text cast errors. Option 3 is
  therefore a spec revision (REQ: `->>` converts to text, then `CAST`).
- Option 1 implemented:
  - `wyrd-queue` `mask_placeholders` also validates every residual `value`
    under `typed_value` (objects, `List`, `LargeList`; other list layouts are
    refused) against the row's metadata, skipping rows nothing reads
    (`ShreddedResiduals`). Before, a residual under a shredded field reached
    arrow's shallow decoder unchecked unless a later function re-validated it.
  - `VariantAsText` has a `gated` flag. The `->>` planner's instance (argument
    always `variant_get` output, gated at its root) skips the gate; the
    registered SQL `variant_as_text` keeps it, since it can take a stored
    column. Same name, so a follower decoding by name gets the gating one.
  - Tests: `variant::tests::shredded_residuals_pass_the_read_gate`
    (wyrd-queue), and
    `oracle::variant_sql::tests::malformed_stored_variant_fails_every_function_without_panicking`
    gains `variant_as_text(v)` and a shredded residual through `->` and `->>`.
  - Lanes after the change: wyrd-queue lib 55/55, L3 836/836, L4 909/909,
    L5 journey pass, L6 `bench:bifrost:reads` exit 0. fmt and clippy
    (`-p wyrd-queue -p vala-bifrost-redux --all-features --all-targets
    -D warnings`) clean.
  - Shredded p50 / p95 (µs), before → after:

    | case | before p50 | before p95 | after p50 | after p95 |
    |---|---:|---:|---:|---:|
    | `narrow/select_small_field` | 758 | 1053 | 469 | 516 |
    | `narrow/select_id_and_small_field` | 787 | 989 | 506 | 592 |
    | `nested/select_inner_small_field` | 1015 | 1334 | 555 | 624 |
    | `nested/sum_inner_small_field` | 1375 | 1466 | 725 | 1328 |
    | `wide/select_small_field` | 1176 | 1421 | 879 | 1075 |
    | `wide/sum_small_field` | 1477 | 1592 | 1230 | 1539 |
    | `wide/select_one_string_field` | 10784 | 11861 | 9535 | 10870 |

### Rules the user set (verbatim intent)

- Use DataFusion / arrow / parquet / iceberg-rust for everything except
  Wyrd-specific logic and Variant shredding/querying. No other custom code;
  no duplicate logic; one obvious way.
- Use only the benchmark labels the bench prints when reporting.
- Benchmarks are like for like: the same stored data, native DataFusion vs
  Wyrd, or unshredded vs shredded (direction change 5).
- No benchmark checks added. Wyrd tests and benchmarks use Wyrd
  configuration (`OracleSessionShape`).
- Plain, short status. Keep this file updated. Never claim a test ran
  without its output.
- Git: local identity (Thorrester), no AI co-author trailers, no identity
  env vars, no stash/rebase/force-push; commit only when asked; messages end
  with the session line.

## Outcome

1. The nested-field benchmark meets current DataFusion performance: every
   Struct case is at or below plain DataFusion reading the same file with
   the same query.
2. Shredding makes querying faster: every shredded Variant case is faster
   than the same query on unshredded Variant, and at or below plain
   DataFusion's Struct time for the same query.

## How this is proven

No new scenario tests are written. The code is revised, simplified and made
faster. Existing tests, journeys and benchmarks prove it.

- A test that only exercised deleted code is deleted with it.
- A test that pinned an old internal shape (for example a partition count) is
  updated to the new shape.
- Nothing else in a test changes.

Proof lanes, run after every step unless the step says otherwise:

| Lane | Command |
|---|---|
| L1 format | `cargo +nightly-2026-03-05 fmt` |
| L2 lint | `CARGO_TARGET_DIR=target/clippy cargo +nightly-2026-03-05 clippy --locked -p vala-bifrost-redux -p wyrd-testing -p wyrd-client --all-features --all-targets -- -D warnings` |
| L3 redux lib | `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support'` |
| L4 redux journeys | `mise run test:bifrost:integration:redux` |
| L5 Oracle journeys | `mise run test:bifrost:journey:oracle` |
| L6 read benches | `mise run bench:bifrost:reads` (direction change 5) |
| L7 Forge journeys (deletes, lineage, rewrites) | `mise run test:bifrost:journey:forge` |
| L8 full gate, once at the end | `mise run verify:bifrost` |

Steps A1–A3 run L1–L3 and L6. Steps A4–A7 run L1–L7. Every L6 result goes
into the benchmark log at the end of this file.

## The rule

We only write new code for:

1. **Wyrd-specific implementation**: memory accounting, tenant proof on file
   footers, the staged-run lease, signed follower assignments, scan evidence
   and metrics, admission; and
2. **extending DataFusion** for what it lacks, such as Variant shredding and
   Variant querying (`variant_sql.rs`, the shredded-leaf read in
   `nested_pushdown.rs` and `leaf_paths.rs`, and the iceberg reader hooks
   they need).

Everything else uses DataFusion, arrow/parquet or iceberg-rust as-is. Every
ADD in the ledger names which of the two it is.

## Why

The benchmark runs in-process: SQL → plan → hot scan → collect, with no
server. It was far slower than plain DataFusion on the same workload. The
reference is the pinned fork's `parquet_struct_projection` bench, run on the
same machine.

| Case | Before (Struct / shredded) | After A1 (Struct / shredded) | Fork bench (LTO build, other data) |
|---|---:|---:|---:|
| narrow select_small_field | 764 / 986 µs | 366 / 628 µs | 208 µs |
| wide select_small_field | 788 / 1035 µs | 345 / 693 µs | 217 µs |
| nested select_inner_small_field | 793 / 1085 µs | 453 / 717 µs | 226 µs |
| wide sum_small_field | 1138 / 1271 µs | 680 / 809 µs | 340 µs |

## Direction change, 2026-10-07 (user-approved)

The first A4 attempt kept a Wyrd wrapper (`HotScanExec`) and a Wyrd reader
stack around DataFusion. A review found that it rebuilt the scan on every
`execute` (each partition then read every file), dropped DataFusion's own
hooks (`with_fetch`, filter and projection pushdown, sort pushdown,
`reset_state`), and duplicated DataFusion's partitioning, reader and footer
cache. The user directed (verbatim): "delete our duplicate implementations
NOW. then proceed with A", and approved design A, "including dropping Wyrd's
per-range/per-batch memory reservations and Wyrd's own footer cache in favor
of DataFusion's". Design authority documents do not override this: "just
because its authority doesnt mean its right. just do what is best and most
performant. we shouldnt invent complexity".

Design A, which replaces the earlier A4 plan below:

1. **Hot scan:** a plain DataFusion `DataSourceExec` (`ParquetSource` +
   `FileScanConfig`, exact manifest row counts), built at planning. No
   wrapper node.
2. **Store:** one small `ObjectStore` adapter over `BifrostStorage`
   (`get_opts`, `get_range`, `head` through the governed `read_range` and
   `stat`; every write returns `NotImplemented`). Wyrd-specific: keeps
   object IO on the node's one governed storage owner.
3. **Readers and footers:** DataFusion's `CachedParquetFileReaderFactory`
   over that store, with one node-wide DataFusion `FileMetadataCache`
   shared by every query runtime (`build_query_runtime`, through
   `CacheManagerConfig`). Its limit is the existing `metadata_cache_bytes`.
4. **Memory accounting (Wyrd-specific):** the cache limit is charged to the
   Oracle memory root once, up front, through the existing
   `OracleMetadataResources::try_reserve_metadata`, so footers never become
   an ungoverned second pool.
5. **Tenant proof (Wyrd-specific):** a reader-factory wrapper of about 30
   lines. Its reader's `get_metadata` calls the inner reader, then checks the
   `wyrd.bifrost.tenant` footer key once, before any row is read. Built at
   plan time with the tenant.
6. **Staged lease (Wyrd-specific):** carried in the `PartitionedFile`
   extension, so it lives as long as the scan.
7. **Iceberg leaf:** split with DataFusion's `FileGroupPartitioner` over task
   byte ranges; footers load through the same store, cache and tenant check.
8. **Accepted tradeoff:** in-flight IO bytes and decoded batches are no longer
   charged per range or per batch to the query pool. DataFusion's own
   operators still charge the per-query pool.
9. **Tests rebuilt on DataFusion:** tenant refusal; every row read once
   across splits; leader and follower read the same rows; the staged lease is
   held while the scan runs; Variant and Struct leaf decoding through
   `DataSourceExec`. Then L1–L7 and the L6 benchmark.

### Deleted so far (compilation is expected to fail until A4 lands)

- `oracle/exec.rs`: `scan_partitions`, `partition_byte_ranges`,
  `HotReadOverride`, `HotObjectSource`, `IcebergParquetReader`, `HotScanExec`,
  `OracleParquetReaderFactory`, `HotParquetPlan`, `HotParquetGovernance`,
  `HotRangeReservation`, `HotDecodedReservation`, `AccountedRangeOwner`,
  `FooterLoader`, `FooterLoadError`, `PublishedFooterLoader`,
  `PublishedFooters`, and the 34 unit tests and helpers that exercised only
  them.
- `oracle/nested_pushdown.rs`: test-only `FileReadPlan::apply`, its helpers
  `read` / `read_trimmed`, and the seven tests built on them.
- `oracle/leaf_paths.rs`: `trimmed_and_stock` and the three tests built on it.
- `storage/cache.rs` (Wyrd's node footer cache, whole file);
  `BifrostStorage::{object_metadata, governed_decode, bounded,
  decode_metadata, metadata_deadline, metadata_cache_enabled}`, the
  `metadata_cache` field and its `inspect`/`close`/`abort` handling, the
  `metadata_resources` argument of `BifrostStorage::new`, and the
  `StorageInspection::metadata_cache` field.
- `storage/telemetry.rs`: `CacheEffect`, `CacheEffectReason`,
  `MetadataLoadOutcome`, `record_cache_effect`, `record_load_terminal`,
  `record_waiter_settled`, `record_resident`, `record_inflight_loads`, and
  the `bifrost_storage_metadata_*` metric families.
- `storage/mod.rs` test `footer_decode_has_no_fixed_memory_slot`.

### Decisions made while implementing design A

These replace the matching items above where they differ.

- **Item 2, store placement.** The `ObjectStore` impl lives on the existing
  `BifrostIcebergStorage` (`catalog/iceberg_storage.rs`), not a new type. It
  already owns the storage handle, the warehouse, and `relativize`, so hot
  and published reads share one location-to-key rule. A bounded range read is
  one `read_range` with no `stat` (DataFusion already knows each file's size);
  `last_modified` is the epoch because objects are immutable. `BifrostCatalog`
  holds one `Arc<BifrostIcebergStorage>` (`objects()`); Oracle leader and
  follower both use it.
- **Item 3, cache owner.** The node cache is a DataFusion `DefaultCache` held
  on `BifrostStorage` (`with_metadata_cache`, `metadata_cache()`), attached at
  server boot and in the test server on Oracle-serving nodes, and in
  `BifrostStorage::for_test` when the fixture serves Oracle. It is passed to
  `CachedParquetFileReaderFactory` and to `DFParquetMetadata`, not through
  `CacheManagerConfig`, because the Oracle leaf carries its own reader factory.
- **Item 4.** The whole `metadata_cache_bytes` budget is charged once through
  `try_reserve_metadata` and held for the owner's life.
- **Item 6, staged runs.** Staged runs are local, rewritable files: they read
  through DataFusion's `LocalFileSystem` with the uncached
  `DefaultParquetFileReaderFactory`. The lease rides on each staged
  `PartitionedFile` as an extension.
- **Item 7, Iceberg leaf (revised; superseded by direction change 2 below,
  because its premise was false).** Forge writes position and equality
  delete files, so published files stay on iceberg-rust's `ArrowReader` inside
  `OracleIcebergScanExec`; a plain `DataSourceExec` would ignore deletes.
  `PublishedFooters` is now a ~40-line `ParquetMetadataLoader`: DataFusion's
  `DFParquetMetadata` over the warehouse store and node cache, then the tenant
  check. Execution-time splitting uses DataFusion's `FileGroupPartitioner`
  (else `FileGroup::split_files`). The plan-time partition count keeps
  DataFusion's listing rule inline in `with_partitions`, because Iceberg tasks
  are only known at execution.
- **One hot-leaf builder.** `exec::hot_parquet_scan(files, HotObjects, tenant,
  schema, predicates, options)` returns the `DataSourceExec` for leader hot
  files, follower hot assignments, and staged runs.
- **Leaf paths.** `LeafPathPushdown` pushes the trimmed projection straight
  into the `DataSourceExec`'s `ParquetSource` (`try_pushdown_projection`).
  The benchmark's leaf-path assertion reads the paths back from that
  projection (`LeafPaths::of_scan`).
- **Dead code removed with the governance path:** `hot_metadata_key`,
  `HotFileSource::metadata_key`, `PooledRangeOwner`,
  `OracleExecutionGrant::{memory, telemetry}`, `Oracle::{memory, telemetry}`
  (test-support keeps `resources` for inspection), `OracleTelemetry::new`
  (now `register`), `OracleResources::try_split_query_memory`,
  `storage::telemetry::gauge_value`, `ScribeTailResolver::{storage,
  staged_io}`, `staged_run_io`, and the unused `writer_epoch` / `binding`
  parameters.
- **Journey assertion changes.** `published.rs`: phase 2 is concurrent and
  repeated queries with equal rows; phase 5 asserts an excluded object's
  footer is never read (`read_range` starts == 0); cache-effect phases and
  constants are gone. `write_read.rs` asserts only `active_requests == 0`.
  `owner_inspection.rs` keeps `metadata_cache_bytes() > 0`.

### Unit tests rebuilt (item 9)

The first deletion pass also removed tests whose behavior is still live. Each
was restored onto the production path rather than left deleted:

- `exec.rs`, driven through `hot_parquet_scan`:
  `hot_parquet_split_partitions_read_every_row_once` (split by DataFusion's
  own `repartitioned`), `hot_parquet_refuses_foreign_or_missing_footer_tenant_before_any_row`,
  `struct_fields_of_null_parents_are_null_with_and_without_pruning` (Parquet
  pruning on and off), `retained_plan_uses_admitted_task_context_only` (the
  hot half calls `reset_state()` per run: a `DataSourceExec` owns one
  execution's shared work queue, and DataFusion requires a reset to run a plan
  again; production never re-executes a plan), and
  `supported_event_time_predicate_excludes_hot_files_before_any_footer_open`.
- `exec.rs`, published: `published_footer_loader_serves_cached_and_refuses_stale_or_foreign`
  (cache hit, stale classification, foreign-tenant refusal), and
  `iceberg_reader_honors_partition_byte_ranges`, which drives the new
  `partition_tasks` (the split `start_stream` already did, moved into one
  function so it is testable). **Deviation from the ledger**, which listed the
  byte-range split test as DELETE: the split is live production code that the
  journeys may not exercise at several partition counts.
- `nested_pushdown.rs`: the six Variant correctness tests
  (`both_readers_use_shared_per_file_plan`, `trimmed_variant_reads_only_its_paths_leaves`,
  `absent_variant_key_reads_null_when_every_residual_is_null`,
  `nested_variant_keys_filter_through_shredded_and_residual_leaves`,
  `shredded_and_residual_files_read_identically`,
  `typed_variant_statistics_prune_only_with_all_null_residuals`) keep their
  `FileReadPlan` leaf and row-group assertions and read rows through one test
  helper, `HotTable`, which serves a fixture file through `hot_parquet_scan`
  (SQL, or `ScanPredicate`s pushed into the scan).
- `leaf_paths.rs`: `literal_key_reads_trim_the_scan_and_match_stock_datafusion`
  and `other_uses_read_the_column_whole` run on `HotTable`; the stock side
  reads the unshredded file with Arrow's `ArrowReaderOptions::with_schema`.
- Deleted as orphans of removed code: `test_query_stream_from_physical`,
  `OracleQueryStream::test_from_physical`, `recorded::{observations, emitted}`,
  and the retired-metric assertion in `storage/mod.rs`.

### Direction change 2, 2026-10-07 (user-approved): published reads use the hot reader

User direction (verbatim): "we already established hours ago that the same
datafusion reader can be used for hot and published data. so why the fuck are
we still using the iceberg for reading? iceberg can be used for metadata, table
catalog, compaction, deletion, etc,." and "STOP WRITING CUSTOM FUCKING CODE
unless its an approved wyrd-specific implementation or related to varient
shredding/querying".

**Item 7 above was wrong.** It claimed Forge writes delete files. It does
not. No production code writes a position or equality delete file. Forge
compaction only consumes delete files that already exist
(`forge/managed/executor.rs`, `forge/managed/handoff.rs`), and
`forge/publication.rs` and `forge/orphan_gc.rs` only sort existing files by
type. Delete files are written only by test fixtures (`oracle/exec.rs` tests,
`tests/integration/forge/rewrite_support.rs`). Bifrost data is append-only
evidence, and retention drops whole files, so Wyrd has no reason to delete
single rows.

Decision:

1. **One reader.** A published leaf is `hot_parquet_scan` (the same
   `DataSourceExec` + `ParquetSource`) over the files Iceberg plans for the
   pinned snapshot. Iceberg is used only to list those files (manifest
   planning, with the pushed filter on the leader), plus metadata, catalog,
   compaction and deletion.
2. **Wyrd-specific checks kept**, moved onto the file listing:
   - a planned file with delete files or key metadata is refused (Wyrd
     writes neither; reading one without applying it would return wrong
     rows);
   - a follower's assigned files must all be in its pinned snapshot's plan
     (the existing drift refusal), and only assigned files are read;
   - a warehouse object that vanishes mid-read keeps its typed stale-object
     class, now raised from the tenant reader wrapper.
3. **Deleted:** `OracleIcebergScanExec` (with `with_partitions`,
   `with_footers`, `with_read_plan`, `file_read_planner`, `start_stream`,
   the lazy reader, the limit stream), `partition_tasks` and its byte-range
   test, `route_published_reads`, `PublishedFooters` and its test,
   `OracleScanMetricsHandle` and `retain_iceberg_task`,
   `PublishedFileReadPlanner`, and `restrict_plan_to_assigned_files`.
4. **Kept for the journey:** `iceberg_projection_probe` records the closure a
   published leaf is built with, now from the shared published-leaf builder.
5. **Known limit:** the DataFusion reader matches columns by name, not
   Iceberg field id. Bifrost never renames a column, so this is the same rule
   hot files already use.

### Direction change 3, 2026-10-07 (user-approved): one scan, as DataFusion builds it

1. **Fork fix (approved: "make the one line fork fix").** DataFusion's
   `CachedParquetFileReader::get_metadata` turned the footer-load error into a
   string, so a file Forge deleted mid-query lost its stale-object class (a
   follower reported it as a security failure). The fork now keeps the typed
   cause, as the reader's other two read paths already did:
   `bohmian-ai/datafusion` `e7c91e1e7`, pinned in `Cargo.toml`.
   `vanished_warehouse_object_fails_as_stale` passes.
2. **Benchmark checks removed (approved).** The benchmark is the agreed
   reference plus Variant unshredded and shredded, nothing else. The leaf-path
   and bytes-read asserts, `Case.leaf_paths`, `large_reads`,
   `smallest_large_leaf`, and the bench-support helpers `hot_leaf_paths` and
   `bytes_scanned` are deleted. Result and expression checks stay.
3. **Labels say reader and type (approved):** `datafusion_struct`,
   `wyrd_struct`, `wyrd_variant_unshredded`, `wyrd_variant_shredded`.
4. **Measured cause of the overhead** (`EXPLAIN ANALYZE`, `wide`, 50-run
   averages, throwaway probe, not committed). Stock DataFusion plans
   `AggregateExec(Single) <- DataSourceExec`: 119 µs execution for the SUM.
   Wyrd planned an always-present published leaf (empty here) beside the hot
   leaf under a `UnionExec`. Two input partitions made DataFusion fan out to
   `RepartitionExec(RoundRobin 32)` with partial and final aggregates, and
   `get_field` stayed in a `ProjectionExec` above the union: 568 µs
   execution. The profile's `ExecutionPlanMetricsSet::register` and
   `lock_slow` are those 32 partitions registering metrics.
5. **Decision (user: "implement and then remeasure. we need to have the same
   runtime performance as datafusion").** A leader plans its published and
   hot files as **one** `hot_parquet_scan`, exactly as a `ListingTable` plans
   one `DataSourceExec` over all its files. No union and no empty leaf.
   `published_parquet_scan` becomes `published_files`: it only lists and
   checks files (the decision-2 checks are unchanged). Separate per-tier
   leaves remain only under a remote placeholder, because a follower is
   delegated one tier at a time; that handoff is unchanged.

### Direction change 4, 2026-10-07 (user-approved): one Bifrost object store

Spec revision 20 (REQ-032 to REQ-034, INV-010, AC-014). The storage-path
experiment below proved Wyrd's byte-proportional overhead is its storage path.
User: "I dont want 3 implemenation of storage. We get 2" and "Ill go with your
recommendation"; uploads overwrite plus read-back (approved).

Bifrost today has four routes: `BifrostStorage` (OpenDAL plus permits,
retries, timeouts, metrics, settlement), the `BifrostIcebergStorage` adapters,
the raw OpenDAL operator (Scribe, Forge, `iceberg-storage-opendal`), and
`LocalFileSystem` for staged reads. They become one `Arc<dyn ObjectStore>`.

1. `storage/`: `BifrostStorage` builds and owns the store
   (`LocalFileSystem`, `AmazonS3`, `GoogleCloudStorage`, `MicrosoftAzure`
   from `wyrd-storage`'s `BackendConfig`, wrapped in `LimitStore`, cloud
   `ClientOptions`/`RetryConfig` from the policy) and the footer cache. Delete
   `run_read`, `run_once`, settlement, `close`/`abort`/`inspect`, the barrier,
   and `telemetry.rs`.
2. `catalog/iceberg_storage.rs`: the Iceberg `Storage` impl delegates to the
   store; the `ObjectStore` impl is deleted. Oracle scans read the store
   directly.
3. Scribe and `parquet/object_uploader.rs`: multipart upload plus the existing
   read-back; staged runs read through a local `LocalFileSystem`.
4. Forge: `OpenDalForgeObjectStore` deleted. `ForgeObjectStore` stays as the
   test fault seam (read, `list_pages`, stat, delete over `object_store`
   types); `ForgeObjects` in redux is its one production implementation.
5. Server boot and state, wyrd-testing harnesses: pass the store; storage
   shutdown hooks deleted.
6. Remove `opendal` and `iceberg-storage-opendal` from `vala-bifrost-redux`.
7. Verify: redux unit and integration lanes, `test:bifrost` journeys, lints,
   and the benchmark.

### Status

- Done: every deletion above; production code compiles; all redux tests
  compile with no warnings; 206/206 `oracle::`, `storage::`, and
  `catalog::iceberg_storage` unit tests pass (before direction change 2).
- Done: direction change 2; fork fix; benchmark checks removed and labels
  renamed.
- Done: direction change 3. One scan; the provider's duplicate `FilterExec`
  and `hot_parquet_scan`'s copies of the session's Parquet options are
  deleted; the projection probe records from `hot_parquet_scan`; read-path
  unit tests and the benchmark's Oracle session use `OracleSessionShape`
  (user: "wyrd tests and benchmarks should use wyrd configurations"). 243/243
  `oracle::`, `storage::`, `catalog::iceberg_storage`, `resources::` unit
  tests pass.
- Filter cases added (user-approved): `wide/filter_selective` (1 of 512 rows)
  and `wide/filter_broad` (461 rows), plus the form
  `datafusion_struct_pushdown` (stock DataFusion with Oracle's
  `pushdown_filters` and `reorder_filters`). Pushdown: selective 5119 → 3312
  µs, broad 6413 → 6229 µs, unfiltered unchanged, so Oracle keeps it on.
- Open: `wyrd_struct` over `datafusion_struct_pushdown` at identical settings:
  +60 µs (small reads), +200 µs (selective), +330 µs (one 16 KiB string
  column), +550 µs (broad). It grows with bytes read. Profile: object_store's
  default `get_ranges` (`Buffered` over ranges) and thread hand-offs
  (`Condvar`), because `BifrostIcebergStorage` serves each range through
  `BifrostStorage::run_read` (permit, timeout, retry, metrics) and OpenDAL,
  where stock reads through `object_store::LocalFileSystem`. Not yet proven;
  a storage-path change is a user decision.
- Open: shredded Variant filters are about 3× the Struct (selective 10370 vs
  3510 µs).
- Next: L1–L8.
- Done: direction change 4 as implemented.
  - One `Arc<dyn ObjectStore>` (`LimitStore` over `object_store`'s backend
    client) in `BifrostStorage`; Scribe, Oracle, Forge, the Iceberg `FileIO`
    and server boot all use it. `opendal` is gone from redux, wyrd-testing and
    wyrd-server production code (wyrd-server keeps it as a dev-dependency for
    wyrd-storage eval tests); the unused `iceberg-storage-opendal` workspace
    entry is removed.
  - Uploads: `BufWriter` (one PUT under the chunk size, multipart above),
    overwrite, then read-back verification.
  - Forge: dead `output_writer`, `write_output_chunk`, `abort_output_writer`,
    `read_range` and `list` seam methods removed (production never called
    them), along with the vacuous `output_writers() == 0` assertions;
    `GcEligibility::NotFile` removed (object stores list no directories);
    path validation is the exact `{warehouse}/{object_prefix}` comparison;
    the never-constructed `ScribeError::ObjectStorePutFailed` removed.
  - Server: `BifrostBuildInputs.storage` and the storage close/abort shutdown
    hooks removed.
  - Tests: journey assertions about the deleted storage lifecycle and
    `bifrost_storage_*` metrics removed (`published.rs`, `write_read.rs`); the
    held-reader and cancelled-read journeys now stall at
    `wyrd_testing::bifrost::object_reads::ObjectReadProbe`, a delegating store
    that `BifrostStorage::with_objects_for_test` installs in every test server.
    The pruning phase counts ranged reads on the same probe.
  - `test:bifrost` after the migration: 7 failures, diagnosed (read-only
    diagnosticians for every test-touching fix) and fixed:
    - **Tenant bypass (security).** Exact hot row counts let DataFusion's
      `AggregateStatistics` answer `count(*)` without opening a file, which
      skipped the footer tenant proof. `hot_parquet_scan` now reports them
      `Inexact`; unit test `hot_scan_row_counts_never_answer_a_query_unopened`.
    - **Bloom not loaded for `FixedSizeBinary` equality.** Scribe files
      carry `wyrd:sensitive` field metadata that the Iceberg table schema
      lacks, so DataFusion's adapter wraps `trace_id` in a same-type cast.
      `try_cast_literal_to_type` had no same-type rule for binary types, so
      the cast stayed, `LiteralGuarantee` saw no bare column, and no Bloom
      filter loaded. Fixed in the DataFusion fork (`38b7a7a0`: a literal
      already of the target type is returned unchanged; covers every binary
      Bloom column). HEAD's custom `HotBloomProbes` had hidden the gap.
    - **Tenant refusal lost its code across peers (2 Oracle journeys).** Once
      the foreign file is opened, the footer refusal was raised as a bare
      `BifrostError` inside `ParquetError::External`; datafusion-distributed
      forwards that as text, and `QueryCatalogError::find` only parsed text
      under `DataFusionError::External`. The refusal now carries the
      `QueryCatalogError` envelope, and `find` parses the chain's leaf text
      whatever wraps it; `late_catalog_error_keeps_its_identity` covers the
      local and forwarded Parquet shapes.
    - **Late-failure ordering (4 tests: Rust SDK, Oracle journey, Python,
      TypeScript).**
      With ≥2 partitions DataFusion inserts a round-robin repartition under
      `ProjectionExec`, so the failing second batch can overtake the first.
      Reproduced on a stock `ListingTable`; Oracle's minimum is 2 partitions.
      The tests now accept the refusal on either surface (precedent:
      `prove_worker_tenant_refusal`) and still require the exact problem and
      no partial result.
    - **Memory accounting (2 tests).** The footer cache charges its budget to
      the Oracle root at composition. `cluster_restart_…` expects that charge;
      `memory_refusal_…` sizes its holds from the root's free bytes.
  - Verification (2026-10-08): `mise run test:bifrost` — 7/9 lanes, the two
    failures (Oracle journey: tenant code; TypeScript: ordering) fixed and
    re-run green: `test:bifrost:journey:oracle` 53/53,
    `test:bifrost:journey:typescript` 90/90; redux 909/909; Rust SDK and
    Python journeys green. `fmt`, `lints`, `check:client-tier`, `py:lints`,
    `git diff --check` pass. DataFusion fork `casts` tests 26/26.
  - **Variant/Struct reads failed after the partitioning flag (Rust SDK,
    MCP, Python, TypeScript journeys).** Symptom: `array_element returned
    Struct(... typed_value ...)`, the shredded type. With no round-robin,
    `ProjectionPushdown` merged `array_element(events, 1)` into the scan;
    `LeafPathPushdown` (last rule) wrapped that whole expression in the
    trimmed read, which the file adapter cannot narrow or unshred. Any scan
    with no repartition above it (one partition, a split file, a follower)
    reached it without the flag. Fix (`LeafPathPushdown::leaf`): a scan
    output that is an expression is not trimmed; the Parquet reader prunes
    its Struct leaves and the adapter reads shredded Variant paths in it or
    unshreds the column inside it.
    - Tried and rejected: running the rule before DataFusion's first
      `ProjectionPushdown` so the trim lands under the expression. Correct,
      but the benchmark regressed (shredded small 516 → 797 µs, Struct
      303 → 338 µs): the trim replaced DataFusion's native Struct pushdown
      and the adapter's direct shredded read. Reverted.
    - The leaf-path unit fixture now runs `ProjectionPushdown` before the
      rule, as a session does. That exposed a second bug: in the scan, the
      adapter left a shredded column raw under any `variant_get`, but a
      computed key (`v ->> CASE ...`) needs the unshredded value.
      `unshred_bare` now exempts only literal-key `variant_get`
      (`literal_key_path`).
    - Tests: cases DataFusion moves into the scan moved to
      `other_uses_leave_the_column_untrimmed` (rule leaves them untrimmed,
      rows equal stock DataFusion), plus the journey's
      `events[1]['name'], events[1]['attributes'] ->> 'k'`, which fails with
      the journey's exact error without the fix. Filter and `unnest` cases
      stay in `literal_key_reads_trim_the_scan_and_match_stock_datafusion`.
    - Benchmark with the fix: back to the previous row (see Benchmark log).
    - Verification (2026-10-08, final code): `mise run test:bifrost` 9/9
      lanes, 9/9 journey capabilities (redux 909/909, Oracle journey 53/53,
      Python 106, TypeScript 90/90); exact journey
      `pg_tests::builtin_variant_and_struct_payloads_are_queryable` passes;
      `fmt`, `lints`, `check:client-tier`, `py:lints`, `git diff --check`
      pass.

## Findings

### F1. Scans split across every CPU (measured)

- **Symptom:** a 512-row, two-row-group read planned 64 partitions on a
  32-CPU host.
- **Evidence:**
  - the profile was dominated by `parking_lot` lock_slow (~16%), malloc
    (~11%), and `ExecutionPlanMetricsSet::register` /
    `BaselineMetrics::new` (~6%);
  - `ProjectionExec::execute` and `UnionExec::execute` were called once per
    partition (33×).
- **Cause:**
  - `HotParquetExec` and `OracleIcebergScanExec` split by
    `target_partitions` without statistics;
  - the empty Iceberg leaf (the bench has no snapshot) also took 32.
  - DataFusion's own rule does not split a scan whose exact row count fits in
    one batch (`enforce_distribution.rs:1549`). The fork's
    `repartition_file_min_size` default is 1 MiB (`config.rs:1675`), not
    10 MiB.
- **What A1 actually changed:** the empty Iceberg leaf now takes 1 partition,
  and that is the measured gain. The 3.3 MB hot files are over 1 MiB, so the
  hot scan still splits 32 ways; A1's hot rule never fires on this bench.
- **Fix site:** A4. The hot scan is DataFusion's `DataSourceExec` with exact
  row-count statistics from the footer, so DataFusion's own rule decides. A
  probe with the hot scan forced to 1 partition measured Struct at
  210–226 µs, against plain DataFusion's 270 µs on the same file.

### F2. Oracle re-implements library features (read from source)

The read path is about 7,200 production lines:
- about 3,000 are Wyrd-specific implementation;
- about 2,350 extend DataFusion (`variant_sql.rs`, `nested_pushdown.rs`,
  `leaf_paths.rs`, the iceberg hooks);
- about 900 duplicate a library feature.

The published scan stays on iceberg-rust's `ArrowReader`. DataFusion's
`ParquetSource` has no delete files, field-id mapping, partition constants or
row lineage. `iceberg-datafusion` has its own scan for the same reason, and
`spec.md` keeps "Moving the published read path to `ParquetSource`" as a
non-goal.

### F3. Per-query overhead (read from source; A4–A6 remove it)

1. **Serial hot IO:** `IcebergParquetReader` (`exec.rs:2741`) implements no
   `get_byte_ranges`. parquet's default therefore makes one storage request
   per column chunk, one after another.
2. **Filters run twice:** `OracleTableProvider::scan` adds a `FilterExec`
   (`exec.rs:2405-2420`). DataFusion adds its own residual filter above the
   provider anyway, because pushdown is `Inexact`.
3. **Per-partition and per-file rework:**
   - every partition compiles the predicate conjunction and the leaf
     projection;
   - every file rebuilds `ArrowReaderMetadata` from the cached footer and
     registers a `ParquetFileMetrics` that is discarded.
4. **Per-column settings:** `ConfigOptions::default()` is allocated per
   trimmed column and per typed comparison (`variant_sql.rs`).
5. **Unneeded Iceberg leaf:** it is always planned (`exec.rs:2120`), even
   when the cut has no published files.
6. **Session rebuilds:** up to four full DataFusion session builds per query
   (planning, execution, each follower fragment, each analytical leaf).
   Measured by F4 before anything is changed for it.
7. **Extra repartition on analytical leaves:** since A1, a small analytical
   leaf resolves to fewer partitions than it advertised, so
   `AnalyticalScanExec::resolve` adds a round-robin `RepartitionExec`. This
   is correct but costs a node.

### F4. Profile of the remaining gap (measured)

- **Method:**
  - frame-pointer `perf -e task-clock` on the narrow cases;
  - a scratch probe timing parse/plan/execute separately over the same
    Oracle tables and plain DataFusion on the identical file.
  - The probe and the folded profiles are in the session scratchpad
    (`profile/`).

Wall time per narrow small-field query, as parse+logical / optimize+physical
/ execute = total, in µs:

| Configuration | µs |
|---|---|
| Fork bench, fork build (full LTO, 1 codegen unit) | 182 |
| Same fork bench, Wyrd release settings | 242–250 |
| Plain DataFusion on Oracle's Struct file | 45 / 138 / 85 = 270 |
| Oracle Struct, as benched (hot scan split 32 ways) | 49 / 134 / 145 = 330 |
| Oracle Struct, hot scan in 1 partition | 38 / 111 / 59 = 210–226 |
| Oracle shredded Variant, as benched | 63 / 173 / 383 = 622 |
| Oracle shredded Variant, 1 partition, no round-robin | 63 / 180 / 480–506 = 738 |
| Oracle shredded Variant, 1 partition, round-robin left on | 74 / 188 / 698 = 965 |

Causes, ranked:

1. **Variant row path, ~390 µs, serial (DataFusion extension).**
   `CAST(s ->> 'small_int' AS INT)` on a shredded file:
   - `FileRead` (`variant_sql.rs:804`) unshreds the typed column back into
     binary Variant (~100 µs);
   - `VariantGet` (`variant_sql.rs:414`) re-validates it and runs
     `variant_get` on the binary form (186 µs);
   - `VariantAsText` (`variant_sql.rs:644`) validates again and renders text
     (78 µs);
   - arrow then parses the text back into Int32 (18 µs).

   Validation is `wyrd-queue/src/variant.rs` `mask_placeholders` (:513) and
   `validate` (:284): a full metadata validation, a scan, a `catch_unwind`,
   and another full validation, per row and per call. That is 166 µs inside
   the steps above.
2. **Hot scan fan-out, ~70–120 µs on Struct.** See F1.
3. **Round-robin above the Variant projection, ~230 µs.** DataFusion adds a
   32-way `RoundRobinBatch` because the scan reports no statistics. It must
   be fixed with cause 2: with only cause 2 fixed, Variant goes from 622 to
   965 µs.
4. **Build settings, ~60 µs, not Oracle.** The fork's 182–208 µs uses full
   LTO. Built like Wyrd, the same bench runs at ~245 µs. The A2
   `_datafusion` form removes this from the comparison.
5. **Per-execute fixed costs, 5–25 µs each**, multiplied by the partition
   count:
   - `ConfigOptions::default()` per `FileRead::expr` (8.7 µs);
   - `ArrowReaderMetadata::try_new` from the cached footer on every query
     (`exec.rs:3298`);
   - the footer-cache hit path building a loader future and telemetry
     (`exec.rs:3212`);
   - throwaway `ParquetFileMetrics` in `FileReadPlan::new`
     (`nested_pushdown.rs:67`);
   - the empty Iceberg leaf (`exec.rs:2129`, `available_parallelism`
     syscalls plus `Table::scan`), about 23 µs.
6. **Variant planning, about +117 µs over Struct.** Parse +14, optimize and
   physical +39, mostly `schema_name` formatting of a three-deep expression.
   INFERRED to be generic DataFusion cost; it shrinks once A7 folds
   `CAST(->>)` into one call.

## Change ledger

Every code item this revision touches. **DELETE** removes it. **REVISE**
changes it in place. **ADD** is new, and only under the rule above. Anything not
listed is unchanged.

### `crates/vala/vala-bifrost-redux/src/oracle/exec.rs`

| Item | Action | Step | Replaced by / change |
|---|---|---|---|
| `scan_partitions` | DELETED | A4 | `FileGroupPartitioner` |
| `OracleIcebergScanExec::with_partitions` | REVISE (done) | A1 | takes `&ConfigOptions`, sizes from snapshot summary |
| `HotParquetExec` + `ExecutionPlan`/`DisplayAs` impls | DELETE | A4 | `DataSourceExec` + `ParquetSource` inside `HotScanExec` |
| `HotParquetExec::with_partitions` | DELETE (revised in A1) | A4 | `FileGroupPartitioner` |
| `HotReaderFactory` | DELETE | A4 | `OracleParquetReaderFactory` |
| `hot_stream`, `hot_piece_metadata` | DELETE | A4 | `ParquetSource` opener |
| `HotBloomProbes`, `BloomProbe` | DELETE | A4 | `ParquetSource` Bloom pruning (`bloom_filter_on_read`) |
| `partition_byte_ranges`, `row_groups_in_byte_range` | DELETE | A4 | `FileGroupPartitioner`, `RowGroupAccessPlanFilter::prune_by_range` |
| `RowGroupSelection`, `parquet_column_index` | DELETE | A4 | `ParquetSource` row-group plan |
| `OracleScanMetricsHandle::record_hot_range`, `record_hot_file`, `record_row_groups`, `record_bloom_pruned`, `record_page_pruned_rows` | DELETE | A4 | DataFusion `ParquetFileMetrics` |
| `OracleScanMetricsHandle` (struct, Iceberg half) | REVISE | A4 | Iceberg counters only |
| `OracleQueryScanStats::visit` | REVISE | A4 | folds `ParquetFileMetrics` into `PruningEvidence` |
| `OracleParquetReaderFactory` | DELETED (rejected design) | A4 | DataFusion `CachedParquetFileReaderFactory` + tenant wrapper |
| `HotScanExec` | DELETED (rejected design) | A4 | plain `DataSourceExec` |
| `HotParquetPlan`, `HotParquetGovernance`, range/decoded reservations, `AccountedRangeOwner` | DELETED | A4 | accepted tradeoff (design A item 8) |
| `FooterLoader`, `PublishedFooterLoader`, `PublishedFooters` | DELETED | A4 | DataFusion `FileMetadataCache` |
| tenant-check reader-factory wrapper | ADD (Wyrd-specific: tenant footer proof) | A4 | wraps DataFusion's factory |
| `ObjectStore` adapter over `BifrostStorage` | ADD (Wyrd-specific: governed storage owner) | A4 | read-only `ObjectStore` |
| `IcebergParquetReader` | DELETED | A4 | DataFusion reader over the `ObjectStore` adapter |
| `HotObjectSource` | DELETED | A4 | the `ObjectStore` adapter |
| `OracleIcebergScanExec::start_stream` limit stream (lines 1707-1724) | DELETE | A5 | iceberg's limit stream |
| `OracleIcebergScanExec` predicate / leaf projection / planner built per partition | REVISE | A6 | built once, shared by `Arc` |
| `OracleTableProvider::scan` provider `FilterExec` (lines 2405-2420) | DELETE | A6 | DataFusion residual filter |
| `OracleTableProvider::persisted_inputs` | REVISE | A4, A6 | builds `HotScanExec`; skips the empty Iceberg leaf and a one-input `UnionExec` |
| test `hot_parquet_split_partitions_read_every_row_once` | REVISE (A1 done) | A1, A4 | runs on `HotScanExec` |
| test `scan_partitions_split_only_large_scans` | DELETE | A1 | covered by the split test and L6 |
| `HotParquetExec` unit tests of governance, tenant refusal, lease, retry/drop | REVISE | A4 | rebuilt on `HotScanExec` |
| unit tests of Bloom probes, byte-range split, row-group selection | DELETE | A4 | outcome proven by L4/L5 |

### `crates/vala/vala-bifrost-redux/src/oracle/follower.rs`

| Item | Action | Step | Replaced by / change |
|---|---|---|---|
| hot leaf (line ~344), `live_leaf` staged scan | REVISE (A1 done) | A1, A4 | `HotScanExec` under session options |
| `restrict_plan_to_assigned_files` | REVISE | A1, A3 | takes `&ConfigOptions`; keeps only the `IcebergTableScan` branch |
| `OracleIcebergScanExec` branch and `DataSourceExec`/`FileScanConfig` branch of `restrict_plan_to_assigned_files` | DELETE | A3 | unreachable: the catalog builds only `IcebergStaticTableProvider` |
| `physical_file_location` | DELETE | A3 | only the deleted branch used it |
| test `scribe_live_sources_keep_the_session_partition_count` | REVISE (done) | A1 | renamed `…_read_every_row_once_at_their_partition_count`; staged expects 1 |
| test `oracle_projection_is_exactly_the_authenticated_assignment` | REVISE | A3 | keeps only the duplicate-assignment assertion; the rest tested the deleted branch (live path proven by L5 distributed journeys) |
| test helpers `file_plan`, `plan_files` | DELETE | A3 | — |

### Other files

| File: item | Action | Step | Change |
|---|---|---|---|
| `oracle/leaf_paths.rs`: `LeafPathPushdown` | REVISE | A4 | targets `DataSourceExec`; Struct half deleted if DataFusion projection pushdown covers it, Variant half stays |
| `oracle/variant_sql.rs`: `ConfigOptions::default()` per column and per comparison | REVISE | A6 | one per adapter |
| `oracle/variant_sql.rs`: `OracleVariantSql` planner rule | REVISE | A7 | folds `CAST(->> / variant_get)` into typed `variant_get` |
| `oracle/variant_sql.rs`: `FileRead` unconditional unshred | DELETE | A7 | passes shredded input through to `variant_get` |
| `oracle/variant_sql.rs`: per-call validation in `VariantGet::invoke`, `VariantAsText::invoke` | DELETE | A7 | validated once where bytes enter |
| `wyrd-queue/src/variant.rs`: `mask_placeholders` | DELETE if no caller remains | A7 | — |
| `oracle/nested_pushdown.rs`: throwaway metrics set in `FileReadPlan::new` | DELETE | A6 | the scan's own metrics set |
| `oracle/exec.rs`: footer-cache hit path (line 3212) | DELETED | A4 | DataFusion `FileMetadataCache` |
| node metadata cache (`storage/cache.rs`) and its telemetry | DELETED | A4 | one node-wide DataFusion `FileMetadataCache`, limit `metadata_cache_bytes`, charged up front to the Oracle root |
| `BifrostStorage::new` `metadata_resources` argument | DELETED | A4 | — |
| workspace + crate `datafusion-datasource` dependency | ADD (already in DataFusion's tree, same fork rev) | A4 | `compute_all_files_statistics` |
| `bench_support.rs`: `hot_leaf_paths`, `bytes_scanned` | REVISE | A4 | read `DataSourceExec` / `ParquetFileMetrics` |
| `benches/nested_field_pushdown.rs`: `write_shape`, `main`, `check`, `smallest_large_leaf` | REVISE | A2 | four forms: plain DataFusion, Struct, unshredded, shredded |
| `architecture/bifrost-design.md` | REVISE | B3 | scan architecture and partition rule |
| `changes/active/bifrost-variant/spec-revision-25-hot-parquet-source.md` | ADD | A4 | one-sentence spec change |
| `TASK-003-shredding-and-leaf-pruning.md` | REVISE | B4 | stale notes, frontmatter, evidence |

## Part A — code changes, in order

### A1. Small scans read one partition per file (done, uncommitted)

- **`oracle/exec.rs`:**
  - new `scan_partitions(files, bytes, &ConfigOptions)` applies DataFusion's
    listing rule;
  - `HotParquetExec::with_partitions` and
    `OracleIcebergScanExec::with_partitions` take `&ConfigOptions`.
    `OracleIcebergScanExec` sizes from the snapshot summary
    (`total-data-files`, `total-files-size`) or the assigned file count;
  - the leader provider passes `state.config_options()`.
- **`oracle/follower.rs`:**
  - the follower hot leaf, `live_leaf`'s staged scan and
    `restrict_plan_to_assigned_files` pass `session.config().options()`;
  - `live_leaf` memtable grouping is unchanged.
- **Tests updated to the new shape:**
  - `hot_parquet_split_partitions_read_every_row_once` sets the threshold to
    0;
  - `scribe_live_sources_keep_the_session_partition_count` is renamed
    `scribe_live_sources_read_every_row_once_at_their_partition_count`, and
    the staged source now expects 1 partition.
- **Remove** the unit test `scan_partitions_split_only_large_scans` that was
  added during diagnosis; the split test and L6 cover the rule.
- **Proof:** L1–L3 already pass (873/873); L6 is recorded.
- **Correction (F1):** the hot half of A1 never fires on this bench, and A4
  deletes it. The `OracleIcebergScanExec` half stays.

### A2. Benchmark compares plain DataFusion, Struct, unshredded Variant and shredded Variant

All changes are in `benches/nested_field_pushdown.rs`:

- **`write_shape`** writes a third file, `<shape>_unshredded.parquet`. It
  holds `cast_to_variant` of the Struct column with no `shred_variant`, under
  the same Variant field metadata. The return type becomes three
  `(schema, path)` pairs.
- **`main`:**
  - registers the table `<shape>_unshredded`;
  - runs each case's `variant_sql` against it, with the table name replaced;
  - reports the ids `<case>_datafusion`, `<case>_struct`,
    `<case>_variant_unshredded` and `<case>_variant_shredded`.
- **`check`** gets a `byte_bound: bool` parameter:
  - true for Struct and shredded, unchanged;
  - false for unshredded, which must read the whole value. The function
    still checks the plan, the rows and the leaf paths. If the leaf paths of
    an unshredded read differ, the expected value is recorded from the first
    run and stated here.
- **Plain DataFusion reference.** `main` also builds a plain
  `SessionContext` (default config, no Oracle rules) with
  `register_parquet("<shape>", <struct file>)`. It runs each case's
  `struct_sql` as `<case>_datafusion`, checked for the same rows. This is the
  reference for the outcome on identical data. The fork's
  `parquet_struct_projection` numbers stay as context only, because its
  strings repeat and compress while ours vary per row.
- `smallest_large_leaf` ignores the unshredded file.
- The median table prints the four forms side by side.
- **Proof:** L6, every assertion passes.

### A3. Delete dead follower branches

In `oracle/follower.rs`, `restrict_plan_to_assigned_files` keeps only the
`IcebergTableScan` branch. That is the only plan
`IcebergStaticTableProvider::scan` returns, and it is the only provider the
catalog builds (`catalog/bifrost_catalog.rs:1378,1493,1531`).

- **Delete:**
  - the `OracleIcebergScanExec` branch;
  - the `DataSourceExec` / `FileScanConfig` branch;
  - `physical_file_location`;
  - their now-unused imports.
- **Stays fail-closed:** any other leaf leaves `file_leaves == 0`, which
  still returns the error.
- **Test:** `oracle_projection_is_exactly_the_authenticated_assignment`:
  - Its "restricted to the assigned file", "missing file" and "extra file"
    assertions run only through the deleted `DataSourceExec` branch, so they
    are deleted.
  - The live path's file-set checks (`OracleIcebergScanExec::plan_tasks`) are
    proven by the distributed journeys in L5 (`oracle/distributed.rs`, the
    follower `with_assigned_files` cases).
  - The duplicate-assignment assertion tests live code that runs before the
    plan is examined, so it is kept, on an `EmptyExec` plan.
  - Delete `file_plan` and `plan_files`.

### A4. Hot reads use DataFusion's `DataSourceExec` + `ParquetSource`

**Superseded on 2026-10-07 by design A** (see "Direction change" above).
The text below is kept only as the record of the rejected wrapper plan;
`HotScanExec`, `OracleParquetReaderFactory` and the kept
`HotParquetPlan`/`HotParquetGovernance` are not built.

- **Delete from `oracle/exec.rs`:**
  - `HotParquetExec` and its `ExecutionPlan` and `DisplayAs` impls;
  - `HotReaderFactory`, `hot_stream`, `hot_piece_metadata`;
  - `HotBloomProbes`, `BloomProbe`;
  - `row_groups_in_byte_range`, `partition_byte_ranges`;
  - `RowGroupSelection`, `parquet_column_index`;
  - the hot counters `record_hot_range`, `record_hot_file`,
    `record_row_groups`, `record_bloom_pruned` and `record_page_pruned_rows`
    on `OracleScanMetricsHandle`.
- **Add, Wyrd-specific only:**
  - `OracleParquetReaderFactory`, implementing DataFusion's
    `ParquetFileReaderFactory`. Its `create_reader` returns the governed
    `IcebergParquetReader` (A5 shape), whose `get_metadata` loads the footer
    through `BifrostStorage::object_metadata` and verifies the tenant key
    before any data range is read. It also owns the staged lease, an
    `Option<Arc<StagedSourceLease>>`, so the lease lives as long as the scan
    and its streams.
  - `HotScanExec`: one thin wrapper over the `DataSourceExec`. At `execute`
    it resolves the admitted grant and memory pool through the existing
    `HotParquetPlan::resolve`, hands the governance to the factory for that
    partition, and charges each yielded batch through the existing
    `HotParquetGovernance::reserve_decoded`. It carries `leaf_paths` for
    `LeafPathPushdown`. Every other `ExecutionPlan` method delegates to the
    inner exec.
- **The hot `FileScanConfig`:**
  - one `PartitionedFile` per `HotFileSource`, with size and location;
  - the closure schema;
  - projection from the leaf paths;
  - `ParquetSource` with the pushed predicate, `pushdown_filters`,
    `bloom_filter_on_read`, page index, and the custom factory;
  - expression adapter `OracleVariantSql::file_adapter()`;
  - each `PartitionedFile` carries exact statistics from the cached footer
    (`num_rows`, and byte size), so DataFusion's own distribution rule
    decides whether to split (F1, F4 causes 2 and 3). Oracle does not split
    eagerly; `DataSourceExec::repartitioned` does it when DataFusion asks.

  `scan_partitions` then serves only `OracleIcebergScanExec`.
- **Cache `ArrowReaderMetadata`** beside the footer in the node metadata
  cache, so a query does not rebuild the Arrow schema from the footer
  (F4 cause 5).
- **Object store:** register `file://` and the hot store URL on the session
  runtime env where the provider is built. Hot reads still go through the
  factory's reader, never through the store.
- **Callers moved to `HotScanExec`:**
  - `OracleTableProvider::persisted_inputs`;
  - follower `hot_leaf` / `signed_hot_source`;
  - follower `live_leaf` (staged runs);
  - `LeafPathPushdown`;
  - `OracleQueryScanStats::visit`, which now reads DataFusion's
    `ParquetFileMetrics` (`bytes_scanned`, `row_groups_pruned_statistics`,
    `row_groups_pruned_bloom_filter`, `page_index_rows_pruned`, files) into
    the same `PruningEvidence` fields;
  - `bench_support::{hot_leaf_paths, bytes_scanned}`.
- **Check before writing code, and stop with a finding if one fails:**
  1. Remote placeholders never serialize their local hot plan. A
     `FileScanConfig` with a custom adapter does not serialize
     (`datafusion-datasource-parquet` `file_scan_config/proto.rs:95`).
  2. DataFusion's Bloom lookup on a nested leaf. The spec non-goal already
     excludes Bloom on shredded Variant leaves. Losing Struct-leaf Bloom is
     recorded here, not patched.
- **Spec change (revision 25, `spec-revision-25-hot-parquet-source.md`):** in
  the read-core paragraph of `spec.md`, replace "`HotParquetExec` and the
  pinned Iceberg `ArrowReader` call the exact same facade" with "DataFusion's
  `ParquetSource` for hot files and the pinned Iceberg `ArrowReader` for
  published files call the exact same facade". It needs the user's approval
  together with this revision.
- **Tests:**
  - `HotParquetExec` unit tests in `oracle/exec.rs` that assert behaviour
    still owned by Oracle (governance, tenant refusal, lease lifetime,
    retry/drop paths) are rebuilt on `HotScanExec`;
  - tests of deleted internals (Bloom probe, byte-range split, row-group
    selection) are deleted, and the journeys in L4/L5 prove the outcome;
  - `hot_parquet_split_partitions_read_every_row_once` keeps its assertion
    (every row once across N partitions) on `HotScanExec`.

### A5. Published and hot reads fetch byte ranges in batches

- In `oracle/exec.rs`, `IcebergParquetReader` wraps iceberg's
  `ArrowFileReader` (`iceberg::arrow` `file_reader.rs:59`) instead of
  `HotObjectSource` plus its own bounds and short-read code.
- `get_byte_ranges` reserves the summed bytes through
  `HotParquetGovernance::reserve_range` before IO. It then delegates, so
  ranges are merged and fetched concurrently.
- Delete `HotObjectSource` and its bounds and short-read checks.
- `OracleIcebergScanExec::start_stream`: delete the copied limit stream
  (`exec.rs:1707-1724`) and use iceberg's.

### A6. Remove duplicate and repeated work

- **`OracleTableProvider::scan`:** delete the provider `FilterExec` and
  project the union directly. Before deleting, confirm that no distributed
  stage boundary relies on it; follower leaves apply their own signed filter.
- **`OracleTableProvider::persisted_inputs`:** skip the Iceberg leaf when the
  snapshot has no files in the cut, and drop the `UnionExec` when only one
  input remains.
- **`OracleIcebergScanExec`:** compile the predicate conjunction, the leaf
  projection and the file read planner once in its constructor, and share
  them through `Arc` with every partition.
- **`variant_sql.rs`:** build one `ConfigOptions` per adapter instead of per
  column and per comparison.
- **`nested_pushdown.rs` `FileReadPlan::new`:** pass the scan's real
  `ExecutionPlanMetricsSet` instead of registering a throwaway one.
- **Footer cache hit path (`exec.rs:3212`):** return the cached entry without
  building a loader future on a hit.

### A7. Variant reads the shredded typed column directly (DataFusion extension)

This is F4 cause 1. All changes are in `oracle/variant_sql.rs` unless noted.

- **Fold casts into the read.** `CAST(variant_get(s, path) AS T)` and
  `CAST(s ->> k AS T)`, where T is a primitive Arrow type, become one
  `variant_get` with `as_type = T`. Arrow's `variant_get` with a typed
  `as_type` reads a shredded `typed_value` leaf directly and falls back to
  the residual only for rows that need it. The planner rule doing this sits
  beside the existing `->>` rewrite in `OracleVariantSql`. `->>` without a
  cast keeps its text semantics.
- **Stop unshredding in `FileRead`.** When every consumer of a trimmed
  shredded column is a `VariantGet` with a path, `FileRead` passes the
  shredded `StructArray` through; arrow's `variant_get` accepts shredded
  input. It unshreds only for a consumer that needs the whole binary value
  (`VariantAsText` of the root, output of the column itself).
- **Validate once, at the trust boundary.** `VariantGet` and `VariantAsText`
  stop calling `wyrd-queue` `mask_placeholders` / `validate` on arrays Oracle
  itself produced from a file it already verified. Validation stays where
  bytes enter: ingest, and the footer/tenant-verified file read.
- **Delete** the code paths these remove: the per-call validation in
  `VariantGet::invoke` and `VariantAsText::invoke`, and `FileRead`'s
  unconditional unshred.
- **Ledger impact:** the `variant_sql.rs` A7 rows are REVISE. If
  `mask_placeholders` loses its last caller, it is DELETE in
  `wyrd-queue/src/variant.rs`.
- **Proof:**
  - L3, including `oracle::nested_pushdown` and the `variant_sql` tests;
  - L4/L5, including `published::struct_and_variant_share_physical_pushdown`
    and the residual / unshredded-key journeys;
  - L6, where `_variant_shredded` must beat `_variant_unshredded` and meet
    `_datafusion`.

Not doing: rewriting `LeafPathPushdown` onto DataFusion's projection pushdown
(high risk, no measured gain).

## Part B — close-out

- **B1. Commit the uncommitted finished work** with the step it belongs to:
  - A2 carries the benchmark: `benches/nested_field_pushdown.rs`,
    `bench_support.rs`, the bench session in `bindings.rs`, `oracle/mod.rs`
    visibility, the `Cargo.toml` criterion dev-dependency and `[[bench]]`,
    `Cargo.lock`, the `mise.toml` task, and
    `spec-revision-24-benchmark-shape.md`;
  - B3 carries `architecture/bifrost-design.md`.

  `spec.md` and `TASK-004-forge-parallelism.md` are never committed.
- **B2. Diagnose `wide/select_one_string_field` on Variant**, which takes
  about twice `nested/select_extra_string` although both return one 16 KiB
  string per row.
  - INFERRED from F4: `FileRead` unshreds the trimmed column, and
    `VariantGet` / `VariantAsText` validate and copy each 16 KiB value
    several times.
  - After A7, rerun L6. If the 2× remains, profile that case and record
    Symptom / Evidence / Cause / Fix site here.
- **B3. Architecture doc (`architecture/bifrost-design.md`):** keep this
  session's additions (Variant shredding, Nested-field reads, the
  `bifrost_variant_residual_share` metric), and add:
  - small scans read one partition per file, by DataFusion's rule;
  - hot files are read by `DataSourceExec` + `ParquetSource` with Oracle's
    reader factory and the `HotScanExec` wrapper;
  - published files are read by iceberg's `ArrowReader` with batched ranges;
  - the memory charge is taken per merged range before IO.
- **B4. Task file (`TASK-003-shredding-and-leaf-pruning.md`):**
  - Frontmatter `spec_revision: 25`.
  - Correct the stale notes:
    - "Added A … uncommitted" and "Added B … Uncommitted": both are
      committed;
    - Scenario 2 "Implemented, not pushed": the pins are pushed;
    - "Known gap … REQ-023's output leaf projection is still open": closed by
      `fdc6fd5ff`.
  - Replace the benchmark evidence table with the final three-way table.
  - Add the new-items table: `OracleParquetReaderFactory`, `HotScanExec`,
    `scan_partitions`, the A2 bench changes, each with the owners searched.
- **B5. Not in TASK-003:** the merge's open follow-up, moving JSON-row →
  Arrow conversion from `wyrd-queue/src/batch_builder.rs` to `wyrd-types`.
  Record it as a follow-up in the task file. It is unrelated to shredding or
  latency and touches the ingest path. The user decides.
- **B6. Verify:** L1–L7, then L8 once as the last command. Record exit codes
  and test counts in the task file.
- **B7. Review:** a fresh `$wyrd-task-review` of the cumulative candidate
  against TASK-003, spec revisions 18–25 and this revision. Fix-required
  findings are fixed before B8.
- **B8. Commit and push** to `wyrd/bifrost-variant/TASK-003`:
  - one commit per A step, B1 folded into A2, and B3/B4 as the last commit;
  - local identity; each message ends with the session line;
  - push with the scratchpad `lfs` PATH prefix.

## Done when

- Every lane L1–L8 passes.
- On the final L6 run (direction change 5), for every case. These targets
  replace the five-form ones and await the user's confirmation:
  - `variant`: shredded p50 < unshredded p50;
  - `hot` and `published`: wyrd p50 ≤ datafusion p50, within the user's
    ±5% of DataFusion.
- A missed case is reported with its profile and is not done; it is never
  hidden or excluded.
- Oracle-owned scan code is only Wyrd-specific implementation and DataFusion
  extensions (the rule above).

## Benchmark log

Columns are DataFusion / Struct / unshredded / shredded.

| After | narrow small | wide small | nested inner small | wide sum |
|---|---|---|---|---|
| A1 | – / 366 / – / 628 µs | – / 345 / – / 693 µs | – / 453 / – / 717 µs | – / 680 / – / 809 µs |
| A2 (noisy: a clippy build overlapped part of the run) | 304 / 446 / 3646 / 689 µs | 287 / 333 / 14944 / 618 µs | 292 / 414 / 6464 / 709 µs | 456 / 599 / 16676 / 769 µs |
| Direction change 2 + fork fix | 273 / 373 / 6144 / 1101 µs | 288 / 383 / 33474 / 1114 µs | 306 / 426 / 11458 / 1237 µs | 456 / 1011 / 34113 / 1387 µs |
| Direction change 3 (one scan, production session in tests and bench) | 272 / 338 / 4311 / 560 µs | 279 / 346 / 16607 / 560 µs | 295 / 361 / 8650 / 563 µs | 442 / 514 / 17034 / 698 µs |
| Filter cases added (DataFusion column is default options) | 283 / 343 / 4283 / 561 µs | 287 / 349 / 16373 / 560 µs | 296 / 368 / 8477 / 573 µs | 436 / 508 / 17099 / 698 µs |
| Direction change 4, first run (inexact row counts split every scan, F1 again) | 270 / 2672 / 4555 / 2579 µs | 284 / 2690 / 14126 / 2515 µs | 298 / 2711 / 7029 / 2578 µs | 449 / 2892 / 14533 / 2660 µs |
| Direction change 4 + `use_row_number_estimates_to_optimize_partitioning` | 272 / 303 / 4048 / 516 µs | 284 / 310 / 15668 / 520 µs | 292 / 329 / 7690 / 530 µs | 441 / 466 / 15879 / 666 µs |
| + expression scan outputs left untrimmed, literal-key-only shredded exemption | 276 / 304 / 4028 / 516 µs | 290 / 316 / 15737 / 533 µs | 315 / 332 / 7691 / 537 µs | 456 / 480 / 16129 / 669 µs |
| 2026-10-08: `LeafPathPushdown` trims only Variant-holding columns | 274 / 299 / 4106 / 516 µs | 290 / 319 / 16420 / 537 µs | 296 / 330 / 7902 / 535 µs | 457 / 485 / 16662 / 675 µs |
| + untrimmed scan kept as built, empty snapshot plans no Iceberg scan | 276 / 260 / 3974 / 493 µs | 294 / 279 / 15779 / 505 µs | 293 / 286 / 7671 / 502 µs | 456 / 443 / 16117 / 645 µs |

Last row, larger reads (DataFusion / Struct / shredded): wide one string
field 4792 / 4795 / 6379 µs; nested extra string 3717 / 3686 / 5121 µs; wide
filter selective 4924 / 692 / 6116 µs; wide filter broad 6158 / 5251 / 7810
µs; narrow id and small field 301 / 292 / 507 µs. `wyrd_struct` ≤
`datafusion_struct` now holds on all ten cases.

Direction change 4, larger reads (DataFusion + pushdown / Struct / shredded):
wide one string field 4795 / 4866 / 6334 µs; nested extra string 3682 / 3694 /
4972 µs; wide filter selective 3298 / 717 / 6511 µs; wide filter broad 5946 /
5085 / 7640 µs. The storage gap is closed (large reads within 0–1.5% or
faster); a fixed 25–37 µs per small query remains.

The first run's regression: the security fix reports hot row counts as
`Inexact`, and DataFusion's `EnforceDistribution` only keeps a scan whose
count fits one batch unsplit when the count is exact, unless
`execution.use_row_number_estimates_to_optimize_partitioning` is set.
`OracleSessionShape::apply` now sets it (stock option; pinned by
`oracle_reader_session_options_contract`). `perf` showed the F1 signature
(parking_lot lock_slow, per-partition metrics registration) before the fix.

### Storage-path experiment, 2026-10-07 (throwaway, reverted)

The Wyrd scan read through `object_store::LocalFileSystem` (footer cache kept)
instead of `BifrostIcebergStorage` → `BifrostStorage::run_read` → OpenDAL.
Medians, `datafusion_struct_pushdown` / `wyrd_struct` today / `wyrd_struct` with
`LocalFileSystem`:

| Case | DataFusion + pushdown | Wyrd | Wyrd on `LocalFileSystem` |
|---|---:|---:|---:|
| wide select small | 288 µs | 352 µs | 317 µs |
| wide sum small | 436 µs | 507 µs | 463 µs |
| wide one string field | 4843 µs | 4969 µs | 4779 µs |
| wide filter selective | 3307 µs | 3563 µs | 3293 µs |
| wide filter broad | 5994 µs | 6487 µs | 5925 µs |
| nested extra string | 3951 µs | 4145 µs | 3695 µs |
| narrow select small | 277 µs | 337 µs | 302 µs |

Proven: the storage path causes the gap that grows with bytes read; on
`LocalFileSystem` large reads match DataFusion. A fixed ~25–30 µs per query
remains on small reads and is not storage.

### Native DataFusion floor, 2026-10-08 (exploration, no code change)

Throwaway probe (`scratchpad/zz_native.rs`, not in the tree): native
`register_parquet` on a 4096-row, 3-column file, `bench_session_config`,
`release` profile, system malloc. p50 µs over 200 runs:

| query | sql→logical | optimize | physical | execute | total | cached plan, `reset_plan_states`, run again |
|---|---:|---:|---:|---:|---:|---:|
| select_narrow | 36 | 25 | 46 | 140 | 250 | 128 |
| point_lookup | 57 | 54 | 103 | 156 | 373 | 102 |
| count_star | 46 | 48 | 86 | 14 | 196 | 13 |
| group_by | 93 | 84 | 116 | 175 | 470 | 131 |

- A physical plan run a second time without `reset_plan_states` returns 0
  rows: the scan's file queue is consumed.
- `target_partitions` 32 vs 1 made no difference natively (one small file is
  one partition either way).
- `perf` sample shares (point_lookup / group_by): logical planning 28 / 35%,
  physical planning 40 / 41%, execution 47 / 41%; malloc/free self 11 / 11%;
  scan pruning (`FilePruner`, `PruningPredicateBuilder`, row-group and page
  pruning) ~14% / 0%; `FilterExec` properties + `FilterPushdown` 11% / 2%;
  `ListingTable::scan` 7 / 5%; zstd 5 / 7%; tokio blocking pool 3 / 4%;
  `TaskContext::from(&SessionState)` 2 / 2%; `recursive` stack checks 2 / 2%.

### Direction change 6, 2026-10-08 (user-approved): DataFusion-level optimizations

Order agreed with the user. Plan cache (exact SQL only) waits until query
repetition is known; it skips planning only on a repeat, so the planning work
below still pays on every other query.

| Step | Item | Where | Status |
|---:|---|---|---|
| 0 | Bench built like production: `dist` profile, mimalloc | `mise.toml`, `benches/bifrost_reads/main.rs`, dev-dep | `dist` reverted to `release` per user (no fat LTO); mimalloc kept. Baseline below was taken on `dist` |
| 1 | Planner internals: per-node schema rebuilds, expression clones in optimizer rules | fork `optimizer`, `sql` | done (local fork): plan signature skipped on the last pass; `RewriteSetComparison` returns early when no node has a set comparison (it merged input schemas per node); SELECT without UNNEST skips the unnest rewrite |
| 2 | Build the scan pruning predicate once (file, row group, page) | fork `pruning` `FilePruner`, `datasource-parquet` opener, `page_filter` | done (local fork): row groups reuse the file pruner's build when predicate and schema are unchanged and no dynamic filter can move; a single-conjunct page filter reuses the row-group build. File pruner now honours `max_in_list_size` |
| 3 | Fewer allocations in planning and scan setup | fork `execution` `TaskContext`, `core` `SessionState` | done (local fork): function registries are `Arc`-shared, so a per-query `TaskContext` no longer copies every function map (largest allocation site, 1.38% of samples). The rest is a long tail under 0.35% each |
| 4 | `FilterExec` properties computed once | fork `physical-plan` | no sound change: the second computation during filter pushdown is required, because the scan really changes (it gains the predicate's constants, and its statistics turn inexact) |
| 5 | `datafusion.optimizer.max_passes` 3 → 1 | session config | done; wyrd hot p50 point_lookup 312→301, select_narrow 174→170, group_by 368→366 µs (p95 noisy: profiling ran alongside) |
| 6 | Cache decoded Parquet reader metadata, not just the footer (built per file per query, twice when the file schema is coerced) | fork `datafusion-datasource-parquet` `opener/mod.rs:1014`, `:1082` (Wyrd uses DataFusion's footer cache) | open |
| 7 | Small local reads inline, not via tokio's blocking pool | Wyrd hot-file store | open |
| 8 | Execution context once per session | Wyrd Oracle | done: `OracleVariantSql::session` clones the template `SessionState` (as `SessionContext::state` does), sets the config with `config_mut`, and gives it a new catalog list holding DataFusion's default catalog. Planning no longer rebuilds; `session_builder` and `planning_runtime` are deleted. Execution still rebuilds once per grant to swap in the grant runtime. Bench sessions are built once, so the bench does not measure this. Test `template_sessions_share_functions_not_tables`; redux suite 910/910 |

Baseline for steps 1–8 (`mise run bench:bifrost:reads`, `dist` + mimalloc; first build 9m 40s):
#### Variant reads, Wyrd hot files: unshredded vs shredded

| case | unshredded p50 | unshredded p95 | shredded p50 | shredded p95 |
|---|---:|---:|---:|---:|
| `wide/select_small_field` | 14765 µs | 17335 µs | 606 µs | 924 µs |
| `wide/sum_small_field` | 14702 µs | 16943 µs | 679 µs | 1210 µs |
| `wide/select_one_string_field` | 17024 µs | 18304 µs | 6570 µs | 7332 µs |
| `wide/filter_selective` | 17980 µs | 20319 µs | 6617 µs | 7165 µs |
| `wide/filter_broad` | 19405 µs | 21921 µs | 7784 µs | 8584 µs |
| `nested/select_extra_string` | 8960 µs | 10716 µs | 5169 µs | 6345 µs |
| `nested/select_inner_small_field` | 7328 µs | 7457 µs | 382 µs | 477 µs |
| `nested/sum_inner_small_field` | 7527 µs | 8567 µs | 502 µs | 825 µs |
| `narrow/select_small_field` | 3751 µs | 4177 µs | 329 µs | 508 µs |
| `narrow/select_id_and_small_field` | 3813 µs | 4270 µs | 355 µs | 531 µs |

#### Hot file: native DataFusion vs Wyrd

| case | datafusion p50 | datafusion p95 | wyrd p50 | wyrd p95 |
|---|---:|---:|---:|---:|
| `point_lookup` | 287 µs | 305 µs | 312 µs | 331 µs |
| `select_narrow` | 183 µs | 193 µs | 174 µs | 182 µs |
| `select_wide` | 9325 µs | 10334 µs | 10106 µs | 11488 µs |
| `count_star` | 146 µs | 240 µs | 157 µs | 282 µs |
| `group_by` | 366 µs | 654 µs | 368 µs | 468 µs |
| `time_range` | 330 µs | 622 µs | 359 µs | 560 µs |
| `struct_field` | 556 µs | 589 µs | 561 µs | 616 µs |
| `list_predicate` | 367 µs | 419 µs | 338 µs | 353 µs |

#### Published Iceberg snapshot: native DataFusion vs Wyrd

| case | datafusion p50 | datafusion p95 | wyrd p50 | wyrd p95 |
|---|---:|---:|---:|---:|
| `point_lookup` | 645 µs | 841 µs | 411 µs | 461 µs |
| `select_narrow` | 282 µs | 354 µs | 256 µs | 315 µs |
| `select_wide` | 10087 µs | 11729 µs | 10085 µs | 12715 µs |
| `count_star` | 10849 µs | 11726 µs | 449 µs | 967 µs |
| `group_by` | 1420 µs | 1519 µs | 502 µs | 673 µs |
| `time_range` | 619 µs | 709 µs | 468 µs | 593 µs |
| `struct_field` | 803 µs | 954 µs | 731 µs | 899 µs |
| `list_predicate` | 630 µs | 734 µs | 462 µs | 572 µs |

Fork steps 2 + 3 and step 8 (local fork snapshot, `dist` + mimalloc), hot file, p50 / p95 µs, against the step 5 run:

| case | datafusion before | datafusion after | wyrd before | wyrd after |
|---|---:|---:|---:|---:|
| `point_lookup` | 275 / 374 | 273 / 412 | 301 / 480 | 297 / 451 |
| `select_narrow` | 178 / 226 | 174 / 193 | 170 / 249 | 166 / 175 |
| `count_star` | 155 / 193 | 136 / 165 | 158 / 227 | 143 / 153 |
| `group_by` | 373 / 433 | 338 / 345 | 366 / 403 | 339 / 350 |
| `time_range` | 317 / 359 | 310 / 325 | 348 / 386 | 342 / 353 |
| `struct_field` | 538 / 1050 | 524 / 567 | 540 / 869 | 531 / 733 |
| `list_predicate` | 357 / 471 | 355 / 429 | 331 / 380 | 328 / 389 |

Plus fork step 1 (planner internals), hot file, p50 / p95 µs, against the run above:

| case | datafusion before | datafusion after | wyrd before | wyrd after |
|---|---:|---:|---:|---:|
| `point_lookup` | 273 / 412 | 267 / 273 | 297 / 451 | 291 / 302 |
| `select_narrow` | 174 / 193 | 171 / 177 | 166 / 175 | 161 / 166 |
| `count_star` | 136 / 165 | 129 / 144 | 143 / 153 | 136 / 146 |
| `group_by` | 338 / 345 | 336 / 353 | 339 / 350 | 337 / 349 |
| `time_range` | 310 / 325 | 308 / 314 | 342 / 353 | 340 / 348 |
| `struct_field` | 524 / 567 | 513 / 543 | 531 / 733 | 528 / 555 |
| `list_predicate` | 355 / 429 | 348 / 403 | 328 / 389 | 321 / 351 |

DataFusion sqllogictest on the fork with steps 1–3: exit 0, 520/520 files.

### Spec revision 21, 2026-10-08 (user-approved): one read path, native DataFusion

Order set by the user: spec revision → delete → implement → focused tests →
release benchmarks.

#### Step 2, deletion: done

Deleted (production): the persisted-tier remote-source placeholder and
persisted follower assignments; `ScanPredicate`/`ScanLeaf`/`ScanLiteral` with
their protobuf and digest encoding; `analytical_scan.rs`; Wyrd event-time file
pruning; `AnalyticalCutTaskCount`, `OracleRouteTasks`, `AnalyticalLeafSplit`
and `frozen_destinations` (upstream `datafusion-distributed` defaults now
decide task counts, routing and leaf splits; `LiveScribeExec` gets
`Maximum(1)` natively); all `work_units` plumbing; the Analytical leaf binding;
`LeafPathPushdown` placeholder branches; Wyrd scan-metric constants.

Deleted (tests): the classifier and placeholder tests in `exec.rs`,
routing/leaf-split tests in `analytical.rs`, four persisted-assignment tests in
`follower.rs`, and two leaf-predicate tests in
`wyrd-testing/tests/bifrost/oracle/peer_network/security.rs`. Remaining
predicate tests now use DataFusion `Expr` filters through `FilterPushdown`.

Result: `mise exec -- cargo check --workspace --all-features --tests` clean
except `HotFileSource.event_time` never read (exec.rs), which step 3 uses.

Owed by step 3:

- Event-time file pruning from `PartitionedFile` statistics (`event_time`).
- Distributed scan metrics: files and partitions are not counted yet
  (`fold_distributed_scan_metrics`).
- Native coverage re-added: Scribe staged-scan row-group pruning
  (`FilterExec` + `FilterPushdown` over the resolved plan) and the live
  predicate journey in `peer_network/security.rs`.
- Stale comments in `wyrd-testing/tests/bifrost/oracle/distributed.rs`
  (`OracleCatalogResolver`, `assignment.predicates`, `with_assigned_files`).
