# TASK-004 R4 Maintainer Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `5cde1b48aab0d70d8686ee8fb5f2978e26cd7f58`
- Approved specification:
  `changes/active/skald-workflow-runtime/spec.md`, Revision 13
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Remediation tasks: R1 close accepted-job gaps, R2 align revision and source
  contracts, and R3 close step-attempt and import gaps
- Review mode: static, read-only inspection of the complete cumulative diff,
  current owners and callers, relevant tests, generated schemas, and recorded
  evidence. No build, test, Cargo, or mise command was run.

The requested candidate resolved to the current `HEAD` before and after source
inspection. `.codegraph/` is absent, so Git, `rg`, and direct source inspection
were used.

## Governing Maintainer Authority

- `AGENTS.md` §§5, 6, 9, 11, 15, and 16: cohesive concrete owners,
  narrow async boundaries, server-owned durable behavior, journey-first proof,
  minimum sufficient machinery, and substantive rustdoc on every changed Rust
  item.
- `architecture/agent-rules.md`: struct-centered Rust, module-scope imports and
  bare interface types, source-owned error behavior, and documentation as a
  merge requirement.
- `architecture/references/languages/maintainer-style.md`: workflows live with
  their owner, helpers isolate real stages, names and types expose the
  contract, tests prove caller-visible outcomes, and documentation records
  lifecycle behavior that cannot be inferred safely from signatures alone.
- `architecture/references/architecture/patterns.md` and
  `architecture/references/languages/rust-core.md`: explicit Cards, gateway,
  Skald, server, and Bifrost ownership; dependency-backed workflows on concrete
  handles; traits only at real environment seams.
- `architecture/wyrd-design.md`, `architecture/wyrd-security-posture.md`, and
  `architecture/bifrost-design.md`: accepted-job authority, exact Card graph
  pinning, governed gateway ownership, and Oracle/query settlement boundaries.
- Standing human direction: mechanisms, checks, files, settings, and options
  unsupported by both established practice and comparable widely used
  projects are drift. The fixed Oracle, follower-release, foreign-tenant
  fixture, and step-attempt decisions are not open for reconsideration.

## Changed-Surface Coverage

| Material surface | Owners, callers, and relevant proof inspected | Maintainer result |
|---|---|---|
| Workflow HTTP and server composition | `components/workflow/routes.rs`, `WorkflowRunHost`, `AppState::workflows`, boot attachment, `/v1` router mounting, and `BoundServer` drain ordering | **PASS.** Routes are typed adapters; authenticated and audited orchestration is discoverable on the concrete host, while process-local state and shutdown remain on one shared owner. |
| Admission, idempotency, retention, cancellation, and shutdown | `WorkflowRuns`, `RunTable`, `Reservation`, `AcceptedRun`, preparation/blocking tracking, host callers, owner tests, and the accepted-run journeys | **PASS.** The mutex protects one compact state table and no locked path awaits. Reservation and accepted-run handles make slot, key, cancellation, and tracker ownership explicit without an actor, scheduler, or second lifecycle registry. |
| Workflow preparation and exact Card graph pinning | `Preparation::{run,prepare}`, `PinnedWorkflowGraph`, `GraphBounds`, Cards resolution and exact-ref read seams, Skald hydration, and registration/run callers | **PASS.** Registry IO stays with Cards and server preparation; the pinned graph is the bounded immutable input to one execution, not a duplicate registry or general graph framework. Pure Skald preparation remains synchronous and dispatch-free. |
| Skald execution and R3 attempt lifecycle | `Workflow::{prepare,run_with_options}`, `PreparedWorkflowRun::execute`, `WorkflowExecutor::{drive,settle}`, `StepTask::run`, `RunLedger::{step_started,step_cancelled,finish}`, server transition storage, and the focused transition/lifecycle assertions | **PASS.** Scheduling now establishes attempt one in both observable ledger state and the existing shared counter before publishing `Running`. Aborted published work follows the existing cancelled-active path; pending work alone becomes `Unstarted`. The correction adds no handshake, channel, owner, option, or downstream guard. |
| Per-Agent tools and built-in services | `AgentTools`, `CardBodyResolver`, `RunTools`, `AgentRunTools`, `QueryTool`, `CardsTool`, their `ToolResolver`/`AgentTool` callers, and tool journeys | **PASS.** The resolver seam has real client and server environments and binds tools per executing Agent. Tool implementations delegate to the existing Cards and bounded-query owners and expose exact schemas; no tool platform or transport was duplicated. |
| Shared bounded query collection | `QueryArguments`, `BoundedQuery`, `ResultCollector`, MCP integration, Workflow tool integration, scheduled queries, `RunningQueryControls::open_cancellable`, and collector/control tests | **PASS.** Collection is owned once and reused by MCP and Workflow. Cancellation while opening stays a focused method on the existing controls owner and reuses the request deadline; result projection and byte accounting remain cohesive on `ResultCollector`. |
| Governed model routing | `ServerWyrdGatewayCaller`, `NativeCall`, `Answer`, `GatewayInvocation`, ingress usage-bound helpers, provider-specific send sites, and gateway dialect/fallback/deadline/cancellation journeys | **PASS.** The adapter performs only native request/response projection and delegates authorization, admission, routing, accounting, and settlement to the existing gateway. It adds no transport, policy path, credential cache, or compatibility layer. |
| Revision 13 provider request contract | `ProviderRequest`, `ProviderName`, Prompt rendering/binding, loader/cache/Card consumers, provider clients, Python accessors/tests, executable fixtures, and generated Prompt/Card schemas | **PASS.** Serde adjacent tagging is the native, conventional mechanism. Typed variants, `RawV1`, examples, and generated declarations use the same `provider`/`body` contract; no handwritten alternate decoder or migration path remains. |
| Configuration and scheduling | `ServerWorkflowConfig`, external gateway binding assignment, boot validation, test-server composition, `.config/nextest.toml`, and affected manifests | **PASS.** The Workflow configuration fields are the approved timeout, capacity, graph, input, result, and retention bounds. Nextest's native test-group facility extends an existing repository pattern to cap production-shaped Postgres fixtures; it is not a custom scheduler or check. |
| Permission, authority, and tenancy | `Resource::Workflows`, `Permission::workflow_run`, builtin grants, `Caller` capture, per-request route authorization, audited Cards/query/gateway calls, and negative/cross-tenant journeys | **PASS.** Existing permission, caller, and audit owners are extended directly. No Workflow principal, bearer retention, grant refresh, second audit sink, or tenant-specific runtime abstraction was added. |
| Oracle and forwarded-query lifecycle | cumulative changes under `vala-bifrost-redux::oracle`, `RunningQueryControls`, peer-cluster helpers, and the forwarded Workflow journey | **PASS.** The implementation deletes the rejected graph-drain polling and idle refusal. `AnalyticalGraphLifecycle` documents and implements leader-side grant-stream close with asynchronous follower settlement and no acknowledgement. The fixed human decision is visible at the actual owner rather than encoded as a new protocol. |
| Client/local composition | shared-client Workflow hydration, local facade exports, `resolve_binding`, registered/local Workflow callers, and SDK fixture updates | **PASS.** Client code remains a thin composition of loader, Cards, Skald, and provider owners and does not acquire accepted-run lifecycle or server authority behavior. |
| Journey and fixture organization | `pg_workflow_runs.rs`, Oracle Workflow journey, `PeerCluster`, gateway mocks, CLI/Python fixtures, and provider-tagged examples | **PASS.** The large server journey target is cohesive around one accepted-run resource and avoids another heavy test binary. Helpers extend the repository-managed server, Postgres, and peer-cluster harnesses instead of adding a parallel fixture system. The approved foreign-tenant limit is stated in the task evidence and does not create fake credentials. |
| Documentation, imports, and declaration parity | changed Rust items and tests, R1/R2/R3 task records, canonical Prompt snippets, generated schemas, Oracle lifecycle rustdoc, and every R3 import site | **PASS.** Non-obvious cancellation, retry, partial-progress, and settlement behavior is documented at its owner. `Value`, `Instant`, `Bytes`, and Unix `PermissionsExt` now live in the appropriate module import blocks, with bare interface names and no enforcement scanner or allow mechanism. |

## Material Findings

None.

I found no changed location with a concrete maintenance cost that warrants a
blocking or advisory finding. In particular, the two R3 correction sites do
not introduce a second attempt owner: the atomic counter remains the running
task's retry progress, while `RunLedger` remains the observable snapshot owner;
both must receive attempt one before publication to keep those existing
representations consistent.

## Drift and Simplicity Assessment

- New stateful behavior stops at the first established owner: Skald executes,
  Cards resolves registry graphs, the gateway invokes models, Oracle settles
  queries, and `WorkflowRuns` retains process-local accepted jobs.
- New mechanisms are conventional and already installed: Axum typed routes,
  serde adjacent enum tagging, Tokio cancellation/watch/task tracking,
  standard mutex-protected in-memory state, and nextest test groups.
- No new crate, actor framework, durable queue, release RPC, acknowledgement
  channel, polling service, compatibility reader, migration, standalone check,
  configuration class, tenant simulator, or credential administration surface
  entered the implementation.
- The explicit Workflow settings correspond to approved, commonly configured
  job-system bounds. No setting or option exists solely for the deleted Oracle
  behavior, the R3 attempt correction, or hypothetical future flexibility.

## Calibration Notes

None. I did not retain line-count preferences, alternative private helper
shapes, or unrelated pre-existing documentation as findings.

## Verification Notes

- Per the strict instruction, this review ran no build, test, Cargo, or mise
  command. It relied on the recorded focused and broader results and verified
  statically that the named tests call the changed owners and assert the stated
  contracts.
- R3 records exact focused Skald tests for published-running attempts and the
  pre-poll abort, plus Skald, Wyrd server, and Oracle journey lanes. The source
  assertions now require attempt one, `Cancelled`, and retained timestamps;
  existing success and retry-exhaustion assertions retain their totals.
- Recorded R1/R2 evidence covers accepted-run journeys, query/Oracle recovery,
  provider dialects, generated contracts, SDK surfaces, formatting, lints, and
  boundary checks. This review did not independently re-execute those claims.
- No maintainer-review input, owner, caller, relevant test, generated
  declaration, or applicable authority was unavailable.

## Overall Result

**PASS**

The cumulative candidate is maintainable under the applicable Wyrd authority.
Owners and method boundaries are discoverable, types and names communicate the
contracts, lifecycle documentation captures the non-obvious behavior, tests
are organized around caller-visible outcomes, R3 closes the observable
step-attempt and module-import gaps at their existing owners, and no unsupported
mechanism or speculative surface remains.
