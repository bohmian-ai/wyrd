# Repository Standards Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-forge`
- Base: `c1508b375`
- Candidate: `7ac45dec99535c881b7a936c66623044f15d8823`
- Range: `c1508b375..7ac45dec9`
- Candidate identity was re-read during this review and remained unchanged.
- Scope: repository standards only. Task acceptance is intentionally not
  adjudicated here.

## Authority coverage

| Changed surface | Governing authority read and applied | Coverage |
|---|---|---|
| Forge leader, scheduler, worker, promotion, maintenance, cleanup, Oracle active reads, Scribe publication, DataFusion/Iceberg pruning | `AGENTS.md` §§4–6, 9–11, 15–16; `architecture/agent-rules.md`; `architecture/bifrost-design.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/rust-core.md`; `architecture/references/domain/{vala-architecture,olap-serving,iceberg,datafusion,analytical-operations-reliability}.md` | Complete for repository standards |
| SQL queries and migrations, RLS, leader singleton, active-read authority, catalog pointer | `AGENTS.md` §§9, 15; `architecture/agent-rules.md`; `architecture/wyrd-design.md` §Coordination clock; `architecture/wyrd-security-posture.md` §§Peer identity, Tenant and data isolation; `architecture/references/architecture/patterns.md` §Storage And Registry Pattern | Complete |
| Rust public/private contracts, protobuf conversion, server gRPC peer adapter | `AGENTS.md` §§4–6, 9, 16; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/references/languages/{rust-core,maintainer-style,errors}.md` | Complete |
| Python/PyO3 package, exports, docstrings, generated stubs and tests | `AGENTS.md` §§7–8, 11, 16; `architecture/references/languages/{pyo3-boundaries,python-api-and-stubs,testing-workflows}.md` | Complete |
| TypeScript/N-API source, declarations and tests | `AGENTS.md` §§2–3, 11, 16; `architecture/references/languages/{typescript-guide,testing-workflows}.md` | Complete |
| Arrow/Parquet schemas, managed columns, filtering and physical projections | `architecture/bifrost-design.md`; `architecture/references/domain/{arrow-analytical-interop,telemetry-observations,iceberg,datafusion}.md`; `architecture/agent-rules.md` field-by-name rule | Complete |
| Rust/Python/TypeScript journeys, SQL/integration tests, capacity harness, task evidence and verification commands | `AGENTS.md` §§11–12, 16; `architecture/agent-rules.md`; `architecture/references/languages/{spec-driven-development,implementation-execution,testing-workflows,maintainer-style}.md` | Complete |
| Cargo manifests/lockfile, `mise.toml`, generated JSON/protobuf/declarations and docs site | `AGENTS.md` §§1, 4, 8, 11–12, 15; `architecture/agent-rules.md`; `architecture/references/README.md` | Complete |

## Rule results

| Repository rule | Evidence | Result |
|---|---|---|
| Candidate is immutable and review covers the complete range | `git rev-parse HEAD` returned `7ac45dec99535c881b7a936c66623044f15d8823`; diff inventory covers 283 changed files | PASS |
| Struct-centered Rust ownership | New stateful workflows are owned by `ForgeSchedule`, `ForgeLeadership`, `ForgeWorker`, `TableAuthority`, `ScribeStagingRuntime`, capacity/deployment owners, and the existing Oracle/Scribe/Forge owners; new free functions reviewed are conversions or deterministic helpers | PASS |
| Async only at IO/composition boundaries | Changed async workflows await SQL, catalog/object storage, gRPC, process, stream, or task lifecycle IO. The three no-`await` Forge peer methods are implementations of tonic's generated async service trait, not voluntarily async helpers | PASS |
| Rustdoc on every materially changed item; `# Errors`, `# Panics`, cancellation/partial progress where applicable | Multiple changed production and test items fail the explicit hard rule; see `STD-001` | **FAIL** |
| Module-scope imports and bare type names in signatures | Multiple changed sites add function-scoped imports and fully qualified signature/field types; see `STD-002` | **FAIL** |
| No unjustified `#[allow]`; no unsafe non-test `unwrap`/`expect` | Re-run `mise run check:clippy-allow-audit` and `mise run check:unwrap-audit`: both passed. Added `#[ignore]` attributes are external-service/Postgres journey gates with explicit reasons and owning `mise` lanes | PASS |
| `TenantConn`/`OperatorPool` boundaries, caller-owned tenant transactions, RLS, no widened tenant query | Re-run `mise run check:tenant-isolation`: passed. New tenant paths use `TenantConn`; singleton/cross-tenant coordination uses `OperatorPool`; no changed tenant callee commits or rolls back its `TenantConn` | PASS |
| PostgreSQL owns database coordination time | Leader terms, task claims, active-read abandonment, ingest stamps and SQL eligibility use `statement_timestamp()`/`clock_timestamp()`. Host `Instant` additions are process-local cadence/measurement; `Utc::now()` additions outside tests are local staging/capacity inputs rather than database eligibility | PASS |
| Test changes require traced diagnosis and a fresh diagnostician for concurrency/timing/test edits | The R1 task's `Diagnoses` section records symptom, evidence, cause and fix site for the `05cceaf35`, `484c3b4f6`, and `3a51a24d5` edits. The removed sibling-route assertion is explained as stale under immediate eligibility; held-reader assertions were strengthened; host-clock brackets were replaced with PostgreSQL time; the age floor was fixed at the shared `MaintenanceProtection::eligibility` owner after both consumers were checked | PASS |
| Python tests are top-level functions and import public projections | No added `class Test*` or nested `def test_*` in changed Python tests; changed tests import `wyrd`/`wyrd.bifrost` public surfaces | PASS |
| PyO3 remains at approved boundary; `wyrd-spec` is PyO3/IO/async free | Changed wrappers remain in `sdks/wyrd-sdk-python` or existing approved owner-crate Python modules; no PyO3 addition to `wyrd-spec` | PASS |
| Generated schema/stub/declaration parity | Task evidence records passing `mise run codegen:check`, Python typecheck, TypeScript typecheck/tests, tonic tests and proto drift; generated outputs correspond to changed source contracts | PASS (recorded evidence) |
| No compatibility routes, task/agent prose in production, or new legacy Wyrd vocabulary | Diff scan found no production task IDs, agent notes, compatibility route, or removed Card-kind vocabulary | PASS |
| Required verification scope and clean diff | The packet is intentionally broad and changes shared Cargo/mise/test infrastructure, contracts, server, SQL, three SDKs, docs and generated artifacts, but has no recorded `mise run gate`; current `git diff --check c1508b375..7ac45dec9` also fails. See `STD-003` and `STD-004` | **FAIL** |

## Material findings

### STD-001 — Missing required Rust documentation is a hard merge blocker

- **Rule:** `AGENTS.md` §16 and `architecture/agent-rules.md` require
  substantive rustdoc on every new or materially modified Rust item, including
  private items and tests; every fallible function needs `# Errors`, every
  possible panic needs `# Panics`, and async/durable operations document
  cancellation or partial progress where relevant.
- **Locations and evidence:**
  - `crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs:827`
    materially changes `ensure_builtin`, which returns
    `Result<TableUid, BifrostCatalogError>` but has no `# Errors` section.
  - `crates/vala/vala-bifrost-redux/src/catalog/wire.rs:15`
    adds/changes `reject_reserved_field_names`, a fallible public function with
    no `# Errors` section.
  - `crates/vala/vala-bifrost-redux/src/forge/expire.rs:198`
    materially changes the durable async
    `run_snapshot_expiry_for_table_inner`; its rustdoc has neither `# Errors`
    nor cancellation/partial-progress behavior despite SQL, catalog and
    object-store effects.
  - `crates/vala/vala-bifrost-redux/src/forge/orphan_gc.rs:1002`
    materially changes `load_maintenance_protection_inner` without any method
    rustdoc or `# Errors` section.
  - `crates/vala/vala-bifrost-redux/src/oracle/query_stream.rs:401`
    materially changes `build_frames` without rustdoc describing its terminal,
    cancellation and active-read-release role.
  - `crates/wyrd/wyrd-server/src/grpc/forge_peer.rs:91`, `:108`, and `:131`
    add three fallible tonic service methods but omit `# Errors` sections.
  - `crates/vala/vala-bifrost-redux/src/scribe/staging_runtime.rs:176` adds the
    `DrivenClaim` associated `Target` type without its required item rustdoc.
  - `crates/vala/vala-bifrost-redux/tests/integration/forge/expired_cleanup.rs:501`
    materially renames and rewrites a test with no rustdoc and no `# Panics`
    section; `crates/vala/vala-bifrost-redux/src/forge/leader.rs:718` documents
    `key` but omits `# Panics` even though both constructors use `expect`.
  - Materially changed constants at
    `crates/wyrd/wyrd-server/src/boot/mod.rs:65`,
    `crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs:2277`, and
    `crates/vala/vala-bifrost-redux/src/scribe/hot_stage.rs:1535` have no item
    rustdoc.
- **Consequence:** Maintainers cannot recover the failure, cancellation,
  partial-progress and panic contracts from the changed owners, and the
  candidate directly violates a rule classified by the repository as
  `BLOCK_BEFORE_MERGE`; green Clippy does not enforce private/test-item
  completeness.
- **Testable correction:** Add substantive rustdoc at every new or materially
  changed item in the full range, including the examples above; add precise
  `# Errors`, `# Panics`, and cancellation/partial-progress sections where the
  signature/body requires them. Re-audit the complete changed-symbol set rather
  than only public exports, then run format, lints, and the owning focused
  tests.

### STD-002 — Changed Rust hides dependencies inside functions and signatures

- **Rule:** `architecture/agent-rules.md` requires all `use` declarations at
  module scope (apart from a test module's own top-level imports and the narrow
  single-generic-function `Trait as _` exception) and requires imported bare
  names in signatures, fields, trait bounds and `where` clauses.
- **Locations and evidence:**
  - `crates/vala/vala-bifrost-redux/src/forge/settings.rs:98-99` uses the fully
    qualified `wyrd_spec::vala::api::CompactionTypeWire` in the `From` impl and
    parameter, then imports it inside `fn from`.
  - `crates/vala/vala-bifrost-redux/src/oracle/exec.rs:3184-3188` uses fully
    qualified Parquet and Wyrd types in `BloomProbe::for_column` and imports
    `ConvertedType`, `LogicalType`, `PhysicalType`, and `ScanLiteral` inside the
    function. The same changed module adds further function-scoped imports at
    `:4758`, `:4826`, `:4890`, `:4967`, `:5054`, and `:5081-5082`.
  - `crates/vala/vala-bifrost-redux/src/forge/leader.rs:1012` imports `rand`
    inside a test function instead of at the `tests` module's dependency block.
  - `crates/vala/vala-bifrost-redux/src/scribe/staging_runtime.rs:2251` and
    `crates/vala/vala-bifrost-redux/src/scribe/persistence.rs:2190-2192` add
    function-scoped imports.
  - `crates/wyrd/wyrd-testing/tests/bifrost/oracle/distributed.rs:2747`,
    `:2802`, `:2848`, `:2884`, `:2907`, and `:3955` add function-scoped
    imports in the external journey module.
  - Changed signatures/fields also retain or add dependency-hiding qualified
    names, for example
    `crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs:264` and
    `:1814`, `crates/vala/vala-bifrost-redux/src/forge/gc.rs:597` and `:633`,
    and `crates/vala/vala-bifrost-redux/src/oracle/exec.rs:2144-2145`.
- **Consequence:** The top-of-module dependency manifest is incomplete, and
  owning types/signatures obscure which crate/module supplies their contract;
  this is the exact maintainability failure the explicit repository rule bans.
- **Testable correction:** Move the imports to each owning module or test
  module's top-level import block and use bare names throughout changed
  signatures, fields, trait bounds and `where` clauses. Preserve only a
  genuinely qualifying local `Trait as _` import with the required narrow
  justification. Run format and lints afterward.

### STD-003 — The immutable candidate fails the required whitespace gate

- **Rule:** `AGENTS.md` §12 requires a clean `git diff --check` before
  completion.
- **Evidence:** A fresh
  `git diff --check c1508b375..7ac45dec9` exits nonzero with:
  - `changes/active/forge-concurrent-planning/revision/TASK-005-R1-implementation-reference.md:464: new blank line at EOF.`
  - `changes/active/forge-concurrent-planning/tasks/TASK-003-maintenance-and-removal.md:307: new blank line at EOF.`
- **Consequence:** The recorded claim that the final cumulative candidate is
  whitespace-clean is stale and the required completion gate is red.
- **Testable correction:** Remove the extra EOF blank lines and rerun
  `git diff --check c1508b375..<new-candidate>`.

### STD-004 — The required broad aggregate has not been evidenced

- **Rule:** `AGENTS.md` §11 requires `mise run gate` when a change is
  intentionally broad, crosses several ownership boundaries without one
  complete capability gate, or changes shared CI/build/test infrastructure.
- **Evidence:** This range changes 283 files across Cargo and lock state,
  `mise.toml`, Wyrd contracts, SQL/migrations, Vala engines, server boot and
  gRPC, Python/PyO3, TypeScript/N-API, protobuf/schema artifacts, docs and
  multiple journey harnesses. Recorded evidence includes strong scoped lanes
  (`verify:bifrost`, principals integration, codegen, docs, Python and
  TypeScript checks in the task files), but no `mise run gate` result. The
  aggregate additionally owns repository-wide inventory, boundary, docs,
  generated, unit/integration and packaging checks not all established by the
  one reported candidate-wide command set.
- **Consequence:** There is no repository-authorized final proof that the
  broad cumulative packet composes outside its individually exercised slices.
- **Testable correction:** After the bounded source corrections, run
  `mise run gate` on the new immutable candidate and record its result. Do not
  replace a red component with a narrower lane.

## Verification notes

- Re-run during this review:
  - `mise run check:unwrap-audit` — PASS.
  - `mise run check:clippy-allow-audit` — PASS.
  - `mise run check:tenant-isolation` — PASS.
  - `git diff --check c1508b375..7ac45dec9` — FAIL as `STD-003` records.
  - Static diff scans for added `#[allow]`, Python `class Test*`, raw SQL pool
    propagation, host-clock coordination, function-scoped imports, qualified
    signature types, and changed-item rustdoc coverage.
- Accepted as recorded evidence, not independently rerun here: candidate-wide
  `mise run verify:bifrost`, `mise run test:principals:integration`, `mise run
  fmt`, `mise run lints`; task-recorded codegen, docs, Python and TypeScript
  lanes.
- A full `mise run gate` is missing and required by the repository's own scope
  rule (`STD-004`).

## Overall result

**FAIL**

The candidate violates hard Rust documentation and import/signature rules,
fails `git diff --check`, and lacks the required broad aggregate proof. These
are repository-standard findings only; no task-acceptance conclusion is
included.
