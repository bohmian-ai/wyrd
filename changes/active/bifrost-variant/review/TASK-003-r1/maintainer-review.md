# TASK-003 r1 — maintainer review (`maintainer-rev`)

Subject: `b145767dd` → `861a0342f` (`wyrd/bifrost-variant/TASK-003`), read with
`git diff` / `git show` only. Governing guidance:
`architecture/references/languages/maintainer-style.md`, `AGENTS.md` §4–§6 and
§16, and `architecture/agent-rules.md`. No source was modified.

## Overall result: **FAIL**

The structural direction is good. A maintainer can find and follow the new read
path. `TenantParquetReaderFactory`/`TenantWarehouseReads` (footer proof), the
`WorkerPlanRewriteHandler` binding, `LiveTableRoutes::selected_by` (which uses
`PruningPredicate` instead of an in-house interval), `ForgeObjects`,
`ShreddedResiduals`, `OracleSessionTemplate`, and `BifrostStorage` each have one
clear owner. Their methods follow the `Cards` handle shape.

The large deletions still left behind rustdoc and configuration that describe
mechanisms that no longer exist. Some of these are on safety-relevant
invariants: the inexact row count that protects the footer tenant proof, the
codec-position contract, and the storage shutdown contract. There are also
orphaned public knobs and dead error vocabulary. `agent-rules.md` makes
inaccurate or missing rustdoc on touched items `BLOCK_BEFORE_MERGE`. MNT-1 to
MNT-4 block. MNT-5 to MNT-9 should be fixed in the same remediation, because
each one is small.

## Changed-surface coverage

For each surface below, I read the changed symbol in its owning module, then
traced its callers and the relevant tests.

| Surface | Read | Notes |
|---|---|---|
| `oracle/exec.rs` (`PruningEvidence`, `RemoteScanMetrics`, `OracleQueryScanStats`, `record_distributed_scan_metrics`, `PhysicalEvidenceWalk`, `TenantParquetReader*`, `TenantWarehouseReads`, `HotObjects`, `hot_parquet_scan`, `PublishedAssignment`, `published_files`, `HotFileSource`, `set_event_time_bounds`, `OracleTableProvider::{cache_footers, persisted_input, scan}`, projection helpers) | full non-test body | MNT-2 |
| `oracle/nested_pushdown.rs` (`FileReadPlan`, `#[cfg(test)]` module) | full header and impl | preference P5 |
| `oracle/live.rs` (`selected_by`, `LiveScribeExec::{fragment, apply_expressions, handle_child_pushdown_result}`, new tests) | diff and context | clean; preference P4 |
| `oracle/analytical.rs` (`AnalyticalSessionBuilder`, `AnalyticalStageEgress::tenant`, `leader_session`, `planning_session`, `settle` fold drop, ingress config) | diff and context | MNT-2 (codec comment), MNT-5 |
| `oracle/session.rs`, `resources.rs` (`OracleSessionShape`, `OracleExecution::session_state`) | full or diff | MNT-2 (template claim) |
| `oracle/bindings.rs`, `codec.rs`, `peer.rs`, `query_stream.rs`, `mod.rs` (config, `OracleTelemetry::register`, planning) | diff and context | MNT-3, MNT-5, P1 |
| `oracle/follower.rs` (`execute`, `validate_assignment`, `preflight`, `FollowerResolutionError`) | header and changed region | MNT-2 |
| Deleted modules (`variant_sql`, `leaf_paths`, `analytical_scan`, `pruning`, `storage/cache`, `storage/telemetry`, bench) | symbol list grepped across `crates/`, `sdks/`, `mise.toml`, `architecture/bifrost-design.md` | no dangling code reference; doc residue is in MNT-2 |
| `storage/{mod,error}.rs`, `catalog/iceberg_storage.rs`, `catalog/bifrost_catalog.rs` | full or diff | MNT-4 |
| `forge/{compact,path,mod}.rs` | diff | MNT-9; otherwise clean |
| `wyrd-spec/src/vala/assignment_authority.rs`, `api.rs`, `wyrd-tonic` proto | full or diff | proto field numbers and names are reserved correctly; v9 domain separator is documented |
| `wyrd-server/src/{state,boot/mod,config}.rs` | diff and context | MNT-1, MNT-3, MNT-5 |
| `wyrd-storage/src/{signer,local,telemetry,factory}` | diff | MNT-6 |
| `wyrd-queue/src/{variant,batch_builder}.rs` | diff | clean, with a good test |
| `bench_support.rs`, `benches/bifrost_reads/*`, `mise.toml` | diff and head | MNT-2, MNT-7, MNT-8 |
| `wyrd-testing/src/bifrost/object_reads.rs` | outline and head | clean |
| Journey tests (`published.rs` and others) | spot-read | names and docs match behavior |

**Generated declaration parity.** No `.pyi`, `index.d.ts`, or SDK source
changed. The removed `wyrd-spec` `JsonSchema`/`ToSchema` types
(`PersistedFileAssignment`, `HotFileDescriptor`, `IcebergFileDescriptor`,
`ScanPredicate`, …) are referenced by no checked-in generated artifact. I found
no parity defect. `codegen:check` was not run on the candidate. A public
`wyrd-spec` type set changed, so AGENTS §11 still calls for that lane, along
with `test:principals:integration` for the served OpenAPI document.

## Material findings

### MNT-1 — Storage shutdown and abort docs describe a storage drain that was deleted (blocking)

- **Location.** `crates/wyrd/wyrd-server/src/state.rs`:
  - `Bifrost::shutdown` rustdoc (around line 1903);
  - `Bifrost::abort` (line 1991);
  - `abort_selected_owners` (lines 2000–2005);
  - the `Bifrost::bifrost_storage` accessor (line 1817).
- **What changed.** The diff removed `storage.close(deadline)` and
  `storage.abort().await`.
- **What the docs still say:**
  - shutdown says "only then is the storage owner closed";
  - shutdown says "a failed shutdown never leaves the metadata cache open, decoded entries retained, or object I/O admissible";
  - shutdown's `# Errors` lists `BifrostError::Internal` "Bifrost storage did not settle before shutdown deadline";
  - abort says "Awaits the storage owner's abort";
  - the accessor says it "makes … the single shutdown real".
- **Rule.** AGENTS §16 says rustdoc must describe how the operation works and
  list its error conditions. `agent-rules.md` says inaccurate rustdoc on a
  touched item is `BLOCK_BEFORE_MERGE`. AGENTS §6 says `async` must be earned.
- **Cost:**
  - An operator or maintainer reasoning about shutdown ordering or a hung drain
    will look for a storage settle step and an error code that cannot occur.
  - `abort_selected_owners` is now an `async fn` that awaits nothing, so every
    caller pays an `.await` for synchronous work.
- **Correction:**
  1. Rewrite the four docs to state the real order: the role drain, after which
     the shared `Arc<BifrostStorage>` simply drops.
  2. Remove the `Internal` storage-settle error from `# Errors`.
  3. Make `abort_selected_owners` synchronous.
  4. The public `abort` may stay `async` for API stability, but its doc must not
     claim it awaits storage.

### MNT-2 — Stale or contradictory rustdoc on the rebuilt Oracle read path (blocking)

All items below are on materially changed symbols. Each one tells a maintainer
something false about the mechanism they are about to change. Same rule as
MNT-1.

1. **Highest priority.** `oracle/exec.rs:1243`, `HotFileSource::row_count`,
   says the count is "Reported as the file's exact statistics". But
   `HotFileSource::statistics` (line 1267) and `hot_parquet_scan` deliberately
   make it `Precision::Inexact`. The reason is that an exact count lets
   `count(*)` skip opening the file, and with it the footer tenant proof.
   - Cost: a maintainer who trusts the field doc and "fixes" the precision
     re-opens a tenant-proof bypass.
   - Correction: say "inexact, so `count(*)` still opens the file and proves its
     footer".
2. `oracle/exec.rs:1007`: `hot_parquet_scan` says "Wyrd adds only the tenant
   proof of every footer and the Variant expression adapter". No adapter is
   installed; only the reader factory is. Delete the adapter clause. The
   trailing paragraph after `# Errors` (line 1021) also belongs above the
   section.
3. `oracle/analytical.rs:7144`: the `leader_session` comment says the planning
   config "already carries this node's Oracle codec — bound to the execution
   lock". `set_distributed_user_codec` no longer appears anywhere in `src/`.
   - Cost: the comment explains a codec-position hazard against a codec that
     does not exist, so the next person to add a codec gets the wrong mental
     model.
   - Correction: state that no user codec is installed on this path, and keep
     the position warning for anyone who adds one.
4. `oracle/follower.rs`:
   - Line 56: `FollowerResolutionError` still describes "A published follower".
     That role is gone.
   - Lines 768–774: the comment says "The leader's own plan has only remote
     leaves … only place a distributed query can observe physical read volume".
     Distributed scans now report through `record_distributed_scan_metrics`, and
     the new `FilterPushdown` block now sits between this comment and the
     snapshot it describes.
   - Line 810: `validate_assignment`'s rustdoc still begins with `preflight`'s
     text ("Performs the complete IO-free validation phase … # Errors …"), and
     `preflight` (line 856) has no rustdoc. The candidate edited this exact doc
     block.
   - Correction: split the doc back onto `preflight`, and move or rewrite the
     two comments.
5. `oracle/exec.rs`:
   - Lines 1–5: the module doc mentions "invariant operators" and "Tenant
     validation surrounds that union". Validation is now the footer reader.
   - Line 384: `open_distributed_scan` mentions "the same one an Interactive
     remote scan reports through". It is now the live Scribe leaf.
   - Line 1715: `OracleScanProjection` mentions "the remote placeholders, the
     provider-local filter". There is no provider-local filter.
6. `oracle/session.rs:1`: the doc says "The session template every Oracle query
   session starts from". `AnalyticalExecution::leader_session`
   (`analytical.rs:7151`) still builds `SessionStateBuilder::new().with_default_features()`.
   Either route it through the template or narrow the claim.
7. `bench_support.rs`:
   - Line 224: `OracleBenchTables` still says a hot table reads through "the
     Variant SQL surface, the leaf path rule".
   - Lines 293 and 443: `native_session` and `wyrd_session` document
     `# Panics … no Oracle capability`, but neither touches a composition now.
     The only panic is in `bench_session_config`'s partition expect.
   - `benches/bifrost_reads/main.rs` says the sides differ by "Wyrd's installed
     rules and functions". Wyrd installs no functions now.
   - These docs mislead whoever reads benchmark evidence.

**Correction for MNT-2 overall:** doc-only edits, plus moving the `preflight`
doc back onto `preflight`.

### MNT-3 — `OracleMemoryResources::reconciliation_limit_bytes` is orphaned, but boot still computes it and can refuse on it (blocking)

- **Location:**
  - `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:239` (a `pub` field
    documented as "Maximum bytes reserved by one query for reconciliation state");
  - `crates/wyrd/wyrd-server/src/boot/mod.rs:1582–1588`, which derives it from
    managed memory and fails boot with "Oracle reconciliation budget is zero".
- **Why it is orphaned.** Its only reader was the deleted exec.rs reservation
  code (base `exec.rs:4421`). Nothing reads the field now.
- **Rule.** AGENTS §15: remove speculative work and fix at the owner. The
  maintainer guide says a public signature must not imply behavior it does not
  have.
- **Cost.** A documented per-query limit that nothing enforces. A maintainer
  tuning reconciliation memory changes nothing. Boot keeps a refusal path for an
  unused budget.
- **Correction.** Delete the field, the boot computation and its error, and the
  constructor arguments.

### MNT-4 — `BifrostStorageError` keeps a vocabulary that only a unit test reaches (blocking)

- **Location.** `crates/vala/vala-bifrost-redux/src/storage/error.rs`:
  - `BifrostStorageError::from_object_store` (new in this diff);
  - the `PermissionDenied` and `Backend` variants.
- **Usage.** `from_object_store` has one call site:
  `storage/mod.rs:373`, inside `#[cfg(test)]`. Production builds only
  `InvalidConfiguration` (policy and composition) and `NotFound`
  (`bifrost_catalog.rs:498`). Every real backend failure now flows as
  `object_store::Error` through Iceberg, Parquet, and DataFusion.
- **Rule.** Maintainer guide: a public type must not allow or advertise states
  that do not occur. AGENTS §15: delete over add.
- **Cost:**
  - Readers of a `pub` enum reasonably believe Bifrost classifies permission
    and backend failures, and will write `match` arms or alerts for them.
  - The test proves a mapping that production never uses.
- **Correction.** Delete `from_object_store`, `PermissionDenied`, and `Backend`.
  Keep `NotFound` and `InvalidConfiguration`. Drop the `from_object_store`
  assertion from `local_store_round_trips_and_reports_missing_objects`; the
  `error_chain_contains_not_found` assertion already covers that test's intent.

### MNT-5 — `file_scan_bytes_per_task: Option<usize>` encodes "positive" only in server validation

- **Location:**
  - `wyrd-server/src/config.rs:491`, with a `Some(0)` check at around line 3128;
  - `oracle/mod.rs:928` (`pub OracleConfig`) and line 1326;
  - `oracle/analytical.rs:6695` (`pub AnalyticalExecutionConfig`) and line 7044.
- **Rule.** Maintainer guide, "Structs and methods": its own example uses
  `NonZeroUsize` so that an invariant lives in the type. AGENTS §4 asks for
  domain types.
- **Cost:**
  - Four structs carry the value. Only the TOML entry point rejects zero.
  - Any other constructor of the `pub` `OracleConfig` (tests, harness,
    enterprise composition) can pass `Some(0)`.
  - That only surfaces when an Analytical query plans, as an opaque
    `QueryExecutionFailed` from the `map_err` at `analytical.rs:7046`.
- **Correction.** Use `Option<NonZeroUsize>` end to end. Serde already
  deserializes it, and the server's zero check then becomes a type error.

### MNT-6 — `BackendSigner` now has two definitions of every operation, split by feature

- **Location.** `crates/wyrd/wyrd-storage/src/signer.rs`. There are five
  operations. Each has a `#[cfg(feature = "cloud")] async fn` and a
  `#[cfg(not(feature = "cloud"))] fn … -> std::future::Ready<…>`, with
  duplicated rustdoc. Converting `LocalSigner`'s methods to synchronous is the
  right change and should stay.
- **Rule:**
  - Maintainer guide: function shape and the main path should be visible once.
  - AGENTS §5 asks for cohesive owners.
  - `agent-rules.md` documents a sanctioned clippy mechanism, which the base
    used.
- **Cost:**
  - Every change to a signer operation (arguments, error, docs) must be made
    twice.
  - The public signature changes shape with a cargo feature (`async fn` versus
    `fn -> Ready`). Feature unification can surprise a downstream crate.
  - This storage churn is outside the Variant read-path task.
- **Correction.** Keep one `async fn` per `BackendSigner` operation, whose
  `Local` arm calls the synchronous `LocalSigner`. Silence only the no-cloud
  lint with
  `// justification: …` + `#[cfg_attr(not(feature = "cloud"), allow(clippy::unused_async))]`.
  If that lint does not fire in any lane, use a plain single `async fn`.

### MNT-7 — The `bench:bifrost:reads` task claims a build profile it does not use

- **Location.** `mise.toml:554–556`.
- **The mismatch.** The description says "Built like the shipped server: `dist`
  profile (fat LTO, one codegen unit)". But `run` is `cargo bench …` with no
  `--profile dist`. The workspace defines no `[profile.bench]`, so the bench
  inherits `release` (`codegen-units = 4`, no LTO).
- **Cost.** Anyone reproducing the TASK-003 latency evidence through the
  documented task gets non-LTO numbers labeled as LTO, which invalidates
  comparisons against the recorded results.
- **Correction.** Append `--profile dist` to `run`, or remove the claim from the
  description.

### MNT-8 — New `bench_support` items break the import and signature rules

- **Location.** `crates/vala/vala-bifrost-redux/src/bench_support.rs`:
  - `publish` (line 346) and `unpublished_table` (line 457) add function-scoped
    `use` blocks;
  - every new signature spells fully qualified paths, for example
    `datafusion::error::Result<iceberg::table::Table>`,
    `std::sync::Arc<crate::catalog::iceberg_storage::BifrostIcebergStorage>`,
    and `crate::oracle::exec::HotFileSource`.
- **Rule.** `agent-rules.md` says all `use` statements go at module top, and
  signatures use bare names. Under AGENTS §5, existing drift is not precedent
  for new items.
- **Cost.** The module's real dependency surface (iceberg memory catalog,
  transactions, Parquet reader) is invisible at the top of the file, and the
  signatures are hard to scan.
- **Correction.** Hoist the imports into a `#[cfg(feature = "test-support")]`
  `use` block and use bare names in the new items. Old items may stay as they
  are.

### MNT-9 — An agent-tooling marker in production code

- **Location.** `crates/vala/vala-bifrost-redux/src/forge/compact.rs:182`,
  `// ponytail: one full walk per page …`.
- **Rule.** `agent-rules.md`: "Never mention references to plans, tasks, or
  other agents in the codebase". The maintainer guide says not to leave agent
  notes.
- **Cost.** `ponytail` is a skill name that means nothing to a future
  maintainer. The ceiling it records is valuable but mislabeled.
- **Correction.** Keep the sentence and drop the tag. For example:
  `// One full walk per page (O(pages × objects)); only the local filesystem lists unordered.`

## Uncertain preferences (non-blocking)

- **P1.** `OracleTelemetry` (`oracle/mod.rs:402`) is a zero-sized "namespace"
  struct holding only associated functions. This predates the candidate; the
  candidate only renamed `new` to `register`. The sibling
  `telemetry::register_analytical_series()` is a free function. Moving
  `register()` there would match the nearby pattern and the
  no-zero-sized-utility-struct rule. It is not blocking, because the shape
  predates this diff.
- **P2.** `hot_parquet_scan(files, objects, tenant, schema, options)` and
  `published_files(scan, assigned)` are free functions threading
  tenant, store, and options. `TenantWarehouseReads` or a small
  `HotScan { objects, tenant }` owner could host them. They are pure plan
  builders with three callers, so either shape is defensible.
- **P3.** `record_distributed_scan_metrics(plan, sink)` could be a
  `RemoteScanMetrics` method, since it only writes into `sink`.
- **P4.** New `#[test]` functions in `live.rs` contain function-scoped `use`
  blocks. The agent-rules exception names `mod tests { use super::*; }` scope.
  Whether that extends to per-test imports needs maintainer calibration.
- **P5.** `oracle/nested_pushdown.rs` is now a `#[cfg(test)]`-only module whose
  name describes a deleted feature. `FileReadPlan` could move into
  `exec::tests`, or the file could be renamed `file_read_plan.rs`.
- **P6.** `OracleTableProvider` has two consecutive inherent `impl` blocks
  (`exec.rs:1353` and `1425`) that could be merged.
- **P7.** `iceberg_storage::store_error` now omits the `object_store` message
  from the display text and keeps it only as the source. That is fine for
  scrubbing, but tracing that prints only `Display` loses the cause.

## Residual verification gap (not a maintainer finding)

`mise run codegen:check` and `test:principals:integration` were not run on the
candidate. `wyrd-spec` public schema types were removed, and AGENTS §11 assigns
that change to those lanes. The other reviewers should confirm whether the
integrated verification covers it.
