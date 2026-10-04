# TASK-004 R3 Maintainer Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `f17726fb25df1fa513875dca8d92f0073340ee0a`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 13
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Prior remediation tasks:
  - `changes/active/skald-workflow-runtime/review/TASK-004-r1/TASK-004-R1-close-accepted-job-gaps.md`
  - `changes/active/skald-workflow-runtime/review/TASK-004-r2/TASK-004-R2-align-revision-and-source-contracts.md`
- Review mode: static, read-only inspection of the complete base-to-candidate
  diff, current source, callers, tests, generated declarations, and recorded
  evidence. No build, test, Cargo, or mise command was run.

The candidate resolved to `f17726fb25df1fa513875dca8d92f0073340ee0a`
before inspection. `.codegraph/` is absent, so repository source, `rg`, and Git
were used directly.

## Governing Maintainer Authority

- `AGENTS.md` §§5, 6, 8, 9, 11, 12, 15, and 16: cohesive concrete owners,
  narrow async boundaries, public declaration parity, server-owned behavior,
  journey-first proof, minimum sufficient machinery, and substantive rustdoc.
- `architecture/agent-rules.md`: struct-centered Rust, module-level imports and
  bare interface types, source-owned error behavior, and documentation as a
  merge requirement.
- `architecture/references/languages/maintainer-style.md`: workflows live with
  their owner, helpers isolate real stages, names and types expose the contract,
  tests prove caller-visible outcomes, and documentation states lifecycle and
  failure behavior that source shape alone cannot convey.
- `architecture/references/architecture/patterns.md` and
  `architecture/references/languages/rust-core.md`: explicit server, Cards,
  gateway, Skald, and Bifrost ownership; concrete handles for dependency-backed
  workflows; traits only where multiple environments implement the seam.
- `architecture/references/languages/spec-driven-development.md` and
  `architecture/references/languages/testing-workflows.md`: Revision 13 and the
  cumulative candidate govern the task; recorded evidence is checked against
  the named source rather than treated as a substitute for it.
- `architecture/wyrd-design.md`, `architecture/wyrd-security-posture.md`, and
  `architecture/bifrost-design.md`: accepted-job authority, exact Card graph
  pinning, governed gateway ownership, and Oracle/query settlement boundaries.
- Standing human direction: unsupported mechanisms, checks, files, settings,
  and options are drift. The deleted graph-drain polling and supervisor idle
  refusal remain deleted; follower release is grant-stream close without a
  leader acknowledgement; and foreign-tenant journeys do not need a model step
  because only the fixture tenant has gateway credentials.

## Changed-Surface Coverage

| Material surface | Changed owners, callers, and proof inspected | Maintainer result |
|---|---|---|
| Workflow HTTP surface and server composition | `workflow_runs_router` and its three typed handlers in `components/workflow/routes.rs`; `WorkflowRunHost::{create,get,cancel}`; `/v1` router mounting; `AppState::workflows`; boot attachment; and `BoundServer` shutdown ordering | **PASS.** The handlers remain parsing adapters, while authenticated/audited orchestration is discoverable on the dependency-owning host. Composition exposes one process-local owner rather than route-local state. |
| Admission, idempotency, snapshots, retention, and shutdown | `WorkflowRuns`, `RunTable`, `Reservation`, `AcceptedRun`, preparation tracking, blocking-work tracking, host callers, focused owner tests, and Scenarios 1, 2, and 6 in `pg_workflow_runs.rs` | **PASS.** `WorkflowRuns` is a cohesive state owner (`runs.rs:212-433`); reservation and accepted-run handles express the ownership transfers directly (`runs.rs:530-688`). R1 corrections reuse `TaskTracker`, its token, watch channels, and cancellation rather than adding a scheduler, actor, or second lifecycle service. |
| Workflow preparation and Skald execution | `AgentTools`, `CardBodyResolver`, `Workflow::{from_card_bodies,prepare,run_with_options}`, `PreparedWorkflowRun`, `WorkflowExecutor::drive`, shared-client hydration, server preparation, and Skald tests | **PASS.** Pure preparation remains synchronous and executor dispatch remains in Skald. The tool-resolver trait has real client/server implementations with per-Agent binding semantics; it is not a trait around one implementation. The prepared-run value gives server acceptance a clear pre-dispatch boundary without duplicating the engine. |
| Exact Card graph pinning and built-in Card reads | `GraphBounds`, `PinnedWorkflowGraph::{pin,check_ref,workflow,agents,body}`, `active_row`, registration resolver context, `get_card_by_ref_for`, host preparation, and Card/tool journeys | **PASS.** Registry IO and audited Card reads remain on Cards owners. `PinnedWorkflowGraph` is a bounded immutable execution snapshot, not a second registry, traversal framework, or client-side graph store. |
| In-process governed gateway adapter | `ServerWyrdGatewayCaller`, `NativeCall`, `Answer`, request/response projection helpers, `GatewayInvocation` caller, and Scenario 5 dialect/fallback/deadline/cancellation assertions | **PASS.** The adapter translates Skald's existing provider enum into the existing governed gateway invocation owner. Provider-specific projection is localized, named by native dialect, and introduces no transport, policy path, credential cache, or compatibility layer. |
| Built-in Workflow tools and bounded query collection | `RunTools`, `AgentRunTools`, `QueryTool`, `CardsTool`, `QueryArguments`, `BoundedQuery`, `ResultCollector`, MCP callers, and Scenario 4 | **PASS.** Tool methods document their provider-visible contracts. MCP and Workflow share one bounded collection owner; exact schemas describe the value actually returned. The run-owned tracker makes query lifetime and terminal ordering visible without moving Oracle settlement into the Agent tool. |
| Query open, cancellation, and settlement | `RunningQueryControls::open_cancellable`, `cancel_while_opening`, bounded and scheduled query callers, `RunningQueryEntry` telemetry cancellation, Oracle pre-stream failure settlement, and recorded focused proof | **PASS.** Cancellation-during-open is a focused method on the existing controls owner and preserves the original deadline. Pre-stream failures reuse the existing analytical settlement path. No second timeout option, retry protocol, query engine, or lifecycle registry was introduced. |
| Oracle graph lifecycle and Bifrost remediation | cumulative changes in `oracle/{analytical,analytical_supervisor,mod,query_stream,running}.rs`, `resources.rs`, peer-cluster helpers, and forwarded Workflow journeys | **PASS.** Deletions remove the rejected drain polling, idle-envelope refusal, and release-time reserved-byte poison. Current `AnalyticalGraphLifecycle` documentation (`analytical.rs:2492-2506`) matches the implemented stream-close release: grants are retained to settlement, their drop is leader-side release, followers settle asynchronously, and the leader awaits no acknowledgement. |
| Server configuration and test scheduling | `ServerWorkflowConfig`, `ServerExternalGatewayBindingConfig`, validation/defaults, `AppState::with_workflow_config`, server/test composition, nextest `pg-servers`, and peer-cluster group membership | **PASS.** Workflow fields are the approved bounded configuration table. The nextest changes use its native test-group mechanism already present in this repository to cap production-shaped Postgres fixtures; they do not create a check or custom scheduler. No runtime option was added for fixed retention or follower release. |
| Permission and accepted authority surface | `Resource::Workflows`, `Permission::workflow_run`, builtin role grants, `Caller` capture at host acceptance, fresh route authorization, and cross-tenant/authority journeys | **PASS.** Naming follows the existing resource/action grammar, grants remain in the existing builtin-role table, and the server host retains a typed `Caller` rather than inventing a Workflow principal or token wrapper. |
| Revision 13 provider request contract | serde-tagged `ProviderRequest`, `ProviderName`, prompt builders and binding, provider send sites, cache/loader/Card consumers, internal gateway Vertex projection, round-trip/mismatch tests, Python accessor updates, generated JSON schemas, and executable fixtures | **PASS.** Serde adjacent tagging is the language-native mechanism. The handwritten discriminator and helper wire structs were deleted; generated contracts project the same tagged enum. `RawV1` carries `serde_json::Value` and converts only at provider send sites, keeping serialization ownership local. |
| Client and SDK-facing local Workflow composition | `WorkflowBodies::hydrate`, `SelectedRoutes::dependencies`, shared `resolve_binding`, local facade exports, and local/registered Workflow callers | **PASS.** The client facade remains thin over loader, Cards, Skald, and provider owners. The shared secret-reference resolver has one concrete operation used by local and server binding preparation; it does not add graph, registry, or server lifecycle logic to the client. |
| Journey and fixture structure | seven production-shaped server scenarios in `pg_workflow_runs.rs`; forwarded Oracle Workflow journey; `PeerCluster`; gateway mock routing; CLI, Python, and Workflow-loading fixtures; provider-tagged examples | **PASS.** New behavior is exercised through real server/Postgres or multi-pod boundaries. Helpers remain local to the owning integration target, and R1/R2 corrections extend existing fixtures rather than introducing a parallel harness. The large Workflow binary is cohesive around one accepted-run resource and avoids another heavy link target. |
| Documentation, metadata, and generated declaration parity | Revision 13 task/remediation front matter; canonical Prompt snippets and checked-in examples; substantive rustdoc on changed production and test-support items; Oracle lifecycle docs; generated schemas; and recorded codegen/typecheck evidence | **PASS.** Current task authority names Revision 13 while the r1 immutable-input line accurately preserves its historical Revision 12 subject. Prompt examples use the tagged envelope. R2 bare-type corrections place dependencies in module-level imports, with `TokioInstant` used only for the real `Instant` collision. No hand-edited stub or duplicate authority file was added. |

## Review Findings

### Critical

None.

### Important

None.

### Suggestions

None. Optional refactors, style preferences, and unrelated pre-existing debt are
outside this acceptance review.

## Drift and Simplicity Assessment

- New stateful behavior is owned by the smallest existing layer: Skald owns
  reusable Workflow preparation/execution, Cards owns registry graph reads,
  the gateway owns governed model invocation, Oracle owns query settlement,
  and `WorkflowRuns` owns only process-local accepted-run lifecycle.
- The implementation uses conventional and already-installed mechanisms:
  serde adjacent tagging, Axum typed routes, Tokio cancellation/watch/task
  tracking, standard mutex-protected in-memory state, schemars/jsonschema for
  declaration proof, and nextest test groups.
- No new crate, third-party runtime dependency, durable queue, actor framework,
  release RPC, acknowledgement protocol, polling service, compatibility reader,
  migration, standalone check, tenant simulator, or credential administration
  surface entered the cumulative implementation.
- I found no changed mechanism, check, file type, setting class, or option that
  lacks both an established Wyrd precedent and a conventional comparable-project
  mechanism.

## Open Questions

None affecting maintainability or acceptance risk.

## Verification Notes

- Per the strict review instruction, no build, test, Cargo, or mise command was
  run during this review.
- The review relied on the task and remediation records' focused and broader
  results, including Workflow scenarios, Skald/shared/Wyrd families, gateway
  and Bifrost journeys, Python and TypeScript lanes, code generation, boundary
  checks, formatting, and lints. Inspection confirmed that the named tests call
  the changed owners and assert the documented contracts.
- R2 records behavior-neutral proof for the imported bare-type edits, tagged
  examples, Revision 13 metadata, and Oracle documentation. No new test or
  custom check was necessary for those corrections.
- Residual limit: this review did not independently execute the recorded
  commands. No source, caller, declaration, authority, or required
  maintainer-review input was unavailable.

## Overall Result

**PASS**

The cumulative candidate is maintainable under the applicable Wyrd authority.
Owner and method boundaries are discoverable, names and types communicate the
contract, rustdoc covers non-obvious lifecycle and failure behavior, tests are
organized around caller-visible outcomes, generated declarations align with
source, the R2 source-shape and documentation findings are closed, and no
unsupported drift remains.
