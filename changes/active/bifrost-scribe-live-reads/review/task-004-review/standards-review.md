# TASK-004 repository-standards review (repo-rev)

- Subject: `a56ab7569..990803fc0` (67 commits, 632 files incl. packet; 434 code files outside `changes/`).
- Role: repository-standards specialist only. No task-acceptance or Ponytail audit.
- Method: static. Read-only `git`/`rg`/Python scans over the candidate (`git show 990803fc0:<path>`). One
  read-only re-run of `scripts/check_tenant_isolation.py` from a scratchpad copy with the D19 exemption removed.
  No mise lanes, builds, Postgres, or benchmarks were run. Rules that only the gate can prove are marked
  **GATE** and rely on the orchestrator's `mise run gate` (`verification.md`, not yet present when this was written).
- Coverage limits: the rustdoc, `# Errors`, unwrap, `#[allow]`, function-scoped-`use`, and free-function
  scans are line-level heuristics over every added line in changed `.rs` files. I verified each hit by hand.
  Struct-field and variant doc coverage is approximate. Async-without-await was not checked exhaustively.

## Authority coverage

| Authority | Read | Applied to |
|---|---|---|
| `AGENTS.md` §1-§16 | full | all surfaces |
| `architecture/agent-rules.md` | full | Rust, SQL, tests, checks |
| `architecture/references/README.md` (router) | full | routing |
| `TESTING.md` | "Writing a test" and lane sections | Rust/Python/TS tests |
| `references/languages/testing-workflows.md` | skimmed for env/skip rules | tests |
| `references/languages/{rust-core,maintainer-style,errors,python-api-and-stubs,typescript-guide}.md` | applied through the AGENTS/agent-rules rules they derive from; not re-read line by line | Rust, errors, SDK |
| `architecture/v1/00-foundations/sql-foundation.md` | D19 diff | wyrd-sql |

## Changed-surface map

| Surface | Representative paths | Rules applied |
|---|---|---|
| Rust server/engine crates | `vala-bifrost-redux/src/**`, `wyrd-server/src/**`, `wyrd-storage`, `wyrd-auth`, `wyrd-tonic`, `vala-eval` | §4, §5, §6, §16; agent-rules (use, FQ paths, docs, struct style) |
| SQL layer | `wyrd-sql/src/{tenant_conn,lib,schema_check,dsn,error}.rs`, `vala-sql` | agent-rules TenantConn/OperatorPool, PG timestamps, pg test placement |
| Contracts and generated files | `wyrd-spec/src/vala/{error,api}.rs`, `wyrd-spec/schemas/*`, `wyrd-client/schemas/*`, `sdks/wyrd-sdk-ts/wyrd/{index.d.ts,src/error-codes.ts}` | WyrdError derive; no hand edits (GATE codegen:check) |
| SDKs | python `tests/unit/client/test_client.py`, examples; TS `src/index.ts`, unit test | Python top-level tests; PyO3 scope; client tier |
| Tests | `wyrd-testing/tests/**`, `wyrd-client/tests/startup_image_journey.rs`, redux `tests/integration/**`, inline tests | `#[ignore]` lanes, rustdoc on tests, plan names, assert rule |
| Checks/scripts | `scripts/check_tenant_isolation.py`, `scripts/checks/from-pools-allowlist.sh`, `scripts/postgres/*`, `scripts/server/*`, `scripts/test-families.sh` | §12 gate circumvention and check add/retire |
| mise.toml | `lints`, `test:server:{startup,peer}`, removed `check:bifrost-oracle-deploy`, `dev:pg:nuke` | §11/§12 |
| Workspace manifests | root `Cargo.toml` profiles, `mimalloc`, iceberg revs; crate manifests | §4 deps/profiles; client tier |
| Deploy/docs | `deploy/kubernetes/kind/*` (new), `deploy/kubernetes/bifrost/*` (deleted), `docs/src/content/docs/**` | GATE docs:check; check retirement |
| Git history | 67 commits | §13 |

## Per-rule results

| # | Rule (source) | Result | Evidence |
|---|---|---|---|
| R1 | Git identity Thorrester <sjforrester32@gmail.com>, no AI trailers (§13) | PASS | `git log --format='%an <%ae> \| %cn <%ce>'`: all 67 commits are author and committer `Thorrester <sjforrester32@gmail.com>`. Bodies have 0 matches for `co-authored\|claude\|anthropic\|session_`. |
| R2 | No `#[allow]` added (§12, agent-rules clippy) | PASS | No added `#[allow(`/`#![allow(`/`#[expect(` line in the code diff. |
| R3 | `#[ignore]` only for sanctioned gated lanes (§11, §12) | PASS | New `#[ignore = ...]` sites are run by their owners: `startup_image_journey.rs` via `scripts/server/test-startup.sh:98` and `test-kind-autoscale.sh:84` (`--run-ignored=only`); `forge/live_rewrite.rs` and `oracle/capacity.rs` by `--run-ignored=all` journey lanes. None hides a failing test. |
| R4 | No weakened or broadened checks; only sanctioned per-file mechanism, test-only (§12, agent-rules) | **FAIL** | The `check_tenant_isolation.py` exemption is dead and unsanctioned (S1). `from-pools-allowlist.sh` `python/`→`sdks/` repoints a stale root, which strengthens the check (PASS). `check_migration_drift` pattern swap follows the role-model change (migration runs as owner). It still asserts a BYPASSRLS/superuser migration login (PASS). |
| R5 | Check retirement states which check, property, and why unreachable (§12) | PASS | `check:bifrost-oracle-deploy` was removed with `deploy/kubernetes/bifrost/`. The rationale is recorded in `changes/active/verified-change-contract/review/TASK-006-r5/...:160`. `wyrd-auth-check` left `test-families.sh` with its crate. `test-roles.sh` embedded-Postgres steps left with the embedded boot. |
| R6 | No `unwrap()` in non-test IO; `expect` only for named invariants (§4) | PASS | No added non-test `.unwrap()`. Seven added `.expect(` lines all name an invariant, e.g. `sampling.rs:64` "a SHA-256 digest has 32 bytes". The `ingress.rs:650` expect is test-support and documents `# Panics`. |
| R7 | Public errors use `WyrdError` derive; no hand-written code/status (§4, §9) | PASS | `wyrd-spec/src/vala/error.rs:98-106` `QueryResourcesExhausted` with `#[wyrd_error(code="WYRD_VALA_503_QUERY_RESOURCES_EXHAUSTED", status=503, ...)]`. `wyrd-sql/src/error.rs:203-216` `SchemaNotReady` is derive-registered. The client maps the code in `wyrd-client/src/error.rs:285`. |
| R8 | Rustdoc on every new or materially modified item, with `# Errors`/`# Panics` (§16, agent-rules) | **FAIL** | Most new items are documented, including tests with `# Panics`. Gaps are listed in S5. |
| R9 | Struct-centered style; free functions only for stateless helpers (§5, agent-rules) | **FAIL** | New IO workflow free functions thread owner state (S3). The decode helpers in `verification/eval.rs` and `wait_for_peer_capacity` are stateless or narrow (PASS). |
| R10 | `use` at module top; bare names in signatures (agent-rules) | **FAIL** | Function-scoped imports and fully-qualified signature types in new code (S4). |
| R11 | Pg-dependent tests in `mod pg_tests`/`pg_*`, never the fast lane; a test must assert (agent-rules, TESTING.md) | **FAIL** | D19's new Postgres test is in `mod tests` and returns green without Postgres (S2). |
| R12 | No plan/task references in the codebase (agent-rules; TESTING.md "must not name a plan") | **FAIL** | `scripts/server/test-kind-autoscale.sh:4` (S6). The added Rust and test diff has no `TASK-`/`D1x`/`R13` identifiers. |
| R13 | Raw `PgPool` only at boundary; TenantConn callees never commit (agent-rules) | PASS (note) | `TenantConn::acquire(&PgPool)` is the existing boundary owner. The new private `begin_bound(&PgPool, String)` stays inside it (see S3c on shape). `query_audit.rs` commits at its own boundary over `ValaPostgres::tenant_conn`. |
| R14 | PostgreSQL owns coordination timestamps (§15) | PASS | The 21 added `Utc::now()` sites are peer wire-token expiry, in-memory reservations (`dispatcher.rs` `PENDING_TTL`), deadline conversion (`oracle/mod.rs:4340`), or tests. None binds a coordination deadline into SQL. |
| R15 | Client-tier dependency rules (§2, §4) | PASS | `wyrd-client/Cargo.toml` adds only the dev-dependency `wyrd-tonic` (`client`,`server`). No `sqlx`/cloud/`datafusion`/`deltalake` enters a client-tier normal dependency. GATE `check:client-tier`. |
| R16 | PyO3 scope; `wyrd-spec` PyO3-free (§7) | PASS | No added `pyo3`/`PyErr`/`Bound<'py` lines; `wyrd-spec/Cargo.toml` unchanged. GATE `check:pyo3-scope`. |
| R17 | Generated stubs/schemas not hand-edited (§8, agent-rules) | GATE | The changed schemas track their sources: `transport_config_http.json` default `8080` and removed `tls` match `wyrd-client/src/transport/config.rs:141`. `error-codes.ts` carries its generator header and the new code. `index.d.ts` is NAPI-generated. No `.pyi` changed. Proof is `codegen:check` in the gate. |
| R18 | Python tests top-level `def test_*` (§16) | PASS | Added tests in `sdks/wyrd-sdk-python/tests/unit/client/test_client.py` are top-level; no added `class Test`. |
| R19 | No legacy names / compat aliases (§2, §12) | PASS | No added `opsml_`/`scouter_`. `ExecutionPath`→`QueryClass` and `IngestOversized` were removed outright, with no alias. |
| R20 | `anyhow` only in binaries; `thiserror` in libs (§4) | PASS | No added `anyhow` usage. |
| R21 | No per-crate profile blocks; no wildcard versions (§4) | PASS | `[profile.release]`/`[profile.dist]` are workspace-root profiles. Added deps are pinned (`mimalloc = "0.1.50"`, iceberg git revs). |
| R22 | Cargo features earned (agent-rules) | PASS | No new crate features. `wyrd-sql`'s `embedded-postgres` use was removed. `sqlx_catalog` gains a TLS backend for a documented need. |
| R23 | Format, lints, docs, codegen, unwrap/clippy-allow audits | GATE | `mise.toml` `lints` now also lints the shipped `wyrd-server` binary at release features (stricter). Results depend on the orchestrator gate. |

## Findings

### S1 — VIOLATION: dead, non-test exemption added to the tenant-isolation check

- Violated: AGENTS.md §12 ("Do not circumvent a gate … Only use a check's own sanctioned mechanism … when the
  usage is legitimately test-only and matches an existing in-pattern precedent"). Also §12 "Adding And Retiring
  Checks": a check change must protect a reachable property.
- Location: `scripts/check_tenant_isolation.py:24-27` (`TRANSACTION_CONTROL_OWNER`) and `:423-424` (the
  `raw transaction control` skip), added in 990803fc0 (D19).
- Evidence:
  - The forbidden pattern is `\bBEGIN;|\bCOMMIT;|\bSAVEPOINT\s+` (`:417`).
  - At the candidate, `crates/wyrd/wyrd-sql/src/tenant_conn.rs` has no match: the statement is
    `"SELECT set_config(...); BEGIN"` with no semicolon after `BEGIN`.
  - I copied the script with the two skip lines removed and ran it against this tree. It printed
    `tenant isolation check passed` (rc=0).
  - So the exemption excuses nothing today. It is also not an existing per-file mechanism (the file's only
    allowlist is `PLATFORM_QUERY_ALLOWLIST`, for a different rule), and it covers production code, not test code.
- Consequence: the one file that owns tenant transactions is now permanently exempt from raw transaction-control
  detection. A later `COMMIT;` or `SAVEPOINT x` added to `TenantConn` passes the gate silently. The check also
  gains a special case that protects nothing.
- Correction: delete `TRANSACTION_CONTROL_OWNER` and the `if label == "raw transaction control" …: continue`
  branch. Then `python3 scripts/check_tenant_isolation.py` (or `mise run check:tenant-isolation`) still passes
  on the candidate.

### S2 — VIOLATION: Postgres test in the fast-lane `mod tests` passes green without Postgres

- Violated:
  - agent-rules: "Tests needing Postgres, Docker, or a live server go in `mod pg_tests` (or a `pg_*` file),
    never the fast lane."
  - TESTING.md "A test must assert".
  - AGENTS §11: a selection must not pass without exercising the test.
- Location: `crates/wyrd/wyrd-sql/src/tenant_conn.rs:164-199`
  (`tests::failed_bind_rolls_back_and_the_connection_stays_usable`), inside `mod tests` (`:121`).
- Evidence:
  - The test starts `let Ok(url) = std::env::var("WYRD_DATABASE_URL") else { return; };`.
  - `wyrd-sql` is in `FAMILY_WYRD` (`scripts/test-families.sh:17`), whose lane runs
    `cargo nextest run -p wyrd-sql` with no Postgres.
  - In that lane the test reports PASS with zero assertions executed. This test is the only automated proof
    that D19's failed bind leaves the pooled connection idle rather than in 25P02.
  - Pre-existing `postgres.rs:301` uses the same shape. Under AGENTS §5 that is drift, not precedent.
- Consequence:
  - The fast lane reports a Postgres behavior as verified when it never ran.
  - In `test:sql`, a missing or renamed env var would turn the D19 proof into a silent pass.
- Correction:
  - Move the test into `#[cfg(test)] mod pg_tests` in `tenant_conn.rs` so the fast lanes exclude it.
  - Make it fail, not return, when `WYRD_DATABASE_URL` is absent.
  - Prove it with `mise exec -- cargo nextest run --locked -p wyrd-sql --lib -E 'test(=tenant_conn::pg_tests::failed_bind_rolls_back_and_the_connection_stays_usable)'`
    under `scripts/postgres/with-test-postgres.sh`, and with `mise run test:sql`.

### S3 — VIOLATION: new IO workflows written as free functions threading owner state

- Violated: AGENTS §5 "Required Struct-Centered Rust Style" ("A workflow function is not made stateless merely
  because all of its dependencies are parameters"). agent-rules makes this a hard acceptance criterion.
- Locations and evidence:
  - a. `crates/wyrd/wyrd-server/src/oracle/query_audit.rs:125` `write_batches(vala, decisions, pending, stop)`
    and `:154` `commit_batch(&vala, &mut batch)`. They were added in 827e37ab6/0d22d834e. The writer loop is the
    owner's background workflow, but it lives as free async functions. `OracleQueryAudit::new` hands them four
    pieces of state (`ValaPostgres`, the receiver, `pending`, `stop`) that form one cohesive writer.
  - b. `crates/wyrd/wyrd-server/src/state.rs:843` `run_scribe_wal_fault_monitor(fault, cluster,
    registered_role, lifecycle, advertise_ready, shutdown)`. It is a new six-parameter IO workflow (cluster
    deactivation, readiness, lifecycle), spawned at `:931`.
  - c. `crates/wyrd/wyrd-sql/src/tenant_conn.rs:108` `begin_bound(&PgPool, String)`. It is a new IO helper
    outside its owner `TenantConn`, and it is the only new library signature taking `&PgPool`.
- Consequence: the workflows are not discoverable on their owners, and dependency threading grows in exactly
  the shape §5 forbids. Reviewers must treat the result as incomplete even though it is behaviorally correct.
- Correction:
  - (a) Introduce a private `OracleAuditWriter { vala, decisions, pending, stop }` with `run(self)` and
    `commit_batch(&mut self, …)`, spawned by `OracleQueryAudit::new`.
  - (b) Move the monitor onto an owning struct, such as a `ScribeWalFaultMonitor` holding those six fields with
    `run(self)`, or a method on the Scribe runtime owner that already holds them.
  - (c) Make it a private associated function on `TenantConn`, e.g. `TenantConn::begin_with(pool, statement)`.
  - Verify with `mise run lints` plus the existing tests: `test:sql`, `test:server:*`, and the `server` journey
    binary `owner_inspection::scribe_wal_fault_is_role_local`.

### S4 — VIOLATION: function-scoped imports and fully-qualified types in new signatures

- Violated:
  - agent-rules: "All `use` statements live at the top of the module". The only exceptions are
    `mod tests` and `use Trait as _` in one generic function.
  - agent-rules: "Bring types in with `use` and use bare names in signatures."
- Locations and evidence:
  - New function-scoped imports:
    - `crates/wyrd/wyrd-storage/src/handle.rs:269-270` inside the new `get_object_bounded` imports
      `OpRead` and `normalize_path` (not trait-as-`_`).
    - `crates/vala/vala-bifrost-redux/src/bench_support.rs:40` adds `use wyrd_spec::vala::api::QueryClass;`
      inside `oracle_telemetry_label_domains`.
  - Fully-qualified types in new production signatures:
    - `vala-bifrost-redux/src/oracle/mod.rs:4340` `datafusion_resources_exhausted(&datafusion::error::DataFusionError)`.
      The same file already uses bare `DataFusionError` at `:4330`.
    - `oracle/query_stream.rs:1296` `datafusion_query_timeout(&datafusion::error::DataFusionError)`.
    - `oracle/exec.rs:901,905,913` `PublishedFooters` fields and `new()` use `crate::storage::BifrostStorage` and
      `wyrd_spec::DataTenantId`.
    - `oracle/exec.rs:3701` `leaf_excludes_span(predicate: &wyrd_spec::vala::assignment_authority::ScanPredicate, …)`.
    - `wyrd-server/src/http/middleware/body_limit.rs:200` `transport_occupied(&vala_bifrost_redux::resources::BifrostResourceError)`.
    - `oracle/admission.rs:554` `park_after_rows() -> Option<datafusion::error::DataFusionError>`.
  - Modified fields de-imported to full paths:
    - `scribe/persistence.rs:112,116` changed `Arc<AtomicU64>` to `Arc<std::sync::atomic::AtomicU64>`.
    - `scribe/wal.rs:61` changed `AtomicBool` the same way.
- Consequence: the module's dependency manifest is incomplete, and signatures hide owning crates. The rule
  exists to prevent exactly this.
- Correction: hoist the imports to each module's `use` block and use bare names at the listed sites.
  `mise run fmt` and `mise run lints` pass.

### S5 — VIOLATION: missing rustdoc / `# Errors` on new or materially modified items

- Violated: AGENTS §16 (rustdoc on every new or materially modified item including fields; `# Errors` on every
  fallible function). agent-rules makes missing rustdoc `BLOCK_BEFORE_MERGE`.
- Locations and evidence:
  - `crates/vala/vala-bifrost-redux/src/resources.rs:1145`: new field `BifrostRoleResources::forge` has no
    rustdoc.
  - `crates/vala/vala-bifrost-redux/src/scribe/preprocess.rs:59` and `:115`: `durable_ack` changed type to
    `Result<DurableCompletion, ScribeError>` and has no rustdoc.
  - `crates/vala/vala-bifrost-redux/src/scribe/wal.rs:1976-1980`: `WalWriter::new` doc gained a new paragraph
    (materially modified). It returns `Result<Self, ScribeError>` but has no `# Errors` section.
  - `crates/vala/vala-bifrost-redux/src/scribe/wal.rs:61`: the modified `#[cfg(test)] static WAL_COUNT_ACTIVE`
    test helper has no rustdoc.
- Consequence: a hard documentation gate fails on touched items, and callers of `WalWriter::new` get no
  documented failure contract.
- Correction: add field docs that state each field's role and invariant. Add `# Errors` to `WalWriter::new`
  naming its validation and IO failures. Document `WAL_COUNT_ACTIVE`. Then run `mise run lints`.

### S6 — VIOLATION: task identifier in a committed script

- Violated: agent-rules "Never mention references to plans, tasks, or other agents in the codebase", and
  TESTING.md "A test must not name a plan".
- Location: `scripts/server/test-kind-autoscale.sh:4`
  `# Required for TASK-006 (AC-037) and runnable on demand; not a default CI gate.` (new file in range).
- Consequence: a maintainer of a permanent test script is sent to an ephemeral plan document that disappears at
  completion.
- Correction: replace the line with a behavior statement, e.g. "Local kind autoscaling journey; runnable on
  demand, not a CI gate." Then `git grep -n 'TASK-\|AC-0' -- scripts` returns nothing for this file.

## Non-findings worth recording

- `ponytail:` annotations: six new ones; 13 already existed at base. They are self-describing ceiling notes, not
  plan or agent references, so I did not raise them.
- The D19 inlined tenant SQL (`begin_tenant_sql`) builds SQL with `format!`. That is injection-safe only through
  the typed `DataTenantId`, and the safety invariant is documented in its rustdoc. The tenant-isolation check
  passes on it.

## Overall

**FAIL** — S1 (dead, unsanctioned check exemption), S2 (Postgres proof that passes silently in the fast lane),
S3 (struct-centered violations in new IO workflows), S4 (import/signature rules), S5 (missing rustdoc /
`# Errors`), S6 (task reference in a script). Gate-dependent rules (R17, R23) await the orchestrator's
`mise run gate`.
