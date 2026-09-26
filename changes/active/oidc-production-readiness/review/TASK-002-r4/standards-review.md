# Repository standards review — TASK-002-r4

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `63e6a545db156a095b670a5bb8bc36f6f36fba32`
- Reviewed range: the complete cumulative base-to-candidate diff (75 files;
  task and prior-review artifacts, Rust contracts/client/auth/server/SQL,
  migration and generated schemas, docs, fixtures, tests, Docker, and mise
  configuration).
- Approved authority: `changes/active/oidc-production-readiness/spec.md`,
  revision 4; original task:
  `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`.
- Prior review/remediation artifacts under `TASK-002-r1`, `TASK-002-r2`, and
  `TASK-002-r3` were included. Lead-directed reuse cleanups and test commits
  recorded in the evidence tables were treated as authorized and still
  audited for repository compliance and regression.

`HEAD` matched the candidate before inspection. The repository has no
`.codegraph/` directory, so inspection used Git, `rg`, and source directly.

## Authority coverage

| Changed surface / language / layer | Applicable authority | Coverage and result |
|---|---|---|
| Change packet, task evidence, prior remediation, and review records | `AGENTS.md` §§12, 14–16; `architecture/agent-rules.md`; `architecture/references/languages/spec-driven-development.md`; `implementation-execution.md` | The approved revision, original task, three remediation rounds, immutable identities, evidence, and authorized follow-ups are recorded. No task artifact is used as a substitute for repository authority. **PASS**. |
| Rust public auth contracts and generated JSON schemas | `AGENTS.md` §§2–4, 9, 12, 16; `wyrd-design.md`; `wyrd-doctrine.mdx`; `architecture-constraints.md`; `patterns.md`; `rust-core.md`; `agent-harness.md` | Typed begin-login, initiation, callback, and token contracts remain in PyO3/IO/async-free `wyrd-spec`; schemas and golden copies project those sources. **PASS**. |
| Shared Rust client and CLI | `AGENTS.md` §§2–3, 9; `wyrd-design.md`; `wyrd-doctrine.mdx`; `architecture-constraints.md`; `patterns.md`; `errors.md` | The shared client projects the server contract. The obsolete direct authorization-code exchange and premature CLI login were removed rather than retained as compatibility paths. No Python or TypeScript implementation was changed or duplicated. **PASS**. |
| Tenant OIDC service and provider trust boundary | `AGENTS.md` §§4–6, 9–10, 16; `architecture/agent-rules.md`; `wyrd-security-posture.md`; `wyrd-design.md` runtime identity; `patterns.md`; `rust-core.md`; `errors.md` | Stateful login/callback behavior remains on `HumanConnections` and `AuthorizationCodeExchange`; provider IO uses screened owners; validation is synchronous where pure; failures use existing structured Wyrd errors. The R3 documentation defect is corrected accurately at `callback.rs:566-604`. **PASS**. |
| HTTP server routes, callback response, OpenAPI, and audit | `AGENTS.md` §§2, 9, 11, 16; `wyrd-security-posture.md`; `patterns.md`; `agent-harness.md`; `errors.md`; `testing-workflows.md` | Routes accept typed bodies, derive authority from verified/server-owned state, return the fixed browser/CLI response shape, retain the single response mapper, and use the canonical transactional audit append. Served OpenAPI has an assembled-server contract proof. **PASS**. |
| Postgres migration, RLS, transactions, durability, and concurrency | `AGENTS.md` §§2, 9, 15–16; `architecture/agent-rules.md`; `wyrd-security-posture.md`; `architecture-constraints.md`; `patterns.md`; `rust-core.md` | Tenant transitions accept `TenantConn`, rely on forced RLS, and leave commit ownership to callers. The sole cross-tenant lookup is the narrow inherent `WyrdPostgres::login_state_tenant` operation; the definer function is least-disclosure and execution-restricted. **PASS**. |
| Rust structure, imports, rustdoc, async, errors, and panic contracts | `AGENTS.md` §§4–6, 15–16; `architecture/agent-rules.md`; `rust-core.md`; `errors.md` | Cumulative owners are cohesive concrete structs, pure helpers remain free and synchronous, imports are module-top, fallible and panicking touched items are substantively documented, and production code adds no unchecked external-input `unwrap`. **PASS**. |
| Unit, integration, Postgres, served-contract, and user-journey tests | `AGENTS.md` §11 and §16; `architecture/agent-rules.md`; `testing-workflows.md`; `spec-driven-development.md` | Tests are placed by runtime/dependency tier. Login, refusal, provider-switch, and machine-independence journeys use the real server. The new advertised-`HS256` case proves the shared verifier guard without a new harness. The gateway helper still asserts the same JSON while avoiding key-order coupling. **PASS**. |
| Generated artifacts, docs, fixtures, Docker, and mise tooling | `AGENTS.md` §§1, 11–12, 15–16; `agent-harness.md`; `testing-workflows.md`; `wyrd-doctrine.mdx` | Schema sources/generator and both generated trees agree; OpenAPI is tested at runtime; docs match the retired grant and current server contract; identity fixtures and selectors remain test-only. No dependency, Cargo feature, credential, or alternate harness was added. **PASS**. |
| Python, TypeScript, PyO3, UI, MCP, and Bifrost | Corresponding `AGENTS.md` and routed authorities | No files or public implementation in these layers changed. Contract-generation coverage checks shared declarations where applicable; no separate layer-specific rule was activated. **N/A**. |

## Applicable rule results

| Applicable repository rule | Exact source evidence | Result |
|---|---|---|
| Server owns durable identity, tenant selection, role mapping, issuance, and audit | `wyrd-auth/src/login.rs:84` (`HumanConnections::begin_login`), `wyrd-auth/src/callback.rs:201` (`AuthorizationCodeExchange::finish_id_token_exchange`), and the server adapters retain the durable workflow; clients only project it. | PASS |
| Public contracts are typed, language-agnostic, and live in `wyrd-spec` without IO/async/PyO3 | `wyrd-spec/src/auth/oidc.rs:193-415` defines `Sha256Hex`, `BeginLogin`, `LoginInitiation`, and `BeginLoginResponse`; the crate diff adds no runtime or binding dependency. | PASS |
| Stateful/dependency-backed workflows use cohesive concrete owners; free helpers are genuinely deterministic | Login and redemption are inherent `HumanConnections` methods; callback completion is an inherent `AuthorizationCodeExchange` method. `verify_id_token_algorithm`, nonce/`azp` checks, hashes, and narrow conversions are deterministic helpers. | PASS |
| Async is limited to IO or composition of IO | Added async service, handler, SQL, and journey methods await database, HTTP, or server operations; contract validation and algorithm/claim checks remain synchronous. | PASS |
| Tenant SQL uses `TenantConn`, forced RLS, no duplicate manual tenant filters, and caller-owned transactions | `wyrd-sql/src/queries/auth/login_state.rs:212-280`; migration lines 49-52 enable and force RLS. Callees do not commit or roll back. | PASS |
| Cross-tenant database access uses a narrow owned capability, not a raw pool passed through domain APIs | `wyrd-sql/src/postgres.rs:165` owns `login_state_tenant`; migration lines 60-74 return only the pending state's tenant and restrict execution to `wyrd_app`. | PASS |
| Secrets remain redacted and provider network calls use the screened owner | `LoginState` at `wyrd-sql/src/queries/auth/login_state.rs:124` wraps the PKCE verifier in `SecretString`; completions/provider credentials remain sealed; discovery, token, and JWKS calls route through the existing screened HTTP capability. | PASS |
| Authentication and authorization uncertainty fails closed | `callback.rs:201-272` orders advertised-set membership, shared external verification, nonce, `azp`, exact active-connection revalidation, identity/role work, canonical audit, issuance, sealing, and commit; errors abort before authority is returned. | PASS |
| Public errors use the derive-backed Wyrd catalog and one HTTP mapper | New paths reuse `WyrdError` variants and server mapping; no hand-written code/status/problem catalog or additional `IntoResponse` implementation appears in the cumulative diff. | PASS |
| Authorization decisions use the canonical same-transaction audit path | `roles_sync_event` at `callback.rs:552` builds the same canonical event and `finish_id_token_exchange` appends it inside the issuance transaction; failure coverage proves rollback. No second audit sink was added. | PASS |
| Imports are module-top and production lint escapes/gate circumvention are absent | Cumulative Rust diff contains no new production `#[allow(clippy::...)]`; the R2 SHA-256, Utoipa, and Wiremock imports remain in module import blocks. No required assertion/test was deleted or ignored to make a lane pass. | PASS |
| Every new/materially modified Rust item has accurate substantive rustdoc, `# Errors`, and `# Panics` where applicable | The R2 item inventory remains documented. R3 remediation now makes `verify_id_token_algorithm`'s summary and errors match its membership-only body and names `ExternalVerifier::verify_external_against` as the symmetric rejection owner (`callback.rs:566-604`). `terminal_json` retains its panic contract (`adapter/tests.rs:1237-1255`). | PASS |
| Generated artifacts come from owning sources; OpenAPI is proved from the served document | Generator/source changes accompany schema/golden changes; recorded `mise run codegen:check` passed. `pg_openapi_contract.rs:260` exercises the assembled server rather than a checked-in OpenAPI snapshot. | PASS |
| User-facing behavior has journey coverage, with negative and edge paths | Four named identity journeys cover login, refusal/isolation, replacement, and machine independence. `identity_e2e.rs:3183` now also exercises a provider-advertised `HS256` token and asserts `WYRD_AUTH_401_INVALID_TOKEN` plus no completion. | PASS |
| External tests earn their placement | `identity_e2e.rs` drives real server/IdP/Postgres behavior; `pg_login_state.rs` drives real Postgres; `pg_openapi_contract.rs` drives the assembled HTTP server. Pure contract/helper tests remain inline. | PASS |
| Lead-directed test cleanup preserves test integrity | `adapter/tests.rs:1246` parses the same terminal frame and panics on missing/malformed data; the change removes neither assertions nor coverage. The identity addition strengthens an existing journey without adding a parallel fixture or dependency. | PASS |
| Completion requires format, lint, targeted/broad tests, docs/codegen, and boundary evidence | R3 implementation evidence records green `fmt`, `lints`, `git diff --check`, focused gateway tests, focused and full identity journey lanes; prior cumulative evidence records SQL, principals, tenant/pool/client boundaries, docs, codegen, and `test:wyrd` (2,213 passing). | PASS |

## Prior standards finding closure

| Finding | Closure evidence | Result |
|---|---|---|
| `FIND-TASK-002-8` / R2 missing-rustdoc inventory | The cited schema projections, SQL constants, signing helper, and test alias remain substantively documented. | CLOSED |
| `FIND-TASK-002-9` / R2 function-scoped imports | The cited SHA-256, Utoipa, and Wiremock imports remain at module scope. | CLOSED |
| `FIND-TASK-002-10` / `STD-R3-001` false algorithm-helper contract | Commit `176905ddc` changes only the helper/caller rustdoc. The helper now documents advertised-set membership and its actual errors; it explicitly assigns symmetric-algorithm rejection to the immediately following shared verifier. Static comparison with the unchanged bodies confirms the contract. | CLOSED |

## Material findings

None.

## Verification notes and limits

- Fresh read-only checks in this review passed: candidate identity, clean
  tracked/untracked status before report creation, cumulative `git diff
  --check`, changed-file inventory, and targeted source/caller/rule scans.
- I relied on the recorded expensive-lane evidence rather than rerunning
  Docker, Postgres, Keycloak/Dex, Cargo-wide, or documentation/codegen lanes.
  The cumulative packet records green focused tests, all four required
  journeys, the full 27-test identity journey lane, principals unit and
  integration, SQL, tenant isolation, pool/client boundary checks, docs,
  codegen, format, lints, and `test:wyrd` with 2,213 passing tests. R3 records
  the focused gateway suite (39 passed), the advertised-`HS256` journey, full
  identity journey lane, rustdoc build, format, lints, and diff check.
- This review did not treat TASK-003's BFF completion page or TASK-004's CLI
  handoff persistence as changed surfaces. No task-acceptance or optional
  improvement judgment is included here.

## Overall result

**PASS**

The cumulative candidate conforms to the applicable repository ownership,
Rust structure/documentation, security, tenant SQL, audit, contract,
generated-artifact, testing, tooling, and documentation rules. All prior
repository-standards findings are closed, and the authorized R3 follow-up
commits introduce no material repository-rule regression.
