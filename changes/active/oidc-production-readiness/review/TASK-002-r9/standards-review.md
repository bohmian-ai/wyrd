# Repository Standards Review — TASK-002 r9

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `fa2bda92a7e79471b79b607870c1e86a9f35639c`
- Range reviewed: the complete `base..candidate` diff, including the original
  task and all eight remediation tasks.
- Candidate identity was confirmed before review and matched `HEAD`.

## Authority coverage

| Changed surface | Applicable authority read | Coverage and result |
|---|---|---|
| Active task/remediation records and implementation evidence | `AGENTS.md` §§12–16; `architecture/agent-rules.md`; `architecture/references/languages/spec-driven-development.md`; `architecture/references/languages/implementation-execution.md` | Packet remains tracked and cumulative, remediation evidence names focused commands, and no task/agent vocabulary entered executable code. **PASS** |
| Shared OIDC/JWT verification (`wyrd-auth-verify`) | `AGENTS.md` §§3–6, 9, 16; `architecture/wyrd-security-posture.md` (federation, token validation, secrets); `architecture/references/languages/rust-core.md`; `architecture/references/languages/errors.md`; `architecture/references/architecture/patterns.md` | Verification remains on the shared Rust owner; OIDC-only `iat` and Subject Identifier rules do not alter the workload verifier; symmetric algorithms, issuer, audience, signature, key, expiry, and `nbf` still fail closed. Added logic and tests are documented and synchronous except for awaited JWKS IO. **PASS** |
| Shared Rust client and CLI auth surface | `AGENTS.md` §§2–4, 9, 13, 16; `architecture/wyrd-design.md` (client model/runtime identity); `architecture/wyrd-doctrine.mdx`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/rust-core.md`; `architecture/references/languages/errors.md`; `architecture/references/languages/agent-harness.md` | Client transport remains in `wyrd-client`; the retired authorization-code exchange and CLI login do not leave a compatibility path; refresh credentials stay out of argv and redacted types are retained. CLI rustdoc now describes the actual token-output helper. **PASS** |
| Public auth contracts, schema generator, JSON schemas, and schema goldens (`wyrd-spec`) | `AGENTS.md` §§2–4, 8–9, 12, 16; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/references/doctrine/architecture-constraints.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/errors.md`; `architecture/references/languages/agent-harness.md` | Contracts are typed, `wyrd-spec` remains IO/async/PyO3-free, `Sha256Hex` has one validated wire form, generated schemas project the source types, and the retired token grant is absent from generated contracts. Recorded `codegen:check` is green. **PASS** |
| Tenant auth owners: login, callback, connection, issuance, refresh, revocation, audit, sealing, and resolvers (`wyrd-auth`) | `AGENTS.md` §§4–6, 9, 15–16; `architecture/agent-rules.md` (SQL capabilities, transaction ownership, audit, imports, rustdoc); `architecture/wyrd-design.md` (runtime identity); `architecture/wyrd-security-posture.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/rust-core.md`; `architecture/references/languages/errors.md` | Cohesive concrete owners (`HumanConnections`, `AuthorizationCodeExchange`, `TenantTokenIssuer`, `RefreshTokens`) retain the workflows. Tenant work uses `TenantConn`; callees do not terminate caller transactions. Login/role/issuance audit remains canonical and fail-closed. Refresh-family and connection-slot locks have one order and deterministic overlap tests. Secrets use redacted wrappers. **PASS** |
| HTTP routing, callback adapter, OpenAPI registration, and server tests (`wyrd-server`) | `AGENTS.md` §§5–6, 9, 11–12, 16; `architecture/agent-rules.md`; `architecture/wyrd-security-posture.md`; `architecture/references/architecture/patterns.md` (server pattern); `architecture/references/languages/rust-core.md`; `architecture/references/languages/testing-workflows.md`; `architecture/references/languages/errors.md`; `architecture/references/languages/agent-harness.md`; `TESTING.md` | Login and callback handlers use typed bodies, stable errors, fixed redirects, scrubbed callback/login tracing, and assembled-server OpenAPI proof. The real-server identity suite covers the changed user journey and negative paths. The materially changed token handler lacks the required trace boundary (**REPO-001**), and the materially changed router constructor retains placeholder rustdoc without its panic contract (**REPO-002**). **FAIL** |
| Durable SQL, migration, RLS state machine, advisory locks, and Postgres integration tests (`wyrd-sql`) | `AGENTS.md` §§3, 11, 15–16; `architecture/agent-rules.md` (TenantConn/RLS/caller-owned transaction); `architecture/wyrd-security-posture.md` (tenant isolation); `architecture/references/architecture/patterns.md`; `architecture/references/languages/rust-core.md`; `architecture/references/languages/testing-workflows.md`; `TESTING.md` | New login-state transitions rely on forced RLS rather than manual tenant predicates; the narrow cross-tenant state lookup is an inherent `WyrdPostgres` operation; database coordination uses `statement_timestamp()`; external `pg_login_state` tests earn their location through real Postgres. **PASS** |
| Gateway test-only repair | `AGENTS.md` §§11, 16; `architecture/references/languages/rust-core.md`; `architecture/references/languages/testing-workflows.md`; `TESTING.md` | JSON comparison is structural and stays test-only; no production or contract behavior changed. **PASS** |
| Docs (`docs/`) | `AGENTS.md` §§1–2, 9, 11–12; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/wyrd-security-posture.md`; `architecture/references/languages/agent-harness.md` | Docs describe the common callback, one-active-connection and bounded-revocation behavior, remove the retired CLI login, and do not expose tokens or provider secrets. Recorded `docs:check` is green. **PASS** |
| Identity fixtures, Docker composition, and `mise.toml` lanes | `AGENTS.md` §§11–12, 15; `architecture/references/languages/testing-workflows.md`; `architecture/references/languages/implementation-execution.md`; `TESTING.md` | The second Keycloak realm and lane registration support the required multi-provider/multi-tenant journeys; tests remain gated, repository-managed, sequential, and credential-free. No new dependency or Cargo feature was added. **PASS** |

## Applicable rule results

| Rule | Evidence | Result |
|---|---|---|
| Server owns durable identity, tenancy, role mapping, issuance, and audit | `wyrd-auth` and `wyrd-server` own all durable behavior; client/CLI changes only project or retire wire calls. | PASS |
| Tenant identity is credential/state-derived and SQL is RLS-bound | Callback resolves only the opaque state hash through `WyrdPostgres::login_state_tenant`, then opens `TenantConn`; login-state transitions contain no tenant-selection predicate. | PASS |
| Caller owns `TenantConn` commit/rollback | SQL/auth callees operate through the supplied transaction; commits remain at route/service boundaries. | PASS |
| Audited authorization and security mutations are canonical, transactional, and fail closed | Role synchronization and token exchange append through the existing auth-audit owner in the issuance transaction; audit-failure tests prove rollback. | PASS |
| Provider IO is screened and bounded | Login/callback reuse `ScreenedHttp` discovery, token, and JWKS paths; no raw server fetch was introduced. | PASS |
| Secrets are redacted and absent from generated/public outputs | Client secret and PKCE verifier use secret wrappers; callback and fixed completion responses contain no provider code or Wyrd token; sentinel debug test is present. | PASS |
| Struct-centered Rust ownership | Stateful workflows remain methods on existing concrete owners; new free functions are narrow deterministic validation/conversion or HTTP adapters around those owners. | PASS |
| Async is limited to IO composition | Validation and claim checks remain synchronous; async functions await HTTP, SQL, or composed IO. | PASS |
| Public errors and HTTP projection remain stable | Public handlers use `WyrdErrorResponse`; contracts use the existing derive-backed catalog; no parallel problem-json or surface-specific error catalog was added. | PASS |
| New/materially changed Rust items have substantive rustdoc, including panic contracts | Most touched items and tests were brought into compliance, but `auth_router` was materially changed and still has only `Build auth routes.` while containing an `expect` with no `# Panics`. | **FAIL — REPO-002** |
| Write handlers carry scrubbed `#[tracing::instrument]` | New login and changed callback handlers are instrumented with scrubbed arguments; the materially changed `/auth/token` write handler is not instrumented. | **FAIL — REPO-001** |
| Public contracts and generated artifacts share one source | `BeginLogin`, `BeginLoginResponse`, callback and token shapes flow from `wyrd-spec`; source and both schema trees move together; recorded `codegen:check` passes. | PASS |
| New user-facing behavior has real-server journey proof | Four ignored, lane-owned identity journeys exercise human login, trust-boundary refusals, provider switch/renewal, and machine independence against a real server/Postgres/provider setup. | PASS |
| Integration and unit test placement follows tier rules | Postgres tests are external/`pg_tests`; pure validation tests remain in-module; no production credentials or synthetic host load are required. | PASS |
| No gate circumvention | No new lint allow, weakened/deleted required assertion, or ignored fast-lane test used to conceal a failure; journey ignores correspond to the gated identity lane. | PASS |
| Verification uses repository `mise` lanes and exact named selectors | The task/remediation evidence records exact `mise exec -- cargo nextest` selectors plus identity, principals, SQL, codegen, boundary, docs, format, and lint lanes. | PASS |
| No compatibility/legacy or commercial-distribution surface | The obsolete authorization-code token grant, GET login route, and CLI login path are removed without aliases; no hosted-signup or edition stub was added. | PASS |

## Material findings

### REPO-001 — Materially changed token write handler is not trace-instrumented

- **Violated rule:** `AGENTS.md` §9 requires write handlers to carry
  `#[tracing::instrument]` with scrubbed arguments; `architecture/references/architecture/patterns.md`
  repeats the server pattern.
- **Location:** `crates/wyrd/wyrd-server/src/components/auth/routes.rs:111`
- **Evidence:** `token` is the public `POST /auth/token` handler. It performs
  API-key, refresh, workload, and delegated issuance and commits durable audit
  and refresh state. The cumulative candidate materially changes this handler
  by retiring the authorization-code grant, but no `#[tracing::instrument]`
  attribute precedes it. The changed login and callback handlers do carry
  scrubbed instrumentation, demonstrating the applicable local pattern.
- **Consequence:** token-exchange requests bypass the repository-mandated
  handler span, leaving the changed credential-issuance boundary without the
  consistent request-level trace context expected for diagnosis and audit
  correlation. Adding default instrumentation without scrubbing would also
  risk recording credential-bearing request debug data.
- **Testable correction:** add the existing minimal handler pattern immediately
  above `token`, using `#[tracing::instrument(level = "debug", skip_all)]` (or
  an equivalently scrubbed attribute that records only non-secret typed fields).
  Do not log `TokenRequest`, headers, API keys, assertions, access tokens, or
  refresh tokens. Prove by source inspection, `mise run fmt`, `mise run lints`,
  and the existing principals/identity route lanes; no new test harness is
  needed.

### REPO-002 — Materially changed router constructor retains placeholder rustdoc and omits its panic contract

- **Violated rule:** `AGENTS.md` §16 and `architecture/agent-rules.md` require
  substantive rustdoc for every materially modified Rust item and `# Panics`
  whenever a panic remains possible; placeholder text that merely restates the
  symbol is explicitly insufficient.
- **Location:** `crates/wyrd/wyrd-server/src/components/auth/routes.rs:40-47`
- **Evidence:** the cumulative diff materially changes `auth_router` by mounting
  the new `POST /auth/login` operation alongside callback, token, and issue-key
  routes under a shared governor. Its entire rustdoc remains `Build auth
  routes.` It does not explain that composition or the shared admission limit,
  and it omits the `# Panics` contract for
  `GovernorConfigBuilder::finish().expect("static auth governor config is valid")`.
- **Consequence:** the public composition owner fails the repository's hard
  documentation gate and does not tell maintainers that changing the static
  governor settings can make route construction panic.
- **Testable correction:** revise only `auth_router`'s rustdoc to describe the
  four mounted auth surfaces and their shared rate-governor role, and add a
  `# Panics` section naming rejection of the static governor configuration.
  Preserve its implementation. Prove by source inspection, `mise run fmt`,
  `mise run lints`, and `git diff --check`; no runtime test is warranted for a
  documentation-only correction.

## Verification limits

- Independently confirmed both commit identities, `HEAD == candidate`, the
  complete changed-file set, and `git diff --check base..candidate` (pass).
- Inspected the complete cumulative source diff, surrounding owners, migration,
  schemas, docs, test registration, and `mise.toml` changes. No `.codegraph/`
  index exists, so repository inspection used Git and `rg` as directed.
- Did not start Cargo-backed commands during this review because the shared
  checkout may have concurrent reviewers and repository guidance forbids
  overlapping Cargo work. The candidate's task/remediation evidence records
  successful focused selectors and the complete identity, principals, SQL,
  codegen, tenant-isolation, client-tier, docs, format, lint, and diff checks.
- Green verification does not close the two source-rule failures above: neither
  handler instrumentation nor substantive rustdoc is established by runtime
  behavior alone.

## Overall result

**FAIL**

The security, tenancy, persistence, contracts, generated artifacts, docs, and
test topology conform to the applicable repository authorities. Acceptance is
blocked only by `REPO-001` and `REPO-002`, both bounded to
`crates/wyrd/wyrd-server/src/components/auth/routes.rs`.
