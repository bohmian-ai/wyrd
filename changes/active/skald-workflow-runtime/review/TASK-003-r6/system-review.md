# TASK-003-r6 system-resilience review

## Review Findings

### Critical

None.

### Important

None.

### Suggestions

None.

## Immutable Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `b954976648b49429f1c0950c4fa3509573885be8`
- Approved specification:
  `changes/active/skald-workflow-runtime/spec.md`, Revision 12
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Remediation authority: TASK-003 R1 through R5 under
  `changes/active/skald-workflow-runtime/review/`

The complete base-to-candidate range was inspected. The human-approved
`WyrdGatewayCall.model` amendment, the corrected native-`401` send-once
contract, and the approved run-start blocking boundary for configuration,
client, and selected-secret reads were treated as authority. The candidate
remained the stated commit throughout this review.

## Deployed-Path and Topology Evidence

| Runtime path | Source evidence | Deployed effect |
|---|---|---|
| Shared local Workflow facade | `crates/shared/wyrd-client/src/workflow/mod.rs:36-188`; `sdks/wyrd-sdk-python/src/workflow.rs`; `sdks/wyrd-sdk-python/src/state/mod.rs` | Rust, Python, TypeScript, and CLI local execution converge on the shared client facade and one Skald executor. A loaded registered or authored-external Workflow retains its loading client; a wholly local Workflow has no retained authority context. |
| Run-start route and ambient-state selection | `workflow/mod.rs:115-161,190-238`; `workflow/local.rs:30-110` | Route discovery occurs before setup. A client-less mixed route performs one blocking `GlobalConfig::load`; the external bindings and Wyrd client derive from that same value. A retained client bypasses ambient client construction, and a native-only run performs no ambient configuration read. |
| Selected secret preparation | `workflow/local.rs:72-149`; `crates/shared/wyrd-utils/src/secret.rs` | Only bindings named by selected `ext_gateway` routes have secret references resolved, on the blocking pool, before execution. Missing or unreadable selected material fails the local run before model dispatch. |
| Per-attempt gateway context | `crates/skald/skald-workflow/src/route.rs:395-427,543-590`; `crates/shared/wyrd-client/src/workflow/gateway.rs:32-124` | Model, fallback, remaining deadline, cancellation token, and correlation are immutable attempt-local values. The public caller projects them to one existing native ingress without shared mutable header or provider state. |
| Native authenticated send | `crates/shared/wyrd-client/src/transport/http.rs:351-411`; `auth.rs:436-544` | A model POST is sent once. A known `401` starts credential renewal before body collection; renewal affects later calls only and never replays the ambiguous model operation. The outer public caller owns timeout and cancellation. |
| Server ingress and governed dispatch | `crates/wyrd/wyrd-server/src/components/gateway/ingress.rs:101-224,249-430`; `routes.rs:1329-1346,1757-1806`; `crates/wyrd-spec/src/gateway/policy.rs:88-184` | Existing middleware authenticates before handlers parse the bounded fallback header. Invalid values refuse only that request before dispatch. The typed fallback enters `GatewayCallRequest`; caller headers are not forwarded to providers. |
| Remote Workflow lifecycle handle | `crates/shared/wyrd-client/src/workflow/remote.rs:23-121`; `transport/http.rs:701-799` | `create` uses the existing retrying transport with one key minted outside the retry loop. `get` and `cancel` are stateless reads/mutations over the server resource; `wait` owns only polling and never owns or cancels the server run. |

## Failure, Recovery, and Availability Assessment

| Failure or interruption | What stops and what survives | Assessment |
|---|---|---|
| Local cancellation before public dispatch | The biased cancellation branch wins before the send future is polled. No gateway request begins; other runs and sibling client/server capabilities are unaffected. | PASS. |
| Cancellation or deadline after model dispatch | Local waiting stops and the step/run records the bounded failure. Already accepted provider work cannot be rolled back, but the client does not resend it. The failure remains scoped to the call and local run. | PASS. |
| Native `401` with a complete, truncated, slow, or pending body | Renewal begins after the response head and before body collection. A renewal failure is returned; after success the original refusal or body-read outcome remains. Exactly one model POST is issued. | PASS. Current ordering agrees with the recorded R1-R3 source evidence. |
| Auth exchange or Wyrd endpoint outage | The affected call returns the existing authentication, transport, timeout, or redacted remote-problem category. There is no detached retry loop, recursive gateway call, process exit, or shared health transition. | PASS. |
| Configuration, ambient credential, or selected-secret failure | The affected local run fails during run-start preparation before Skald dispatch. Completed reads are discarded, and an already-started blocking read may finish after caller drop without publishing a partial dependency set. | PASS under the approved run-start boundary. |
| Configuration replacement during a client-less mixed route | `load_local_setup` performs one `GlobalConfig::load` and derives both `global.workflow` and `ClientConfig::from_global_with_env(&global)` from that value, preventing the previously diagnosed two-file-generation split. | PASS. R5 now drives an actual client-less mixed Workflow through `run_with` and observes both consumers. |
| Retained Cards client with missing or hostile ambient client configuration | `load_local_setup` reuses the retained client for `wyrd_gateway` and does not assemble an ambient gateway client. External bindings still read only their separately required workflow configuration. | PASS. The recorded Python journey covers the public retained-client boundary after the shared-path changes. |
| Pure native run | Both route flags are false; setup uses defaults without reading the ambient config, building a Wyrd client, or resolving an execution secret. | PASS in source. |
| Gateway/server process loss after request acceptance | The public model call has ambiguous external effects, so the client deliberately does not replay it. A later call may use a renewed credential, but the interrupted call is not reconstructed. No new queue, WAL, or cross-process recovery promise was introduced. | PASS. Adding one would be unapproved DRIFT. |
| Local process restart | Process-local runs and their in-memory polling/execution state end with the process; the candidate makes no durability claim for local execution. A new process can reload the Workflow and start a new explicit run. | PASS for the approved local boundary. |
| Remote create response loss or client restart | Retries inside one live `create` retain one idempotency key. Once the caller has a run ID, a fresh stateless handle can resume observation or cancellation through `get`, `wait`, or `cancel`. A separately invoked `create` intentionally represents a new submission. | PASS. Server-side durable execution and shutdown/drain are owned by later tasks, not invented here. |
| Dropped `wait`, `get`, or `cancel` future | Dropping `wait` stops polling only. A cancellation request whose answer is lost may already have taken effect and can be reconciled with `get`; no client-side process or sibling capability is failed. | PASS. |

## Affected Capabilities and Recovery Boundary

The cumulative candidate affects local Workflow execution projected through
the first-class SDKs and CLI, the Rust remote-run handle, and existing public
OpenAI Chat/Responses, Anthropic Messages, and Gemini GenerateContent ingress.
Its failures remain request- or local-run-scoped. The candidate adds no server
process, durable queue, recovery controller, health loop, public ingress,
provider-credential owner, transport, retry setting, cache, watcher, checker,
or compatibility path.

R5 changes runtime proof and contract documentation, not deployed behavior.
Its mixed-route case now crosses the real producer-to-consumer path:
`Workflow::run_with` -> `SelectedRoutes::of` -> one blocking
`load_local_setup` snapshot -> selected secret preparation and the configured
public gateway client. The architecture and Python declarations now describe
the same request and run-start boundaries. These are established test and
documentation mechanisms, not novel resilience machinery; no DRIFT is present.

The accepted-run service, server affinity, shutdown/drain, and durable
server-run recovery remain the responsibility of TASK-004/TASK-005. Their
absence from this client/public-gateway task is not a regression and does not
justify adding recovery machinery here.

## Open Questions

None.

## Verification Notes

- This review was strictly source-only. It ran no build, compile, test, Cargo,
  mise, pnpm, pytest, formatting, lint, type-check, code-generation, or other
  verification command.
- Recorded implementation evidence was inspected but not rerun. R5 records the
  exact focused shared-client selector as `1 passed, 221 skipped`, the retained
  Python journey as `1 passed`, the shared family as `708 passed, 15 skipped`,
  and the relevant lint, format, type, and generated-stub checks as passing.
- Source inspection independently confirms that the R5 mixed-route test at
  `workflow/mod.rs:611-835` constructs a client-less Workflow containing both
  route families, enters `Workflow::run_with`, and observes the configured
  binding at the external upstream and the configured authenticated client at
  the public gateway.
- Source inspection also confirms that R5 did not alter the native send-once,
  renewal, timeout/cancellation, remote lifecycle, or server-ingress runtime
  paths established by prior remediation.

## Overall Result

**PASS**

No material system-resilience finding remains. Failure propagation is scoped,
ambiguous native model work is not amplified by replay, run-start ambient work
uses the approved blocking and single-snapshot boundary, and R5 closes the
previous mixed-route recovery-proof gap without adding nonstandard machinery.
