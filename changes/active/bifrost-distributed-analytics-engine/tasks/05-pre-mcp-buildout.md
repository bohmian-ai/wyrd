---
id: BIFROST-R5-T05-PRE-MCP-BUILDOUT
title: Mount authenticated Streamable HTTP MCP and connect the official Rust client
kind: implementation
mode: RECONCILE
status: proposed
spec: SPEC-bifrost-distributed-analytics-engine
spec_revision: 5
depends_on: [BIFROST-R5-T04A-PRODUCTION-ACTIVATION]
requirements: [REQ-007, REQ-010, REQ-012]
invariants: [INV-002, INV-004, INV-005, INV-008, INV-009]
acceptance: [AC-006, AC-008, AC-009]
parent_task: BIFROST-R4-T05-MCP
frozen_candidate:
  commit: 5d3cc09f75d0d3583164baeb481179ed92358808
  task_blob: 52ede2f8a23ad5187778021dc1274231ac02b093
  uncommitted: none
---

# MCP client/server buildout

## Outcome and value

One official `rmcp` Streamable HTTP endpoint is mounted at `/mcp` on the
existing `wyrd-server` listener. A real local or remote Rust MCP client can
initialize, list tools, call a minimal test-only capability, cancel work, and
close cleanly while the existing Wyrd edge binds tenant, principal,
delegation, roles, request ID, and audit context from credentials.

Required execution skill: `$wyrd-implement`.

## Reconciliation amendment

### Retained

- Task 04 and its Task 04A completion successor remain unchanged. MCP work
  starts only after `BIFROST-R5-T04A-PRODUCTION-ACTIVATION` completes.
- The existing public listener, JWT verifier, `x-wyrd-access-token` contract,
  optional `wyrd-request-id`, authorization, tenant derivation, principal Card
  scope, delegated `act` chain, `Caller`, and audit context remain authoritative.
- `wyrd-client::AuthMiddleware` remains the sole client credential and token
  refresh owner.

### Deleted or invalidated

- The frozen Task 05 assumption that an in-process Skald `ToolRegistry` is an
  MCP server is invalidated.
- The standalone `wyrd-mcp` binary and its Skald/Vala/Arrow server behavior are
  removed; the crate becomes the narrow first-party Rust MCP connectivity
  owner.
- Server self-calls through `wyrd-client` or `vala-sdk`, custom JSON-RPC, stdio,
  legacy HTTP+SSE, WebSocket, another listener, and a separately deployed MCP
  service are not carried forward.

### Unfinished work moved here

- Pin the official `rmcp` SDK, mount `/mcp`, preserve verified Wyrd context at
  the adapter edge, add the thin authenticated HTTP decorator, and prove the
  real protocol lifecycle and credential rejections.
- Bifrost discovery and query behavior remains wholly in successor Task 05.

## Owners and exact write set

- Workspace `Cargo.toml` and `Cargo.lock`: pin `rmcp = 3.2.0` with default
  features disabled. `wyrd-server` enables only `server`, `macros`, and
  `transport-streamable-http-server`; `wyrd-mcp` enables only `client` and
  `transport-streamable-http-client-reqwest`.
- `crates/wyrd/wyrd-server/Cargo.toml` and
  `crates/wyrd/wyrd-server/src/{lib.rs,http/router.rs,mcp/mod.rs}`: declare the
  MCP module and mount one
  `StreamableHttpService` at `/mcp` inside the existing request-ID,
  authentication, bounds, and shutdown layers.
- `crates/wyrd/wyrd-server/src/{state.rs,app/server.rs}`: store one
  `tokio_util::task::TaskTracker` for MCP in-flight work on `AppState`, close it
  after transport admission stops, and await it within the existing server
  shutdown deadline. Do not add a custom counter or another deadline.
- `crates/wyrd/wyrd-testing/src/server.rs`: add one default-false
  `WyrdTestServerBuilder::with_mcp_context_probe_for_test()` control that opts
  only connectivity journeys into the test-support context probe. Ordinary
  `WyrdTestServer::{start_in_process,start_bound}` startup never registers it.
- `crates/wyrd/wyrd-server/src/components/auth/{principal_extractor.rs,token_extract.rs}`:
  change `AuthenticatedPrincipal` to retain the existing
  `Arc<wyrd_auth_verify::VerifiedToken>`, expose narrow `principal()` and
  `delegation_chain()` accessors plus a crate-private `from_verified` constructor,
  and have `verify_authenticated_principal` pass the verifier result directly
  to that constructor. Update the existing `From<AuthenticatedPrincipal> for
  Principal` conversion to clone only the verified principal when an owned
  service input is required. Keep `RequestId` as its existing separate
  extension and keep `Caller` unchanged. Do not accept or reconstruct identity
  in tool arguments.
- `crates/wyrd/wyrd-server/src/components/auth/{caller_extractor.rs,routes.rs}`
  and `crates/wyrd/wyrd-server/src/components/eval/routes.rs`: mechanically
  replace direct moves or borrows of `AuthenticatedPrincipal.principal`.
  `Caller::from_request_parts` converts the wrapper into its existing owned
  `Principal`; auth routes borrow `principal()`; eval handlers use the existing
  wrapper-to-`Principal` conversion where they require ownership. No consumer
  stores a second principal beside the retained verified token.
- `crates/wyrd/wyrd-mcp/{Cargo.toml,src/lib.rs,src/client.rs}`: remove the
  standalone binary and Skald/Vala/Arrow server dependencies. Implement only
  `rmcp`'s `StreamableHttpClient` transport trait by decorating the existing
  reqwest transport with headers obtained from `AuthMiddleware`; do not wrap
  `rmcp`'s client, `ClientHandler`, lifecycle, tool, or protocol types.
- `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/{main.rs,connectivity.rs}`: replace
  the obsolete `layout` and `rbac` registrations with the ordinary
  `connectivity` module and replace in-process registry proof with a real
  client-to-server journey. Add only the smallest existing `wyrd-testing`
  server helper needed to expose a test-only capability and observe
  cancellation.

No other production file is in scope beyond the owners and mechanical consumers
listed above. In particular, this task does not change Bifrost catalog/query
behavior, `vala-sdk`, or Skald.

## Ordered implementation scenarios

### Scenario 1 — One authenticated endpoint and real protocol lifecycle

**Trace.** REQ-007, REQ-010, REQ-012; INV-002, INV-004, INV-005, INV-008, INV-009; AC-006,
AC-008, AC-009. Revision 5 MCP transport, authentication, local/remote-client,
and lifecycle obligations.

**RED.** Add ignored journey
`connectivity::pg_tests::streamable_http_client_reaches_authenticated_server_context`. Start
`WyrdTestServer`, mint a real scoped access token with an `act` chain, connect
an official `StreamableHttpClientTransport` to the production `/mcp` route,
initialize, list tools, invoke one test-only capability, cancel its pending
call, and close the client. The capability reports only trusted test evidence
for tenant, principal, delegation, roles, request ID, and cancellation. Start
the server through
`WyrdTestServer::builder().with_mcp_context_probe_for_test().start_bound()` and
assert the probe is present only for that opted-in fixture; compiling
`test-support` alone does not register it.

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey -E 'test(=connectivity::pg_tests::streamable_http_client_reaches_authenticated_server_context)' --run-ignored=all"
```

Add the focused server test
`components::auth::principal_extractor::pg_tests::authenticated_principal_preserves_verified_delegation_chain`.
Mint a delegated token and assert `AuthenticatedPrincipal::principal()` is the
verified effective principal while `delegation_chain()` preserves the complete
initiator-first chain from the same `VerifiedToken`.

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::auth::principal_extractor::pg_tests::authenticated_principal_preserves_verified_delegation_chain)'"
```

**GREEN.** Mount `StreamableHttpService` at `/mcp` on the existing router and
listener. Use `NeverSessionManager`, advertise only the current 2026-07-28
protocol, disable legacy session mode, require stateless protocol metadata,
and let `rmcp` own framing, MCP headers, sessions, and request cancellation.
Build `StreamableHttpServerConfig` with
`state.shutdown_token.clone()` as its `cancellation_token`, so Wyrd process
shutdown stops admission and cancels every active MCP request. Call
`disable_allowed_hosts()` because Wyrd's public listener and gateway own remote
host routing, and call `disable_allowed_origins()` explicitly: Wyrd has no
authoritative browser-Origin allowlist, so Origin enforcement remains disabled
behind the existing gateway/TLS and mandatory JWT boundary. A browser-Origin
policy requires a separately approved configuration source and tests. Read
`AuthenticatedPrincipal` and the separate
`RequestId` from request extensions. The handler derives the existing `Caller`
for server operations and consults `delegation_chain()` only for
delegation-aware attribution and audit.

**REFACTOR.** Keep one concrete server handler and one route. Do not introduce
a transport trait, handler framework, session store, sticky routing, second
listener, or generic tool-injection system.

### Scenario 2 — Thin reuse of the existing credential owner

**Trace.** REQ-012; INV-002, INV-009; AC-009. Revision 5 client reuse and
header requirements.

**RED.** Add
`client::tests::decorator_adds_current_wyrd_headers_once_per_request`.
Use a bounded local HTTP recorder and issue two MCP requests. Assert the
decorator obtains the current bearer for each request, adds
`x-wyrd-access-token: Bearer <JWT>`, preserves a supplied `wyrd-request-id` or
generates one when absent, leaves all MCP headers untouched, never writes the
standard `Authorization` header, and delegates each request exactly once.

```bash
mise exec -- cargo nextest run --locked -p wyrd-mcp --lib -E 'test(=client::tests::decorator_adds_current_wyrd_headers_once_per_request)'
```

**GREEN.** Implement `rmcp`'s existing HTTP transport trait for one small
decorator around its reqwest client. On each request call
`AuthMiddleware::bearer()`, preserve a caller-supplied request ID from rmcp
custom headers or use `AuthMiddleware::request_id(None)`, and delegate all MCP
behavior once. Do not add reactive 401 parsing or retry: `bearer()` already
owns proactive refresh, while rmcp's reqwest transport erases the typed status
and Wyrd problem body for a pre-protocol 401.

**REFACTOR.** Keep the decorator in `wyrd-mcp`, where the `rmcp` dependency is
already required. Do not add `rmcp` to `wyrd-client`, copy token exchange, or
invent a Wyrd MCP client facade.

### Scenario 3 — Credentials fail at the existing edge and all cancellation joins

**Trace.** REQ-007, REQ-010, REQ-012; INV-002, INV-004, INV-005, INV-008, INV-009; AC-006,
AC-008, AC-009. Revision 5 rejection and cleanup obligations.

**RED.** Add ignored journey
`connectivity::pg_tests::mcp_rejects_credentials_and_joins_request_and_process_cancellation`.
Start its bound server with the same explicit
`with_mcp_context_probe_for_test()` opt-in. Through a real `reqwest` client,
prove missing and malformed/invalid credentials are rejected
by the production `/mcp` edge with the canonical HTTP status and Wyrd problem
fields. Use the official rmcp client only after protocol admission: prove a
valid under-scoped principal is rejected and an allowed principal can invoke
the tool. The under-scoped case lacks `Permission::bifrost_query_read()`; assert
the test capability returns the existing permission denial and that the
existing audited-denial path records the required permission, verified caller,
request ID, denial, and failure before the tool returns. Prove an allowed call
binds the verified tenant/principal rather than client arguments.

Drive two cancellation cases with deterministic entry and cleanup latches.
First retain the returned rmcp `RequestHandle`, invoke
`RequestHandle::cancel(None).await`, and assert the capability observes
`RequestContext<RoleServer>::ct.cancelled()` and joins its work before client
close. Then start another pending call, cancel the owning `WyrdTestServer`
without cancelling its request handle, and assert `AppState::shutdown_token`
cancels the request context. Hold cleanup after the HTTP cancellation response,
assert the server drain remains pending while the MCP tracker is non-empty,
release cleanup, and assert the tracker reaches zero before drain returns.

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey -E 'test(=connectivity::pg_tests::mcp_rejects_credentials_and_joins_request_and_process_cancellation)' --run-ignored=all"
```

**GREEN.** Reuse the existing verifier, request-ID handling, authorization,
tenant derivation, principal Card scope, delegation, and audit context. One
`#[cfg(feature = "test-support")]` context-probe tool owned by
`wyrd-server::mcp` is registered only when the test builder's explicit probe
control is true. It derives `Caller`, then calls
`query::service::authorize_audited` with
`Permission::bifrost_query_read()`, operation `wyrd.mcp.test.context`, and
resource `mcp.test.context` before returning any trusted context. Its pending
operation receives `RequestContext<RoleServer>`, obtains an RAII token from
`AppState`'s MCP `TaskTracker`, and holds it through cancellation cleanup and
result creation. Race only its owned work against `context.ct.cancelled()`;
either rmcp request cancellation or the server-configured
`AppState::shutdown_token` reaches that same branch. After the supervised
transport drain stops admission, `BoundServer::run` closes the tracker and
awaits `TaskTracker::wait()` with `timeout_at` using the unchanged process
deadline; a non-empty tracker at the deadline is a lifecycle failure, never a
clean drain. At the adapter edge translate public derive-backed
`WyrdError` values into protocol-correct MCP errors. Never accept tenant,
principal, delegation, roles, or execution path as tool input.

**REFACTOR.** Delete any duplicated auth parsing or context construction. The
test-only capability remains unavailable in production builds, absent from
ordinary `WyrdTestServer` MCP startup, and is not a general extension point.

## Cross-scenario decisions and authority

- There is exactly one `/mcp` endpoint and one public listener. The same route
serves local and remote clients.
- The normal handler catalog contains only production tools: none in this
  predecessor and exactly the three Bifrost tools after Task 05. The context
  probe is a default-off connectivity-fixture option and never joins the
  ordinary catalog.
- `rmcp` owns the protocol. Wyrd owns identity, authorization, tenancy, audit,
and public Wyrd errors at the adapter boundary.
- `StreamableHttpServerConfig::cancellation_token` is the existing
  `AppState::shutdown_token`; per-request work additionally observes
  `RequestContext<RoleServer>::ct`. Every tool request holds an `AppState` MCP
  `TaskTracker` token through cleanup; after transport admission stops,
  `BoundServer::run` closes and awaits that tracker under its existing shutdown
  deadline before it may report a clean drain.
- rmcp Host and Origin validation are explicitly disabled. The deployed gateway,
  TLS, and Wyrd JWT edge are the current trust boundary; browser-Origin
  enforcement is out of scope until an authoritative allowlist is specified.
- The client decorator obtains `AuthMiddleware::bearer()` for every request and
  never implements reactive 401 retry or parses rmcp transport error strings.
- `wyrd-server::mcp` and its Bifrost MCP adapters do not invoke or depend on
  `wyrd-client`, `vala-sdk`, or Skald. Retain the existing server-level
  `wyrd-client` dependency used by `ServerBifrostPeerCredentials`; it is
  unrelated private Oracle peer credential reuse. `wyrd-mcp` is client
  connectivity only.
- Task 05 may add the three Bifrost tools to this handler; no Bifrost tool is
implemented here.

Authority: `architecture/wyrd-design.md` §§Client model, MCP and agent
surfaces; `architecture/wyrd-doctrine.mdx`; `architecture/wyrd-security-posture.md`
§§Trust boundaries, Authorization and policy; `architecture/agent-rules.md`;
`architecture/operations/reliability-and-recovery.md` §Oracle failure boundaries;
`architecture/references/languages/{agent-harness,errors,testing-workflows}.md`;
and `AGENTS.md` §§2, 3, 9, 11.

## Broader verification

Do not run any lane in this section during scenario implementation. Complete
each Red–Green–Refactor cycle with only its exact named command, and proceed
only after that focused test passes. After every required focused command in
this task passes, run the broader lanes below once as final consolidation; do
not restart the full set after each edit.

```bash
mise run fmt
mise run lints
mise run test:wyrd
mise run test:e2e
mise run test:bifrost:journey:mcp
mise run check:client-tier
mise run check:unwrap-audit
git diff --check
```

## Completion evidence and stop conditions

Provide the dependency/feature diff, production `/mcp` router proof, verified
request-context and direct-consumer propagation, per-request decorator/header
evidence, direct-HTTP canonical credential failures, real rmcp
initialize/list/call/cancel/close transcripts, the exact permission and
audited-denial evidence, request-cancellation joining, and tracker-zero
process-shutdown evidence before server drain. Return `SPEC_REVISION_REQUIRED` if implementation
needs another transport/listener/service, server-side Skald or SDK self-calls,
identity tool arguments, a custom MCP protocol layer, or weaker Wyrd edge
security. Stop for an approved-plan revision if Task 04A changes the existing
auth, listener, or shutdown seams assumed here.
