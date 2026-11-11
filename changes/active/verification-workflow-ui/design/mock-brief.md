# Verification mock: retained composition and correction rationale

Status: implemented in `design/mock/` and approved by the user.

This supplement preserves the source comparison and rationale for the accepted
UI. It is not a new implementation assignment. The three superseded prototype
folders were removed with authorization; only `mock/` remains.

Final evidence after the standalone-Repositories correction: 28 real-browser
checks passed independently, including visible navigation from default Verify
through Repository policy/activity, GitHub Settings and no-Card instructions.
The screens were inspected as well as reached by test. All runtime
requests stayed inside `mock/`. Before authorized cleanup, all 46 recorded
original canvas/previous prototype files matched their checksums. The original
canvases remain; the superseded executable prototypes have now been removed. The main list retains Service and uses internal
scrolling for readable columns. Wide PR diagrams fit at about 72% at 1440px,
with zoom available for closer inspection. Backend responses remain simulated.

This brief records the user's latest directions and source comparison. It
supersedes the rejected consolidation's presentation and packaging choices.
It does not authorize production work, a new product design, or specification.

## Deliverable and fixed navigation

Build ONE complete mock in `design/mock/`, with one entry point and shared Wyrd
shell. All mock pages, styles, fixtures and runtime assets belong there. No
navigation, iframe, stylesheet, script or data dependency on the sibling mocks.
The build may use the existing installed Wyrd UI toolchain; bundle brand/fonts
into its distribution. Keep the original canvases as references; the approved centralized mock
supersedes the removed executable prototypes.

Default: Verify discovery. Selecting an execution opens its workflow PAGE,
with a way back that retains discovery state. Preserve the original quick
inspection drawer as a secondary explicit quick-view action if reused; row
selection must not require expanding a drawer to reach the page.

Latest user correction supersedes the earlier navigation assumption:
Repositories is a separate visible primary-nav destination. Cards already has
an existing UI; do not invent a reduced Repository/Verifier Cards catalog to
host this work. Preserve the exact Verifier definition view linked from
executions/bindings, and leave general Cards browsing to its existing UI.
Repository identity remains a native Card regardless of workspace placement.
Repository index/details, GitHub Settings, Verify, execution pages and PR review
context are navigable within the same mock, without sibling-application hops
or preview-control-only access. Settings owns integration administration.

## What remains approved

| Surface | Preserve from source | Agreed additions |
|---|---|---|
| Verify discovery | Original Verify's WHERE/zoom/range/Live toolbar; compact adjacent linked histograms; concise legends; aligned axes; Group by; facet rail; dense ruled table. Resemble the actual Observe Traces investigation board. | All execution statuses, separate status/verdict filters, repository/PR context, exact execution links; selected execution opens its workflow page. |
| Ordinary verification | Original inputs, steps with attached checks, after-all requirements, reason/actual/check, exact trace citations and collapsed input content. | Actual styled Svelte Flow, generic instance/output inspection where needed. |
| Session verification | Original chronological pinned member runs, two session requirements, selected recorded reason/judge and exact member/answer citations. | Same workflow page/inspector; clean aggregated dependency presentation and member navigation/return. |
| Drift verification | Original compact samples-to-requirement graph, previous/next windows, exact baseline, reason, feature table, score history and dashed threshold. | Every recorded feature, recorded verdicts and exact window links; same shared execution components. |
| Generic/PR workflow | Approved HTML workflow mock's execution identity/context header, graph above inspector, custom node anatomy, parallel branches, attached checks, repeat/fanout and exact instances, readable/source output view. | Svelte Flow with corrected layout/routing; all outputs equal, intermediate and completed examples; waiting/approval/cancel. |
| Repository Cards | Approved repository mock's index, access/no-Card/readiness cases, exact versions, selected policy vs viewing, Verifier bindings, activity, read-only YAML/TOML, provider links and declaration instructions. | Separate Repositories workspace in the same mock shell; execution/Verifier/filter links stay internal. |
| GitHub Settings | Approved repository mock's connect/callback-completed association, account/App/access, manage access, disconnect with retained Cards/evidence, access lost and viewer states. | Same shell and routes; no separate installation-management product. |
| Verifier Card | Original Verifier definition header, Graph/YAML, Runs on and Results in Verify. | Shared graph without runtime state; exact version links/filtering; view-only version selection. |
| PR review navigation | Agreed latest review set by default; explicit earlier-set selection; multiple Verifier executions per set; exact execution links remain pinned. | Make this already-agreed behavior visible in the mock. PR context selects executions and then uses the generic execution viewer. No second code-review engine, Verifier kind, findings schema or specialized DAG. |

Workflow authoring remains YAML/SDK. Repository policy remains normal TOML/YAML
declaration. No visual editor, activate/pause/rollback button, primary output,
autofix/merge action, new chart type, tab system, dashboard KPI cards or invented
workflow feature is requested.

## Retained diagnosis of the superseded prototypes

1. **Fragmented delivery and hidden approved UI.** The previous agent brief
   explicitly limited repository work to links in three files and kept its
   pages in a separate repository prototype. The rejected app shell left Cards inert and
   sent Repositories into a sibling application. PR executions existed but
   became ordinary scattered rows. Root correction: one shared shell and route
   family containing all approved surfaces; obvious in-product links/examples.

2. **Chart/layout drift.** The previous brief explicitly asked for only a few
   dozen records. the rejected fixture catalog clustered almost all examples near
   the end of a 24-hour range; its 24-bucket chart was nearly empty with a final
   spike. `Timeline.svelte` allowed verbose identity/count legends to wrap,
   misaligning the plots; `app.css` grew the chart area. Original Verify uses
   compact 56px histogram bands with clear adjacent series, short legends and
   tight spacing. Restore that composition. Use coherent deterministic sample
   records across the range so the chart demonstrates the approved density;
   charts/facets/list totals must describe those actual records. Paginate or
   scroll the dense table honestly rather than inventing counts or drawing
   disconnected bars. Do not reproduce the old static reference's inconsistency
   between its illustrative WHERE and displayed counts.

3. **Detail hierarchy lost.** Original graphs explicitly label Inputs, Steps,
   After all steps; session uses Member runs / Requirements. The consolidation
   replaced these with one generic sentence. Fixed-height graph panels also
   overwhelmed simple drift checks. Restore context-appropriate stage labels,
   compact simple graphs and readable nodes, retaining the same renderer.

4. **Bad edges are custom geometry, not an inherent Svelte Flow limitation.**
   `DepEdge.svelte` constructs cubic curves through Dagre interior bends, with
   horizontal tangents at each bend. `layout.js` discards Dagre endpoints;
   node components expose one center handle per side and edges have no
   `markerEnd`. Therefore many curves converge at one point without directional
   arrows. Session expands each member into an edge to each requirement (8
   curves for 4 members/2 requirements), despite the original showing a clear
   shared dependency spine. Extra invisible sink-to-requirement rank edges
   create long spans in the approval example. Correct routing and attachment
   together, not just curvature or spacing. Preserve actual dependencies; a
   presentation bus/ports may show fan-in/out without adding a runtime node.

5. **State color can imply unsupported causality.** Every incoming edge to a
   failed session requirement becomes red because `DepEdge` checks only the
   target outcome. Passing members then appear to fail. Keep member verdicts
   and session judgment distinct. Selection emphasis and failure indicators
   must not claim a member caused the session failure.

6. **Agreed PR grouping was absent.** The consolidated router and execution
   page have no review-set selection despite the recorded decision. Implement
   explicit fixture membership, latest-by-default general PR links, historical
   selection and exact execution links. Include both a new head and a rerequest
   on the same head so a SHA alone cannot impersonate review-set identity.

7. **Verification checked behavior but not fidelity.** Earlier passing checks
   compared counts/interactions and no-overflow, not the approved composition.
   Visual inspection must compare actual references with the delivered pages;
   passing assertions alone is not a completion claim.

## Sources to inspect visually and in code

Paths below are relative to the repository root unless absolute.

- `changes/active/wyrd-ui-main-integration/references/canvas/`
  `{Verify,Verifier,VerifyDrift,VerifySession}.dc.html` and corresponding `png/`
  images. The copies in this packet's `references/original-canvas/` are identical.
- `crates/wyrd/wyrd-server/wyrd-ui/src/routes/t/[tenantKey]/observe/+layout.svelte`:
  toolbar, compact chart band, list controls/facet rail, spacing. Reuse the
  investigation grammar, not trace-specific duration/error charts in Verify.
- `design/mock/src/{ExecPage,Graph,Inspector,Preview}.svelte`: accepted workflow,
  instance, judgment and output presentation.
- `design/mock/src/{Repositories,RepoDetail,Undeclared,Settings,PullPage}.svelte`:
  accepted standalone Repository workspace, PR review context and GitHub settings.
- `design/mock/src/{Verify,Timeline,VerifierPage}.svelte`: discovery, compact
  linked charts and exact Verifier definitions.
- Current `brand/DESIGN.md`, `theme.css`, local logos/fonts and `wyrd-ui` skill.
- `design/notes.md` is the finalized design; latest user directions take precedence.
- Backend worktree `/home/thorrester/Documents/GitHub/wyrd-worktrees/code-review-workflow`,
  HEAD `3ca43ae03` at recheck: `changes/active/github-verifier-pipelines/spec.md`
  remains revision 3 approved. This research rechecked declaration/connect,
  selection, admission and supersession text; it is not a claim of runtime
  integration. `design/source-alignment.md` carries earlier deeper owner research.

## Concrete required examples and interactions

The Verify list must make ordinary policy, session, drift, running/completed
PR review and approval-wait workflows reachable through actual rows and filters.
A compact preview-only examples selector may additionally aid review, but must
not be the only route. Keep preview controls subordinate and technical stack
labels outside product UI. Provide meaningful sample data over the chart range.

Show queued, running (including waiting), completed, cancelled, errored and timed
out work. Status is independent of nullable recorded verdict. Keep compact
passed/failed/inconclusive charts and neutral unjudged executions distinguishable
without treating no-verdict as a fourth verdict. Status/verdict/Verifier/failed
check/Card/service/run/session/repository/PR filters combine; both timelines and
facets/table follow the same population. Keep short legends and aligned plots.

Workflow pages show metadata, exact links, graph above inspector, step/requirement
selection, outputs and citations. Preserve item/iteration/attempt producer
identity, safe Markdown/JSON/directory previews, equal output treatment and
retained evidence after cancellation. No final-report shortcut or auto-selection
on live updates. Keep state/viewport stable during updates.

Session member runs are chronological and pinned; selection opens its exact
execution, citations open the cited answer/trace, and Back restores session
selection. A session judgment is recorded evidence, never a grouped-row aggregate.
Drift windows retain exact result/baseline and recorded feature verdicts; chart
history includes threshold and selected-window indication.

Waiting for an input and waiting for approval remain distinct; running peers
continue. Operator can Approve/Reject with optional reason; viewer cannot.
Show exact occurrence/expiry and simulated pending/accepted/conflict/expired
states. Cancel unfinished work retains outputs and handles a result winning the
race. Do not add further action machinery beyond the existing mock capability.

Repositories: registered + ready, registered + access removed, access granted
but not configured, and accessible provider repository with no Card. Header,
version selection, policy/bindings, activity and declaration content stay like
the approved mock. Viewing an old version never changes selection. Connection
alone never creates a Card or activates its Verifiers. Full GitHub Settings
connected/disconnected/access-lost/admin/viewer and disconnect behavior remains.

PR context shows repository, PR/head, latest set and prior sets, membership and
each execution's status/verdict separately. No invented combined verdict. A
current link follows latest set; explicitly chosen history stays pinned. Use
the same generic workflow page for each selected member.

## Graph presentation requirements

Use real Svelte Flow with custom Wyrd nodes and automatic layout. Prefer existing
Dagre and installed Svelte Flow routing/helpers over another graph dependency.
The private routing algorithm can change, the dependency graph cannot.

- Direction is visible with arrowheads terminating at node boundaries.
- Nodes/columns align, titles/kind/status fit, parallel work remains parallel.
- Routes avoid node interiors, labels and double-back/S-shaped tangles. Separate
  ports or clear shared lanes handle fan-in/out. No all-members-to-all-checks
  spaghetti in the session sample. No weaving across the approval branch.
- Final requirements stay in an intelligible stage; no huge gaps or massive
  fixed panel around a single drift requirement. Include stage headings.
- Selection does not reorder nodes; live progress never relayouts them.
- Fit is useful at desktop width; zoom and pan work, graph stays inside its
  panel on mobile. Preserve keyboard selection and visible focus.
- Inspect the exact reported samples: session `exe-6c19e5f0`, waiting
  `exe-4d1e9a07`, PR `exe-7f3a91c2`, ordinary `exe-7c1e02a4`, drift
  `exe-d1f7a00e`. The user's screenshots are `/tmp/codex-clipboard-R85soR.png`
  and `/tmp/codex-clipboard-VtZ6mJ.png`; compare them before and after.

## Scope and proof

The approved mock lives only in `design/mock/`. Its README records build and
browser-check commands, local toolchain dependencies and limitations. Original
references and production files were not edited to implement the mock.

Checks exercise internal navigation and exact links, ordinary/session/drift/PR/
approval examples, filter/chart/list consistency, review history, full Repository/
Settings routes/actions, dark/light/mobile, safe outputs and graph geometry.
Independent visible-click inspection ran Verify → Repositories → payments-api
policy/activity → Manage connection → GitHub Settings → Repositories → no-Card
instructions. Runtime requests stayed inside the mock folder. The user then
approved the centralized UI; automated checks alone did not establish that approval.

All responses remain simulated. This approved visual direction is input to
`$wyrd-spec`, not evidence that backend operations or production journeys ship.
