# Independent maintainer review — TASK-001

Result: **FAIL** (one explicit component-contract obligation missing from the immutable candidate).

Subject: base `7b36f8fc6`, candidate `dd98e9ef78d20e7de47e7af623644fed0ff9075a`, inspected in `/tmp/wyrd-task001-review-dd98`. First review, full cumulative range. Source unchanged. Approved revision-2 spec read from the original workspace; original task read at candidate. The user's instruction to ignore the recorded BLOCKED label was applied; missing human visual acceptance is not the basis of this maintainer finding.

## Authority and scope

Read root contributor instructions, `agent_docs/index.md`, `agent_docs/maintainer-style.md`, `agent_docs/agent-surfaces.md`, repo UI skill, `brand/DESIGN.md`, approved spec REQ-004/014, original TASK-001, relevant graph design notes, and architecture Verifier section (`architecture/wyrd-design.md:1437`). No changed path has a nested AGENTS.md. CodeGraph was used first for graph and binding navigation; all conclusions use candidate source and actual base-to-candidate diff.

## Changed-surface coverage

| Surface | Source and callers inspected | Maintainer assessment |
|---|---|---|
| Definition projection and binding loader | `packages/ui/src/lib/verifier.ts` complete; `loaders.ts`, exact detail server loader, unit and real-host tests | Coherent Verifier owner; no transport/credential duplicate. Graph projection is presentation of the existing Card, not durable workflow execution. `readVerifier` mirrors nearby `readPrompt` with a distinct kind guard. |
| Automatic graph geometry | `verifier-layout.ts` complete, `VerifierGraph.svelte`, projection/layout tests | Layout/routing responsibility is cohesive and pure. Dagre supplies ranks; local packing/ports/corridors meet approved specific routing obligations. Six-lane permutation search is explicitly bounded. No line-count finding. |
| Graph interaction and rendering | `VerifierGraph.svelte`, `VerifierGraphNode.svelte`, `VerifierGraphEdge.svelte` complete; `VerifierDefinition` and exports | One ephemeral graph owner supplies selection through Svelte context; positions are computed once, with explicit re-key contract and keyed caller. Node/edge decomposition corresponds to Svelte Flow extension points. No second graph store or authoring workflow. |
| Definition inspector and binding rows | `VerifierDefinition.svelte`, `RunsOn.svelte`, existing `UsedBy.svelte` and `Panel` use | Typed props, existing action intents, standard loading/denied/error handling; selected declarations remain local to definition owner. |
| Catalog and public projections | `schemas.ts`, `catalog.json`, `loaders.ts`, `index.ts`, `WyrdView.svelte`; associated catalog/view assertions | New catalog loaders, renderer entries and exports align. Brand contract parity is missing: MNT-001. No SDK wire change is introduced. |
| Conventional route and version state | `cards-host.ts`, full `cards/detail/+page.server.ts`, changed page and surrounding navigation/action code; page tests | Existing exact route/header/tab and version helpers extended. No extra Cards browser or version resolver. Existing header refusal gates the dependent panels. |
| Fixtures and gallery | `ui-fixtures.ts` diff and consumers; `_gallery/+page.svelte` changes and existing checkpoint shape | Fixtures reuse executor/registered-card owners; ordinary/session/drift cases are supplemental display proof. No production mock transport. |
| Unit tests | All new projection, layout, interaction, loader/state/view and exact page test bodies in `ui-library.test.ts`, `ui-pages.test.ts` | Observable graph identities, layouts, read plans, selected declarations, exact navigation and negative states are understandable. The explicit layout assertions correctly exercise the approved presentation rather than merely invoking the code. |
| Integration tests and test setup | All new real-host and Chromium journey bodies plus changed AppBridge setup; `vitest-setup.ts` | Existing real server/SDK/MCP/BFF and browser harness reused; native browser stubs remain test-only. No new test runner. |
| Package/build delivery | Package manifests, lockfile diff and `vite.app.config.ts` | Svelte Flow moves to its actual package owner; approved Dagre dependency pinned there. Browser NODE_ENV substitution is local to artifact build. |
| Public development seed | `scripts/dev/example-app.sh` setup and changed declaration loop | Existing public declaration loop extended with existing policy-desk fixture; no new lifecycle controller. |
| Task evidence | Original task plus evidence/notes diff | Evidence describes limits candidly; it does not substitute for committed contracts or source proof. |

## Existing alternative and reuse analysis

- `packages/ui/src/lib/layout.ts` implements a 12-column view-panel grid and its editing geometry. It has no node dependency rank, graph edge routes, arrow labels or graph targets. Extending that unrelated responsibility would conflate panel-grid editing with the new approved read-only graph. The new pure `verifier-layout.ts` is justified.
- Existing `cards.ts::loadUsedBy` orchestrates exact Prompt -> inbound Agent -> Agent execution route and inbound Services. `loadRunsOn` needs exact Verifier -> inbound Service/Agent -> top-level/component verification bindings. Both reuse Context executor and stable read contracts; their domain-specific result shapes differ. There is no existing generic full workflow that supplies the new output. Extracting a generic orchestration framework would add machinery, not remove an unearned owner.
- Existing server `Spec::binding_sites` remains the registration/validation authority. The UI enumerates the three legal declared binding positions for display and does not create/resolve/validate bindings. Calling the server's internal Rust traversal from the UI is neither an existing public capability nor required. This task adds no replacement backend binding workflow.
- Existing `WyrdView` owns cancellation and superseded panel loading; new catalog entries use it directly. Existing Node/MCP executors, detail route, CardHeader, CardFacts and shared action intents remain the integration owners.

## Proposed finding

### MNT-001 — VIOLATION: new component contracts are absent from the candidate

**Obligation:** `brand/DESIGN.md:259–262` explicitly requires adding each new component to `brand/components.json`, marking the built implementation `built`, and testing it. The repo UI skill's component workflow additionally requires its catalog flag to match the generated catalog. TASK-001 requires changed catalog/contracts to regenerate normally; REQ-014 extends the existing shared component catalog.

**Locations:** candidate `brand/components.json:27` (`components` owner; lacks all three new names), `packages/ui/src/lib/index.ts:17–19` (exports the three implementations), `packages/ui/src/lib/schemas.ts:521–535` (two placeable components).

**Evidence and reachable consequence:** a fresh checkout of `dd98e9ef7` has implementations and the generated view catalog for `VerifierDefinition` and `RunsOn`, and exports `VerifierGraph`, while the machine-readable brand contract has none of these entries. The file describes itself as consumed by A2UI/agents and names per-component props/tokens/rules. A maintainer or agent using that authoritative component inventory cannot find the newly shipped components or their intended constraints. This violates the explicit required component delivery workflow, regardless of the existence of uncommitted entries in another working tree.

**Materiality:** this is a required authoring/delivery contract, not naming or preferred decomposition. The immutable candidate cannot borrow unrelated pending workspace edits to pass it.

**Smallest correction:** include the three entries in the existing `brand/components.json` owner, reflecting actual props, anatomy and states, marking implementations built, and matching catalog placement (`VerifierDefinition`/`RunsOn` placeable; `VerifierGraph` a graph primitive). Preserve the current brand document's unrelated contents and shared generator behavior. Do not introduce a second inventory or carry an unrelated brand rewrite into this task. This is a bounded data change; no product or API decision, new dependency, helper or framework is required.

**Focused closure proof:** inspect all three entries at the new immutable candidate, compare them to the corresponding exported implementations and existing generated catalog, and run the narrow existing brand/catalog checks affected by those entries. Existing implementation tests already exercise the components; no broad journey suite is needed for this contract-only correction.

## Verification limits

This is source/diff and test-quality review; no command results are claimed as independently rerun. Existing task evidence was inspected as claims, including its ungreen full integration command and stale visual shots. No visual acceptance was granted. These limits do not create additional maintainer findings or expand remediation scope.

No other material maintainer finding was established. Optional refactors and speculative graph hardening were omitted.
