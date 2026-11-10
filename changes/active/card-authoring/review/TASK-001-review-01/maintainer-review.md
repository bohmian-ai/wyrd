# Maintainer review — TASK-001, first review

Result: **FAIL**

Subject: base `5234961d51798581a026834cfffd5efaadc226b4`, candidate `790da3369c1b0107765db234d72c1ae0c4680446`. HEAD matched the candidate at the start and end of source inspection; tracked implementation was unchanged. No prior evidence carried forward. This review inspected source independently and did not consume another reviewer report or an implementation completion summary.

## Authority and coverage

Read the approved specification revision 3, original task including its verification evidence, root AGENTS instructions, architecture authority map and Card/file-authoring/registration sections of `architecture/wyrd-design.md`, `changes/spec-driven-development.md`, and `agent_docs/{index,rust,maintainer-style}.md`. No nested AGENTS exists in the changed client, loader, spec, Rust SDK or fixture subtrees. There is no `.codegraph` directory.

| Changed surface | Source and connected coverage | Maintainer assessment |
|---|---|---|
| Definitions and graph lowering | Complete `authoring/mod.rs`: Definition, metadata validation, CardDefinition conversions/declaration, Dependencies, Graph, Linked, Binding, Prompt, Agent, Service, Artifact/LocalSource/ArtifactFile and helpers | Concrete value owners and canonical specs are discoverable; replacement edits fail to keep their captured dependency ownership synchronized (M-001). |
| Verifier authoring | Complete `authoring/verifier.rs`: Verifier inputs/edits/plan/validation; Step/Requirement; Action/Check; AgentCall/CodeCall | Thin variants and direct fields preserve native meaning. Planner/validator delegation is appropriate. Workspace replacement shares M-001. |
| Loader integration | Complete loader lib and order modules; resolution state/path resolver; validator and canonical graph build, root and composition owners | Uses existing validation, ordering and reference inventory; no second registration compiler found. See comparison below. |
| Cards/state entry points | Cards declare_from_path/declare/upload_artifact/declare_authored and their engine/saga ownership; state declare_with, authored forms and extracted pull_declared | Artifact transfer remains on upload_artifact; final declaration remains on Cards::declare. Shared pull/hydrate tail removes duplication. No new lifecycle owner. |
| Public projections/helpers | Client module export, Rust SDK authoring reexport, integration module registration, Reference input/step_output/at with surrounding parser representation | Public surface belongs to the common client; reference helpers call the canonical parser. |
| Local proof | Entire authoring/tests.rs, all three tests and helper functions | Checks canonical parity, snapshot edits and conflicts; does not exercise replacement of dependency-bearing fields. Mandatory test/helper rustdoc is missing (M-002). |
| Real journeys and fixtures | Entire integration/authoring.rs, four journeys, Story/provider/request helpers, all five YAML files, release_checks.py and fixture index | Real server setup and outcome assertions follow nearby owners. One checked-in Python story feeds file and code forms. Story fields lack mandatory rustdoc (M-002). PDF treated as fixture bytes, not rendered. |
| Normative documentation | New in-memory-authoring subsection and adjoining file/reference lifecycle contract | Describes actual existing-owner delegation. No independent workflow added by documentation. |

## Comparison with existing workflow owners

The existing `wyrd-loader::resolve_tree`/`ResolutionState` discovers children from filesystem paths, resolves those paths to sibling identities through `ReferenceSlotVisitor`, and supplies AuthoredCards to the validator/order pipeline. Its PathSandbox, config/default application, source provenance and depth limit are filesystem responsibilities. It cannot consume captured in-memory child values without introducing a false filesystem bridge. The new Dependencies carries those value snapshots and Graph only flattens them, deduplicates equal captured identities and rejects conflicting captures. Graph does not compute dependency edges, select the root, validate composition, rewrite registry identity, schedule work, or perform IO. `CardDefinition::declaration` then delegates those graph rules to `declaration_input_from_cards` -> existing `validate_tree`/`order_cards` -> canonical `wyrd_spec::graph` and reference inventory. Consequently the private name Graph alone is not evidence of a forbidden duplicate graph store. Value capture and equal-definition coalescing are approved outcomes that earn this local adaptation.

The new loader entry point supplies already typed/resolved values to those existing phases; it does not duplicate their implementations. The Cards method sequences existing `upload_artifact` operations followed by existing `declare`, which is precisely the publication-first SDK adaptation approved in revision 3. `upload_artifact` still owns spawn_blocking preparation, retained scratch lifetime and the ordinary saga. State extraction reuses the existing pull/load tail for both entry points. Removing these adapters entirely would lose the requested in-memory surface; replacing them with direct saga/transport/preparation code would add a duplicate owner. No separate DRIFT finding is justified by the inspected orchestration.

## Material findings

### M-001 — Replacing a dependency-bearing field leaves the old definition/source in the declaration

Classification: INCORRECT. Locations: `authoring/mod.rs:450-453` (`Binding::set_runs_on`), `authoring/verifier.rs:537-552` (`AgentCall::add_workspace_input` / `add_workspace_file`), and their shared append-only capture at `authoring/mod.rs:72-74`.

Authority: REQ-002 direct incremental edits and REQ-008 constructor/incremental parity; maintainer-style requires retained state to have one coherent owner. A field and the definitions it owns must represent the same current value.

Source-proven path: construct an AgentCall with `add_workspace_file("/inputs/policy.json", local_policy.file(...))`, then change that path to a scope input using `add_workspace_input` before adding the call to its Verifier. The canonical `workspace` map now contains only Scope, but the old Artifact remains in `dependencies.artifacts`. Action/Check -> Step/Requirement -> Verifier -> Service absorb those dependencies. `Graph::add` unconditionally gathers that Artifact; declaration therefore still lists it in `Declaration.artifacts`; `Cards::declare_authored` publishes it before submitting a graph that does not reference it. An unavailable old path refuses an otherwise valid scope-only graph; an available path unnecessarily publishes obsolete local files. Replacing a local Artifact by another Artifact at the same workspace path likewise retains both, and a changed source with the same identity spuriously triggers the duplicate-submission conflict.

Sibling replacement: call `Binding::set_runs_on` with a sibling Trigger definition, then set an inline or different registered Trigger. The canonical runs_on is replaced while the old Trigger remains in Dependencies.cards and is emitted by Graph. This can unnecessarily declare unused content or refuse a standalone Agent-root graph through the existing multiple-root check (`graph/root.rs::pick_root`); a Service root can instead carry the obsolete peer through declaration. The root preference for Service means multiple-root refusal is not asserted for that variant.

Concrete maintenance cost/consequence: every future replaceable authoring field must manually coordinate a canonical value with a historical append-only bag, while consumers cannot distinguish active dependencies from superseded ones. Constructors and edits with identical final specs have different declarations and side effects. This is an ordinary pre-registration edit, not an unbounded hostile-input case.

Minimum correction: keep dependency capture synchronized with the current replaceable slot at its authoring owner, retaining dependencies still used elsewhere. A replaced workspace file or Trigger must cease contributing its old local source/definition once no current reference uses it. Do not fix this with upload-error suppression, rollback, a new registration engine or repeated downstream guards. Existing reference inventory can assist determining live references, but final specs must not be supplemented by unused historical captures. Add small offline assertions comparing a directly constructed scope-only/current-Trigger definition with the same definition reached by replacement; also assert replacement at one workspace path preserves a local Artifact still mounted at another path. A no-upload capture check for the obsolete source closes the IO consequence if needed; no new harness is warranted.

### M-002 — New private test items do not meet the mandatory rustdoc gate

Classification: VIOLATION. Locations: `authoring/tests.rs:12` (rule), `:19` (reviewer), `:36` (policy), `:172` (yaml), `:176` (card), `:182` (core_primitives_match_canonical_contracts), `:453` (offline_edits_preserve_snapshots), `:544` (story), `:744` (declarations_match_file_authored_tree); `sdks/wyrd-sdk-rust/tests/integration/authoring.rs:98-99` (Story fields).

Authority: root AGENTS Code and `agent_docs/rust.md` Rustdoc explicitly require documentation on every new/changed item, including private/test helpers, tests and fields, with Errors/Panics as applicable; missing documentation blocks merge. The module's `deny(missing_docs)` does not establish private-item coverage.

Evidence/consequence: those new functions and fields have no rustdoc. This is an explicit repository gate, irrespective of probability or a successful compiler/lint run. Nearby documented fact_step, release_inputs and the integration journey tests already demonstrate the intended shape.

Minimum correction: document the listed new items with their workflow purpose and relevant failure/panic contracts; review the new private conversion implementations for the same requirement when closing the rustdoc gap. Keep this bounded documentation correction in the behavioral remediation, without a new checker or style-only review round.

## Verification and limits

Task-recorded local authoring tests, four journeys, concurrency regression and format/lint/dependency/docs results were read as evidence. No Cargo, build, tests or server were executed in this review as instructed; replacement behavior above is established by complete producer-to-consumer source tracing, not claimed as a runtime reproduction. No source, task, fixture or authority was modified. Only this report was written.

Result remains **FAIL** for M-001 and the explicit M-002 gate.
