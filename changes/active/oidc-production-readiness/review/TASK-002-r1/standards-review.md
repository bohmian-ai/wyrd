# Repository Standards Review — TASK-002 r1

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `87de451ed87ad059cefd579eb15ef4b028a92547`
- Candidate checked at review time: `87de451ed87ad059cefd579eb15ef4b028a92547`
- Complete range reviewed: 46 files, 3,728 insertions, 1,663 deletions
- Task context used only to select applicable authorities and changed surfaces: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`

This report audits repository-rule conformance only. It does not judge task acceptance.

## Authority coverage

| Changed surface | Paths | Applicable authorities read and applied |
|---|---|---|
| Public auth contracts and generated schemas | `crates/wyrd-spec/src/auth/{mod,oidc,token}.rs`; `crates/wyrd-spec/examples/gen_schemas.rs`; `crates/wyrd-spec/{schemas,tests/schemas}/auth_*.json` | `AGENTS.md` §§2, 3, 4, 5, 6, 9, 11, 12, 16; `architecture/agent-rules.md`; `architecture/wyrd-design.md` runtime identity and API-key/JWT/auth sections; `architecture/wyrd-doctrine.mdx`; `references/architecture/patterns.md` contract/client/server patterns; `references/languages/rust-core.md`; `references/languages/errors.md`; `references/languages/testing-workflows.md`; `references/languages/spec-driven-development.md`; `references/languages/implementation-execution.md` |
| Shared Rust client and CLI retirement | `crates/shared/wyrd-client/src/auth.rs`; `crates/wyrd/wyrd-cli/src/auth/{login,mod,refresh}.rs` | `AGENTS.md` §§2–6, 9, 11–12, 16; `wyrd-design.md` client and runtime identity sections; `wyrd-doctrine.mdx` public surfaces; `patterns.md` client/contract ownership; `rust-core.md`; `errors.md`; `testing-workflows.md` |
| OIDC login, callback, connection, issuance, and completion domain logic | `crates/wyrd/wyrd-auth/src/{login,callback,connections}.rs` | `AGENTS.md` §§2–6, 9–12, 16; `architecture/agent-rules.md` SQL tenancy, audit, SSRF, struct ownership, docs, async; `architecture/wyrd-security-posture.md` trust boundaries, credential lifecycle, federation, tenant isolation, audit, SSRF, secrets; `wyrd-design.md` runtime identity/auth/audit; `patterns.md` server, external-network, audit, storage; `rust-core.md`; `errors.md`; `testing-workflows.md` |
| Server HTTP/OpenAPI auth adapters | `crates/wyrd/wyrd-server/src/auth/{login,callback}.rs`; `crates/wyrd/wyrd-server/src/components/auth/routes.rs`; `crates/wyrd/wyrd-server/src/audit/mod.rs`; `crates/wyrd/wyrd-server/tests/{identity_e2e,pg_openapi_contract}.rs` | `AGENTS.md` §§2–6, 9–12, 16; `agent-rules.md` audit, tenancy, testing, rustdoc; `wyrd-security-posture.md`; `wyrd-design.md`; `patterns.md` server and audit patterns; `rust-core.md`; `errors.md`; `testing-workflows.md` |
| Tenant SQL state, cross-tenant resolver, user identity, and migration | `crates/wyrd/wyrd-sql/src/queries/auth/{login_state,mod,users}.rs`; `crates/wyrd/wyrd-sql/src/queries/platform/tenant_resolver.rs`; `crates/wyrd/wyrd-sql/migrations/20260925000001_auth_login_state_binding.sql`; `crates/wyrd/wyrd-sql/tests/pg_migration.rs` | `AGENTS.md` §§2–6, 9–12, 16; `agent-rules.md` raw-pool ban, `TenantConn`, RLS, transaction ownership, named-field mapping, testing; `wyrd-security-posture.md` tenant/data isolation, secrets, federation; `patterns.md` storage/registry and server ownership; `rust-core.md`; `testing-workflows.md`; `spec-driven-development.md`; `implementation-execution.md` |
| Identity journey infrastructure and provider fixtures | `mise.toml`; `docker-compose.yml`; `tests/fixtures/identity/keycloak-realm{,-2}.json` | `AGENTS.md` §11–12; `agent-rules.md` environment-owned integration tests and gate integrity; `wyrd-security-posture.md` federation and trust; `testing-workflows.md`; `implementation-execution.md` repository-managed environments |
| Authentication documentation | `docs/src/content/docs/api/schemas.md`; `docs/src/content/docs/concepts/{authentication,identity-and-auth}.svx`; `docs/src/content/docs/reference/cli.svx`; `docs/src/content/docs/self-hosting/{authentication,sso-and-oidc}.svx` | `AGENTS.md` §§1–3, 9, 11–12; `wyrd-design.md`; `wyrd-doctrine.mdx`; `wyrd-security-posture.md`; `patterns.md` public surfaces and contract ownership |

No Bifrost, PyO3, Python, TypeScript, MCP, Vala analytical, DataFusion, Arrow, or Iceberg implementation surface changed, so their focused references were not applicable.

## Rule results

| Rule / authority | Evidence | Result |
|---|---|---|
| Durable auth behavior remains server-owned; pure wire contracts remain in `wyrd-spec` | Typed `BeginLogin`, `BeginLoginResponse`, `Sha256Hex`, and retired token variant are in `wyrd-spec`; state, provider exchange, issuance, sealing, SQL, and callback behavior remain under `wyrd-auth`, `wyrd-server`, and `wyrd-sql`. | PASS |
| Public request/response bodies are typed and OpenAPI is mounted from the same route registration | `POST /auth/login` consumes `Json<BeginLogin>` and returns `Json<BeginLoginResponse>`; `GET /auth/callback` consumes `Query<CallbackQuery>`; `pg_openapi_contract.rs::tenant_login_operations_publish_their_contract` checks the served document. | PASS |
| Public errors use the existing derive-backed `WyrdError` boundary and do not expose provider/store strings | Login and callback map failures to existing typed `WyrdError` variants through `WyrdErrorResponse`; provider/store errors are logged internally and rendered as stable redacted errors. No parallel problem-json mapper was added. | PASS |
| Server handlers are instrumented with scrubbed arguments | `wyrd-server/src/auth/login.rs:49-53` skips state and request; `components/auth/routes.rs:320` uses `skip_all` for code/state callback material. | PASS |
| Tenant and connection identity derive from trusted state, not headers | Login resolves only `BeginLogin.tenant_route_key`; callback hashes the opaque state and uses the narrow resolver before opening `TenantConn`; callback receives no `HeaderMap`. Tests cover hostile host/forwarded headers. | PASS |
| Provider HTTP uses the screened capability and provider/token destinations remain bounded | `HumanConnections` owns `ScreenedHttp`; discovery and code exchange reuse it; the authorization endpoint scheme is screened before emitting the URL. | PASS |
| Authorization-code callback does not return provider code or Wyrd tokens | `components/auth/routes.rs:341-354` returns a fixed completion redirect or static HTML; the issued response is sealed before commit and later redeemed through `LoginCompletions`. | PASS |
| Audit-required issuance fails closed in the issuance transaction | `AuthorizationCodeExchange::finish_id_token_exchange` issues, seals, completes state, and commits on one `TenantConn`; failed completion/audit returns before commit. Best-effort failure diagnostics do not replace the issuance audit. | PASS |
| `TenantConn` is the tenant boundary; tenant queries must rely on RLS rather than duplicate predicates | New SQL constants explicitly add `data_tenant_id = $1` to purge, consume, complete, and redeem statements, and the unit test requires those predicates. | **FAIL — STD-001** |
| Library function signatures use only `TenantConn` or `OperatorPool`, never raw `PgPool` | New `resolve_by_login_state_for_app(pool: &PgPool, ...)` accepts the raw pool and the callback passes `WyrdPostgres::app_pool()` through it. | **FAIL — STD-002** |
| A callee accepting `&mut TenantConn<'_>` never commits or rolls back | Query functions operate on the supplied transaction only; commit remains with `HumanConnections`, `AuthorizationCodeExchange`, or `LoginCompletions`, which acquired the transaction. | PASS |
| Secret-bearing structs use `SecretString` or a redacted custom `Debug` | `NewLoginState` and `ConsumedLoginState` derive `Debug` while storing the PKCE verifier as public `String`. | **FAIL — STD-003** |
| Core workflows have cohesive concrete owners and async is limited to IO composition | `HumanConnections`, `AuthorizationCodeExchange`, and `LoginCompletions` own state and dependencies; pure hashing/binding helpers remain synchronous; async methods await SQL or HTTP. | PASS |
| New materially changed Rust items have intent/error documentation | Changed production owners, fields, enums, helpers, handlers, and SQL operations carry rustdoc and `# Errors` where fallible. New journey/helper documentation is present on the materially added paths inspected. | PASS |
| Generated schemas have source definitions and generator registration | `gen_schemas.rs` adds both source-backed schemas, and source definitions/tests accompany both generated and golden JSON files. Available evidence records `codegen:regen` followed by `codegen:check`. | PASS |
| A new public/server auth capability has real-server journey and negative coverage | Four ignored `identity_e2e` journeys exercise login, boundary refusal, provider replacement, and machine independence; `mise.toml` asserts exact selectors and includes them in the identity lane. | PASS |
| Migration and changed persistence contract have live migration coverage | `pg_migration.rs` verifies legacy human refresh revocation and preserves machine rows; the available verification record includes `test:sql` and the focused migration target. | PASS |
| No gate was weakened or bypassed | The identity lane adds four required selector checks and a second provider fixture; no test was deleted, ignored to evade a failure, assertion weakened, lint allowance added, or boundary glob broadened. | PASS |
| Public docs and CLI documentation track the retired and added surfaces | CLI login references are removed with the command; auth concepts and self-hosting docs describe the body-routed begin flow, common callback, refresh cutoff, and no callback token body. | PASS |

## Material repository-rule findings

### STD-001 — Manual tenant predicates duplicate the `TenantConn` RLS boundary

- Violated rule: `architecture/agent-rules.md`: “TenantConn (Postgres RLS) is the load-bearing tenant boundary. Do not add manual per-query tenant filters on a TenantConn path.” `AGENTS.md` and `patterns.md` repeat that tenant SQL relies on RLS.
- Location: `crates/wyrd/wyrd-sql/src/queries/auth/login_state.rs:20-24,36-64,199-224,238-305`; the added unit assertion at `:317-329` cements the violation.
- Evidence: `PURGE_EXPIRED_LOGIN_STATE_SQL`, `CONSUME_LOGIN_STATE_SQL`, `COMPLETE_LOGIN_STATE_SQL`, and `REDEEM_LOGIN_COMPLETION_SQL` each filter `data_tenant_id = $1`; each is executed only through `&mut TenantConn<'_>`. The functions additionally bind `conn.data_tenant_id()` solely to satisfy those predicates.
- Consequence: tenant isolation now has two independently maintained mechanisms on the same path. That is the exact drift the rule forbids: changes can accidentally make application predicates disagree with transaction-local RLS, and the test would reward retaining the duplicate boundary.
- Testable correction: remove the manual tenant predicates and corresponding tenant binds from the four `TenantConn` statements, preserving state/binding/expiry predicates and relying on forced RLS. Replace the SQL-text test with proof that the statements keep their one-use and expiry predicates without asserting a second tenant boundary; run the focused login-state tests, `mise run test:sql`, and `mise run check:tenant-isolation`.

### STD-002 — The new cross-tenant state resolver propagates raw `PgPool` through a library API

- Violated rule: `architecture/agent-rules.md`: raw `sqlx::PgPool` is banned from library function signatures and fields; the only permitted SQL capabilities are `TenantConn` and `OperatorPool`, with pool construction/access kept at the `WyrdPostgres` boundary.
- Location: `crates/wyrd/wyrd-sql/src/queries/platform/tenant_resolver.rs:51-60`; caller at `crates/wyrd/wyrd-auth/src/callback.rs:119-122`.
- Evidence: `resolve_by_login_state_for_app(pool: &PgPool, ...)` is a new public library function, and the auth owner passes `postgres.app_pool()` into it.
- Consequence: the function makes an unrestricted raw pool part of the query-layer API instead of exposing the single narrow capability needed for state-to-tenant resolution. That weakens the repository's role/capability boundary and creates another raw-pool propagation precedent on a security-sensitive cross-tenant lookup.
- Testable correction: put the narrow login-state tenant lookup behind the existing sanctioned Postgres owner/capability rather than accepting `PgPool`; the auth caller must not obtain or pass a raw pool. Preserve the SECURITY DEFINER function's one-column/one-purpose behavior and prove unknown, expired, and consumed states return no tenant. Run the focused callback tests plus `mise run check:from-pools-allowlist` and `mise run check:tenant-isolation`.

### STD-003 — PKCE verifier is exposed by derived `Debug` on new durable state types

- Violated rule: `AGENTS.md` §4 requires `secrecy::SecretString` for secrets and redacted custom `Debug` for secret-bearing structs; `architecture/wyrd-security-posture.md` requires secrets to stay out of logs, traces, errors, audit, and generated artifacts.
- Location: `crates/wyrd/wyrd-sql/src/queries/auth/login_state.rs:125-163`, especially `#[derive(Debug, Clone)]` and `code_verifier: String`; construction/exposure at `crates/wyrd/wyrd-auth/src/login.rs:117-125`; consumption at `crates/wyrd/wyrd-auth/src/callback.rs:174-182`.
- Evidence: both `NewLoginState` and `ConsumedLoginState` derive `Debug`, and both hold the live PKCE verifier as a public plain `String`. Formatting either value prints the verifier. The predecessor auth boundary used `SecretString` for `LoginStateEntry.code_verifier`; this change drops that protection while `wyrd-sql` already depends on `secrecy`.
- Consequence: routine diagnostic formatting of login state can disclose the verifier that authorizes the in-flight code exchange, contrary to the repository's mandatory secret-handling boundary.
- Testable correction: retain the verifier as `SecretString` across the public SQL/auth state types, exposing it only at the SQL bind and provider request boundaries, or provide an explicit redacted `Debug` if decoding mechanics require an internal string row. Add one focused assertion that debug output omits a sentinel verifier and run the auth/SQL focused tests, `mise run lints`, and the relevant secret/redaction checks.

## Verification reviewed

- Direct review check: `git diff --check 3fc085acf5b3a710d5dc80892bd2e664b3db6174 87de451ed87ad059cefd579eb15ef4b028a92547` — PASS.
- Candidate remained at `87de451ed87ad059cefd579eb15ef4b028a92547` while this report was written.
- Available implementation evidence records all four focused identity journey wrappers, the complete 27-test identity journey lane, focused auth/server/spec/migration tests, `test:principals:integration`, `test:principals:unit`, `test:sql`, `codegen:regen`, `codegen:check`, docs generation/check, `check:tenant-isolation`, `check:client-tier`, `fmt`, `lints`, and `git diff --check`, all exit 0.
- I did not launch Cargo-backed verification in this shared checkout. The available evidence does not record `mise run check:from-pools-allowlist`; that omitted gate is directly relevant to STD-002. Green listed lanes do not override the source-level rule failures above.

## Overall result

**FAIL**

The changed surfaces otherwise follow the repository's ownership, typed-contract, header-free tenant routing, screened-provider, fail-closed issuance, OpenAPI, migration, documentation, and journey-test rules. STD-001, STD-002, and STD-003 are explicit repository-boundary violations and must be corrected before this candidate satisfies repository standards.
