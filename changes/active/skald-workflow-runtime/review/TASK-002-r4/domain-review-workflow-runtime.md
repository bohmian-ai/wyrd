# Workflow runtime, lifecycle, and concurrency domain review

## Result and immutable subject

**PASS.** No material proposed findings in this domain.

Reviewed the complete cumulative range from base
`0569b79702218600c4f9790f45cc03100d5c6f1c` to candidate
`2ff7bbfa57737ce8a7a69287e2fa462c313b1a8c`. Both identities resolved before
and after source inspection. The candidate includes
`375d97e67f3affe0d5c59727ef3135b22a459140`; its Workflow-selector error
changes were reviewed as part of the cumulative source rather than treated as
pre-approved by the R3 task edit.

Authority applied: `AGENTS.md` §§2–12 and the async/runtime, PyO3, SDK, testing,
and completion rules; `architecture/agent-rules.md`; the spec-driven-development
and maintainer-style references; applicable Workflow, client/server, Skald, and
SDK sections of `architecture/wyrd-design.md` and `wyrd-doctrine.mdx`; approved
Revision 12; `TASK-002-cleanup`; R1/R2/R3 verdicts and the R2/R3 remediation
tasks. In particular this review applied REQ-001–003, REQ-013/013A/014,
REQ-024/025, REQ-040/052, REQ-054–059, INV-002/003/005/007/008/014, and
AC-001/003/006/013/029–031 where they govern authored/registered local loading
and execution. Later server-hosted run lifecycle, shared gateway preparation,
and production cancellation hosting remain TASK-003–005 concerns and were not
made acceptance conditions here.

No `.codegraph/` directory exists, so navigation used the explicit Git range,
`git show` at the candidate, `rg`, callers, and tests. No peer conclusion was
used as proof.

## Boundary and source coverage

| Boundary | Producer-to-consumer trace and source evidence | Result |
|---|---|---|
| Authored filesystem loading | `wyrd_client::Workflow::from_path` copies the caller path into owned `PathBuf`, sends `wyrd_loader::load` plus entry canonicalization through `tokio::task::spawn_blocking`, and awaits its closed result before constructing `WorkflowBodies` (`crates/shared/wyrd-client/src/workflow.rs:57–73,99–121`). Loader parsing, sandbox resolution, validation, and ordering remain synchronous under their existing owner (`wyrd-loader/src/lib.rs:71–145`). | PASS |
| Lazy registered reads | `WorkflowBodies::external_refs` follows local sibling bodies synchronously and stops at external `Ref`s; only a nonempty result constructs ambient `Cards` and enters the existing exact graph traversal (`workflow.rs:65–72`; `cards/hydrate/workflow.rs:76–102,165–181`). Fully local bundles therefore require neither client construction nor credentials. | PASS |
| Provenance and complete publication | Authored siblings and registered envelopes remain in separate stores. `body` serves `Sibling` only from the sibling map and `Ref` only from registered Cards, including a supplied UID assertion (`cards/hydrate/workflow.rs:45–73,139–162`). Hydration occurs only after every read and Active check succeeds; all intermediate values are request-local and no `Workflow` is returned early. | PASS |
| Exact registered graph | `WorkflowCards::load` rejects wrong-kind and versionless named selectors before reads, including the catalog correction from commit `375d97e67`; `load_workflow` resolves the root and UID-bearing Agent/Prompt closure in `GraphScope::Runtime`, checks every Card Active, then hydrates (`workflow.rs:148–191`; `cards/hydrate/graph.rs:38–57,311–412`; `cards/hydrate/workflow.rs:183–216`). Runtime scope performs no artifact-inventory or filesystem publication work. | PASS |
| Traversal concurrency and cancellation | `GraphTraversal` owns one pending stack, state/alias maps, preloaded responses, and result nodes for one call. Reads are awaited sequentially with no detached per-node tasks, unbounded fan-out, shared cache, or lock held across provider IO (`cards/hydrate/graph.rs:96–149,162–250,311–370`). Dropping a load stops further polling; completed reads and an already-started blocking filesystem job may finish, but there is no durable or partially published Workflow state. | PASS |
| Skald lowering and binding | The new `CardBodyResolver` adapts exact environment-owned bodies to the existing `AgentResolver` and `PromptResolver`. `Workflow::from_card_bodies` delegates to `from_card_with_agent_resolver` and existing `validate`; registration uses the separate `validate_card_bodies`, clears tool names only on temporary validation values, binds no execution tool, and performs no IO (`skald-workflow/src/bodies.rs:28–121,124–222`; `workflow_surface.rs:360–439`). Prompt interpolation remains in the existing Prompt/Agent construction path. | PASS |
| Provider and tool dispatch | Loading and registration invoke no provider. Executable hydration resolves declared Agent tools through the existing process/caller registry, and `Workflow::run` delegates to the unchanged Skald runtime; route suitability is checked before dispatch. The Rust journey uses `as_skald().run_with_options` with a controlled gateway, while Python/TypeScript correctly prove the unconfigured `wyrd_gateway` refusal before a step starts. No Workflow-level provider branch or second tool loop was added. | PASS |
| Python ownership | `PyWorkflow::from_path` and `PyWorkflowCards::load` release the GIL with `Python::detach`, then use the shared Wyrd runtime to await the Rust facade (`sdks/wyrd-sdk-python/src/workflow.rs:501–508`; `src/state/mod.rs:2557–2639`). Inputs are converted before detachment, no `Bound` value crosses an await, and returned Skald ownership is moved back into `PyWorkflow`. Synchronous Python `run` uses the same established bridge. | PASS |
| Node ownership | `NativeWorkflow` owns the shared Rust `Workflow`; napi async `from_path`, registered load, and `run` await the Rust owners and project closed native error/result values (`sdks/wyrd-sdk-ts/native/src/workflow.rs:21–151`; `native/src/cards.rs`). The TypeScript wrapper adds only selector serialization, promise/error projection, and result typing (`wyrd/src/index.ts:1195–1243,1325–1374`). There is no JS graph, validator, executor, or transport. | PASS |
| Public behavioral proof | The Rust journey covers local and mixed authored runs, ambient missing/denied reads, sibling/external collision, deleted dependency, exact/UID registered load, post-v2 pinning, stable selector errors, and controlled provider results (`sdks/wyrd-sdk-rust/tests/workflow_loading.rs:266–477`). Python and TypeScript cover the equivalent public paths and full exact Workflow→Agent→Prompt relationships (`test_cards_crud.py:482–653`; `workflow-loading.test.ts:71–210`). The TypeScript test's `60_000` per-test budget matches the established `startTestServer` integration convention in `cards-state`, `gateway-admin`, `operator-connections`, and `verification-run`; it is not a bespoke runtime mechanism. | PASS |

## Failure, cancellation, and recovery assessment

- Loader parse, entry canonicalization, malformed graph, missing credential,
  denied/missing/deleted dependency, inactive Card, wrong UID, wrong selector,
  missing tool, and route mismatch all return before provider dispatch and
  before a complete Workflow is published. The operation owns no durable write
  to roll back.
- `spawn_blocking` is the ordinary Tokio boundary already used in the
  repository and in comparable async Rust systems for synchronous filesystem
  libraries. Runtime shutdown or a panicking blocking job maps through the
  derive-backed Workflow internal error; abandonment cannot synchronously stop
  an operating-system file read, and the rustdoc states that standard limit
  without promising a nonstandard cancellable filesystem mechanism.
- Registry reads use the existing transport's timeout/retry behavior. This
  candidate adds no retry owner, background loader, task registry, cache, or
  shared mutable graph. A registry outage blocks/refuses only the affected
  load; fully local loading and already hydrated workflows remain independent.
- Python loading is intentionally synchronous to Python callers while the GIL
  is detached. Node promises own native async work through napi's normal
  object/future lifetime. An abandoned result can leave already completed
  reads or blocking work, but neither boundary can expose a partial Workflow or
  write registry state.
- Once a complete Workflow is returned, provider failure, retry, timeout,
  explicit cancellation, total-deadline handling, peer draining, and terminal
  `WorkflowRun` projection remain owned by the unchanged Skald executor and run
  ledger. TASK-002 introduces no server job owner or restart-resume claim.

## Prior remediation closure inspected

- **FIND-TASK-002-11 / commit `375d97e67`:** wrong-kind and versionless shared
  selectors plus malformed Python/TypeScript Workflow selector inputs now use
  `WorkflowInvalidCardRef`; canonical wrapper projection preserves its catalog
  metadata. The cumulative server refusal table expects the same code. The R3
  record deliberately retains the existing `^1.0.0` server-read behavior; this
  review does not reopen that human-accepted observation as a new mechanism.
- **FIND-TASK-002-13:** closed at the shared owner. The loader remains
  synchronous and is offloaded once with the installed Tokio blocking pool;
  neither Python nor Node adds a compensating pool or SDK-specific filesystem
  path. Entry canonicalization moved into the same blocking closure.
- **FIND-TASK-002-7 runtime slice:** each language journey now checks all three
  exact Agent/Prompt pairs and still executes the pinned Workflow after a v2
  registration. These assertions observe both stored references and resulting
  provider-shaped output, so output-only success cannot conceal relationship
  drift.

## Drift and over-engineering audit

No domain finding is proposed under the standing direction. The candidate uses
the standard Tokio blocking pool, the existing loader, Cards graph traversal,
Skald resolver/executor, Wyrd runtime bridge, and napi async projection. It
does not add a dedicated pool, cancellation shim, filesystem supervisor,
registry cache, alternate parser, graph store, executor, runtime option, or
new check. The 60-second TypeScript integration timeout is an established
repository convention for tests that start a real server, not a new product or
runtime option. Requiring any of those absent mechanisms would be unsupported
DRIFT and is explicitly rejected.

## Evidence limits

This was a source-only independent domain review as assigned. No build, test,
database, Python, Node, or live-provider command was run. I inspected the
candidate's test bodies and the R3 implementation record reporting the focused
loader/client tests, Rust/Python/TypeScript journeys, server registration tests,
workspace-hack, format/lint/type/codegen, and relevant boundary checks passing;
those reports are evidence available to the orchestrator, not independently
rerun logs from this reviewer.

The journeys use deterministic mock providers and repository-managed local
services. They do not qualify live-provider outages, process-crash recovery,
or future server-hosted run cancellation, none of which TASK-002 claims. There
is no scheduler-starvation load test for `spawn_blocking`; source placement and
the normal integration journeys are proportionate proof for this standard
boundary, and the repository prohibits synthetic host-load reproduction.

Proposed finding ledger: **empty**. Overall sensitive-domain result: **PASS**.
