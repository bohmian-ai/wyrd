# Repository standards review — TASK-002-r2

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `8b201627c0a957dccf46649d00c8c205689bc5de`
- Reviewed range: the complete cumulative base-to-candidate diff (59 files; Rust contracts, client, auth/server, SQL and migration, generated schemas, docs, identity fixtures, tests, and `mise.toml`).
- Candidate identity was rechecked before this report and remained `8b201627c0a957dccf46649d00c8c205689bc5de`.

## Authority coverage

| Changed surface | Applicable authority | Coverage and result |
|---|---|---|
| Repository workflow, ownership, completion, documentation | `AGENTS.md` §§1–6, 9, 11–12, 15–16; `architecture/agent-rules.md` | Reviewed all changed Rust and verification surfaces. **FAIL**: mandatory Rust-item documentation and top-level-import rules are violated (STD-001, STD-002). Other applicable ownership, async, error, and completion rules pass on source evidence or the recorded checks below. |
| Approved task/remediation artifacts | `architecture/references/languages/spec-driven-development.md`; `architecture/references/languages/implementation-execution.md` | Original task, remediation task, full cumulative diff, named proof, and generated/static lanes are present. Review source was kept immutable. **PASS**. |
| Wyrd public auth/client/server contract | `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/references/doctrine/architecture-constraints.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/agent-harness.md` | Typed `BeginLogin`/response contracts live in `wyrd-spec`; durable login behavior remains on `HumanConnections`/`AuthorizationCodeExchange`; HTTP/OpenAPI, schema, docs, and shared-client retirements agree. The server remains the serving owner. **PASS**. |
| Authentication, identity, secrets, external-provider IO, audit | `architecture/wyrd-security-posture.md`; `architecture/wyrd-design.md` auth doctrine; `architecture/agent-rules.md`; `architecture/references/architecture/patterns.md` | Tenant is derived from opaque state then RLS, provider calls use the existing screened HTTP owner, PKCE is `SecretString`, completions are sealed, issuer/audience/nonce/`azp`/advertised algorithm checks precede identity issuance, role-sync and successful token issuance audit in the issuing transaction. **PASS**. |
| Rust structure, async, errors, imports, documentation | `AGENTS.md` §§4–6, 9, 16; `architecture/agent-rules.md`; `architecture/references/languages/rust-core.md`; `architecture/references/languages/errors.md` | Concrete owners and narrow synchronous helpers are used; async functions await IO; public errors remain derive-backed `WyrdError` projections. **FAIL** only for STD-001 and STD-002. |
| Tenant SQL, RLS, transaction ownership, migration | `AGENTS.md` §§9, 15; `architecture/agent-rules.md`; `architecture/wyrd-security-posture.md`; `architecture/references/doctrine/architecture-constraints.md`; `architecture/references/architecture/patterns.md`; Rust-core Postgres rules | Login-state tenant transitions accept `TenantConn`, contain no duplicate tenant selector, and do not commit. The narrow cross-tenant lookup is an inherent `WyrdPostgres` operation over its private app pool; the SECURITY DEFINER function fixes `search_path`, revokes PUBLIC, and grants only `wyrd_app`. **PASS**. |
| Tests, journeys, OpenAPI and generated schemas | `AGENTS.md` §11; `architecture/agent-rules.md`; `architecture/references/languages/testing-workflows.md`; `architecture/references/languages/agent-harness.md` | Real-server identity journeys cover login, refusal, provider replacement, and machine independence; Postgres integration tests cover state/RLS; served OpenAPI and generated schemas have contract proof. `codegen:check` passed during this review. **PASS**, subject to the documentation blocker. |
| CLI and docs | `AGENTS.md` §§2, 9; `architecture/wyrd-doctrine.mdx`; `architecture/references/architecture/patterns.md` | The obsolete direct authorization-code exchange and CLI login path are removed consistently; docs state that later BFF/CLI handoff owners consume the new contract rather than preserving a compatibility route. **PASS**. |
| Deployment/test fixtures/tooling | `AGENTS.md` §§11–12, 15–16; `architecture/references/languages/testing-workflows.md` | Keycloak realm fixtures and focused identity tasks remain repository-managed, gated integration infrastructure; no real credentials or production-only dependency was added. **PASS**. |

## Applicable rule results

| Rule | Evidence | Result |
|---|---|---|
| Durable server behavior stays in Rust owners; contracts stay typed and language agnostic | `wyrd-spec/src/auth/oidc.rs`, `wyrd-auth/src/login.rs`, `wyrd-auth/src/callback.rs`, and server adapters preserve this split. | PASS |
| Stateful workflows use cohesive concrete owners | Login begin/redemption are inherent `HumanConnections` methods; callback exchange is an `AuthorizationCodeExchange` method; the exceptional lookup is on `WyrdPostgres`. | PASS |
| Async is limited to real IO/composition | Added async paths await database, HTTP, server, or mock-server IO; parsing, claim checks, hashing, and URL construction remain synchronous. | PASS |
| Tenant work uses `TenantConn`, RLS, and caller-owned commit | `wyrd-sql/src/queries/auth/login_state.rs` takes `&mut TenantConn<'_>`, has no manual tenant predicate in transitions, and never commits. | PASS |
| Narrow cross-tenant capability does not expose a raw pool signature | `WyrdPostgres::login_state_tenant` owns the app-pool use; the former public pool-taking query was removed. `mise run check:from-pools-allowlist` passed. | PASS |
| Secrets are redacted and provider URLs use the screened owner | PKCE is `SecretString`; completion ciphertext is sealed; provider discovery/token/JWKS calls use `HumanConnections`' screened HTTP capability. | PASS |
| Public errors use stable Wyrd catalog mapping | New refusals use existing `WyrdError` variants and server `WyrdErrorResponse`; no parallel problem-json/error-code implementation was added. | PASS |
| Authorization/audit transaction rules remain intact | Provider-driven role changes append `auth.user.roles.sync` through the canonical append before the issuing transaction commits; audit failure rolls back state and issuance. | PASS |
| Generated artifacts derive from owning sources | Generator changes accompany JSON schema/golden changes; `mise run codegen:check` passed. | PASS |
| New user-facing behavior has real journeys and supporting integration proof | Four named identity journeys plus Postgres/OpenAPI tests are in the candidate and included in repository tasks. | PASS |
| Every new/materially modified Rust item has complete rustdoc, including `# Errors`/`# Panics` where applicable | New trait methods, SQL constants, a panicking test helper, and a test type alias lack the required documentation. | **FAIL — STD-001** |
| All imports are at module top (apart from the documented narrow exception) | New non-generic functions contain local imports in `Sha256Hex::digest`, `PartialSchema::schema`, and the identity mock-provider helper. | **FAIL — STD-002** |
| No gate circumvention or weakened test | No new `allow`, deleted required assertion, or ignored fast-lane test was found; the new ignored tests are explicitly gated real-service journeys. | PASS |

## Material findings

### STD-001 — BLOCK_BEFORE_MERGE: new Rust items do not meet mandatory documentation rules

- Violated authority: `AGENTS.md` §16 and `architecture/agent-rules.md` require rustdoc for every new or materially modified Rust item, regardless of visibility, and require `# Errors` or `# Panics` for fallible or panicking operations. `architecture/references/languages/rust-core.md` makes missing or incomplete rustdoc a hard blocker.
- Exact evidence:
  - `crates/wyrd-spec/src/auth/oidc.rs:235`, `:241`, `:247`, `:257`, `:261`, and `:280` add `Display`, serialization/deserialization, JSON Schema, and OpenAPI trait methods without rustdoc; the serializer/deserializer methods are fallible and also lack `# Errors`.
  - `crates/wyrd/wyrd-sql/src/queries/auth/login_state.rs:29`, `:39`, `:49`, and `:58` add or materially rewrite the insert, consume, complete, and redeem SQL constants without rustdoc explaining their state-transition role and invariants.
  - `crates/wyrd/wyrd-server/tests/identity_e2e.rs:3082` adds a helper that calls two `expect`s but has no `# Panics`; `:3362` adds the local `Mutation` type alias with no rustdoc.
- Consequence: the candidate violates a repository hard acceptance rule even though formatting, lint, codegen, and runtime tests may pass; the undocumented state-transition and wire-projection items omit precisely the invariants maintainers need to preserve.
- Testable correction: add intent/workflow/invariant rustdoc to each new or materially modified item in the cumulative diff, including the locations above; add accurate `# Errors`/`# Panics` sections wherever the operation can fail or panic. Then rerun `mise run fmt`, `mise run lints`, and the focused auth/spec/SQL checks. No behavior, abstraction, or new test harness is needed.

### STD-002 — VIOLATION: new function-scoped imports bypass the module dependency manifest

- Violated authority: `architecture/agent-rules.md` requires imports at module top; its `use TraitName as _` exception is limited to a single generic function where the trait does not belong at module scope. `architecture/references/languages/rust-core.md` likewise requires module-top imports.
- Exact evidence:
  - `crates/wyrd-spec/src/auth/oidc.rs:216` imports `sha2::Digest as _` inside the non-generic `Sha256Hex::digest` method.
  - `crates/wyrd-spec/src/auth/oidc.rs:281` imports OpenAPI schema types inside `PartialSchema::schema`.
  - `crates/wyrd/wyrd-server/tests/identity_e2e.rs:3052-3053` imports wiremock matchers/types inside `mount_mock_provider_advertising`.
- Consequence: the changed modules conceal dependencies at use sites and do not satisfy the repository's explicit import-layout rule.
- Testable correction: move these imports to the owning module's top-level import block, using `#[cfg(feature = "server")]` where needed for OpenAPI-only types; run `mise run fmt` and `mise run lints`. No new abstraction or dependency is warranted.

## Verification notes and limits

Executed during this review:

1. `mise run check:from-pools-allowlist` — PASS.
2. `mise run check:tenant-isolation` — PASS.
3. `mise run codegen:check` — PASS.
4. `git diff --check 3fc085acf5b3a710d5dc80892bd2e664b3db6174..8b201627c0a957dccf46649d00c8c205689bc5de` — PASS.

The candidate records successful focused auth/spec/SQL tests, all four identity journey wrappers, the complete 27-test identity lane, principals integration/unit, SQL, client-tier, format, and lint lanes. This review did not rerun those longer runtime lanes; it inspected their source coverage and used the recorded results as available verification evidence. CodeGraph was unavailable because this repository has no `.codegraph/` index, so source and call-path inspection used the immutable diff and repository files.

## Overall result

**FAIL**

Security, tenancy, persistence, contract, generated-artifact, and journey-test rules pass. The candidate cannot receive a repository-standards pass until STD-001 and STD-002 are corrected because rustdoc completeness is an explicit hard blocker and import placement is a mandatory agent rule.
