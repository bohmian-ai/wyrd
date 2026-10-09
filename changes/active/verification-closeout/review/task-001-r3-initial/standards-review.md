# Repository standards review — TASK-001 R3

**Subject:** `c46afdcac` → `437205debc628538ba6aa4ec828601c7c40145b4` (complete cumulative diff). The candidate commit was fixed for this review; the working tree was clean when inspected. This is a repository-rule audit, not an acceptance or design review. Verification results below are those recorded in the task artifact; I did not rerun the lanes.

## Authority coverage

| Changed surface | Applicable authority inspected | Coverage evidence |
|---|---|---|
| Public principal, Role, and auth contracts; migration | `AGENTS.md` §§2–4, 8–9, 11, 16; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/wyrd-security-posture.md`; references `doctrine/architecture-constraints.md`, `architecture/patterns.md`, `languages/errors.md`, `languages/testing-workflows.md` | `wyrd-spec/src/auth/tenant_principals.rs`, auth issuance/verification, `wyrd-sql` migration and auth queries, generated stubs and declarations |
| Server principal routes, authorization, audit, boot | `AGENTS.md` §§3–6, 9, 11, 16; agent rules for `TenantConn`, authorization audit, SQL and documentation; design/security authorities; references `architecture/patterns.md`, `languages/agent-harness.md`, `languages/errors.md` | `components/principals/routes.rs`, `components/auth/routes.rs`, admin routes, server integration target and `pg_principal_roles.rs` |
| Bifrost Gate, Scribe and OTLP attribution | `AGENTS.md` §§3–6, 9–11, 16; agent rules for cross-tier imports, SQL capability, audit, tests, rustdoc; `architecture/bifrost-design.md`; references `domain/olap-serving.md`, `domain/telemetry-observations.md`, `architecture/patterns.md` | `gate/attribution.rs`, `gate/mod.rs`, `contracts.rs`, Scribe and signal edits, Bifrost journeys |
| Shared client, CLI, Rust SDK | `AGENTS.md` §§3–6, 8–11, 16; agent rules; references `languages/rust-core.md`, `languages/errors.md`, `languages/testing-workflows.md`, `languages/maintainer-style.md` | `wyrd-client/principals/handle.rs`, CLI principal commands, Rust OTLP adapter and journeys |
| Python SDK and PyO3 | `AGENTS.md` §§7–8, 11, 16; agent rules; references `languages/pyo3-boundaries.md`, `languages/python-api-and-stubs.md`, `languages/testing-workflows.md` | `src/principals.rs`, package exports, generated `.pyi`, gateway/OTLP auth, Python unit and integration tests |
| TypeScript SDK and napi | `AGENTS.md` §§3, 8–9, 11, 16; agent rules; references `languages/typescript-guide.md`, `languages/testing-workflows.md` | native principal adapter, public `wyrd/src/index.ts`, `otel.ts`, declarations, package inventory, journeys |
| Documentation, fixtures, task and mise lanes | `AGENTS.md` §§1, 11–16; references `languages/spec-driven-development.md`, `languages/implementation-execution.md`, `languages/testing-workflows.md`; approved specification | docs pages, `fixtures/README.md`, `mise.toml`, task verification evidence |

## Rule assessment

| Rule | Result | Source and verification evidence |
|---|---|---|
| Tenant SQL must use `TenantConn` or `OperatorPool`, with transaction lifecycle owned by caller | **PASS** | Production auth query functions in `principal_directory.rs` and `role_assignments.rs` take `&mut TenantConn<'_>`; route handlers own `tenant_conn` and commit. No raw `PgPool`, caller-supplied `PgConnection`, or callee commit was found in those new owners. `check:tenant-isolation` is recorded green; this assessment also inspected the actual signatures. |
| `TenantConn` queries rely on RLS without parallel tenant filters | **FAIL, non-blocking** | New `principal_directory.rs` SQL repeats `data_tenant_id = wyrd.current_tenant()` in both union arms (lines 21–30); the new user-assignment listing in `role_assignments.rs` repeats it at line 45. `architecture/agent-rules.md` forbids manual tenant filters on `TenantConn` paths. This duplicates RLS but does not show a reachable isolation failure; note S-03 below. |
| Cross-tier imports use the owning tier's re-exports | **FAIL, non-blocking** | New `vala-bifrost-redux/src/gate/attribution.rs:21–22` imports `wyrd_sql::{WyrdPostgres, queries::cards::get_card_by_ref}` directly. `architecture/agent-rules.md` explicitly requires Vala/Bifrost imports through the tier's re-exports. Adjacent `gate/mod.rs:44` uses `vala_sql::audit_outbox::AuditOutbox`. See S-02. |
| Public contracts are typed and server owned; SDKs project the shared client | **PASS** | Principal wire structs live in `wyrd-spec/src/auth/tenant_principals.rs`; Rust `Principals` uses shared `WyrdClient` HTTP; PyO3 and napi `Principals` delegate to that handle. No client-tier SQL/DataFusion/cloud dependency appeared in changed manifests. `check:deps` and `codegen:check` are recorded green. |
| Auth decisions are checked before protected writes and staged on the canonical non-blocking audit path | **PASS** | New principal reads call `authorize` and Role writes call `audit::authorize` before SQL in `components/principals/routes.rs:601–889`; `pg_principal_roles.rs` reads decision rows. Existing audit architecture remains the task's governing path. |
| Server handlers use typed response/errors and tracing | **PASS** | Principal routes return `Json<PrincipalPage/PrincipalRoles/RoleAssignmentChange>` or `WyrdErrorResponse`, declare OpenAPI responses, and use `#[tracing::instrument(skip(state, caller, ...))]`. |
| PyO3 boundary is Python SDK owned and uses `__new__`, explicit signature, GIL release | **PASS** | `sdks/wyrd-sdk-python/src/principals.rs:37–54` uses `#[new] fn __new__` with `#[pyo3(signature = (client=None))]`; blocking handle calls use `py.detach`; Python package exports and stub assembly are present. |
| TypeScript/napi and Python public declarations track exports | **PASS** | TypeScript `wyrd/src/index.ts`, napi `native/src/principals.rs`, `index.d.ts`/`index.d.cts`, Python `principals/__init__.py` and generated `.pyi` project the new surface; recorded `ts:pack:check`, `py:typecheck`, and `codegen:check` passed. |
| Test tier ownership and runtime placement | **PASS** | SQL/server/Bifrost tests use repository-managed Postgres journeys; Python runtime behavior is exercised in Python tests and Node behavior in Vitest. `pg_principal_roles.rs` exercises real server and Postgres, earning an external integration target. New ignored Rust journeys are selected by their integration lanes. |
| Named Rust tests in the task report include exact focused `mise exec -- cargo nextest` commands | **FAIL, non-blocking evidence gap** | Task implementation evidence names `builtin_roles::tests::exactly_four_roles_are_built_in`, `roles_are_strictly_nested`, and `principal_roles::direct_and_idp_user_assignments_coexist` but reports regex suite selectors (`test(/^builtin_roles::tests::/)`, `test(/^principal_roles::/)`) rather than each required exact `test(=...)` command. Green aggregate results remain useful, but the report does not meet `AGENTS.md` §11 command precision. |
| New or materially modified Rust items have complete rustdoc | **FAIL, blocking** | Several new declarations have no rustdoc: `crates/wyrd/wyrd-cli/src/principal/mod.rs:1` (`pub mod assignment`), `crates/wyrd/wyrd-sql/src/queries/auth/role_assignments.rs:37` (`LIST_USER_ROLE_ASSIGNMENTS_SQL`), `sdks/wyrd-sdk-rust/src/lib.rs:57` (`pub mod otel`), `sdks/wyrd-sdk-python/src/lib.rs:20` (`mod principals`), `sdks/wyrd-sdk-ts/native/src/lib.rs:13` (`pub mod principals`), and added test module declarations including `crates/wyrd/wyrd-server/tests/integration/main.rs:21`. `AGENTS.md` §16 makes missing rustdoc a hard blocker independent of compiler and lint success. See S-01. |
| Formatting, linting, generated contracts, docs and scoped journeys | **PASS on recorded evidence** | Task records `mise run fmt`, `lints`, `py:lints`, `py:typecheck`, `codegen:check`, `docs:check`, `check:deps`, `check:tenant-isolation`, scoped principal, OTLP, gateway, identity, and first-class SDK lanes as passed. These checks do not cover every rustdoc declaration or direct Vala import. |

## Findings

### S-01 — Missing required rustdoc on changed Rust items (blocking)

**Rule:** `AGENTS.md` §16 and `architecture/agent-rules.md` require rustdoc on every new or materially modified item, including private constants, modules and test modules; missing rustdoc is `BLOCK_BEFORE_MERGE`.

**Location/evidence:** New module declarations at `crates/wyrd/wyrd-cli/src/principal/mod.rs:1`, `sdks/wyrd-sdk-rust/src/lib.rs:57`, `sdks/wyrd-sdk-python/src/lib.rs:20`, `sdks/wyrd-sdk-ts/native/src/lib.rs:13`; new query constant at `crates/wyrd/wyrd-sql/src/queries/auth/role_assignments.rs:37`; added integration submodule at `crates/wyrd/wyrd-server/tests/integration/main.rs:21`. The enclosing file or module documentation does not document each declaration. Additional changed declarations should receive the same inspection.

**Consequence:** The candidate fails a mandatory merge rule even though format, Clippy and codegen pass. A maintainer lacks declaration-local workflow context where the module tree and SQL query are introduced.

**Testable correction:** Document each added/materially modified declaration in its source, including modules, constants, helpers and test items, with workflow purpose and relevant invariants; then inspect the complete changed Rust declaration set against §16. Keep the existing lint and codegen gates intact.

### S-02 — Direct Wyrd SQL import from a Vala tier (non-blocking)

**Rule:** `architecture/agent-rules.md`, cross-tier import rule.

**Location/evidence:** `crates/vala/vala-bifrost-redux/src/gate/attribution.rs:21–22` directly imports `WyrdPostgres` and `get_card_by_ref` from `wyrd_sql`. The immediate Vala Gate module imports its SQL surface through `vala_sql` at `gate/mod.rs:44`.

**Consequence:** The Gate's database dependency bypasses the tier's declared surface and couples Vala directly to an underlying control-plane query owner. This is a structural boundary violation; no concrete runtime failure was established in this standards pass.

**Testable correction:** Put the required registry lookup behind the applicable tier-owned/re-exported surface or existing server-composition owner, then verify the Gate imports only the allowed boundary and run `check:deps` plus the focused Gate tests.

### S-03 — Manual tenant predicate on a `TenantConn` query (non-blocking)

**Rule:** `architecture/agent-rules.md`, `TenantConn` RLS rule.

**Location/evidence:** New `principal_directory.rs:21–30` explicitly constrains each union arm with `data_tenant_id = wyrd.current_tenant()` while both functions accept `&mut TenantConn<'_>`. The new `LIST_USER_ROLE_ASSIGNMENTS_SQL` in `role_assignments.rs:37–48` also repeats the predicate.

**Consequence:** Tenant policy is encoded twice and may drift between RLS and query text. No isolation breach is demonstrated because the query remains under `TenantConn` and RLS.

**Testable correction:** Remove redundant tenant filters where RLS alone supplies the boundary; retain only predicates independently required for a join or index, with that need documented.

## Overall: FAIL

S-01 violates the repository's explicit rustdoc completion gate. S-02 and S-03 are rule failures recorded as non-blocking structural notes under the task-review threshold. The task's verification evidence is reported, not independently rerun here. This report makes no claim about task acceptance or security-domain correctness.
