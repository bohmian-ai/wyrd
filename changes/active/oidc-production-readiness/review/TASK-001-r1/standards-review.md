# Repository Standards Review — TASK-001 r1

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `a5c8041a348a66bfb56fbac492383e8c688b0590`
- Candidate: `e126cdca7d4bf5bc467279df05cc3e199eb7fdf2`
- Candidate checked at review start and completion: `HEAD == e126cdca7d4bf5bc467279df05cc3e199eb7fdf2`
- Diff: 47 files, 4,922 insertions, 418 deletions
- CodeGraph: not used because the repository has no `.codegraph/` directory.

## Authority coverage

| Changed surface | Applicable authority read | Evidence inspected | Coverage |
|---|---|---|---|
| Public OIDC connection contracts, schemas, and stable errors (`wyrd-spec`) | `AGENTS.md` §§2–9, 16; `architecture/wyrd-design.md` protocol/runtime-identity/error sections; `architecture/wyrd-doctrine.mdx`; `architecture/references/languages/errors.md`; `architecture/references/architecture/patterns.md` | `auth/human_connection.rs`, `auth/mod.rs`, `error.rs`, served OpenAPI registration and contract test | COMPLETE |
| Tenant identity/auth lifecycle (`wyrd-auth`) | `AGENTS.md` §§3–6, 9, 15–16; `architecture/agent-rules.md`; `architecture/wyrd-security-posture.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/rust-core.md`; `architecture/references/languages/agent-harness.md` | `connections.rs`, `callback.rs`, `pg_resolvers.rs`, `issuance.rs`, `platform_login.rs`, `sealing.rs`, deleted `issuer.rs`, all direct callers | COMPLETE |
| HTTP routes, boot/config, OpenAPI, CLI | `AGENTS.md` §§2–6, 9, 11, 16; security posture; agent-harness, errors, and testing references | admin identity routes, auth login/callback, server boot/config/router/OpenAPI, CLI trusted-issuer path | COMPLETE |
| Tenant SQL and migration | `AGENTS.md` §§3, 9, 15–16; `architecture/agent-rules.md` SQL/RLS/transaction rules; security posture tenant/data isolation | new migration, tenant queries, operator rewrap queries, row types, platform query additions, migration tests | COMPLETE |
| Secret sealing and rotation | `AGENTS.md` §§3–6, 9; security posture secret/audit requirements; Rust-core reference | `wyrd-crypt`, `SealingKeyring`, rewrap owner, config/boot wiring, rotation journey and docs | COMPLETE |
| Rust/TypeScript generated contract projection | `AGENTS.md` §§2, 8–9, 11; errors, TypeScript, agent-harness references | derive-backed error variants and generated `error-codes.ts`; implementation evidence for `codegen:check` | COMPLETE |
| Tests, fixtures, and `mise` lanes | `AGENTS.md` §§4, 11–12, 16; `architecture/agent-rules.md`; testing-workflows, spec-driven-development, implementation-execution references | identity journeys, served OpenAPI test, migration test, fixture/test-server additions, identity lane changes | COMPLETE |
| Public documentation | `AGENTS.md` §§1–2, 11–12; doctrine; security posture; approved spec | four changed authentication/identity pages and implementation evidence for `docs:check` | COMPLETE |
| Dependency/lockfile changes | `AGENTS.md` §§1, 4, 15; Rust-core reference | `wyrd-crypt/Cargo.toml`, workspace dependency usage, `Cargo.lock` | COMPLETE |

## Applicable-rule results

| Rule | Result | Exact evidence |
|---|---|---|
| Durable identity behavior remains server-owned; contracts remain in `wyrd-spec`; Postgres behavior remains in `wyrd-sql` | PASS | Typed contracts are in `crates/wyrd-spec/src/auth/human_connection.rs`; lifecycle owner is `crates/wyrd/wyrd-auth/src/connections.rs`; SQL is in `crates/wyrd/wyrd-sql/src/queries/auth/human_connections.rs`. |
| `wyrd-spec` stays IO-free, async-free, and PyO3-free | PASS | New contract module contains synchronous validation/schema types only; no IO, async, PyO3, or server dependency was added. |
| Public cross-boundary failures use derive-backed `WyrdError` metadata and project to TypeScript | PASS | `crates/wyrd-spec/src/error.rs:781-837` adds four derived variants; `sdks/wyrd-sdk-ts/wyrd/src/error-codes.ts:34-60` projects their generated codes. |
| Public HTTP operations use typed schemas, stable problems, versioned `/v1` routes, and trace instrumentation | PASS | `crates/wyrd/wyrd-server/src/components/admin/identity.rs:96-384`; each operation declares typed OpenAPI bodies/responses, `WyrdProblem`, and `#[tracing::instrument(skip_all,...)]`. |
| Tenant identity comes from the verified principal, not request data | PASS | All six handlers use `caller.data_tenant_id`; no request contract contains tenant identity (`identity.rs:122-380`). |
| Tenant SQL uses `TenantConn`, relies on RLS, and tenant callees do not commit/rollback | PASS | Query signatures in `human_connections.rs:81-324` take `&mut TenantConn`; SQL has no manual tenant predicates; transaction completion stays in the lifecycle owner. Migration enables and forces RLS at lines 137-141. |
| Cross-tenant rewrap uses the privileged capability explicitly | PASS | `human_connections.rs:327-365` uses `&OperatorPool` with compare-and-swap updates and explicit tenant-isolation annotations. |
| Raw `PgPool` is banned from library fields and signatures | **FAIL (REPO-001)** | New `HumanConnections` stores `PgPool` and accepts it in its constructor (`wyrd-auth/src/connections.rs:22,75-79,114-119`); new `ServerAuth::human_connections` accepts `&PgPool` (`wyrd-server/src/components/auth/state.rs:8,62`). |
| One authorization evaluation produces one canonical audit row; non-decision mechanics are not audit events | **FAIL (REPO-002)** | `identity.rs:236-246` records the one `decide` result, then fabricates a second Allowed event without a second permission evaluation; `connections.rs:297-318` appends that synthetic event with the test-stamp mutation. |
| Provider URL IO is resolved, screened, pinned, bounded, and redirect-safe | PASS | `connections.rs:541-673` obtains a fresh screened client for discovery, JWKS, authorization, and token URLs; existing `ScreenedHttp::client_for` pins screened addresses, applies a timeout, and refuses redirects. |
| Secrets use secret-bearing types, remain redacted, and are version-sealed/rewrapped | PASS | `ConnectionInput.client_secret` and activation recovery key use `SecretBearer`; `SealingKeyring` has redacted `Debug`; read shapes omit secret fields; `connections.rs:838-854` logs internal causes but returns generic payloads. |
| Stateful workflows have cohesive concrete owners | PASS | `HumanConnections`, `SealedSecretRewrap`, and `SealingKeyring` own their dependencies and expose inherent workflow methods; no new single-implementation trait/factory was introduced. |
| Every new/materially modified Rust item, including private fields/constants/helpers/tests, has meaningful rustdoc | **FAIL (REPO-003)** | Missing examples include every field of `HumanConnections`, `StagedCandidate`, and `TestTarget` (`connections.rs:75-104`), `LOCK_SLOT_SQL` plus `select_sql`/`swap_sql` (`wyrd-sql/.../human_connections.rs:35,390,403`), and journey constants/fields (`identity_e2e.rs:780,796-798,934-937`). |
| Types are imported at module scope and used bare in signatures | **FAIL (REPO-004)** | Added signatures use qualified types at `wyrd-spec/src/auth/human_connection.rs:209,291-293`, `wyrd-auth/src/connections.rs:83,663,801,849`, `wyrd-sql/tests/pg_migration.rs:165,229`, and `wyrd-testing/src/server.rs:3896`. |
| Test tasks use only the minimal needed feature set; `--all-features` is reserved for lint/type-check lanes | **FAIL (REPO-005)** | The materially rewritten identity journey task uses `--all-features` for both list and run commands at `mise.toml:660,669,678`, contrary to `AGENTS.md` §4 and testing-workflows. |
| User-facing HTTP behavior has real-server journey coverage and served OpenAPI coverage | PASS | `identity_e2e.rs:1469` and `:1791` are ignored provider/Postgres journeys under the repository setup lane; `pg_openapi_contract.rs:169` verifies the served document. |
| Postgres migration behavior is isolated from the fast lane and receives upgrade/preflight coverage | PASS | `wyrd-sql/tests/pg_migration.rs:154-421` runs in `pg_tests` against isolated unmigrated databases and checks valid migration plus refusal atomicity. |
| Generated artifacts are changed through their source and verified for drift | PASS WITH EVIDENCE LIMIT | `error-codes.ts` identifies itself as generated and matches the new derive catalog. The task record reports `mise run codegen:check` passed; this reviewer did not rerun that expensive lane. |
| Required format, lint, docs, SQL, journey, OpenAPI, and boundary checks are green | PASS WITH EVIDENCE LIMIT | The candidate's immutable task record reports all required commands exit 0. This review independently ran `mise run check:from-pools-allowlist`, `mise run check:tenant-isolation`, and `git diff --check`; all returned 0, although the from-pools script emitted `rg: python/: No such file or directory` and does not enforce the signature/field ban found above. |
| No legacy compatibility route, commercial hook, Python/PyO3 surface, or unrelated Bifrost behavior entered the diff | PASS | Complete name-status and source diff show none of those surfaces. |

## Material repository-rule findings

### REPO-001 — Raw application pool escapes its sanctioned owner

- **Violated rule:** `architecture/agent-rules.md:6` bans raw `sqlx::PgPool` in library fields/signatures and confines pool construction/ownership to the sanctioned Postgres handles.
- **Location:** `crates/wyrd/wyrd-auth/src/connections.rs:22,75-79,114-119`; `crates/wyrd/wyrd-server/src/components/auth/state.rs:8,62-64`.
- **Evidence:** The new `HumanConnections` capability stores a cloned `PgPool`, and server auth exposes another `&PgPool` signature solely to construct it. The owner later calls `TenantConn::acquire(&self.app, tenant)` directly at `connections.rs:469-473`.
- **Consequence:** The tenant connection lifecycle bypasses the repository's sanctioned Postgres ownership boundary. Future code can acquire arbitrary transactions from a raw pool rather than having the database capability and role separation expressed by construction.
- **Testable correction:** Compose the lifecycle owner with the existing sanctioned Wyrd Postgres handle (or caller-owned `TenantConn` at the operation boundary), remove every new raw-pool field/signature/import, and run the focused auth/server tests plus the SQL boundary checks. Do not expand the pool-construction allowlist.

### REPO-002 — Candidate testing emits two audit rows for one permission evaluation

- **Violated rule:** `architecture/agent-rules.md:12-14`, security posture “Audit integrity and privacy,” and agent-harness “Audit”: audit records authorization decisions, exactly one row per permission evaluation; mechanics that evaluate no permission are not audit.
- **Location:** `crates/wyrd/wyrd-server/src/components/admin/identity.rs:236-248`; `crates/wyrd/wyrd-auth/src/connections.rs:287-318`.
- **Evidence:** The handler calls `decide` once, commits that Allowed event before provider IO, then creates `identity.oidc.candidate.tested` with the same principal/permission and passes it to `begin_locked`, which appends it. No second RBAC/policy evaluation occurs between those events.
- **Consequence:** One authorization decision appears as two decisions in retained audit evidence, corrupting decision cardinality and attributing an engine state transition as an authorization verdict.
- **Testable correction:** Either perform a real second permission evaluation at the pre-stamp commit boundary and audit that independently, or keep the single pre-network decision and remove the synthetic Allowed audit event. Add a focused assertion on exact audit event count/operations for one successful candidate test and keep audit-failure behavior fail-closed.

### REPO-003 — New Rust items lack mandatory rustdoc

- **Violated rule:** `AGENTS.md:706-720` and `architecture/agent-rules.md:35`; missing rustdoc on any touched Rust item is `BLOCK_BEFORE_MERGE`.
- **Location:** Representative omissions: `crates/wyrd/wyrd-auth/src/connections.rs:75-104`; `crates/wyrd/wyrd-sql/src/queries/auth/human_connections.rs:35,390,403`; `crates/wyrd/wyrd-server/tests/identity_e2e.rs:780,796-798,934-937`.
- **Evidence:** The new structs have undocumented private fields; the lock SQL and private SQL-selector methods have no rustdoc; grouped comments document only the immediately following test constant, leaving the remaining constants and helper fields undocumented.
- **Consequence:** The candidate violates a hard repository acceptance gate despite compiling and passing ordinary public-doc lints; invariants around pool/secret ownership and test fixtures are not documented at the items that carry them.
- **Testable correction:** Audit every added or materially modified Rust item and add intent/workflow/invariant rustdoc to each missing field, constant, helper, trait method, and test item, including required `# Errors`/`# Panics`/cancellation notes. Re-run format, lints, docs, and a source review because public-only rustdoc linting does not prove private-item coverage.

### REPO-004 — Added signatures hide dependencies behind qualified paths

- **Violated rule:** `architecture/agent-rules.md:9-10` requires top-of-module imports and bare type names in signatures.
- **Location:** `crates/wyrd-spec/src/auth/human_connection.rs:209,291-293`; `crates/wyrd/wyrd-auth/src/connections.rs:83,663,801,849`; `crates/wyrd/wyrd-sql/tests/pg_migration.rs:165,229`; `crates/wyrd/wyrd-testing/src/server.rs:3896`.
- **Evidence:** New signatures spell `serde_json::Value`, `crate::auth::IssuerTokenPolicy`, `std::fmt::*`, `reqwest::Client`, `wyrd_dev_fixtures::pg::UnmigratedDatabase`, and `url::Url` inline instead of declaring them in module imports.
- **Consequence:** The module dependency surface is split between imports and signatures, directly violating the repository's readability/ownership convention.
- **Testable correction:** Import each type in the owning module's top `use` block and use its bare name in every added/materially modified signature; run format and lints.

### REPO-005 — Identity test lane forces the full feature union

- **Violated rule:** `AGENTS.md` §4 and `architecture/references/languages/testing-workflows.md`: test/build tasks declare the minimal feature set; `--all-features` belongs to workspace lint/type-check gates because it defeats artifact reuse.
- **Location:** `mise.toml:660,669,678`.
- **Evidence:** Both the new selection check and filtered/unfiltered journey execution use `-p wyrd-server --all-features --test identity_e2e`.
- **Consequence:** Every identity journey and even selector listing recompiles the heavy workspace feature union, imposing the exact permanent CI/local cost the rule prohibits.
- **Testable correction:** Replace `--all-features` with default features or the smallest explicit feature list actually required by `identity_e2e`; run the filtered admin and rotation journeys plus the unfiltered identity lane to prove equivalent coverage.

## Verification limits

- Review was static over the complete immutable base-to-candidate range and surrounding callers/owners.
- The candidate's task record contains detailed green results, but those claims were not treated as source proof and the long Cargo/provider/Postgres/codegen/docs lanes were not rerun within the review budget.
- Independently executed: `mise run check:from-pools-allowlist` (exit 0 with a non-fatal missing `python/` diagnostic), `mise run check:tenant-isolation` (pass), and `git diff --check base..candidate` (pass).
- No required authority or changed surface was unavailable.

## Overall result

**FAIL**

The candidate violates mandatory repository rules for SQL capability ownership, audit cardinality, Rust documentation, signature/import style, and test feature scope. These are bounded implementation corrections; authority coverage itself is complete and the review is not blocked.
