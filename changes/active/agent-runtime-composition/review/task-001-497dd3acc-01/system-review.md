# System resilience review — TASK-001

Result: **FAIL** — one proposed regression, `SYS-001`.

## Immutable subject and authority

Repository `/home/thorrester/Documents/GitHub/wyrd-worktrees/agent-runtime-composition`; base `06a368906187be20364a2d959da311151964b617`; candidate `497dd3acc8fd06567a0e0ff01baf599f9af1711b`. HEAD remained the candidate. Source was not edited. The only review write is this report.

Read the subject, approved revision 3 specification, original task at base, actual cumulative diff, agent rules, relevant design/client/Agent and doctrine runtime boundaries, spec-driven-development and maintainer references, architecture constraints, errors, and telemetry references. Used CodeGraph first and bounded source searches where its result did not locate the requested owner. Implementation evidence is treated as reported outcomes, not independently executed proof.

## Topology and changed deployed paths

| Path | Owner, process boundary, and effect |
|---|---|
| Application inference | Rust support-desk `answer` (`examples/support-desk/rust/support_desk.rs:181`), Python `answer` (`examples/support-desk/python/support_desk.py:96`), and TypeScript `answer` (`examples/support-desk/typescript/support-desk.ts:122`) now make native gateway calls in the application process. Python retains an OpenAI client on `Desk`; Rust creates an HTTP client per call; TypeScript uses the existing authenticated fetch adapter. No new server deployment or execution service is added. |
| App server inference | `examples/support-desk-app/rust/lib.rs:119` replaces invocation inside `/ask` and `/reviews` with an application HTTP exchange. Its router has no timeout layer. This file is documentation/example source, and the implementation evidence explicitly says it is not compiled by a lane. The compiled support-desk Rust consumer independently exhibits the same issue described below. |
| Authentication | Rust obtains the existing client's `access_token()` on each call; Python installs `client.http_auth`; TypeScript calls `client.httpAuth`. `WyrdClient::access_token` delegates to `AuthMiddleware::bearer` (`client.rs:496`, `auth.rs:841`), preserving normal cached-token expiry renewal and its single-flight lock. No provider credential moves into the application. |
| Wyrd ingress and dependencies | Native requests still enter `wyrd-server` gateway routes, authenticate, authorize subject/target, run governed invocation, and emit capture through existing owners. `components/gateway/routes.rs:59` retains a five-minute server call deadline. These modules, gateway engine, audit, SQL, Scribe, and deployment/target selection are unchanged. The logical server can consist of multiple replicas/targeted pods; this deletion introduces no new pod dependency or shared-server failure policy. |
| Observations and verification | Each application retains its Run and Card headers, then writes observations/submits existing verification work. `Run::scope` only attaches observation/trace scope (`wyrd-client/src/observe/mod.rs:93`); it is not an operation deadline. Bifrost and telemetry startup/shutdown remain the existing owners. |
| Internal Agent execution | Skald's substantive loop/run lifecycle remains Rust-owned. Changes to agent/callback/session/run/loop symbols remove Python registration, Python-only fields/accessors, and corresponding initializers. Vala/server execution, hosted tools, budgets, route restrictions, workspaces, and cleanup machinery are not moved or materially rewritten. |

The diff also removes public/native invocation projections, runtime-only wrappers/tests/features/dependencies and stale runtime examples. These are removal paths, not additional deployed components.

## Failure and recovery assessment

| Credible event | What stops / survives / recovers | Assessment |
|---|---|---|
| Gateway refusal or unavailable deployment | Application request returns an error before ticket write/verification submission. Other application requests and server capabilities are not deliberately stopped. Native gateway still owns authorization/capture and provider-secret isolation. | Source preserves the failure boundary; changed journeys assert viewer/granted-deployment/foreign-Card refusals and upstream header stripping. |
| Token cache reaches refresh skew; token endpoint unavailable | Existing `bearer` refreshes under its existing gate or returns an auth error. No new retry loop, duplicate refresh owner, or process termination is introduced. | Normal renewal source preserved. Previous private native transport's reactive 401 refresh was removed with invocation; existing stock-client auth adapters obtain cached/expiry-renewed credentials. No new auth API or broader behavior is required by this report. |
| Provider outage with reachable Wyrd | Existing server invocation deadline and engine outcomes remain active; application receives a native failure envelope if the reply reaches it. Python explicitly disables OpenAI automatic retries; Rust and TypeScript example code add no model replay loop. | Preserves server integrity and avoids introducing application-level retry amplification. |
| Application-to-Wyrd link stalls, or response body never completes | Rust's new HTTP exchange has no deadline. TypeScript's fetch carries no total abort signal. The server's provider deadline cannot bound a response that cannot reach the application. The affected Run never reaches observation/verification or normal shutdown. | **SYS-001**. |
| Caller cancellation / process replacement | Dropping a Rust caller future abandons the HTTP future; application code does not promise rollback or durable invocation replay. Accepted server work remains server-owned. Run IDs remain client-side opaque IDs, and submitted verification is resumed by existing execution IDs. No new durable client correlation store exists. | No new durability or replay claim; unchanged server machinery must remain the recovery owner. No basis to terminate unrelated server capabilities. |
| App/server restart | Hydration/registration and retained server verification/evidence remain existing behavior. New native request code retains no durable client state to reconstruct. A model call whose response is lost is not made idempotent by this migration. | No invented replay or global shutdown is warranted. Existing interrupted-write/outbox limits remain unchanged. |

## Proposed finding

### SYS-001 — REGRESSION / Important: migrated native requests lose their finite total deadline

**Locations:** `examples/support-desk/rust/support_desk.rs:190–199` (primary), `examples/support-desk/typescript/support-desk.ts:129–144`, and sibling `examples/support-desk-app/rust/lib.rs:136–146`.

**Obligation:** task consumer migration must preserve useful application workflow/error behavior (REQ-013/016 and AC-009); AGENTS.md §6 requires bounded external calls/timeouts when available. This is a regression on the actual migrated caller, not a request to harden unrelated server execution.

**Evidence and producer-to-consumer path:** before deletion, `Run::invoke_on` installed `PublicWyrdGatewayCaller`; `skald-agent/src/gateway.rs:25,152–158` supplied a ten-minute budget, and the deleted `gateway_caller.rs:133–151` bounded the entire native send/body read with `tokio::time::timeout(call.timeout, send)`. The candidate replaces this with `reqwest::Client::new().post(...).send().await` followed by `reply.json().await` and no timeout. The locked reqwest 0.13.4 local source `async_impl/client.rs:313–314` initializes read and total timeouts to `None`. The TypeScript sibling similarly calls `client.httpAuth`/fetch without `signal`; that existing adapter only installs auth headers and delegates to `fetch` (`sdks/wyrd-sdk-ts/wyrd/src/index.ts:1555–1560`). It adds no total operation bound. Runtime/native projections and the old `public_gateway_call_context_and_errors` timeout proof were removed, so they cannot bound the replacement calls.

**Reachable consequence:** a connection accepted by a stalled gateway/proxy, or a response whose body does not terminate, can indefinitely retain the Rust application request. A continually incomplete response can also outlive any total bound in the TypeScript consumer. `serve` awaits that answer before recording the ticket, submitting its Verifier, advancing the batch, or executing shutdown. The app-server example similarly retains `/ask` or `/reviews`. The five-minute Wyrd provider deadline only controls work inside Wyrd and cannot release a client waiting on its own failed network hop. Python's stock OpenAI client already supplies a finite default request timeout and is not included in this finding.

**Minimum testable correction:** keep inference application-owned and set a finite total request deadline directly on the existing Rust native request and a total abort signal on the TypeScript fetch, covering body consumption as well as response headers. Preserve the former finite budget rather than adding a new runtime, public SDK option, retry layer, or server shutdown policy. Apply the same direct correction to the Rust app example. Timeout should fail only the affected application request and must not resend an uncertain model call.

**Focused closure proof:** exercise the application-owned request against a held response/body and prove it exits at its finite deadline without replay; prove a subsequent normal call can complete. Use the existing gateway/mock-upstream test infrastructure and owning language runtime with controlled test time as appropriate. Healthy support-desk journeys alone do not cover this condition. No broad journey aggregate is required for this bounded fix.

## Verification and residual limits

Source-inspected the native application call sites, their sequential workflow consumers, auth token cache and adapter paths, unchanged server ingress deadline, relevant internal Skald gateway/loop lifecycle, and changed gateway/support-desk/local-development test assertions. Reported J-R/J-P/J-T and preservation journeys demonstrate healthy/correlation/refusal claims only; no logs or new executions independently establish their results here. No Cargo/build/test commands were run, per orchestration instructions.

No process-kill, shared-server fail-closed policy, new retry mechanism, persistent migration, or platform-specific recovery recommendation is proposed. Broader package/export/standards acceptance belongs to the independent reviewers. The residual runtime risk is the unbounded migrated request in SYS-001; outage/deadline closure is not established by the reported healthy journeys.
