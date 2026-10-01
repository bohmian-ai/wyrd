# Repository Standards Review — TASK-001 r3

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `a5c8041a348a66bfb56fbac492383e8c688b0590`
- Candidate: `eb4142cc95fa24b0525c71f66bfc978577cd4b7c`
- Range reviewed: the complete cumulative `base..candidate` diff, including the
  original TASK-001 implementation and both remediation rounds.
- Candidate identity at review start and completion:
  `eb4142cc95fa24b0525c71f66bfc978577cd4b7c`.
- CodeGraph: not used because the repository has no `.codegraph/` directory.

This report reviews repository standards only. It does not use the current
`task-rev` conclusions and does not decide task acceptance.

## Authority coverage

| Changed surface | Applicable authority read | Evidence inspected | Coverage |
|---|---|---|---|
| Repository-wide Rust structure, dependencies, rustdoc, and verification | `AGENTS.md`; `architecture/agent-rules.md`; `architecture/references/languages/spec-driven-development.md`; `architecture/references/languages/implementation-execution.md`; `architecture/references/languages/rust-core.md` | Complete cumulative diff, manifests/lockfile, `mise.toml`, changed Rust owners and tests | COMPLETE |
| Public human-connection contracts, validation, schemas, and stable errors | `architecture/wyrd-design.md` (doctrine, public surfaces, runtime identity); `architecture/wyrd-doctrine.mdx`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/errors.md`; `architecture/references/languages/agent-harness.md` | `wyrd-spec/src/auth/human_connection.rs`, `wyrd-spec/src/error.rs`, server route/OpenAPI registration, generated TypeScript error-code projection | COMPLETE |
| Human OIDC trust, SSRF, provider IO, credentials, and secret rotation | `architecture/wyrd-security-posture.md`; `architecture/agent-rules.md`; Rust-core external-URL and secret rules | `wyrd-auth-oidc` screening/discovery/JWKS, `wyrd-auth` connection/callback/resolver/sealing owners, `wyrd-crypt`, server configuration and boot | COMPLETE |
| Tenant authorization, canonical audit, and HTTP administration | `AGENTS.md` §§2, 3, 5, 9, 16; security posture authorization/audit rules; architecture patterns server/audit rules; agent-harness audit rules | `components/admin/identity.rs`, auth/server assembly, permission additions, audit call paths and focused audit tests | COMPLETE |
| Tenant persistence, migration, RLS, refresh binding, and operator maintenance | `architecture/agent-rules.md` SQL/RLS/transaction rules; `AGENTS.md` §§3, 9, 15; architecture patterns storage rules; security posture tenant/data isolation | migration, human-connection/login-state/refresh-token SQL, row types, `PgIssuerResolver`, rewrap queries, migration tests | COMPLETE |
| Real-server journeys, fixtures, OpenAPI proof, and test lanes | `AGENTS.md` §§11–12, 16; `architecture/agent-rules.md`; `architecture/references/languages/testing-workflows.md` | `identity_e2e.rs`, `pg_openapi_contract.rs`, `pg_migration.rs`, `wyrd-testing`, `wyrd-dev-fixtures`, identity and principal tasks in `mise.toml` | COMPLETE |
| Public authentication documentation | `AGENTS.md` §§1–2, 11–12; `architecture/wyrd-doctrine.mdx`; `architecture/wyrd-security-posture.md`; approved specification | Four changed authentication/identity documentation pages and their statements about optional OIDC, connection lifecycle, secret rotation, and recovery | COMPLETE |
| TypeScript generated error projection | `AGENTS.md` §§8–9, 11; errors reference; TypeScript guide | derive-backed Rust error additions and `sdks/wyrd-sdk-ts/wyrd/src/error-codes.ts` | COMPLETE |
| Python, PyO3, UI, MCP, and Bifrost | Governing authorities checked for applicability | No materially changed implementation surface in the candidate | NOT APPLICABLE |

## Applicable-rule results

| Rule | Result | Exact evidence |
|---|---|---|
| Durable contracts stay in `wyrd-spec`; server/auth and SQL owners retain durable behavior | PASS | Typed connection contracts live in `crates/wyrd-spec/src/auth/human_connection.rs`; lifecycle orchestration is owned by `wyrd_auth::connections::HumanConnections`; SQL and migration behavior remain in `wyrd-sql`. |
| `wyrd-spec` remains IO-free, async-free, database-free, and PyO3-free | PASS | The new contract module performs synchronous decoding/validation only and introduces no server, runtime, SQL, network, or PyO3 dependency. |
| Stateful workflows use cohesive concrete owners rather than threaded free-function orchestration | PASS | `HumanConnections`, `SealedSecretRewrap`, `SealingKeyring`, and `ScreenedHttp` own their dependencies and expose inherent workflow methods; pure decoding and classification remain narrow helpers. |
| Tenant SQL uses `TenantConn`, relies on RLS, and callees do not commit or roll back | PASS | Human-connection, login-state, and refresh-token query functions take `&mut TenantConn<'_>`; the migration enables and forces RLS; lifecycle owners retain transaction completion. |
| Cross-tenant maintenance uses the explicit operator capability | PASS | Sealed-secret inventory and CAS rewrap use `OperatorPool`; tenant request handlers do not receive it. |
| Raw application pools do not escape sanctioned owners | PASS | `HumanConnections`, `PgLoginStateStore`, and the materially changed `PgIssuerResolver` own `WyrdPostgres`; the resolver acquires via `WyrdPostgres::tenant_conn`. Remaining raw pools are sanctioned construction/fixture boundaries or untouched owners. |
| Tenant identity derives from verified caller state | PASS | Every administration handler uses `caller.data_tenant_id`; request bodies and paths do not supply the effective tenant. Login selection is subsequently bound to server-owned, single-use state. |
| Each permission evaluation produces one canonical audit row at its commit boundary | PASS | Candidate testing performs and records the pre-probe decision, then performs a fresh `identity.oidc.candidate.tested` decision whose event commits with the stamp; mutation handlers pass their evaluated event into the tenant transaction. Denials and audit failures are exercised by focused server tests. |
| Public routes use versioned typed contracts, structured Wyrd errors, and scrubbed trace instrumentation | PASS | `components/admin/identity.rs` registers `/v1/identity/oidc/*`, advertises `ConnectionInput`/typed responses, returns `WyrdErrorResponse`, and instruments each handler with `skip_all`. Candidate PUT uses bounded raw extraction only to preserve authorization-before-decoding, then immediately decodes through the typed `ConnectionInput` owner. |
| Public cross-boundary errors use the derive-backed catalog and generated projections | PASS | New variants and metadata are in `wyrd-spec/src/error.rs`; HTTP uses the single Wyrd mapper; the TypeScript literal union contains the generated codes. No parallel problem-json implementation was added. |
| Provider URL IO is scheme-checked, resolved, screened, pinned, redirect-safe, proxy-free, timed, and body-bounded | PASS | `ScreenedHttp::client_for` enforces HTTPS under `BlockInternal`, screens all resolved addresses, pins them with `resolve_to_addrs`, refuses redirects, disables ambient proxies, and applies a timeout. Discovery, JWKS, candidate probes, and token exchange use `read_bounded_body` with a 1 MiB decoded ceiling. |
| Secrets remain redacted, sealed at rest, and loaded from restrictive files | PASS | Secret inputs use redacted wrappers; `SealingKeyring` has redacted `Debug`; connection views omit secrets; active and retained key files use `read_secret_file`; boot rewrap is CAS-based and keyless boot refuses stored ciphertext. |
| No tenant-scoped provider or refresh state is widened across tenants or replacement revisions | PASS | Human connections and refresh families use typed connection/revision bindings under RLS; provenance-free legacy human refresh rows remain unbound; refresh issuance rechecks the active exact binding. |
| All new or materially modified Rust items have substantive rustdoc, including `# Errors` on fallible functions | **FAIL — STD-R3-001** | The new fallible test helper `screening::tests::bounded_get` returns `Result<Vec<u8>, BodyError>` at `crates/shared/wyrd-auth-oidc/src/screening.rs:481-496` but documents only `# Panics`; it has no required `# Errors` section. |
| Imports are module-scoped and signatures use imported bare names | PASS | The prior ordinary function-local `wiremock` imports were moved to the test module import block; materially changed signatures use top-level imports and bare type names. |
| Async remains limited to operations that await IO or compose it | PASS | Provider, SQL, server-handler, and test-server async functions directly await network/database/process IO; validation, decoding, URL qualification, and crypto transformations remain synchronous. |
| New user-facing HTTP behavior has real-server journey and served-OpenAPI coverage | PASS | `identity_e2e.rs` covers tenant administration, rotation, session cutoff, and cross-replica behavior through `WyrdTestServer`; `pg_openapi_contract.rs` checks the assembled server contract. |
| Postgres migration behavior remains outside the fast lane and has upgrade/preflight coverage | PASS | `wyrd-sql/tests/pg_migration.rs` uses the isolated unmigrated-database fixture and covers partial state, RLS, legacy refresh provenance, and post-migration behavior. |
| Test tasks avoid an unnecessary all-feature union | PASS | The identity journey task lists and runs `wyrd-server --test identity_e2e` with default features; `--all-features` is not forced into that journey lane. |
| No gate was weakened to hide a violation | PASS | The from-pools check removes only the nonexistent `python/` search root; it continues scanning `crates/`, its live protected surface. No lint allowance, ignored required test, weakened assertion, or compatibility route entered the diff. |
| Generated artifacts and documentation have the required recorded verification | PASS WITH EVIDENCE LIMIT | The committed task records report green `codegen:check` and `docs:check`; the generated TypeScript union agrees with the derive catalog and documentation agrees with the implemented contract. This static reviewer did not rerun those lanes. |

## Material repository-rule findings

### STD-R3-001 — New fallible OIDC test helper omits required error documentation

- **Violated rule:** `AGENTS.md` §16 and `architecture/agent-rules.md` require
  every new or materially modified Rust item, including test helpers, to carry
  substantive rustdoc, and every fallible function to include `# Errors`.
- **Location:** `crates/shared/wyrd-auth-oidc/src/screening.rs:481-496`.
- **Evidence:** `bounded_get` is new in the R2 remediation, returns
  `Result<Vec<u8>, BodyError>`, and propagates `read_bounded_body(response)`.
  Its rustdoc contains a `# Panics` section for URL/client/request fixture
  failures but no `# Errors` section for `BodyError::TooLarge` or
  `BodyError::Read`.
- **Consequence:** The candidate misses a hard repository acceptance rule on a
  changed Rust item even though ordinary public-item rustdoc or Clippy lanes may
  not detect a private test helper omission.
- **Testable correction:** Add a `# Errors` section stating that the helper
  returns `BodyError::TooLarge` when the decoded provider body exceeds the
  shared cap and `BodyError::Read` when response transfer/decoding fails. Run
  format/lints and retain the four focused bounded-body tests; no new helper or
  test is needed.

## Verification limits

- This was a static inspection of the complete immutable cumulative range and
  surrounding live owners/callers. Per assignment, no Cargo-backed or `mise`
  Cargo lane was run.
- `git diff --check base..candidate` completed successfully.
- The TASK-001-R2 implementation record reports all focused tests, three
  filtered and one unfiltered identity journey, principals/OpenAPI integration,
  SQL, platform journeys, codegen, tenant/pool/client/PyO3/unwrap boundaries,
  docs, format, lints, and diff checks green. Those command results were
  available evidence but were not independently re-executed in this review.
- Provider qualification against controlled external Okta, Keycloak, and Entra
  accounts is outside this bounded TASK-001 static review and is not inferred
  from local mock-provider evidence.
- No applicable authority or changed source surface was unavailable.

## Overall result

**FAIL**

The cumulative candidate conforms to the reviewed ownership, tenancy, audit,
security, contract, testing, and verification-shape rules, but the missing
mandatory `# Errors` rustdoc on one newly added fallible test helper is a hard
repository-standard violation. The correction is bounded and requires no
specification or architecture decision.
