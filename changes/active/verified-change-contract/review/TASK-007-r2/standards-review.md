# TASK-007 R2 Repository Standards Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t007`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `b50ce7bc45c67f62dc7dabe34a2a1ef1ed421c34`
- Range reviewed: the complete cumulative `base..candidate` diff, including the
  original TASK-007 implementation and TASK-007-R1 remediation.
- Candidate identity was re-resolved after inspection and remained unchanged.

This report audits repository standards only. It does not decide task
acceptance and does not adopt another reviewer's conclusions.

## Authority coverage

| Changed surface | Applicable authority read and applied | Coverage |
|---|---|---|
| Operator/Card/API/error contracts and generated JSON schemas | `AGENTS.md` §§2-4, 8-9; `architecture/wyrd-design.md` Doctrine 4, 15, 20-21 and Operator section; `architecture/wyrd-doctrine.mdx`; `references/doctrine/positioning-and-vocabulary.md`; `references/languages/errors.md` | Complete |
| Shared cryptography, key configuration, mounted secrets, and async key resolution | `AGENTS.md` §§3-6; `architecture/agent-rules.md`; `architecture/wyrd-security-posture.md` Source credentials, SSRF defense, and cryptography sections; `references/architecture/patterns.md`; `references/languages/rust-core.md` | Complete |
| Postgres connection and dispatch persistence, migration, RLS, leases, and fencing | `AGENTS.md` §§3-6, 9; `architecture/agent-rules.md` SQL/TenantConn/OperatorPool rules; `references/doctrine/architecture-constraints.md`; `references/architecture/patterns.md`; `references/languages/rust-core.md` | Complete |
| Server CRUD, boot readiness, authorization, transactional audit, runtime claims, delivery, SSRF screening/pinning, and provider protocols | `AGENTS.md` §§5-6, 9-10; `architecture/agent-rules.md`; `architecture/wyrd-design.md` Operator section; `architecture/wyrd-security-posture.md`; `references/doctrine/architecture-constraints.md`; `references/architecture/patterns.md`; `references/languages/rust-core.md` | Complete |
| Shared Rust client and Rust SDK | `AGENTS.md` §§2-3, 5, 9; `architecture/wyrd-design.md` Client model; `references/architecture/patterns.md`; `references/doctrine/architecture-constraints.md` | Complete |
| Python/PyO3 package, exports, stubs, and tests | `AGENTS.md` §§7-8, 11; `references/languages/pyo3-boundaries.md`; `references/languages/python-api-and-stubs.md`; `references/languages/testing-workflows.md` | Complete |
| TypeScript/N-API package, declarations, and tests | `AGENTS.md` §§2-3, 9, 11; `references/languages/typescript-guide.md`; `references/languages/testing-workflows.md`; `references/languages/errors.md` | Complete |
| CLI, MCP, HTTP/OpenAPI, stable errors, and machine-readable agent surfaces | `AGENTS.md` §§2, 9, 11; `references/languages/agent-harness.md`; `references/languages/errors.md`; `references/doctrine/architecture-constraints.md` Surface Alignment | Complete |
| Rust, SQL, server, CLI, MCP, Python, TypeScript, generated-contract, and journey evidence | `AGENTS.md` §§11-12; `TESTING.md`; `references/languages/testing-workflows.md`; `references/languages/spec-driven-development.md`; `references/languages/implementation-execution.md` | Complete |
| Review/remediation artifacts in the candidate | `references/languages/spec-driven-development.md`; supplied `wyrd-task-review` workflow | Complete |

`architecture/bifrost-design.md` and the Vala analytical references are not
applicable: the changed MCP test harness reuses its existing Bifrost-hosted
journey binary, but this candidate does not change Bifrost ingest, storage,
query, Iceberg, DataFusion, or analytical reliability behavior.

## Rule results

| Repository rule | Evidence | Result |
|---|---|---|
| Durable contracts belong in `wyrd-spec`; durable behavior remains server-owned; all SDKs consume `wyrd-client`. | Contracts are in `wyrd-spec/src/operator_connection.rs` and `card/operator.rs`; persistence and delivery remain in `wyrd-sql`/`wyrd-server`; Rust, PyO3, and N-API projections delegate through `wyrd_client::operator_connections::OperatorConnections`. `mise run check:client-tier` passed. | PASS |
| `wyrd-spec` stays IO-, async-, SQL-, and PyO3-free, and public wire types are typed/schema-backed. | The new spec modules contain pure validation/serialization only and derive `JsonSchema`/`ToSchema`; IO, SQL, and bindings stay in owning crates. `mise run check:pyo3-scope` and `mise run codegen:check` passed. | PASS |
| Public errors use the derive-backed Wyrd catalog and must not disclose secret locations. | New errors use `#[wyrd_error]`, but `OperatorKeyUnavailable` publishes concrete key selector forms in its RFC 9457 `remediation`; see `REPO-R2-1`. | FAIL |
| Secret-bearing Rust structs use `SecretString` and a redacted custom `Debug`. | Secret bytes use `SecretBearer`, `SecretString`, `Zeroizing`, and redacted request types. The new key configuration/owner types still derive `Debug` over selector-bearing state; see `REPO-R2-2`. | FAIL |
| Tenant data uses caller-owned `TenantConn`; cross-tenant discovery uses the narrow `OperatorPool`; callees accepting `TenantConn` do not commit. | Connection and dispatch queries accept `&mut TenantConn`; only key-version and due-tenant discovery use `OperatorPool`; transaction owners in the server commit after composing work. Migration enables and forces RLS. `mise run check:tenant-isolation` passed. | PASS |
| Permission decisions are authorized and durably audited without turning engine mechanics into audit. | `OperatorConnectionControl` uses `operators:read`/`operators:write`; mutations append the allowed decision in the mutation transaction and record denied/uncommitted decisions through the canonical audit owner. Dispatch/rewrap mechanics do not invent audit events. | PASS |
| External HTTP resolves, screens, and pins before credentials are attached; redirects repeat the check; production TLS is not bypassed. | `OperatorDelivery::http` and `post_json` call `ScreenedHttp::client_for` before `Credential::attach`/bearer insertion; redirects are manual and same-origin. Vault uses a bounded, no-redirect client and production config requires HTTPS. | PASS |
| Provider-specific request/response details stay behind provider modules. | Slack JSON, Slack response parsing/transient codes, PagerDuty Events API v2 JSON, and both provider sends live directly in the 1,200-line `verification/operators.rs`; no Slack or PagerDuty provider module exists. See `REPO-R2-3`. | FAIL |
| Stateful and dependency-backed Rust behavior has a cohesive concrete owner; async is limited to real IO/composition. | `OperatorConnectionControl`, `OperatorKeys`, `OperatorDispatchQueue`, `ClaimLoop`, `OperatorWorker`, `OperatorDelivery`, and the shared client handle own their dependencies and expose inherent workflows. Pure parsing/rendering remains synchronous; mounted-file reads use `spawn_blocking`. | PASS |
| MCP reads remain discoverable, writes require explicit permission, and tool execution reuses typed server authorization/audit. | `mcp/operators.rs` always exposes read descriptors, conditionally advertises writes through `operators_write`, and delegates every call to `OperatorConnectionControl`, which authorizes again. Inputs/outputs are schema-backed and bounded. | PASS |
| HTTP handlers use typed bodies/paths, structured errors, scrubbed tracing, and runtime OpenAPI registration. | The five routes use typed IDs/contracts, raw body decoding that does not echo secret values, `#[tracing::instrument(skip(...))]`, `WyrdErrorResponse`, and `utoipa_axum` route registration. `pg_openapi_contract.rs` covers the mounted document. | PASS |
| First-class Rust, Python, and TypeScript surfaces preserve one contract and own runtime-specific projections only. | Rust re-exports the wire types; Python and TypeScript convert only at their binding boundaries and expose closed provider unions/flattened views. Public package exports and integration journeys exist. `py:typecheck` and `ts:typecheck` passed. | PASS |
| Generated artifacts are owner-generated, not independently edited. | Schema sources/generator and Python stub source/assembler changed with their generated outputs. `mise run codegen:check` passed without drift. | PASS |
| User-facing capabilities have real user journeys plus focused seam tests, and verification uses repository lanes. | Candidate contains Rust, HTTP, CLI, MCP, Python, and TypeScript journeys plus SQL/server/unit coverage. The implementation records green repository lanes and exact focused selectors; independently rerun static/generation gates are listed below. | PASS |
| No new unchecked unwrap/expect escape or check circumvention entered production code. | Production fallbacks are bounded conversions or invariant endpoint literals; test expectations remain in tests. No new suppressions were introduced for this feature. `mise run check:unwrap-audit`, Clippy, and `git diff --check` passed. | PASS |

## Material findings

### REPO-R2-1 — Public key-unavailable errors disclose the forbidden selector contract

- **Violated rule:** `architecture/wyrd-design.md` Operator section requires
  responses and errors to carry only redacted metadata and never key locations;
  the security posture requires credential/secret-resolution diagnostics to
  remain redacted. Public errors are RFC 9457 payloads including remediation.
- **Location:** `crates/wyrd-spec/src/error.rs:1696-1703`, especially the
  `OperatorKeyUnavailable` remediation at line 1702; projection is confirmed by
  `WyrdError::as_problem_json` at `crates/wyrd-spec/src/error.rs:3476-3486`.
- **Evidence:** The remediation returned to every authorized HTTP/SDK/MCP/CLI
  caller names `WYRD_OPERATOR_KEK_V<version>`, `<dir>/v<version>`, and the Vault
  `<mount>/data/<prefix>/<data_tenant_id>/<version>` selector. R1 made the error
  detail and logs constant, but its sentinel test uses concrete values and does
  not reject these generic selector forms.
- **Consequence:** A routine 503 response reveals deployment secret-provider
  topology and exact selector conventions, contradicting the candidate's own
  architecture authority and the redaction boundary the remediation was meant
  to restore.
- **Testable correction:** Keep the stable code/status/title, replace the
  remediation with a selector-free operator instruction such as restoring the
  configured Operator key provider, and extend the existing
  `key_failures_disclose_no_selector` proof to reject the generic environment,
  file, and Vault selector forms in the complete RFC 9457 payload.

### REPO-R2-2 — New secret configuration types use derived rather than custom redacted Debug

- **Violated rule:** `AGENTS.md` §4 requires redacted custom `Debug`
  implementations for secret-bearing structs; the Operator architecture also
  forbids logs and diagnostics from carrying key locations.
- **Location:** `crates/wyrd/wyrd-server/src/config.rs:1797-1818`
  (`VaultKeysConfig`) and `:1830-1854` (`OperatorKeysConfig`), plus
  `crates/wyrd/wyrd-server/src/components/operators/keys.rs:220-228`
  (`OperatorKeys`).
- **Evidence:** All three derive `Debug`. `VaultKeysConfig` holds a
  `SecretString` token plus the Vault address, mount, prefix, and token-file
  path; its containing config and owner therefore format the selector-bearing
  state transitively. `SecretString` redacts only its value and does not redact
  the other key locations.
- **Consequence:** Any diagnostic, panic context, or future structured field
  using ordinary `?config`/`?keys` formatting can disclose the exact
  secret-provider selectors even though normal key-error logging is bounded.
- **Testable correction:** Remove unneeded `Debug` implementations, or add
  custom redacted implementations that expose at most source kind, active
  version, and client presence while omitting address, directory, mount,
  prefix, token-file, and token. Add one formatting test with sentinel selector
  and token values covering the nested config and `OperatorKeys` owner.

### REPO-R2-3 — Provider wire handling is not isolated behind provider modules

- **Violated rule:** `AGENTS.md` §10: “Keep provider-specific request/response
  details behind provider modules.”
- **Location:** `crates/wyrd/wyrd-server/src/verification/operators.rs:519-669`
  and `:935-1034`.
- **Evidence:** `OperatorDelivery::send`, `slack`, and `pager_duty` directly own
  Slack `chat.postMessage` JSON, Slack JSON response/error parsing and transient
  code classification, and PagerDuty Events API v2 payload construction inside
  the generic worker/delivery module. The verification tree contains no Slack
  or PagerDuty provider module.
- **Consequence:** Provider protocol changes modify the generic leased-worker,
  retry, SSRF, and credential-attachment owner in the same file, defeating the
  repository's required provider boundary and making provider-specific review
  inseparable from shared delivery invariants.
- **Testable correction:** Keep `OperatorWorker` and `OperatorDelivery` as the
  owning structs, but move only Slack- and PagerDuty-specific payload and
  response mechanics into focused private provider modules. Reuse the existing
  screened-send/status/body helpers and preserve all current limits and
  credential ordering. Existing provider unit and Postgres journeys must pass;
  source inspection must show provider wire literals/parsing confined to those
  modules.

## Verification evidence and limits

Independently run against the candidate, all exit 0:

- `mise run codegen:check`
- `mise run check:client-tier`
- `mise run check:pyo3-scope`
- `mise run check:unwrap-audit`
- `mise run check:tenant-isolation`
- `mise run py:typecheck`
- `mise run ts:typecheck`
- `mise run fmt`
- `mise run lints`
- `mise run py:format`
- `mise run py:lints`
- `git diff --check f8811ac5035c3aa165d34c38992f9889b3c9081f b50ce7bc45c67f62dc7dabe34a2a1ef1ed421c34`

The task and remediation artifacts record successful SQL/shared/server/CLI/MCP
and all-language integration/journey lanes plus exact focused Operator tests.
Those long Postgres and journey lanes were not rerun during this bounded Wave 1
review. Live Slack/PagerDuty delivery remains intentionally gated release
evidence and was not run because no credentials are available. These limits do
not block the standards judgment: the retained failures are direct source and
public-contract violations that green behavioral lanes do not negate.

## Overall result

**FAIL**

The candidate satisfies the ownership, tenancy, authorization/audit, SSRF,
binding, generated-contract, and verification-lane standards inspected here,
but three material repository-rule violations remain: the public stable error
still publishes key selector conventions, new secret-bearing configuration
owners lack the required custom redacted `Debug`, and provider wire mechanics
remain inside the generic delivery module instead of provider modules.
