# TASK-010 Repository Standards Review

## Review findings

### Critical

None.

### Important

#### REPO-TASK-010-1 — New OAuth items do not meet the mandatory rustdoc contract

- **Rule:** `AGENTS.md` §16 and `architecture/agent-rules.md:35` require substantive rustdoc on every new or materially modified Rust item, including fields, associated types, private helpers, and tests. Every fallible function also requires `# Errors`; missing documentation is `BLOCK_BEFORE_MERGE`.
- **Locations:** `crates/wyrd/wyrd-server/src/auth/oauth.rs:32`, `crates/wyrd/wyrd-server/src/auth/oauth.rs:187-190`, and `crates/wyrd-spec/src/auth/token.rs:257-258`.
- **Evidence:** The public tuple field of `OAuthError` has no field documentation. `FromRequest::Rejection` has no associated-type documentation. The fallible `OAuthForm::from_request` documents only that it reads the request and omits `# Errors`, although it rejects a wrong content type, an unreadable body, and an invalid or repeated form parameter. The newly added fallible test helper `parse` likewise has no `# Errors` section.
- **Impact:** The candidate leaves the OAuth wire boundary's rejection behavior undocumented at the exact extraction point and violates a repository hard acceptance rule that explicitly includes test helpers. Passing compilation and lint lanes do not waive this source-level requirement.
- **Required correction:** Document the tuple field and associated type, and add accurate `# Errors` sections to both fallible functions. Describe the existing behavior only; do not add a new wrapper, check, option, or error mechanism. Re-run `mise run fmt` and `mise run lints`.

#### REPO-TASK-010-2 — A new `TenantConn` query adds a forbidden parallel tenant predicate

- **Rule:** `architecture/agent-rules.md:11`, `architecture/references/architecture/patterns.md:172-174`, and `architecture/references/languages/rust-core.md:590-597` make `TenantConn`/Postgres RLS the load-bearing tenant boundary and prohibit manual per-query tenant filters on that path.
- **Location:** `crates/wyrd/wyrd-sql/src/queries/auth/refresh_tokens.rs:47-56,169-178`.
- **Evidence:** The new `ACTIVE_REFRESH_SQL` adds `AND data_tenant_id = $2`; `active_refresh` derives the same tenant from `conn.data_tenant_id()` and binds it while executing through `&mut TenantConn<'_>`. `mise run check:tenant-isolation` passes, but that structural check does not negate the directly observed production query violation.
- **Impact:** The query creates a second, hand-maintained tenant condition beside RLS. It currently binds the tenant already carried by `TenantConn`, so this is not evidence of a present cross-tenant read; the defect is the duplicated isolation authority that the repository rule forbids because it can drift.
- **Required correction:** Remove the `data_tenant_id` predicate and its second bind from the newly added active-refresh lookup, leaving token hash, revocation, and expiry classification under `TenantConn`'s RLS scope. Update any focused query assertion and re-run `mise run test:sql` plus `mise run check:tenant-isolation`.

### Suggestions

None.

## Non-blocking notes

- `crates/wyrd/wyrd-sql/src/row_types/auth/human_connections.rs:79` uses `wyrd_spec::auth::OAuthClientId` in a struct field, while `crates/wyrd/wyrd-server/src/components/auth/routes.rs:392,608` use fully qualified types in return signatures. `architecture/agent-rules.md:9` calls for top-level imports and bare names. These are import placement/source-shape issues only; under the lead's direction they do not block TASK-010 and do not contribute to the result.
- The deleted `/internal/bff/v1` consumers are not a TASK-010 gap. Consumer failures caused only by that deletion remain TASK-011 work as the approved task and lead direction state.

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `fe51df0af6f6852fa7a8bc7276f3ce1d31c86daa`
- Candidate: `04366e7fc28c466fcdcbc7279885cfee82a988a2`
- Diff reviewed: `git diff fe51df0af6f6852fa7a8bc7276f3ce1d31c86daa..04366e7fc28c466fcdcbc7279885cfee82a988a2`
- Candidate was still `HEAD` when this report was completed.
- Scope: repository-rule compliance only. Task acceptance was not re-reviewed, and other reviewers' conclusions were not used as evidence.

## Authority coverage

| Changed surface | Applicable authority | Coverage |
|---|---|---|
| Workspace/crate manifests, lockfile, feature use, and `mise` lanes | `AGENTS.md` §§1, 4, 11-12, 15-16; `architecture/agent-rules.md`; `architecture/references/README.md`; `architecture/references/languages/testing-workflows.md`; `mise.toml`; workspace and changed crate manifests | Reviewed the pinned dependency, crate placement, feature declarations, lockfile delta, canonical task definitions, and recorded/rerun verification. |
| `wyrd-spec` OAuth request/response contracts and generated schemas | `AGENTS.md` §§2-4, 8-9, 12, 16; `architecture/wyrd-design.md` doctrine/runtime identity/error catalog; `architecture/wyrd-doctrine.mdx`; `architecture/references/architecture/patterns.md`; `rust-core.md`; `errors.md` | Reviewed the form grant enum, OAuth client/error/metadata types, API-key subject-token type, schema generator inputs, generated schema deltas, and contract tests. `REPO-TASK-010-1` applies to one new test helper. |
| Shared Rust client auth, saved credentials, platform auth, and middleware | `AGENTS.md` client ownership, secret, async, error, and testing rules; `architecture/wyrd-design.md` client/runtime identity sections; `wyrd-security-posture.md`; `patterns.md`; `rust-core.md`; `errors.md` | Reviewed OAuth form encoding, public-client identification, refresh handling, secret-bearing values, errors, async boundaries, and Rust/Python/TypeScript journey support. |
| `wyrd-auth` callback, device, refresh, revoke, issuance, API-key exchange, and audit paths | `AGENTS.md` §§3-6, 9-12, 16; `architecture/agent-rules.md` tenancy/audit/async/rustdoc rules; `architecture/wyrd-design.md` runtime identity and audit sections; `wyrd-security-posture.md`; `patterns.md`; `rust-core.md`; `errors.md` | Reviewed concrete workflow owners, transaction boundaries, tenant capability types, canonical audit append use, secret handling, conventional OAuth grant composition, and focused tests. |
| Server OAuth endpoints, route composition, configuration, boot, OpenAPI, and error rendering | Same server/security authorities plus the approved OAuth endpoint exception from Wyrd problem JSON | Reviewed authorize/callback/device/token/revoke/metadata handlers, `OAuthClients`, `TokenGrants`, configuration parsing, route registration, boot composition, tracing/error mapping, and OpenAPI coverage. `REPO-TASK-010-1` applies. |
| SQL queries, row types, migrations, and tenant transaction ownership | `architecture/agent-rules.md:6-11`; `AGENTS.md` §§3, 9, 12; `patterns.md` SQL/tenant transaction rules; `rust-core.md` SQL capability rules; changed migrations and SQL tests | Reviewed every changed auth query/migration/row type for `TenantConn` versus `OperatorPool`, raw pools, RLS dependence, caller-owned commit/rollback, and schema/query agreement. `REPO-TASK-010-2` applies. |
| CLI, server fixtures, identity/platform/CLI journeys, Python and TypeScript test support | `AGENTS.md` §11; `architecture/agent-rules.md` journey/runtime-placement rules; `spec-driven-development.md`; `implementation-execution.md`; `testing-workflows.md` | Reviewed test placement, ignored environment-owned journey setup, exact focused identity commands, language-runtime ownership, supporting integration lanes, and the task's durable verification record. |
| Docs, scripts, deleted BFF/session surfaces, and task evidence | `AGENTS.md` Wyrd-native/public-contract/completion rules; `architecture/wyrd-doctrine.mdx`; approved spec rev 11; TASK-010; TASK-004 r2 lead routing | Reviewed changed docs/scripts, legacy route/session removal, public vocabulary, generated-artifact workflow, and the explicit TASK-011 boundary for BFF/UI consumers. |

`architecture/references/README.md` routed this review to the listed architecture and language authorities. `architecture/wyrd-design.md` was applied at its doctrine, runtime-identity, client/server, audit, and error-catalog sections. Bifrost execution/storage, PyO3 implementation, and production Python or TypeScript API authorities are not changed by this range.

## Per-rule results

| Rule | Exact evidence | Result |
|---|---|---|
| Durable auth behavior remains server-owned; clients project wire contracts | Grant execution is owned by `AuthorizationCodeExchange`, `CliLogins`, refresh/revoke services, `TokenGrants`, and server route composition. Shared/language clients encode the same HTTP forms and cache only client credentials/tokens. | PASS |
| The approved API-key flow is RFC 8693 token exchange only | `crates/wyrd-spec/src/auth/token.rs:59-88,114-124` models token exchange and `urn:wyrd:oauth:token-type:api_key`; the contract test at lines 380-412 accepts that shape and rejects `grant_type=wyrd_api_key`. No runtime alias was found. | PASS |
| Stateful/dependency-backed workflows have cohesive concrete owners | `OAuthClients`, `TokenGrants`, `AuthorizationCodeExchange`, `CliLogins`, and the refresh/revoke owners hold their dependencies and expose inherent workflow methods; pure parsing and mapping remain narrow helpers. | PASS |
| Async exists only at IO or intentional IO-composition boundaries | New async functions await HTTP, SQL, token verification/issuance, or composed server operations. Form parsing, request classification, metadata construction, config parsing, and conversions remain synchronous. | PASS |
| New/materially changed Rust items carry substantive rustdoc and fallible functions carry `# Errors` | The tuple field, associated type, OAuth extractor, and test parser identified in `REPO-TASK-010-1` are missing required documentation. | **FAIL** |
| Tenant work uses `TenantConn`; cross-tenant work uses `OperatorPool`; production signatures do not propagate raw pools | Changed production query/service paths use the repository capability types. `mise run check:tenant-isolation` and `mise run check:from-pools-allowlist` both passed. | PASS |
| `TenantConn` queries rely solely on RLS rather than a parallel tenant filter | The newly added active-refresh lookup manually binds `data_tenant_id` despite accepting `&mut TenantConn<'_>`; see `REPO-TASK-010-2`. | **FAIL** |
| A `TenantConn` callee never commits or rolls back its caller-owned transaction | Changed SQL helpers operate on `conn.transaction()` but leave commit/rollback to the owning callback/grant/service layer. No violating query/helper was found. | PASS |
| Authorization decisions use the canonical audit path and required transaction boundary | Code redemption, refresh/revoke, issuance, API-key exchange, and delegation reuse the canonical auth audit append/publisher model. No second sink/table/publisher or unaudited new permission decision was introduced. | PASS |
| Secrets use secret-bearing/redacted types and are not logged or returned through diagnostic errors | Presented codes, API keys, refresh/device tokens, PKCE verifier, and client secrets use `SecretBearer`/`SecretString` or hashes. New diagnostics log stable codes/classifications rather than credential material. | PASS |
| Public errors use the catalog, with OAuth endpoints rendering their approved RFC bodies | Non-OAuth public failures remain `WyrdError`; the OAuth endpoints centrally map to RFC 6749/RFC 8628 bodies and no-store headers as explicitly allowed by the approved task/spec. | PASS |
| Dependencies/features stay in the narrow owner | `oauth2 = 5.0.0` is workspace-pinned with default features disabled and is consumed by `wyrd-server` only as a dev-dependency for the off-the-shelf journey client. No new Cargo feature or production OAuth client stack was added. | PASS |
| Generated artifacts come from source and verify cleanly | The schema generator source and generated OAuth schemas move together; the candidate records `mise run codegen:check` passing. No hand-maintained parallel OpenAPI artifact was introduced. | PASS |
| New user-facing behavior has correctly placed real-server journeys | The task records exact server, CLI, Rust, shared-client, Python, and TypeScript identity journey filters, plus principals/OpenAPI, SQL, platform, and CLI lanes. Environment-dependent journeys remain in the gated lane as required. | PASS |
| Verification uses the narrowest complete task lanes | TASK-010 records focused identity filters and the owning principals, SQL, platform, CLI, contract, docs, boundary, format, lint, Python-lint, and TypeScript-typecheck lanes. Full unfiltered journeys remain change-review work by lead direction. | PASS |
| No compatibility alias, deleted BFF route obligation, or novel security mechanism is required | No `wyrd_api_key` grant alias exists. Deleted `/internal/bff/v1` consumer repair is TASK-011. This review requires only existing RLS and rustdoc rules and introduces no mechanism/check/file/setting/option absent from RFCs or comparable projects. | PASS |

## Open questions

None.

## Verification notes

The candidate's durable task record reports successful exact identity journey filters for server, CLI, Rust, shared client, Python, and TypeScript; focused auth/server tests; `mise run test:principals:integration`; `mise run test:sql`; `mise run test:platform:journey`; `mise run test:cli:journey`; `mise run codegen:check`; `mise run docs:check`; `mise run check:client-tier`; `mise run check:unwrap-audit`; `mise run fmt`; `mise run lints`; `mise run py:lints`; `mise run ts:typecheck`; and `git diff --check`.

This independent standards pass additionally ran:

- `mise run check:tenant-isolation` — PASS
- `mise run check:from-pools-allowlist` — PASS
- `mise run check:clippy-allow-audit` — PASS
- `mise run check:unwrap-audit` — PASS
- `mise run py:format:check` — PASS
- `git diff --check fe51df0af6f6852fa7a8bc7276f3ce1d31c86daa..04366e7fc28c466fcdcbc7279885cfee82a988a2` — PASS

Per the approved review scope, full unfiltered and cross-language journeys run at change review; their absence here is not a TASK-010 gap or a basis for weakening task-review verification. No required sub-reviewer was unavailable for this report.

## Overall result

**FAIL**

The candidate violates the hard rustdoc contract and adds a forbidden manual tenant predicate on a `TenantConn` query. Import/path-shape observations and TASK-011's deleted-BFF consumer work are explicitly non-blocking and do not affect this result. No nonstandard remediation is required.
