# TASK-004 R5 Maintainer Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `d4d4e2da53abfc677abdb804e71517c3b6849f49`
- Approved specification:
  `changes/active/skald-workflow-runtime/spec.md`, Revision 13
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Remediation tasks: TASK-004 R1 through R4 in their preceding review
  directories
- Review mode: static, read-only inspection of the cumulative diff, current
  owners and callers, relevant tests, generated contracts, and recorded
  evidence. No build, test, Cargo, or mise command was run.

The candidate resolved to the requested commit before and after inspection.
`.codegraph/` is absent, so the review used Git, `rg`, and direct source
inspection.

## Governing Maintainer Authority

- `AGENTS.md` §§5, 6, 9, 11, 12, and 16: cohesive concrete owners, narrow
  async boundaries, server-owned behavior, journey-first proof, targeted
  completion evidence, and substantive rustdoc on every changed Rust item.
- `architecture/agent-rules.md`: struct-centered Rust, module-scope imports,
  bare interface types, repository-owned test placement, and documentation as
  a merge requirement.
- `architecture/references/languages/maintainer-style.md`: workflows stay with
  their owner, helpers isolate real stages, names and types expose contracts,
  tests prove caller-visible outcomes, and documentation states lifecycle
  behavior that cannot be inferred safely from a signature.
- `architecture/wyrd-design.md`, `architecture/wyrd-security-posture.md`, and
  `architecture/bifrost-design.md`: accepted-job authority, exact Card graph
  pinning, one total run deadline, governed gateway ownership, and Oracle/query
  settlement boundaries.
- Standing human direction: a mechanism, check, file, setting, or option with
  neither established Wyrd precedent nor precedent in comparable widely used
  projects is drift. The fixed Oracle graph-release, follower-release,
  foreign-tenant fixture, published-step-attempt, and prepared-deadline
  decisions were treated as authority and were not reopened.

## Changed-Surface Coverage

| Material surface | Owners, callers, and relevant proof inspected | Maintainer result |
|---|---|---|
| Workflow HTTP and server composition | `components/workflow/routes.rs`, `WorkflowRunHost`, `AppState::workflows`, boot attachment, router mounting, and `BoundServer` shutdown ordering | **PASS.** Typed routes remain thin adapters; authorization, preparation, and execution remain discoverable on the dependency-owning host. |
| Admission, idempotency, retention, cancellation, and shutdown | `WorkflowRuns`, `RunTable`, `Reservation`, `AcceptedRun`, preparation tracking, host callers, and accepted-run journeys | **PASS.** One cohesive owner retains the process-local state and lifecycle invariants without IO under its lock, an actor, a scheduler, or a second run registry. |
| Exact Card graph preparation | `Preparation::{run,prepare}`, `PinnedWorkflowGraph`, `GraphBounds`, Cards resolution/read seams, Skald hydration, and registration/run callers | **PASS.** Registry IO stays with Cards and server preparation; the pinned graph is one bounded execution snapshot, not a second registry or general graph framework. |
| Skald prepare/execute seam | `Workflow::prepare`, `WorkflowExecutor::{new,deadline,drive,settle}`, `PreparedWorkflowRun::{snapshot,deadline,execute}`, `StepTask`, and run-ledger transition tests | **PASS.** The executor remains the single owner of planning, attempt state, cancellation, terminalization, and the absolute total deadline. The new deadline accessors expose that existing value without sampling or owning another clock. |
| R4 deadline projection | `RunTools::{new,bind_deadline,for_agent,drain}`, `AgentRunTools`, `QueryTool::invoke`, `Preparation::prepare`, and the focused `tools_use_the_prepared_run_deadline` test | **PASS.** `Arc<OnceLock<Instant>>` is a standard-library one-assignment value justified by the existing hydration order: Agent tools are cloned before the prepared executor exists and cannot run until after preparation. Binding occurs before acceptance, every clone shares the same value, and query invocation only derives remaining duration from it. |
| Built-in read tools and bounded query collection | `QueryTool`, `CardsTool`, `BoundedQuery`, `ResultCollector`, MCP integration, Cards read integration, and declared-tool journeys | **PASS.** Concrete tool schemas and failure behavior remain documented. Workflow and MCP reuse the same query collection owner; R4 changes neither result decoding nor settlement ownership. |
| Forwarded Oracle query proof | `workflow_forwarded_query_settles_before_the_run_ends`, its upstream script, query-control callers, and the changed deadline-case assertions | **PASS.** The deadline case now supplies a post-tool model answer, so an early tool timeout would become an observable success instead of being hidden by a pending continuation. The change extends the existing journey and adds no fixture, pause protocol, or test binary. |
| Gateway, external route, and provider request contracts | `ServerWyrdGatewayCaller`, `GatewayInvocation`, provider projections, Revision 13 `ProviderRequest`, Prompt consumers, examples/fixtures, and generated schemas | **PASS.** Existing gateway/provider owners remain intact; serde adjacent tagging is the native contract mechanism and no compatibility decoder or parallel transport remains. |
| Authorization, audit, tenancy, and configuration | `Resource::Workflows`, `Permission::workflow_run`, builtin grants, captured `Caller`, per-call Cards/query/gateway decisions, `ServerWorkflowConfig`, and server composition | **PASS.** Existing authority, audit, and config owners are extended directly. The Workflow fields are the approved bounded job settings rather than speculative options. |
| Oracle lifecycle and analytical cleanup | cumulative Oracle changes, `RunningQueryControls`, query stream/resource owners, peer-cluster helpers, and recovery journeys | **PASS under the standing decisions.** Deleted graph-drain polling and supervisor idle refusal remain deleted. Follower release remains grant-stream close and the leader awaits no release acknowledgement. R4 does not touch or recreate those mechanisms. |
| Client/local composition and language contracts | shared-client Workflow hydration, local facade exports, provider request consumers, Python tests, and generated schema changes | **PASS.** Client code remains a thin composition over loader, Cards, Skald, and provider owners. `PreparedWorkflowRun::deadline` is a Rust execution-boundary value and creates no Python, TypeScript, wire, or generated-declaration obligation. |
| Test and fixture organization | `pg_workflow_runs.rs`, Oracle `workflow.rs`, server/tool unit tests, `PeerCluster`, gateway mocks, CLI/Python fixtures, and nextest grouping | **PASS.** External binaries continue to earn their location by driving real server/Postgres or peer-cluster boundaries. R4 adds one owner-local unit test and strengthens an existing journey; no new harness or repository check entered the candidate. |
| Documentation, imports, and declaration parity | changed production/test Rust items, R1–R4 records, architecture authority, provider-tagged schemas, and current import blocks | **PASS.** The new executor/prepared-run accessors, shared deadline field, binding method, host seam, and focused test have substantive rustdoc. Imports remain module-scoped and interfaces use bare names. No generated declaration was hand-edited. |

## Review Findings

None.

No changed location has a concrete maintenance cost that warrants a blocking
or advisory finding. In particular, the two-phase deadline handoff is not
speculative flexibility: it is the minimum standard-library mechanism needed
to cross the existing `from_card_bodies` hydration order while preserving the
single deadline already owned by `PreparedWorkflowRun`.

## Drift and Simplicity Assessment

- The R4 correction reuses the existing prepared executor, `RunTools`, Agent
  tool resolver, bounded query owner, cancellation tree, and Oracle settlement
  path. It adds no new owner or duplicated workflow.
- `std::sync::OnceLock` is a widely used native one-assignment primitive and is
  already used in Wyrd. It replaces the earlier second deadline sample rather
  than adding another clock or configuration surface.
- No timer service, task, channel protocol, retry, poll, scheduler, setting,
  option, dependency, checker, fixture system, compatibility path, or public
  wire field was added.
- The existing public `PreparedWorkflowRun` is the narrowest cross-crate owner
  from which `wyrd-server` can read the exact deadline. The accessor returns the
  executor's stored `Instant`; it neither permits mutation nor changes local
  duration-based run options.
- The fixed human decisions remain visible in their existing owners. No
  remediation asks for deleted Oracle polling, a follower acknowledgement,
  foreign-tenant gateway credentials, a different attempt lifecycle, or a
  second deadline boundary.

## Prior-Finding Closure

- `FIND-TASK-004-1` through `FIND-TASK-004-20` remain closed at their existing
  lifecycle, proof, contract, documentation, attempt-accounting, and import
  owners.
- `FIND-TASK-004-21` is closed at the root ownership seam:
  `WorkflowExecutor::new` fixes the deadline,
  `PreparedWorkflowRun::deadline` exposes that exact value,
  `Preparation::prepare` binds it before acceptance, and every hydrated
  `RunTools` clone shares the same `OnceLock`.
- The changed Oracle journey makes the former early-timeout path observable by
  answering the post-tool continuation, while retaining the broader
  settlement, pod-loss, and sibling-service assertions.

## Open Questions

None.

## Verification Notes

- Per the strict review instruction, I ran no build, test, Cargo, or mise
  command. I relied on the recorded R4 remediation evidence and inspected
  statically that the named tests reach the changed owners and assert the
  stated contract.
- R4 records a red/green focused test showing that a clone created before a
  one-second paused-time advance receives the later prepared deadline exactly.
  It records the strengthened forwarded-Oracle deadline case with a completed
  post-tool continuation, plus passing Skald, Wyrd server, and Oracle journey
  lanes, formatting, lints, and `git diff --check`.
- Earlier recorded evidence covers all seven accepted-run scenarios, the
  bounded query collector and Oracle lifecycle paths, gateway/provider
  dialects, generated contracts, SDK surfaces, boundary checks, and the prior
  remediation closures. This review did not independently re-execute those
  results.
- No maintainer-review input, source owner, caller, relevant test, generated
  contract, or applicable authority was unavailable.

## Overall Result

**PASS**

The cumulative candidate is maintainable under the applicable Wyrd authority.
Its owners and method boundaries remain discoverable, its names and types
communicate the lifecycle contracts, and R4 closes the final deadline ownership
gap with one standard-library binding at the existing server/Skald seam. No
unsupported mechanism, speculative option, duplicate workflow, or unresolved
maintainer finding remains.
