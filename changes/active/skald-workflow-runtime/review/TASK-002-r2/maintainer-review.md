# Maintainer review — TASK-002-r2

Result: **FAIL**

## Subject and authority

Reviewed the cumulative base `0569b79702218600c4f9790f45cc03100d5c6f1c` to candidate `e7d16b5bd622b9a564a49edb18239df7f209ca92`, not merely the latest cleanup commit. HEAD still equals the candidate. Source was not edited.

Applied `AGENTS.md` §§3–9, 11–12, 16; `architecture/agent-rules.md`; the Workflow, client model, composite-registration and authoring authority in `architecture/wyrd-design.md`; `wyrd-doctrine.mdx`; `architecture/references/languages/{maintainer-style,spec-driven-development,rust-core,typescript-guide,python-api-and-stubs}.md`; approved Revision 12 and `TASK-002-cleanup.md`. Revision 12 supersedes the original task's conflicting private mechanics. Historical review artifacts in the cumulative range are not treated as independent proof or current reviewer conclusions.

## Changed-surface coverage

| Changed surface | Owning symbols, consumers and proof inspected | Assessment |
|---|---|---|
| Shared public loading | `wyrd-client/src/workflow.rs`: Workflow facade, from_path/run/as_skald/into_skald, WorkflowCards::load, Cards::workflow; Rust re-export and Python/Node consumers; from_path_uses_existing_loader | Facade stores only Skald runtime; lazy client creation and precise selector errors are discoverable and documented. |
| Shared graph | `cards/hydrate/{mod,graph,workflow}.rs`: GraphScope, GraphTraversal construction/preloaded roots/run/load_card/schedule_relationships/finish, resolve_graph/resolve_refs, WorkflowBodies authored/external_refs/body/hydrate, CardGraphHydrator methods; existing bundle writer and disk hydration caller | Existing traversal is extended for in-memory consumption rather than replaced. Dependency-owning workflow shape remains incomplete at the two graph entry functions (MAIN-002). |
| Loader/reference contract | Existing loader pipeline, parse/resolve/validation/ordering seams and their callers; added WorkflowSpec validation; canonical ReferenceSlotVisitor, InlineableRef::to_durable; loader bundle test | No second parser, discriminator inventory or loader dialect. Pure validation stays synchronous and provenance survives projection. |
| Skald lowering | `bodies.rs`: CardBodies, card_body_dependencies, Workflow::from_card_bodies/validate_card_bodies, CardBodyResolver body/AgentResolver/PromptResolver; existing from_card_with_agent_resolver and ResolvedGraph::resolve, builder/run and YAML/disk consumers | Resolver adapter fills an existing seam; it does not duplicate Prompt binding, resolved validation, planning or execution. Registration-specific suppression of tool binding stays in the declarative validation entry. |
| Server composition | `cards/resolve.rs`: resolve_card_references, EffectiveSpecs new/load/body/validate_workflows and sibling binding consumers; registration preflight/plan/write call chain in service.rs; registration request mutations and exact-UID replacement test | Existing EffectiveSpecs gains Workflow body closure and delegates validation to Skald. Binding/baseline/Operator sibling consumers remain in their owner. SQL stays server-owned. |
| SQL fence | recheck_active_card_refs full body, write_registration caller, relationship_recheck_blocks_target_lifecycle_race and server replacement interleaving | Expected UID and Active-row lock are explicit; docs explain transaction ownership and cancellation. |
| Rust SDK | lib.rs re-export smoke references, manifest and full workflow_loading.rs | Thin SDK projection; exact identities, stored UIDs, output, newer-version pinning and selector refusals are clear. Recorded lack of ambient-ref Rust proof is a verification limit rather than concealed by its name. |
| Python | PyWorkflow::from_path and native conversion; PyCards getter/PyWorkflowCards::load/native registration; public exports, authoritative stub templates and generated package declarations; integration and save/load consumers | Runtime/stubs expose the same selector alternatives and Workflow return. Existing shared runtime bridge releases the GIL. New public exports are registered. |
| TypeScript | NativeWorkflow/NativeWorkflowLoad/selector conversion/native run and NativeCards::load_workflow; wrapper Workflow/WorkflowCards/WorkflowSelector/WorkflowRun; package exports and generated .d.ts/.d.cts; full new integration test | Selector and native handle conversion are coherent. Public input/result typings erase parts of the approved closed contract (MAIN-001). |
| Docs/examples/fixtures | Changed architecture authoring/Workflow sections, skills and execution guidance; site Workflow loading edits, holder README and Python example; complete code-review and shared fixture YAML diff | Entry paths, native Prompt bodies, exact versions and loader-local dependency syntax are consistent with the new loading entrypoints. No new persistent implementation-history notes were found in changed production source. |
| Manifests/gates | Changed manifests, Cargo.lock workspace-owner edges; task focused-command and lane evidence, mise verification entries, static diff check | No new third-party dependency or feature abstraction. Verification claims have no raw logs in this subject; not independently rerun here. |

## Existing-owner comparison

- **Authored loading:** `wyrd_loader::load` already owns filesystem discovery, sandboxed references, diagnostics and ordering. The gap is asynchronous registered reads plus runtime construction; the facade calls this owner and supplies its bodies to the existing hydration owner. No replacement parser or independent filesystem traversal is needed or present.
- **Registered traversal:** pre-existing `CardGraphHydrator` and `GraphTraversal` already own authenticated graph hydration, alias/cycle/exact-identity checks and bundle publication. `GraphScope` extends the same traversal to omit artifacts and non-runtime relationships. WorkflowBodies is a per-load adapter holding loader siblings separately from fetched registered Cards, not an enduring registry cache or competing identity owner. It serves the two provenance forms through the same Skald resolver callback.
- **Runtime construction:** existing `Workflow::from_card_with_agent_resolver`, `Agent::from_card`, PromptResolver and `ResolvedGraph::resolve` already lower and validate native runtime values. CardBodyResolver supplies fetched bodies through those seams; a second graph/validator/executor is unnecessary and absent.
- **Server:** EffectiveSpecs already caches effective sibling/external bodies for binding/baseline validation; its new Workflow method supplies those bodies to the same Skald checks. A second server graph store or HTTP loading workflow would be redundant and is absent.
- **Language SDKs:** Rust remains a re-export; Python and Node wrappers perform runtime-boundary conversions. Their existence is earned by their foreign-runtime boundary. MAIN-001 concerns the typed projection, not an argument for extra SDK execution machinery.

## Material findings

### MAIN-001 — TypeScript loading's runnable result does not project the approved typed run contract

**Classification:** VIOLATION / incomplete public projection.

**Changed locations:** `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1263`–`1273`, `:1321`.

**Authority:** Revision 12 Public APIs and REQ-054 fix `run(input?: Record<string, JsonValue>): Promise<WorkflowRun>`. Maintainer Style “Python and TypeScript: document the typed contract” explicitly makes a hidden result shape or invalid permitted call a finding. TypeScript Guide requires invalid states to be unrepresentable and wire fields to remain the contract.

**Source and reachable consequence:** Public run accepts `Record<string, unknown>` and feeds it directly to JSON.stringify. Therefore `workflow.run({code: () => "diff"})` and `workflow.run({code: 1n})` are statically valid: a function is silently omitted and a bigint throws a raw serialization TypeError before the native contract. The approved JSON-valued signature excludes both.

The native result is the Rust portable WorkflowRun (`NativeWorkflow::run` delegates to the shared Workflow and NativeLifecycleResult::outcome). Its closed `steps` shape is `BTreeMap<String, WorkflowStepResult>` with status, text, structured_output, attempts, timestamps and error (`wyrd-spec/src/card/workflow.rs:775`–`789`). The wrapper instead declares every step as unknown. Its run error declares only code/message although the same native WorkflowRunError includes details and remediation (`:723`–`733`). A caller cannot inspect attempts, terminal status, or catalog remediation through the supported type without rebuilding/casting its own parallel contract. This creates a concrete safe-change cost: those private consumer shapes will not be checked when the authoritative snapshot changes.

**Smallest correction:** Keep the current native execution and lifecycle conversion. Project the existing JSON value and closed WorkflowStepResult/WorkflowStepStatus/WorkflowRunError wire shapes accurately in the public TypeScript source; use the approved JSON-valued input type and retain genuinely open JSON output/details fields as JSON values. Include all authoritative error fields. Do not add a JavaScript execution engine or second validator.

**Closure proof:** A compile-time SDK check accepts nested JSON input and ordinary access to step status/attempts and error details/remediation, and rejects function/bigint inputs without casts; run `mise run ts:typecheck` and the owning declaration checks/build. Existing runtime journey continues to exercise the same native handle and catalog errors.

### MAIN-002 — New graph entry orchestration remains outside its existing dependency owner

**Classification:** VIOLATION of required Rust structure.

**Changed locations:** `crates/shared/wyrd-client/src/cards/hydrate/graph.rs:322` and `:350`.

**Authority:** AGENTS.md §5 requires dependency-backed multi-step workflows to be inherent methods of their concrete owner; passing dependencies into a free function does not make a workflow stateless. Maintainer Style “Layout: put a workflow with its owner” and Rust Core “Required Structural Style” apply to new and materially changed symbols.

**Source and concrete maintenance cost:** New resolve_refs owns the workflow of exact registry reads, response-to-reference conversion, traversal seeding, traversal completion and result publication while accepting RegistryEngine and GraphScope as parameters. Materially rewritten resolve_graph runs the parallel selected-root preparation sequence with the same dependency bundle. These are asynchronous IO orchestration, not deterministic conversion helpers.

Their callers are all methods of the existing dependency-owning CardGraphHydrator: disk hydrate (`hydrate/mod.rs:84`), resolve_external (`hydrate/workflow.rs:179`) and load_workflow (`:200`). Each reaches through self.context.engine and threads it into the module-level entrypoint. Root-loading and graph lifecycle behavior are therefore discovered and changed outside the handle that is documented as owning them, and every new hydration consumer must know the private engine extraction and entry protocol. GraphTraversal already owns the mutable traversal invariants, so the missing shape does not justify another service or traversal.

**Smallest correction:** Put these graph entry operations on the existing CardGraphHydrator, taking its registry dependency from its owned context and retaining the existing GraphTraversal state machine for traversal. Preserve GraphScope, root-response reuse, exact reads, alias/cycle/UID checks and all disk/runtime behavior. Pure alias/identity conversion helpers may remain free. No new owner, cache, traversal, public API or runtime behavior is required.

**Closure proof:** Source review shows the three existing hydration callers invoke methods through the owning handle rather than threading its RegistryEngine into workflow functions. Run the retained exact authored-load test and Workflow/Service hydration regression lane; no new structural-test framework is needed.

## Verification and limits

Performed `git diff --check 0569b797 e7d16b5b` successfully and checked HEAD against the immutable candidate. No builds, live environments, code generation or source mutations were performed. Task evidence claims focused tests and broader format/lint/typecheck/codegen lanes passed, and explicitly records incomplete Python/TypeScript execution until TASK-003; those claims do not establish the missing public type precision or structural compliance.

No personal-preference findings are retained. The two findings are source-local, bounded corrections under existing authority; neither needs a new product decision.
