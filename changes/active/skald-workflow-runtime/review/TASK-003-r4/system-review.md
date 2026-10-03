# TASK-003-r4 system-resilience review

## Review Findings

### Critical

None.

### Important

- **SYS-R4-001 — a mixed local run assembles dependencies from two ambient
  configuration snapshots**
  (`crates/shared/wyrd-client/src/workflow/mod.rs:178-210`,
  `crates/shared/wyrd-client/src/config.rs:91-103`,
  `crates/shared/wyrd-client/src/client.rs:47-54`). **Classification:
  INCORRECT.** R3 requires the run-start preparation owner to load one ambient
  `GlobalConfig` snapshot and reuse its Workflow and client sections. When a
  client-less Workflow selects both `ext_gateway` and `wyrd_gateway`,
  `load_local_setup` first calls `GlobalConfig::load()` for the external
  bindings at lines 195-199. It then calls `WyrdClient::from_global()` at lines
  200-208; that reaches `ClientConfig::from_global()` and independently calls
  `GlobalConfig::load()` again. A configuration replacement between those
  reads can therefore give one Workflow run external origins/secret references
  from one version and the Wyrd endpoint, tenant, or token-cache settings from
  another. A transient read/parse failure on the second read can also refuse a
  run after its first snapshot was valid. The recorded R3 evidence says the
  complete ambient setup was moved to one blocking operation, but neither the
  existing selected-dependencies cases nor the implementation record proves
  one snapshot, and source contradicts that claim. Keep the human-approved
  run-start `spawn_blocking` boundary, read `GlobalConfig` once whenever the
  selected routes require ambient state, and build the existing `ClientConfig`
  and `WyrdClient` from that same value. Do not add caching, a watcher, a new
  configuration surface, a timing fixture, or another owner. Closure needs
  source evidence of one read feeding both consumers plus the existing focused
  selected-local-dependencies proof rerun by the implementer.

### Suggestions

None.

## Immutable Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `cfd60f6442e4dbdfcbc57cfec0bde494a225ae89`
- Approved specification:
  `changes/active/skald-workflow-runtime/spec.md`, Revision 12
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Remediations:
  `review/TASK-003-r1/TASK-003-R1-preserve-client-context-and-native-call-safety.md`,
  `review/TASK-003-r2/TASK-003-R2-close-renewal-proof-and-source-contracts.md`,
  and
  `review/TASK-003-r3/TASK-003-R3-close-pending-renewal-and-async-context-contracts.md`

The human-approved `WyrdGatewayCall.model` amendment, the R1 same-spec native
`401` correction, and the approval to keep configuration, client, and secret
reads at run start on the blocking pool were treated as authority. The complete
base-to-candidate range was inspected. The candidate remained unchanged.

## Deployed-Path Evidence

| Runtime path | Source evidence | Deployed effect |
|---|---|---|
| Shared local Workflow | `wyrd-client/src/workflow/mod.rs:35-175`; Python `PyWorkflow` retains `ClientWorkflow`; TypeScript `NativeWorkflow` retains the shared facade | A Cards-loaded Workflow keeps its loading client. A file-loaded Workflow builds ambient client context only when selected routes need it. Rust, Python, TypeScript, and CLI local execution converge on this shared owner. |
| Run-start dependency preparation | `workflow/mod.rs:130-149,178-210`; `workflow/local.rs:30-138` | Route selection happens before preparation. Ambient configuration/client work runs in one blocking-pool task, and selected secret files use the blocking pool. Mixed external/public-gateway routes nevertheless consume two independently loaded config snapshots, producing SYS-R4-001. |
| Public Wyrd gateway call | `workflow/gateway.rs:32-125`; `transport/http.rs:351-411` | Model, fallback, timeout, cancellation, and correlation remain request-local. The client sends one native model request through the shared authenticated pool and never replays it. |
| Native `401` recovery | `transport/http.rs:398-410`; `auth.rs:503-543` | Once response headers identify `401`, the existing refresh path is polled before body collection. Renewal failure is authoritative; after successful renewal, the original refusal or body-read result remains. |
| Public authenticated ingress | `wyrd-server/src/components/gateway/ingress.rs`; `components/gateway/routes.rs`; `wyrd-spec/src/gateway/policy.rs` | Existing authentication precedes fallback parsing. Valid fallback enters the existing governed invocation; invalid, oversized, duplicate, empty, or self-referential values stop before provider dispatch. |
| Remote Workflow runs | `workflow/remote.rs:23-121` | Create preserves one idempotency key across its transport retries; get and cancel return direct snapshots; wait polls once per second and dropping it does not cancel the server run. |

## Failure, Recovery, and Availability Assessment

| Failure or recovery path | What stops and what survives | Assessment |
|---|---|---|
| Public call cancelled before dispatch | The biased cancellation branch wins before the native send is polled. No gateway request starts; sibling client and server capabilities remain available. | PASS. |
| Public call cancelled after model dispatch | Local IO stops and no client resend occurs. The server may already own bounded gateway work; the client makes no rollback claim and cannot mutate the completed local run afterward. | PASS; recorded post-dispatch cancellation evidence matches source. |
| Native `401` with a pending, truncated, or complete body | Refresh begins after the response head and before body collection. A pending body cannot postpone the refresh attempt; the call remains bounded by the outer deadline/cancellation and sends one model POST. | PASS; the R3 source ordering and recorded pending-body case agree. Requiring an already-started exchange to survive arbitrary caller cancellation was previously rejected as unapproved drift and is not reopened. |
| Wyrd server or network unavailable | Public calls end at their per-call timeout/cancellation and do not replay model work. Remote lifecycle calls retain the established bounded transport behavior; any accepted server run remains server-owned. | PASS. |
| Slow configuration, credential, or selected secret storage | All task-added synchronous reads reachable from local run preparation are placed on Tokio's blocking pool. Dropping the run may leave a bounded blocking read to finish, but it writes no durable Workflow state. | PASS under the explicit human approval of these run-start reads. |
| Ambient configuration rotates during a mixed `ext_gateway` + `wyrd_gateway` run start | The first read supplies external bindings; the second independently supplies client endpoint/tenant/cache settings. The run can combine two generations or fail on only the second read. | **FAIL — SYS-R4-001.** |
| Invalid or unavailable selected external binding | Preparation fails before provider dispatch and exposes no secret value. Unselected bindings are not read. | PASS. |
| Remote create response is lost | The server may already have accepted the run. Retries inside that create retain its idempotency key; a later separate create is intentionally a new submission. | PASS. |
| Remote wait/get/cancel interruption | Dropping wait stops only polling. A lost cancel answer may follow an applied idempotent cancellation and can be reconciled with get. | PASS. |
| Client or server process exits during a public call | The client does not recover an ambiguous model response or replay it. Existing gateway process ownership controls settlement and shutdown; this task adds no queue, WAL, health loop, or cross-process recovery claim. | PASS; demanding such machinery would be DRIFT. |

## Affected Capabilities and Recovery Boundary

The candidate affects the shared Rust client used by Rust, Python, TypeScript,
and CLI local Workflow execution; the Rust/CLI remote run facade; and existing
public OpenAI Chat/Responses, Anthropic, and Gemini ingress. Failures ordinarily
remain request- or local-run-scoped, and the candidate adds no server process,
durable queue, alternate audit path, provider credential owner, or health loop.

SYS-R4-001 is confined to client-less local Workflows that select both public
Wyrd gateway and external-gateway routes, but those are an approved mixed-route
path. It can make one run internally inconsistent during normal atomic config
replacement and can turn a valid first snapshot into a run-start refusal on a
failed second read. Other requests and server capabilities remain available.

## Open Questions

None.

## Verification Notes

- This review was strictly source-only. It ran no build, compile, test, lane,
  Cargo, mise, pnpm, pytest, formatter, linter, package-manager, or other
  verification command.
- The implementer's recorded evidence was inspected but not rerun. It records
  six passing focused cases covering pending-body renewal, selected local
  dependencies, file loading, remote lifecycle, and auth renewal, plus clippy,
  formatting, and diff checks.
- Source agrees with the recorded pending-body renewal ordering, blocking-pool
  placement, client retention, and remote lifecycle behavior.
- The R3 record's claim that local setup uses one ambient configuration
  snapshot is contradicted by the two `GlobalConfig::load` paths in current
  source. No recorded case covers a client-less mixed external/public-gateway
  run consuming one coherent snapshot. This is a finding, not a residual
  verification limit.
- TASK-004 and TASK-005 own accepted server-run behavior and complete
  cross-language Workflow journeys; their future evidence is not charged to
  TASK-003.

## Overall Result

**FAIL**

The candidate closes the prior pending-body renewal and async-filesystem
findings while preserving send-once model calls, prompt cancellation, server
failure isolation, remote-run ownership, and selected-secret boundaries. One
bounded resilience defect remains: mixed local route preparation can combine
two ambient configuration generations instead of using the one run-start
snapshot required by R3.
