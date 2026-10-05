# Maintainer review — TASK-002-r5

Result: **PASS**

## Immutable subject and authority

Reviewed the complete cumulative range from base
`0569b79702218600c4f9790f45cc03100d5c6f1c` to candidate
`2d669917c03699876b3c8926f0de5ac88c578c01`, including the original
`TASK-002-cleanup.md` and the R2, R3, and R4 remediation tasks. `HEAD` still
equalled the candidate after source inspection and before this report was
written. The repository has no `.codegraph/` directory, so navigation used
the immutable Git objects and repository search.

Applied `AGENTS.md` ownership, struct-centered Rust, async, PyO3, SDK,
testing, generated-artifact, documentation, and simplicity rules;
`architecture/agent-rules.md`; the Workflow authoring, client, registry, and
runtime authority in `architecture/wyrd-design.md` and
`architecture/wyrd-doctrine.mdx`; and the applicable architecture patterns,
Rust, PyO3, Python/stubs, TypeScript, testing, errors, maintainer-style, and
spec-driven-development references. Prior reports and findings were treated
as hypotheses and checked against the cumulative candidate.

## Changed-surface coverage

| Surface | Owners, callers, tests, and declarations inspected | Maintainer assessment |
|---|---|---|
| Authored loading | `wyrd_loader::{load,validate_card}`, canonical `ReferenceSlotVisitor`, `InlineableRef::to_durable`, checked-in Workflow bundles, loader tests, and `Workflow::from_path` callers | Parsing, sandboxing, and reference discovery remain on the existing loader/visitor. Pure Workflow validation is synchronous. No replacement parser, keyed dialect, compatibility path, or duplicate slot inventory remains. |
| Shared Workflow facade | `wyrd_client::Workflow::{from_path,run,as_skald,into_skald}`, `load_bundle`, `WorkflowCards::load`, `Cards::workflow`, and Rust/Python/Node callers | The facade is the narrow client-IO owner Rust needs around the foreign Skald type. Filesystem IO uses Tokio's installed blocking pool; graph reads remain async. Names, errors, and cancellation/partial-progress documentation expose the workflow without a second runtime or transport. |
| Card graph composition | `CardGraphHydrator`, `GraphTraversal`, `GraphScope`, `resolve_graph`, `resolve_refs`, root selection, `WorkflowBodies::{authored,external_refs,extend_registered,hydrate,body}`, Service hydration callers, and Workflow tests | The existing hydrator owns all registry traversal. `GraphScope` expresses two real consumers (bundle publication and runtime composition), and `WorkflowBodies` holds per-load provenance rather than introducing another graph store, traversal, cache, or lifecycle owner. |
| Skald lowering | `card_body_dependencies`, `Workflow::{from_card_bodies,validate_card_bodies}`, `CardBodyResolver`, `from_card_with_agent_resolver`, Prompt/Agent resolvers, runtime validation, and execution callers | The new synchronous adapter uses Skald's existing resolver and validation seams. It owns no IO, executor, provider registry, or persistent state; its fields share one hydration responsibility. |
| Server preflight and write | `EffectiveSpecs::{resolve,new,validate_bindings,validate_baselines,validate_workflows,body,load}`, `graph_ready_submissions`, `RegistrationWriter::write`, `persist_node`, `recheck_active_card_refs`, server callers, and Postgres regressions | Existing preflight and write owners retain their responsibilities. Provenance caches are explicit, SQL stays behind `TenantConn`, and the exact-UID recheck remains in the caller-owned transaction. No repository layer, transaction wrapper, or duplicate registration plan was added. |
| Rust SDK | Thin `wyrd_sdk` exports, `workflow_loading` journey helpers, exact graph assertions, selector refusals, and real-server execution | The SDK remains implementation-free. The external journey target is earned by the real server/runtime boundary; helpers consolidate repeated envelope assertions rather than create a fixture framework. |
| Python boundary | `PyWorkflow::from_path`, `PyCards::workflow`, `PyWorkflowCards::load`, shared identity parsers, public exports, generated stubs, unit tests, and integration journey | PyO3 converts inputs, releases the GIL through the shared runtime, and delegates to `wyrd-client`. Workflow-specific errors stay on the Workflow boundary; shared registry selector errors stay at their existing generic owner. Public docs, signatures, exports, and generated declarations align. |
| TypeScript/Node boundary | `NativeWorkflow`, `NativeWorkflowLoad`, `WorkflowSelectorJson`, `parse_workflow_selector`, `NativeCards::load_workflow`, public `Workflow`/`WorkflowCards`, run types, napi declarations, and unit/integration tests | The boundary parses raw strings into existing domain types so each malformed field can be named, then delegates to the shared client. This is the standard foreign-runtime conversion pattern already used by the Python SDK, not a second validator. The public selector union and generated declarations remain aligned. |
| Version invariant and errors | `VersionBlock::{parse,from_str,deserialize}`, all located `VersionBlock` consumers, `WorkflowInvalidCardRef`, Python/Node projections, server refusal test, and exact-version unit proof | Manual Serde deserialization now routes through the newtype's established parser, preventing an invalid exact-version value at its producer. The R4 correction removes the prior consumer-specific inconsistency without adding a downstream guard or parallel error catalog. Serialization and schema shape remain unchanged. |
| Contracts, docs, generated state, examples, and process artifacts | Error-code projections, Python/TypeScript generated declarations, Hakari output, example/fixture bundles, architecture/spec/task updates, canonical/mirrored workflow skills, and their existing sync convention | Generated outputs are derived through established repository mechanisms. The cumulative range adds no task-specific gate, scanner, allowlist, controller, feature flag, runtime setting, or user-facing option. Skill mirroring and its sync check predate this task. |
| Verification shape | In-module loader/client tests, SQL/server integration targets, three SDK journeys, selector/type tests, R4 focused proof, and recorded aggregate lanes | Test placement follows the runtime boundary: pure invariants are unit-tested, Postgres/server behavior uses external targets, and each first-class SDK has a real journey. R4 extends the existing TypeScript journey instead of adding a new harness. |

## R4 closure and existing-owner check

| R4 obligation | Source assessment |
|---|---|
| Exact-version construction | Closed at `VersionBlock` itself. `Deserialize` reads the ordinary string wire form and calls `VersionBlock::parse`; valid exact, prerelease, and build forms round-trip, while ranges and partials cannot inhabit the newtype. This is conventional validated-newtype Serde and the smallest shared correction. |
| TypeScript field-specific refusal | Closed at the existing Node selector boundary. `WorkflowSelectorJson` deliberately keeps boundary strings raw, and `parse_workflow_selector` uses existing `CardUid`, `SpaceName`, `CardName`, and `VersionBlock` constructors. Shape failures retain `field = selector`; value failures name their actual field. |
| Server proof | The test no longer manufactures an invalid `VersionBlock` and expects a Registry-family error. It statically proves ranged `CardRef` decoding fails while retaining the server's genuine wrong-kind and read-path refusals. |
| Dependency and declarations | The Node crate's direct `wyrd-semver` edge is earned by boundary construction and matches the existing Python SDK pattern. It adds an already-installed workspace crate, no new package or feature. Cargo lock state and napi/public declarations remain consistent. |

The cumulative candidate stops at the first established mechanism for each
concern: loader and canonical visitor, Cards hydrator/traversal, Skald
resolver/runtime, `EffectiveSpecs`/`RegistrationWriter`, `TenantConn`, domain
newtype constructors, Tokio's blocking pool, the derive-backed error catalog,
foreign-runtime wrappers, generated declarations, and Hakari. No changed
mechanism, check, file, setting, or option lacks both an established Wyrd
owner and a conventional counterpart in comparable Rust, Python, TypeScript,
Cargo, or integration-test practice. No DRIFT finding is warranted under the
human standing direction.

## Material findings

None.

## Calibration notes

- `pg_workflow_registration.rs` and the three language journeys are large,
  but each drives a required real boundary and reuses its existing fixture.
  Splitting them would add heavy targets or duplicate server setup without
  reducing maintenance risk.
- The Node and Python wrappers each perform field-to-domain conversion because
  that work belongs at their separate foreign-runtime boundaries. Moving it
  into another shared parser would add indirection while weakening precise
  boundary errors.
- The R4 server assertion checks typed decode rather than making a registry
  request because the invalid state is now rejected before a `CardRef` exists.
  This is a direct invariant test, not a replacement server-validation layer.

These are nonblocking calibration observations, not findings.

## Verification and limits

This was a read-only maintainer audit; no build, test, code-generation, or
runtime lane was rerun. The implementation records passing focused semver,
server, Rust, Python, and TypeScript journey commands plus the applicable
shared/SDK, typecheck, napi, codegen, boundary, format, and lint lanes. Those
are supplied evidence for orchestration to reconcile. I independently ran
`git diff --check 0569b79702218600c4f9790f45cc03100d5c6f1c..2d669917c03699876b3c8926f0de5ac88c578c01`,
which exited successfully.

Candidate immutability and availability of every other required independent
report remain orchestration gates; they do not limit this report's maintainer
source coverage.

## Overall result

**PASS** — the cumulative candidate is maintainable under current Wyrd
authority. Every materially changed owner, boundary, caller, test, public
declaration, and generated projection reviewed has a discoverable role and a
concrete current consumer. R4 closes the remaining selector invariant at the
existing producers, and no material layout, ownership, method-shape,
naming/type, documentation, test-clarity, declaration-parity, or unsupported
mechanism finding remains.
