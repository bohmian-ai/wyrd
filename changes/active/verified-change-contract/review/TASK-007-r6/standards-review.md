# TASK-007-r6 Repository Standards Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t007`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `cd002ab1394cdc1679d95f957313a7f697d7e0a5`
- Approved specification: `changes/active/verified-change-contract/spec.md`,
  revision 36
- Inputs: original TASK-007, TASK-007-R1 through TASK-007-R5, their review
  artifacts, implementation evidence, and the complete cumulative
  `base..candidate` diff
- Scope: repository-rule compliance only. This report does not decide task
  acceptance and does not perform the Wave 2 Ponytail validation.

## Authority coverage

| Changed surface | Applicable authority inspected | Coverage |
|---|---|---|
| Approved change packet, architecture update, and cumulative remediation records | `AGENTS.md` §§1-2 and 12-16; `architecture/agent-rules.md`; `architecture/wyrd-design.md` (coordination clock and Operator contract); `architecture/wyrd-doctrine.mdx`; `architecture/references/languages/spec-driven-development.md`; `architecture/references/languages/implementation-execution.md` | Complete. Revision 36, the original task, five remediation tasks, and the active design consistently govern the cumulative candidate. |
| Operator/Card, connection, request/response, failure-context, identity, and stable-error contracts in `wyrd-spec` | `AGENTS.md` §§2-6, 9 and 16; `architecture/wyrd-design.md`; `architecture/references/doctrine/positioning-and-vocabulary.md`; `architecture/references/doctrine/architecture-constraints.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/rust-core.md`; `architecture/references/languages/errors.md` | Complete. The cumulative contracts remain typed, closed where required, synchronous, IO-free, PyO3-free, and schema-derived. |
| Cryptography, key sources, configuration, OIDC URL screening, permission, and credential handling | `AGENTS.md` §§3-7, 9, 15-16; `architecture/agent-rules.md`; `architecture/wyrd-security-posture.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/rust-core.md` | Complete. Secret material stays in redacted server-owned owners; environment KEKs are now refused in every production topology at `config.rs:1905-1926`. |
| Postgres migration, RLS, tenant connection CRUD, dispatch claim/settlement, retry bounds, and rotation discovery | `AGENTS.md` §§3-6, 9, 15-16; `architecture/agent-rules.md`; `architecture/wyrd-security-posture.md`; `architecture/references/doctrine/architecture-constraints.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/rust-core.md` | Complete. Tenant work uses `TenantConn`, cross-tenant discovery uses `OperatorPool`, callers retain transaction lifecycle, and PostgreSQL owns retry coordination time. |
| Server routes, service, boot/readiness, registration, delivery worker, provider adapters, audit, metrics, and health | `AGENTS.md` §§2-6, 9-12, 15-16; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/wyrd-security-posture.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/rust-core.md`; `architecture/references/languages/errors.md` | Complete. Durable behavior remains server-owned and struct-centered; security, authorization/audit, screened outbound IO, bounded delivery, retry, fencing, and cancellation rules remain represented. |
| HTTP, shared Rust client, Rust SDK, CLI, and MCP | `AGENTS.md` §§2-4, 9, 11 and 16; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/agent-harness.md`; `architecture/references/languages/errors.md` | Complete. These surfaces continue to project the shared typed contract and stable errors; the R5 correction changes no public surface. |
| Python/PyO3 package, exports, stubs, typing, and journey | `AGENTS.md` §§2-3, 7-8, 11-12 and 16; `architecture/agent-rules.md`; `architecture/references/languages/pyo3-boundaries.md`; `architecture/references/languages/python-api-and-stubs.md`; `architecture/references/languages/errors.md` | Complete. PyO3 remains confined to the Python SDK, wrapping `wyrd-client`; public exports, generated typing, and journey coverage are present. |
| TypeScript/N-API package, declarations, wrappers, errors, and journey | `AGENTS.md` §§2-3, 9, 11-12 and 16; `architecture/agent-rules.md`; `architecture/references/languages/typescript-guide.md`; `architecture/references/languages/errors.md` | Complete. The N-API layer remains a thin `wyrd-client` projection and the public declarations preserve the closed wire contract. |
| Rust, SQL, HTTP/OpenAPI, MCP, CLI, Python, TypeScript, and generated-contract verification | `AGENTS.md` §§8, 11-12 and 16; `architecture/agent-rules.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/testing-workflows.md`; `architecture/references/languages/agent-harness.md` | Complete. The capability retains real-server journeys for shipped public surfaces, supporting seam tests, served OpenAPI/MCP proof, and generated schema/stub/declaration checks. The R5 delta adds focused configuration and Postgres regressions. |

## Applicable rule results

| Repository rule | Exact source and verification evidence | Result |
|---|---|---|
| Server-owned durable behavior; language surfaces project the one shared client owner. | Persistence and delivery remain in `crates/wyrd/wyrd-server/src/components/operators/`, `crates/wyrd/wyrd-server/src/verification/operators.rs`, and `crates/wyrd/wyrd-sql/src/queries/`; shared client behavior remains in `crates/shared/wyrd-client/src/operator_connections.rs`. The R5 diff touches no client surface. | PASS |
| `wyrd-spec` remains foundational, typed, synchronous, IO-free, SQL-free, server-tier-free, and PyO3-free. | The cumulative contract additions are confined to typed identities, values, validation, schemas, and derive-backed errors under `crates/wyrd-spec`; the R5 diff does not change that crate. Earlier cumulative evidence reports `check:client-tier` and `check:pyo3-scope` green. | PASS |
| Tenant SQL uses `TenantConn` under RLS, leaves commit/rollback to callers, and uses `OperatorPool` only for explicit cross-tenant work. | `OperatorDispatchQueue::retry` accepts `&mut TenantConn<'_>` at `operator_dispatches.rs:279-294` and does not commit or roll back. The existing connection, dispatch, and discovery owners retain the approved abstractions; cumulative evidence reports `check:tenant-isolation` green. | PASS |
| PostgreSQL owns coordination timestamps and retry settlement is bounded before interval construction. | `operator_dispatches.rs:99-118` uses `statement_timestamp()` and bounds the provider delay with `LEAST($5::bigint, $6::bigint)` before multiplying by the interval, while retaining the absolute deadline and lease-token fence. `pg_verifier_runs.rs:1996-2045` proves maximum accepted delay settlement at the database-owned five-minute deadline. | PASS |
| Production key sources enforce the approved secret-provider boundary without disclosing or eagerly reading keys. | `config.rs:1895-1926` rejects `OperatorKeySource::Env` whenever `production`, retaining single-tenant file/Vault and multi-tenant Vault rules. `config.rs` test `operator_key_source_follows_deployment` covers the deployment matrix without reading a key. Existing readiness behavior and redacted `Debug` owners remain unchanged. | PASS |
| Secrets stay outside Cards, responses, generated artifacts, logs, audit, errors, and CLI argv. | The cumulative public view remains metadata-only; credentials remain redacted/encrypted and CLI input remains file/stdin based. R5 changes only source validation, SQL arithmetic, and tests and introduce no secret value or selector into a public payload. | PASS |
| Tenant-controlled outbound URLs are resolved, screened, and pinned before credentials are attached and on redirects. | The cumulative server owner in `verification/operators.rs` retains the screened/pinned client path before `Credential::attach`; the R5 diff does not alter outbound IO. Prior focused and journey evidence covers that boundary. | PASS |
| Authorization decisions use the canonical transactional audit path; engine mechanics do not create audit events. | The cumulative connection service retains its canonical allowed/denied decision recording in the owning transaction. Dispatch scheduling and delivery remain engine mechanics without a parallel audit writer. R5 changes no authorization or audit path. | PASS |
| External IO, retries, concurrency, cancellation, and work are bounded. | The cumulative worker retains global/per-tenant permits, attempt timeout, attempt budget, five-minute deadline, redirect/body limits, and lease fencing. `operator_dispatches.rs:112-113` now prevents an accepted oversized provider delay from exceeding SQL interval or dispatch-deadline bounds. | PASS |
| New and materially modified Rust follows cohesive concrete owners, module-scope imports, bare declaration types, synchronous validation, earned async, and required rustdoc. | The R5 production changes remain on existing owners `OperatorKeysConfig` and `OperatorDispatchQueue`; validation is synchronous and the retry method directly awaits database IO. Updated rustdoc at `config.rs:1895-1905` and `operator_dispatches.rs:99-105`, plus test rustdoc at `pg_verifier_runs.rs:1996-2005`, describes the new invariants and panic behavior. The test-only declaration imports are module-scoped at `pg_operator_connection_routes.rs:11-21`. | PASS |
| Public failures use the derive-backed stable error catalog and do not leak internal database, transport, crypto, or filesystem detail. | The cumulative Operator variants remain in `crates/wyrd-spec/src/error.rs` with `#[wyrd_error]` metadata. The R5 validation reuses `ConfigError::Invalid` before server boot and adds no public code, message catalog, or boundary conversion. | PASS |
| HTTP, MCP, CLI, schemas, Rust, Python, and TypeScript project one typed contract; MCP writes remain scope-gated. | Shared `wyrd-spec` request/view types and `wyrd-client::OperatorConnections` remain the projection owners. R5 changes no wire type, route, tool, declaration, stub, or generated schema. | PASS |
| Python and TypeScript runtime boundaries stay thin and synchronized. | Both native wrappers continue to delegate to `wyrd-client`; package exports, generated stubs/declarations, unit proof, and real-server journeys remain in the cumulative candidate. No R5 boundary change required regeneration. | PASS |
| Every shipped user/agent surface has a real client-to-server journey, with focused lower-tier regression proof. | The cumulative candidate contains Rust, Python, TypeScript, CLI, MCP, HTTP, SQL, OpenAPI, and provider journeys. R5 adds `config::tests::operator_key_source_follows_deployment` and Postgres-backed `maximum_retry_after_settles_at_the_deadline` rather than replacing higher-tier coverage. | PASS |
| Generated artifacts derive from owners; served OpenAPI and runtime MCP use their own tests. | Generated schemas, Python stubs, and TypeScript declarations are present with earlier `codegen:check` evidence; served OpenAPI and MCP catalog/runtime tests remain present. R5 modifies no generated or public contract artifact. | PASS |
| No gate or assertion was weakened or bypassed, and no unrelated dependency, feature, migration, or cleanup entered R5. | The R5 implementation diff adds one configuration guard, one SQL-native bound, two focused regressions, and module-scope test imports. It adds no `#[allow]`, `#[ignore]`, dependency, Cargo feature, migration, public error, or contract. `git diff --check base..candidate` exits 0. | PASS |

## Material repository-rule findings

None.

The R5 candidate closes the two previously validated gaps at their existing
owners without reopening a prior standards issue:

- production now refuses the development-only environment KEK at the existing
  configuration validator; and
- retry SQL bounds an accepted provider delay with the existing dispatch
  deadline before PostgreSQL constructs an interval.

No repository-rule correction is required.

## Verification limits

- This was a time-bounded static standards audit of the complete cumulative
  diff, applicable authorities, source owners, tests, and recorded evidence.
  Generated JSON schema snapshots were traced to their Rust owners rather than
  reread line by line.
- Independently run here: `git diff --check
  f8811ac5035c3aa165d34c38992f9889b3c9081f..cd002ab1394cdc1679d95f957313a7f697d7e0a5`
  (exit 0).
- TASK-007-R5 implementation evidence reports all required focused tests,
  `mise run test:sql`, `mise run test:wyrd` (2160 passed), `mise run fmt`,
  `mise run lints`, `mise run check:tenant-isolation`, and `mise run
  check:unwrap-audit` green. Earlier cumulative evidence reports the client
  tier, PyO3, codegen, SDK, CLI, MCP, OpenAPI, provider, and journey lanes
  green. Those longer Cargo/Postgres/language lanes were not rerun during this
  sub-review.
- The credentialed Slack/PagerDuty live-provider smoke was not run. It remains
  an intentionally gated release check; local-provider journeys cover the
  provider wire, retry, screening, credential-order, and disclosure paths.
- CodeGraph was unavailable because this checkout has no `.codegraph/`
  directory; source inspection used the immutable candidate directly.

## Overall result

**PASS**

No material repository-rule violation remains in the immutable cumulative
candidate. The ownership, security, tenancy, durability, audit, SQL clock,
Rust structure and documentation, public-surface alignment, language
boundaries, generated-contract, and verification rules applicable to the
changed surfaces are satisfied.
