# Maintainer review — TASK-002-r4

Result: **PASS**

## Subject and authority

Reviewed the complete cumulative diff from base
`0569b79702218600c4f9790f45cc03100d5c6f1c` to candidate
`2ff7bbfa57737ce8a7a69287e2fa462c313b1a8c`, including
`375d97e67f3affe0d5c59727ef3135b22a459140`. The latter remains part of the
reviewed source even though the human removed FIND-TASK-002-11 from the R3
remediation task. `HEAD` equalled the candidate when this report was written.
The repository has no `.codegraph/` directory, so source navigation used the
immutable Git objects and repository search.

Applied `AGENTS.md` ownership, struct-centered Rust, async, PyO3, public SDK,
testing, documentation, generated-artifact, and simplicity rules;
`architecture/agent-rules.md`; the Workflow, client, registration, and authored
reference authority in `architecture/wyrd-design.md` and
`architecture/wyrd-doctrine.mdx`; `maintainer-style.md` and
`spec-driven-development.md`; approved Revision 12; `TASK-002-cleanup.md`; the
R2 and R3 remediation tasks; and prior finding history as hypotheses rather
than conclusions.

## Changed-surface coverage

| Surface | Owners, callers, tests, and declarations inspected | Maintainer assessment |
|---|---|---|
| Authored loading | `wyrd_loader::{load, validate_card}`, canonical `ReferenceSlotVisitor`, `InlineableRef::to_durable`, code-review bundle, loader test, and `Workflow::from_path` callers | Parsing and sandboxing stay with the existing loader. Pure Workflow validation is a synchronous extension of that owner. No keyed-form normalizer, second parser, slot inventory, or compatibility dialect remains. |
| Shared Workflow facade | `wyrd_client::Workflow::{from_path,run,as_skald,into_skald}`, `load_bundle`, `WorkflowCards::load`, `Cards::workflow`, Rust/Python/Node projections, and focused client test | The facade exists only because client IO cannot be attached to Skald's foreign type. Filesystem work is isolated through Tokio's installed blocking pool, while graph reads remain async. Names, result types, cancellation/partial-progress notes, and error documentation expose the operation clearly. |
| Card graph composition | `CardGraphHydrator`, `GraphTraversal`, `GraphScope`, `resolve_graph`, `resolve_refs`, `load_root`, `resolve_root_selector`, `WorkflowBodies::{authored,external_refs,extend_registered,hydrate,body}`, bundle hydration and Workflow callers | One existing dependency-owning hydrator now owns every graph entry. The existing traversal retains alias, cycle, exact-identity, UID, and read invariants. `GraphScope` has two concrete consumers and expresses the real bundle/runtime difference; `WorkflowBodies` is a per-load provenance adapter, not another traversal or cache. |
| Skald lowering | `card_body_dependencies`, `Workflow::{from_card_bodies,validate_card_bodies}`, `CardBodyResolver`, existing `from_card_with_agent_resolver`, Prompt/Agent resolvers, validation and execution callers | The synchronous adapter fills existing resolver seams and leaves runtime construction, Prompt binding, resolved validation, and execution in Skald. Its state and methods are cohesive; it introduces no executor, provider registry, or filesystem/registry dependency. |
| Registration preflight | `EffectiveSpecs::{resolve,new,validate_bindings,validate_baselines,validate_workflows,body,load}`, `graph_ready_submissions`, `resolve_external`, and Workflow registration journeys | Preflight is discoverable through the existing effective-spec owner. Separate sibling/external maps make provenance explicit, while tenant reads remain on the borrowed `TenantConn`. Graph-only holders reuse the established version-intent mechanism and do not mutate authored submissions or create a parallel registration plan. |
| Atomic write and UID fence | `RegistrationWriter::write`, `recheck_active_card_refs`, `persist_node`, service caller, SQL race test and server replacement journey | The dependency-owning writer carries server state and caller once; the pure plan remains a value. The SQL owner accepts the already-resolved `(CardRef, CardUid)` pairs and documents locks, cancellation, errors, and transaction ownership. No trait, repository layer, or second transaction abstraction was added. |
| Rust SDK proof | Thin `wyrd_sdk` re-exports; `workflow_loading` helpers, ambient child, exact relationship assertions, selector refusals, and real-server journey | The SDK remains implementation-free. Test helpers name the scenario and reduce repeated receipt/envelope inspection. Process isolation for ambient environment variables follows an existing repository precedent and the conventional solution for process-global environment mutation; it is not a product mechanism or a new harness. |
| Python boundary | `PyWorkflow::from_path`, `PyCards::workflow`, `PyWorkflowCards::load`, shared selector parsers and `invalid_selector`, public package exports, generated agent/cards stubs, unit and integration callers | Wrappers convert typed inputs, release the GIL through the shared runtime, and delegate to the shared client. Generic selector errors are produced once at the generic selector owner; Workflow-specific selector errors stay on the Workflow view. Public signatures, docs, exports, overloads, and generated declarations agree. The journey's supplemental HTTP envelope reads reuse the existing public route because Python intentionally has no generic envelope-get projection; they add no SDK transport. |
| TypeScript/Node boundary | `NativeWorkflow`, `NativeWorkflowLoad`, `parse_workflow_selector`, `NativeCards::load_workflow`, public `Workflow`, `WorkflowCards`, selector/JSON/run types, generated napi declarations, unit and integration tests | Native code owns only string conversion and async handle projection. The public discriminated selector rejects mixed shapes; recursive `JsonValue` and the closed run/step/error interfaces match the Rust wire snapshot. Generated declarations match the native source. No JavaScript executor, graph, transport, or validator was introduced. |
| Contract and generated state | `WyrdError::WorkflowInvalidCardRef`, error projections, `CardRef` conversion helper, generated Python/TypeScript declarations and error-code list, workspace-hack feature union | The Workflow selector error uses the existing derive-backed catalog and canonical foreign-runtime projections rather than a parallel error scheme. `to_durable` centralizes an existing reference conversion and preserves provenance. Hakari regeneration updates the repository's established generated feature union; it adds no setting, option, or check. |
| Tests, examples, fixtures, and docs | Server Postgres target, three SDK journeys, selector unit/type checks, checked-in Workflow fixtures/example, docs and README consumers | External tests earn their files by driving Postgres or a real server/runtime. Assertions observe durable envelopes, relationships, exact identities, refusals, and native outputs. The broad journeys are long because they prove required cross-boundary scenarios; helpers keep their setup and graph comparisons findable. Examples use the same loader/runtime contract. |
| Workflow-process artifacts | Changed architecture, skills, task/spec packet, prior review records and generated skill mirror | Current authority consistently names the same owners and prohibits replacement machinery. The canonical/mirrored skill arrangement and sync check predate this task; this diff does not add a task-specific controller, symbol allowlist, lexical ban, or review-only production mechanism. |

## Prior-remediation and `375d97e67` closure

| Item | Source assessment |
|---|---|
| Prior graph-owner finding | Closed. All selected-root and authored-ref graph IO entrypoints are inherent methods on `CardGraphHydrator`; callers no longer extract and thread `RegistryEngine` into free workflow functions. |
| Prior TypeScript contract finding | Closed. `JsonValue`, run/step statuses, full step results, details and remediation are explicit public types, with declaration and compile-time coverage. |
| FIND-TASK-002-7 | The strengthened Rust, Python, and TypeScript journeys reuse their existing clients/routes and compare complete exact spec refs and outbound relationships. The added helpers do not create a second registry API or fixture framework. |
| FIND-TASK-002-12 | `invalid_selector` is the one generic Python request-validation constructor used by the shared identity parsers. Kind-specific body validation remains separate. |
| FIND-TASK-002-13 | `load_bundle` is a narrow synchronous filesystem helper owned beside `Workflow::from_path`; `spawn_blocking` is the installed runtime's standard boundary, with no pool, service, or option added. |
| FIND-TASK-002-14/-15 | The established Hakari output is regenerated and the prior report's trailing whitespace is removed. No check was weakened, duplicated, exempted, or added. The explicit cumulative `git diff --check` completed successfully during this source review. |
| Removed FIND-TASK-002-11 / commit `375d97e67` | Reviewed. `WorkflowInvalidCardRef` lives in the canonical error catalog; the shared Rust view and the Python/Node selector-conversion owners emit it, and generated/public declarations project it. This is one established error mechanism, not a wrapper-specific exception hierarchy or validation layer. |

## Existing-owner and industry-standard check

The cumulative candidate stops at the first established mechanism for each
changed concern: the existing loader and canonical visitor for authored input,
the existing Cards hydrator and traversal for graph reads, Skald's existing
resolver/executor seams, `EffectiveSpecs` and `RegistrationWriter` for server
orchestration, `TenantConn` and existing SQL queries for persistence, Tokio's
blocking pool for synchronous filesystem work, the derive-backed error catalog
for stable errors, and Hakari for the generated Cargo feature union. The SDK
wrappers are required foreign-runtime boundaries and contain conversions only.

No changed mechanism, check, file, setting, or option was found that lacks both
an established Wyrd owner and a conventional counterpart in comparable Rust,
Python, TypeScript, Cargo, or integration-test practice. In particular, the
candidate adds no bespoke scanner, lexical ban, controller, cache, transport,
parser dialect, compatibility alias, runtime, retry setting, or user-facing
configuration knob. No DRIFT finding is warranted under the human standing
direction.

## Material findings

None.

## Calibration notes

- `pg_workflow_registration.rs` and the three language journeys are sizeable,
  but they each drive the required real cross-boundary matrix. Splitting them
  would create additional heavy test targets or duplicate setup without making
  the scenario safer to change.
- `WorkflowBodies` uses a linear registered-body lookup. The graph is bounded
  to one Workflow's Agent/Prompt closure, and replacing it with another index
  would be a speculative optimization with no demonstrated maintenance value.
- The TypeScript journey narrows the generic envelope's relationship payload
  locally for assertions. That cast does not widen the exported SDK contract
  or justify a new public Workflow-only envelope type.

These are not findings and require no remediation.

## Verification and limits

This was a read-only maintainer audit. No build, test, code-generation, or
runtime lane was executed. The candidate's R3 implementation evidence records
passing focused loader/client/server/SQL and three-language journey commands,
selector/type checks, workspace-hack, codegen, format/lint, boundary, and
cumulative hygiene checks; those remain supplied evidence for the orchestrator
to reconcile, not independent execution by this reviewer. I did run the static
`git diff --check BASE..CANDIDATE`, which exited successfully.

Candidate immutability and the completeness of every other required independent
report remain orchestration gates; they do not limit this report's complete
maintainer source coverage.

## Overall result

**PASS** — all materially changed symbols, their owning modules, reachable
callers, relevant tests, public documentation, and generated declarations are
maintainable under current Wyrd authority. The cumulative candidate uses
existing owners and conventional mechanisms, and no material layout, owner,
method-shape, naming/type, documentation, test-clarity, declaration-parity, or
unsupported-mechanism finding remains.
