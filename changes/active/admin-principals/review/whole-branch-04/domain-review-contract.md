# Public contract domain review

## Result

**FAIL**

## Reviewed boundary and authority coverage

Reviewed public HTTP/OpenAPI contracts, stable error projection, CLI and shared
Rust client surfaces, Rust/Python/TypeScript SDK boundaries, MCP client and
server projections, operator journeys, public documentation, generated
schemas/stubs, and codegen/docs/check wiring.

Authority coverage included `AGENTS.md`,
`.agents/skills/wyrd-task-review/SKILL.md`, `architecture/agent-rules.md`, the
applicable Wyrd design/doctrine, security, public-surface, error, SDK/stub,
testing, and spec-driven-development authorities; approved specification
revision 10; all eight original task packets; the whole-branch-03
verdict/remediation evidence; the immutable base-to-candidate diff; and live
consumers.

The immutable subject was base
`c5c20754a167e8f4d74a555a720bd51df6179a6f` and candidate
`96a1bd81b1d028fa81d6e0f385e3cd9b62080e30`. The approved spec hash was
revalidated as
`05982825655110b7a514f16ffdd953e4b1e1df4b7422e9e77461f2e514a10837`.

## Proposed findings

### `R4-CONTRACT-01` — incomplete canonical OpenAPI contract

- **Classification:** `INCORRECT`
- **Violated obligation:** `REQ-036`, `REQ-049`, `AC-014`, `AC-019`.
- **Location:** `crates/wyrd/wyrd-server/src/http/openapi.rs:146`,
  `crates/wyrd/wyrd-server/src/http/router.rs:51`, and
  `crates/wyrd/wyrd-server/src/components/principals/routes.rs:178,319,358`.
- **Evidence:** `WyrdApiDoc` does not describe several ordinary JSON route
  groups merged into the public `/v1` router, including storage upload,
  evaluation, and authorization routes. Within the newly documented
  administrative surface, `issue_credential` and `list_credentials` both call
  `require_principal`, which returns
  `WYRD_AUTH_404_PRINCIPAL_NOT_FOUND`, but neither OpenAPI operation declares
  that reachable 404 response. The contract test validates only codes already
  present in response descriptions, so it cannot detect omitted reachable
  errors.
- **Observable consequence:** An independent client generated from
  `/openapi.json` cannot discover all served HTTP operations or implement
  complete error handling for the administrative operations this change adds.
- **Testable correction:** Add typed `utoipa` annotations for every served
  public JSON operation and declare every reachable stable error, beginning
  with the missing principal 404s. Add a closure test that fails when a served
  handler or reachable administrative error is absent from the generated
  document, without introducing another hand-maintained route/error catalog.

### `R4-CONTRACT-02` — OpenAPI acceptance is proved by parallel lists and substring inspection

- **Classification:** `VIOLATION`
- **Violated obligation:** `REQ-049` and `AC-019` require exact runtime coverage
  without a parallel route catalog and require `/openapi.json` to serve the
  `utoipa` document.
- **Location:** `crates/wyrd/wyrd-server/src/http/openapi.rs:21,268,294` and
  `crates/wyrd/wyrd-server/src/http/router.rs:177`.
- **Evidence:** `ANONYMOUS_PATHS` is a handwritten second list of six routes
  controlling generated security metadata.
  `every_served_auth_and_admin_route_is_documented` scans only the auth and
  tenant-admin source files and hard-codes an expected count of six, omitting
  platform and principal route tables. `only_json_openapi_is_served` searches
  `router.rs` source substrings; it never sends an HTTP request to the assembled
  router and therefore does not prove that `GET /openapi.json` returns the
  runtime document or that `/openapi.yaml` is actually unrouted.
- **Observable consequence:** The focused contract lane can remain green while
  runtime routing, security metadata, or route coverage drifts. It already
  failed to detect `R4-CONTRACT-01`.
- **Testable correction:** Co-locate anonymous security declarations with the
  owning `utoipa` operations instead of maintaining `ANONYMOUS_PATHS`. Exercise
  an assembled router or real test server and assert `GET /openapi.json`
  returns a valid document with the expected media type while
  `/openapi.yaml` returns 404. Expand route/error closure proof to all public
  JSON route owners.

### `R4-CONTRACT-03` — the first-party MCP client bypasses required shared transport renewal

- **Classification:** `VIOLATION`
- **Violated obligation:** `REQ-047`, `REQ-048`, `AC-018`, and `TASK-008`'s
  ownership rule that MCP is a consumer and owns no Wyrd transport.
- **Location:** `crates/wyrd/wyrd-mcp/src/client.rs:30,38,46,50` and
  `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/connectivity.rs:95`.
- **Evidence:** `WyrdMcpHttpClient` owns a raw `reqwest::Client`, inserts
  `X-Wyrd-Access-Token` itself, and its public constructor requires callers to
  provide that client. The implementation explicitly states that it performs
  no reactive 401 parsing or retry. The first-party journey constructs
  `reqwest::Client::new()` directly. This differs from `HttpTransport`, whose
  401 path calls `AuthMiddleware::force_refresh()` and retries once.
- **Observable consequence:** An API-key or workload-authenticated MCP request
  that receives an authentication refusal is returned as a failure without the
  mandated one-time durable-credential re-exchange and retry. Wyrd also retains
  two owners for HTTP/header behavior.
- **Testable correction:** Keep `rmcp` protocol framing in the MCP layer, but
  have shared `wyrd-client` provide the configured HTTP/authentication
  behavior, including one reactive `force_refresh` and replay. Add a bounded
  MCP test where the first request returns 401, exactly one new exchange
  occurs, one replay succeeds, and a second refusal is not retried.

### `R4-CONTRACT-04` — public documentation publishes the superseded identity and renewal model

- **Classification:** `DRIFT`
- **Violated obligation:** `REQ-036`, `REQ-040`, `REQ-048`, and `AC-013`.
- **Location:** `docs/src/content/docs/concepts/authentication.svx:22,38,103`,
  `docs/src/content/docs/concepts/identity-and-auth.svx:33,39,89`, and
  `docs/src/content/docs/self-hosting/authentication.svx:13,19`.
- **Evidence:** The docs still state that only User, Service, and Agent
  principals exist; describe Service and Agent principals as necessarily
  Card-bound; and repeatedly claim API-key exchange returns access plus refresh
  tokens. The accepted model adds `global_admin` and `tenant_admin`, permits
  Card-free machine principals, and reserves refresh tokens for human OIDC
  sessions.
- **Observable consequence:** Operators and SDK authors are directed to
  implement a refresh flow the server no longer returns and are given a second,
  incompatible principal model.
- **Testable correction:** Update the concepts and self-hosting pages to the
  five principal kinds, optional machine Card binding, platform/tenant scope,
  and machine durable-credential re-exchange versus human refresh rotation.
  Regenerate `llms.txt`/`llms-full.txt` and run `mise run docs:check`.

### `R4-CONTRACT-05` — operator journey does not prove the returned tenant credential through the CLI

- **Classification:** `MISSING`
- **Violated obligation:** `AC-002`, `REQ-040`, and `TASK-008`'s criterion that
  a once-returned credential is used on a subsequent real-server CLI call.
- **Location:** `docs/src/content/docs/self-hosting/running-the-server.svx:67,84`
  and `crates/wyrd/wyrd-cli/tests/operator_journey.rs:103,121`.
- **Evidence:** Tenant creation prints `admin_credential`, but the documentation
  immediately asks for an unexplained `<tenant access token>`. The journey
  similarly calls the test fixture's in-process `exchange_api_key` helper and
  passes the resulting `WYRD_ACCESS_TOKEN` to the CLI. The returned credential
  never traverses the shipped CLI/shared-client credential exchange before
  issuer configuration and restricted-principal creation.
- **Observable consequence:** The required operator path can pass even if the
  CLI cannot consume the credential tenant creation actually returns, while an
  operator following the page lacks the documented handoff needed for the next
  command.
- **Testable correction:** Pass the returned tenant credential to subsequent
  CLI commands as `WYRD_API_KEY` or an equivalent supported CLI credential
  input, removing the fixture exchange from this journey. Update the operator
  documentation to use that same handoff and prove issuer configuration plus
  restricted-principal creation against the real server.

## Verification limits

This was a static review. Existing appended command evidence was examined but
not treated as proof of properties the tests do not assert. The approved
duplicate machinery is gone: root `openapi.yaml`, `gen_openapi.rs`, YAML
`utoipa` features/dependencies, YAML endpoint, OpenAPI codegen lane, docs
snapshot parsing, and release-digest ownership. JSON schemas, Python `.pyi`,
TypeScript declarations/error unions, Rust SDK re-export, docs generation, and
client-tier checks remain.

No source files or candidate commits were changed.
