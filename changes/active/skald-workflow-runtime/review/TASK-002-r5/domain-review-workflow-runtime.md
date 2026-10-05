# Workflow runtime and lifecycle domain review

## Result and immutable subject

**PASS.** The proposed finding ledger is empty.

Reviewed the complete cumulative range from base
`0569b79702218600c4f9790f45cc03100d5c6f1c` to candidate
`2d669917c03699876b3c8926f0de5ac88c578c01`. The candidate identity matched
`HEAD` before and after source inspection and focused verification.

Authority applied: `AGENTS.md` async/runtime, PyO3, SDK, testing, and completion
rules; `architecture/agent-rules.md`; the applicable Workflow, client/server,
Skald, and SDK sections of `architecture/wyrd-design.md` and
`architecture/wyrd-doctrine.mdx`; the Rust, PyO3, TypeScript, errors, testing,
maintainer-style, and spec-driven-development references; approved Revision
12; `TASK-002-cleanup.md`; and the R2, R3, and R4 remediation tasks as
hypotheses to recheck against current source. No `.codegraph/` directory
exists, so navigation used the immutable Git range, `rg`, current source,
callers, and tests. Earlier review conclusions were not accepted as proof.

This domain covers TASK-002 authored and registered loading, exact graph
hydration, Skald validation/binding/execution, cancellation and partial-progress
behavior, and the Python and Node runtime boundaries. Server-hosted job
recovery, shared gateway preparation, and later all-route execution remain the
approved scope of TASK-003 through TASK-005 and were not added as acceptance
conditions here.

## Boundary and source coverage

| Boundary | Producer-to-consumer trace and evidence | Result |
|---|---|---|
| Blocking filesystem load | `wyrd_client::Workflow::from_path` owns the caller path, runs the existing synchronous `wyrd_loader::load` and entry canonicalization together in `tokio::task::spawn_blocking`, awaits the completed result, and only then constructs `WorkflowBodies` (`crates/shared/wyrd-client/src/workflow.rs:57-73,99-121`). No polling thread performs filesystem IO, and no SDK adds a pool or loader. | PASS |
| Lazy Cards creation | `WorkflowBodies::external_refs` follows authored sibling bodies synchronously and returns only external `Ref`s. `Workflow::from_path` constructs ambient `Cards` only when that list is nonempty (`workflow.rs:65-72`; `cards/hydrate/workflow.rs:76-102`). Fully local loading therefore needs no client or credential. | PASS |
| Provenance and publication | Authored sibling specs and registered Cards remain separate stores. `WorkflowBodies::body` serves a `Sibling` only from the sibling map and a `Ref` only from registered Cards, including a supplied UID check; hydration occurs only after all reads and Active checks complete (`cards/hydrate/workflow.rs:27-34,104-162`). No partially hydrated `Workflow` escapes. | PASS |
| Exact registered graph | `WorkflowCards::load` rejects versionless or wrong-kind selectors before IO and delegates to the existing `CardGraphHydrator`. Runtime traversal follows only exact UID-bearing Agent/Prompt relationships, checks response identity and Active status, and performs no artifact inventory or disk publication (`workflow.rs:148-191`; `cards/hydrate/graph.rs:38-57,96-149,207-250,311-412`; `cards/hydrate/workflow.rs:183-216`). | PASS |
| Traversal cancellation | One call owns one depth-first `GraphTraversal`; reads are awaited sequentially and there are no detached node tasks, shared graph cache, locks across IO, or unbounded fan-out. Dropping the future stops later reads; already completed reads or an already-started blocking file load may finish, but they mutate no durable state and cannot publish a partial Workflow (`cards/hydrate/graph.rs:96-149,162-250,311-370`). | PASS |
| Skald hydration and registration validation | `CardBodyResolver` adapts environment-owned exact bodies to the existing Agent and Prompt resolver seams. Runnable hydration binds declared tools through the existing resolver; declarative registration validation clears tool names only on temporary values and performs no provider, secret, registry, or filesystem IO (`crates/skald/skald-workflow/src/bodies.rs:58-121,124-222`; `workflow_surface.rs:360-439`). No second executor or validation graph was introduced. | PASS |
| Dispatch and refusal | Loading and registration never dispatch. `Workflow::run` delegates to the unchanged Skald runtime, while explicit execution dependencies remain available through `as_skald`. Invalid graphs, missing tools, and route mismatch refuse before dispatch; step failures remain terminal run data under the existing executor contract (`wyrd-client/src/workflow.rs:76-95`). | PASS |
| Python runtime/GIL | `PyWorkflow::from_path`, `PyWorkflowCards::load`, and synchronous `run` convert inputs before `Python::detach`, use the shared `wyrd_runtime` bridge, and reacquire Python only after the Rust operation completes (`sdks/wyrd-sdk-python/src/workflow.rs:472-508,528-550`; `src/state/mod.rs:2557-2639`). No `Bound` value crosses an await and no ad hoc Tokio runtime exists. | PASS |
| Node async lifetime | `NativeWorkflow` owns the shared Rust Workflow. Napi `from_path`, registered loading, and run await existing Rust owners and return closed native result/error objects; the TypeScript layer adds only promise, selector, and typed result projection (`sdks/wyrd-sdk-ts/native/src/workflow.rs:21-160`; `native/src/cards.rs:153-172`). Abandoning a promise may leave completed reads, but exposes no partial handle or durable mutation. | PASS |
| R4 exact-version invariant | `VersionBlock` now deserializes a string through its established `parse` constructor, so ranges and partials cannot inhabit the exact-version type (`crates/shared/wyrd-semver/src/block.rs:12-43,145-166`). The TypeScript native selector retains raw strings only until the existing domain constructors validate `uid`, `space`, `name`, and `version`, mapping failures to the existing Workflow error and precise field (`sdks/wyrd-sdk-ts/native/src/workflow.rs:53-107`). This is ordinary validated-newtype and FFI-boundary conversion, not a new validator or runtime mechanism. | PASS |
| Public proof | Rust, Python, and TypeScript journeys cover local, mixed, collision, credential refusal, deleted dependency, registered exact/UID loads, exact Workflow-to-Agent-to-Prompt relationships, post-v2 pinning, and native execution. The extended TypeScript journey uses an underprivileged client to demonstrate malformed selectors refuse before registry authorization (`sdks/wyrd-sdk-ts/wyrd/tests/integration/workflow-loading.test.ts:71-229`). | PASS |

## Failure, cancellation, and recovery assessment

- Loader errors, malformed selectors, missing credentials, denied/missing or
  inactive Cards, exact-identity mismatch, invalid resolved graphs, unavailable
  bindings, and route mismatches all return without durable writes. The complete
  Workflow value is the publication boundary.
- Tokio's installed blocking pool is the standard repository and ecosystem
  mechanism for this bounded synchronous filesystem library. Runtime shutdown
  or blocking-task panic maps to the derive-backed Workflow internal error.
  Requiring a dedicated pool, cancellable filesystem shim, or task supervisor
  would add unsupported machinery and is DRIFT under the standing direction.
- Registry reads retain the existing transport timeout and authorization path.
  The candidate adds no retry owner, background loader, cache, or shared mutable
  traversal. A registry outage affects only loads that need external Cards;
  fully local loads and already hydrated Workflows remain independent.
- Python intentionally presents a synchronous call while detached from the GIL;
  Node presents an ordinary promise backed by napi-owned Rust state. Neither
  language layer duplicates graph, validation, execution, or lifecycle logic.
- Provider retry, explicit run cancellation, deadlines, and terminal ledger
  state remain owned by the pre-existing Skald executor. TASK-002 makes no new
  process-crash, server-resume, or durable-run claim.

## Prior remediation closure

- R2/R3 runtime proof remains closed: all three language journeys execute the
  same Native runtime and inspect exact registered dependency identities and
  relationships, rather than treating output text alone as pinning proof.
- The blocking-load correction remains at the shared owner; both SDK runtime
  boundaries consume it without compensating pools or filesystem paths.
- R4 closes the remaining selector inconsistency at its producers. Invalid
  `VersionBlock` deserialization is impossible globally, while TypeScript maps
  malformed individual fields locally before registry IO. Valid exact,
  prerelease, and build-metadata versions preserve their serialized form.

## Drift and over-engineering audit

No unsupported mechanism, check, file, setting, or option was introduced in
the runtime/lifecycle boundary. The added `wyrd-semver` dependency is the
existing owner of `VersionBlock`, explicitly intended for direct import, and
the manual `Deserialize` implementation is the conventional Rust newtype
invariant. The TypeScript integration test extends the existing journey and its
established real-server timeout; it does not add a harness or gate. A dedicated
loader runtime, cancellation shim, selector framework, graph store, registry
cache, SDK executor, feature flag, or lexical check would be DRIFT and is not
required.

## Verification and limits

Independently run against the immutable candidate:

```text
mise exec -- cargo nextest run --locked -p wyrd-semver --lib \
  -E 'test(=block::tests::serde_preserves_the_exact_version_invariant)'
PASS: 1 selected, 1 passed, 78 skipped
```

`git diff --check` for the complete base-to-candidate range passed during this
review. The R4 implementation evidence records passing Rust, Python, and
TypeScript Workflow journeys; shared/SDK tests; TypeScript type/native checks;
code generation; client-tier boundaries; format; and lints. I inspected those
test bodies and current source but did not independently rerun the database,
Python, or Node integration lanes. Live-provider outages, process-crash
recovery, and future server-hosted run cancellation are outside TASK-002 and
are not concealed as verification limits for an otherwise required behavior.

## Proposed findings

None.

**Overall sensitive-domain result: PASS.**
