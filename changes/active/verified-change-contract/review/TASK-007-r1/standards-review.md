# TASK-007 Repository Standards Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t007`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `2bd4ded8f212f7885b0dd78bcd450fa0ff118f3e`
- Range reviewed: the complete `base..candidate` diff (117 files; Rust contracts,
  cryptography, auth screening, SQL migration/queries, server HTTP/MCP/runtime,
  CLI, shared client, Rust/Python/TypeScript SDKs, generated schemas/stubs, and
  all changed tests and task evidence).

## Authority coverage

| Changed surface | Applicable authority read | Coverage result |
|---|---|---|
| Repository workflow, implementation evidence, skills, task packet | `AGENTS.md` §§1, 11-16; `architecture/agent-rules.md`; `architecture/references/languages/spec-driven-development.md`; `architecture/references/languages/implementation-execution.md` | Covered |
| Operator/Card contracts, identities, errors, generated schemas | `architecture/wyrd-design.md` (including the candidate Operator section); `architecture/wyrd-doctrine.mdx`; `architecture/references/doctrine/positioning-and-vocabulary.md`; `architecture/references/doctrine/architecture-constraints.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/errors.md` | Covered |
| Rust, async, cryptography, URL screening, secret/key handling | `AGENTS.md` §§4-7, 15-16; `architecture/agent-rules.md`; `architecture/wyrd-security-posture.md`; `architecture/references/languages/rust-core.md`; `architecture/references/architecture/patterns.md` | Covered; failures `STD-007-01`, `STD-007-03`, and `STD-007-04` |
| PostgreSQL migration, RLS, tenant transactions, leases and dispatch durability | `AGENTS.md` §§3, 9-11, 15; `architecture/agent-rules.md`; `architecture/wyrd-security-posture.md`; `architecture/references/languages/rust-core.md`; `architecture/references/languages/implementation-execution.md` | Covered; pass |
| Server HTTP, authz/audit, runtime OpenAPI, MCP | `AGENTS.md` §§2, 9, 11; `architecture/wyrd-security-posture.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/agent-harness.md`; `architecture/references/languages/errors.md` | Covered; pass |
| Shared Rust client and Rust SDK | `AGENTS.md` §§2-5, 9; `architecture/references/architecture/patterns.md`; `architecture/references/languages/rust-core.md` | Covered; pass |
| PyO3 wrapper, Python package/stubs/tests | `AGENTS.md` §§7-8, 11; `architecture/references/languages/pyo3-boundaries.md`; `architecture/references/languages/python-api-and-stubs.md`; `architecture/references/languages/testing-workflows.md` | Covered; failure `STD-007-02` |
| N-API wrapper, TypeScript source/declarations/tests | `AGENTS.md` §§2-4, 11; `architecture/references/languages/typescript-guide.md`; `architecture/references/languages/testing-workflows.md`; `architecture/references/languages/errors.md` | Covered; failure `STD-007-02` |
| CLI secret input and test coverage | `AGENTS.md` §§4, 9, 11; `architecture/wyrd-security-posture.md`; `architecture/references/languages/agent-harness.md`; `architecture/references/languages/testing-workflows.md` | Covered; pass |
| Generated JSON schemas, Python stubs, N-API declarations, served OpenAPI | `AGENTS.md` §§8, 11-12; `architecture/agent-rules.md`; `architecture/references/languages/agent-harness.md`; `architecture/references/languages/python-api-and-stubs.md`; `architecture/references/languages/typescript-guide.md` | Covered; generation evidence recorded, but public typing fails `STD-007-02` |
| Journey, integration, unit, live-provider, and boundary verification | `AGENTS.md` §§11-12; `architecture/agent-rules.md`; `architecture/references/languages/testing-workflows.md`; `architecture/references/languages/implementation-execution.md` | Covered; recorded non-credentialed lanes pass; live smoke remains gated |

`architecture/bifrost-design.md` and the analytical-domain references were not
routed: the candidate changes no Bifrost ingest, query, storage, publication,
maintenance, or analytical contract. The touched Bifrost MCP test files only
extend the server tool-catalog expectation for the new non-Bifrost tools.

## Rule-by-rule results

| Repository rule | Evidence | Result |
|---|---|---|
| Durable contracts belong in PyO3-free, IO-free `wyrd-spec`; durable behavior remains server-owned | Typed closed unions and IDs are in `crates/wyrd-spec/src/operator_connection.rs`; SQL/server own persistence and delivery; SDKs delegate through `wyrd-client`; no PyO3/IO was added to `wyrd-spec` | PASS |
| Struct-centered Rust ownership | `OperatorConnectionControl`, `OperatorKeys`, `OperatorDispatchQueue`, `OperatorWorker`, `OperatorDelivery`, and `OperatorConnections` own cohesive state/workflows; provider sets use closed enums/matches rather than speculative traits | PASS |
| Tenant SQL uses `TenantConn`; cross-tenant discovery uses `OperatorPool`; callees do not commit; RLS is load-bearing | Migration enables and forces RLS; tenant query APIs accept `&mut TenantConn`; `due_tenants` and key-version discovery alone accept `&OperatorPool`; SQL helpers do not commit or add redundant tenant predicates | PASS |
| PostgreSQL owns lease/retry/deadline timestamps | Dispatch SQL consistently uses `statement_timestamp()` and accepts durations | PASS |
| Authorization is server-owned and audited; engine mechanics are not audit | HTTP and MCP share `OperatorConnectionControl`; reads/writes evaluate typed permissions; writes compose allowed audit rows with tenant mutations; worker claim/delivery transitions do not emit authorization audit | PASS |
| Public errors use the derive-backed catalog and preserve cross-surface structure | New Operator errors are `WyrdError` variants with metadata and are projected by HTTP/Python/TS/MCP/CLI paths | PASS |
| Secret-bearing types redact diagnostics and plaintext is not persisted or returned | Request secrets use `SecretBearer`; stored rows contain envelope ciphertext/nonces/wrapped DEKs; `SealedSecret` has a redacted `Debug`; view types contain authority metadata only | PASS |
| TLS is mandatory for credential-bearing production integrations; file-mounted secrets require restrictive permissions | Production validation accepts `http://` Vault addresses and the Vault token-file branch bypasses the existing owner-only reader | **FAIL — `STD-007-01`** |
| Do not block async request paths without an explicit blocking strategy | Operator key resolution performs `std::fs::metadata`/`read_to_string` directly in async create/update and worker paths | **FAIL — `STD-007-03`** |
| Before user/tenant URL fetches, resolve, reject, and pin; redirects re-screen | `ScreenedHttp` resolves/screens/pins; Operator HTTP renders an effective URL, rechecks origin, builds a screened client for every same-origin redirect, and disables automatic redirects | PASS |
| First-class SDKs project the same typed wire contract; Python stubs are precise; TypeScript declarations match runtime | Rust view serializes provider-specific fields flattened at top level, while the new TS interface declares a nonexistent nested `config`; TS updates and all Python request/response stubs erase the closed union into `Record/Mapping[..., unknown/Any]` | **FAIL — `STD-007-02`** |
| MCP reads are discoverable and writes require explicit permission scope | Per-caller catalog includes writes only with `operators:write`; invocation still routes through server authorization/audit | PASS |
| OpenAPI is runtime-owned; schemas/stubs/declarations are generated, not an OpenAPI snapshot | Routes are registered through `utoipa-axum`; served OpenAPI has a Postgres contract test; schema/stub generators were extended and `codegen:check`/`ts:napi:check` evidence is recorded; no OpenAPI file was added | PASS |
| Every user/agent-facing surface has a real client-to-server journey | HTTP/Postgres, Rust SDK, Python SDK, TypeScript SDK, CLI, and MCP journeys are present; provider delivery uses a real server/Postgres plus local mock providers | PASS |
| Production imports remain in the top-of-module dependency manifest | New production helpers add function-scoped imports in server delivery, key reading, and the testing library | **FAIL — `STD-007-04`** |
| No gate circumvention | No new production `#[allow]`; one credentialed live-provider test is deliberately ignored as a documented release smoke while non-credentialed local mock journeys cover behavior | PASS |
| Dependency cost remains narrow and no unapproved dependency is added | Only already-workspace `zeroize` is added to `wyrd-server`; no cloud/KMS SDK or client-tier SQL/analytical dependency is introduced | PASS |

## Material repository-rule findings

### STD-007-01 — Vault can transmit its root credential over plaintext and accepts a broadly readable token file

- Severity: **Critical**
- Violated rules: `architecture/wyrd-security-posture.md` requires mandatory TLS
  verification and restrictive permissions for file-mounted secrets; `AGENTS.md`
  §4 requires secret-safe handling.
- Locations:
  - `crates/wyrd/wyrd-server/src/config.rs:1900-1903` accepts both `http` and
    `https` for `vault.addr`, including in the multi-tenant production path.
  - `crates/wyrd/wyrd-server/src/components/operators/keys.rs:203-209` reads
    `vault.token_file` with unrestricted `std::fs::read_to_string`, while the
    same module's `read_owner_only` check is used only for KEK files.
  - `crates/wyrd/wyrd-server/src/components/operators/keys.rs:218-222` sends the
    Vault token in `X-Vault-Token` to that accepted address.
- Evidence: a production configuration with `source = "vault"`, an
  `http://...` address, and a group/world-readable token file passes
  `OperatorKeysConfig::validate`; the next key lookup transmits the token and
  receives tenant KEK material without TLS. No test rejects either case.
- Consequence: network observers can recover the Vault token and tenant KEK,
  and local users allowed to read a loose token file gain the same authority;
  either defeats the envelope-encryption boundary for every tenant reachable
  by that token.
- Testable correction: require `https` for Vault in production (permit a
  loopback-only HTTP exception only if an existing development rule requires
  it), and route `vault.token_file` through the existing owner-only file
  validation. Add configuration/key-reader tests proving production rejects
  plaintext Vault and that group/other-readable token files fail closed before
  any request is sent.

### STD-007-02 — Python and TypeScript declarations do not project the closed wire contract, and the TypeScript read shape is false

- Severity: **Important**
- Violated rules: `AGENTS.md` §§2, 8-9; `architecture/references/languages/python-api-and-stubs.md`
  (precise public types, not broad `Any`); `architecture/references/languages/typescript-guide.md`
  (declarations must not fork wire fields and should make invalid states
  unrepresentable).
- Locations:
  - `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1319-1331` models updates as
    `Record<string, unknown>` and declares `OperatorConnectionView.config`.
  - `crates/wyrd-spec/src/operator_connection.rs:485-501` defines the actual
    serialized view with `#[serde(flatten)]`, so provider-specific fields such
    as `workspace_id`, `origin`, and `auth` are top-level and no `config` field
    exists.
  - `sdks/wyrd-sdk-python/python/wyrd/operators/__init__.pyi:29-76` and its
    generator input `sdks/wyrd-sdk-python/python/wyrd/stubs/operators.pyi:27-74`
    expose all requests and views as `Mapping[str, Any]` / `dict[str, Any]`.
  - `sdks/wyrd-sdk-ts/wyrd/tests/integration/operator-connections.test.ts:42-60`
    checks only common fields, so it does not detect the declared/runtime
    provider-config mismatch.
- Evidence: the Rust contract and generated schema are closed provider-tagged
  unions, but TypeScript accepts arbitrary update keys and promises a property
  absent at runtime; Python type checking cannot reject any malformed provider
  payload or establish any response field.
- Consequence: correctly type-checked TypeScript code can dereference
  `view.config` and fail at runtime, while both SDKs lose compile-time checking
  for the exact public contract this repository requires first-class clients
  to project.
- Testable correction: derive or hand-author exact provider-discriminated
  create/update/view unions for TypeScript and precise `TypedDict` unions for
  Python from the owning Rust/schema shapes; keep generated artifacts generated.
  Add static type assertions and a journey assertion for each provider-specific
  read field, then pass `codegen:check`, `py:typecheck`, `ts:typecheck`, and the
  Python/TypeScript integration lanes.

### STD-007-03 — Operator key resolution blocks Tokio request and worker threads with filesystem IO

- Severity: **Important**
- Violated rules: `AGENTS.md` §6 and
  `architecture/references/languages/rust-core.md` async guidance prohibit
  blocking disk work inside async request paths without an explicit blocking
  strategy.
- Locations:
  - `crates/wyrd/wyrd-server/src/components/operators/keys.rs:150-173` is async
    but calls `read_owner_only`, whose `std::fs::metadata` and
    `std::fs::read_to_string` occur at `416-433`.
  - The Vault token path calls `std::fs::read_to_string` directly at `203-209`.
  - These paths are awaited by connection create/update and each delivery
    credential lookup (`components/operators/service.rs:109-130,293-307` and
    `verification/operators.rs:513-523`).
- Evidence: no `tokio::fs`, `spawn_blocking`, or other bounded blocking bridge
  surrounds these reads.
- Consequence: a slow or stalled secret mount blocks a Tokio executor thread
  during administrative requests and dispatch delivery, reducing unrelated
  server capacity and undermining the delivery timeout/concurrency bounds.
- Testable correction: move permission metadata and file reads through one
  async filesystem path or the repository's bounded blocking mechanism,
  preserving owner-only validation and zeroizing buffers. Exercise file and
  token-file success/refusal through the async owner, then run the server and
  shared test lanes.

### STD-007-04 — New production code hides imports inside functions

- Severity: **Important** (`BLOCK_BEFORE_MERGE` repository rule)
- Violated rule: `architecture/agent-rules.md` requires every non-test import
  at module top; its narrow `Trait as _` exception is for a single generic
  function, which none of these functions is.
- Locations:
  - `crates/wyrd/wyrd-server/src/verification/operators.rs:891`
    (`base64::Engine as _` inside `HttpRequest::render`).
  - `crates/wyrd/wyrd-server/src/components/operators/keys.rs:419`
    (`PermissionsExt as _` inside `read_owner_only`).
  - `crates/wyrd/wyrd-testing/src/server.rs:668-669` (`Engine as _` and
    `PermissionsExt as _` inside `generated_operator_keys`).
- Evidence: all three are ordinary non-generic production/library helpers, not
  imports scoped to a `#[cfg(test)] mod tests`.
- Consequence: the module headers no longer describe their dependency surface,
  violating a hard repository readability gate even though compilation passes.
- Testable correction: move these imports into the corresponding top-of-module
  import blocks (using appropriate `#[cfg(unix)]` placement for
  `PermissionsExt`) and run `fmt` and `lints`.

## Verification limits

- I inspected the immutable source/diff and the task's recorded verification
  evidence. I did not rerun the large Cargo/Postgres/Python/TypeScript lanes
  during this time-bounded review.
- The candidate records exit 0 for `test:sql`, `test:shared`, `test:wyrd`, the
  Rust/CLI/platform/Bifrost journeys, MCP, Python and TypeScript integration and
  typecheck lanes, `codegen:check`, tenant/client/PyO3/unwrap boundary checks,
  formatting, and lints. Those green lanes do not cover the concrete TLS,
  token-file-mode, runtime TypeScript-shape, or async-blocking gaps above.
- The credentialed Slack/PagerDuty live smoke remains intentionally unrun
  because no credentials are available in the fast/review environment. Local
  mock-provider delivery journeys are present; live-provider behavior remains
  gated release evidence.
- This was a static repository-standards review, not task-acceptance review. It
  does not rely on or incorporate another reviewer's conclusions.

## Overall result: FAIL

The candidate violates mandatory secret-transport/file-permission rules,
publishes SDK types that do not match the closed runtime contract, performs
blocking filesystem IO in async server paths, and violates the repository's
top-level-import rule. These are bounded implementation corrections; no missing
authority or unresolved architecture decision blocked this review.
