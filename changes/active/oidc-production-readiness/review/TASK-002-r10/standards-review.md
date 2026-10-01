# Repository Standards Review — TASK-002 r10

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `6e21d8ed00d5159ec71e3f2e2414e80fd16f76af`
- Scope: complete base-to-candidate diff, including the original task, all nine remediation packets and their prior review artifacts, Rust auth/client/spec/server/CLI/SQL changes, migration, generated schemas, tests, identity fixtures, documentation, Docker composition, and `mise` task wiring.
- Candidate stability: `HEAD` resolved to the candidate before and after inspection. The reviewed source was not edited.
- CodeGraph: `.codegraph/` is absent, so repository inspection used Git and source reads as directed by `AGENTS.md` §18.

## Authority coverage

| Changed surface | Applicable authority read and applied | Coverage result |
|---|---|---|
| Active task, remediation, and review packet | `AGENTS.md` §§11–16; `architecture/agent-rules.md`; `architecture/references/languages/spec-driven-development.md`; `architecture/references/languages/implementation-execution.md`; `architecture/references/languages/testing-workflows.md` | Covered. The packet is tracked under `changes/active/oidc-production-readiness/`, retains the approved-spec/task/remediation/evidence chain, names exact focused commands, and records broader verification. This review does not adopt prior reviewers' conclusions. |
| Shared verifier (`wyrd-auth-verify`) | `AGENTS.md` §§3–6, 10, 16; `architecture/references/languages/rust-core.md`; `architecture/wyrd-security-posture.md` (federation, signing algorithms, fail-closed verification, secret handling) | Covered. OIDC-specific subject and token-binding validation remains on the verifier owner; generic workload verification remains distinct. New/modified items are documented and tested. |
| Shared Rust client (`wyrd-client`) | `AGENTS.md` §§2–4, 9; `architecture/wyrd-design.md` client model/runtime identity; `architecture/wyrd-doctrine.mdx` public surfaces; `architecture/references/doctrine/architecture-constraints.md`; `architecture/references/architecture/patterns.md` | Covered. The obsolete tenant authorization-code helper is removed instead of preserving a second durable login path. No server or SQL behavior moved into the client. |
| Wire contracts and schema generation (`wyrd-spec`) | `AGENTS.md` §§2–4, 9, 12, 16; `architecture/wyrd-design.md` doctrine/client model/runtime identity; `architecture/wyrd-doctrine.mdx`; `architecture/references/languages/errors.md`; `architecture/references/languages/rust-core.md`; `architecture/references/architecture/patterns.md` | Covered, with `REPO-TASK-002-1`. Typed `BeginLogin`, `LoginInitiation`, and `Sha256Hex` contracts are foundational and IO-free; generated schemas match their source. One materially changed contract test lacks mandatory rustdoc. |
| Tenant auth domain (`wyrd-auth`) | `AGENTS.md` §§3–6, 9–10, 15–16; `architecture/agent-rules.md`; `architecture/wyrd-security-posture.md`; `architecture/references/languages/rust-core.md`; `architecture/references/architecture/patterns.md`; `architecture/references/doctrine/architecture-constraints.md` | Covered. Stateful workflows remain inherent methods on `HumanConnections`, `AuthorizationCodeExchange`, `TenantTokenIssuer`, and `RefreshTokens`; pure claim checks remain narrow free helpers. Secrets are redacted, provider IO uses the screened owner, and transaction/lock/audit responsibilities remain explicit. |
| HTTP/auth server and OpenAPI | `AGENTS.md` §§2, 6, 9, 11, 16; `architecture/wyrd-design.md` runtime identity and public surfaces; `architecture/wyrd-security-posture.md`; `architecture/references/architecture/patterns.md` server pattern; `architecture/references/languages/errors.md`; `architecture/references/languages/testing-workflows.md` | Covered, with `REPO-TASK-002-2`. Requests/responses are typed, login derives routing only from the body route key and authority from consumed state, callback/token handlers use scrubbed instrumentation, and OpenAPI is tested from the served document. The materially changed auth-routes module documentation omits one of the four surfaces it now owns. |
| Postgres migration and SQL owners (`wyrd-sql`) | `AGENTS.md` §§3–6, 9, 11, 15–16; `architecture/agent-rules.md` SQL/RLS/transaction/import/column-mapping rules; `architecture/wyrd-security-posture.md` tenant/data isolation; `architecture/references/languages/rust-core.md` Postgres boundaries; `architecture/references/architecture/patterns.md` storage pattern; `architecture/references/doctrine/architecture-constraints.md` | Covered. Tenant work uses `TenantConn` and forced RLS without parallel tenant predicates; cross-tenant state-to-tenant lookup is a narrow app-role capability owned by `WyrdPostgres`; callees do not end caller-owned transactions; returned login-state fields use named `FromRow` mapping; PostgreSQL derives expiry/coordination timestamps. |
| CLI retirement and refresh output | `AGENTS.md` §§2–4, 9, 16; `architecture/wyrd-design.md` runtime identity/client model; `architecture/wyrd-doctrine.mdx` public-surface alignment; `architecture/references/architecture/patterns.md` client pattern | Covered. The unsafe/obsolete interactive login command is deleted with its module registration and docs references; refresh remains a client projection and keeps refresh material out of argv. |
| Rust unit, Postgres integration, real-server identity journey, and OpenAPI tests | `AGENTS.md` §11 and §16; `architecture/agent-rules.md` test placement/environment rules; `architecture/references/languages/testing-workflows.md`; `architecture/references/languages/spec-driven-development.md` | Covered, with `REPO-TASK-002-1`. SQL tests that require Postgres live in `pg_*` targets/modules; cross-crate and real-server behavior earns external test targets; the four public tenant-login journeys are selected by the repository-managed identity lane. |
| Keycloak fixtures, Docker, and `mise` identity wiring | `AGENTS.md` §§1, 11–12, 15; `architecture/references/languages/testing-workflows.md`; `architecture/references/languages/implementation-execution.md` | Covered. The second realm is a repository-local fixture, mounted read-only, and included in the existing sequential identity environment rather than a new harness. The lane verifies each required selector resolves exactly once. |
| Generated JSON schemas and docs site | `AGENTS.md` §§8–12, 16; `architecture/agent-rules.md` generated-artifact rule; `architecture/wyrd-doctrine.mdx` public surfaces; `architecture/references/architecture/patterns.md` verification pattern | Covered. Schema changes are source-derived and `codegen:check` is clean; auth, identity, self-hosting, SSO, CLI, and schema-index prose project the retired grant/current callback model consistently. |
| Python and TypeScript | `AGENTS.md` §§7–8 and client-surface rules | No source changed. Independent code generation produced no Python/TypeScript diff, so language-runtime lanes are not independently applicable to this candidate. |

## Applicable-rule results

| Rule | Evidence | Result |
|---|---|---|
| `wyrd-spec` remains foundational, IO-free, async-free, SQL-free, and PyO3-free (`AGENTS.md` §§2–3; architecture constraints) | Contract additions are typed values, validation, serde/schema projection, and hashing only. Dependency and client-tier evidence is recorded; no forbidden imports appear in the diff. | PASS |
| Durable behavior stays server-owned; clients only project wire contracts (`AGENTS.md` §9; design client model; architecture patterns) | Login-state persistence, provider exchange, role replacement, issuance, completion sealing, refresh, and audit stay in `wyrd-auth`/`wyrd-server`/`wyrd-sql`. `wyrd-client` removes the obsolete helper. | PASS |
| Struct-centered Rust ownership; free functions only for pure/stateless work (`AGENTS.md` §5; agent rules; Rust core) | Dependency-backed workflows are methods on existing concrete owners. Added free functions are claim validation, narrow event construction, conversion, URL construction, and test helpers determined by inputs. | PASS |
| Async only at IO/composition boundaries (`AGENTS.md` §6; Rust core) | Added async operations await HTTP, SQL, server calls, or test synchronization. Hashing, parsing, claim checks, initiation validation, and mapping remain synchronous. | PASS |
| Typed identifiers and secret redaction (`AGENTS.md` §4; security posture) | Tenant/principal/connection identities use domain types/UUIDs; provider and refresh secrets use `SecretString`; `LoginState` debug redaction is tested; handlers skip credential-bearing arguments. | PASS |
| Public failures use stable typed errors (`AGENTS.md` §§4, 9; errors reference) | New refusals map through existing `WyrdError` catalog variants/codes and `WyrdErrorResponse`; no parallel problem mapper or handwritten public catalog was introduced. | PASS |
| Tenant identity and SQL capability boundaries (`AGENTS.md` §§9, 15; agent rules; security posture) | `mise run check:tenant-isolation` and `mise run check:from-pools-allowlist` pass. Tenant queries take `TenantConn`, rely on forced RLS, and do not commit/rollback. The one pre-auth cross-tenant answer is private to the app-pool owner and returns only a tenant ID for an exact random-state digest. | PASS |
| PostgreSQL owns coordination/expiry timestamps (`AGENTS.md` §15) | Login expiry, consumption, completion, refresh issuance, and revocation use `statement_timestamp()`/database time; callers bind durations. | PASS |
| Audit and authorization transactions fail closed (`AGENTS.md` §2; agent rules; security posture) | Role replacement, role-sync audit, issuance audit, refresh mutation, completion persistence, and callback commit share the tenant transaction. No parallel audit sink was added. | PASS |
| External provider URLs use the existing resolve/screen/pin owner (`agent-rules.md`; security posture; architecture patterns) | Login discovery and endpoints route through `ScreenedHttp`; no direct `reqwest` fetch or string-only SSRF check was added. | PASS |
| Write handlers use scrubbed trace instrumentation (`AGENTS.md` §9; Rust core) | `POST /auth/login`, `POST /auth/token`, and callback durable completion boundaries use explicit `tracing::instrument` with credential-bearing arguments skipped. | PASS |
| No production unwrap/expect escape and no unjustified Clippy suppression (`AGENTS.md` §§4, 12; agent rules) | `mise run check:unwrap-audit` and `mise run check:clippy-allow-audit` pass. | PASS |
| Generated artifacts come from source and remain aligned (`AGENTS.md` §§8, 11–12; agent rules) | Independently run `mise run codegen:check` passed and restored a clean worktree. | PASS |
| Rustdoc covers every new or materially modified Rust item, including modules and tests (`AGENTS.md` §16; `architecture/agent-rules.md`; Rust core Documentation) | Most additions carry workflow, error, panic, and durability detail. `crates/wyrd-spec/src/auth/token.rs:226` and `crates/wyrd/wyrd-server/src/components/auth/routes.rs:1` do not satisfy the rule. | **FAIL** |
| Test taxonomy and placement (`AGENTS.md` §11; agent rules; testing workflows) | Unit checks cover pure contracts/claims, Postgres tests cover RLS/concurrency, served OpenAPI covers the runtime document, and real-server identity journeys cover the user-facing flows. | PASS |
| Canonical verification and no gate circumvention (`AGENTS.md` §§11–12; agent rules) | The packet records the required focused selectors and broader lanes as passing; no new `allow`, ignored fast-lane test, weakened assertion, or widened boundary allowlist appears. Static checks and diff hygiene are independently clean. | PASS |
| Public surfaces and documentation use one contract (`AGENTS.md` §§2, 9; doctrine public surfaces) | HTTP, OpenAPI, schemas, CLI retirement, and docs consistently remove public `authorization_code` exchange and token-bearing callback behavior. | PASS |
| No legacy compatibility path or task/agent reference in permanent source (`AGENTS.md` §§1–2; agent rules) | The old helper/CLI/route/grant are deleted rather than aliased; no plan/task/agent identifier was added to production source. | PASS |

## Material repository-rule findings

### REPO-TASK-002-1 — Materially changed token-contract test lacks mandatory rustdoc

- **Violated rule:** `AGENTS.md` §16 and `architecture/agent-rules.md` require rustdoc for every new or materially modified Rust item, explicitly including tests and test functions. `architecture/references/languages/rust-core.md` makes missing documentation a hard blocker.
- **Location:** `crates/wyrd-spec/src/auth/token.rs:225-241` (`new_grant_variants_reject_unknown_fields`).
- **Evidence:** The cumulative diff materially narrows this test after retiring `TokenRequest::AuthorizationCode`: it deletes that variant's unknown-field case while retaining the `jwt-bearer` and `refresh_token` contract checks. The current function is preceded only by `#[test]`, with no rustdoc describing the remaining contract.
- **Consequence:** A maintainer cannot tell from the required item-level documentation whether the narrowed test intentionally covers only the surviving recently added grants or accidentally lost authorization-code coverage. More directly, the candidate violates an explicit hard repository acceptance rule even though the assertions pass.
- **Testable correction:** Add concise rustdoc immediately above `#[test]` explaining that the surviving `jwt-bearer` and `refresh_token` request variants deny unknown fields after authorization-code retirement. Do not change the test body or add another test. Verify with `mise run fmt`, `mise run lints`, and `git diff --check`.

### REPO-TASK-002-2 — Auth-routes module contract is stale after adding login initiation

- **Violated rule:** `AGENTS.md` §16 requires every materially modified Rust module to document its workflow role and operation; it also requires documentation to be implementation-correct. The Rust core Documentation section rejects incomplete item documentation as a hard blocker.
- **Location:** `crates/wyrd/wyrd-server/src/components/auth/routes.rs:1-2`.
- **Evidence:** The module now mounts four surfaces and `auth_router` correctly documents them: tenant human login initiation, OIDC callback, token exchange, and Card-bound API-key issuance. The module-level rustdoc still says the tenant auth surfaces are only “token exchange, OIDC callback, and Card-bound API key issuance,” omitting the newly mounted `POST /auth/login` surface.
- **Consequence:** The file's required module contract gives an incomplete dependency/ownership overview precisely where the public auth surface changed, contradicting the complete router documentation below it and violating the hard Rustdoc rule.
- **Testable correction:** Update only the module-level rustdoc to include tenant human login initiation alongside the other three owned surfaces. Do not change routing or handler behavior. Verify with `mise run fmt`, `mise run lints`, and `git diff --check`.

## Verification evidence and limits

Independently executed during this standards review:

- `mise run check:from-pools-allowlist` — PASS
- `mise run check:tenant-isolation` — PASS
- `mise run check:unwrap-audit` — PASS
- `mise run check:clippy-allow-audit` — PASS
- `mise run codegen:check` — PASS; generated schemas/stubs restored with no worktree diff
- `git diff --check 3fc085acf5b3a710d5dc80892bd2e664b3db6174 6e21d8ed00d5159ec71e3f2e2414e80fd16f76af` — PASS

Available implementation evidence in the immutable task/remediation packet records successful focused verifier, auth, server, SQL, OpenAPI, concurrency, and real-server identity-journey commands, plus `test:principals:unit`, `test:principals:integration`, `test:sql`, `test:identity:journey`, `docs:check`, `fmt`, and `lints`. Those historical command records were checked for selector precision and applicability but were not independently re-run as part of this standards role. No Python or TypeScript source changed; `codegen:check` produced no SDK diff.

The two retained failures are static documentation-contract defects. Existing runtime checks cannot close them because compilation and behavior tests do not enforce complete rustdoc for private modules/tests.

## Overall result

**FAIL**

The candidate satisfies the reviewed architecture, security, tenancy, SQL, generated-contract, public-surface, and verification-boundary rules, but `REPO-TASK-002-1` and `REPO-TASK-002-2` violate the repository's explicit hard Rustdoc acceptance criterion.
