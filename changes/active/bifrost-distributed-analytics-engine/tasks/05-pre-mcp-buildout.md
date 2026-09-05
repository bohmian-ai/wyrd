---
id: BIFROST-R5-T05-PRE-MCP-BUILDOUT
title: Mount authenticated Streamable HTTP MCP and connect the official Rust client
kind: implementation
mode: RECONCILE
status: proposed
spec: SPEC-bifrost-distributed-analytics-engine
spec_revision: 6
depends_on: [BIFROST-R6-T04-REMEDIATION-02-EXACT-BINDINGS-AND-ATTEMPT-EVIDENCE]
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

- Task 04, Task 04A, and the first Task 04 remediation remain the reviewed
  implementation history. MCP work starts only after
  `BIFROST-R6-T04-REMEDIATION-02-EXACT-BINDINGS-AND-ATTEMPT-EVIDENCE` closes the
  final validated Task 04 reliability findings.
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
security. Stop for an approved-plan revision if the Oracle remediation changes
the existing auth, listener, or shutdown seams assumed here.

## Execution evidence

### Scenario 1 — one authenticated endpoint and real protocol lifecycle

RED. `connectivity::pg_tests::streamable_http_client_reaches_authenticated_server_context`
failed with `Error: ConnectionClosed("initialize response")`. A direct-`reqwest`
probe of the same POST showed the cause: `HTTP 200` carrying
`{"code":-32022,"message":"Unsupported protocol version","data":{"requested":"2026-07-28","supported":["2026-07-28"]}}`.
`components::auth::principal_extractor::pg_tests::authenticated_principal_preserves_verified_delegation_chain`
failed to compile with E0599 (`no method named principal`/`delegation_chain` on
`AuthenticatedPrincipal`) before the extractor retained its verified token.

GREEN. `AuthenticatedPrincipal` retains `Arc<VerifiedToken>` behind
`principal()`/`delegation_chain()` and a crate-private `from_verified`; the
`/mcp` route mounts one `StreamableHttpService` inside the existing
`require_authenticated` and request-id layers; the handler recovers both
extensions from the transport's `axum::http::request::Parts`. The apparent
version contradiction is `rmcp`'s lifecycle split, not a mismatch: `initialize`
negotiates only down to a pre-2026-07-28 revision, and `ServiceExt::serve` takes
that path by default. Advertising exactly the current revision — as this task
requires — therefore admits only `server/discover` plus self-contained
per-request metadata, so the journey opts into
`ClientLifecycleMode::Discover`. Both tests pass.

Consequence, recorded on `WYRD_MCP_PROTOCOL_VERSION`: a client that cannot speak
2026-07-28 cannot reach `/mcp` at all. Confirmed as intended, not pending.
This is an interoperability ceiling for the successor task's Bifrost tools and
for any agent outside Wyrd's own SDKs; widening the advertised set is a product
decision, not a local change in the adapter.

A second journey, `connectivity::pg_tests::default_server_advertises_no_mcp_tools`,
pins the other half of the catalog contract: compiling `test-support` is not
enough to expose the probe, so a fixture that did not call
`with_mcp_context_probe_for_test` advertises nothing.

### Scenario 2 — per-request Wyrd headers on the client decorator

RED. With header injection removed from `post_message`,
`client::tests::decorator_adds_current_wyrd_headers_once_per_request` failed on
`assertion left == right failed: the current bearer is obtained for every
request / left: 0 / right: 2` — no `/auth/token` exchange occurred, so no Wyrd
header could have been written.

GREEN. `WyrdMcpHttpClient` implements `rmcp`'s `StreamableHttpClient` by
delegating once to `reqwest::Client` after inserting `x-wyrd-access-token` and,
when the caller supplied none, a minted `wyrd-request-id`. The recorder mints a
rotating token that always expires inside the middleware's proactive-refresh
skew, so two MCP requests observe `Bearer access-1` and `Bearer access-2` — a
pinned first bearer fails the test. `Authorization` is never written, `accept`
and `content-type` pass through untouched, and each message produces exactly one
recorded HTTP request. Passes.

`StreamableHttpError<E>` is `#[non_exhaustive]`, so the decorator cannot
re-map exhaustively into a custom error type; it uses `type Error =
reqwest::Error` and surfaces credential failures as
`StreamableHttpError::Io(io::Error::other(..))`.

REFACTOR. `contains_key`-then-`insert` became `Entry::Vacant` under
`clippy::map_entry`.

### Scenario 3 — credentials, RBAC, and both cancellation paths

RED. The implementation preceded this test, so RED was established by mutation:
replacing `let _token = state.mcp_tasks.token()` in the probe with a unit
binding failed
`connectivity::pg_tests::mcp_rejects_credentials_and_joins_request_and_process_cancellation`
at `connectivity.rs:310` — "the drain cannot report settled while a tracker
token is still held". The token was restored and the test passes.

GREEN. One journey covers the single lifecycle: direct-`reqwest` posts prove the
public edge refuses a missing credential with `401` /
`WYRD_AUTH_401_UNAUTHENTICATED` and a malformed one with `400` /
`WYRD_AUTH_400_BAD_TOKEN_FORMAT` before MCP framing exists; an under-scoped
principal is refused through `authorize_audited` with
`WYRD_PERMISSION_403_DENIED_RBAC` and exactly one new `deny` row in
`vala.audit_outbox`; an allowed call carrying spoofed `tenant_id` and
`principal_id` arguments still reports the verified tenant and principal;
`RequestHandle::cancel(None)` reaches the handler's own `context.ct`; and
process shutdown cancels in-flight work, then blocks its drain on the tracker
until the parked invocation releases, after which
`cancel_and_join_for_test` returns cleanly. Rendezvous is deterministic through
zero-permit semaphores keyed by a caller-supplied `hold_id` — no sleeps.

### Broader verification

`mise run fmt`, `mise run lints`, `mise run check:client-tier`,
`mise run test:bifrost:journey:mcp` (3/3), and `git diff --check` all pass, as do
every task-named focused command.

Command correction: the task lists `mise run test:e2e`, which is not a task in
`mise.toml`. Substituted the e2e targets that cover the touched auth seam:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test auth_e2e --test authz_check_e2e --test identity_e2e --test-threads=2"
```

30 passed, 0 failed.

`mise.toml`'s `test:bifrost:journey:mcp` description was updated from "Bifrost
MCP surface and its RBAC" to name what the binary now proves. The lane itself
selects the target whole and needed no change.

### Limitations

`mise run check:unwrap-audit` fails, and fails identically at this change's base
commit `f6148fbbb`: `comm -13` over the sorted findings at base and at HEAD
reports zero new entries. The findings are a method named `expect` on a byte
cursor in `otlp_json.rs`/`otlp_trace_json.rs` and on test harness types, none of
them in this task's write set.

`mise run test:wyrd` does not pass cleanly on this machine. The failures are
Postgres connection exhaustion — `expected to read 5 bytes, got 0 bytes at EOF`
— and the failing set changes with `--test-threads`, which is the signature of a
resource ceiling rather than a defect. Re-running the reported failures at
`--test-threads=2` passes them (`pg_authz_check_route`, `pg_bootstrap_key`,
`pg_eval_v1_protocol`: 17/17). One genuine failure survives and is also
pre-existing: `wyrd-sql tests::transaction_discipline_is_documented` asserts on
a sentence absent from `architecture/v1/00-foundations/sql-foundation.md` at
base commit `f6148fbbb`; nothing under `architecture/` is in this write set.

## CR-005 remediation — verified delegation reaches Bifrost attribution and audit

Trace: REQ-012; AC-009; approved specification revision 6. Reviewed candidate
`171fac9242363ec95f7e260759ec5df4a7f37aab`.

### Scenario 1 — a delegated agent's real Bifrost read is attributed

RED. `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs::query::pg_tests::delegated_agent_query_is_attributed_in_its_durable_audit_record`
mints a two-hop delegated token (initiator → middle → subject), calls the real
`bifrost.query` tool, and reads the committed `bifrost.query.read_decision` row
for its own request id. Against the pre-change caller boundary — reproduced by
returning an empty chain from `Caller::from_authenticated` — it fails with
`"the read decision records a delegation chain"`, which is exactly CR-005: the
row named the effective principal with no delegation.

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(=query::pg_tests::delegated_agent_query_is_attributed_in_its_durable_audit_record)'"
```

GREEN. `Caller` gained `delegation_chain` and one inherent
`Caller::from_authenticated` constructor that derives tenant, effective
principal, and chain together from the already verified
`AuthenticatedPrincipal`; the HTTP extractor and `mcp/mod.rs` both use it, and
the two gRPC boundaries copy the chain the verifier produced (`AuthContext`
now retains it). `AuthorizedQueryContext` carries the same typed chain, set by
`query/service.rs::oracle_context` and preserved through the signed forwarding
envelope by the existing whole-struct serialization. The journey now asserts
the ordered `[initiator, middle]` chain, each step's `principal_kind`, card
identity and non-empty card scope, the effective principal, tenant, and request
id — and that the same tenant's nondelegated caller still commits a row with no
`delegation_chain` key at all.

The same journey covers the denial flow: an under-privileged delegated token is
refused with `WYRD_PERMISSION_403_DENIED_RBAC`, no rows, no additional
read-decision acceptance (`bifrost_read_decision_count_for_tenant` unchanged),
and exactly one `vala.query.sync` deny row whose typed
`delegation_attribution` detail still names the delegator. The trust boundary
retains its existing proof in
`connectivity::pg_tests::mcp_rejects_credentials_and_joins_request_and_process_cancellation`,
which passes spoofed `tenant_id`/`principal_id` arguments and still observes
the server-verified identity.

### Scenario 2 — forwarding-only ingress audits the same chain

RED. `crates/wyrd/wyrd-testing/tests/bifrost/server/query.rs` —
`prove_scheduled_analytical_completion`, reached from
`query::generated_grpc_and_scheduled_queries_share_audit_terminal_and_cleanup`
— now runs its scheduled Analytical query from the Scribe-only ingress under a
delegated context and asserts the leader's committed read decision carries the
same ordered chain. Neutering `AuthorizedQueryContext::with_delegation_chain`
failed that test; restoring it passes.

```bash
mise run test:bifrost:journey:server
```

GREEN. 4/4. Because the ingress node owns no Oracle, the audited decision was
built and committed by the remote leader from the context the signed envelope
carried, so remote and local execution demonstrably audit the same chain.

### Scenario 3 — durability across WAL acceptance and replay

RED/GREEN.
`oracle::query_audit::pg_tests::oracle_audit_relay_preserves_delegation_through_replay_pg`
publishes a delegated read decision, loses its checkpoint in the existing
commit/checkpoint window, restarts the publisher, and asserts the replayed
Postgres row still carries the exact ordered chain — proving the chain is
present before the fsynced WAL acceptance and survives framing and relay. A
nondelegated record published alongside it keeps a detail with no
`delegation_chain` key.

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --lib --all-features -E 'test(/oracle::query_audit::pg_tests::/)'"
```

7/7.

The canonical-hash half is pinned in `wyrd-spec`:
`vala::audit_detail::tests::delegation_attribution_is_omitted_when_absent_and_hashed_when_present`
proves an empty chain is omitted from `audit_detail_canonical_json` entirely
(so historical records keep their original bytes and entry hash), while adding,
lengthening, or reordering a chain each produce a different canonical detail.

```bash
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=vala::audit_detail::tests::delegation_attribution_is_omitted_when_absent_and_hashed_when_present)'
```

### Owners changed

`wyrd-spec` gained `AuditDelegationStep` (principal id, principal kind tag,
card ref, card scope — verified identity only, no credential material) and
`AuditDetail::DelegationAttribution`, plus a `default`/`skip_serializing_if`
`delegation_chain` on the existing `BifrostQueryReadDecision` and
`BifrostSecurityViolation` details. The runtime-to-wire conversion lives on
`wyrd_runtime::DelegationStep::audit_projection` /
`audit_delegation_chain`, keeping runtime types out of `wyrd-spec`. Event
building changed only at the existing owners: `wyrd-server/src/audit.rs`
(attribution-only detail, and only where `detail` was previously `None`),
`wyrd-server/src/oracle/query_audit.rs`, and the retained transactional Oracle
audit plus both read-decision builders in
`vala-bifrost-redux/src/oracle/mod.rs`. Unauthenticated peer and tail security
rejections carry an empty chain, because no verified caller exists there. No
new column, table, audit event, MCP audit service, dependency, tool argument,
or delegation policy was added, and permission evaluation still runs against
the effective principal alone.

### Broader verification

`mise run fmt`, `mise run lints`, `mise run codegen:regen` +
`mise run codegen:check`, `mise run test:bifrost:journey:mcp` (7/7),
`mise run test:bifrost:journey:oracle` (23/23),
`mise run test:bifrost:journey:server` (4/4), and `git diff --check` all pass.
`crates/wyrd-spec/schemas/bifrost_audit_event.json` and its test copy were
regenerated by the existing generator; no generated artifact was hand-edited
and no historical audit row was rewritten.

### Limitations

`mise run test:sql` fails only on `wyrd-sql tests::transaction_discipline_is_documented`,
identically on the unmodified tree (verified by `git stash`), and is the
pre-existing failure this remediation was scoped to leave alone.

`mise run test:wyrd` does not run clean on this machine: 16 failures with the
change, 19 at the same tree without it. The overlapping families
(`state::tests`, `pg_authz_check_route`, `forge_harness`, `oracle::query_audit`)
are the known shared-Postgres/parallelism ceiling recorded above, not a
regression — the whole `oracle::query_audit::pg_tests` module passes 7/7 when
run as its own selection.
