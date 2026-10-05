# TASK-002 Behavior Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `0569b79702218600c4f9790f45cc03100d5c6f1c`
- Candidate: `e165360b1264d3628b13b02c41567c047bf96930`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 11
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-002-load-and-register-graphs.md`

The complete base-to-candidate diff was reviewed. The candidate remained at the
stated commit throughout this review.

## Caller-to-result paths reviewed

1. Authored Workflow entry -> `wyrd_loader::load` -> keyed inlineable-reference
   normalization -> containing-file-relative path resolution -> pure
   `WorkflowSpec::validate` -> dependency-first `LoadedTree`.
2. `WorkflowLoader::load_file` -> local Agent/Prompt bodies plus exact registry
   reads for remaining `ref` dependencies -> `WorkflowGraph::hydrate` ->
   Skald's existing `Workflow::from_card_with_agent_resolver` -> existing Prompt
   binder and Workflow executor.
3. Composite `POST /v1/cards` -> tenant-scoped reference resolution ->
   `EffectiveSpecs::validate_workflows` -> pure/resolved Skald validation ->
   existing transactional registration and UID binding.
4. `WorkflowLoader::load_registered` -> exact active Workflow read -> exact
   locked Agent/Prompt reads with UID assertions -> the same graph hydration
   seam -> local Skald execution with the registered Workflow identity.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001: one ordinary `wyrd/v1` Workflow Card and no second local format | `examples/workflows/code-review/workflow.yaml`; loader decodes it through the existing `Spec::Workflow` envelope path | `wyrd-loader::tests::load_explicit_workflow_bundle`; `wyrd-client::workflow_loader::tests::hydrate_local_workflow_graph` | PASS |
| REQ-002 and REQ-025: Workflow Agent targets support inline, containing-file-relative `path`, and exact registry `ref`; a ref without a client refuses clearly | `wyrd-loader/src/parse.rs:245-326`; canonical slot visitor/path resolver; `workflow_loader.rs:74-103,142-176,238-264` | Local bundle test covers path plus inline Prompt; hydration test covers offline `ref` refusal; registered journey covers authored exact refs | PASS |
| REQ-003, REQ-040, INV-014: Prompt remains the native request owner and Skald's Prompt binder remains the only renderer | Checked-in Agent YAML uses native `request`; `GraphResolver` returns `Prompt::from_native`; hydration delegates to `Workflow::from_card_with_agent_resolver` | Hydration test proves both reviewer outputs and the original input reach the final native request through declared Prompt variables | PASS |
| REQ-013: pure Workflow validation runs during local load and before composition proceeds | `wyrd-loader/src/validate.rs:61-76`; `WorkflowGraph::new` at `workflow_loader.rs:193-204` | Loader bundle test rejects a dependency cycle; the pre-existing Revision-11 contract suite remains the exhaustive pure-rule owner | PASS |
| REQ-013A: resolved graph validation rejects Prompt coverage and route/request incompatibility | `WorkflowGraph::hydrate` and `WorkflowGraph::validate` at `workflow_loader.rs:266-315` reuse Skald resolved validation | Hydration test rejects an extra Prompt binding and route dialect mismatch before dispatch; registration matrix covers the same resolved rules | PASS |
| REQ-014: pure/resolved checks run at the earliest boundary with enough information | Local loader validates pure rules; client graph validates after bodies are gathered; `EffectiveSpecs::validate_workflows` validates registration before the write transaction | Local, registration, and registered-local focused tests all exercise their boundary; server acceptance is explicitly TASK-004 scope | PASS |
| REQ-024 and AC-001 task-owned Rust slice: actual bundle loads and executes locally on the one Skald runtime | `WorkflowLoader::load_file` composes directly into `skald_workflow::Workflow`; no alternate executor was added | Hydration test executes three gateway calls and proves the dependent final request receives both reviewer outputs | PASS |
| REQ-024 and AC-003 task-owned Rust slice: registered graph fetches and executes locally with equivalent shape/bindings | `WorkflowLoader::load_registered` and common `WorkflowGraph::hydrate` path | `fetches_and_executes_locked_workflow_graph` compares outputs and step keys with authored external-ref execution | PASS |
| REQ-028 and AC-002: composite registration accepts the real bundle, preserves exact graph refs/relationships, and remains declarative | Existing `Cards::register_from_path`; server `resolve_card_references` now calls `validate_workflows` before writes; existing UID binding/persistence remains unchanged | Registration journey verifies four active Cards, exact Agent UIDs/versions and direct relationships; its Prompt-ref/tooling subgraph verifies a locked Prompt UID | PASS |
| REQ-029 and INV-005 task-owned registered-local slice: exact active versions are fetched and later versions do not float | `WorkflowLoader::read` uses `Cards::get(CardSelector::exact(...))`, asserts returned identity/optional UID through the existing Cards read, and rejects non-active Cards | Registered journey adds Agent v2, proves it is unused, preserves Workflow UID/version, and rejects missing, mismatched, non-exact, foreign, pending, and deleted inputs with no dispatch | PASS |
| REQ-052: graph hydration preserves each step's resolved Agent and Prompt semantics rather than introducing a second lowering path | `WorkflowGraph` supplies already-fetched bodies through the existing Agent/Prompt resolver seams; Skald runtime source change is documentation only | Actual three-step execution and resolved binding/route negative cases exercise the shared lowering path | PASS |
| INV-002 and INV-003: pure contracts remain IO-free in `wyrd-spec`; Card envelopes remain declarative | No `wyrd-spec` or Card-envelope dependency/state change; IO is confined to loader/client/server owners and `WorkflowGraph` itself is synchronous and IO-free | Diff inspection; reported `check:pyo3-scope` and codegen evidence are consistent with the source | PASS |
| INV-008: local tools come only from the caller registry; registration does not reject declared built-ins or install server tools locally | `WorkflowLoader` owns `Arc<dyn ToolResolver>`; `WorkflowGraph::validate` suppresses execution-environment tool binding only for declarative registration | Registration journey accepts `bifrost.query` and `cards.get`; local hydration uses the supplied registry | PASS |
| AC-006 task-owned boundary proof: malformed pure/resolved graphs refuse without a partial durable graph or provider dispatch | Pure contract owner plus loader validation; server validation occurs in `resolve_external` before `write_registration` | Cycle, binding, output, graph, and route cases; sibling/external parity; operation/Card counts stay unchanged; gateway-call count stays unchanged | PASS |
| AC-013: focused unit and integration evidence covers loader hydration, registration, exact client transport, and runtime execution | Tests live with the owning loader/client and in a real client-to-server Postgres target | Independently rerun four exact focused tests listed below; broader recorded lanes are credible but were not rerun in this reviewer context | PASS |
| Non-goals: no WyrdState Workflow root, Skald registry dependency, loader registration, registration-time secret/endpoint resolution, blanket tool prohibition, PromptDraft rewrite, compatibility path, second YAML/executor, or new third-party package | Complete diff changes only loader normalization/validation, client composition, server pre-write validation, Skald documentation, tests, and the checked-in example; all dependency additions are existing workspace crates | Diff and caller inspection | PASS |
| Raw Skald YAML entry points do not compete with bundle/reference semantics | `skald-workflow/src/workflow_surface.rs` now explicitly documents `from_yaml_str`/`load` as single-document wire-form parsing without dependency resolution; shared composition is `WorkflowLoader` | Source inspection and client hydration proof | PASS |
| TASK-004/TASK-005 boundaries remain excluded | No server Workflow-run host, CLI workflow commands, or WyrdState expansion entered this diff | Diff inspection | PASS |

## Review Findings

No proposed findings. I found no reachable MISSING, INCORRECT, DRIFT,
VIOLATION, or REGRESSION against TASK-002's approved scope.

## Open Questions

None.

## Verification Notes

Independently rerun with the required shared target directory:

- `CARGO_TARGET_DIR=/home/thorrester/Documents/GitHub/wyrd/target mise exec -- cargo nextest run --locked -p wyrd-loader --lib -E 'test(=tests::load_explicit_workflow_bundle)'` — PASS (1/1).
- `CARGO_TARGET_DIR=/home/thorrester/Documents/GitHub/wyrd/target mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=workflow_loader::tests::hydrate_local_workflow_graph)'` — PASS (1/1).
- Repository-managed Postgres wrapper plus exact `registers_only_valid_explicit_workflow_graphs` selector — PASS (1/1).
- Repository-managed Postgres wrapper plus exact `fetches_and_executes_locked_workflow_graph` selector — PASS (1/1).
- `git diff --check 0569b79702218600c4f9790f45cc03100d5c6f1c..e165360b1264d3628b13b02c41567c047bf96930` — PASS.

I inspected but did not independently rerun the broader recorded `test:shared`,
`test:skald`, `test:cards:integration`, `codegen:check`, boundary, format, and
lint lanes. The focused behavior proof and source paths do not reveal a gap;
the orchestrator should retain the broader evidence as a verification limit
rather than treating this report as a replacement for those lanes.

## Overall result

**PASS**
