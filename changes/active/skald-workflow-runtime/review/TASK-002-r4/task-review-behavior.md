# Independent behavior review — TASK-002 R4

## Subject and method

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `0569b79702218600c4f9790f45cc03100d5c6f1c`
- Candidate: `2ff7bbfa57737ce8a7a69287e2fa462c313b1a8c`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 12
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-002-cleanup.md`
- Remediation inputs: `review/TASK-002-r2/TASK-002-R2-close-cleanup-review-gaps.md` and `review/TASK-002-r3/TASK-002-R3-close-remaining-review-gaps.md`

`HEAD` matched the candidate before and after inspection. The repository has no
`.codegraph/` index, so the complete diff and current owners/callers were traced
directly. I reviewed commit `375d97e67f3affe0d5c59727ef3135b22a459140`
as part of the cumulative candidate even though the human removed
`FIND-TASK-002-11` from the R3 remediation task. I did not run builds or tests
and did not modify implementation source.

The caller-to-result paths inspected were:

- authored file -> `wyrd_loader::load` -> `WorkflowBodies` -> lazy Cards reads
  -> `CardGraphHydrator` -> Skald hydration/validation -> SDK run;
- registered selector -> Cards read -> locked relationship traversal -> active
  body set -> Skald hydration/validation -> SDK run;
- composite request -> pure graph -> `EffectiveSpecs` provenance-aware body
  resolution -> declaration-only Skald validation -> expected-UID recheck ->
  audited transaction and relationships;
- Python/TypeScript selector conversion -> shared `WorkflowCards::load` ->
  catalog error projection; and
- each Rust/Python/TypeScript journey's local, mixed, collision, inactive,
  exact/UID, after-v2, and relationship assertions.

The standing direction was applied as an acceptance constraint: no finding or
correction below requires a bespoke mechanism, check, file, setting, or option.
The retained correction uses the existing typed constructors and catalog error
projection. I found no new nonstandard mechanism in the reviewed runtime path
that should be added or retained as remediation.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001/002/003, AC-001: one normal Workflow Card, Agent steps, and existing Prompt binding | `wyrd-loader/src/lib.rs:80-145`; `skald-workflow/src/bodies.rs:35-121`; `wyrd-client/src/workflow.rs:57-73`; checked-in `examples/workflows/code-review` | loader and shared-client focused tests exercise the actual example; SDK journeys exercise equivalent Native local/mixed graphs; full example route closure remains assigned to TASK-005 | PASS for TASK-002 allocation |
| REQ-013/013A/014, AC-006: pure and resolved failures occur at the earliest informed boundary without dispatch | `wyrd-loader/src/validate.rs:61-77`; `skald-workflow/src/bodies.rs:58-121`; `EffectiveSpecs::validate_workflows` at `resolve.rs:406-465` | loader/client negative assertions and `registers_only_valid_explicit_workflow_graphs` cover cycles, bindings, dialect and no-write/no-dispatch boundaries | PASS |
| REQ-025/054/055, AC-029/030: authored local and mixed loading uses the shared loader, remains credential-free when local, and lazily reads external Cards | `Workflow::from_path` at `wyrd-client/src/workflow.rs:27-74`; `WorkflowBodies::external_refs` at `cards/hydrate/workflow.rs:76-102`; `resolve_external` at `:165-181` | Rust, Python and TypeScript journeys cover local success, absent credentials, denied reads, registered refs and Native runs | PASS |
| REQ-056, INV-005: sibling/external provenance, exact versions, locked UIDs, active state and authority are preserved | separate stores and provenance lookup at `cards/hydrate/workflow.rs:27-34,139-161`; exact graph traversal at `graph.rs:339-369`; active check at `workflow.rs:104-125` | all three journeys distinguish local and registered same-identity bodies, reject a deleted dependency and prove pinned output after v2 | PASS |
| REQ-028: composite registration validates and writes the same exact graph without provider execution | `EffectiveSpecs` at `resolve.rs:111-226,406-514`; `RegistrationWriter::write` at `service.rs:1040-1148` | PG registration tests retain invalid-graph no-write and declaration-only assertions | PASS |
| REQ-014/056 and preserved FIND-4: a dependency replaced after preflight cannot be silently rebound | expected `(CardRef, CardUid)` carried into `recheck_active_card_refs` at `service.rs:1087-1092`; SQL identity+UID+Active lock at `relationships.rs:17-68` | `refuses_stale_preflight_after_dependency_replacement` and `relationship_recheck_blocks_target_lifecycle_race` | PASS |
| REQ-052: registration validates declaratively without resolving execution tools | `Workflow::validate_card_bodies` clears inline and referenced Agent tool names only for validation at `skald-workflow/src/bodies.rs:90-121,175-203` | registration negative/positive graph tests | PASS for TASK-002; shared selected execution setup remains TASK-003 |
| Public Rust authored and registered API contract | `wyrd-client/src/workflow.rs:19-191`; Rust SDK re-export | `workflow_loading_journey` covers local/mixed runs, exact/UID load, identity assertions, denial and inactive dependency | PASS |
| Public Python authored and registered API contract | `PyWorkflow::from_path` at `sdk-python/src/workflow.rs:472-507`; `PyWorkflowCards::load` at `state/mod.rs:2562-2639` | Python integration journey covers both APIs, public runs and selector negatives | PASS |
| Public TypeScript authored and registered API shape | native projection at `sdk-ts/native/src/workflow.rs:21-152`; public `WorkflowCards`/`Workflow` at `wyrd/src/index.ts:1195-1244,1325-1375` | TypeScript type test and real-server journey cover public load/run, mixed selector, denial and inactive dependency | PASS except malformed-field metadata below |
| INV-007 and prior FIND-TASK-002-11: malformed Workflow selectors use `WYRD_WORKFLOW_400_INVALID_CARD_REF` with the bad field named consistently | shared Rust returns `field=version|kind` at `wyrd-client/src/workflow.rs:167-185`; Python returns `uid|space|name|version|selector` at `state/mod.rs:2608-2633`; TypeScript converts every serde field failure to `field=selector` at `native/src/workflow.rs:76-97` | journeys assert the code for mixed/versionless cases, but no TypeScript proof checks malformed individual fields or `details.field` | **FAIL — FIND-TASK-002-11** |
| INV-007 and FIND-8: TypeScript input/result is the native portable Workflow contract | recursive `JsonValue` and complete snapshots at `wyrd/src/index.ts:1246-1374`; native run delegates at `native/src/workflow.rs:126-151` | `workflow-types.test.ts` and integration run assertions | PASS |
| AC-029/FIND-7: each language checks exact Workflow->Agent and Agent->Prompt refs plus relationships | Rust `assert_locked_graph` in `workflow_loading.rs:207-249`; Python envelope checks in `test_cards_crud.py:612-625`; TypeScript checks in `workflow-loading.test.ts:153-170` | strengthened three language journey commands reported passing in R3 implementation evidence | PASS |
| REQ-057/059, AC-031: no Workflow principal/WyrdState root, duplicate loader/traversal/runtime, keyed normalization or compatibility alias | Workflow facade delegates at `wyrd-client/src/workflow.rs:1-9`; hydration extends Cards owner; Skald owns body resolution; obsolete symbol search finds no retired Workflow loader/graph implementation | source/diff audit plus shared Service/loader regression evidence | PASS |
| Async filesystem boundary/FIND-13 | `Workflow::from_path` moves loader and canonicalization together through `tokio::task::spawn_blocking` at `wyrd-client/src/workflow.rs:39-64,99-121` | focused loader/client tests and Rust/Node journeys reported passing | PASS |
| FIND-12 generic Python registry selectors use request Validation while real Data validation remains Data-owned | `invalid_selector` and shared parsers in `sdk-python/src/state/mod.rs:3030-3100` | parameterized public registry-surface test and genuine Data validation regression reported passing | PASS |
| Generated declarations, workspace feature union and cumulative patch hygiene/FIND-14/15 | generated Python/TypeScript declarations and regenerated `workspace-hack/Cargo.toml`; prior report EOF repaired | reported codegen/type/hakari checks and explicit `git diff --check`; this review also observed a clean cumulative `git diff --check` | PASS based on supplied evidence/source |
| Non-goals: no server Workflow execution, CLI lifecycle, public hydrator, new parser/transport/cache, secret resolution or provider dispatch during load/apply | cumulative production diff is confined to loader/client/Skald composition, registration validation and SDK projections; later TASK-003/004/005 surfaces are absent | source/diff audit | PASS |

## Proposed finding

### FIND-TASK-002-11 — INCORRECT: TypeScript still loses the malformed selector field

- **Discovery source ID:** `BEH-R4-001`
- **Violated obligation:** Revision 12 REQ-054 and INV-007; the previously
  selected `FIND-TASK-002-11` outcome requiring
  `WYRD_WORKFLOW_400_INVALID_CARD_REF` with `details.field` naming `uid`,
  `space`, `name`, `version`, `kind`, or `selector` as appropriate.
- **Exact location:**
  `sdks/wyrd-sdk-ts/native/src/workflow.rs:53-97`; public caller
  `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1213-1243`.
- **Evidence:** `WorkflowSelectorJson` deserializes `uid`, `space`, and `name`
  directly into domain types. Any malformed individual value therefore fails
  the single `serde_json::from_str` at lines 83-84. Its error is passed to the
  `invalid` closure, which always emits
  `details: { "field": "selector" }` at lines 76-82. Only after successful
  deserialization does the shape match run. Thus `{uid: "bad"}` or a malformed
  `space`/`name`/`version` reaches the public `WorkflowCards.load`, is correctly
  refused before IO with the Workflow code, but falsely identifies the whole
  selector rather than the bad field. Python already names each field at
  `state/mod.rs:2614-2627`, and shared Rust names its representable malformed
  cases at `wyrd-client/src/workflow.rs:169-185`. The TypeScript journey checks
  only a mixed shape and only the code (`workflow-loading.test.ts:186-195`), so
  it cannot close the individual-field requirement.
- **Observable consequence:** JavaScript callers and TypeScript callers that
  cross the static type boundary receive non-actionable, cross-language
  inconsistent structured diagnostics for malformed selector values. A user
  cannot tell whether `uid`, `space`, `name`, or `version` must be corrected
  from `details.field`, despite the selected public error contract.
- **Required testable correction:** keep the current public selector type,
  shared Cards loader and `WorkflowInvalidCardRef` catalog variant. At the
  existing TypeScript native selector-conversion boundary, parse the closed
  selector shape and convert each present field with its existing domain
  constructor so a value failure names that field; reserve `selector` for
  missing, mixed, or unknown-field shape failures. Do not add a validator,
  schema package, compatibility option, check, or new abstraction. Extend the
  existing TypeScript test surface with malformed `uid`, `space`, `name`, and
  `version` cases asserting the Workflow code, status, and exact
  `details.field`, plus the existing mixed-selector case, all before registry
  IO.

## Overall result

**FAIL.** The cumulative candidate closes the prior graph, provenance,
registration, async-boundary, relationship-proof, generic Python selector,
generated-state and hygiene findings. One bounded portion of the human-selected
selector contract remains open in the TypeScript projection, so this task does
not yet satisfy the original outcome exactly.
