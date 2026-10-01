# TASK-007-r5 Repository Standards Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t007`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `1e857c89f116fc7901e365bc456db6a20cbea684`
- Approved specification: `changes/active/verified-change-contract/spec.md`,
  revision 36, explicitly approved 2026-09-24
- Candidate observed at review start and finish:
  `1e857c89f116fc7901e365bc456db6a20cbea684`
- Scope: repository-rule compliance of the complete cumulative diff. This
  report does not decide task acceptance.

## Authority coverage

| Changed surface | Applicable authority inspected | Coverage |
|---|---|---|
| Architecture, approved change packet, and remediation records | `AGENTS.md` §§1-2 and 14-16; `architecture/agent-rules.md`; `architecture/wyrd-design.md` doctrine and Operator contract; `architecture/wyrd-doctrine.mdx`; `architecture/references/languages/spec-driven-development.md`; `architecture/references/languages/implementation-execution.md` | Complete. Revision 36, TASK-007, R1-R4 records, and the active design consistently record the approved env/file/HashiCorp Vault KV v2 key-source boundary and its explicit deferrals. |
| Operator/Card, connection, ID, failure-context, request/response, and error contracts in `wyrd-spec` | `AGENTS.md` §§2-4, 9, 15-16; `architecture/wyrd-design.md`; `architecture/references/doctrine/positioning-and-vocabulary.md`; `architecture/references/doctrine/architecture-constraints.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/rust-core.md`; `architecture/references/languages/errors.md` | Complete. Contracts are typed, closed where required, synchronous, IO-free, PyO3-free, schema-derived, and use Wyrd-native vocabulary. |
| Cryptography, secret/key sources, config, OIDC address screening, and permissions | `AGENTS.md` §§3-7, 9 and 15-16; `architecture/agent-rules.md`; `architecture/wyrd-security-posture.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/rust-core.md` | Complete. Cryptography stays in `wyrd-crypt`; the concrete key owner stays server-owned; secrets are redacted; the existing screened/pinned HTTP owner remains the network boundary. |
| Postgres migration, tenant connection CRUD, dispatch claims, settlement, and rotation discovery | `AGENTS.md` §§3-6, 9, 15-16; `architecture/agent-rules.md`; `architecture/wyrd-security-posture.md`; `architecture/references/doctrine/architecture-constraints.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/rust-core.md` | Complete. Tenant operations use `TenantConn` under forced RLS, cross-tenant discovery uses `OperatorPool`, PostgreSQL owns coordination time, and query callees do not end caller-owned transactions. |
| Server routes, service, boot/readiness, registration validation, delivery worker, provider adapters, audit, metrics, and health | `AGENTS.md` §§2-6, 9-12, 15-16; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/wyrd-security-posture.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/rust-core.md`; `architecture/references/languages/errors.md` | Complete. Durable behavior is server-owned and struct-centered; authorization/audit, secret resolution, bounded delivery, retries, fencing, and cancellation preserve their governing boundaries. |
| HTTP, shared Rust client, Rust SDK, CLI, and MCP | `AGENTS.md` §§2-4, 9, 11, 16; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/agent-harness.md`; `architecture/references/languages/errors.md` | Complete. All surfaces project the same typed operations and stable errors. The MCP list description now promises only fields present in the redacted response and is pinned by catalog proof. |
| Python/PyO3 package, exports, stubs, typing, and journeys | `AGENTS.md` §§2-3, 7-8, 11-12, 16; `architecture/agent-rules.md`; `architecture/references/languages/pyo3-boundaries.md`; `architecture/references/languages/python-api-and-stubs.md`; `architecture/references/languages/errors.md` | Complete. PyO3 remains in the Python SDK, wraps `wyrd-client`, registers and exports the public package surface, and has generated typing plus runtime coverage. |
| TypeScript/napi package, declarations, wrapper, errors, and journeys | `AGENTS.md` §§2-3, 9, 11-12, 16; `architecture/agent-rules.md`; `architecture/references/languages/typescript-guide.md`; `architecture/references/languages/errors.md` | Complete. The napi layer is a thin projection of `wyrd-client`; public unions/declarations match the closed wire contract and generated artifacts. |
| Rust, SQL, HTTP/OpenAPI, MCP, CLI, Python, and TypeScript tests and generated artifacts | `AGENTS.md` §§8, 11-12, 16; `architecture/agent-rules.md`; `architecture/references/architecture/patterns.md` verification pattern; `architecture/references/languages/testing-workflows.md`; `architecture/references/languages/agent-harness.md` | Complete. The capability has real-server journeys on every shipped public surface, supporting SQL/server/provider tests, served-OpenAPI proof, MCP catalog/runtime proof, and generated schema/stub/declaration checks. |

## Applicable rule results

| Repository rule | Source and verification evidence | Result |
|---|---|---|
| Server-owned durable behavior; clients are projections over the one shared client owner. | `components/operators/service.rs`, `components/operators/keys.rs`, `verification/operators.rs`, and `wyrd-sql` own persistence and delivery. `crates/shared/wyrd-client/src/operator_connections.rs:32-127` is the shared client handle used by Rust, Python, TypeScript, and CLI. | PASS |
| `wyrd-spec` remains foundational: IO-free, async-free, SQL-free, server-tier-free, and PyO3-free. | New Operator and connection modules contain typed values, validation, serialization, schemas, and errors only. Fresh `check:client-tier` and `check:pyo3-scope` passed. | PASS |
| Public contracts use domain types and closed enums/unions rather than raw durable identifiers or surface-specific shapes. | `wyrd-spec/src/operator_connection.rs`, `wyrd-spec/src/card/operator.rs`, and `wyrd-spec/src/ids.rs` own the common identities and closed provider/action/auth shapes; generated schemas and all three SDK projections match. | PASS |
| Tenant SQL uses `TenantConn`, relies on RLS, and leaves transaction lifecycle to callers; cross-tenant work uses only `OperatorPool`. | `20260601000032_operator_connections.sql:40-50` forces RLS and narrows grants. Connection/dispatch query owners use the approved connection abstractions and contain no callee commit/rollback. Fresh `check:tenant-isolation` passed. | PASS |
| PostgreSQL supplies coordination timestamps and durable claims remain fenced and bounded. | Dispatch query SQL derives claim/retry eligibility from database time; worker claims and settlements preserve lease tokens and per-tenant/global permit bounds. Provider and fairness/restart tests cover the seam. | PASS |
| Every authorization decision is audited at its commit boundary and failures fail closed; engine mechanics do not create audit. | `components/operators/service.rs:133,323,338` uses canonical denial and uncommitted-decision recording around the connection transaction. Dispatch delivery/rewrap performs no permission decision and emits no parallel audit event. | PASS |
| Secret material stays outside Cards, responses, errors, logs, audit, and CLI argv; secret-bearing Rust values redact. | Public `OperatorConnectionView` is metadata-only; request secrets use redacted types; CLI reads request bodies from file/stdin; key/config/envelope debug output is redacted; stable errors disclose no key location or credential bytes. | PASS |
| Revision-36 key-source behavior is recorded in every governing artifact without adding a speculative resolver/provider. | `spec.md:1042-1065`, `TASK-007...md:93-108`, and `wyrd-design.md:1382-1397` align on the server-owned env/file/Vault owner, production Vault/HTTPS readiness rule, runtime failure behavior, and explicit `SecretRef`/AWS/GCP deferral. | PASS |
| Tenant-controlled outbound URLs are resolved, screened, and pinned before credential attachment, with the same procedure on redirects. | `verification/operators.rs:660,733-751` obtains the screened client before `Credential::attach`; effective redirects return through the screened client path and preserve the approved authority. | PASS |
| External IO, retries, concurrency, and cancellation are bounded; pure transformations stay synchronous. | Operator attempts, response bodies, redirects, claims, key reads, and rewrap discovery have explicit limits. Async methods directly await database, filesystem, resolver, or HTTP work; rendering and validation remain synchronous. | PASS |
| New/materially changed Rust follows cohesive concrete owners, top-level import manifests, bare declaration types, and required rustdoc. | `OperatorConnections`, `OperatorConnectionService`, `OperatorKeys`, and `OperatorDelivery` own their stateful workflows. The cumulative added-declaration sweep confirms the R4 import correction; no candidate-added qualified field/parameter/return declarations remain in its scoped list. New and modified items document workflow role, errors, and relevant cancellation/side effects. | PASS |
| Public failures use the derive-backed stable error catalog and do not leak internal transport/database/crypto details. | Operator error variants live in `wyrd-spec/src/error.rs` with `#[wyrd_error]` metadata; HTTP, MCP, CLI, Python, and TypeScript use the shared projection. | PASS |
| HTTP, MCP, CLI, schemas, and SDKs project the same redacted typed contract; MCP writes require scopes. | The shared request/view types drive every surface. `mcp/operators.rs:81-89` describes the redacted response accurately, and `wyrd-mcp/.../discovery.rs:106` pins that text. | PASS |
| Python and TypeScript runtime boundaries remain thin and their public exports/declarations/stubs stay synchronized. | Both native wrappers call `wyrd_client::operator_connections::OperatorConnections`; public packages export the wrappers; fresh `codegen:check`, `check:pyo3-scope`, and `check:client-tier` passed. | PASS |
| Every shipped user/agent surface has a real client-to-server journey, with supporting negative, tenancy, durability, provider, and boundary tests. | Rust SDK, Python, TypeScript, CLI, MCP, HTTP, SQL, OpenAPI, and provider journey targets are present. Examples include `pg_operator_connection_routes.rs:179,384,506,588,730` and `pg_operator_delivery.rs:475,638,771,860,1002,1144,1325`. The R4 execution record reports all required focused/broader lanes green. | PASS |
| Generated schemas, stubs, and declarations are source-derived and drift-free; served OpenAPI and runtime MCP use their owning tests. | Fresh `mise run codegen:check` passed. The candidate includes served-OpenAPI and MCP catalog/runtime tests rather than treating codegen as substitute proof. | PASS |
| No gate was weakened or bypassed. | No candidate-added production lint suppression, ignored required test, weakened boundary glob, or hand-maintained parallel error catalog was found. Fresh format, boundary, tenant, unwrap, codegen, and diff checks passed. | PASS |

## Material repository-rule findings

None.

The prior standards findings are closed in the cumulative candidate:

- The declaration/import rule is satisfied at the candidate-added sites named
  by R4; imports are module-scoped and declaration types are bare.
- The MCP list descriptor no longer promises an unavailable `secret_version`
  field, and its redacted field list is asserted by the existing catalog
  journey.
- Revision 36 resolves the former key-source authority conflict by explicitly
  approving the concrete server-owned env/file/HashiCorp Vault source and
  deferring the absent generic resolver and cloud-provider additions.

## Verification limits

- Independently run during this review, all exit 0: `git diff --check
  f8811ac5..1e857c89`, `mise run fmt`, `mise run check:client-tier`, `mise run
  check:pyo3-scope`, `mise run check:tenant-isolation`, `mise run
  check:unwrap-audit`, and `mise run codegen:check`.
- The R4 execution record reports `mise run lints`, `mise run test:wyrd`, the
  focused MCP catalog test, and `mise run test:bifrost:journey:mcp` green. This
  reviewer did not rerun those longer Cargo/Postgres lanes inside the
  sub-review budget.
- Earlier cumulative implementation/review evidence reports the SQL, shared,
  server/provider, CLI, Rust SDK, Python, TypeScript, OpenAPI, and complete
  journey lanes green. They were inspected as available evidence but not all
  independently repeated here.
- The credentialed Slack/PagerDuty live-provider smoke was not run. It remains
  an intentionally gated release check; repository-owned local-provider
  journeys cover wire shape, credential order, retries, disclosure, and
  failure classification.
- CodeGraph was unavailable because this checkout has no `.codegraph/` index;
  source and callers were inspected directly.

## Overall result

**PASS**

No material repository-rule violation remains in the immutable cumulative
candidate. The security, tenancy, durability, audit, ownership, Rust structure,
public-surface, language-boundary, generated-contract, and testing rules
applicable to the changed surfaces are satisfied.
