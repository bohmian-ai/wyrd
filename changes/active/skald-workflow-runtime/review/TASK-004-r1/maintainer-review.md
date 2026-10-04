# TASK-004 Maintainer Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `96e993a16706d2fb759e4cdb7371ff490b198a35`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, revision 12
- Task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Review mode: static, read-only inspection of the complete base-to-candidate diff, candidate source, callers, relevant tests, and the task's recorded verification evidence. No build, test, Cargo, or mise command was run.

The candidate identity was resolved before and after source inspection and remained `96e993a16706d2fb759e4cdb7371ff490b198a35`.

## Governing Maintainer Authority

- `AGENTS.md` §§5, 6, 11, 15, and 16: struct-centered ownership, narrow async boundaries, user-journey placement, the Ponytail reuse ladder, and mandatory substantive rustdoc for every new or materially modified Rust item.
- `architecture/agent-rules.md`: struct-centered Rust is a hard acceptance criterion; rustdoc for new private and public items is `BLOCK_BEFORE_MERGE`; external tests must earn their heavy binary; no invented gate or check may replace root-cause correction.
- `architecture/references/languages/maintainer-style.md`: workflows stay with their owner, shared dependencies are owned once, helpers expose meaningful stages, journeys assert caller-visible outcomes, and documentation must explain the contract a maintainer cannot infer safely.
- `architecture/references/architecture/patterns.md` and `architecture/references/languages/rust-core.md`: server behavior remains on cohesive concrete owners, pure work remains synchronous, traits are reused only where an established multi-implementation boundary exists, and dependency cost remains narrow.
- `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, `architecture/bifrost-design.md`, and the applicable agent/OLAP reference: server/client, Card, gateway, query-terminal, cancellation, and analytical-ownership boundaries.
- Human standing direction: Wyrd follows established repository and widely used project mechanisms. I found no candidate-only check, file type, setting class, option class, or mechanism that lacks both an established Wyrd precedent and a conventional ecosystem precedent. In particular, the nextest test group uses the repository's existing nextest grouping mechanism; the Workflow configuration fields and process-local run host are explicit approved-task requirements; and the candidate deletes bespoke polling/drain machinery instead of replacing it with another custom mechanism.

## Changed-Surface Coverage

| Material surface | Changed symbols and owner shape inspected | Callers and proof inspected | Maintainer result |
|---|---|---|---|
| Accepted-run HTTP surface | `workflow_runs_router`, the three handlers, `WorkflowRunHost::{create,get,cancel,authorize_run,timeout}` | `/v1` router composition, `AppState`, Rust-client/HTTP journeys in `pg_workflow_runs.rs` | PASS. Handlers remain thin and the dependency-owning host exposes discoverable operations. |
| Process-local run lifecycle | `WorkflowRuns`, `RunTable`, `Reservation`, `AcceptedRun`, `Preparation` | server composition, `BoundServer::run` shutdown ordering, replay/cancel/retention/shutdown scenarios | PASS. One concrete owner contains admission, snapshots, retention, cancellation, and tracked shutdown; lock-held code performs no IO. |
| Workflow graph preparation | `GraphBounds`, `PinnedWorkflowGraph::{pin,check_ref,workflow,agents,body}`, `active_row` | registration resolver, Skald body hydration, server preparation, graph/snapshot journey | PASS. The extension stays on the existing Cards resolver and does not add a second graph type outside the required pinned server value. |
| Skald prepare/execute seam | `AgentTools`, `CardBodyResolver`, `Workflow::prepare`, `PreparedWorkflowRun`, executor transition observer | shared-client hydration, server preparation, Skald unit tests | PASS. Preparation is synchronous pure work; execution remains on the existing executor; the new value is required to publish an accepted queued snapshot before dispatch. |
| Gateway execution adapter | `ServerWyrdGatewayCaller`, `NativeCall`, `Answer`, extracted usage-bound helpers | `GatewayInvocation::run`, native ingress/routes, gateway and workflow journeys | PASS. The adapter is narrow, uses the existing invocation owner, and does not create a second transport or policy path. |
| Built-in Workflow tools | `RunTools`, `AgentRunTools`, `QueryTool`, `CardsTool` | `AgentTools` hydration, `get_card_by_ref_for`, `BoundedQuery`, declared-tool journey | **FAIL: MAINT-001.** The owner and resolver layout are cohesive, but eight new trait methods omit required rustdoc. |
| Shared bounded query collection | `QueryArguments`, `BoundedQuery`, `ResultCollector`, projection/accounting helpers | MCP adapter after extraction, Workflow query tool, focused collector tests and recorded MCP journeys | PASS. Existing MCP collection behavior was moved to one shared owner rather than copied; arguments and result accounting remain named and bounded. |
| Oracle open/cancel/settle path | `RunningQueryControls::open_cancellable`, `cancel_while_opening`, scheduled caller integration, `release_error`, pre-stream telemetry marker | Workflow forwarded-query journey, scheduled-query caller, running-query registry, recorded focused Oracle evidence | PASS. The lifecycle control remains the owner and the helper isolates one cancellation/open ordering rule without adding another engine or retry path. |
| Analytical cleanup simplification | removal of graph-child polling/drain loops, supervisor refusal, and release-time nested-child poison; retained shared-root/view ownership | analytical supervisor, admission guard, resource governor, updated architecture and recorded peer/readiness journeys | PASS. This deletes custom timeout/polling machinery and relies on the established reference-counted shared memory root, which is the simpler conventional ownership model. |
| Auth, permission, config, and composition | `Resource::Workflows`, `Permission::workflow_run`, builtin grants, `ServerWorkflowConfig`, `AppState::with_workflow_config` | boot composition, role tests, config tests, route authorization journeys | PASS. Names and types are domain-specific; the configuration is the exact approved table rather than speculative options. |
| Test infrastructure and journeys | nextest `pg-servers`, `pg_workflow_runs.rs`, Oracle `workflow.rs`, peer-cluster gateway/pause support, fixture repairs | task scenarios 1–7, recorded diagnosis/evidence, relevant existing heavy-test layout | PASS. Both external binaries cross real server/HTTP/Postgres or peer boundaries and therefore earn their location. Helpers are local to the journeys that use them. The concurrency cap reuses nextest's standard group mechanism and addresses a recorded shared Postgres ceiling. |
| Documentation and declarations | module docs, item rustdoc, route OpenAPI annotations, architecture update, task evidence | public/internal signatures and generated-contract evidence recorded in the task | **FAIL only at MAINT-001.** Other materially added owners, values, helpers, routes, and tests have substantive intent/error/cancellation documentation. No hand-edited generated declaration entered the diff. |

## Material Finding

### MAINT-001 — New production `AgentTool` associated methods lack mandatory rustdoc

- Severity: must-fix
- Changed locations: `crates/wyrd/wyrd-server/src/components/workflow/tools.rs:147`, `:151`, `:155`, `:159`, `:238`, `:242`, `:246`, and `:261`.
- Governing rule: `AGENTS.md` §16 and `architecture/agent-rules.md` require substantive rustdoc on every new or materially modified Rust item, including private methods; missing rustdoc is a hard merge blocker. `maintainer-style.md` further requires documentation to state the contract a maintainer needs to change behavior safely.
- Evidence: both new production implementations of `AgentTool` document `invoke`, but their `name`, `description`, `input_schema`, and `output_schema` methods have no item documentation. These methods define the provider-visible names and descriptions and the machine-readable JSON contracts for `bifrost.query` and `cards.get`; inherited trait prose does not describe the concrete schemas, the Agent-space default, or the complete-result shape of these implementations.
- Concrete maintenance cost: a maintainer changing either tool has to reconstruct which method is the stable provider contract and why its concrete schema differs from the other tool. The omission is especially misleading because `invoke` is thoroughly documented while the four declaration methods that make the tool discoverable are silent.
- Smallest testable correction: add concise rustdoc to each of the eight associated methods, stating its concrete returned name/description/schema and the relevant invariant (closed arguments, Agent-space default, or complete-result/Card object). Do not introduce a helper, wrapper, lint, check, schema generator, or new abstraction; this is a local documentation correction only.

## Ponytail Assessment

- Delete: no new lifecycle owner, graph owner, query collector, gateway adapter, or test fixture can be deleted while preserving the approved accepted-job behavior. The candidate appropriately deletes the earlier graph-drain polling and poison checks.
- Reuse repository behavior: the implementation reuses Cards resolution and audited reads, `GatewayInvocation`, Skald's executor and `AgentTool` boundary, the query service and Oracle lifecycle controls, `TaskTracker`, `CancellationToken`, watch channels, and nextest test groups.
- Standard/native mechanisms: `Mutex` for short process-local state, `watch` for current snapshot/outcome publication, `TaskTracker` for shutdown drain, `CancellationToken` for structured cancellation, and serde/default config are conventional and already established in Wyrd.
- Installed dependencies: BLAKE3/JCS, Arrow JSON encoders, and existing gateway/query crates are reused; no new dependency or speculative generic framework was added.
- New code is limited to the required cohesive server host, its run-state owner, two concrete tool adapters, and their production-shaped journeys.

## Verification Evidence and Limits

Per the review instruction, I ran no commands that build, test, lint, format, invoke Cargo, or invoke mise. I relied on the task's recorded successful evidence, including all seven exact `pg_workflow_runs` scenarios, the focused collector and Oracle lifecycle tests, the forwarded-Oracle journey, MCP query journeys, server/scheduled-query regression journey, scoped Wyrd/shared/principals/gateway/Bifrost lanes, codegen, boundary checks, formatting, lints, and `git diff --check`.

This static maintainer review does not independently certify those command results. The limit does not change MAINT-001: the missing documentation is directly visible in the immutable candidate source and is governed by an explicit hard repository rule.

## Calibration Notes

No uncertain preference is promoted to a finding. In particular, I did not flag the size of the production-shaped journey files, the `AppState` handles used by existing server patterns, the per-Agent resolver factory, the standard nextest concurrency group, or the generic tool output schemas as maintainer failures without a stronger violated authority and concrete maintenance cost.

## Overall Result

**FAIL**

The candidate's owner layout, naming, method boundaries, tests, and deletion of bespoke cleanup machinery are otherwise maintainable and appropriately minimal. MAINT-001 is a bounded local correction, but the repository explicitly makes this documentation omission a merge blocker.
