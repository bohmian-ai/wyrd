# Forge merge ledger

Every conflict, silent break, and follow-up fix resolved when the Forge rewrite
merged into the verified-change contract branch. When this branch merges with
`bifrost-variant`, the same decisions recur. Apply each replay rule as
written; deviate only with a recorded reason in this file.

## Topology

| Ref | Commit | Meaning |
|---|---|---|
| main base | `55db718bf` | merge base of Forge and verification |
| verification side | `b1b8f25f7` | first parent of the merge |
| Forge side | `2880705a0` | second parent of the merge (Forge tip) |
| merge | `ce866229d` | "Merge the Forge rewrite into the verified-change contract branch" |
| AUTO tree | `1ac66cb57` | `git merge-tree --write-tree b1b8f25f7 2880705a0`: 86 conflicted files; 28 more hand-edited without a conflict (silent breaks) |
| bifrost-variant fork | `5ab92b003` | variant forked from Forge here, before `32a9c5b64..2880705a0` |
| variant tip analysed | `509f972c6` | `wyrd/bifrost-variant/TASK-003` |

Ground rule from the merge: Forge, its leader, peer route, and tests come from
the rewrite unchanged apart from call sites of shared APIs this branch changed.
Verification, auth, OTLP, SHA-256 credentials, eager built-in tables, the audit
outbox, and the SDKs stay as on this branch; `WYRD_TENANT` is the only tenant
selection.

## How to replay

1. Merge, then walk the index below in order; each entry names files, what
   each side had, the resolution, and one replay rule.
2. Never hand-merge `Cargo.lock`, `workspace-hack`, generated `.pyi`, or
   napi `.d.ts`/`.d.cts`; regenerate them (L-entries say how).
3. After resolving, grep for each silent break listed below; they compile or
   pass `cargo check` while failing at runtime or in a later lane.
4. Finish with `mise run gate`.

## Index

| ID | Decision | Kind |
|---|---|---|
| L01 | Renumber the audit-progress migration and read the owed-tenant scan from `vala.audit_publication` | conflict + silent-break |
| L02 | Pin the compaction core to the ring-free revision and regenerate the lockfile and workspace-hack | conflict + silent-break + follow-up-fix |
| L03 | Fold vala-sql tests into one `tests/integration` binary, including the new Forge leader test | conflict + silent-break |
| L04 | Take Forge's active-table-read model over the reader-epoch and protection model | conflict |
| L05 | Keep Forge query logic in verification's pedantic-lint shape | conflict |
| L06 | Combine verification's merged match arm with Forge's `ScanLiteral::Bytes` | conflict |
| L07 | Keep verification's deletion of retired check scripts and Forge's owner rule in tenant isolation | conflict |
| L08 | Keep the checked-in proto descriptor deleted | conflict (modify/delete) |
| L09 | Keep typed `PyDim` docs and add Forge's Python help rustdoc | conflict |
| L10 | Document correlation columns without `CodeAxis` | conflict |
| L11 | Type-check both new Python test files in the stub lane | conflict |
| L12 | Keep audit publication continuous (no 5 s tick) | conflict |
| L13 | Remove the catalog-transaction audit from Bifrost table registration | conflict |
| L14 | Remove the standalone audit-denial path from verification control | conflict |
| L15 | Drop both the Oracle continuity monitor and the Oracle audit drain from shutdown | conflict |
| L16 | Use Forge's managed-column table without CodeAxis or batch id | conflict |
| L17 | Resolve the Forge config at boot with the hourly maintenance timer and Forge's reduced field set | conflict |
| L18 | Call Drift verify without the Verifier ref | conflict |
| L19 | Call `superuser_pool()` synchronously in Forge-side tests | silent-break |
| L20 | Box large futures that pedantic clippy flags | silent-break |
| L21 | Drop the tenant argument from `client_from_options` and expect an empty foreign result table | silent-break |
| L22 | Delete the reader-authority integration test | conflict |
| L23 | Put Forge's test content into the relocated `tests/integration/` binaries | conflict |
| L24 | Take the more precise wording in doc-only and literal conflicts | conflict |
| L25 | Keep one `capacity` benchmark and the Forge benchmark, and delete the other three | conflict |
| L26 | Measure the Forge backlog from `vala.forge_tasks`, not planning demands | silent-break |
| L27 | Have `LocalServer::start` take both the owner URL and the envelope | conflict |
| L28 | Merge release_server process plumbing: OperatorRun, role-aware readiness, joined peer ports | conflict |
| L29 | Have the release_server peer-port unit test expect the joined-replica range | follow-up-fix |
| L30 | Drop `has_demand` from `ForgeWorkflowInspection` and keep the synchronous `superuser_pool()` | conflict |
| L31 | Disable the audit publisher in single-table Forge and Oracle journeys | conflict |
| L32 | Remove the `audit` field from `CreateTableRequest` literals | silent-break |
| L33 | Call `client_from_options` with three arguments | conflict |
| L34 | Add `params: Vec::new()` to Forge-side `BifrostQueryRequest` literals | silent-break |
| L35 | Drop `.await` on `superuser_pool()` in Forge-side journeys | conflict |
| L36 | Keep the verification side's client-owned event-time fix in observation journeys | conflict |
| L37 | Merge production_closeout: Forge geometry assertion and held-reader constant, verification Duration style | conflict |
| L38 | Fix pedantic clippy findings in Forge-origin code now that wyrd-testing inherits workspace lints | silent-break |
| L39 | Drop the per-constructor `tenant` argument and select the tenant only through `WYRD_TENANT` | conflict |
| L40 | Keep the injected `Environment` on the client credential and Cards config path | conflict |
| L41 | Carry both the query `params`/typed terminal and the Forge `compaction_type` through Rust and Python Bifrost | conflict |
| L42 | Thread the TypeScript `compactionType` through the `TableConfigOptions` object and catalog-result native call | conflict (the native function was rewritten silently inside the conflicted file) |
| L43 | Keep the Python `wyrd.verification` module and the TypeScript `Verification` client deleted | conflict (modify/delete) |
| L44 | Remove WyrdTestServer hooks and the duplicate method the merge resurrected in the testing stub | follow-up-fix |
| L45 | Resolve Python stub docstring conflicts to the verification text and keep Forge's non-conflicting doc expansions | conflict |
| L46 | Regenerate the Python public `__init__.pyi` stubs and the napi `.d.ts`/`.d.cts` from source | conflict |
| L47 | Merge the gRPC transport test conflict: keep the verification wiremock exchange and take the Forge refresh tests | conflict + silent-break |
| L48 | Keep both Python compaction-type and query-param Bifrost journeys | conflict |
| L49 | Keep the relocated Rust observe-run journey and drop the tenant argument | conflict (rename/modify) |
| L50 | Take the verification Python CLI module wholesale | conflict |
| L51 | Settle audit staging before the progress row so a freeze never waits | follow-up-fix |
| L52 | Keep the tenant audit table out of journeys that count pod-wide or single-table work | follow-up-fix |
| L53 | Reconcile Forge-branch tests and stubs with the TASK-008/TASK-016 public surfaces | follow-up-fix |
| L54 | Wait for the Oracle envelope release as well as the graph drop before asserting revocation | follow-up-fix |
| L55 | Bring SDK journeys in line with the TASK-016 query, drift, and binding surfaces | follow-up-fix |
| L56 | Drop merge-hygiene leftovers | follow-up-fix |
| L57 | Publish audit continuously instead of on a fixed 5 s tick | forge-delta-missing-from-variant |
| L58 | Carry main's OIDC and workflow-runtime merge and its repairs | forge-delta-missing-from-variant |
| L59 | Select the tenant only through WYRD_TENANT | forge-delta-missing-from-variant |
| L60 | Bound snapshot-expiry authority by the lease and refuse cuts on an unknown expiry | forge-delta-missing-from-variant |
| L61 | Linearize Forge leader handlers with the term | forge-delta-missing-from-variant |
| L62 | Pin the compaction fork without the unconsumed planning surface (superseded) | forge-delta-missing-from-variant |
| L63 | Forge packet review records and authority docs | forge-delta-missing-from-variant |
| L64 | Wait for a routable Oracle at test-server start and keep audit publication out of the stage-pressure journey | follow-up-fix |

## Workspace, dependencies, SQL, and checks

### L01. Renumber the audit-progress migration and read the owed-tenant scan from `vala.audit_publication`
- **Kind:** conflict + silent-break
- **Files:** crates/vala/vala-sql/migrations/20261003000200_audit_publication_progress.sql, crates/vala/vala-sql/migrations/20261003000300_audit_publication_operator_read.sql, crates/vala/vala-sql/src/queries/audit_staging.rs
- **Verification side (b1b8f25f7):** `20261003000000_audit_publication_progress.sql` (0fa3d0d6b) creates `vala.audit_publication(data_tenant_id PK, published_seq, publishing_seq_hi, updated_at)` with RLS and a `wyrd_app` grant, backfills it from `vala.audit_chain_head`, then runs `ALTER TABLE vala.audit_chain_head DROP COLUMN publishing_seq_hi, DROP COLUMN published_seq`. `audit_staging.rs` imports `use uuid::Uuid;`.
- **Forge side (2880705a0):** `20261003000000_forge_leader_peer_uri.sql` and `20261003000100_drop_forge_planning_demands.sql` use the same version prefix. 2880705a0 ("Publish audit continuously instead of on a fixed 5s tick") adds `list_tenants_owing_publication(directory: &OperatorPool) -> Result<Vec<DataTenantId>, SqlError>`. It reads `WHERE head.last_seq > head.published_seq` on the operator pool, and its imports are `sqlx::types::Uuid` and `wyrd_spec::DataTenantId`.
- **Resolution:** AUTO kept two migrations with version `20261003000000`, which sqlx rejects as a duplicate. The verification migration is renamed byte-for-byte to `20261003000200_audit_publication_progress.sql` and runs after both Forge migrations. Two silent breaks are fixed. (1) The owed-tenant query is rewritten to `FROM vala.audit_chain_head AS head JOIN platform.tenants AS tenant USING (data_tenant_id) LEFT JOIN vala.audit_publication AS progress USING (data_tenant_id) WHERE head.last_seq > COALESCE(progress.published_seq, 0) AND tenant.status = 'active' AND tenant.deleted_at IS NULL ORDER BY head.data_tenant_id`, and its rustdoc now names the progress row. (2) A new migration `20261003000300_audit_publication_operator_read.sql` runs `GRANT SELECT ON vala.audit_publication TO wyrd_platform_admin;` because the scan runs on the operator pool. Imports become `use uuid::Uuid;` plus `use wyrd_spec::DataTenantId;`, and `sqlx::types::Uuid` is dropped. The query string uses `r"..."`. Forge's continuous `AuditPublisher` loop (250 ms idle, 5 s retry per failed tenant) is kept. That loop is the removal of the audit 5-second polling, and it came from the Forge branch, not from this branch. Its owner file `wyrd-server/src/audit/publication.rs` is outside this group.
- **Why:** Without the rewrite, the query reads a column the verification migration drops, so every publisher turn fails in Postgres while everything still compiles. Without the grant, `wyrd_platform_admin` cannot read the RLS-protected progress row. Forge's own test `only_active_tenants_with_unpublished_rows_owe_publication` (pg_audit_staging) now passes through the new join.
- **Replay rule:** Keep HEAD's migration numbering (000000/000100 Forge, 000200/000300 audit), any `audit_chain_head.published_seq` reference becomes `vala.audit_publication`, and bifrost-variant's only change here (making `entry_hash` `pub` with `#[must_use]`) goes onto HEAD's `audit_staging.rs` unchanged. Number any new bifrost-variant migration after `20261003000300`.
- **Commit:** ce866229d. Uncertain: a dev database that already applied the old `20261003000000_audit_publication_progress` version will see a version mismatch and needs a reset.

### L02. Pin the compaction core to the ring-free revision and regenerate the lockfile and workspace-hack
- **Kind:** conflict + silent-break + follow-up-fix
- **Files:** Cargo.toml, Cargo.lock, crates/shared/workspace-hack/Cargo.toml
- **Verification side (b1b8f25f7):** `iceberg-compaction-core` rev `170bf8e1164be84f8e27be70d088459097871f21` (515f16c97). iceberg-rust is unchanged from main at `330fe331…`. workspace-hack has no `ring` features: `rustls` features are `["logging","prefer-post-quantum","std","tls12"]`, and the target-specific `hyper-rustls` has `["aws-lc-rs","http1","http2","native-tokio","tls12"]`.
- **Forge side (2880705a0):** iceberg-rust rev `97c32f6377cbdfe51dbeb69e77a2a726669de93a` for `iceberg`, `iceberg-catalog-sql`, `iceberg-datafusion`, and `iceberg-storage-opendal`, which merged cleanly. `iceberg-compaction-core` rev `35f037e5413a3dfbffb996fbb8c882da79644804` (3a196c103, which deletes the unconsumed `NonCommittingCompaction` accessors). workspace-hack adds `ring` to `rustls`, `hyper-rustls`, and `rustls-webpki`.
- **Resolution:** In ce866229d the pin conflict was resolved to Forge's `35f037e…`, matching the iceberg-rust `97c32f6…` pin. `Cargo.lock` was re-resolved, which dropped the `wyrd-test-contract-macros` package and kept the verification deps. workspace-hack was regenerated with `cargo hakari generate`, which pulled in Forge's `ring` feature lines. Follow-up 161488d9c repinned to `rev = "4e9f325d993d900bcdec2fa74239c6a6173f49cd"` ("REST catalog dropped so Ring stays out of the graph") and regenerated `Cargo.lock` and workspace-hack. That removed every `ring` feature and `rustls-webpki`, and restored `hyper-rustls` to `["aws-lc-rs","http1","http2","native-tokio","tls12"]`. The other Cargo.toml changes merged automatically: the verification pedantic lint table, `[profile.dev] debug = "line-tables-only"`, and the removal of `wyrd-test-contract-macros`.
- **Why:** The workspace-hack must equal hakari's output, enforced by `check:workspace-hack` (`cargo hakari generate --diff`, `manage-deps --dry-run`, `verify`). AUTO's text merge was stale. Forge's compaction rev brings the REST catalog, and through it `ring`, back into the graph. The verification branch had kept `ring` out.
- **Replay rule:** Bifrost-variant pins iceberg-rust `c41cbd0…`, compaction `c04c45f…`, and Arrow/Parquet 60. Choose one compaction rev that builds against bifrost-variant's iceberg-rust with the REST catalog dropped (no `ring` in workspace-hack). Then run `cargo update -w`, `cargo hakari generate`, and `cargo hakari manage-deps`, and never hand-merge Cargo.lock or workspace-hack. Uncertain: I could not check whether `c04c45f` already drops the REST catalog, or whether a rev combining both exists.
- **Commit:** ce866229d, 161488d9c

### L03. Fold vala-sql tests into one `tests/integration` binary, including the new Forge leader test
- **Kind:** conflict + silent-break
- **Files:** crates/vala/vala-sql/Cargo.toml, crates/vala/vala-sql/tests/integration/main.rs, crates/vala/vala-sql/tests/integration/pg_forge_leader.rs, crates/vala/vala-sql/tests/integration/pg_forge_operations.rs (plus the auto-renamed pg_forge_tasks.rs, pg_forge_file_list.rs, pg_oracle_membership.rs)
- **Verification side (b1b8f25f7):** 515f16c97 moves every `tests/pg_*.rs` under `tests/integration/` with a `main.rs` of `mod pg_*;` lines, removes all `[[test]]` blocks, and adds `[lints] workspace = true`. `PgFixture::superuser_pool()` becomes sync: `pub fn superuser_pool(&self) -> Result<PgPool, FixtureError>`.
- **Forge side (2880705a0):** Adds `tests/pg_forge_leader.rs` (431eccbfc) and a `[[test]] name = "pg_forge_leader"` beside the existing `[[test]]` list, and calls `fixture.superuser_pool().await` everywhere.
- **Resolution:** Cargo.toml keeps only `[lints] workspace = true` with no `[[test]]` blocks. AUTO had left `tests/pg_forge_leader.rs` at the crate's test root. It is moved to `tests/integration/pg_forge_leader.rs` and `mod pg_forge_leader;` is added to `main.rs` in alphabetical order. Every `fixture.superuser_pool().await.expect(..)` in Forge test code becomes `fixture.superuser_pool().expect(..)`. One blank line is added in `pg_forge_operations.rs` after the nested `async fn` (rustfmt).
- **Why:** Left at the root, `pg_forge_leader.rs` would be a separate auto-discovered test binary. It would fail to compile because `.await` is applied to a sync function, and the `--test integration -E 'test(/^<module>::/)'` lanes would never select it.
- **Replay rule:** For any `crates/vala/vala-sql/tests/*.rs` file or `[[test]]` entry from bifrost-variant, move it under `tests/integration/`, add `mod <name>;` to `main.rs`, delete the `[[test]]`, and drop `.await` from `superuser_pool()`. Bifrost-variant touches none of these files today.
- **Commit:** ce866229d

### L04. Take Forge's active-table-read model over the reader-epoch and protection model
- **Kind:** conflict
- **Files:** crates/vala/vala-sql/src/queries/oracle_reader_authority.rs, crates/vala/vala-sql/tests/integration/pg_oracle_membership.rs, crates/wyrd/wyrd-tonic/src/private_conversion.rs, crates/vala/vala-sql/src/queries/file_list.rs
- **Verification side (b1b8f25f7):** Still carries the three-owner lock order (cluster_nodes, `oracle_reader_epochs`, maintenance authority) docs, and tests for `OracleReaderEpochs` and `OracleTableProtections`. These include `oracle_reader_authority_schema_contract_matches_migration`, `oracle_reader_authority_corruption_and_failed_narrowing_fail_closed`, `stale_heartbeat_and_replacement_startup_cannot_remove_reader_protection`, the `ReaderAuthority::superuser()` helper, `follower_reader_cut_round_trips_and_rejects_tampering` with `test_reader_cut()`, and `unresolved_hot_cut_projects_bounds_without_migration`. Its only own edits were lint-shape changes, sync `superuser_pool()`, and one doc phrase.
- **Forge side (2880705a0):** fa2ad497b, 3f69e10a5, and later commits replace reader protection with `OracleActiveTableReads`/`ActiveReadOwner`, document a single serialization row (`vala.bifrost_table_maintenance_authority`, `FOR SHARE`), remove `FollowerReaderCut` from the wire, and delete `HotFileCatalog::unresolved_for_cut`.
- **Resolution:** All four files take Forge's content. `oracle_reader_authority.rs` and `private_conversion.rs` are byte-identical to 2880705a0. `pg_oracle_membership.rs` is Forge's file with `superuser_pool()` made sync. The verification-only tests listed above are deleted. In `file_list.rs` the promotable-rows query uses Forge's `Vec<(Uuid, …)>` with `use uuid::Uuid;` and verification's `r"..."` raw string.
- **Why:** The APIs those tests exercised no longer exist on the Forge side.
- **Replay rule:** Bifrost-variant already contains fa2ad497b and 3f69e10a5. Take bifrost-variant's logic in `private_conversion.rs` (it changed that file heavily). Keep `FollowerReaderCut` and `unresolved_for_cut` absent. Reapply HEAD's pedantic shape, for example `Err(A { .. } | B { .. })` instead of `Err(A { .. }) | Err(B { .. })`.
- **Commit:** ce866229d

### L05. Keep Forge query logic in verification's pedantic-lint shape
- **Kind:** conflict
- **Files:** crates/vala/vala-sql/src/queries/forge_tasks.rs, crates/vala/vala-sql/src/queries/forge_operations.rs
- **Verification side (b1b8f25f7):** Clippy pedantic (515f16c97) gives `r"..."` instead of `r#"..."#`, `impl ForgeOperations<'_>`, and `ForgeTasks::enqueue` running directly on `self.operator_pool.pool()`.
- **Forge side (2880705a0):** `enqueue` opens a transaction (`let mut tx = self.operator_pool.pool().begin()`), runs `Self::admit_cleanup_handoff(&mut tx, task)` for `ForgeTaskStrategy::ExpiredCleanup`, inserts with `.fetch_one(&mut *tx)`, then calls `tx.commit()` and returns `Ok(task_id)`. It adds `async fn refuse_unsettled_promotion(&self, conn: &mut PgConnection) -> Result<(), SqlError>` in `impl<'resource> ForgeOperations<'resource>`. `cancel_superseded` docs drop "successor demand".
- **Resolution:** Forge's bodies and docs (transactional `enqueue`, `refuse_unsettled_promotion`, and the `cancel_superseded` text "Caller-owned rollback removes the cancellation" with "Never panics in practice…") are written with `r"..."` strings and the header `impl ForgeOperations<'_>`.
- **Why:** The behavior belongs to the Forge rewrite. `mise run lints` runs pedantic clippy with `-D warnings`, so Forge's formatting would fail it.
- **Replay rule:** Take bifrost-variant's Forge logic, then apply clippy pedantic fixes (`needless_raw_string_hashes`, `needless_lifetimes`, `match_same_arms`) until `mise run lints` is clean.
- **Commit:** ce866229d

### L06. Combine verification's merged match arm with Forge's `ScanLiteral::Bytes`
- **Kind:** conflict
- **Files:** crates/wyrd-spec/src/vala/assignment_authority.rs
- **Verification side (b1b8f25f7):** `push_literal` uses `ScanLiteral::I64(value) | ScanLiteral::TimestampMicros(value) => { buffer.extend_from_slice(&value.to_be_bytes()); }`. It also has `#[expect(clippy::unnecessary_wraps, reason = …)]` on `push_scribe_cut`, `hex::encode` fingerprints in tests, and `1.000_000_1_f64`.
- **Forge side (2880705a0):** Adds `ScanLiteral::Bytes(Vec<u8>)` (digest tag 6) and a separate `TimestampMicros` arm.
- **Resolution:** Keep the combined `I64 | TimestampMicros` arm and add `ScanLiteral::Bytes(value) => push_bytes(buffer, value, "bytes length")?`. The separate `TimestampMicros` arm is removed.
- **Why:** A duplicate arm trips `match_same_arms`. `Bytes` is the Forge contract.
- **Replay rule:** Take bifrost-variant's predicate model (`ScanLeaf`, `In`) wholesale, then reapply the combined `I64 | TimestampMicros` arm, the `#[expect(clippy::unnecessary_wraps)]`, and the test lint fixes.
- **Commit:** ce866229d

### L07. Keep verification's deletion of retired check scripts and Forge's owner rule in tenant isolation
- **Kind:** conflict
- **Files:** scripts/checks/from-pools-allowlist.sh, scripts/checks/mocks-scope.sh, scripts/check_unwrap_audit.py, scripts/check_tenant_isolation.py
- **Verification side (b1b8f25f7):** 515f16c97 cuts the repository checks from 42 to 9 and deletes the first three scripts. Its `check_vala_query_modules` operator branch is `if has_public_async_fn(code) and not has_platform_executor(code)`.
- **Forge side (2880705a0):** Edits the deleted scripts (for example 30d31ebac allowlists wiremock seams). Its operator branch accepts methods of a struct owning `OperatorPool` (`owns_operator_pool` regex `struct\s+\w+\s*\{[^}]*\bOperatorPool\b`, `&self`) and checks per function through `public_async_fns(code)`.
- **Resolution:** The three scripts stay deleted (nothing in the merged tree references them). `check_tenant_isolation.py` takes Forge's per-function block with the message "operator public async fn {fn_name} must take PgPool or OperatorPool, or be a method of an OperatorPool-owning struct".
- **Why:** Those checks were retired. `ForgeLeaderElection` in `forge_leader.rs` (on `VALA_OPERATOR_ALLOWLIST`) holds its `OperatorPool` as a field, so the old file-level rule would fail it.
- **Replay rule:** Resolve every modify/delete on a script HEAD deleted as delete. Bifrost-variant edits `scripts/checks/object-store-pin.sh` (0.13 to 0.14), which HEAD has also deleted, so expect the same conflict there. Keep HEAD's `check_tenant_isolation.py`.
- **Commit:** ce866229d

### L08. Keep the checked-in proto descriptor deleted
- **Kind:** conflict (modify/delete)
- **Files:** crates/wyrd/wyrd-tonic/proto/wyrd.v1.bin
- **Verification side (b1b8f25f7):** 515f16c97 deletes the file and strips `file_descriptor_set_path`/`WYRD_PROTO_SNAPSHOT` from `build.rs`. The `check:proto-drift` gate is gone.
- **Forge side (2880705a0):** Regenerated the binary after its proto changes.
- **Resolution:** File deleted. `wyrd.v1.proto` merged as text and `build.rs` compiles from it.
- **Why:** Nothing consumes the snapshot any more. The only descriptor registration is `tonic_health::pb::FILE_DESCRIPTOR_SET`.
- **Replay rule:** Bifrost-variant modifies `wyrd.v1.bin`; resolve it as delete and merge only `wyrd.v1.proto`.
- **Commit:** ce866229d

### L09. Keep typed `PyDim` docs and add Forge's Python help rustdoc
- **Kind:** conflict
- **Files:** crates/wyrd/wyrd-interfaces/src/data/schema.rs
- **Verification side (b1b8f25f7):** `shape`/`dims` return `Vec<PyDim>` (1d42a1e1b) with docs "Return the field shape as typed dimensions." and "…; alias of `shape`."
- **Forge side (2880705a0):** 6d74cf8a3 adds rustdoc to every `PyFieldSpec` and `PyDataSchema` method. Its `shape`/`dims` docs still describe JSON dicts with `# Errors`.
- **Resolution:** The verification docs stay on `shape`/`dims`. All of Forge's other docs are kept: `__new__` with `WYRD_DATA_400_VALIDATION`, `name`, `dtype`, `nullable`, `extra`, `to_dict`, `columns`, `fields`, `is_empty`, `contains_column`, `column`, and `column_names`.
- **Why:** The getters no longer serialize JSON and cannot fail.
- **Replay rule:** Keep HEAD's version of this file.
- **Commit:** ce866229d

### L10. Document correlation columns without `CodeAxis`
- **Kind:** conflict
- **Files:** docs/src/content/docs/bifrost/architecture.svx
- **Verification side (b1b8f25f7):** Removed `CorrelationPolicy::CodeAxis` from code and docs. `principal_id` is listed as `Utf8 (nullable)`.
- **Forge side (2880705a0):** Lists `card_uid`, `principal_id` (non-null), and `wyrd_request_id` (non-null) under "`Observation` or `CodeAxis`".
- **Resolution:** `card_uid | Utf8 (nullable) | Observation`, `principal_id | Utf8 | Observation`, `wyrd_request_id | Utf8 | Observation`.
- **Why:** `CodeAxis` has no references in the merged code. Forge's non-null types and `wyrd_request_id` match the managed columns.
- **Replay rule:** Never write `CodeAxis` back into docs or code.
- **Commit:** ce866229d

### L11. Type-check both new Python test files in the stub lane
- **Kind:** conflict
- **Files:** mise.toml
- **Verification side (b1b8f25f7):** The `ty check` stub lane adds `tests/unit/cards/test_registered_card_typing.py`.
- **Forge side (2880705a0):** The same lane adds `tests/unit/runtime/test_public_api_parity.py`.
- **Resolution:** The `run` line lists both files, `test_registered_card_typing.py` before `test_public_api_parity.py`. Every other mise lane merged automatically and already uses `--test integration -E 'test(/^module::/)'`.
- **Why:** Both files exist and both type checks hold.
- **Replay rule:** Keep HEAD's lanes, and rewrite any bifrost-variant lane path to the TASK-017 story file names (for example `query-bifrost.test.ts`, not `oracle-query.test.ts`). Its new TypeScript journeys (`variant-tables`, `every-iceberg-column-type`, `register-a-table-from-a-model`, `three-timestamp-types`) go into HEAD's lane.
- **Commit:** ce866229d

## Server, Bifrost engine, and MCP

### L12. Keep audit publication continuous (no 5 s tick)
- **Kind:** conflict
- **Files:** crates/wyrd/wyrd-server/src/audit/publication.rs
- **Verification side (b1b8f25f7):** Publisher loop on `tokio::time::interval(PUBLICATION_INTERVAL)` with `PUBLICATION_INTERVAL = 5s`. Tenants come from `list_active_tenant_ids`. Docs say publication progress lives on a separate progress row, not the chain head (0fa3d0d6b).
- **Forge side (2880705a0):** "Publish audit continuously instead of on a fixed 5s tick". Adds `PUBLICATION_IDLE = 250ms`, `PUBLICATION_RETRY = 5s`, `list_tenants_owing_publication`, `TenantCycles::{ready, finish_one, retry_at}`, `publish_logged -> bool`, and tests `a_failed_cycle_waits_out_its_retry` and `an_empty_set_never_finishes_a_cycle`. Its docs still describe the "chain head".
- **Resolution:** Kept all of Forge's continuous-publication code: `while !shutdown.is_cancelled() { sweep; select { finish_one | sleep(PUBLICATION_IDLE) } }`, a per-tenant 5 s retry backoff only after a failed cycle, and `vala_sql::queries::audit_staging::list_tenants_owing_publication`. Verification's wording replaced "chain head" in both doc conflicts: the `PUBLICATION_RETRY` doc says "a progress row held by a concurrent publisher", and the sweep doc says "held progress row or staging row delays its own tenant and nobody else".
- **Why:** The fixed 5 s tick is gone. Audit publication now runs continuously and backs off only per failing tenant. Verification's audit model keeps progress on a per-tenant progress row, so chain-head wording would be wrong.
- **Replay rule:** Keep HEAD's continuous publisher (`PUBLICATION_IDLE`/`PUBLICATION_RETRY`/`list_tenants_owing_publication`, no `PUBLICATION_INTERVAL`), because bifrost-variant 509f972c6 still has the 5 s `PUBLICATION_INTERVAL`. Discard any bifrost-variant hunk that brings the interval back.
- **Commit:** ce866229d

### L13. Remove the catalog-transaction audit from Bifrost table registration
- **Kind:** conflict
- **Files:** crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs, crates/wyrd/wyrd-server/src/bifrost/service.rs
- **Verification side (b1b8f25f7):** `register_dataset(tenant, table, user_fields, physical_layout, compaction_target_file_size_bytes: Option<u64>)`. There is no `audit` parameter and no `CreateTableRequest.audit`. `register_table` calls `audit::authorize(...)?` up front (outbox, non-blocking) and checks fingerprint, then layout, then compaction target.
- **Forge side (2880705a0):** `register_dataset(..., compaction: CompactionRegistration, audit: Option<AuditEvent>)`, `CreateTableRequest.audit`, and `append_registration_audit(&mut conn, request.audit.as_ref(), &fqn)` committed inside the catalog transaction, which maps failure to `BifrostCatalogError::AuditUnavailable`. `register_table` builds `allowed` plus a `record_allowed().await?` closure and passes `Some(allowed.clone())`. It adds a `CompactionTypeMismatch` check.
- **Resolution:** Signature is `register_dataset(&self, tenant, table, user_fields, physical_layout: Option<PhysicalLayoutWire>, compaction: CompactionRegistration) -> Result<TableUid, _>`. The merge removed `audit`, `AuditEvent`, `TenantConn` imports, `append_registration_audit`, and `CreateTableRequest.audit`. The concurrent-winner branch keeps `compaction.assert_matches(physical.metadata(), &fqn)?` and drops the audit append. `register_table` keeps verification's structure (`audit::authorize` first, fingerprint mismatch returns early, then layout, then the target check), and Forge's check `if let Some(kind) = body.compaction_type && existing.compaction_type != Some(kind) { CompactionTypeMismatch }` was added after the target check. The create call passes `CompactionRegistration { target_file_size_bytes: body.compaction_target_file_size_bytes, compaction_type: body.compaction_type.map(Into::into) }`. The `ReservedColumn` error doc stays. Error docs drop "audit" ("metadata, Iceberg, or SQL errors"). The `ensure_builtin` doc keeps verification's eager wording ("tenant provisioning and server boot are its only production callers"), not Forge's "Built-ins are lazy". Test call sites use `CompactionRegistration::default()` with no trailing `None`.
- **Why:** AGENTS.md: audit is non-blocking and has one write path through the process audit outbox. No error may report an audit write failure, and Forge's in-transaction append plus `AuditUnavailable` violates that. Built-ins are eager on this branch.
- **Replay rule:** Keep HEAD's 5-argument `register_dataset(..., CompactionRegistration)`, with no audit parameter, no `append_registration_audit`, and no `record_allowed`. Delete the trailing `None` audit argument (and `audit: None` fields) from every bifrost-variant call site.
- **Commit:** ce866229d

### L14. Remove the standalone audit-denial path from verification control
- **Kind:** conflict
- **Files:** crates/wyrd/wyrd-server/src/components/verification/service.rs
- **Verification side (b1b8f25f7):** No `deny_scope` helper. Card-scope denials go through the audit outbox.
- **Forge side (2880705a0):** `async fn deny_scope<T>(&self, caller, operation, resource)` builds `audit::audit_event(..., AuditOutcome::Denied)`, awaits `audit::record_audit(...)?` (can return `WyrdError::AuditUnavailable`), then returns `Self::scope_denied(resource)`.
- **Resolution:** Took verification's file wholesale (merged equals b1b8f25f7 for this file). `deny_scope` is gone.
- **Why:** Same rule as above: audits never block or fail an operation, and there is no `AuditUnavailable` refusal.
- **Replay rule:** Take HEAD's verification/service.rs and never reintroduce `deny_scope` or any awaited `record_audit` call on a request path.
- **Commit:** ce866229d

### L15. Drop both the Oracle continuity monitor and the Oracle audit drain from shutdown
- **Kind:** conflict
- **Files:** crates/wyrd/wyrd-server/src/state.rs, crates/vala/vala-bifrost-redux/src/oracle/analytical.rs, crates/vala/vala-bifrost-redux/src/oracle/mod.rs
- **Verification side (b1b8f25f7):** `Oracle::shutdown` awaits `await_role_task(&self.continuity_monitor, deadline, "oracle continuity monitor")` (reader-pin era). The Oracle owns no audit. oracle/mod.rs imports `vala_sql::audit_outbox::AuditOutbox`. analytical.rs imports `peer::{AuthorizedStage, OracleStageAuthority, StageOperationV1}`, with no `PeerSecurityError::AuditUnavailable` mapping.
- **Forge side (2880705a0):** No continuity monitor (fa2ad497b, "Replace reader protection with active table reads"). `let audit = self.audit.shutdown(deadline).await;` plus `|| audit != 0` in the retained-state check, and `OracleBuildInputs.audit`. oracle/mod.rs imports `oracle_reader_authority::ActiveReadOwner`. analytical.rs imports `PeerSecurityError` and `query_stream::RunningQueryTerminalOwner`, and maps `PeerSecurityError::AuditUnavailable => BifrostError::QueryAuditUnavailable`.
- **Resolution:** Shutdown now does `engine.shutdown`, the heartbeat `await_role_task`, and the snapshot-poller `await_role_task`. The retained check is `active_queries|queued_queries|peer_running|reserved_memory_bytes != 0`, with the message "Oracle shutdown retained admission, resource, or peer state". There is no `continuity_monitor` and no `audit` in the shutdown or in `OracleBuildInputs`. oracle/mod.rs imports both `AuditOutbox` and `ActiveReadOwner`. analytical.rs imports `peer::{AuthorizedStage, OracleStageAuthority, StageOperationV1}` plus `query_stream::RunningQueryTerminalOwner`, without `PeerSecurityError` at module scope (it remains only inside tests) and without the `AuditUnavailable` arm. The `Oracle::new` `# Panics` doc takes Forge's two-line wording.
- **Why:** Forge replaced reader pins with active table reads, so the continuity monitor no longer exists. Verification moved Oracle audit to the process outbox, so the Oracle has no audit drain and no audit-unavailable error.
- **Replay rule:** Keep HEAD's Oracle shutdown, `OracleBuildInputs` (no `audit`), and imports. Drop any bifrost-variant `self.audit.shutdown`, `audit != 0`, `PeerSecurityError::AuditUnavailable`, or `QueryAuditUnavailable` mapping.
- **Commit:** ce866229d

### L16. Use Forge's managed-column table without CodeAxis or batch id
- **Kind:** conflict
- **Files:** crates/vala/vala-bifrost-redux/src/tables/managed_columns.rs, crates/vala/vala-bifrost-redux/src/tables/mod.rs
- **Verification side (b1b8f25f7):** `CorrelationPolicy { Observation, None }`. CodeAxis and `agent_traces` were removed by 1090580f2 ("Provision every Bifrost built-in eagerly, drop lazy creation and the unused agent_traces table"). `ensure_managed_columns` pushes fields by hand, including `WYRD_BATCH_ID: FixedSizeBinary(16)`. Also has `CorrelationPolicy::appended_correlation_columns()` and the test `correlation_policies_are_explicit`.
- **Forge side (2880705a0):** Static `MANAGED_COLUMNS` table with `ManagedColumn::{identity,time}`, `ManagedScope`, `CorrelationPolicy::{appends, managed_columns}`, and `ensure_managed_columns` = `user_fields.extend(policy.managed_columns().map(ManagedColumn::untagged_arrow))`. The per-row batch id is dropped (37f596736, "Stamp wyrd_ingested_at from PostgreSQL and drop the per-row batch id"). Still has `CodeAxis` with `(Self::CodeAxis, _) => column.field.name != RUN_ID`.
- **Resolution:** `MANAGED_COLUMNS = [RUN_ID, CARD_UID, PRINCIPAL_ID (Correlation), WYRD_REQUEST_ID (Request), WYRD_EVENT_TIME, WYRD_INGESTED_AT (Time)]` with no `WYRD_BATCH_ID`. `CorrelationPolicy::appends` matches only `(_, Time) | (Observation, _) => true, (None, _) => false`. The `CodeAxis` variant, its arm, its doc, and the test `code_axis_policy_omits_run_id` were removed. `appended_correlation_columns` and `correlation_policies_are_explicit` were removed. Kept tests: `none_policy_appends_only_the_time_columns` and Forge's sensitivity-tag test. A HEAD grep finds no `CodeAxis` anywhere under crates.
- **Why:** Forge's table-driven managed columns and the dropped batch id are the newer storage contract. Verification removed the only CodeAxis user (`agent_traces`).
- **Replay rule:** Keep HEAD's `MANAGED_COLUMNS`/`appends` with no CodeAxis and no `WYRD_BATCH_ID`. Drop any CodeAxis variant, arm, or test and any `agent_traces` table that bifrost-variant (5 hits at 509f972c6) brings back.
- **Commit:** ce866229d

### L17. Resolve the Forge config at boot with the hourly maintenance timer and Forge's reduced field set
- **Kind:** conflict
- **Files:** crates/wyrd/wyrd-server/src/boot/mod.rs
- **Verification side (b1b8f25f7):** `DEFAULT_MAINTENANCE_INTERVAL = Duration::from_mins(1)`. `resolve_forge_config` sets `snapshot_retention`, `retain_last`, `maintenance_trigger_snapshot_count`, and `maintenance_trigger_interval`, written in the `map_or(base, from_secs)` style required by pedantic clippy. The `compose_bifrost` errors doc mentions "built-in table reconciliation", with a `# Panics` for the compaction owner. Event-time windows use `map_or_else(|| Duration::from_hours(720) / from_hours(24), from_secs)`.
- **Forge side (2880705a0):** `DEFAULT_MAINTENANCE_INTERVAL = Duration::from_hours(1)` (37a526d61, "Run Forge maintenance on the leader's hourly timer and drop the durable planner"), with docs, plus `DEFAULT_HINT_CAPACITY` docs. `ForgeConfig` keeps only `orphan_gc_*` and `default_target_file_size_bytes`, written in `.map(..).unwrap_or(..)` style. Event-time windows use `.map(..).unwrap_or_else(|| Duration::from_secs(30*24*60*60))`.
- **Resolution:** `const DEFAULT_MAINTENANCE_INTERVAL: Duration = Duration::from_hours(1);` keeps Forge's docs. `resolve_forge_config` uses only Forge's fields (`orphan_gc_ttl`, `orphan_gc_max_list_pages`, `orphan_gc_run_budget`, `default_target_file_size_bytes`, `maintenance_interval_secs`), written in verification's `map_or(default, Duration::from_secs)` style. The `ForgeRuntimeConfig` keys `snapshot_retention_secs`, `retain_last`, `maintenance_trigger_snapshot_count`, and `maintenance_trigger_interval_secs` are gone. The test asserts `orphan_gc_ttl == Duration::from_hours(1)`. Event-time windows keep verification's `map_or_else(|| Duration::from_hours(720)/from_hours(24), Duration::from_secs)`. The `compose_bifrost` doc keeps verification's "built-in table reconciliation" error clause and Forge's `# Panics` text. Verification's eager `BuiltinTables::new(..).reconcile(&operator_pool)` before role activation is kept.
- **Why:** Forge maintenance runs on the leader's hourly timer, so the 1-minute loop and the durable planner are gone. Workspace pedantic clippy (`map_unwrap_or`) requires the `map_or` form. Built-ins are eager on this branch.
- **Replay rule:** Keep HEAD's `resolve_forge_config` and `DEFAULT_MAINTENANCE_INTERVAL = from_hours(1)`, written in `map_or` style. bifrost-variant (fork 5ab92b003) already has `from_hours(1)`, so a conflict here should be style-only: take HEAD. Note that the "Forge 5 s loop" removal is not in this file (outside this scope).
- **Commit:** ce866229d

### L18. Call Drift verify without the Verifier ref
- **Kind:** conflict
- **Files:** crates/wyrd/wyrd-server/src/verification/runner.rs
- **Verification side (b1b8f25f7):** `Box::pin(self.drift.verify(tenant, run, spec, telemetry)).await`. 8b7218a92 ("Write Verifier results through the capture writer and remove SYSTEM tokens") dropped `verifier: &CardRef`.
- **Forge side (2880705a0):** `Box::pin(self.drift.verify(tenant, verifier, run, spec, telemetry)).await` (main's signature).
- **Resolution:** Kept verification's 4-argument call. The runner otherwise matches verification.
- **Why:** On this branch `DriftEngine::verify(&self, tenant, run, spec, telemetry)` takes no Verifier ref.
- **Replay rule:** Keep HEAD's `drift.verify(tenant, run, spec, telemetry)` and drop the `verifier` argument from any bifrost-variant call.
- **Commit:** ce866229d

### L19. Call `superuser_pool()` synchronously in Forge-side tests
- **Kind:** silent-break
- **Files:** crates/vala/vala-bifrost-redux/src/scribe/staging_runtime.rs, crates/vala/vala-bifrost-redux/tests/integration/forge/support.rs, crates/wyrd/wyrd-server/tests/integration/pg_router_smoke.rs, crates/wyrd/wyrd-server/src/bifrost/service.rs (tests)
- **Verification side (b1b8f25f7):** `wyrd_dev_fixtures::pg::PgFixture::superuser_pool(&self) -> Result<PgPool, FixtureError>` is sync (515f16c97). The test helper `caller_with(...) -> Caller` in bifrost/service.rs is sync.
- **Forge side (2880705a0):** `pub async fn superuser_pool`. Forge code calls `.superuser_pool().await` (`staging_runtime.rs` pg_tests, `PromotionIntegrationFixture` in forge/support.rs) and `caller_with([...]).await`.
- **Resolution:** Removed `.await` at every Forge-introduced call: `database.superuser_pool().expect("superuser pool")`, `.superuser_pool().expect("fixture superuser pool")`, `caller_with([...]);`. These were auto-merged hunks that no longer compiled.
- **Why:** The fixture API became sync on the verification side. Auto-merge kept Forge's new call sites unchanged.
- **Replay rule:** After merging, run `git grep -n 'superuser_pool().await\|\.superuser_pool()\s*$' -A1` and the `caller_with(...).await` pattern, then delete each `.await`. bifrost-variant still has async callers, for example in scribe/persistence.rs and scribe/shards.rs.
- **Commit:** ce866229d

### L20. Box large futures that pedantic clippy flags
- **Kind:** silent-break
- **Files:** crates/wyrd/wyrd-server/src/oracle/forwarding.rs, crates/wyrd/wyrd-server/src/state.rs, crates/wyrd/wyrd-server/tests/integration/pg_router_smoke.rs, crates/vala/vala-bifrost-redux/src/oracle/analytical.rs (context)
- **Verification side (b1b8f25f7):** Workspace `pedantic = warn`. Futures are already boxed where they were flagged.
- **Forge side (2880705a0):** `self.forward(context, request).await` in `ReadyOracleForwarder::dispatch`. `accept_with_deadline_for_test(...).await` in `Bifrost`. `server_with_forge_observer().await` in router smoke. The forward path grew with Forge's `planner.request_deadline(request.deadline_ms)`.
- **Resolution:** `Box::pin(self.forward(context, request)).await`, `Box::pin(self.query_forwarder.as_ref().ok_or(OracleRoleUnavailable)?.accept_with_deadline_for_test(..)).await`, and `Box::pin(server_with_forge_observer()).await` at every router-smoke call. Router smoke also changes `match joined { Ok(..) => .., Err(_) => {..} }` to `if let Ok(outcome) = joined { .. } else { .. }` (pedantic `single_match_else`). forwarding.rs dropped the now-unused `Duration` import from the module scope, and the tests module imports it.
- **Why:** These are clippy hand edits (most likely `large_futures` / `single_match_else` under pedantic). The merge message does not name the lint, so treat the specific lint as unconfirmed.
- **Replay rule:** After merging, run `mise run lints` and wrap each flagged future in `Box::pin(...)` at the call site, as HEAD does, instead of adding `#[allow]`.
- **Commit:** ce866229d

### L21. Drop the tenant argument from `client_from_options` and expect an empty foreign result table
- **Kind:** silent-break
- **Files:** crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs
- **Verification side (b1b8f25f7):** `client_from_options(server_url, credential, grpc_url, tenant: Option<&str>)` is called with a trailing `None`. The foreign tenant's `bifrost.query` expects an empty `[]` because built-ins are provisioned eagerly.
- **Forge side (2880705a0):** 1ca7a4316 ("Select the tenant through WYRD_TENANT, not SDK constructors") makes it `client_from_options(server_url, credential, grpc_url)`. The foreign query expects `WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND` (lazy built-ins).
- **Resolution:** Verification's test body with the trailing `None` removed (3-argument call). The doc and assertion keep verification's eager form: "reads its own provisioned, empty result table" and `assert_eq!(foreign_rows, json!([]))`.
- **Why:** Tenant selection is through `WYRD_TENANT` only, and built-ins are eager on this branch.
- **Replay rule:** Use the 3-argument `client_from_options`, and keep HEAD's empty-table assertion over any `TABLE_NOT_FOUND` expectation for built-in tables.
- **Commit:** ce866229d

### L22. Delete the reader-authority integration test
- **Kind:** conflict
- **Files:** crates/vala/vala-bifrost-redux/tests/integration/oracle/reader_authority.rs, crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs (test `protected_cut_reuses_immutable_metadata`)
- **Verification side (b1b8f25f7):** Modified the file only to make `superuser_pool()` calls sync. Kept the catalog test `protected_cut_reuses_immutable_metadata` (`prepare_reader_identity`, `TEST_METADATA_POINTER_READS`).
- **Forge side (2880705a0):** Deleted the file in fa2ad497b, and the reader-pin catalog API went with it.
- **Resolution:** The file is deleted (modify/delete resolved as delete). The catalog test `protected_cut_reuses_immutable_metadata` was removed. The production-pin test call sites take `CompactionRegistration::default()` with no trailing `None`.
- **Why:** Reader protection (epochs, `oracle_table_protections`, `prepare_reader_identity`) was replaced by active table reads, so these tests cover removed behavior.
- **Replay rule:** Keep reader_authority.rs deleted and drop any reader-pin/epoch test that bifrost-variant edits.
- **Commit:** ce866229d

### L23. Put Forge's test content into the relocated `tests/integration/` binaries
- **Kind:** conflict
- **Files:** crates/wyrd/wyrd-server/tests/integration/pg_router_smoke.rs, crates/wyrd/wyrd-server/tests/integration/pg_grpc_ingest_smoke.rs, crates/vala/vala-bifrost-redux/tests/integration/forge/production_routes.rs
- **Verification side (b1b8f25f7):** 515f16c97 moved `crates/wyrd/wyrd-server/tests/*.rs` to `tests/integration/*.rs` (one test binary). Router smoke still had the Oracle epoch tests (`oracle_epoch_cutoff_removes_readiness_...`, `supervisor_first_loss_...`, `blocked_renewal_...`, `retirement_first_loss_...`) and durable-planner coordinator tests (`coordinator_partial_pass_is_not_ready`, `coordinator_task_insert_failure_clears_readiness`, `coordinator_preseeded_demand_requires_roster_discovery`). production_routes had only `.await` removals.
- **Forge side (2880705a0):** Files at the old `tests/pg_*.rs` paths. Router smoke drops the epoch/planner tests and adds `drive_maintenance_pass`, `drive_forge_pass`, `coordinator_object_store_failure_keeps_readiness`, and `coordinator_promotion_debt_read_failure_clears_readiness`. production_routes deletes `scheduler_*` tests.
- **Resolution:** Files stay at the `tests/integration/` paths. Router smoke content is Forge's, with verification API adaptations: no `audit: None` in `CreateTableRequest`, sync `superuser_pool`, `Box::pin(server_with_forge_observer())`, and `if let` instead of `match`. grpc ingest smoke is verification's file, with only Forge's `register_dataset` calls changed to `CompactionRegistration::default()` plus the import `catalog::{CompactionRegistration, TableRef}` and a `# Panics` doc. production_routes equals Forge (2880705a0) exactly.
- **Why:** Forge owns Forge, leader, and peer tests unchanged apart from call sites of shared APIs this branch changed (merge message). The verification branch owns the test-binary layout and the result-table and SYSTEM-attribution tests.
- **Replay rule:** Apply bifrost-variant edits to `tests/pg_*.rs` onto the `tests/integration/` paths, keep HEAD's Forge test set, and adapt only shared-API call sites.
- **Commit:** ce866229d

### L24. Take the more precise wording in doc-only and literal conflicts
- **Kind:** conflict
- **Files:** crates/wyrd/wyrd-server/src/app/server.rs, crates/wyrd/wyrd-server/src/state.rs
- **Verification side (b1b8f25f7):** One-line `# Panics` docs on `BoundServer::run` and `Oracle::new`. `LimitsConfig.timeout: std::time::Duration::from_secs(1)` in the state tests.
- **Forge side (2880705a0):** Two-line invariant `# Panics` docs, and `timeout: Duration::from_millis(1000)`.
- **Resolution:** Forge's `# Panics` text for `BoundServer::run` ("Binding sets both together, so this is an invariant violation...") and for `Oracle::new`. Verification's `std::time::Duration::from_secs(1)` literal.
- **Why:** No behavior difference. The longer docs name the invariant, and the literal follows the pedantic-clean form.
- **Replay rule:** Keep HEAD's text, whichever side it came from.
- **Commit:** ce866229d

## Test harness, capacity tools, and journeys

### L25. Keep one `capacity` benchmark and the Forge benchmark, and delete the other three
- **Kind:** conflict
- **Files:** crates/wyrd/wyrd-testing/Cargo.toml; crates/wyrd/wyrd-testing/src/bin/verification_capacity/{main,evidence,step}.rs (and the rest of that directory); crates/wyrd/wyrd-testing/src/bin/bifrost_ingest_capacity/main.rs (and the rest of that directory); crates/wyrd/wyrd-testing/src/bin/bifrost_query_capacity/{main,run}.rs (and the rest of that directory)
- **Verification side (b1b8f25f7):** Deleted `verification_capacity`, `bifrost_ingest_capacity` and `bifrost_query_capacity` and replaced them with one `[[bin]] name = "capacity"` (`src/bin/capacity/main.rs`). Added `[lints] workspace = true` to the crate.
- **Forge side (2880705a0):** Kept and edited `bifrost_query_capacity`, `verification_capacity` and `bifrost_ingest_capacity`, and added `bifrost_forge_capacity`. Had no `[lints]` table.
- **Resolution:** The three modify/delete binaries stay deleted. Cargo.toml keeps `[[bin]] capacity`, then `[[bin]] name = "bifrost_forge_capacity"`, `path = "src/bin/bifrost_forge_capacity/main.rs"`, `bench = false` with its `mise run bench:bifrost:forge-capacity` comment, then `[lints] workspace = true` last. `num-traits = { workspace = true }`, which is on the verification side only, stays.
- **Why:** dec4213fa ("Replace both capacity benchmarks with one bench:capacity", REQ-171 rev 57) deletes bench:verification:capacity, bench:bifrost:ingest-capacity and their binaries. The Forge benchmark (55ee10855) is new work from the rewrite.
- **Replay rule:** Delete `src/bin/{verification_capacity,bifrost_ingest_capacity,bifrost_query_capacity}` and their `[[bin]]` entries, keep `capacity` and `bifrost_forge_capacity`, and keep `[lints] workspace = true` as the last table. bifrost-variant 509f972c6 still declares all four old binaries.
- **Commit:** ce866229d

### L26. Measure the Forge backlog from `vala.forge_tasks`, not planning demands
- **Kind:** silent-break
- **Files:** crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs, crates/wyrd/wyrd-testing/src/bin/capacity/report.rs, crates/wyrd/wyrd-testing/src/bin/capacity/step.rs
- **Verification side (b1b8f25f7):** `Queue` counted the Forge backlog as `SELECT COUNT(*) FROM vala.forge_planning_demands WHERE first_requested_at <= $2`. The docs said "Forge demand".
- **Forge side (2880705a0):** The rewrite removed the planning-demand table, so Forge work only exists in `vala.forge_tasks`.
- **Resolution:** The query becomes `(SELECT COUNT(*) FROM vala.forge_tasks WHERE created_at <= $2 AND state NOT IN ('succeeded','failed','cancelled'))`. The `Backlog::forge` doc becomes "Forge tasks created before load stopped and not yet terminal". Every "Forge demand" in docs and in the report SLO string becomes "Forge tasks". The clock doc becomes "every run, audit row, and Forge task".
- **Why:** After the merge, `vala.forge_planning_demands` does not exist, so the benchmark would fail at runtime even though it compiles.
- **Replay rule:** Anywhere the merged tree counts Forge backlog or demand, read non-terminal rows of `vala.forge_tasks` by `created_at`. Never reference `vala.forge_planning_demands`.
- **Commit:** ce866229d

### L27. Have `LocalServer::start` take both the owner URL and the envelope
- **Kind:** conflict
- **Files:** crates/wyrd/wyrd-testing/src/release_server.rs, crates/wyrd/wyrd-testing/src/bin/capacity/main.rs, crates/wyrd/wyrd-testing/src/bin/bifrost_forge_capacity/main.rs
- **Verification side (b1b8f25f7):** `start(binary: &Path, owner_url: &str, tenants: &[&str], env: &[(&str, &str)]) -> Result<Self>`. The caller supplies the owner URL, and the 8-CPU/16-GiB envelope is implicit.
- **Forge side (2880705a0):** `start(binary, tenants, env, envelope: Envelope)` reads `WYRD_TEST_DATABASE_ADMIN_URL` inside `start`, and adds the `Envelope { cpu_percent, memory_bytes }` type, `Envelope::POD`, `start_replica` and `start_forge_worker`.
- **Resolution:** `pub async fn start(binary: &Path, owner_url: &str, tenants: &[&str], env: &[(&str, &str)], envelope: Envelope) -> Result<Self>`. `start` does not read the environment, and the doc says it "Migrates `binary` as the database owner at `owner_url` and serves it with `env` added in `envelope`". Call sites:
  - `capacity/main.rs` imports `Envelope` and passes `Envelope::POD`, including in the test stand-in `LocalServer::start(&server, "postgres://unused", &[], &[], Envelope::POD)`.
  - `bifrost_forge_capacity/main.rs` passes `&std::env::var("WYRD_TEST_DATABASE_ADMIN_URL")?` before `&["forge"]`.
- **Why:** The verification side moved the owner URL to the caller (0530fa638, shared starter). The Forge side needs smaller envelopes for the leader and worker pods.
- **Replay rule:** Keep the five-argument `start(binary, owner_url, tenants, env, envelope)`, and have every caller resolve `WYRD_TEST_DATABASE_ADMIN_URL` itself. bifrost-variant 509f972c6 still has the four-argument form that reads the environment internally.
- **Commit:** ce866229d

### L28. Merge release_server process plumbing: OperatorRun, role-aware readiness, joined peer ports
- **Kind:** conflict
- **Files:** crates/wyrd/wyrd-testing/src/release_server.rs
- **Verification side (b1b8f25f7):**
  - Migrated through `OperatorRun::spawn(cmd, &workdir.join("migrate.stderr"))?.finish().await?`.
  - The readiness loop probed `/readyz` and kept the last body in `last`.
  - A replica's peer listener used `PEER_PORT` for ordinal 0 and `JOINED_PEER_PORT` otherwise.
- **Forge side (2880705a0):**
  - Used a synchronous `run(operator(..., Role::Serving, env).arg("migrate")...)`.
  - Added `enum Role { Serving, ForgeWorker }`. A ForgeWorker is probed at `http://127.0.0.1:{HTTP_PORT+1}/metrics` and gets `WYRD_TARGET=forge-worker` and no peer settings.
  - Always used `replica_port(PEER_PORT, ordinal)`.
- **Resolution:**
  - Migration uses `OperatorRun::spawn(operator(binary, &workdir, &storage_url, 0, Role::Serving, env).arg("migrate").env("WYRD_DATABASE_URL", owner_url), ...)?.finish().await?`, then `Self::serve(binary, root, &storage_url, 0, Role::Serving, env, envelope)`.
  - Readiness picks `probe` by `self.role` and keeps the verification-side `if let Ok(response) ... last = response.text()` body capture. The error text is "last probe body: {last}".
  - In `operator()`, `if role == Role::ForgeWorker { WYRD_TARGET=forge-worker } else if WYRD_PEER_TLS_DIR { base = if ordinal == 0 { PEER_PORT } else { JOINED_PEER_PORT } }`.
  - The `start_replica` doc combines the envelope-scope text with the port-base text.
  - The Forge-side `fn run` helper still exists at line 726 of the merged file. I did not check whether anything still calls it.
- **Why:** OperatorRun gives a bounded, reaped migration with stderr capture (verification). Forge workers have no `/readyz` and must not bind a peer listener (Forge).
- **Replay rule:** Keep OperatorRun-based migrate and setup, the role-based readiness probe with body capture, and the `JOINED_PEER_PORT` base for joined serving replicas. ForgeWorker gets only `WYRD_TARGET=forge-worker`.
- **Commit:** ce866229d

### L29. Have the release_server peer-port unit test expect the joined-replica range
- **Kind:** follow-up-fix
- **Files:** crates/wyrd/wyrd-testing/src/release_server.rs (`mod tests`)
- **Verification side (b1b8f25f7):** n/a. This is the side that defines `JOINED_PEER_PORT`.
- **Forge side (2880705a0):** The test hard-coded `Some("127.0.0.1:50072")` for a serving replica at ordinal 2, which is `PEER_PORT` arithmetic.
- **Resolution:** The expectation becomes `format!("127.0.0.1:{}", super::replica_port(super::JOINED_PEER_PORT, 2))`, and the doc now says the address comes "from the joined-replica peer range".
- **Why:** After the merged `operator()` started using `JOINED_PEER_PORT` for ordinal > 0, the Forge-side literal was wrong. The merge left it in place.
- **Replay rule:** Derive the expected peer address in release_server tests from `replica_port(JOINED_PEER_PORT, ordinal)`, never from a literal.
- **Commit:** 41065b39b

### L30. Drop `has_demand` from `ForgeWorkflowInspection` and keep the synchronous `superuser_pool()`
- **Kind:** conflict
- **Files:** crates/wyrd/wyrd-testing/src/server.rs
- **Verification side (b1b8f25f7):** `let pool = self.inner.fixture.superuser_pool().map_err(sql)?;` is synchronous; `wyrd_dev_fixtures::pg::superuser_pool` is a plain `fn` here. The function also ran `SELECT EXISTS(... vala.forge_planning_demands ...)` into `has_demand`. The struct had `storage_root: Option<Arc<tempfile::TempDir>>` and imported `DriftBaselineQueue`.
- **Forge side (2880705a0):** `superuser_pool().await`, no demand query, `_storage_root: Option<Arc<TempDir>>`, and imports `StorageHandle`.
- **Resolution:**
  - Synchronous `superuser_pool().map_err(sql)?`. The demand query, the `has_demand` struct field and the field initializer are all removed, and the docs say "Inspect durable Forge task state".
  - Field `storage_root: Option<Arc<TempDir>>`, keeping the verification-side name without the underscore, with the `TempDir` alias.
  - Imports `use wyrd_sql::queries::drift_baselines::DriftBaselineQueue;` and `use wyrd_storage::{BackendConfig, StorageHandle, StorageSettings};`.
- **Why:** The demand table no longer exists after the rewrite. `superuser_pool` became synchronous on the verification side, which owns `wyrd-dev-fixtures`.
- **Replay rule:** In wyrd-testing, call `superuser_pool()` without `.await` everywhere (bifrost-variant 509f972c6 has 9 `.await` sites in server.rs plus some in tests), and remove any `has_demand` or `forge_planning_demands` reference.
- **Commit:** ce866229d

### L31. Disable the audit publisher in single-table Forge and Oracle journeys
- **Kind:** conflict
- **Files:** crates/wyrd/wyrd-testing/src/bifrost/cluster.rs; tests/bifrost/forge/production_closeout.rs; tests/bifrost/oracle/published.rs
- **Verification side (b1b8f25f7):** `start_with_embedded_forge_observer` and `start_embedded_forge_uncertainty_for_test` wrap the call in `Box::pin(Self::start_spec_with_all_options(BifrostClusterSpec::one_mixed(), ...))`.
- **Forge side (2880705a0):** Adds `BifrostClusterSpec::without_audit_publication_for_test()` (field `audit_publication_disabled: bool`, applied through `builder.without_audit_publication_for_test()` on every start and restart) and uses `one_mixed().without_audit_publication_for_test()` without `Box::pin`.
- **Resolution:**
  - In ce866229d, both functions become `Box::pin(Self::start_spec_with_all_options(BifrostClusterSpec::one_mixed().without_audit_publication_for_test(), ...))`.
  - Follow-up 2d25fac7c also opts out in `CloseoutJourney::start_with_config`: `dedicated_forge_workers().with_scribe_geometry_for_test(..).without_audit_publication_for_test()`.
  - 2d25fac7c does the same in `restart_recovers_hot_promotion_with_empty_schedule`: `two_mixed().without_audit_publication_for_test()`.
  - 2d25fac7c does the same in published.rs: `one_mixed().with_metadata_cache_mode(ScribeCacheMode::Enabled).without_audit_publication_for_test()`.
  - 2d25fac7c removes the production_closeout comment "Audited reads above keep publishing retained audit...".
- **Why:** The verification branch's audit outbox and publisher write to the tenant's audit-log table through Scribe and Forge. That table then takes the one-shot faults, the parked promotion, and the worker slots a single-table journey arms for its own table, and it joins recovered membership. The commit title is "Keep the tenant audit table out of the single-table Forge and Oracle journeys".
- **Replay rule:** Combine `Box::pin(...)` with `.without_audit_publication_for_test()`. Apply the opt-out to every Forge or Oracle journey from bifrost-variant that counts per-table tasks, promotions, membership or one-shot faults. Check new journeys too, not just these four sites.
- **Commit:** ce866229d; 2d25fac7c

### L32. Remove the `audit` field from `CreateTableRequest` literals
- **Kind:** silent-break
- **Files:** crates/wyrd/wyrd-testing/tests/bifrost/server/query.rs
- **Verification side (b1b8f25f7):** `CreateTableRequest { table, user_fields, tenant, physical_layout }`.
- **Forge side (2880705a0):** Adds `pub audit: Option<AuditEvent>` (an audit event committed with the control row), and the test passes `audit: None`.
- **Resolution:** The merged `CreateTableRequest` has no `audit` field. The `audit: None` line in `configured_default_deadline_is_shared_by_local_and_forwarded_queries` is deleted.
- **Why:** Audit has one write path, the non-blocking outbox to `vala.audit_staging` (AGENTS.md §2). Committing audit inline with a catalog row contradicts that, so the verification side wins.
- **Replay rule:** Delete every `audit: None` and `audit: Some(..)` in `CreateTableRequest` literals. bifrost-variant 509f972c6 has them in `src/bifrost/forge_harness.rs`, `query_fixture.rs` and `scribe_workload.rs`.
- **Commit:** ce866229d

### L33. Call `client_from_options` with three arguments
- **Kind:** conflict
- **Files:** crates/wyrd/wyrd-testing/tests/bifrost/server/verification_runtime.rs, crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs
- **Verification side (b1b8f25f7):** `wyrd_client::bifrost::client_from_options(server_url, credential, grpc_url, tenant: Option<&str>)`, and tests pass a fourth argument `None`.
- **Forge side (2880705a0):** `client_from_options(server_url, credential, grpc_url)`.
- **Resolution:** The merged facade has the three-argument signature. The trailing `None` is removed in `two_bindings_share_one_client_observation` and in the `client` closure of `a_gate_write_run_start_and_query_succeed_while_audit_commits_fail`.
- **Why:** Per the merge message, tenant selection is `WYRD_TENANT` only, so there is no per-call tenant override.
- **Replay rule:** Call `client_from_options(url, credential, grpc_url)` and never pass a tenant argument.
- **Commit:** ce866229d

### L34. Add `params: Vec::new()` to Forge-side `BifrostQueryRequest` literals
- **Kind:** silent-break
- **Files:** crates/wyrd/wyrd-testing/tests/bifrost/oracle/distributed.rs (3 sites: `settle_query`, `held_cut_owns_active_reads_until_all_descendants_settle`, `held_query_ids`); tests/bifrost/forge/public_support.rs (field order only)
- **Verification side (b1b8f25f7):** `BifrostQueryRequest { sql, deadline_ms, params: Vec<QueryParam> }`.
- **Forge side (2880705a0):** `BifrostQueryRequest` has no `params`.
- **Resolution:** `params: Vec::new()` is appended after `deadline_ms` in each Forge-origin literal. In public_support.rs `public_query`, which auto-merged with `params` first, the field moves after `deadline_ms`; the content is unchanged.
- **Why:** The verification side's parameterized queries added a required field, so Forge-side literals would not compile.
- **Replay rule:** Give every `BifrostQueryRequest { .. }` from bifrost-variant `params: Vec::new()` as its last field.
- **Commit:** ce866229d

### L35. Drop `.await` on `superuser_pool()` in Forge-side journeys
- **Kind:** conflict
- **Files:** crates/wyrd/wyrd-testing/tests/bifrost/oracle/published.rs
- **Verification side (b1b8f25f7):** `superuser_pool()` is synchronous.
- **Forge side (2880705a0):** `.fetch_one(&cluster.pg_fixture().superuser_pool().await?)`.
- **Resolution:** `.fetch_one(&cluster.pg_fixture().superuser_pool()?)`.
- **Why:** The `wyrd-dev-fixtures` signature is `pub fn superuser_pool(&self) -> Result<PgPool, FixtureError>`.
- **Replay rule:** Remove `.await` after `superuser_pool()` in every bifrost-variant test (509f972c6 has sites in oracle/capacity.rs and oracle/published.rs, plus server.rs).
- **Commit:** ce866229d

### L36. Keep the verification side's client-owned event-time fix in observation journeys
- **Kind:** conflict
- **Files:** crates/wyrd/wyrd-testing/tests/bifrost/server/verification_runtime.rs, crates/wyrd/wyrd-testing/tests/bifrost/server/eval_verification.rs
- **Verification side (b1b8f25f7):**
  - verification_runtime: `const EVENT_TIME_LEAD: chrono::Duration = chrono::Duration::minutes(1)` and `observation_batch(record, card_ref)`, which stamps `wyrd_event_time = Utc::now() - EVENT_TIME_LEAD` (3cafd46c8).
  - eval_verification `sealed_replay_on_a_later_day_activates_once`: `let emitted = chrono::Utc::now(); let frame = answered_observation(&subject, &record, emitted);`, then `shift_receipt_clock_for_test(Duration::from_hours(24))`.
- **Forge side (2880705a0):**
  - verification_runtime: `observed_in_current_month(now)` and a three-argument `observation_batch(record, card_ref, event_time)` (179e91bb9; same flake, different fix).
  - eval_verification: an unstamped frame, `before` and `after` read from `SELECT clock_timestamp()`, `blocked_activations(&superuser, 1)`, and `Duration::from_secs(86_400)`.
- **Resolution:** The verification side is taken in full. That means `EVENT_TIME_LEAD`, the two-argument `observation_batch`, the `Datelike` import removed, `observed_in_current_month` deleted, and the `answered_observation`/`emitted` and `from_hours(24)` form in eval_verification.
- **Why:** On the verification branch the client owns `wyrd_event_time`. Both commits fix the same host-versus-Postgres clock-skew flake, and the verification fix matches its client event-time model. The later follow-up 6e84727c9 changed the builder argument to `Some(at.timestamp_micros())`; that is a verification-side API change, not a merge fix.
- **Replay rule:** Where bifrost-variant fixes an observation-window race with Postgres-clock reads, keep the verification side's client-stamped `wyrd_event_time` version.
- **Commit:** ce866229d

### L37. Merge production_closeout: Forge geometry assertion and held-reader constant, verification Duration style
- **Kind:** conflict
- **Files:** crates/wyrd/wyrd-testing/tests/bifrost/forge/production_closeout.rs
- **Verification side (b1b8f25f7):**
  - `const REWRITE_BOUND: Duration = Duration::from_mins(10)`.
  - `assert_output_geometry` requires exactly one residue below target and at least one output at or above target.
  - `spec.nodes[3].roles = std::mem::take(&mut spec.nodes[0].roles)`.
- **Forge side (2880705a0):**
  - `from_secs(600)` and `const HELD_READER_PASSES: usize = 3`.
  - `assert!(!outputs.is_empty(), "the rewrite publishes outputs")`, documented as "balanced Forge outputs within the 1 GiB target".
  - `spec.nodes[3].roles = spec.nodes[0].roles.clone()`.
- **Resolution:**
  - `REWRITE_BOUND = Duration::from_mins(10)` plus Forge's `HELD_READER_PASSES`.
  - Forge's non-empty assertion; the residue and target-roll assertions are dropped, and the per-file `<= target` loop stays.
  - Roles are cloned through `let scribe_roles = spec.nodes[0].roles.clone(); spec.nodes[3].roles = scribe_roles;`.
  - The float_cmp `reason` on `compaction_geometry_exact_rows_and_non_destructive_second_pass` becomes "Prometheus renders this gauge as whole numbers...".
  - `.map(|server| server.node_id())` becomes `.map(wyrd_testing::WyrdTestServer::node_id)`.
- **Why:** The rewrite's planner balances outputs instead of rolling at the target and leaving one residue, so the verification assertion would fail. The rest is lint style. I could not confirm why `mem::take` became `clone`, since node 0's roles are overwritten on the next line either way. The result matches the Forge side's semantics.
- **Replay rule:** Use the rewrite's balanced-output geometry assertion, keep `HELD_READER_PASSES`, and write durations with `from_mins` and `from_hours`.
- **Commit:** ce866229d

### L38. Fix pedantic clippy findings in Forge-origin code now that wyrd-testing inherits workspace lints
- **Kind:** silent-break
- **Files:** crates/wyrd/wyrd-testing/src/capacity.rs; src/bin/bifrost_forge_capacity/report.rs; tests/bifrost/forge/live_rewrite.rs; tests/bifrost/oracle/{analytical_lifecycle,capacity,distributed,workflow,published}.rs; tests/bifrost/oracle/peer_network/{analytical,join}.rs; tests/bifrost/forge/scribe_promotion.rs (blank line only)
- **Verification side (b1b8f25f7):** wyrd-testing has `[lints] workspace = true` (clippy pedantic, `allow_attributes_without_reason`, `unwrap_used`; truncation and sign casts stay on).
- **Forge side (2880705a0):** wyrd-testing had no `[lints]`, so this code was never linted as pedantic.
- **Resolution:**
  - Every `prove_*()` future awaited directly in a `#[tokio::test]` is wrapped as `Box::pin(prove_...()).await`, presumably for `clippy::large_futures` (I did not check the exact lint name). Sites: analytical_lifecycle ×3, capacity ×1, peer_network/analytical ×3, peer_network/join ×1, workflow ×1.
  - `#[expect(clippy::float_cmp, reason = "...")]` is added on `Report::checks` (bifrost_forge_capacity/report.rs) and on `failed_worker_restarts_while_the_api_serves` (live_rewrite.rs).
  - In published.rs the existing float_cmp `#[expect]` moves below Forge's `# Panics` doc.
  - In distributed.rs, `Duration::from_secs(180)` becomes `from_mins(3)`, `from_secs(120)` becomes `from_mins(2)`, and `.map(|group| group.num_rows())` becomes `.map(parquet::file::metadata::RowGroupMetaData::num_rows)`.
  - In src/capacity.rs `Percentiles`, `use num_traits::ToPrimitive;` is added and `(... .ceil() as usize)` becomes `let rank = (q * sorted.len() as f64).ceil().to_usize().unwrap_or(0); let index = rank.clamp(1, sorted.len()) - 1;`.
- **Why:** These are needed for the `mise run lints` (`--all-features`) gate. AGENTS.md §12 forbids `#[allow]`, so suppressions use `#[expect]` with a reason, and the rest are rewritten.
- **Replay rule:** After merging bifrost-variant, run `mise run lints` and fix each finding in Forge-origin wyrd-testing code with the same idioms: `Box::pin` large futures, `#[expect(clippy::float_cmp, reason=...)]` only where equality is exact, `from_mins`/`from_hours`, method paths instead of closures, `ToPrimitive` instead of `as` casts. Never add `#[allow]`.
- **Commit:** ce866229d

## Client and SDKs

### L39. Drop the per-constructor `tenant` argument and select the tenant only through `WYRD_TENANT`
- **Kind:** conflict
- **Files:** crates/shared/wyrd-client/src/cards/{config.rs,handle.rs,error.rs}; sdks/wyrd-sdk-python/src/bifrost/mod.rs; sdks/wyrd-sdk-python/src/state/mod.rs; sdks/wyrd-sdk-python/python/wyrd/bifrost/__init__.py; sdks/wyrd-sdk-python/python/wyrd/stubs/{bifrost,state}.pyi; sdks/wyrd-sdk-python/tests/unit/client/test_client.py; sdks/wyrd-sdk-rust/tests/integration/observe_run.rs; sdks/wyrd-sdk-ts/native/src/{cards,client,lib}.rs; sdks/wyrd-sdk-ts/wyrd/src/index.ts; sdks/wyrd-sdk-ts/wyrd/index.d.ts and index.d.cts
- **Verification side (b1b8f25f7):** Had the e73f0163e/05f649dce `tenant: Option<&str>` parameter on every constructor: `Cards::new(server_url, credential, tenant)`, `config::load(env, url, cred, tenant)`, `client_from_options(url, cred, grpc, tenant)`, `Bifrost(..., tenant=None)`, `PyWyrdState.start_bifrost(tenant=)`, TS `connectBifrost/connectCards/connectWyrdClient/startBifrost/describeTableConfig(..., tenant)`. It also had the `#[allow(clippy::too_many_arguments)]` attributes and a test_client.py block asserting that `tenant="acme"` beside a credential is refused.
- **Forge side (2880705a0):** 1ca7a4316 "Select the tenant through WYRD_TENANT, not SDK constructors" removes `tenant` from every Rust, Python, and TypeScript constructor.
- **Resolution:** The merge takes the Forge removal everywhere:
  - Rust signatures are `Cards::new(server_url: Option<&str>, credential: Option<SecretString>)`, `config::load(environment: Environment, server_url, credential)` and `client_from_options(server_url, credential, grpc_url)`.
  - The pyo3 signatures are `(table=None, server_url=None, credential=None, grpc_url=None, client=None, client_byte_limit_bytes=None)` for `Bifrost.__new__` and `(table=None, server_url=None, credential=None, grpc_url=None, client_byte_limit_bytes=None)` for `start_bifrost`. Both drop `too_many_arguments`. The refusal message is "cannot be combined with server_url, credential, or grpc_url".
  - `PyCards.__new__` is `(server_url=None, credential=None)`. The merge did not keep the Forge `#[allow(clippy::needless_pass_by_value)]` and its justification comment.
  - TS native/d.ts/index.ts drop `tenant` and `options.tenant` (also `readonly tenant?: never`). Docs now say "the saved login for `WYRD_TENANT` when set, otherwise the newest".
  - test_client.py equals the Forge side byte for byte: the tenant-refusal block and the imports of `Gateway`, `OperatorConnections`, and `WyrdState` are removed.
  - The `connect()` helper in observe_run.rs drops its trailing `None`.
- **Why:** The merge message says "WYRD_TENANT-only tenant selection merged around them". From 1ca7a4316: "A credential already names its tenant… WYRD_TENANT or configuration remains the one selector."
- **Replay rule:** bifrost-variant (509f972c6) forked before e73f0163e and never had `tenant`, so this should not conflict again. If any bifrost-variant hunk or new call site passes a fourth `tenant`/`None` argument to `client_from_options` or a third to `Cards::new`, delete it and do not reintroduce `tenant` on any SDK surface.
- **Commit:** ce866229d

### L40. Keep the injected `Environment` on the client credential and Cards config path
- **Kind:** conflict
- **Files:** crates/shared/wyrd-client/src/transport/credential.rs; crates/shared/wyrd-client/src/cards/{config.rs,handle.rs,error.rs}
- **Verification side (b1b8f25f7):** Reads every environment tier through `crate::environment::Environment`:
  - `CredentialChain::env_only(environment: &Environment, tenant_override: Option<&str>)` and `CredentialChain::credentials_file(environment: &Environment)`.
  - `explicit_token_from_env(&Environment)` and `workload_token_from_env(&Environment, Option<&str>)`.
  - `resolve()` uses `map_or(Err(NoCredentials), …)`.
  - `config::load(environment: Environment, …)`.
  - Tests build an `Environment::from([...])` with a tempdir and take no `ENV_MUTEX` and set no process env.
- **Forge side (2880705a0):** Kept the process-global form: `use wyrd_utils::config_dir::wyrd_config_dir;`, `std::env::var` readers, a 2-argument `config::load(server_url, credential)`, and `ENV_MUTEX` plus `unsafe { std::env::set_var/remove_var("HOME") }` in the cards error test.
- **Resolution:**
  - credential.rs: verification side wholesale (merge == b1b8f25f7). The `wyrd_utils::config_dir::wyrd_config_dir` import is gone.
  - handle.rs: `config::load(Environment::Process, server_url, credential).map_err(WyrdError::from)?`.
  - error.rs: keeps the verification test `cards_client_failures_keep_client_catalog_identity`. Its `cards_in` closure calls `config::load(environment.clone(), server_url, credential.map(SecretString::from))` with no tenant argument. The Forge `Cards::new(...)` + `ENV_MUTEX` + HOME-removal body is dropped.
  - config.rs keeps the verification doc wording minus "and tenant selector".
- **Why:** The merge message says "auth … stay as on this branch". With the environment injected, tests stop mutating process environment under a mutex.
- **Replay rule:** Keep the verification `Environment`-parameterised signatures and drop any `wyrd_config_dir`/`std::env::var`/`ENV_MUTEX` form that bifrost-variant reintroduces. bifrost-variant did not touch these files after 5ab92b003, so this should merge cleanly.
- **Commit:** ce866229d

### L41. Carry both the query `params`/typed terminal and the Forge `compaction_type` through Rust and Python Bifrost
- **Kind:** conflict
- **Files:** crates/shared/wyrd-client/src/bifrost/mod.rs; sdks/wyrd-sdk-python/src/bifrost/mod.rs; sdks/wyrd-sdk-python/python/wyrd/bifrost/__init__.py; sdks/wyrd-sdk-python/python/wyrd/stubs/bifrost.pyi; sdks/wyrd-sdk-rust/src/lib.rs
- **Verification side (b1b8f25f7):** Had positional SQL binds and a typed terminal:
  - `pub use wyrd_spec::vala::api::QueryParam;`
  - Python imports `{BifrostQueryRequest, PhysicalLayoutWire, QueryParam, QueryTerminalFrame}`.
  - Stubs: `QueryParam = None | bool | int | float | str`, `class QueryTerminal`, `terminal -> QueryTerminal | None`.
  - `sql(self, query, params: Sequence[QueryParam] | None = None)`, `sql(..., params=None, *, model: type[_Row])`, and `stream(query, params=None, *, deadline_ms=None)`.
- **Forge side (2880705a0):** c3a205277 and 1c3d18755 added the compaction type:
  - `pub use wyrd_spec::vala::api::CompactionTypeWire;`
  - Python `PyTableConfig.from_json_schema/from_arrow_ipc(..., compaction_target_file_size_bytes=None, compaction_type=None)`, the `apply_compaction_type(config, raw: Option<&str>) -> WyrdPyResult<TableConfig>` helper and the `compaction_type` getter.
  - Stub/`__init__.py` `compaction_type: str | None = None` with long Google-style docstrings, and `terminal -> dict[str, Any]`.
  - Rust SDK test `sdk_bifrost_names_the_compaction_type` with `use super::bifrost::{CompactionTypeWire, TableConfig};`.
- **Resolution:** The merge takes the union.
  - wyrd-client re-exports both `CompactionTypeWire` (with its doc line) and `QueryParam`.
  - The Python ext import is `use wyrd_spec::vala::api::{BifrostQueryRequest, CompactionTypeWire, PhysicalLayoutWire, QueryParam, QueryTerminalFrame};`.
  - `from_json_schema`/`from_arrow_ipc` wrap `apply_compaction_type(apply_compaction_target(apply_layout(..)?, bytes), compaction_type)?`.
  - Stubs keep the verification `QueryParam`, `QueryTerminal`, the `params` overloads (one-line `...` bodies), and `terminal -> QueryTerminal`. They add the Forge `compaction_type` args, getter, and expanded `TableConfig`/`describe`/`Bifrost.__init__` docstrings.
  - The `__init__.py` `sql` docstring keeps the verification `params`/`model` paragraphs and the Forge `Raises:` block.
  - The Rust SDK lib.rs keeps the verification module docs and `cli`/`WyrdError` re-exports, plus the Forge compaction-type test.
- **Why:** The merge message says "the rewrite's unrelated additions … merged around them". The two features touch adjacent lines but are independent.
- **Replay rule:** In every Bifrost import or re-export conflict, keep both `QueryParam`/`QueryTerminalFrame` and `CompactionTypeWire`. In stubs, keep the `params: Sequence[QueryParam] | None` overloads and `QueryTerminal` return types, and add bifrost-variant's timestamp/doc additions on top.
- **Commit:** ce866229d

### L42. Thread the TypeScript `compactionType` through the `TableConfigOptions` object and catalog-result native call
- **Kind:** conflict (the native function was rewritten silently inside the conflicted file)
- **Files:** sdks/wyrd-sdk-ts/native/src/lib.rs; sdks/wyrd-sdk-ts/wyrd/src/index.ts; sdks/wyrd-sdk-ts/wyrd/index.d.ts; sdks/wyrd-sdk-ts/wyrd/index.d.cts; (follow-up) sdks/wyrd-sdk-ts/wyrd/tests/unit/bifrost-query.test.ts; sdks/wyrd-sdk-ts/wyrd/tests/integration/bifrost-write.test.ts
- **Verification side (b1b8f25f7):** `TableConfig.fromJsonSchema(table, schema, options: TableConfigOptions = {})`. Native `table_config_from_json_schema(...) -> Result<NativeTableConfigResult>` delegates to `declared_table_config(...) -> StdResult<TableConfig, BifrostClientError>`, so refusals return as catalog metadata (`nativeHandle(declared.config, declared.error)`). Imports `{BifrostQueryRequest, QueryParam}` and `PhysicalLayoutWire`.
- **Forge side (2880705a0):** Positional `fromJsonSchema(table, schema, layout?, compactionTargetFileSizeBytes?, compactionType?)` returning `new TableConfig(tableConfigFromJsonSchema(...))`. Native returns `Result<NativeTableConfig>` with `napi::Error`, and `apply_compaction_type(..) -> napi::Result<TableConfig>` uses `napi::Error::from_reason`. It also has `#[allow(clippy::needless_pass_by_value)]` and `export type CompactionType = "auto" | "full" | "small-files" | "files-with-delete"`.
- **Resolution:**
  - The Rust native keeps the verification result shape: `table_config_from_json_schema(table, schema_json, layout_json, compaction_target_file_size_bytes, compaction_type: Option<String>) -> Result<NativeTableConfigResult>`, which calls `declared_table_config(..., compaction_type.as_deref())`.
  - `apply_compaction_type` is rewritten as `fn apply_compaction_type(config: TableConfig, raw: Option<&str>) -> StdResult<TableConfig, BifrostClientError>`. Its error is `validation_error(format!("invalid compaction type: {error}"), "compactionType")`.
  - The import is `use wyrd_spec::vala::api::{BifrostQueryRequest, QueryParam}; use wyrd_spec::vala::api::{CompactionTypeWire, PhysicalLayoutWire};`, and the Forge `needless_pass_by_value` allow is not kept.
  - index.ts keeps `options: TableConfigOptions` and adds `readonly compactionType?: CompactionType` to `TableConfigOptions`. It passes `options.compactionType` and adds the `CompactionType` type, `TableDescription.compaction_type?`, and the `get compactionType()` getter. Unknown spellings throw `WYRD_SPEC_400_VALIDATION` through `nativeHandle`.
  - d.ts/d.cts declare `tableConfigFromJsonSchema(..., compactionType?): NativeTableConfigResult`.
  - Follow-up 41065b39b rewrote the TS tests from the positional form `fromJsonSchema(fqn, SCHEMA, undefined, undefined, kind)` to `fromJsonSchema(fqn, SCHEMA, { compactionType: kind })`.
- **Why:** On this branch, catalogued declaration refusals come back as `WyrdError` metadata, not napi errors, and the public API takes one options object. 41065b39b: "pass the TypeScript compaction type through TableConfigOptions."
- **Replay rule:** bifrost-variant still has the positional `fromJsonSchema(..., layout?, compactionTargetFileSizeBytes?, compactionType?)`, a positional `fromArrow(...)`, and native `table_config_from_json_schema`/`table_config_from_arrow_ipc` that raise `napi::Error` (including in `apply_compaction_type`). Fold all of them into the `TableConfigOptions` object and the `declared_table_config`-style `StdResult<_, BifrostClientError>` → `NativeTableConfigResult` path, with `validation_error(..., "compactionType")`. Then rewrite every TS test call to the options form.
- **Commit:** ce866229d; follow-up 41065b39b

### L43. Keep the Python `wyrd.verification` module and the TypeScript `Verification` client deleted
- **Kind:** conflict (modify/delete)
- **Files:** sdks/wyrd-sdk-python/src/verification.rs; sdks/wyrd-sdk-python/python/wyrd/stubs/verification.pyi; sdks/wyrd-sdk-python/python/wyrd/verification/__init__.pyi; sdks/wyrd-sdk-ts/native/src/verification.rs; sdks/wyrd-sdk-ts/wyrd/src/index.ts (`export class Verification`); sdks/wyrd-sdk-ts/wyrd/index.d.ts and index.d.cts (`connectVerification`)
- **Verification side (b1b8f25f7):** Deleted them. ea13a8824 "Project observe.verify, Judgment, and Run aliases to Python and drop wyrd.verification"; 51baa7f21 typed the TS observe.verify/Run alias and removed `class Verification`; eaf24f690 deleted the TS native verification.rs.
- **Forge side (2880705a0):** Modified them. The changes were docs only (407167771 and 2ed6cd036: "`WYRD_TENANT` selects the saved user login", "UUID" → "`UUIDv7`", expanded stub docstrings) plus the tenant thread that was added and then removed (net zero).
- **Resolution:** All of these files stay deleted. The `Verification` class and `connectVerification(serverUrl?, credential?)` declaration are not resurrected, and the Forge doc edits are discarded.
- **Why:** The verification work moved this surface to `observe.verify`/`Judgment`/`Run` (the merge message says "Verification … and the SDKs stay as on this branch").
- **Replay rule:** Resolve any modify/delete on these paths as delete, and never re-add `Verification`/`connectVerification`. bifrost-variant has not changed them since 5ab92b003, so git should apply the deletion cleanly.
- **Commit:** ce866229d

### L44. Remove WyrdTestServer hooks and the duplicate method the merge resurrected in the testing stub
- **Kind:** follow-up-fix
- **Files:** sdks/wyrd-sdk-python/python/wyrd/stubs/testing.pyi; sdks/wyrd-sdk-python/python/wyrd/testing/__init__.pyi (also crates/wyrd/wyrd-testing/src/release_server.rs and sdks/wyrd-sdk-python/tests/unit/runtime/test_public_api_parity.py in the same commit; both are outside this scope)
- **Verification side (b1b8f25f7):** Methods `wait_for_baseline(verifier, timeout)` and `make_binding_due(binding_id)`, plus the scoped-key, seed-tenant, SSO, and saved-login helpers. It had no Oracle or query fault hooks.
- **Forge side (2880705a0):** `make_binding_due` (Args/Raises docstring), `verification_runs`, `retire_fitted_format`, `table_describe_count`, `fail_table_describe`, `restore_table_describe`, `ensure_builtin_table`, `prepare_oracle_query_fixture`, `fail_next_query_after_schema/_batch`, `stall_next_query_after_schema`, `wait_query_schema_stall`, `bifrost_query_resource_snapshot`, `wait_bifrost_query_resources_released`, `query_denied_token`, `bifrost_read_decision_count`, `wait_oracle_audit_staged`.
- **Resolution:** ce866229d took the verification conflict hunk (`wait_for_baseline` + `make_binding_due`), but auto-merged the Forge `make_binding_due`, `verification_runs`, `retire_fitted_format` and all the Oracle/query hooks around it. That left `make_binding_due` defined twice. 41065b39b deleted the duplicate and every resurrected hook. HEAD's `WyrdTestServer` stub methods are `access_token`, `flush_bifrost`, `__enter__/__exit__`, `base_url`, `api_key`, `tenant_id`, `bootstrap_service`, `credential_registered_service`, `make_binding_due` (Forge docstring), `wait_for_baseline`, `scoped_api_key`, `revoke_scoped_role`, `seed_tenant`, `bootstrap_service_in_tenant`, `activate_human_sso`, `save_human_login`, `expire_saved_login`, `saved_login_is_stale`, `revoke_saved_login`.
- **Why:** 41065b39b: "Drop testing hooks the merge resurrected so WyrdTestServer exposes only its three sanctioned controls."
- **Replay rule:** After merging bifrost-variant, check that stubs/testing.pyi has exactly the HEAD method list above and no duplicate `def`. Drop any Oracle/query fault-injection hook that comes back, and run `mise run codegen:stubs`.
- **Commit:** 41065b39b

### L45. Resolve Python stub docstring conflicts to the verification text and keep Forge's non-conflicting doc expansions
- **Kind:** conflict
- **Files:** sdks/wyrd-sdk-python/python/wyrd/stubs/{agent,cards,data,state}.pyi (plus the bifrost.pyi docstring hunks)
- **Verification side (b1b8f25f7):** Has the stub contracts below. In each case the verification docstring wording is kept and the Forge rewrite is dropped.
  - agent.pyi `Agent.__init__` Args in `name (Type): …` style, including `mock_provider (MockProvider | None)`, and `def to_card(self) -> AgentCard`, plus the `ConversationTurn`/`Conversation`/`CallbackContext`/`MockProvider` classes and `from .cards import AgentCard`.
  - cards.pyi `CardRef.__init__(..., *, space: str, uid=...) -> None: ...` with no docstring, plus `__str__`.
  - data.pyi typed-arg docstrings for `FieldSpec` and DataCard `from_artifact`, including `target_columns`.
  - state.pyi `start_bifrost` prose ("The transport arguments are ``Bifrost(...)``'s and pass straight through…").
- **Forge side (2880705a0):** 407167771-style rewrites of those docstrings: `to_card(self) -> JsonDict`, a documented `CardRef.__init__` with Raises, "as for the interface overload" args, and Args sections. Also non-conflicting expansions (`Role.System`, `SessionTurn(role: Role | str, content, call_id=None)` with `model_dump/model_validate*`, AgentCard Attributes, `VersionBump` semantics, …).
- **Resolution:** In every conflict hunk the verification side wins, including `to_card -> AgentCard`, the undocumented `CardRef.__init__` plus `__str__`, and the verification `data`/`state` text. All the Forge non-conflicting doc and API additions stay as auto-merged. 41065b39b then changed the Agent callback sentence to "Every callback receives a read-only ``CallbackContext`` with ``agent_id``, ``session_id``, ``iteration``, and ``conversation`` attributes as its first argument."
- **Why:** These stubs describe the verification branch's typed Card envelopes (`AgentCard`) and its typed `CallbackContext`. 41065b39b: "read CallbackContext as typed attributes."
- **Replay rule:** In stub conflicts, keep the HEAD signatures (`to_card -> AgentCard`, `CardRef.__str__`, `CallbackContext` attributes, the `QueryParam`/`QueryTerminal` types) and take bifrost-variant's doc additions only where they do not contradict them.
- **Commit:** ce866229d; follow-up 41065b39b

### L46. Regenerate the Python public `__init__.pyi` stubs and the napi `.d.ts`/`.d.cts` from source
- **Kind:** conflict
- **Files:** sdks/wyrd-sdk-python/python/wyrd/{agent,bifrost,cards,data,state,testing}/__init__.pyi; sdks/wyrd-sdk-ts/wyrd/index.d.ts; sdks/wyrd-sdk-ts/wyrd/index.d.cts
- **Verification side (b1b8f25f7):** Generated artifacts of its own sources.
- **Forge side (2880705a0):** Generated artifacts of its own sources.
- **Resolution:**
  - Each merged `wyrd/<m>/__init__.pyi` equals `stubs/<m>.pyi` with the two-line `# AUTO-GENERATED STUB FILE` header and imports rewritten to `.._wyrd`/`..cards`/`..prompt` (checked: byte-identical apart from those lines). They are produced by `sdks/wyrd-sdk-python/scripts/assemble_stubs.py` (`mise run codegen:stubs`).
  - index.d.cts is identical to index.d.ts (`ts:build` runs `cp index.d.ts index.d.cts`).
  - The declarations match the merged native: there is no `tenant`, no `connectVerification`, and `tableConfigFromJsonSchema(..., compactionType?) : NativeTableConfigResult`.
- **Why:** These files are generated (`codegen:check` and `ts:napi:check` fail on drift), and only the `stubs/*.pyi` sources and the native Rust are hand-authored.
- **Replay rule:** Never hand-resolve these files. Resolve `stubs/*.pyi` and `native/src/*.rs`, then run `mise run codegen:stubs` and `mise run ts:build`, and take their output.
- **Commit:** ce866229d

### L47. Merge the gRPC transport test conflict: keep the verification wiremock exchange and take the Forge refresh tests
- **Kind:** conflict + silent-break
- **Files:** crates/shared/wyrd-client/src/bifrost/grpc.rs
- **Verification side (b1b8f25f7):** `unauthenticated_refusal_forces_one_shared_refresh_and_resends` mounts a `wiremock::MockServer` (`let exchange = …` returning `token-1`/`token-2`, `up_to_n_times(1)`). `retain_held_batch(service, attempts) -> (usize, Vec<u64>)` uses a static bearer, and the loss observer ends with `.push(rows);` (from the pedantic-lint pass in 515f16c97).
- **Forge side (2880705a0):** Main's (via fe2da3e3b) tests without the exchange block: `RefreshHeldIngest`, `delayed_refresh_spends_one_transport_budget_then_retains`, and `retain_held_batch(service, attempts, credential: ResolvedCredential, http_timeout_ms: u64) -> (usize, Vec<u64>, Arc<AuthMiddleware>)`. The `HeldSource`/`SequencedSource` `fn identity(&self) -> &str` methods are new from main.
- **Resolution:** The merge keeps the verification exchange block in the conflict hunk and takes all the Forge/main test additions and the new `retain_held_batch` signature. The imports become `{AccessTokenSource, MintedAccessToken, ResolvedCredential}` + `wyrd_spec::auth::SecretBearer`. It also hand-edits (neither side had this) `fn identity(&self) -> &'static str` on `HeldSource` and `SequencedSource`, and keeps `.push(rows);`.
- **Why:** The verification side is authoritative for auth. The `&'static str` edit is not explained in any commit message; it presumably satisfies the workspace pedantic clippy (`unnecessary_literal_bound`) that this branch enforces. I am uncertain whether the kept `exchange` mock is still functionally needed.
- **Replay rule:** Keep HEAD's version of this test module, including `&'static str` identities, `.push(rows);` and the exchange mock. Layer bifrost-variant's grpc.rs changes on top, applying the 3-argument `retain_held_batch` and 3-tuple return to any bifrost-variant call sites.
- **Commit:** ce866229d

### L48. Keep both Python compaction-type and query-param Bifrost journeys
- **Kind:** conflict
- **Files:** sdks/wyrd-sdk-python/tests/integration/bifrost/test_bifrost_e2e.py
- **Verification side (b1b8f25f7):** `test_negative_non_select_query_is_refused(wyrd_server, query_table)` uses the shared `query_table` fixture.
- **Forge side (2880705a0):** Adds `test_compaction_type_registers_describes_and_conflicts`, which asserts `"created"`/`"already_exists"`, `described.compaction_type == "small-files"` and `WYRD_VALA_409_BIFROST_COMPACTION_TYPE_MISMATCH`/409. Its non-select test has no `query_table`.
- **Resolution:** The merge keeps the Forge compaction-type test verbatim, inserted before the verification `test_negative_non_select_query_is_refused(wyrd_server, query_table)`. Later, unrelated TASK-017 commits d9af2ef2e and bae38db2e rewrote this file. At HEAD, the test is `test_compaction_type_registers_describes_and_conflicts(wyrd_server)` with fixed table `vala.datasets.compaction_type`.
- **Why:** The two tests cover independent features, so both are kept.
- **Replay rule:** Resolve against HEAD's rewritten file. Keep its compaction-type and `query_table`-based tests, and add only bifrost-variant's new tests (timestamp/Variant), adapted to HEAD's fixtures.
- **Commit:** ce866229d

### L49. Keep the relocated Rust observe-run journey and drop the tenant argument
- **Kind:** conflict (rename/modify)
- **Files:** sdks/wyrd-sdk-rust/tests/integration/observe_run.rs (Forge path sdks/wyrd-sdk-rust/tests/observe_run.rs)
- **Verification side (b1b8f25f7):** Moved the file into the single `tests/integration` binary. It has `BURST_OBSERVATIONS: u16`, `BURST_FEATURES: u16`, `usize::from(...)`, a `feature_count` local, and the added `sustained_hundred_feature_drift_lands_exactly_once_with_flat_client_bytes` (AC-041) test.
- **Forge side (2880705a0):** Old path with `u32` constants, `usize::try_from(...).expect(...)`, `i64::from(BURST_OBSERVATIONS * BURST_FEATURES)`, and the tenant thread that was added and then removed.
- **Resolution:** The merge keeps the verification file at `tests/integration/observe_run.rs` (u16 constants and the sustained test). The only change vs b1b8f25f7 is dropping the trailing `None` from `client_from_options(...)` in `connect()`. Afterwards, 15155ba5d (TASK-017, not merge-related) deleted this file: the reliability tests moved to crates/wyrd/wyrd-testing/tests/bifrost/server/observe_ingest.rs, and sdks/wyrd-sdk-rust/tests/integration/observe_a_run.rs remains.
- **Why:** The merge message says "the SDKs stay as on this branch". The tenant removal follows the WYRD_TENANT-only decision.
- **Replay rule:** bifrost-variant edits the old `sdks/wyrd-sdk-rust/tests/observe_run.rs`: its Eval/observe row `context`/`media` become `serde_json::Value` read as Variant. Resolve that modify/delete as delete. Then port the Variant-as-JSON-value readback assertions into whichever HEAD test reads back eval context; I could not find an equivalent `context`/`media` assertion at HEAD, so confirm that owner before dropping the coverage.
- **Commit:** ce866229d (later deletion 15155ba5d)

### L50. Take the verification Python CLI module wholesale
- **Kind:** conflict
- **Files:** sdks/wyrd-sdk-python/python/wyrd/cli/__init__.py
- **Verification side (b1b8f25f7):** The module docstring is "The ``wyrd`` CLI, in process and as the installed executable." and the module exposes the in-process CLI commands (about 79 more lines than the Forge side).
- **Forge side (2880705a0):** A docstring-only change: "Python entry point for the Wyrd CLI. ``run_wyrd_cli()`` reads ``sys.argv``…"
- **Resolution:** merge == b1b8f25f7 byte for byte, and the Forge docstring is dropped.
- **Why:** The verification branch rewrote this module into the in-process CLI surface (the merge message says "SDKs stay as on this branch").
- **Replay rule:** Keep HEAD's cli/__init__.py. bifrost-variant does not touch it, so no conflict is expected.
- **Commit:** ce866229d

## Follow-up fixes after the merge

### L51. Settle audit staging before the progress row so a freeze never waits
- **Kind:** follow-up-fix
- **Files:** crates/vala/vala-sql/src/queries/audit_staging.rs, crates/vala/vala-sql/tests/integration/pg_audit_staging.rs
- **Verification side (b1b8f25f7):** Publication progress lives in its own row, `vala.audit_publication` (migration `20261003000000_audit_publication_progress.sql`, renumbered `20261003000200` in the merge). `settle_publication` upserted the progress row first (`INSERT ... ON CONFLICT DO UPDATE SET published_seq = GREATEST(...)`). It then ran `DELETE FROM vala.audit_staging WHERE seq <= (SELECT published_seq FROM vala.audit_publication)`. `freeze_publication_range` does an unconditional `INSERT ... ON CONFLICT DO NOTHING` and then a `FOR UPDATE NOWAIT` read.
- **Forge side (2880705a0):** Progress lives in columns on `vala.audit_chain_head` (`UPDATE vala.audit_chain_head SET published_seq = ...`). The publisher sweeps continuously: it waits 250 ms only when idle and starts the next sweep as soon as a cycle finishes. Before this, it ran on a fixed 5 s tick.
- **Resolution:** `settle_publication(conn, seq_hi)` runs `DELETE FROM vala.audit_staging WHERE seq <= $1` (bound to `seq_hi`) first, then the progress upsert, and returns the delete's `rows_affected()`. The freeze insert gains `AND NOT EXISTS (SELECT 1 FROM vala.audit_publication)`, so it skips a row that already exists and never queues on the ON CONFLICT check. The rustdoc states the lock order: staging rows are always taken before the progress row. Regression test: `pg_audit_staging::pg_tests::audit_staging::settlement_waiting_on_staged_rows_never_stalls_a_freeze`.
- **Why:** The settle held the uncommitted progress-row upsert while it waited on staged-row locks, so a concurrent freeze queued behind it. That breaks the "lock acquisition never waits" contract. The bug was already in the verification shape. Forge's continuous publisher made freeze/settle collisions frequent enough for the gate to hit it (TASK-017 evidence: "fixes a lock-order stall that a gate surfaced").
- **Replay rule:** Take HEAD's `audit_staging.rs` wholesale (delete-before-upsert settle, guarded freeze insert, `vala.audit_publication` progress), then re-apply only the variant's one change: `pub fn entry_hash` with `#[must_use]` and its doc lines, because the variant's `verification_runtime.rs` calls it.
- **Commit:** ffaa142b4

### L52. Keep the tenant audit table out of journeys that count pod-wide or single-table work
- **Kind:** follow-up-fix
- **Files:** crates/wyrd/wyrd-testing/tests/bifrost/forge/production_closeout.rs, crates/wyrd/wyrd-testing/tests/bifrost/oracle/published.rs, crates/wyrd/wyrd-testing/tests/bifrost/scribe/budgets.rs, crates/wyrd/wyrd-testing/tests/bifrost/scribe/telemetry.rs
- **Verification side (b1b8f25f7):** Built-in tables are created eagerly, and the audit outbox stages every permission decision. The tenant audit table therefore exists and receives rows during every journey.
- **Forge side (2880705a0):** Adds `BifrostClusterSpec::without_audit_publication_for_test()` and applies it only to the fault journeys that failed on the Forge branch. The publisher now publishes the tenant audit table within about 250 ms.
- **Resolution:** Add `.without_audit_publication_for_test()` in four places:
  - `CloseoutJourney::start_with_config`, chained after `.with_scribe_geometry_for_test(...)`.
  - `restart_recovers_hot_promotion_with_empty_schedule`: `BifrostClusterSpec::two_mixed().without_audit_publication_for_test()`.
  - `prove_published_governance`: `one_mixed().with_metadata_cache_mode(...).without_audit_publication_for_test()`.
  - `staged_backlog_survives_abrupt_restart`: `one_mixed().without_audit_publication_for_test()`.

  `scribe_shards_obey_global_and_tenant_budgets` replaces `start_scribe_server()` with `WyrdTestServer::builder().without_audit_publication_for_test().start_bound()`, and the `start_scribe_server` import is dropped. The stale comment "Audited reads above keep publishing retained audit..." in `compaction_geometry_exact_rows_and_non_destructive_second_pass` is shortened.
- **Why:** With continuous publication, the tenant audit table took Forge worker slots, parked promotions, and pod-wide Scribe insertion and budget counters that these journeys assert as theirs alone. Which verification change made this visible is my inference; the commits do not name the trigger.
- **Replay rule:** Wherever the variant edits these four files, keep every `without_audit_publication_for_test()` call from HEAD, and add it to any new variant journey that asserts pod-wide Scribe or Forge counts or a single table's promotion or compaction.
- **Commit:** 2d25fac7c, e03e3b9c8

### L53. Reconcile Forge-branch tests and stubs with the TASK-008/TASK-016 public surfaces
- **Kind:** follow-up-fix
- **Files:** crates/wyrd/wyrd-testing/src/release_server.rs, sdks/wyrd-sdk-python/python/wyrd/{agent/__init__.pyi,stubs/agent.pyi,stubs/testing.pyi,testing/__init__.pyi}, sdks/wyrd-sdk-python/tests/unit/runtime/test_public_api_parity.py, sdks/wyrd-sdk-ts/wyrd/tests/integration/bifrost-write.test.ts, sdks/wyrd-sdk-ts/wyrd/tests/unit/bifrost-query.test.ts
- **Verification side (b1b8f25f7):**
  - `WyrdTestServer` exposes only `flush_bifrost`, `wait_for_baseline`, and `make_binding_due`.
  - The joined peer port is `JOINED_PEER_PORT = 30052`.
  - Callbacks receive a typed `CallbackContext`.
  - TS `TableConfig.fromJsonSchema(table, schema, options: TableConfigOptions = {})`.
- **Forge side (2880705a0):** `testing.pyi` declares about 14 extra hooks (`prepare_oracle_query_fixture`, `query_denied_token`, `fail_next_query_after_schema`, `wait_oracle_audit_staged`, ...). The release_server test hard-codes `"127.0.0.1:50072"`. The parity test reads `ctx["iteration"]`. TS tests call `fromJsonSchema(fqn, SCHEMA, undefined, undefined, compactionType)`.
- **Resolution:**
  - Delete the 137 resurrected hook lines from both testing stubs.
  - The release_server test expects `format!("127.0.0.1:{}", super::replica_port(super::JOINED_PEER_PORT, 2))`.
  - The agent docstring reads "Every callback receives a read-only ``CallbackContext`` with ... attributes".
  - The parity test uses `ctx.iteration`.
  - TS tests use `fromJsonSchema(fqn, SCHEMA, { compactionType })`.
- **Why:** The merge auto-took Forge-side tests and stubs written against the pre-TASK-016 surfaces.
- **Replay rule:** Keep HEAD's versions of these files. In `bifrost-query.test.ts`, the only one the variant edits, rewrite every `undefined, undefined, <type>` call to `{ compactionType: <type> }`. Regenerate stubs with `mise run codegen:check` and never keep a testing hook beyond the three sanctioned controls.
- **Commit:** 41065b39b

### L54. Wait for the Oracle envelope release as well as the graph drop before asserting revocation
- **Kind:** follow-up-fix
- **Files:** crates/vala/vala-bifrost-redux/src/oracle/analytical.rs (test module)
- **Verification side (b1b8f25f7):** Not involved (pre-113298433 synchronous `release_dropped`).
- **Forge side (2880705a0):** 113298433 replaced `release_dropped` with `reclaim()`, which spawns the `supervisor.release_graph` task. The graph leaves `live_graphs()` before its envelope returns the analytical query slot.
- **Resolution:** The leader-lifecycle test's `await_until` also requires `oracle.snapshot().is_ok_and(|live| live.oracle_analytical_queries == 0)`.
- **Why:** The test read the envelope count in the window between graph removal and the spawned release.
- **Replay rule:** Keep HEAD's `reclaim()` and this two-condition wait. The variant's analytical.rs edits (the `OracleVariantSql::shared().install(...)` session builders and the `begin_draining` admission test) apply on top of HEAD's version.
- **Commit:** b83f34bd8

### L55. Bring SDK journeys in line with the TASK-016 query, drift, and binding surfaces
- **Kind:** follow-up-fix
- **Files:** sdks/wyrd-sdk-python/tests/integration/{state/test_observe_journey.py,test_drift_journey.py,test_verification_execute_journey.py}, sdks/wyrd-sdk-ts/wyrd/tests/integration/verification-execute.test.ts
- **Verification side (b1b8f25f7):** These lines were already stale on this side: `query.sql(sql, DriftRow)` positional, `"latency": "drift"`, and Eval bindings on `runs_on: {kind: schedule}`. This is TASK-016 test debt rather than a strict Forge×verification interaction; the packet records it as a post-merge fix.
- **Forge side (2880705a0):** Same positional shape.
- **Resolution:** Python `query.sql(..., model=DriftRow)` (`params` now holds the second positional slot). The drift verdict is expected as `"Drift"`. Eval Verifiers (`EVAL_VERIFIERS` / TS `EVAL`) bind `runs_on: {kind: observations_ready}`. HEAD later renamed these files (TASK-017: `test_observe_a_run.py`, `test_query_bifrost.py`, etc.).
- **Why:** TASK-016 changed the signatures to Python `sql(query, params=None, *, model=...)` and TS `sql(query, params?, rows?)`. Registration also requires Eval bindings on `observations_ready`.
- **Replay rule:** Rewrite every variant-added call:
  - Python `bifrost.sql(q, Model)` becomes `bifrost.sql(q, model=Model)`. Seen in test_variant_tables.py, test_register_a_table_from_a_model.py, and others.
  - TS `x.sql(q, rows)` becomes `x.sql(q, [], rows)`. Seen in variant-tables, register-a-table-from-a-model, every-iceberg-column-type, and others.
  - Use bound `$n` params instead of f-string or template-literal values where a value is interpolated.
- **Commit:** bd9f0b2c3

### L56. Drop merge-hygiene leftovers
- **Kind:** follow-up-fix
- **Files:** crates/vala/vala-bifrost-redux/src/oracle/analytical.rs, changes/active/forge-concurrent-planning/review/packet-wide-7fcb45fc1/navigation-map.md
- **Verification side (b1b8f25f7):** `#[allow(clippy::type_complexity, reason = "...")]` on `AnalyticalStageIngress::publish`.
- **Forge side (2880705a0):** The same allow, written as a `// justification:` comment plus a bare `#[allow]`. Its navigation-map.md has a trailing blank line.
- **Resolution:** The allow is removed entirely, and the trailing blank line is trimmed. The `publish` signature is identical on both sides; why the allow became unneeded in the merged tree is not recorded.
- **Why:** The lint and whitespace checks failed on the merged tree.
- **Replay rule:** Do not reintroduce the `type_complexity` allow on `publish`. The variant does not carry navigation-map.md.
- **Commit:** 8d98c3ceb, cd7844cd1

### L64. Wait for a routable Oracle at test-server start and keep audit publication out of the stage-pressure journey
- **Kind:** follow-up-fix (found under the gate's SDK-lane concurrency after the merge)
- **Files:** crates/wyrd/wyrd-testing/src/server.rs, crates/wyrd/wyrd-testing/tests/bifrost/scribe/write_read.rs
- **Symptom:** Under gate load, Scribe restart journeys received a 503 (`no ready Oracle advertises both query classes … live_oracles=0`). `acknowledged_rows_survive_stage_pressure_and_restart` failed its flush with `ingest busy for table: memory`.
- **Cause 1:** `wait_for_ready` polled only `/healthz`, which always answers `ok`. A peer-mode Oracle is activated afterwards, in the peer serve task (`app/server.rs` `activate_peer_roles`). A query sent straight after a start or restart could therefore find no live Oracle.
- **Cause 2:** Forge's continuous audit publisher (L57) writes `vala.system.audit_log` through the same Scribe. A member it stages before the test fills the root is refused by that full root, and `flush_staged` returns the refusal.
- **Resolution:**
  - `wait_for_ready(base_url, state)` also waits for `state.bifrost_query().is_none_or(Oracle::is_ready)`. That is the check `/readyz` reports, read in process. Polling `/readyz` itself was rejected because its snapshot is cached on a 5 s tick and it also gates on Forge, which quiet tests hold back.
  - The stage-pressure journey opts out with `.without_audit_publication_for_test()`, as the other Scribe isolation journeys do.
- **Replay rule:** Keep both. Any variant Scribe journey that fills the root, or counts staged work while the audit publisher runs, takes the same opt-out (see also L52).

**Not merge fallout** (verification work after the merge; still carry it through the variant merge): 6e84727c9 (client UTC stamp on observation writes), 5ff25deb6 (Forge commit retries bounded by count, not a deadline), ac327a227 (`WYRD_VALA_403_BIFROST_CARD_SCOPE` maps to `CardScopeDenied`), b620b069f (Oracle `$n` binding via `with_param_values`, REQ-200), d7524b19b (removes the empty `test:bifrost:unit:python:inner` lane and fixes select-ci paths).

## Forge commits the bifrost-variant branch lacks

The variant forked from Forge at `5ab92b003`. These first-parent Forge commits up to `2880705a0` reached this branch only through `ce866229d`.

### L57. Publish audit continuously instead of on a fixed 5 s tick
- **Kind:** forge-delta-missing-from-variant
- **Files:** crates/wyrd/wyrd-server/src/audit/publication.rs, crates/vala/vala-sql/src/queries/audit_staging.rs, crates/vala/vala-sql/migrations/20261003000300_audit_publication_operator_read.sql (merge), crates/wyrd/wyrd-server/src/app/server.rs, crates/wyrd/wyrd-testing/src/bifrost/cluster.rs, tests under wyrd-testing/tests/bifrost/{forge/production_closeout.rs,scribe/source_boundary_recovery.rs,server/query.rs,server/verification_runtime.rs}
- **Verification side (b1b8f25f7):** `PUBLICATION_INTERVAL = 5s` with `tokio::time::interval`, listing every active tenant. Progress lives in `vala.audit_publication`.
- **Forge side (2880705a0):** `PUBLICATION_IDLE = 250ms` and `PUBLICATION_RETRY = 5s`. `run()` loops `sweep` and then selects over cancel, `cycles.finish_one()`, or the idle sleep. `TenantCycles` gains `retry_at`, `ready()`, `finish_one()`, and a `JoinSet<bool>`. New `list_tenants_owing_publication(&OperatorPool)`. `server_shutdown_result` appends "shutdown was triggered by: ...". The cluster spec gains `audit_publication_disabled` / `without_audit_publication_for_test()` and `WyrdTestCluster::await_audit_published`.
- **Resolution:** HEAD keeps Forge's loop on verification's storage. `list_tenants_owing_publication` reads `FROM vala.audit_chain_head head JOIN platform.tenants ... LEFT JOIN vala.audit_publication progress USING (data_tenant_id) WHERE head.last_seq > COALESCE(progress.published_seq, 0)`. Migration `20261003000300` grants `SELECT ON vala.audit_publication TO wyrd_platform_admin`. Then ffaa142b4 applies.
- **Why:** Work-driven publication replaces the fixed tick. Verification's split progress row is the AGENTS.md-mandated watermark plus frozen bound.
- **Replay rule:** The variant still has the 5 s tick and progress on the chain head. Take HEAD's `publication.rs`, `audit_staging.rs` (keep the variant's `pub fn entry_hash`), the migrations, `cluster.rs`, and `server.rs` wholesale. Never restore `PUBLICATION_INTERVAL` or `published_seq` on `vala.audit_chain_head`.
- **Commit:** 2880705a0 (merged in ce866229d; follow-ups ffaa142b4, 2d25fac7c, e03e3b9c8)

### L58. Carry main's OIDC and workflow-runtime merge and its repairs
- **Kind:** forge-delta-missing-from-variant
- **Files:** fe2da3e3b: about 74 hand-resolved files (Python/TS stubs, `wyrd/otel.py`, Observer removal, auth migrations). 113298433: oracle/analytical.rs, wyrd-server/src/oracle/lifecycle_controls.rs, wyrd-server/src/verification/runner.rs, and three SDK journeys.
- **Verification side (b1b8f25f7):** Already contains main@55db718bf (PRs #109 workflow-runtime and #128 OIDC production readiness).
- **Forge side (2880705a0):** fe2da3e3b merges main@55db718bf. It adopts main's OIDC auth (saved logins, renewable credentials), the Vertex fold, and the Observer removal, and moves `20261002000000_workload_role.sql` to `20261002000100`. 113298433 does three things:
  - Analytical `reclaim()` spawns the release on the current runtime (outside a runtime it retains `RECLAIM_WITHOUT_RUNTIME` residue).
  - `Box::pin` on `self.drift.verify(...)` and `self.eval.execute(...)` in `VerifierEngines`, and `cancel_while_opening(Box::pin(open), ...)`.
  - Journeys use RFC 8693 token exchange and the tagged Prompt request shape.
- **Resolution:** HEAD has all of it; ce866229d's merge base was 55db718bf, so these were not new there.
- **Why:** The variant forked at 5ab92b003, before main@55db718bf. Main's #109/#128 delta (720 files) is absent from the variant.
- **Replay rule:** Treat everything from main#109/#128 as HEAD-owned. Keep HEAD's `reclaim()`, the `Box::pin` engine calls in runner.rs (the variant's only runner.rs change, `%error` to `?error`, applies on top), and the `20261002000100_workload_role.sql` numbering. Rewrite any variant journey still using the pre-#128 token or Prompt shapes.
- **Commit:** fe2da3e3b, 113298433

### L59. Select the tenant only through WYRD_TENANT
- **Kind:** forge-delta-missing-from-variant
- **Files:** crates/shared/wyrd-client/src/{bifrost/facade.rs,cards/config.rs,cards/error.rs,cards/handle.rs,workflow/mod.rs}, sdks/wyrd-sdk-python/src/{bifrost/mod.rs,client.rs,gateway.rs,operators.rs,state/mod.rs,verification.rs}, sdks/wyrd-sdk-ts/native/src/*, all generated .pyi and index.d.ts/.d.cts, SDK tests
- **Verification side (b1b8f25f7):** `client_from_options(server_url, credential, grpc_url, tenant)`, inherited from main e73f0163e, with `tenant=` on every SDK constructor.
- **Forge side (2880705a0):** `client_from_options(server_url, credential, grpc_url)`. `cards::config::load(server_url, credential)`. No `tenant` pyo3 or TS parameter. `WYRD_TENANT`/config is the only selector.
- **Resolution:** HEAD keeps the 3-arg form and no constructor `tenant` (merge message: "WYRD_TENANT-only tenant selection merged around them").
- **Why:** A credential already names its tenant.
- **Replay rule:** The variant never had the tenant argument (5ab92b003 predates e73f0163e), and its new code already calls the 3-arg `client_from_options`. Keep HEAD's signatures and add no `tenant` parameter anywhere.
- **Commit:** 1ca7a4316

### L60. Bound snapshot-expiry authority by the lease and refuse cuts on an unknown expiry
- **Kind:** forge-delta-missing-from-variant
- **Files:** crates/vala/vala-bifrost-redux/src/{catalog/bifrost_catalog.rs,catalog/error.rs,forge/expire.rs}, crates/vala/vala-sql/migrations/20260910000025_oracle_reader_authority.sql (edited in place), crates/vala/vala-sql/src/queries/{forge_operations.rs,oracle_reader_authority.rs}, tests, docs/src/content/docs/bifrost/forge.svx
- **Verification side (b1b8f25f7):** Absent.
- **Forge side (2880705a0):** `BifrostCatalogError::UnresolvedExpiry(String)` maps to `QueryVisibilityUnavailable` (`WYRD_VALA_503_QUERY_VISIBILITY_UNAVAILABLE`). Cut acquisition maps `SqlError::Conflict { detail }` to `UnresolvedExpiry`. Expiry authority is bounded by the Forge lease TTL.
- **Resolution:** HEAD keeps it.
- **Why:** An expiry whose commit outcome is unknown must block reader cuts until reconciliation.
- **Replay rule:** Keep HEAD's hunks in `bifrost_catalog.rs`, which the variant also edits, and keep the in-place migration edit. Databases migrated from the variant need a reset because the checksum of `20260910000025` differs.
- **Commit:** 0e1968570

### L61. Linearize Forge leader handlers with the term
- **Kind:** forge-delta-missing-from-variant
- **Files:** crates/vala/vala-bifrost-redux/src/forge/{leadership.rs,metrics.rs,scheduler.rs}, tests/integration/forge/production_routes.rs, docs/src/content/docs/bifrost/forge.svx
- **Verification side (b1b8f25f7):** Absent.
- **Forge side (2880705a0):** notify, pull, and report validate the term and run under one slot read guard. Every handler decision races term replacement. The scheduler run guard revokes the term on exit and records the `scheduler_stopped` revocation.
- **Resolution:** HEAD keeps it unchanged.
- **Why:** Forge correctness under term replacement.
- **Replay rule:** Take HEAD for leadership.rs, scheduler.rs, and metrics.rs, which the variant does not touch. Merge the variant's `production_routes.rs` edits on top of HEAD's added tests.
- **Commit:** a26fda2e3, 3f591937e

### L62. Pin the compaction fork without the unconsumed planning surface (superseded)
- **Kind:** forge-delta-missing-from-variant
- **Files:** Cargo.toml, Cargo.lock
- **Verification side (b1b8f25f7):** rev 170bf8e1
- **Forge side (2880705a0):** rev 35f037e5
- **Resolution:** Superseded in HEAD by 4e9f325d (161488d9c).
- **Why:** Drops `NonCommittingCompaction` accessors and `Compaction::plan_compaction_with_report`.
- **Replay rule:** Apply the 161488d9c rule: build a combined fork commit of c04c45f + 35f037e + 4e9f325.
- **Commit:** 3a196c103

### L63. Forge packet review records and authority docs
- **Kind:** forge-delta-missing-from-variant
- **Files:** AGENTS.md, architecture/agent-rules.md, architecture/bifrost-design.md, architecture/references/languages/maintainer-style.md, changes/active/forge-concurrent-planning/**, crates/vala/vala-bifrost-redux/src/oracle/analytical_supervisor.rs (docs)
- **Verification side (b1b8f25f7):** Absent.
- **Forge side (2880705a0):** Spec revision 12, the packet-wide review of 7fcb45fc1, TASK-PACKET-R2 evidence, and corrected Analytical ownership docs.
- **Resolution:** HEAD keeps them.
- **Why:** Forge packet bookkeeping.
- **Replay rule:** Take HEAD for these. In `architecture/bifrost-design.md`, combine the variant's Variant/timestamp sections with HEAD's text, including 5ff25deb6's "Each call is bounded by the catalog request timeout" wording, which drops the "within the original deadline" phrasing.
- **Commit:** 32a9c5b64, 0aba11f67, a84f839e7


## Timing loops: audit and Forge

- **Audit 5 s polling removal:** This was done on the Forge branch, not the verification branch, in 2880705a0 "Publish audit continuously instead of on a fixed 5s tick". The verification side still had `PUBLICATION_INTERVAL = 5s` at b1b8f25f7. It reached this branch through merge ce866229d.
  - In HEAD: yes. In the variant (509f972c6): no (`PUBLICATION_INTERVAL` is still at `audit/publication.rs:45`).
  - 5 s survives only as `PUBLICATION_RETRY`, the backoff for a failed tenant.
- **Forge has no 5 s loop to remove.** `git log -S"from_secs(5)"` over `forge/` since main (55db718bf) finds no removed 5 s timer. Forge follows RisingWave's pull model:
  - The worker pulls the leader at most once per `DEFAULT_PULL_INTERVAL = 5s` (`forge/worker.rs:65`), for at most `min(max_task_parallelism - running, 4)` tasks. Both values are RisingWave's: `iceberg_compaction_pull_interval_ms = 5000` (`risingwave/src/common/src/config/storage.rs:1218`) and `MAX_PULL_TASK_COUNT = 4` (`risingwave/src/storage/src/hummock/compactor/mod.rs:412`). This is the pull period added in e04cef237. It is not a leftover loop; do not remove it.
  - Between pulls, the worker claims durable SQL work and sleeps 250 ms when idle, as it did at main.
- **What was actually removed:** the leader-side polling planner. Everything below is in the variant already; none of it needs replaying.
  - 12bef6736 dropped the scheduler's per-table candidate scan in favour of RisingWave's task-type planner.
  - 6531e5bea selects due tables from a leader-owned sorted index instead.
  - 37a526d61 runs maintenance on the leader's hourly timer and drops the durable planner. `DEFAULT_MAINTENANCE_INTERVAL` went from 60 s to 1 h (`wyrd-server/src/boot/mod.rs:78`). The timer is `scheduler.rs:328` `maintain`, and its first tick fires at once like RisingWave's GC loop. L17 records the conflict that commit caused.
- **Boot-delay removals:**
  - **c6ceefb0a "Elect the Forge leader immediately at boot":** the first leader heartbeat no longer waits one `LEADER_HEARTBEAT` (10 s) unless a test owns the trigger. In the variant: yes. In HEAD: yes. In b1b8f25f7: no.
  - **a5319f4ad:** the coordinator plans on start instead of deferring one `maintenance_interval`. In the variant, HEAD, and b1b8f25f7: yes.
  - **4ddb42cee:** the worker backs off and retries on a database read failure instead of exiting. This is not a tick change. In the variant and HEAD: yes.
- The only timing change the variant lacks is the audit publisher (2880705a0).
- **Correction:** an earlier draft of this section said Forge's "5 s loop" survived, as if it were debt. That was wrong. The 5 s value is the RisingWave pull period, by design.

## Repository standards the variant has not adopted

Commit 515f16c97 ("Close out the repository check audit, pedantic lints, and test binary layout") and commit a354e407f landed on the verification side only. The variant (509f972c6) predates both. Conflict markers will not show most of this: AUTO merges the files cleanly, and then `mise run lints`, `check:deps` or the test lanes fail. Apply these rules to every variant-origin line, not only to conflicted files.

**S1. Workspace pedantic clippy reaches 35 more crates.**
- HEAD's `[workspace.lints.clippy]` adds the following to the variant's table:
  - `unwrap_used = "warn"` and `allow_attributes_without_reason = "warn"`.
  - Reasoned allows for `doc_markdown`, `too_many_lines`, `needless_pass_by_value`, `similar_names`, `unused_self` and `cast_precision_loss`. The truncating, sign-loss and wrapping cast lints stay on.
- `mise run lints` runs with `-D warnings`.
- 35 manifests carry `[lints] workspace = true` in HEAD but not in the variant:
  - workspace-hack.
  - Every `wyrd-auth-*` crate, plus wyrd-client, wyrd-crypt, wyrd-dev-fixtures, wyrd-error-derive, wyrd-queue, wyrd-runtime, wyrd-semver, wyrd-telemetry, wyrd-tls, wyrd-utils and wyrd-version.
  - Every `skald-*` crate.
  - vala-drift, vala-eval, vala-ingest and vala-sql.
  - wyrd-spec, wyrd-cli, wyrd-mcp, wyrd-server, wyrd-sql, wyrd-testing, wyrd-tonic, wyrd and examples/rust.
- Variant code added in those crates has never been linted as pedantic.
- **Replay rule:** keep HEAD's lint table and every `[lints] workspace = true` line. Fix each finding at the site: no blanket `#[allow]`, and any `#[expect]` or `#[allow]` states a `reason`. L05, L20 and L38 show the recurring shapes:
  - `r"..."` instead of `r#"..."#`.
  - `Err(A | B)` patterns.
  - `map_or` / `map_or_else`.
  - `Box::pin` on large futures.
  - `if let .. else` instead of a single-arm `match`.
  - Checked float-to-int conversions.
  - No `unwrap()` outside tests.
- Tests read configuration through `wyrd_client::Environment` and do not mutate the process environment.

**S2. Repository checks went from 42 to 9.** The following scripts are deleted, along with their `mise` tasks:
- `scripts/check_clippy_allow.py`, `check_unwrap_audit.py`, `check_test_contracts.py` and `add_justification.py`.
- `scripts/checks/{bifrost-resource-governance,client-tier,error-coverage,fixtures-no-server,forbid,from-pools-allowlist,mocks-scope,no-legacy-server-vocab,no-testing-in-prod-deps,no-tonic-outside-wyrd-tonic,object-store-pin,proto-drift,pyo3-scope,rustls-provider,single-into-response-impl,test-coverage}.sh`.
- `scripts/sync-agent-skills.sh` and `scripts/test-families.sh`.
- The retired tasks include:
  - `check:client-tier` and its CLI, registry and SDK variants.
  - `check:pyo3-scope` and `check:sdk-pyo3-scope`.
  - `check:unwrap-audit`, `check:clippy-allow-audit` and `check:test-contracts`.
  - `check:mocks-scope`, `check:from-pools-allowlist` and `check:object-store-pin`.
  - `check:proto-drift`, `check:rustls-provider` and `check:single-into-response-impl`.
  - `check:error-coverage` and `check:test-coverage`.
  - The `check:registry-*` and `check:security-*` tasks.
  - `check:tokens`, `check:skills-sync` and `skills:sync`.
  - `check:default` and `lints:default`.
- Clippy, the compiler or Postgres readiness now enforce those rules. `check:deps` owns crate boundaries and the object-store pin. `check:tenant-isolation` keeps only the TenantConn code-shape guards.
- **Replay rule:** resolve every modify/delete on these files as delete (L07). If a variant mise task or CI step calls one, delete the call. Do not restore the script.

**S3. One integration test binary per crate.**
- vala-sql, wyrd-server, wyrd-sql, wyrd-storage and wyrd-client no longer have `[[test]]` blocks. Their `tests/*.rs` targets fold into `tests/integration/main.rs` as `mod` lines. The variant still declares 8, 8, 3, 4 and 1 `[[test]]` blocks.
- Lanes select modules with `--test integration -E 'test(/^<module>::/)'`.
- Every PgFixture-backed `pg_*` module shares the `pg-servers` nextest group in `.config/nextest.toml`.
- `PgFixture::superuser_pool()` is synchronous.
- **Replay rule:** move each new variant test file under `tests/integration/` and add its `mod` line in alphabetical order. Remove its `[[test]]` block, drop `.await` on `superuser_pool()`, and point any variant mise lane at `--test integration` with a module filter (L03).

**S4. Bins and profile.**
- The three capacity bins are one `[[bin]] capacity` (L25). `bench:bifrost:ingest-capacity`, `bench:bifrost:query-capacity` and `bench:verification:capacity` are gone.
- `[profile.dev] debug = "line-tables-only"` replaces the variant's `[profile.dev.package."*"]` form.
- The checked-in proto descriptor stays deleted (L08).

## Predicted overlap with bifrost-variant


The two branches' common ancestor is 5ab92b003. 104 files are changed on both sides (5ab92b003..509f972c6 and 5ab92b003..HEAD).

**Audit publication and Variant `detail` (2880705a0, ffaa142b4)**
- crates/vala/vala-sql/src/queries/audit_staging.rs: take HEAD, keep the variant's `pub fn entry_hash`.
- crates/vala/vala-bifrost-redux/src/tables/audit/projection.rs: the variant's Variant `detail` column goes on top of HEAD.
- crates/wyrd/wyrd-testing/src/server.rs: the variant's `audit_cell_text` Variant decode, plus HEAD's three-control surface.
- crates/wyrd/wyrd-testing/tests/bifrost/server/verification_runtime.rs (variant +2019 lines), eval_verification.rs, query.rs: keep HEAD's audit-publication knobs and outbox semantics.

**Audit-table isolation knob (2d25fac7c, e03e3b9c8)**
- crates/wyrd/wyrd-testing/tests/bifrost/oracle/published.rs (variant +439)
- crates/wyrd/wyrd-testing/tests/bifrost/oracle/{analytical_activation,distributed,peer_cluster,support}.rs

**Compaction pin and dependency graph (161488d9c, 3a196c103)**
- Cargo.toml, Cargo.lock, crates/shared/workspace-hack/Cargo.toml
- The variant moves to Arrow/Parquet 60 forks, so expect conflicts in every crate Cargo.toml: crates/vala/vala-bifrost-redux, crates/wyrd/{wyrd-server,wyrd-testing,wyrd-cli}, crates/shared/{wyrd-client,wyrd-queue}, sdks/{wyrd-sdk-python,wyrd-sdk-ts/native}.
- scripts/checks/object-store-pin.sh: HEAD deleted it and the variant modified it. Keep it deleted; check:deps owns the pin.

**Oracle reclaim and bind params (113298433, b83f34bd8, b620b069f, 8d98c3ceb)**
- crates/vala/vala-bifrost-redux/src/oracle/{analytical.rs,mod.rs,codec.rs,follower.rs,peer.rs,query_stream.rs}
- In oracle/mod.rs, keep HEAD's `plan_physical(session, context, sql, params)`, `bind_values`, and `with_param_values` alongside the variant's `map_query_planning_error` rewrite. An unbound placeholder must still be refused as invalid SQL.

**Forge (0e1968570, 4ddb42cee, 5ff25deb6)**
- crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs
- crates/vala/vala-bifrost-redux/src/forge/worker.rs: the variant's `inspect_err` warn in `rewrite_plan` goes on top of HEAD's deadline-free `RewritePublication`.
- crates/vala/vala-bifrost-redux/tests/integration/forge/{production_routes.rs,support.rs}

**Tenant selection and client errors (1ca7a4316, ac327a227, 6e84727c9)**
- crates/shared/wyrd-client/src/{error.rs (variant +519), bifrost/facade.rs, bifrost/handle.rs, bifrost/grpc.rs, bifrost/mod.rs, bifrost/query.rs, observe/drift.rs, observe/tests.rs}
- crates/shared/wyrd-queue/src/{batch_builder.rs,producer.rs,queue.rs,error.rs}: keep HEAD's `Row.event_time_micros: Option<i64>`, the stamp-change sealing, and `insert_observation`. Merge them with the variant's timestamp kinds (`TimestampKind`/`parse_tz`).
- crates/shared/wyrd-queue/src/schema.rs: the variant deleted it and moved it to wyrd_types.

**TASK-016/017 public surfaces (41065b39b, bd9f0b2c3, d7524b19b)**
- Python: sdks/wyrd-sdk-python/python/wyrd/bifrost/{__init__.py,__init__.pyi}, stubs/bifrost.pyi, src/bifrost/mod.rs.
- TypeScript: sdks/wyrd-sdk-ts/{native/src/lib.rs, wyrd/src/index.ts, wyrd/src/error-codes.ts, wyrd/index.d.ts, wyrd/index.d.cts}, wyrd/tests/unit/bifrost-query.test.ts.
- crates/wyrd/wyrd-tonic/{proto/wyrd.v1.proto,src/query_conversion.rs}
- proto/wyrd.v1.bin: HEAD deleted it. Do not restore it.
- crates/wyrd-spec/src/{vala/api.rs,vala/error.rs,error.rs,vala/assignment_authority.rs}
- crates/wyrd/wyrd-server/src/{query/routes.rs,query/service.rs,query/scheduled.rs,bifrost/routes.rs,bifrost/service.rs,mcp/bifrost.rs}
- mise.toml: keep HEAD's Python Bifrost lane story list (fixed in `abfc6f580`). Add the variant's new TS files to HEAD's renamed TS lane list (`query-bifrost`, `observe-a-run`, `verify-in-real-time`, `scheduled-drift-alerts-operator`).

**TASK-017 deleted or renamed tests the variant modified (modify/delete conflicts)**
- crates/shared/wyrd-client/tests/pg_bifrost_e2e.rs, now tests/integration/pg_bifrost_e2e.rs
- sdks/wyrd-sdk-python/tests/integration/{test_bifrost_query.py,state/test_observe_journey.py,test_drift_journey.py}
- sdks/wyrd-sdk-rust/tests/{drift_verification.rs,observe_run.rs}
- sdks/wyrd-sdk-ts/wyrd/tests/integration/{oracle-query,drift-verification,observe-run}.test.ts
- Apply the TASK-017 rule: keep HEAD's public-story files and move the variant's Variant/Iceberg assertions into Rust owner tests or the variant's new `tests/integration/bifrost/*` stories. Do not move them back into the deleted files.

**Other overlaps**
- crates/vala/vala-bifrost-redux/src/tables/{mod.rs,managed_columns.rs,signal.rs}, tables/dev/agent_traces.rs (HEAD deleted it), scribe/{execution_lanes.rs,ingress.rs}, gate/error.rs, resources.rs: this is the eager built-in tables and managed-columns topic of the ce866229d conflict set. Keep HEAD's `builtin_tables()` eager-ensure calls.
- crates/wyrd/wyrd-server/src/verification/{runner.rs,drift.rs,eval.rs,results.rs}, components/gateway/capture.rs: 113298433 `Box::pin` plus the variant's `UTC_TIME_ZONE` and Variant edits.
- crates/wyrd/wyrd-testing/tests/bifrost/otlp/{support.rs,trace_export.rs}, tests/gateway/peer.rs: OTLP and SHA-256 credentials stay as in HEAD.
- crates/wyrd/wyrd-cli/{src/query/mod.rs,tests/query_server_journey.rs}, crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs: `sql(query, params)`.
- docs/src/content/docs/bifrost/schema.svx, architecture/bifrost-design.md
- .agents/skills/wyrd-task-review/SKILL.md, .claude/skills/wyrd-task-review/SKILL.md: HEAD made `.claude/skills` a symlink, so keep it deleted and apply the variant's skill edits to `.agents/skills` only.
