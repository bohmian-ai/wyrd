# TASK-001 system resilience review

Result: **PASS** for the changed runtime and recovery boundaries. No material proposed findings.

## Immutable subject and scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/ambient-auth`.
- Base: `5bcd9e4872f1376f1722922fa63b67384243be04`.
- Candidate: `824f0c491ea2686977c9da9fce03ac4ef65c01b1`.
- External example: `bef9fc0..b283666df93a32cb73aa8c07f881d9271797dff0`.
- Authority: approved ambient-auth specification r3, original TASK-001, root AGENTS.md, agent-rules, design/doctrine, security posture, reference router, spec-driven-development, maintainer-style, testing-workflows, and PyO3-boundaries.

Reviewed the cumulative 27-file change and expanded the supplied navigation map using CodeGraph before source searches. No other discovery reports were read. This report evaluates runtime failure propagation and recovery; it does not substitute for the independent whole-task acceptance and standards reports.

## Deployed path and source coverage

| Path | Source and affected capability | Assessment |
|---|---|---|
| Application process → stock provider/MCP client → TS fetch → public Wyrd edge | `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1569` obtains a bearer through existing `accessToken()` on each invocation, removes the three provider carriers, and forwards input and request-init fields to fetch | No new cache, worker, retry loop, durable state, or server lifecycle coupling. Provider and MCP calls use the same narrow adapter. |
| Python application → httpx/httpx2 auth → native client → Rust | `_http_auth.py:39-52`; `sdks/wyrd-sdk-python/src/client.rs:118-139` | Sync token acquisition releases the GIL through the existing bridge; async auth uses `asyncio.to_thread`. The getter retains the exact client, including delegated identity, rather than resolving a second ambient identity. |
| Both languages → credential exchange/renewal | TS native `client.rs:103-113`; shared `client.rs:438-440`; shared `auth.rs:638-659,840-889,942-1013` | Both call `AuthMiddleware::bearer`; the existing mutex, credential-specific exchange, cache, and renewable single-flight owner remain authoritative. |
| CLI helper process → shared client → stdout | `auth/access_token.rs:28-32`, `auth/mod.rs:53`, CLI `client.rs:39-44,79-85` | Reuses ambient construction and shared token acquisition. A failure returns the CLI error before any token output. No parallel credential store or daemon is added. |
| Public edge → gateway or MCP authorization | server `components/auth/token_extract.rs:172-232`, `http/middleware/authenticate.rs`; SDK header changes | Gateway still refuses multiple credential carriers. The adapter removes placeholder carriers before dispatch and supplies the existing Wyrd header. Server authentication and authorization are unchanged. |
| Sibling administration, Bifrost, tracing | Python gateway module/stub removals, TS removal of `gatewayFetch`, shared Rust owner/callers | Gateway administration remains exported. SDK changes do not alter shared transport/cache behavior, Bifrost owners, trace exporters, permission decisions, Scribe outbox, or server shutdown. |
| Caller examples and journeys | New stock-client guide; TS/Python provider and MCP journeys, local-development adapters, CLI journey; external `src/agent.ts:19-20` | New call sites use per-request transport authentication. Test helper changes remain test-only. Documentation and declarations introduce no deployed background mechanism. |

## Failure and recovery paths

| Credible interruption | Failure boundary and surviving capabilities | Recovery evidence and limits |
|---|---|---|
| `/auth/token` unreachable or exchange refused | A caller requiring exchange receives the existing structured client/server error before fetch/auth-flow yield; no gateway/provider request is dispatched. Cached fresh tokens remain usable through the unchanged owner. No shared server is terminated. | `TokenExchange::new` (`auth.rs:248-263`) retains the configured total timeout and refuses redirects; `bearer` only replaces cached state after success. A later call retries through the same owner. Revoked-key TS journey checks a WyrdError cause and that the upstream received nothing. No live outage/recovery test was run for this review. |
| Token becomes stale in a long-lived provider/MCP process | Both adapters ask the client on every request; they retain no bearer. Concurrent request token acquisition reaches the shared single-flight gate. | Shared `client.rs:503-541` already tests re-exchange inside the expiry skew; new TS and Python unit tests prove two requests ask for two tokens. This composition supports renewal without duplicating its mechanism. No new real-expiry end-to-end journey is present. |
| Application cancels while acquiring a token | TS forwards the original signal to fetch after token acquisition. Python `to_thread` cancellation stops the waiting coroutine but does not kill the underlying token thread. These adapters do not promise an earlier abort of token acquisition. | Shared renewable mint cancellation explicitly retains an in-flight mint for the next caller (`auth.rs:970-1013`). API-key/workload exchange uses existing bounded IO. No changed code deletes persisted login state or starts another lifecycle owner. This preserves the previous helper's cancellation behavior; adding an abort API is outside this task. |
| Network/gateway/provider response fails after token acquisition | Error belongs to the provider/MCP request and its stock client's existing retry policy. The adapter adds no retry and cannot crash the Wyrd process. | Request-init fields, including body/method/signal, reach native fetch. Python yields the original request. Recovery remains the stock client's next request with newly acquired current credentials. No server code is changed. |
| Client process crashes/restarts or replica rolls | Adapter state is only a client reference. No pending writes or auth state are made durable in either language layer. Restart reconstructs through the existing credential chain. | Shared middleware keeps renewable/delegated tokens in memory and has existing API-key disk cache policy. Saved-login renewal and file ownership are not changed. Public edge replica behavior is unchanged. |
| HTTP extras unavailable in Python | Accessing `http_auth` raises the specific ImportError; constructing/using unrelated native capabilities and retained gateway administration does not import the new optional module. | Getter imports lazily; `wyrd.gateway` no longer imports the auth helper. No component error becomes a process exit. |
| Invalid/revoked credential | Refuses the requesting operation; no fallback to provider placeholder, different principal, or API key is added. | TS revoked-key journey and native structured-error projections support this boundary. Existing valid cached access-token lifetime remains unchanged by this task. |

## Proof assessment

Inspected the actual assertions in TS gateway unit/inference/MCP journeys, Python unit/client and integration/gateway tests, local-development call-site changes, and the CLI token journey. They cover header replacement, per-request acquisition, stock client interoperability, request refusal, and token-only CLI output. The original task records successful focused provider/MCP and CLI runs plus format/lint/codegen/typecheck checks; these are recorded claims, not reruns performed by this reviewer. Full integration and local-development aggregate lanes are explicitly deferred to change review.

The external example is a narrow source change and its operational proof is incomplete: `package.json` resolves `file:vendor/wyrd-ai`, while that vendor directory is ignored and is not part of the immutable external commit. The current local vendor source lacks `httpAuth`, matching the task's statement that the example was not built. This review cannot establish deployability of that external dependency closure; the whole-task reviewer must reconcile AC-006. No missing MCP connection was inferred from a source file that has none.

## Proposed findings

None. No changed owner introduces a reachable shared-process crash, sibling capability shutdown, unbounded retry loop, lost durable state, altered renewal authority, or recovery regression. Failure is appropriately contained at the requesting operation. No implementation changes or tests were executed by this reviewer.

HEAD was checked as `824f0c491ea2686977c9da9fce03ac4ef65c01b1` before completing this report.
