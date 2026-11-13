---
id: TASK-002
kind: implementation
status: proposed
spec: SPEC-verification-workflow-ui
spec_revision: 3
requirements: [REQ-002, REQ-004, REQ-005, REQ-006, REQ-007, REQ-009, REQ-014, AC-002, AC-003, AC-005, AC-008, AC-009]
depends_on: [TASK-001]
---

# Exact execution, publications, session and drift inspection

## Outcome and prerequisite

Opening `/t/[tenantKey]/verify/executions/[executionId]` shows that execution's
workflow, current recorded state, judgments and retained outputs. Selecting a
node shows its recorded details; refresh preserves graph position, viewport and
exact producer selection. Session and drift use the same inspector. Cancel
uses the existing action. TASK-003 supplies discovery later; no list or PR
admission is required to open an existing execution.

Consume TASK-001's graph and existing backend operations recorded in the
[finalized read map](../design/contract-alignment-research.md). Record their
integrated revision and real host proof. This is UI work: components, Bifrost
queries, typed UI adapters, navigation and tests. No server fields/endpoints,
SDK wire-contract changes, query services, storage or execution logic. An
undelivered read/host capability remains an explicit prerequisite, never a
fixture substitute or permission to implement the backend here.

## Data and existing owners

| Content | Existing source | UI change and consumer |
|---|---|---|
| Graph and description | Exact Verifier Card from the Postgres-backed registry through existing Card APIs | Resolve its exact reference/UID, then reuse TASK-001's graph and exact Card content. No direct browser database access. |
| Current lifecycle/error and selected judgment | Existing execution status; compact VerificationResult | Shared execution context and requirement presentation; lifecycle and verdict stay separate. |
| Completed steps/checks/provenance/evidence | Existing explicit Execution.details operation | Selected-node inspector; do not assume compact result contains these fields. |
| Attempts and accepted progress | Existing bounded invocation pages | Exact step/item/iteration/attempt selection and recorded speculative disposition. |
| Outputs | Existing publication pages, exact publication/manifest and file/content/download operations | Equal named metadata first; selected body only, with bounded safe previews and normal host download. |
| Correlation, retained lookup and history | Existing permitted Bifrost results/inputs/outputs/events queries | Parameterized bounded UI queries, preserving server-selected result identity rather than choosing the latest result row. |
| Session/drift/citations | Recorded group snapshot, requirement drift detail, executed provenance/window, existing Bifrost history and exact trace reads | Contextual inspector with recorded members, features, scores and thresholds; never recompute a verdict or membership. |

Extend `packages/ui/src/lib/{contract,loaders,node,mcp}.ts`, catalog/view exports
and host dispatch. Deliver the missing runtime requirement/step/session/drift
panels within that existing family; they are not already implemented components.
Reuse Chart, generic panels, existing exact trace inspection and TASK-001's
projection/layout rather than creating a second loader or graph owner.

For a completed execution, its result supplies exact Card references. Before
that, existing Bifrost execution metadata exposes `verifier_card_uid`; existing
HTTP/MCP Card reads accept kind and UID. Wire the missing UI host adapter to
that existing operation. TypeScript Cards.get currently accepts a versioned
reference, not a bare UID: do not pass an unsupported selector. Reuse an exact
retained CardRef where available or the existing UID-capable host operation.
Until identity/definition is visible or authorized, explain that availability
without erasing readable execution state or inventing a different declaration.

Adapt the shared graph projection to integrated Step contracts: dependency
lists and any_succeeded, foreach, repeat and speculative controls. Include their
declared external references and annotations while retaining one node per
authored step. Runtime state never owns layout. Preserve the existing exact
Verifier Card view with no runtime coloring; TASK-001 stays complete.

## Interaction and failure behavior

Compact results retain requirement judgments, actual/citations/custom data and
check-specific drift detail. Load full details explicitly when needed for the
inspector, using authored recorded data without compiling Prompts or invoking
providers. Details are completed records, not the progress source or a wait loop.
Keep state, judgment, details and output availability independent. A live/direct
result does not certify retained visibility; normal refresh may reveal records
later. Unknown, foreign, denied, unavailable and not-yet-visible responses must
remain truthful. Do not treat every 404 as running, block a BFF load on wait,
resubmit work or issue dependent sensitive reads after a denied prerequisite.

Use generated producer identity, including nullable item/iteration and one-based
attempt. Preserve exact execution/result, step/requirement, producer and output
in navigation/copy links; a stale response cannot replace newer selection.
Respect opaque cursors and bounded pages. Do not replace selected publications
with StepResult.output, a final-step shortcut or a primary-report heuristic.
Preview bounded JSON, safe text/Markdown and directory manifests; never execute
HTML or download a directory merely to list it. File access stays with hosts.

Use the selected execution's recorded group snapshot for ordered session members
and restore selection when returning. Keep drift window/method/baseline facts,
all recorded features, scores/thresholds and bounded history from the existing
reads; if context is missing or refused, say so instead of adding a server read
or inventing it from current Card status. Later spans do not revise judgments.
Exact citations use current trace permissions independently of list filters.

Host cancellation guards duplicate pending actions and reads back existing
status; terminal completion can win. Retained progress/outputs stay readable.
Waiting/approval decisions belong to TASK-006; provider delivery to TASK-007.

## Observable proof

At the existing UI unit owners, prove graph controls and stable selection,
compact-only judgment rendering, explicitly loaded details and independent
refusals, exact producer/output changes and safe/oversized previews. Prove that
the shared projection still renders the definition page after Step adaptation.

Through existing public SDK/server setup and the production BFF/Chromium page,
open an exact execution while unfinished, select an intermediate publication,
refresh and reopen after settlement. The page and shared host component must
show the same recorded identities. Include no-result and delayed-details states;
loading or refusing details must not hide other readable content. Inspection
must not invoke a provider or create replacement work.

Exercise owner-distinct pinned sessions, drift history, out-of-range/exact trace
citations, later spans, foreign/denied/unavailable reads and cancel/completion
races. Repeat affected adapters in Node/MCP/custom/built-artifact hosts. Compare
ordinary/session/drift and wide graph/inspector regions with the approved mock,
both themes and narrow/wide layouts, keyboard/focus/touch. Fixtures supplement
these real journeys. Existing scoped operations and Bifrost permissions must
produce the refusals; UI filters are not authorization.

## Execution and verification

This task is against approved spec revision 3, whose scope transfer is
synchronized. Activation requires only this task's named contract/authority
and predecessor outputs. Unrelated backend delivery
or authority amendments do not block it. This packet consumes existing read/action
capabilities and adds no underlying server changes; a missing required capability
remains an explicit external dependency. Record the integrated source revisions
and actual dependency journeys it consumes; upstream task status is not proof.

Read the repo-local `wyrd-ui` skill before UI edits. Reuse the shared component,
loader, catalog, Node/MCP executor and host action owners. Test shared components
in `src/lib/ui-library.test.ts` and pages in `src/lib/ui-pages.test.ts`;
authorized host/built-artifact proof belongs to `ui-library.integration.test.ts`
and production BFF/Chromium/public-login proof to `ui-pages.integration.test.ts`.
Record actual focused selectors for added tests before their first run; follow
the existing TDD discipline without inventing test names in this task.

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts src/lib/ui-pages.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise exec -- scripts/postgres/with-test-postgres.sh -- env WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts src/lib/ui-pages.integration.test.ts
```

Narrow test files with actual relevant selectors. The integration command needs
the existing `ts:build`, `ts:build:testing`, `build:server` and UI-build
prerequisites; inspect `mise.toml` and the managed Postgres wrapper before
running it. Missing prerequisites are not passing or skipped proof.

Extend normal `mise run dev:full` public setup where needed to demonstrate this
task's real inputs. Independently compare each introduced component/page with
the approved mock on desktop/mobile, light/dark, keyboard/focus/touch, long
content and error states. Record reference regions, screenshots and individual
human acceptance. Fixtures and automated checks alone do not prove fidelity.

Use the existing UI integration owners to prove real Bifrost/domain inputs through
Node/MCP/BFF/custom/built-artifact adapters. Do not add or change server/SDK wire
contracts or their generators. Regenerate the UI catalog when schemas change and
run `mise run codegen:check` where applicable. Full shipped-surface journey lanes
belong to integrated change review; focused UI/host journeys prove this task.
