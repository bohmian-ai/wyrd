# TASK-003 r1: repository-standards review (`repo-rev`)

Subject: `git diff b145767dd 861a0342f` (120 files), branch
`wyrd/bifrost-variant/TASK-003`. Scope is repository standards only. This
review does not cover task acceptance, optional improvements, or the Ponytail
audit. No source file was modified. The `check:proto-drift` script rewrote
`crates/wyrd/wyrd-tonic/proto/wyrd.v1.bin` in the working tree as a side
effect, and that file was restored with `git checkout --` right away. Only
this report is new.

Check logs are in `<scratchpad>/stdrev/`
(`/tmp/claude-1000/-home-thorrester-Documents-GitHub-wyrd-bifrost-variant-task-003/47a7157d-7209-4e02-889a-b40216f53f0b/scratchpad/stdrev/`).

## 1. Authority coverage

| Changed surface | Applicable authorities |
|---|---|
| Oracle read path (`oracle/{exec,live,analytical,analytical_transport,follower,peer,codec,mod,session,query_stream,bindings,nested_pushdown}.rs`) | AGENTS.md §2, §4, §5 (struct style), §6, §9, §16; agent-rules (use/imports, bare names, rustdoc, async, no plan refs, struct style); refs `languages/rust-core.md`, `languages/maintainer-style.md`, `domain/olap-serving.md`, `domain/datafusion.md`; `architecture/bifrost-design.md` |
| Storage, catalog, uploader (`storage/*`, `catalog/*`, `parquet/object_uploader.rs`, `wyrd-storage/src/{signer,local,telemetry,factory}`) | AGENTS.md §4, §6, §15 (wyrd-storage owner), §16; agent-rules; `languages/errors.md` (crate-local `thiserror`); `domain/iceberg.md` |
| Forge (`forge/*`) | AGENTS.md §5, §16; agent-rules (audit vs lineage, struct style); `rust-core.md` |
| Server (`wyrd-server/src/{boot,config,otlp_*,state,verification,components}`) | AGENTS.md §9, §16; agent-rules (TenantConn/OperatorPool, audit outbox) |
| Shared (`wyrd-queue`, `wyrd-bench`) | AGENTS.md §2 (client tier: no datafusion/sqlx in client cone), §16 |
| `wyrd-spec` (`vala/api.rs`, `vala/assignment_authority.rs`) | AGENTS.md §2 (IO-free, PyO3-free, not a dumping ground), §8/§11 codegen; agent-rules (no hand-edited generated artifacts) |
| Proto (`wyrd-tonic/proto/wyrd.v1.proto`, `private_conversion.rs`) | agent-rules (generated artifacts); `check:proto-drift`; `check:no-tonic-outside-wyrd-tonic` |
| Cargo manifests (`Cargo.toml`, crate manifests, `Cargo.lock`) | AGENTS.md §4 (no wildcard versions, no per-crate profiles), §15 (dependency cost); agent-rules (earned features); `check:workspace-hack`, `check:object-store-pin`, `check:rustls-provider`, `check:no-testing-in-prod-deps` |
| `mise.toml`, `mise.lock` | AGENTS.md §11; `languages/testing-workflows.md` |
| Benches (`benches/bifrost_reads/*`, `src/bench_support.rs`) | AGENTS.md §16; agent-rules (top-of-module `use`, bare names); maintainer-style (documentation matches behavior) |
| Rust tests (`wyrd-testing/**`, `wyrd-client/tests/pg_bifrost_e2e.rs`, `tests/integration/forge/*`, inline `mod tests`) | AGENTS.md §11 (journey tier, negative flows), §16 (rustdoc incl. `# Panics` on test helpers); agent-rules (test placement, `pg_*`) |
| Python integration test | AGENTS.md §11, §16 (top-level `def test_*`); `py:format:check`, `py:lints` |
| TypeScript integration test | AGENTS.md §11; `languages/typescript-guide.md`; maintainer-style |
| Docs (`architecture/bifrost-design.md`, `docs/.../schema.svx`) | AGENTS.md §11 (`docs:check`); agent-rules (no plan refs in permanent artifacts) |
| Commits | AGENTS.md §13 |

## 2. Rule results

| # | Rule (source) | Result | Evidence |
|---|---|---|---|
| R1 | Generated artifacts regenerate from source, never stale (agent-rules; `check:proto-drift`) | **FAIL** | `wyrd.v1.proto` changed (removed persisted/predicate messages); `wyrd.v1.bin` not updated. `check:proto-drift` exits 1 (`check_proto-drift.log`). See STD-1. |
| R2 | Workspace-hack matches the feature union (`mise run check` → `check:workspace-hack`, part of `gate`) | **FAIL** | `cargo hakari generate --diff` shows arrow-family entries moved to the git patch and `object_store` aws/azure/gcp added (`check_workspace-hack.log`). See STD-2. |
| R3 | No plan/task/spec IDs in permanent code (agent-rules; maintainer-style) | **FAIL** | `Cargo.toml:230` "spec REQ-032"; `parquet/object_uploader.rs:157` "spec REQ-034". See STD-3. |
| R4 | Bare type names in signatures and struct fields (agent-rules) | **FAIL** | `bench_support.rs:293,319,351,429,443,461`; `oracle/bindings.rs:242`; `resources.rs:4074`; `oracle/mod.rs:1143`; `wyrd-storage/src/signer.rs` (`std::future::Ready` returns); `tests/integration/forge/support.rs` (`vala_bifrost_redux::forge::ForgeObjectPages`). See STD-4. |
| R5 | `use` only at the top of the module, except `mod tests` and single `Trait as _` (agent-rules) | **FAIL** | `bench_support.rs:352-355` and `:462` import types inside production (`bench-support`) functions. The function-scoped imports in `exec.rs`, `follower.rs`, `live.rs`, and `nested_pushdown.rs` are inside `#[cfg(test)]` modules, which is allowed. See STD-5. |
| R6 | Rustdoc on every new or modified item (AGENTS §16; agent-rules) | PASS (minor gap) | Scanned every added item declaration, field, and variant. All carry rustdoc except a local `const HOUR_MICROS` inside a test fn (`live.rs:976`) and test trait-impl methods (`telemetry.rs` FaultWriter/FaultDeleter, `forge_harness.rs` and `support.rs` `ForgeObjectStore` impls), which inherit the trait docs. Sampled docs (`settle`, `selected_by`, `TenantWarehouseReads`, `record_distributed_scan_metrics`, `OracleSessionTemplate`) are substantive. |
| R7 | `# Errors` on every fallible fn (AGENTS §16) | PASS | 62 new fallible fns. The only ones without `# Errors` are `ObjectStore`/`Display` trait impls in `wyrd-testing/src/bifrost/object_reads.rs`. |
| R8 | `# Panics` wherever a panic remains possible (AGENTS §16; rust-core) | **FAIL** | 10 new panicking helpers lack `# Panics`. See STD-6. |
| R9 | Struct-centered style: workflows that mutate an owner's state are methods (AGENTS §5; agent-rules) | **FAIL** | `oracle/exec.rs:433` `record_distributed_scan_metrics(plan, sink: Arc<RemoteScanMetrics>)`, `:464` `fold_distributed_scan_metrics` with `&mut Option<u64>` and `&mut PruningEvidence` out-params, `:105` `parquet_pruning` → `PruningEvidence`. See STD-7. |
| R10 | Async only at IO boundaries (AGENTS §6) | PASS | No new `async fn` without an await. The `wyrd-storage/src/signer.rs` local signing is now sync, returning `std::future::Ready` in non-cloud builds. |
| R11 | No `unwrap` in non-test IO paths; `expect` names an invariant (AGENTS §4) | PASS | `check:unwrap-audit` passed. |
| R12 | No unjustified `#[allow(clippy…)]` (agent-rules) | PASS | `check:clippy-allow-audit` passed. `mise run lints` is clean (implementer log). |
| R13 | Client tier: no sqlx/datafusion/cloud SDKs in the client cone; PyO3 scope (AGENTS §2, §7) | PASS | `check:client-tier`, `check:registry-client-tier`, `check:cli-client-tier`, `check:pyo3-scope`, `check:sdk-pyo3-scope` all exit 0. |
| R14 | Tenancy: TenantConn/OperatorPool only; RLS is the boundary (agent-rules) | PASS | `check:tenant-isolation` and `check:from-pools-allowlist` pass. No raw `PgPool` added. |
| R15 | Single object_store/datafusion/arrow/parquet versions (`check:object-store-pin`) | PASS | "OK: single object_store (v0.14.2) + single datafusion/arrow/parquet". |
| R16 | Single TLS provider (`check:rustls-provider`) | **FAIL** (outside changed surface) | See STD-9. |
| R17 | Every crate in one family lane (`check:test-coverage`, in `gate`) | **FAIL** (outside changed surface) | `wyrd-types` has no family lane. See STD-8. |
| R18 | Cargo features earned; no wildcard versions or per-crate profiles (agent-rules; AGENTS §4) | PASS | New `vala-bifrost-redux/cloud` only forwards `wyrd-storage/cloud` and the object_store cloud backends, and is enabled by `wyrd-server/cloud`. Versions are exact or git-rev pinned. No profile blocks were added. |
| R19 | No test tier in production deps; tonic confined (`check:no-testing-in-prod-deps`, `check:no-tonic-outside-wyrd-tonic`) | PASS | Both exit 0. |
| R20 | Public errors via the `WyrdError` derive; crate-local `thiserror` (AGENTS §4; errors.md) | PASS | `storage/error.rs` stays crate-local `thiserror` and maps `object_store` failures to fixed phrases. No hand-written code/status. The catalog is unchanged (codegen clean). The move from the query-time `parse_json` code to `WYRD_VALA_500_QUERY_EXECUTION_FAILED` follows spec revision 26 and `bifrost-design.md`, and the docs site was updated. |
| R21 | Contract/codegen drift (`codegen:check`, AGENTS §11) | PASS | `mise run codegen:check` exit 0 with no working-tree change (`codegen_check.log`). |
| R22 | Python: top-level `def test_*`, format, lint (AGENTS §11, §16) | PASS | Only an existing top-level test changed. `py:format:check` and `py:lints` exit 0. |
| R23 | Docs-site change runs `docs:check` (AGENTS §11) | PASS | `mise run docs:check` exit 0 (`docs_check.log`). |
| R24 | Journey tier covers every surface for touched user-visible behavior (AGENTS §11) | PASS | Rust (`pg_bifrost_e2e.rs`, `wyrd-testing/tests/bifrost/**`), Python, and TypeScript journeys were updated together. MCP query test updated. `test:bifrost` passed (implementer log). |
| R25 | Test placement: inline by default; `pg_*`/external only when earned (agent-rules) | PASS | New harness code is in `wyrd-testing/src/bifrost/object_reads.rs`. New unit tests are inline. |
| R26 | Scoped verification lane run (`verify:bifrost` = `check:bifrost` + `test:bifrost`) | PASS | fmt, lints, and `test:bifrost` ran (implementer). I added `check:bifrost-resource-governance`, `check:object-store-pin`, and `check:tenant-isolation`, all exit 0. |
| R27 | Git identity; no AI co-author trailers (AGENTS §13) | PASS | All 5 commits are authored and committed by `Thorrester <sjforrester32@gmail.com>`. No `Co-authored-by`. |
| R28 | Documentation describes the actual operation (maintainer-style) | **FAIL** (minor) | See STD-10. |

## 3. Material findings

**STD-1: stale generated protobuf descriptor (blocking)**
- Rule: agent-rules "Never hand-edit generated artifacts … regenerate"; `check:proto-drift` (`scripts/checks/proto-drift.sh`).
- Location: `crates/wyrd/wyrd-tonic/proto/wyrd.v1.proto` changed. `crates/wyrd/wyrd-tonic/proto/wyrd.v1.bin` was not regenerated.
- Consequence: gRPC reflection and descriptor-based MCP schema generation still advertise the removed `PersistedFileAssignment`, `ScanPredicate`, and related fields that the service no longer accepts. The script exits 1.
- Correction: run `mise run check:proto-drift` (or `WYRD_PROTO_SNAPSHOT=1 cargo build --locked -p wyrd-tonic --all-features`) and commit the regenerated `wyrd.v1.bin`. Re-run `mise run check:proto-drift` and get exit 0.

**STD-2: workspace-hack drift from the Cargo changes (blocking)**
- Rule: `check:workspace-hack` runs inside `mise run check`, which is a dependency of `gate`. AGENTS §12 says a red gate blocks.
- Location: `Cargo.toml` `[patch.crates-io]` arrow-rs git pins, the new `object_store` workspace dependency, and the `vala-bifrost-redux` `cloud` feature. `crates/shared/workspace-hack/Cargo.toml` was not regenerated.
- Consequence: the hakari feature union no longer matches the workspace, so artifact reuse across lanes degrades and `mise run check`/`gate` fail.
- Correction: run `cargo hakari generate` and `cargo hakari manage-deps`, then commit. `mise run check:workspace-hack` exits 0.

**STD-3: change-packet requirement IDs in permanent code**
- Rule: agent-rules "Never mention references to plans, tasks … Do not hardcode plan/task names, IDs". maintainer-style "Do not leave task IDs … in permanent code".
- Location: `Cargo.toml:230` "(spec REQ-032)"; `crates/vala/vala-bifrost-redux/src/parquet/object_uploader.rs:157` "(spec REQ-034)".
- Consequence: the REQ IDs point into `changes/active/bifrost-variant/spec.md`, which is deleted on completion. A future maintainer gets a dangling reference.
- Correction: replace each ID with the requirement stated in place, or a reference to `architecture/bifrost-design.md`. Then `git grep -nE 'REQ-0[0-9]{2}' -- Cargo.toml crates/` returns nothing for these lines.

**STD-4: fully qualified types in new signatures and fields**
- Rule: agent-rules "Bring types in with `use` and use bare names in signatures … struct fields, function parameters, return types".
- Location: `crates/vala/vala-bifrost-redux/src/bench_support.rs:293,319,351,429,443,461` (`datafusion::prelude::SessionContext`, `datafusion::error::Result<iceberg::table::Table>`, `crate::oracle::exec::{HotFileSource,OracleTableProvider}`); `src/oracle/bindings.rs:242` (`datafusion::prelude::SessionConfig`); `src/resources.rs:4074` (`datafusion::config::ConfigOptions`); `src/oracle/mod.rs:1143` field `resources: crate::resources::OracleResources`; `crates/wyrd/wyrd-storage/src/signer.rs` non-cloud `presign_single_put`/`init_multipart`/`presign_get` (`std::future::Ready<…>`); `tests/integration/forge/support.rs` `-> vala_bifrost_redux::forge::ForgeObjectPages`.
- Consequence: the module's dependency manifest is hidden in signatures, against the stated repository rule.
- Correction: add top-of-module `use` items (cfg-gated where the item is feature-gated) and use bare names. `rg -n '\) -> (datafusion|crate|std::future|vala_bifrost_redux)::' <files>` returns no added signature.

**STD-5: function-scoped imports in production source**
- Rule: agent-rules "All `use` statements live at the top of the module". The exceptions are `mod tests` and a single `Trait as _`.
- Location: `crates/vala/vala-bifrost-redux/src/bench_support.rs:352-355` (`DataFusionError`, `MemoryCatalogBuilder`, `MEMORY_CATALOG_WAREHOUSE`, `Transaction`, `NamespaceIdent`, `TableCreation`, and others inside `publish`) and `:462` (`FormatVersion`, `SortOrder`, `TableMetadataBuilder`, `UnboundPartitionSpec` inside `unpublished_table`). This module is compiled under `bench-support`, not `cfg(test)`.
- Consequence: the module's iceberg dependency surface is hidden inside function bodies.
- Correction: move these imports to the module's top `use` block. Afterwards `rg -n '^\s{5,}use ' crates/vala/vala-bifrost-redux/src/bench_support.rs` matches nothing.

**STD-6: panicking new helpers without `# Panics`**
- Rule: AGENTS §16 "Add `# Panics` whenever a panic remains possible". This covers test helpers. The diff's own new test functions follow it.
- Location (new items whose bodies `expect`/`assert`):
  - `src/forge/compact.rs:463` `seed_reversed`
  - `src/forge/orphan_gc.rs:2294` `orphan_matrix_aged`
  - `src/oracle/exec.rs:2951` `write_split_fixture`
  - `src/oracle/exec.rs:2982` `collect_int64`
  - `src/oracle/exec.rs:3091` `build_hot_batch_fixture`
  - `src/parquet/object_uploader.rs:407` `remote_bytes`
  - `crates/wyrd-spec/src/vala/assignment_authority.rs:276` `base_vector`
  - `crates/wyrd-spec/src/vala/assignment_authority.rs:289` `digest_of`
  - `benches/bifrost_reads/main.rs:38` `main`
  - `benches/bifrost_reads/main.rs:65` `text`
  - Also the undocumented local `const HOUR_MICROS` at `src/oracle/live.rs:976`.
- Consequence: the rustdoc rule is a hard blocker, and these docs omit the failure behavior.
- Correction: add a `# Panics` section to each helper naming its fixture invariant, and document `HOUR_MICROS`. Re-running the scan (`stdrev/errscan.py`) reports `NO_PANICS 0`.

**STD-7: free functions mutating an owner's state (struct-centered style)**
- Rule: AGENTS §5 "Public operations and internal orchestration that use an owner's state … MUST be inherent methods". rust-core "Which type owns the identity or invariant this behavior changes?"
- Location: `crates/vala/vala-bifrost-redux/src/oracle/exec.rs`:
  - `:433` `record_distributed_scan_metrics(plan, sink: Arc<RemoteScanMetrics>)` exists only to fill `RemoteScanMetrics`.
  - `:464` `fold_distributed_scan_metrics(node, &mut Option<u64>, &mut PruningEvidence)` threads two accumulator out-params through recursion.
  - `:105` `parquet_pruning(&MetricsSet) -> PruningEvidence` is a constructor of `PruningEvidence`, which already owns `add`.
- Consequence: the scan-evidence accumulation is split across free functions and out-params rather than discoverable on `RemoteScanMetrics` and `PruningEvidence`. This is the functional drift the rule rejects for new code.
- Correction:
  - `RemoteScanMetrics::record_distributed_plan(&self, plan)`;
  - an accumulator method such as `PruningEvidence::from_parquet_metrics(&MetricsSet)`;
  - a fold method on an owning accumulator (for example a `WorkerScanStats`/evidence struct holding `bytes_scanned`) that walks the plan;
  - existing tests keep passing (`mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(/oracle::/)'`).

**STD-8: `check:test-coverage` red (`gate` member; outside the changed surface)**
- Rule: AGENTS §12 "A red gate blocks the change regardless of who turned it red … fix or report as a blocker with the diagnosis".
- Diagnosis: `scripts/checks/test-coverage.sh` reports `wyrd-types` (`crates/shared/wyrd-types`) has no family lane in `scripts/test-families.sh`. This diff does not touch it.
- Correction: assign `wyrd-types` to its family in `scripts/test-families.sh` and get `mise run check:test-coverage` to exit 0. Otherwise carry it as an explicit blocker for whoever runs `gate`.

**STD-9: `check:rustls-provider` red (standalone check; outside the changed surface)**
- Rule: AGENTS §12, as for STD-8.
- Diagnosis: `cargo tree --workspace --all-features -e features,no-dev` enables `rustls`/`hyper-rustls` feature `ring` through `gcp_auth v0.12.7` ← `iceberg-catalog-rest` ← `iceberg-compaction-core` ← `vala-bifrost-redux`. `workspace-hack` also pins `ring` features. This diff does not change those dependencies, and the check is not in `gate` or CI workflows.
- Correction: owner decision required. Either disable `gcp_auth`'s `ring` path in the iceberg fork, or update the policy. Record it as a known red check if it stays out of scope.

**STD-10: benchmark task description contradicts its command (minor)**
- Rule: maintainer-style "Public names … and documentation must describe the same operation".
- Location: `mise.toml` `bench:bifrost:reads` says "Built like the shipped server: `dist` profile (fat LTO, one codegen unit)", but its `run` is `cargo bench …` without `--profile dist`. The bench profile inherits `release` (`codegen-units = 4`, no LTO).
- Consequence: anyone running the task gets timings that are not the production-build numbers the description promises.
- Correction: either add `--profile dist` to the command or correct the description. `mise tasks info bench:bifrost:reads` then matches the build actually used.

## 4. Checks run by this reviewer

All checks ran sequentially with `nice`.

| Check | Exit |
|---|---|
| `check:unwrap-audit` | 0 |
| `check:clippy-allow-audit` | 0 |
| `check:client-tier` | 0 |
| `check:pyo3-scope` | 0 |
| `check:sdk-pyo3-scope` | 0 |
| `check:registry-client-tier` | 0 |
| `check:cli-client-tier` | 0 |
| `check:object-store-pin` | 0 |
| `check:no-testing-in-prod-deps` | 0 |
| `check:from-pools-allowlist` | 0 |
| `check:tenant-isolation` | 0 |
| `check:no-tonic-outside-wyrd-tonic` | 0 |
| `check:mocks-scope` | 0 |
| `check:no-legacy-server-vocab` | 0 |
| `check:bifrost-resource-governance` | 0 |
| `codegen:check` | 0 |
| `py:format:check` | 0 |
| `py:lints` | 0 |
| `docs:check` | 0 |
| `check:proto-drift` | **1** (STD-1) |
| `check:workspace-hack` | **1** (STD-2) |
| `check:test-coverage` | **1** (STD-8) |
| `check:rustls-provider` | **1** (STD-9) |

Still not run: `mise run gate`. It would also fail on STD-2 and STD-8. `ts:typecheck` was not run separately; the TypeScript journey compiled and ran inside `test:bifrost`.

## 5. Verdict

**FAIL.**

- Blocking findings caused by this diff: STD-1 (stale `wyrd.v1.bin`) and STD-2 (workspace-hack drift).
- Standards violations in changed code: STD-3, STD-4, STD-5, STD-6, and STD-7. STD-6 is a hard blocker under AGENTS §16.
- Red checks outside the changed surface: STD-8 and STD-9, reported with their diagnoses as AGENTS §12 requires.
- Minor: STD-10.
