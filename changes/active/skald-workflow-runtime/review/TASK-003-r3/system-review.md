# TASK-003-r3 system-resilience review

## Review Findings

### Critical

None.

### Important

- **SYS-R3-001 — cancellation can preserve a token after its native request was
  refused with `401`** (`crates/shared/wyrd-client/src/workflow/gateway.rs:96-116`,
  `crates/shared/wyrd-client/src/transport/http.rs:398-410`,
  `crates/shared/wyrd-client/src/auth.rs:503-540`). **Classification:
  INCORRECT.** The approved same-spec TASK-003 wording and R2 remediation require
  every observed native `401` to renew through the existing auth owner without
  replay so the refused cached credential is not reused. `post_native` learns
  the status at line 402, but then awaits both body collection and
  `force_refresh`. `PublicWyrdGatewayCaller::call` races that whole future with
  cancellation and the step deadline. If cancellation fires after the response
  headers identify `401` but while the body is pending or an API-key/workload
  exchange is pending, the `post_native` future is dropped. For API-key,
  workload, and delegated credentials, dropping `force_refresh` releases its
  cache mutex without replacing or invalidating the cached refused token. A
  later public gateway call can therefore send one more non-idempotent model
  POST with the already-refused bearer and fail again; sibling client calls
  sharing the same auth cache also incur another refusal before their ordinary
  retry path recovers. The R2 cut-off-body proof waits for the body-read error
  and completed renewal, while its post-dispatch cancellation proof uses a
  request that returns no `401`; neither covers this reachable composition.
  Keep prompt cancellation and the send-once model request, but make the
  existing auth owner retire the refused cached credential at a
  cancellation-safe boundary once `401` is known, then use the existing renewal
  path. Focused closure must hold a native `401` after headers, cancel the
  Workflow call, observe no model resend, and prove the next call cannot reuse
  the refused bearer. Do not add a retry option, detached renewal worker,
  response-provenance mechanism, or new transport.

- **SYS-R3-002 — local Workflow execution performs configuration filesystem IO
  on the async executor** (`crates/shared/wyrd-client/src/workflow/mod.rs:127-150`,
  `crates/shared/wyrd-client/src/global_config.rs:71-104`). **Classification:
  VIOLATION.** A selected `ExtGateway` route enters async `Workflow::run_with`
  and directly calls synchronous `GlobalConfig::load`, whose implementation
  executes `std::fs::read_to_string`. This is the same execution-preparation
  path that correctly sends selected secret-file reads to `spawn_blocking` in
  `workflow/local.rs:115-130`, but the configuration read remains on a Tokio
  worker. A slow or unavailable mounted/home configuration filesystem can block
  a current-thread runtime entirely or remove one worker from a shared SDK
  runtime, delaying cancellation and unrelated client work before the Workflow
  reaches its bounded network operations. This contradicts `AGENTS.md` section
  6's prohibition on blocking filesystem work in async paths and the task's
  shared-runtime availability boundary. Move only the existing synchronous
  config load onto the repository's established blocking boundary; retain lazy
  selected-route loading, the current `GlobalConfig` owner, and all current
  errors. Source proof should show no direct filesystem read on `run_with`'s
  executor, and the existing selected-dependency evidence should be rerun and
  recorded. No async config API, watcher, cache, setting, or new harness is
  warranted.

### Suggestions

None.

## Immutable Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `0732ed92fc2907cb806ac918b09abbf6d44ff04f`
- Approved specification:
  `changes/active/skald-workflow-runtime/spec.md`, Revision 12
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Prior remediations:
  `changes/active/skald-workflow-runtime/review/TASK-003-r1/TASK-003-R1-preserve-client-context-and-native-call-safety.md`
  and
  `changes/active/skald-workflow-runtime/review/TASK-003-r2/TASK-003-R2-close-renewal-proof-and-source-contracts.md`

The human-approved 2026-10-03 addition of `pub model: ModelRef` to
`WyrdGatewayCall` and the R1 same-spec native-`401` task-wording correction were
treated as authority, not drift. The cumulative base-to-candidate range was
reviewed. The candidate identity remained unchanged during this review.

## Deployed-Path Evidence

| Runtime path | Source evidence | Deployed effect |
|---|---|---|
| Shared local Workflow ownership | `wyrd-client/src/workflow/mod.rs:34-42,75-94,127-153,248-274`; Python wrapper retains `wyrd_client::Workflow` | Registered and authored-external-ref Workflows retain their loading client. A selected public-gateway route uses that client; a client-less file Workflow lazily assembles the ambient client. No bearer is copied into the Workflow. |
| Selected local dependencies | `wyrd-client/src/workflow/local.rs:30-104` | Route discovery selects only required external bindings and whether a public Wyrd gateway is needed. Missing bindings remain absent so Skald refuses before dispatch. |
| Secret resolution | `wyrd-client/src/workflow/local.rs:107-137`; `wyrd-utils/src/secret.rs:23-75` | Only selected `SecretRef`s are read, file reads are bounded and owner-only, and each read runs on Tokio's blocking pool. Errors retain no path or value. Cancellation can leave a bounded blocking read to finish, but it owns no durable state. |
| Public Wyrd gateway call | `wyrd-client/src/workflow/gateway.rs:50-124`; `wyrd-client/src/transport/http.rs:351-410` | Model, fallback, timeout, cancellation, and correlation are per-call. One native request is sent through the shared authenticated pool. Caller cancellation or deadline drops local IO without claiming rollback of server-side gateway work. |
| Native `401` recovery | `wyrd-client/src/transport/http.rs:398-410`; `wyrd-client/src/auth.rs:451-541` | Complete and body-read-error `401` paths attempt the existing `force_refresh` without replay. The refresh itself is not cancellation-safe for cached exchanged credentials, producing SYS-R3-001. |
| Public authenticated ingress | `wyrd-server/src/components/gateway/ingress.rs:101-139,143-224,261-303,343-360`; `wyrd-server/src/components/gateway/routes.rs:863-920,1332-1346,1769-1805` | Protocol middleware authenticates before handlers interpret the fallback header. Valid requests enter the existing `GatewayInvocation`, which owns authorization, non-blocking canonical audit, admission, provider dispatch, and bounded settlement. Invalid fallback values stop before that dispatch. |
| Remote run handle | `wyrd-client/src/workflow/remote.rs:23-121` | Create reuses one idempotency key across transport retries; get and cancel project direct snapshots; wait polls once per second. Dropping wait stops polling only, while an accepted run remains server-owned. |

## Failure, Recovery, and Availability Assessment

| Failure or recovery path | What stops and what survives | Assessment |
|---|---|---|
| Public call cancelled before dispatch | The biased cancellation branch wins before the send future is polled. No request is made. Other Workflow steps and server capabilities remain available. | PASS; recorded focused evidence agrees with source. |
| Public call cancelled after model POST dispatch | Local IO is dropped promptly and no client resend occurs. The shared server may continue the already accepted gateway operation under its existing bounded, tracked settlement and non-blocking audit owner; it cannot mutate the local terminal Workflow run afterward. | PASS for the approved ownership boundary; the new R2 direct proof agrees with source. |
| Native `401` with complete or unreadable body and no concurrent cancellation | The request is sent once. `force_refresh` runs after body collection succeeds or fails; successful refresh prepares a later call, renewal failure is authoritative, and unreadable body is returned only after successful renewal. | PASS; the R2 ordering fix and recorded raw-server evidence agree. |
| Cancellation/deadline after native `401` headers | The outer select drops body collection or `force_refresh`. The old cached access token can survive and be used by the next call. | **FAIL — SYS-R3-001.** |
| Wyrd server/network unavailable | Public calls are bounded by their per-step timeout/cancellation and normalize transport failure without client model replay. Remote lifecycle calls use the existing bounded transport behavior; accepted jobs, if any, remain server-owned. | PASS. |
| Server process exits after accepting a public gateway call | The client cannot recover that response. Gateway settlement and invocation audit have the existing process-owned shutdown/drain semantics; this task adds no alternate WAL, queue, or durable recovery claim. Unrelated replicas/capabilities are not crashed by the client error. | PASS; adding a recovery queue or setting would be DRIFT. |
| Fallback header is malformed, repeated, oversized, empty, duplicate, or self-referential | Authentication completes first; typed header validation refuses before gateway dispatch. No provider, accounting, capture, or gateway authorization work starts from that invalid request. | PASS. |
| Gateway authorization denied or audit work is slow/fails | Existing `GatewayInvocation` remains the owner. Authorization precedes protected work; invocation audit uses the approved tracked non-blocking canonical path, so audit persistence latency does not delay or reverse the decision, while shutdown drains tracked work. | PASS; TASK-003 does not create a bypass or second audit path. |
| Remote create response is lost | The server may already own the run. Transport retries preserve one idempotency key within the call; dropping and manually calling `create` again uses a new key, as documented. | PASS. |
| Remote get/cancel/wait interruption | A dropped wait sends no cancel or resubmission. A dropped cancel may have been applied and can be followed by get; cancellation is server-idempotent. | PASS. |
| Selected secret unavailable or invalid | Preparation fails before provider dispatch, without exposing the secret. Unselected bindings are not read. | PASS. |
| Slow configuration filesystem during selected external-route preparation | Synchronous config IO blocks an async runtime worker before dependency construction or cancellation reaches network work. | **FAIL — SYS-R3-002.** |

## Affected Capabilities and Recovery Boundary

The candidate affects the shared Rust client used by Rust, Python, TypeScript,
and CLI local execution; the Rust/CLI remote Workflow lifecycle facade; and the
existing public OpenAI Chat/Responses, Anthropic, and Gemini gateway ingress.
The public call does not add a server-run owner, provider credential owner,
gateway route, health loop, durable queue, retry worker, or recovery protocol.
Native and external-gateway execution retain their existing Skald owners.

For valid public calls, failure is request- or local-run-scoped. The shared
server's Cards, Bifrost, other gateway calls, and unrelated SDK operations
remain available; already accepted gateway settlement stays server-owned and
bounded. SYS-R3-001 widens the effect into the shared client's auth cache until
a later request refreshes it. SYS-R3-002 can temporarily reduce or eliminate
availability of the embedding async runtime, depending on its worker topology.

## Open Questions

None.

## Verification Notes

- This review was strictly source-only. It ran no build, compile, test, lane,
  Cargo, mise, pnpm, pytest, formatter, linter, package-manager, or other
  verification command.
- The implementer's recorded evidence was reviewed but not rerun. It records
  passing focused coverage for remote lifecycle, post-dispatch cancellation,
  complete-body renewal success/failure, and truncated-body renewal. Source
  agrees for those exact cases.
- No recorded case composes a known native `401` with Workflow cancellation or
  deadline expiry before renewal completes, so the evidence does not close
  SYS-R3-001.
- The selected-local-dependencies test records lazy/selected secret behavior,
  but no evidence can override the direct synchronous filesystem call in the
  async `run_with` path; SYS-R3-002 is source-established.
- TASK-004 and TASK-005 retain ownership of accepted server-run and complete
  cross-language Workflow journeys. Their absence is not reported here.

## Overall Result

**FAIL**

The cumulative candidate preserves the intended server failure isolation,
send-once model behavior, gateway authorization/audit owner, remote-run
ownership, and selected-secret boundaries. Two bounded resilience defects
remain: cancellation can leave a refused cached credential reusable after a
known native `401`, and selected external-route preparation performs blocking
configuration IO on the async executor.
