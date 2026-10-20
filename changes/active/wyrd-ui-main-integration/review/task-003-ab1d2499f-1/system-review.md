# TASK-003 system-resilience review

Result: **FAIL**. One material cancellation defect is proposed below. This is an independent system review, not a task-wide acceptance verdict.

## Immutable subject and authority

Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`.
Base: `695ca914df61a8683059f6f32279ec2148424e06`.
Candidate: `ab1d2499f6a08a6b5afe1f6c904e72746d526bda`.
HEAD remained the candidate after inspection. No tracked source was changed.

Authority consulted: approved spec revision 12, original TASK-003 including its acceptance/evidence matrix, AGENTS.md, architecture/agent-rules.md, reference router, spec-driven-development and maintainer-style references, testing-workflows, Wyrd client model and browser-session doctrine, Bifrost query/terminal contract, and deployment-and-release topology. Source navigation followed subject.md; no `.codegraph/` exists. The base-to-candidate diff and scoped source were inspected independently. Excluded commits were checked separately: 45797e42e owns the Linux builder changes, 2884da447 the callback expectation, 62966bbdd generated docs, fc0d28fda CLI journey selection. TASK-027 layout changes are an authorized successor, not TASK-003 drift.

## Deployed paths and ownership

| Path | Runtime and retained state | Failure boundary and recovery assessment |
|---|---|---|
| First-party browser → `remoteLoader` → tenant BFF `ui/load` → `nodeExecutor(session.client())` → SDK/shared Rust client → server Cards/Oracle | Node handles one panel request; BrowserSession holds only the selected opaque cookie and trusted origin; Rust/Postgres remains session authority. `ui/load/+server.ts:23–28` passes the incoming signal. | HTTP/SDK failures reject or produce a structured panel result; WyrdView handles rejection without crashing its host. A new request reconstructs its executor. No new durable store or replica affinity; process replacement retains Rust-owned sessions. Session expiry remains authorized by the server on each read. |
| Custom app SSR/component page and `/view/load` | `examples/ui-app/src/executor.ts` makes a fresh SDK/executor per invocation. `+page.server.ts` shares it within that load. Browser output carries props; view page uses `remoteLoader`. | Failed load is scoped to its page/request; a subsequent request can reconnect. Bifrost's ordinary eager ingest connection remains existing SDK behavior, not a new transport owner. No new polling/retry loop or health dependency was introduced. |
| MCP App artifact → `mcpExecutor` → existing host `callServerTool` → Wyrd MCP | Artifact holds App and renderer, not credentials. `main.ts:24–27` destroys the prior render before mounting the new document. Host owns transport/credential. | `mcp.ts:36–43` preserves structured tool refusals and converts host call failures to panel errors. AbortSignal is passed through to the supported host API. A replacement result creates new load controllers. A host disconnection stops its calls; no app-side retry amplification or persistent state appears. |
| View → independent portable loaders → component state | `WyrdView.svelte:start/settle/abortAll` gives each valid panel its own controller; only current, non-aborted controllers may settle results. `loadCardHeader` reads the exact card before paged versions. | Invalid/failed panels do not block siblings; failed card reads do not trigger version reads; version failure retains the card with a separate error. Aborted card loads check the signal before subsequent pages. Removed/replaced panel results cannot overwrite current state. Action pending state prevents duplicate action dispatch; host refusal remains visible. |
| Browser-session Bifrost connection | `WyrdClient::is_browser_session` feeds `Bifrost::connect_with` → private `read_only` → shared `from_sink`, reusing QueryClient and QueryOnlySink. | Browser queries do not dial bearer-only ingest, so ingest-channel outage no longer prevents these HTTP reads. Credentials remain in the shared HTTP owner. No new server process/lifecycle is added; ordinary client connection behavior is preserved. |
| Production UI package/bundle | `scripts/build.sh:43–67` builds native SDK in the bundle profile, packs SDK, installs runtime dependencies, copies local addon beside its entry point, and imports both built handler and SDK. | Missing addon/import failures stop bundle generation before the artifact is accepted. Native launch/image topology remains Rust + Node (+ nginx in images), with existing supervisor ownership. The new package adds no daemon or mutable durable state. Linux/image closure has not been independently proven here; see proof limits. |

The newly needed orchestration is UI projection and per-view supersession expressly required by REQ-030. It reuses Cards, Bifrost, the shared SDK transport and JsonResultBuilder rather than creating another query or identity service. BrowserSession.api already owns direct session HTTP calls, but the new SDK constructor is needed for the approved portable executor surface; it does not introduce Node session persistence or token refresh.

## Failure and recovery assessment

| Obligation/path | Evidence | Assessment |
|---|---|---|
| Superseded/removed/unmounted view cannot display stale results | `WyrdView.svelte:172–212`; scenario 3 assertions on aborted controllers and ignored late results | PASS for result isolation. |
| Cancellation reaches an active query wherever supported | `node.ts:30–36`; SDK `BifrostQueryStream.return`, native `close/collect` | FAIL during asynchronous query startup: SYS-003-1. |
| Already running collection can be interrupted | `node.ts:31–36`; native `collect` selects `closing.notified`; `close` signals before waiting for lock (`native/src/lib.rs:1398–1403,1443–1445`) | Implementation supports interruption after the listener is installed. It does not cover the earlier startup window. |
| Query error, malformed/incomplete terminal or exceeded bounds cannot become usable partial rows | SDK `collect` → native JsonResultBuilder; shared QueryResultStream; `loadChart` maps refusal; renderer catches rejection | PASS by source ownership and existing focused/live recorded evidence; no alternative terminal parser introduced. |
| Auth/upstream outage stays request/component-scoped | `node.ts:9–11,49–57`, `remoteLoader`, renderer rejection handler; BrowserSessions read path | Errors do not terminate Node/Rust or mark an unavailable dependency successful. The new endpoint does not cache success or session validity. No newly changed crash-recovery path warrants an unrelated deployment refactor. |
| Render failure does not block sibling components | Validations and per-panel promise settlement in WyrdView, own version-result state in loader | PASS for supplied built-in executor/loaders and recorded scenario 1/3 evidence. |
| Bundle/platform closure | Build/import checks in canonical build script; task records successful darwin-arm64 closure only | UNVERIFIED for Linux x86_64/arm64 and selected images. This is a completion evidence limit, not a demonstrated platform implementation defect. |

## Proposed finding

### SYS-003-1 — INCORRECT: an abort during Node query startup never closes the eventual stream

**Violated obligation:** REQ-030 explicitly requires changing input/range, removing a panel or leaving a host to supersede outstanding loads and propagate cancellation to active executor work where supported. TASK-003 scenario 3 and its live verification instructions include cancellation.

**Exact location:** `crates/wyrd/wyrd-server/wyrd-ui/packages/ui/src/lib/node.ts:30–34` (the await precedes abort subscription).

**Producer → consumer trace:** WyrdView starts `loadChart` with one panel controller; the loader invokes `nodeExecutor.run`; its initial `throwIfAborted()` succeeds. The executor awaits Bifrost connection and `Bifrost.stream()`. That method awaits the native HTTP query start before returning a stream (`sdks/wyrd-sdk-ts/wyrd/src/index.ts:1287–1315`; shared `QueryClient::query` awaits the HTTP response). During that await a range/document replacement, host exit or incoming request disconnect aborts the controller. Once the query start returns, `node.ts:32` subscribes to an already-aborted signal. Abort events are not replayed to new subscribers. It then calls `collect`, and there is no success-path abort check in `run`. The eventual result may be reported as `{ok:true}` even though the signal was aborted. WyrdView's generation guard prevents stale display, but the obsolete query continues consuming Oracle and Node collection work until terminal/deadline/ceiling.

**Existing cancellation owner:** `BifrostQueryStream.return()` closes its native stream; native `close()` notifies an in-progress collection and drops its response. No new cancellation protocol, downstream renderer guard or server owner is needed. The defect is the adapter failing to observe cancellation once the stream becomes available.

**Sibling callers/consumers inspected:** first-party `ui/load` and custom-app `/view/load` forward request signals to this same executor; custom direct SSR uses the same factory without a signal; MCP executor independently passes the signal to `callServerTool` and is not affected by this adapter race. `loadCardHeader` checks abort before issuing its dependent listing; other SDK reads lack a public stream close capability and do not justify widening this correction.

**Observable consequence:** a viewer brushing or leaving during slow query start leaves superseded read work active despite an available close mechanism. Repeating range changes can retain several obsolete queries; the corresponding direct executor call incorrectly resolves success after cancellation. This does not assert that stale UI rows are displayed or that server deadlines cease to exist.

**Independent focused reproduction:** ran `mise exec -- node --input-type=module` in the UI directory with an in-memory replacement of `Bifrost.connect`, importing the current built `packages/ui/dist/node.js` (body matched source). The fake SDK stream-start promise was held; `run()` began with a live signal; the signal was aborted while start was held; start was released with a stream recording `collect`/`return`. Assertions passed: `collect` called once, `return` called zero times, result `ok === true`. Output: `Reproduced: abort during stream startup is ignored; collect runs once, close never runs, executor returns success.` No file or dependency was added. This is a deterministic adapter regression proof, not a claim of live Oracle cancellation proof.

**Smallest testable correction:** keep cancellation wiring in `nodeExecutor` and use the existing stream close path. When an awaited stream becomes available, observe an already-aborted signal, close that stream and reject cancellation rather than draining it; retain the current listener for aborts during collection and its cleanup. Do not add a query service, retry policy, host lifecycle or repeated component guards. Prove both the deferred-start abort case (stream closed, no collect/success) and abort during collection. The existing pre-aborted-signal test is insufficient.

## Proof and completion limits

Historical task evidence includes green Node/MCP real-server journeys, bounded/failure/denial results, Rust browser-session connection test, built AppBridge and custom app proof, package checks/builds, codegen and human visual approval. Orchestrator `verification.md` adds fresh rendering/dependent-load tests, complete ui-library.test.ts (7 tests), package checks, build and diff checks at the pinned candidate. These checks are credible within their asserted scope, but they do not close SYS-003-1: live scenario 2 cancellation at `ui-library.integration.test.ts:184–187` starts with `AbortSignal.abort()`, so it only proves refusal before any query starts.

Degraded terminal parity was not produced live, and Linux native/image bundles were not run locally or by this reviewer. Shared JsonResultBuilder supports the degraded projection by source; the missing live proof remains unverified. The original task explicitly says missing cross-platform proof remains unverified and must not be reported as a pass. Release-workflow wiring is future execution, not recorded evidence that those candidates passed. Therefore this report does not award complete cross-platform/degraded acceptance; final acceptance must retain these limits and obtain the required evidence before claiming those obligations fully proven. I am not proposing a speculative code change or a separate platform defect based solely on absent execution.

Skipped: rerunning live Postgres/MCP journeys, Rust aggregates, Linux bundle/images and new browser inspection. Risk: cancellation during SDK stream startup leaves obsolete queries active; platform/degraded evidence remains incomplete.
