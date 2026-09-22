# Repository standards review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate: `261168087376095fa5ad9d66946e755f3baa8fe4`
- Scope: the complete base-to-candidate diff, including the user-authorized test,
  production, documentation, and gate corrections.
- Accepted constraint: the five-minute stateless tenant-JWT revocation window is
  not a finding.

The candidate commit remained unchanged throughout this review. The shared
worktree acquired unrelated uncommitted `mise.toml` edits during review, so all
source judgments use the immutable commit objects rather than those worktree
edits.

## Authority coverage

| Changed surface | Governing authority read and applied | Coverage result |
|---|---|---|
| Tenant and platform principal, credential, JWT, refresh, OIDC, delegation, RBAC, and audit code | `AGENTS.md` §§2–6, 9, 11–12; `architecture/agent-rules.md`; `architecture/wyrd-design.md` Doctrine 18 and Runtime identity; `architecture/wyrd-security-posture.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/rust-core.md`; `architecture/references/languages/errors.md` | PASS |
| Tenant/platform SQL, migrations, RLS, transaction boundaries, and audit staging | `AGENTS.md` §§2–4, 9; `architecture/agent-rules.md` SQL, RLS, audit, and transaction rules; `architecture/wyrd-security-posture.md` Tenant and data isolation / Audit integrity | PASS |
| Bifrost HTTP/gRPC admission, scoped permissions, Gate audit, and audit publication | `AGENTS.md` §§2–3, 9–12; `architecture/bifrost-design.md`; `architecture/wyrd-design.md` Bifrost; `architecture/references/domain/olap-serving.md`; `architecture/references/architecture/patterns.md` | PASS |
| Shared Rust client plus Rust SDK projection | `AGENTS.md` §§2–6, 9, 11–12; `architecture/wyrd-design.md` Client model; `architecture/references/architecture/patterns.md`; `architecture/references/languages/rust-core.md` | PASS |
| Python SDK, PyO3 client composition, package exports, stubs, and journeys | `AGENTS.md` §§2–8, 11–12; `architecture/references/languages/pyo3-boundaries.md`; `architecture/references/languages/python-api-and-stubs.md`; `architecture/references/languages/testing-workflows.md` | PASS |
| TypeScript SDK, N-API client composition, declarations, errors, and journeys | `AGENTS.md` §§2–6, 9, 11–12; `architecture/references/languages/typescript-guide.md`; `architecture/references/languages/testing-workflows.md`; `architecture/references/languages/errors.md` | PASS |
| HTTP/OpenAPI, CLI, MCP, generated schemas, structured errors, and public documentation | `AGENTS.md` §§2, 8–12; `architecture/wyrd-doctrine.mdx`; `architecture/references/languages/agent-harness.md`; `architecture/references/languages/errors.md`; `architecture/references/languages/spec-driven-development.md` | PASS |
| Test harness, family lanes, identity-provider lane, Python toolchain, and aggregate gate | `AGENTS.md` §§11–12; `architecture/agent-rules.md` test/gate rules; `architecture/references/languages/testing-workflows.md`; `architecture/references/languages/implementation-execution.md` | PASS |

## Applicable rule results

| Rule | Source evidence | Verification evidence | Result |
|---|---|---|---|
| Server owns identity, authorization, tenancy, policy, audit, and durable behavior | Delegation verifies both signed tokens, constructs the directed invoke-policy question, and commits its decision in `wyrd-auth` (`crates/wyrd/wyrd-auth/src/exchange_api_key.rs:240`); issuance remains on the single `TenantTokenIssuer` (`crates/wyrd/wyrd-auth/src/issuance.rs:292`). | Focused exchange selectors and the production token-exchange journey recorded in the R11 evidence; cumulative `test:wyrd`, `test:sql`, and gate passed. | PASS |
| RFC 8693 identity and authority are fail-closed and do not invent a second permission plane | The subject becomes the issued principal, the actor becomes outer `act`, and authority is narrowed to the subject/actor intersection (`exchange_api_key.rs:301`, `issuance.rs:332`); the obsolete preview gate is absent and retired config is rejected (`crates/wyrd/wyrd-server/src/config.rs:3914`). | Directed A→B allow / B→A deny journey and production-valid exchange selector passed. | PASS |
| Audit names the permission actually evaluated, uses the canonical path, and fails closed | Delegation records `invoke` while retaining operation `auth.token.exchange`; Gate adds the existing delegation-attribution detail and commits through the tenant-scoped canonical append (`crates/wyrd/wyrd-server/src/bifrost/gate_audit.rs:27`). | Allowed, denied, allowed-then-refused, audit-failure, and delegated native-ingest proofs recorded positive selections; gate passed. | PASS |
| SQL uses the sanctioned tenant/operator capabilities and preserves caller-owned tenant transactions | New issuance and authorization work takes `TenantConn`; Gate obtains a tenant connection through `ServerPostgres` and owns the decision transaction at the composition boundary. No new raw-pool domain signature was introduced. | Reproduced `check:tenant-isolation` and `check:from-pools-allowlist`: exit 0; reported full gate also passed. | PASS |
| Secrets are redacted and CLI credentials do not enter argv or derived debug output | Secret-bearing Rust paths use `SecretString`; the assembled Clap tree contains no secret-valued argument and explicitly rejects former spellings (`crates/wyrd/wyrd-cli/src/cli.rs:113`). Live docs contain no `--token`, `--api-key`, `--credential`, `--client-secret`, or `--refresh-token` invocation. | CLI structural selectors and secret-source journeys passed; cumulative docs gate passed. | PASS |
| One shared Rust client owns exchange, bearer renewal, HTTP, and gRPC; language bindings remain projections | Rust owns `WyrdClient::on_behalf_of`; Python passes the wrapped client to `wyrd_client::Bifrost`, and TypeScript's N-API layer calls the same Rust Bifrost composition without exposing tokens or building a second transport (`sdks/wyrd-sdk-python/src/bifrost/mod.rs:263`, `sdks/wyrd-sdk-ts/native/src/client.rs:102`). | Reproduced `check:client-tier` and `check:pyo3-scope`: pass. Python and TypeScript delegated-client journeys passed. | PASS |
| Public Python and TypeScript surfaces are typed, registered, generated, and exercised in their own runtimes | Python exposes the public `WyrdClient` and optional `Bifrost(client=...)`; TypeScript exposes the mutually exclusive `Bifrost.connect({ client })` union while retaining zero-argument environment resolution. | Reproduced `codegen:check` and `ts:napi:check`: pass. Reported Python integration 57/57 and TypeScript integration 18/18 pass. | PASS |
| Public contracts use typed domain values and stable generated errors | `IssueKeyResponse.key_id` is `Uuid`; preview-only error/config residue is removed; HTTP/Python/TypeScript projections continue to use the central Wyrd error catalog. | Served OpenAPI UUID selector, codegen, error checks, and gate passed. | PASS |
| New/materially modified Rust follows the required struct-centered shape, async boundary, and documentation contract | Stateful workflows remain inherent methods on `TenantTokenIssuer`, `DelegateToken`, `WyrdClient`, Bifrost, and the server composition handles; async methods await network/database IO; R9/R10/R11-touched items carry intent and applicable `# Errors`/`# Panics` contracts. | Reported strict `check:docs`, lints, and focused selectors pass. No new single-implementation trait, utility struct, or parallel owner was found. | PASS |
| User-visible Rust, Python, TypeScript, CLI, MCP, and Bifrost behavior has real journey coverage | The candidate contains real server-backed delegation, credential, platform, CLI, Python, TypeScript, MCP, HTTP, and Bifrost journeys; environment gates that previously returned without exercising behavior were removed, while external-identity tests are visibly ignored in family lanes and explicitly run by the identity lane. | Full `mise run gate` reported green; identity 20/20, Python integration 57/57, TypeScript integration 18/18, CLI 33/33, and all Bifrost ignored journeys were actually selected. | PASS |
| Generated artifacts and boundary checks remain drift-free | Schemas/stubs/declarations are generated from owners; no compatibility route, alternate audit sink, delegation store, permission, role, or transport was added. | Reproduced `codegen:check`, `ts:napi:check`, `check:client-tier`, `check:pyo3-scope`, `check:mocks-scope`, `check:tenant-isolation`, and `git diff --check`: pass. | PASS |

## Material repository-rule findings

None.

The additional test and production changes are within the user's explicit
override and were reviewed for actual rule violations rather than rejected for
scope alone; no material standards regression was found.

## Verification limits

- I did not rerun the complete aggregate gate; I reviewed its recorded green
  result and positive lane/test counts, then independently reran the boundary
  and generation checks listed above.
- `check:from-pools-allowlist` exited successfully but emitted its pre-existing
  `rg: python/: No such file or directory` diagnostic after scanning `crates/`;
  the candidate does not modify that script or add a raw-pool call site.
- The accepted five-minute JWT revocation window was intentionally excluded.

## Overall result

**PASS** — every materially changed surface is covered by its applicable
repository authority, all applicable rules pass, and this review proposes no
repository-standards finding.
