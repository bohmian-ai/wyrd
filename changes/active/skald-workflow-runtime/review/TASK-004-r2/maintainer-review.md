# TASK-004 R2 Maintainer Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `e86831e5ac784028f8022cc3faeee1c22b12c665`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, revision 13
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Remediation task: `changes/active/skald-workflow-runtime/review/TASK-004-r1/TASK-004-R1-close-accepted-job-gaps.md`
- Review mode: static, read-only inspection of the complete base-to-candidate changed-file inventory and diff, current source, callers, tests, generated declarations, and recorded evidence. No build, test, Cargo, or mise command was run.

The candidate identity was resolved before and after inspection and remained
`e86831e5ac784028f8022cc3faeee1c22b12c665`. `.codegraph/` is absent, so the
review used repository source, `rg`, and Git directly.

## Governing Maintainer Authority

- `AGENTS.md` §§5, 6, 8, 9, 11, and 12: cohesive concrete owners, narrow async
  boundaries, declaration parity, server-owned durable behavior, journey
  coverage, and completion evidence.
- `architecture/agent-rules.md`: struct-centered Rust and substantive rustdoc
  are hard acceptance criteria; tests use repository-native owners and may not
  invent a gate to conceal a defect.
- `architecture/references/languages/maintainer-style.md`: workflows stay with
  their owner; helpers isolate real stages; names and types expose the contract;
  tests prove caller-visible outcomes; documentation states non-obvious
  lifecycle and failure behavior.
- `architecture/references/architecture/patterns.md` and
  `architecture/references/languages/rust-core.md`: server, Cards, gateway,
  Skald, and Bifrost ownership remains explicit; traits and dynamic dispatch
  require real polymorphic consumers; pure work remains synchronous.
- `architecture/references/languages/spec-driven-development.md` and
  `architecture/references/languages/testing-workflows.md`: the cumulative
  candidate and recorded evidence remain subordinate to the approved revision.
- `architecture/wyrd-design.md`, `architecture/wyrd-security-posture.md`, and
  `architecture/bifrost-design.md`: accepted-job authority, exact Card pinning,
  gateway ownership, and Oracle/query settlement boundaries.
- Human standing direction: Wyrd uses established repository or conventional
  ecosystem mechanisms. Unsupported bespoke mechanisms, checks, files,
  settings, or options are DRIFT. The round's settled Oracle/follower and
  foreign-tenant harness decisions were treated as authority and not
  re-litigated.

## Changed-Surface Coverage

| Material surface | Changed symbols, callers, and proof inspected | Maintainer result |
|---|---|---|
| Accepted-run HTTP and composition | `workflow_runs_router`, three route handlers, `WorkflowRunHost::{create,get,cancel}`, `/v1` router composition, `AppState::workflows`, boot attachment, and `BoundServer` shutdown ordering | PASS. Handlers remain typed and thin; lifecycle work is discoverable on the dependency-owning host and run owner. |
| Process-local admission, idempotency, snapshots, retention, and shutdown | `WorkflowRuns`, `RunTable`, `Reservation`, `AcceptedRun`, preparation tracking, callers in `host.rs`, owner tests, and Scenarios 1, 2, and 6 | PASS. One cohesive concrete owner contains the state and invariants; the R1 tracker-token and tracked-blocking corrections do not add a parallel controller or workflow. |
| Graph pinning and Card reads | `GraphBounds`, `PinnedWorkflowGraph`, `active_row`, `get_card_by_ref_for`, registration resolver context, host preparation, and Cards/tool journeys | PASS. Existing Cards owners retain SQL and audited-read behavior; the pinned value is a bounded execution snapshot, not another registry or traversal service. |
| Skald prepare/execute and Agent tool hydration | `AgentTools`, `CardBodyResolver`, `Workflow::prepare`, `PreparedWorkflowRun`, executor transition observation, shared-client hydration, server hydration, and Skald tests | PASS. Pure preparation stays synchronous; execution remains on the existing executor; per-Agent tool lookup is the required variation across local and server environments, not a single-implementation trait. |
| In-process gateway adapter | `ServerWyrdGatewayCaller`, `NativeCall`, `Answer`, shared usage-bound helpers, `GatewayInvocation` callers, and Scenario 5 dialect/fallback/deadline/cancellation assertions | PASS. The adapter projects native requests into the existing governed invocation owner and introduces no transport, registry, policy path, or compatibility layer. |
| Built-in Workflow tools and shared bounded query collection | `RunTools`, `AgentRunTools`, `QueryTool`, `CardsTool`, `BoundedQuery`, `ResultCollector`, MCP caller, Cards caller, tool schemas, and Scenario 4 | PASS. The previous missing method rustdoc and broad output schemas are closed locally. MCP and Workflow share one collection owner instead of duplicating decoding and settlement. |
| Query open/cancel/settlement integration | `RunningQueryControls::open_cancellable`, `cancel_while_opening`, scheduled and bounded-query callers, current source comments, and recorded focused proof | PASS. The absolute deadline is passed through the existing control owner. No second lifecycle owner or fresh cleanup budget appears. |
| Oracle and Bifrost remediation | cumulative Oracle diff, query stream/resource owners, peer-cluster membership helper, fixture event time, forwarded Workflow journey, and server scheduled-query cleanup wait | PASS within the settled human decisions. The candidate leaves the bespoke graph-drain polling and idle refusal deleted; follower release remains grant-stream close, while tests wait on observable cleanup through an existing bounded condition pattern. |
| Configuration and test scheduling | `ServerWorkflowConfig`, validation/defaults, tenant-qualified external binding wrapper, state composition, nextest `pg-servers`, and their callers | PASS. Workflow fields are the approved bounded configuration table. The nextest group reuses the repository's existing test-group mechanism and the standard nextest concurrency control for a documented shared Postgres connection ceiling; it is not a new check or custom scheduler. |
| Revision 13 provider-tagged requests | `ProviderRequest`, RawV1 consumers, prompt binding, provider send sites, loader/examples/fixtures, Python accessors, generated JSON schemas, and round-trip/mismatch tests | PASS. Serde adjacent tagging is the conventional native mechanism; the hand-written discriminator, helper wire structs, and manual equality were deleted. Generated schema copies consistently project the tagged shape. |
| Journey and fixture structure | `pg_workflow_runs.rs`, Oracle `workflow.rs`, `PeerCluster`, gateway mock routing, foreign-tenant cases, Python state journey, and related fixture edits | PASS. The external test binaries earn their placement by driving live server/Postgres or multi-pod boundaries. R1 additions extend existing scenarios and fixture owners rather than adding one-off binaries or a parallel harness. |
| Documentation and declaration parity | rustdoc on changed production and test-support items, accepted-job design/security prose, ProviderRequest docs, generated schemas, and recorded codegen/typecheck evidence | PASS. The concrete tool declaration methods now document their provider-visible contracts; Revision 13 prose and generated schemas agree with the source enum. No hand-edited stub or new standalone authority file entered the implementation. |

## Material Findings

None.

The R1 maintainer blocker is closed: all eight concrete `AgentTool` declaration
methods now carry substantive rustdoc at
`crates/wyrd/wyrd-server/src/components/workflow/tools.rs:147-168` and
`:246-280`, including the closed input/output contracts and the Agent-space
default. The surrounding output-schema correction is also located on the
existing tool/query owners rather than in a generator or new abstraction.

## Drift and Simplicity Assessment

- The candidate deletes the hand-written `ProviderRequest` discriminator and
  uses serde's ordinary adjacent tagging.
- It reuses `TaskTracker`, `CancellationToken`, watch channels, existing Cards
  and gateway owners, the current query controls, standard JSON Schema tooling
  already present in the workspace, and nextest's native test-group facility.
- The R1 proof additions use existing test-support controllers, mock provider
  roots, condition waits, and peer-cluster composition. No new repository
  check, compatibility reader, transport surrogate, durable run store, actor,
  polling service, or production scheduling option was added.
- I found no changed mechanism, check, file type, setting class, or option that
  lacks both an established Wyrd precedent and a conventional comparable-project
  mechanism.

## Verification Evidence and Limits

Per the review instruction, no command that builds, tests, invokes Cargo, or
invokes mise was run. The review relied on the remediation task's recorded
successful focused scenarios and broader lanes, including Skald, Wyrd, shared,
principals, gateway, Bifrost, Python, TypeScript, codegen, boundary checks,
formatting, and lints. Source inspection confirmed that the named tests reach
the changed owners and assert the described contracts.

This static review does not independently rerun those commands. No source,
caller, declaration, or required maintainer-review input was unavailable.

## Calibration Notes

No uncertain preference is promoted to a finding. In particular, the large
production-shaped Workflow journey remains one integration-test binary to avoid
another heavy link target; its seven named scenarios and local helpers remain
navigable. The `pg-servers` cap is standard nextest resource coordination, not
a bespoke correctness mechanism. The approved fixed Workflow bounds are not
speculative tuning knobs. The human-approved follower cleanup wait and
foreign-tenant model limitation are recorded without requesting replacement
machinery.

## Overall Result

**PASS**

The cumulative candidate is maintainable under the applicable Wyrd authority:
owners and method boundaries are discoverable, documentation and generated
contracts are aligned, the R1 maintainer finding is closed, and no material
maintainer or unsupported-DRIFT finding remains.
