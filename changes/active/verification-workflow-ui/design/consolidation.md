# One verification UI

Status: finalized; centralized mock approved by the user.

[`mock/`](mock/README.md) is the sole approved mock. The original canvases are
retained; superseded prototypes were removed with user authorization. The
[retained rationale](mock-brief.md) explains earlier corrections without making
those discarded drafts competing references. The existing Cards UI stays
outside this work; Repositories is a separate workspace.

## Structure

One Verify destination owns discovery, filtering and execution investigation.
Use the original Verify dashboard as the visual foundation and the approved
Svelte Flow graph/inspector as its execution-detail implementation.

```text
Verify — WHERE · time range · Live
  linked timelines · facets · grouping · execution table
    select execution → workflow page → return to discovery state
      shared workflow graph → selected-node inspector
        requirement judgment / outputs / drift detail / approval

Exact Verifier definition → graph / YAML / bindings (no new Cards browser)
Repositories → exact Repository Card / policy / activity / PR review sets
  → the same filtered Verify or exact execution
Settings → Integrations → GitHub
```

Latest user correction: Repositories is a separate visible workspace. Cards
already has its own UI and is not redesigned here. The mock's former two-kind
Cards index was an incorrect invention, not an approved navigation pattern.

Definition and execution are different contexts for the same graph renderer.
Session and drift are contextual data/detail renderers, not separate top-level
verification products or new Verifier kinds. Keep declared checks and recorded
execution state authoritative; never infer a verdict from a graph or score.

## Consolidation map

| Existing reference | Retain | Integrate with the approved extension |
|---|---|---|
| Original Verify | WHERE, range/Live, two compact linked timelines, facets, grouping, dense list and return state | List every execution status; add status and relevant repository/PR filters. Latest instruction: row selection opens the workflow page directly. Timelines/counts include unfinished work honestly, with no recorded verdict distinct from failed/inconclusive. |
| Original Verifier | Exact Card/version, description, Graph/YAML, where-it-runs, Results in Verify | Render its declaration through the shared styled Svelte Flow component without runtime state. Results in Verify filters exact Verifier identity, not a second results tab. |
| Original ordinary judgment drawer | Inputs → steps → final requirements, reason/actual/check/citations, collapsed inputs | Use the shared execution inspector and graph; short/simple workflows need no loop/attempt controls. Preserve exact trace/span navigation. |
| Original VerifySession | Session grouping, exact pinned member executions, recorded session judgment and member citations | Selecting a session judgment opens that execution. Member references and session requirements occupy the same contextual inspection system; grouping does not manufacture an aggregate verdict. |
| Original VerifyDrift | Window navigation, exact baseline, recorded feature scores/thresholds/verdicts and score history | A selected drift check opens the specialized requirement detail within the same inspector. Do not replace its statistical view with generic JSON or invent baseline distributions. |
| New workflow/Svelte Flow previews | Live graph, parallel/repeated steps, exact instance selection, intermediate/all outputs, stable viewport | Integrate these into the original dashboard/workflow-page journey instead of maintaining a workflow-only verification page. Include agreed waiting, approval and cancellation states. |
| Repository preview | Versioned Cards, access/readiness distinction, bindings and verification activity | Repository/PR links enter the same Verify filtering/execution selection. Connection management stays in Settings. |

## Approved dashboard behavior

The dashboard is the landing view with no execution selected. Preserve the two
linked timeline regions and original visual hierarchy. Adapt the first chart to
the all-status execution population: recorded passed/failed/inconclusive outcomes
and a clearly neutral **No recorded verdict** series. That last series is a
population bucket, not a fourth verdict; queued/running/waiting/error/cancelled
executions must never be painted as failed judgments. The second chart remains
by Verifier. Both use the same filtered execution population and time range.

Keep status and verdict as independent filters/columns. Retain existing subject,
Workload, application Run and session context, with repository/PR context where
present. Do not replace general verification filters with repository-only ones.
Opening an execution preserves list filters/range/group/position. Workflow navigation and
copy-link preserve exact execution/selected-result identity. List grouping is a
navigation affordance, not a new execution/result or a source of authority.

The workflow page uses the shared graph and inspector. The replacement mock
does not require a drawer/Expand step to reach it. A requirement's check determines its useful
detail: assert/code/agent/trace retain their actual/reason/citations; drift adds
the recorded statistical table/history; approval adds the agreed authorized
decision controls. Ordinary step outputs still list equally by exact producer.

## Consolidated visual deliverable

The [complete verification mock](mock/dist/index.html) is the sole approved
visual target, with every route and runtime asset in `mock/`.
Its landing dashboard contains mixed representative executions:
ordinary policy verification, a running generic PR workflow, waiting for approval,
a session judgment and a drift-window judgment. Click each from the same table.
It also demonstrates the exact Verifier definition and navigation back into Verify.

The end-to-end visual review must exercise: dashboard → workflow page → select
requirement/output → return with filters preserved; drift previous/next window;
session member citation; definition → filtered execution history; repository →
the same execution. Inspect desktop/mobile and both themes. The replacement's
updated 28 browser checks passed independently on 2026-11-10; its README records
the final focused commands and limits. They exercise sample records and simulated server
responses; they do not establish production API or authorization behavior.

The user approved the combined experience. The original canvases and design
record remain for traceability; only `mock/` remains as an executable mock.
Next is specification, followed by human spec approval and task planning.
