# Public Contract, HTTP, SDK, CLI, MCP, and Docs Review

## Reviewed Boundary

Fresh Wave-1 review of candidate
`c9e1092bbdb4df3781eb91b0eb33150e00df7623` against base
`c5c20754a167e8f4d74a555a720bd51df6179a6f`, limited to public typed
contracts, HTTP routing, runtime OpenAPI, stable error projection, shared-client
transport and authentication renewal, Rust SDK projection, CLI credential
handoff, MCP catalog/operations/authentication, generated schemas, public docs
and LLM indexes, and the user/agent journeys that prove those surfaces.

The approved authority is `SPEC-admin-principals` revision 10, status
`approved`, SHA-256
`05982825655110b7a514f16ffdd953e4b1e1df4b7422e9e77461f2e514a10837`.
I read all original `TASK-001` through `TASK-008` packets, the complete
whole-branch-04 verdict and contract findings, its validated ledger, and
`TASK-001-008-R4-close-cumulative-findings.md`. I received no other
whole-branch-05 review conclusion or intended verdict.

## Authority and Source Coverage

| Boundary | Authority and source inspected | Result |
|---|---|---|
| Public contract and stable errors | `AGENTS.md`; `.agents/skills/wyrd-task-review/SKILL.md`; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/references/languages/{errors,testing-workflows}.md`; approved spec, task packets, complete base-to-candidate diff; `wyrd-spec` administrative DTOs/error catalog; server route annotations and response mapping | **FAIL — `CONTRACT-05-01`.** `/auth/token` can return stable errors omitted from its OpenAPI operation. |
| Runtime OpenAPI | `REQ-049`, `AC-014`, `AC-019`; `http/openapi.rs`, assembled router, all public route modules, runtime contract tests, codegen wiring | PASS apart from `CONTRACT-05-01`. The runtime JSON document remains the sole OpenAPI artifact; no snapshot, YAML endpoint, file generator, release digest, or generated-client contract was reintroduced. |
| Shared client and Rust SDK | `REQ-047`, `REQ-048`, `AC-018`; `wyrd-client` credential resolution, auth middleware, HTTP transport, principal/platform handles and tests; `wyrd-sdk-rust` re-export | **FAIL — `CONTRACT-05-03`.** Tenant credential revocation selects the raw one-shot request helper and therefore skips the shared reactive-renewal contract. The Rust SDK otherwise remains a thin projection of `wyrd-client`. |
| CLI | `REQ-036`, `REQ-040`, `REQ-047`; CLI client construction and auth/platform/principal commands; real-server operator journey; public CLI docs | PASS. CLI HTTP calls use `wyrd-client`, and the tenant admin credential returned by provisioning is handed to subsequent real CLI operations through the supported credential input. |
| MCP server and client | `architecture/references/languages/agent-harness.md`; `REQ-036`, `REQ-047`, `REQ-048`, `AC-014`, `AC-018`; `wyrd-mcp` transport; server catalog/dispatch/principal tools; real MCP discovery, authorization, act/observe, and renewal journeys | **FAIL — `CONTRACT-05-02` and `CONTRACT-05-04`.** MCP retains a second Wyrd HTTP/header/retry implementation, and the new principal tools do not publish typed output schemas or derive their input schemas from `wyrd-spec`. |
| Generated and documented surfaces | Doctrine, agent-harness, SDK/stub, testing, and public-surface authorities; JSON schemas, Python stubs/exports, TypeScript declarations/error unions, docs pages, `llms.txt`/`llms-full.txt`, docs/codegen tasks | PASS. No stale bootstrap-key, three-principal, machine-refresh-token, or removed Python/TypeScript administrative binding contract was found in the changed public material. |

CodeGraph was used first, then the full cumulative diff and the current source
owners, callers, tests, and generated/documented consumers were traced. Changes
outside this domain were not reviewed for acceptance.

## Material Proposed Findings

### `CONTRACT-05-01` — `/auth/token` omits reachable stable errors from OpenAPI

- **Classification:** `INCORRECT`
- **Violated obligation:** `REQ-049`, `AC-014`, and `AC-019` require each
  served operation to declare every reachable stable `WyrdError` code and
  require exact contract evidence without an independent error list.
- **Exact location:**
  `crates/wyrd/wyrd-server/src/components/auth/routes.rs:72-99`,
  `crates/wyrd/wyrd-auth/src/audit.rs:91-106`,
  `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:778-804`,
  `crates/wyrd/wyrd-auth/src/refresh.rs:66-86`, and
  `crates/wyrd/wyrd-server/src/http/openapi.rs:258-307`.
- **Evidence:** The operation declares only one 503 code,
  `WYRD_AUTH_503_VERIFY_UNAVAILABLE`, and no 500 response. API-key exchange
  appends its grant audit through `append_auth_audit`, whose documented and
  implemented failure is `WyrdError::AuditUnavailable`
  (`WYRD_AUDIT_503_UNAVAILABLE`); `ExchangeError::Wyrd` returns that error
  unchanged. Refresh rotation has the same pass-through through
  `RefreshError::Wyrd`. Token issue failures map to `WyrdError::Internal`, and
  the route also returns `Internal` when its signing handle is absent. The
  OpenAPI test reconstructs and validates only codes already written in
  response descriptions, so it cannot detect any of these omissions.
- **Observable consequence:** A client implemented from `/openapi.json` cannot
  implement complete handling for actual token-grant 503 and 500 responses,
  including the audit fail-closed response central to this change.
- **Required testable correction:** Declare every stable 503 and 500 code the
  operation can return, including `WYRD_AUDIT_503_UNAVAILABLE`, using the
  existing typed operation/error catalog. Add focused failure injection for
  token-exchange/refresh audit failure and token issue/configuration failure
  that asserts the runtime problem and its OpenAPI declaration; do not add a
  parallel error catalog.

### `CONTRACT-05-02` — MCP still owns a second Wyrd HTTP authentication path

- **Classification:** `VIOLATION`
- **Violated obligation:** `REQ-047` forbids a Wyrd-owned caller from
  constructing its own HTTP/authentication/status behavior; `REQ-048` and
  `AC-018` assign bounded reactive renewal to `wyrd-client`. The accepted R4-9
  correction additionally required no independent Wyrd header or raw-client
  construction in the MCP crate.
- **Exact location:** `crates/wyrd/wyrd-mcp/src/client.rs:31-75` and
  `crates/wyrd/wyrd-mcp/src/client.rs:91-175`.
- **Evidence:** `WyrdMcpHttpClient` stores a raw `reqwest::Client` and
  `AuthMiddleware`, defines both Wyrd header names locally, formats and inserts
  the bearer and request id itself, interprets rmcp errors through its own
  `is_unauthorized`, and implements its own force-refresh/one-replay loop. The
  values are cloned from `WyrdClient`, but the request never traverses the
  shared `HttpTransport::send_with_retry` owner. This directly contradicts the
  R4 evidence row claiming that no raw client or Wyrd header construction
  remains in the crate.
- **Observable consequence:** Wyrd has two owners for header vocabulary, 401
  classification, request decoration, and renewal/replay semantics. A change
  to the shared transport can leave MCP behavior observably different while
  both local test suites remain green.
- **Required testable correction:** Put the authenticated rmcp-operation
  decoration and bounded refusal replay behind an existing `wyrd-client`
  transport/auth capability; retain only rmcp protocol framing in `wyrd-mcp`.
  Prove through that shared owner that a first 401 causes exactly one durable
  re-exchange and replay and that a second 401 is terminal, with no Wyrd header
  constants, bearer construction, raw client ownership, or independent status
  classification left in `wyrd-mcp`.

### `CONTRACT-05-03` — tenant credential revocation bypasses reactive renewal

- **Classification:** `INCORRECT`
- **Violated obligation:** `REQ-048` and `AC-018` require `wyrd-client` to
  re-exchange a machine's durable credential after one authentication refusal
  and retry the refused request at most once.
- **Exact location:**
  `crates/shared/wyrd-client/src/principals/handle.rs:147-170`,
  `crates/shared/wyrd-client/src/transport/http.rs:307-345`, and
  `crates/shared/wyrd-client/src/transport/http.rs:628-746`.
- **Evidence:** `Principals::revoke_credential` calls `request_raw`. That helper
  obtains the cached bearer, sends once, and maps a non-success response; it
  contains no 401 branch or `force_refresh`. The required one-time refresh and
  replay exists only in `send_with_retry`, reached by the ordinary JSON/control
  helpers. The corresponding platform credential revocation already uses that
  shared control path, showing that DELETE/no-content does not require the raw
  streaming helper.
- **Observable consequence:** A tenant administrator authenticated by an API
  key or workload credential receives a terminal 401 when its cached access
  token is refused, even though the durable credential remains valid and every
  other control request renews once. Credential revocation therefore violates
  the shared-client contract on a security-sensitive operation.
- **Required testable correction:** Route tenant credential revocation through
  the existing no-content/JSON control request path (or make authenticated raw
  requests reuse the same single transport owner if another real streaming
  caller requires it). Add a focused test on this public method proving one
  re-exchange/replay after the first 401 and a terminal second refusal.

### `CONTRACT-05-04` — principal MCP tools do not expose typed input/output contracts

- **Classification:** `VIOLATION`
- **Violated obligation:** The agent-harness contract requires stable MCP
  names, typed inputs and outputs from `wyrd-spec` schemas, stable errors, and
  schema/behavior tests; `REQ-036` requires MCP to project the shared contract
  rather than introduce a surface-specific model.
- **Exact location:**
  `crates/wyrd/wyrd-server/src/mcp/principals.rs:71-162` and
  `crates/wyrd/wyrd-server/src/mcp/principals.rs:194-247`, with the existing
  shared response at
  `crates/wyrd-spec/src/auth/tenant_principals.rs:69-75`.
- **Evidence:** Both tool input schemas are handwritten JSON objects, separate
  from the private deserialization structs. Neither `Tool` descriptor sets an
  output schema. The listing result serializes the existing schemars-backed
  `CredentialListResponse`, but the catalog never advertises that schema; the
  revocation result is an ad hoc `{"revoked": true, "credential_id": ...}`
  shape with no `wyrd-spec` DTO. The installed rmcp API supports typed/raw
  output schemas, while the MCP journey checks names and selected result fields
  but not input/output schema publication or result conformance.
- **Observable consequence:** An agent discovering these new administrative
  tools cannot learn their result contract from the MCP catalog, and the
  handwritten input/result shapes can drift independently from the HTTP and
  `wyrd-spec` contracts.
- **Required testable correction:** Define or reuse schemars-backed
  `wyrd-spec` DTOs for the principal tool inputs and outputs, reuse
  `CredentialListResponse`, and attach both input and output schemas to the
  descriptors. If revocation needs a structured acknowledgement, give it one
  minimal shared typed response. Extend the real catalog journey to assert the
  published schemas and validate both tool results against them.

## Verification Limits

- This was a static, review-only audit. I did not modify candidate source or
  run builds/tests. A repository-owned `cargo nextest` journey was already
  active, so I did not start a competing Cargo process.
- I examined the R4 packet's recorded green format, lint, OpenAPI, shared-client,
  CLI, MCP, docs, codegen, and journey evidence. Those results support the
  branches they execute, but none asserts the omitted `/auth/token` errors, the
  absence of a second MCP transport owner, reactive renewal by the tenant
  revocation method, or MCP input/output schema publication.
- `git diff --check` passed for the immutable base-to-candidate range. The
  approved spec checksum was rechecked. Candidate HEAD was rechecked before
  writing this report and remained
  `c9e1092bbdb4df3781eb91b0eb33150e00df7623`.

## Overall Result

**FAIL**

The CLI handoff, Rust SDK projection, public documentation, generated artifacts,
and most HTTP/OpenAPI projection satisfy this review's boundary. The candidate
still omits reachable token errors from the canonical runtime contract, retains
an explicitly forbidden second MCP authentication transport, bypasses reactive
renewal on tenant credential revocation, and ships new agent tools without the
required typed output contract.
