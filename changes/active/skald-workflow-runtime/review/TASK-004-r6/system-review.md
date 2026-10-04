# System-resilience review — TASK-004 r6

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `5f3b521b5005c26277d53e7dfd2458c4f740e8be`
- Approved authority:
  `changes/active/skald-workflow-runtime/spec.md`, Revision 14
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Remediations: TASK-004 R1 through R5 in their preceding review
  directories
- Review mode: complete cumulative diff, applicable architecture and security
  authority, current source, prior validated findings, and recorded evidence
  only. No build, test, Cargo, mise, formatter, linter, code-generation, or
  package-manager command was run.

The candidate resolved to the requested commit before source review and again
before this report was written. `.codegraph/` is absent, so immutable Git
objects, repository search, and direct source inspection were used.

The standing human decisions are treated as authority: follower grant-stream
close is release and the leader awaits no acknowledgement; Oracle graph-drain
polling and idle refusal stay deleted; the foreign-tenant harness stays
unchanged; published `Running` reserves attempt one and interrupted published
work settles `Cancelled`; query tools receive the prepared run deadline once,
with a shorter positive `deadline_ms` winning; Revision 14 has one request
variant per wire schema, `Prompt.provider` is the optional dispatch target, a
Vertex Prompt carries a Google GenerateContent body, and the local
`wyrd-client` gateway's Vertex refusal remains prior behavior.

## Deployment topology and changed-path coverage

The runtime topology remains one shared `wyrd-server` process. `WorkflowRuns`
owns process-local preparation, acceptance, retained snapshots, cancellation,
and shutdown tracking. Skald owns the bounded executor and its absolute run
deadline. `RunTools` owns run-local Cards and query adapters. `BoundedQuery`
retains the Oracle response while `RunningQueryControls` routes cancellation
and confirms or honestly fails settlement. Gateway calls retain their existing
separate tracked settlement. `BoundServer` applies one shutdown deadline across
these owners. Revision 14 changes typed request/provider projection across
Skald, the in-process server gateway, the external gateway adapter, the local
client gateway, and Python declarations; it adds no deployed process, queue,
durable owner, background worker, retry loop, health gate, or recovery protocol.

| Runtime path | Ownership and credible failure trace | Recovery and sibling-service effect | Result |
|---|---|---|---|
| Create, replay, and preparation | HTTP create authenticates and audits before `WorkflowRuns::admit`. A reservation takes a tracker token under the run-table lock, graph pinning and binding resolution happen in tracked preparation, blocking Skald preparation stays tracker-owned, and `Reservation::accept` atomically transfers the active slot to the queued run (`components/workflow/host.rs:75-123,244-400`; `components/workflow/runs.rs:258-366,530-635`). | Request/waiter disconnect does not own preparation. Registry, binding, validation, cancellation, or shutdown failure removes the key, releases capacity once, and wakes all waiters. No provider or tool call happens before acceptance. | PASS |
| Accepted execution and terminal state | The accepted reservation token is the executor cancellation root. Non-terminal observations replace whole snapshots; the terminal snapshot is committed only after Skald returns and run-owned query tasks drain (`host.rs:263-287`; `runs.rs:638-688`). Published `Running` reserves attempt one at the scheduler/ledger source, so pre-poll interruption settles the published step as `Cancelled` rather than regressing it to unstarted. | Completion, explicit cancel, run deadline, and shutdown converge on the executor's terminal result and one stored terminal boundary. Capacity is not released while a run-owned query owner is still settling. | PASS |
| Prepared run deadline and query projection | `WorkflowExecutor` owns one absolute deadline, exposed by `PreparedWorkflowRun::deadline`; server preparation binds that exact instant once into the `Arc<OnceLock<Instant>>` shared by every hydrated `RunTools` clone before acceptance (`skald-workflow/src/workflow.rs:89-107,207`; `workflow_surface.rs:572-596,696`; `host.rs:374-400`; `tools.rs:43-96`). `QueryTool::invoke` applies `requested.min(remaining)` and transfers the complete query to the run's task tracker (`tools.rs:188-233`). | Hydration or planning cannot create an earlier tool-only deadline. An omitted or longer deadline ends with the run; a shorter positive deadline ends the query first while the run remains able to continue. No fresh cleanup timer is created after expiry. | PASS |
| Oracle open, cancel, and settlement | `BoundedQuery::run` projects the same deadline into the query request and `RunningQueryControls::open_cancellable`, then retains the stream in `ResultCollector`; every nonterminal error delegates to `cancel_and_settle` (`query/collect.rs:275-324,373-399`; `oracle/lifecycle_controls.rs:35-162`). Cancellation signals immediately while opening or consuming, but does not let waiter drop destroy the response owner. | Success still requires a valid terminal and clean EOF. Deadline, transport, protocol, or owner loss returns an honestly incomplete/unavailable/protocol outcome with no partial rows. The original query deadline is the only cleanup bound. | PASS |
| Forwarded Analytical query | The leader's graph lifecycle joins its attempt and descendants, closes exchanges, then drops participant grant streams and releases its own graph/admission owner (`vala-bifrost-redux/src/oracle/analytical.rs:2256-2277,2690-2785`). Follower grant-stream close is the release signal; no leader acknowledgement or polling loop exists. | Follower loss fails the selected attempt rather than returning rows. Residue stays attributable and prevents a false clean state. Surviving membership can serve later queries; unrelated interactive capacity is not deliberately taken down by a component failure. | PASS |
| Gateway calls and Revision 14 request routing | Server Workflow calls enter the existing in-process `GatewayInvocation` with captured authority, ordinary per-call authorization, current deployment/credential admission, the remaining deadline, and separate cancellation (`components/gateway/workflow.rs:34-131`). Revision 14 keeps OpenAI Chat as one schema; native dispatch selects the client by resolved `Prompt.provider`, external routes select their configured protocol and origin, and the server gateway derives Vertex ingress from the Prompt-derived model identity while decoding the shared Google response (`skald-spec/src/prompt.rs:142-173`; `skald-runtime/src/dispatch.rs:14-84`; `skald-workflow/src/route.rs:281-355,394-455`; `components/gateway/workflow.rs:177-243`; `skald-providers/src/clients/vertex.rs:105-188`). | Missing providers, protocol mismatch, unavailable deployment, credential failure, endpoint refusal, timeout, and decode failure remain call/run-scoped typed failures. The external adapter retains endpoint screening, bounded transport, redaction, and retry ownership. The local `wyrd-client` gateway continues refusing a GenerateContent body for a Vertex model and gains no new availability claim. | PASS |
| Shutdown | `BoundServer::run` fixes one deadline, drains Workflow admission/preparation/execution first while query and gateway dependencies are available, then drains supervised server work, MCP, gateway accounting, capture, and Bifrost using only the remaining budget (`app/server.rs:744-895`; `runs.rs:417-433`). | Deadline exhaustion is surfaced as a shutdown failure. Bifrost is awaited through abort rather than reported clean, and Workflow cannot publish a late terminal mutation after process exit. No subsystem receives a refreshed shutdown allowance. | PASS |
| Restart and rolling replacement | Accepted runs, idempotency keys, captured authority, and retained snapshots exist only in the process-local `WorkflowRuns` table (`runs.rs:89-115,212-243`). | Process/pod loss intentionally loses the run. A restarted or non-owning replica returns the common not-found response and does not replay provider/tool calls, transfer ownership, or claim durable recovery. Rolling replacement therefore requires the already-approved affinity/drain behavior rather than an added queue or lease. | PASS |
| Dependency outage and bounded pressure | Registry and secret resolution failures occur before acceptance; gateway and Oracle outages become typed run failures; active/preparing runs hold the configured global and tenant slots; only terminal entries are retention-expired or oldest-first evicted. Graph, input, step, aggregate run, query row, and query byte ceilings are applied by their existing owners. | Failure remains scoped to the request, run, or dependency. The candidate adds no unbounded retry, N+1 recovery scan, independent watchdog, health-amplification loop, or new configuration surface. Recorded sibling journeys cover Cards, gateway, and Bifrost serviceability during preparation, settlement, and near-ceiling terminalization. | PASS |

## Failure and recovery evidence

- R1 source and recorded owner evidence cover reservation ownership before task
  spawn, blocking preparation after caller cancellation, cancel-while-opening
  under the original deadline, tenant and authority boundaries, pod loss, and
  sibling query recovery.
- R2 and R3 preserve the follower release model and correct the externally
  visible step-attempt state at its scheduler/ledger source. No acknowledgement,
  lifecycle service, or downstream status repair was added.
- R4 source fixes the prepared Workflow deadline as the sole absolute owner and
  binds it once to all run tools. R5's current Oracle journey drives omitted,
  explicit-longer, and explicit-shorter positive query deadlines through the
  real Workflow tool and forwarded Oracle path
  (`wyrd-testing/tests/bifrost/oracle/workflow.rs:74-107,154-269,336-376`). It
  requires omitted/longer cases to reach `TimedOut` at the run boundary and the
  shorter case to surface `WYRD_VALA_504_QUERY_TIMEOUT` without rows, accept a
  scripted continuation, and succeed before the run deadline.
- Revision 14's one-variant refactor removes destination identity from the
  request wire and moves it to the existing Prompt owner. Recorded focused
  native-dispatch evidence covers a custom OpenAI-compatible target and Vertex;
  the existing server route journey covers a stored custom-provider Prompt on
  direct external OpenAI Chat and a stored Vertex Prompt through the in-process
  gateway. Source tracing confirms that both reuse existing clients, route
  adapters, timeouts, cancellation, and typed failure paths.
- The recorded final-tree evidence reports successful format, lint,
  `git diff --check`, codegen, client-tier and PyO3 boundary checks, Skald,
  shared, Wyrd server, gateway journey, Python unit/type/integration,
  TypeScript unit/integration, and Bifrost Oracle journey lanes. These results
  were treated as claims and checked against the assertions and production
  paths above.

## Affected capabilities

The cumulative change affects accepted Workflow create/get/cancel, bounded
Skald DAG execution, provider selection, in-process gateway calls, direct
external gateway calls, built-in Cards and Bifrost reads, Oracle forwarding and
lifecycle control, retention, and server shutdown. Source tracing found no
changed reachable failure path that crashes the shared server, takes an
unrelated service offline, reports partial query data as success, releases run
capacity before its tracked query owner reaches a bounded outcome, extends a
deadline during cleanup, or claims durable restart/multi-replica recovery.

## Material findings

None.

No reviewed mechanism, file, setting, check, or option lacks an established
repository/native-platform precedent or a comparable ordinary implementation.
The one-time deadline handoff uses `OnceLock`, process-local accepted jobs use
the existing tracker/cancellation model, and Revision 14 deletes duplicate
wire variants rather than adding compatibility machinery. Requiring an
acknowledgement protocol, cleanup poller, durable run owner, migration reader,
provider alias, new test harness, setting, or repository check would be DRIFT.

## Recorded proof and verification limits

- This was a strictly read-only source review. No command that builds, tests,
  formats, lints, generates, or mutates implementation state was run.
- Recorded evidence is not independently re-executed evidence. The production
  ownership paths and named assertions were inspected directly; environmental
  failures outside those recorded runs remain a normal residual risk.
- The foreign-tenant journey does not add a second tenant's gateway credential
  or model step. Its registry, run, Card, query, and idempotency isolation
  assertions remain the approved proof boundary.
- The process-local contract deliberately provides no recovery after owner-pod
  loss and no cross-replica run lookup. That is approved behavior, not an
  availability finding.
- A query may end with bounded, honestly unconfirmed cleanup when its original
  deadline or transport is exhausted. It may not claim clean settlement or
  return partial rows; the current controls preserve that distinction.
- Gateway accounting/capture/audit settlement remains gateway-owned and is not
  awaited by Workflow terminalization. Workflow cancellation still signals the
  call; the approved boundary avoids coupling run capacity to unrelated gateway
  post-response work.

## Overall result

**PASS**

The cumulative candidate satisfies the reviewed deployment, interruption,
dependency-failure, recovery, shutdown, and sibling-serviceability obligations.
R5 supplies the missing real-path deadline matrix, and Revision 14 simplifies
provider/request representation without introducing a second lifecycle,
recovery path, or resilience regression.
