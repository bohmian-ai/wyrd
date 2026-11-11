# UI integration delivery plan

Approved specification: [revision 28](spec.md), 2026-11-06. Status: proposed
implementation assignments. The caller approved consolidation and paired
component/page delivery; planning establishes no implementation or visual PASS.

The reusable `wyrd-ai-ui` library and gallery remain the primary deliverable.
Every new component is delivered with its first working conventional consumer.
After a completed slice, `mise run dev:full` exposes that feature with real
representative data at the task's documented route. The existing shell, session
BFF, Node/MCP executors, Observe and real-browser runner are reused.

## Eight remaining delivery tasks

Numbers identify the new assignments below. Older task IDs in immutable reviews
refer to the old filenames and reviewed revision, not to these reused numbers.
The [transfer record](references/consolidation-coverage.md) maps every removed
assignment and each component to its current owner.

| Task | Outcome and first conventional consumer | Real dependencies | Completion gates |
|---|---|---|---|
| [005 — Cards and Prompts](tasks/TASK-005-cards-and-prompts.md) | Registry, exact generic Card detail, Prompt workspace; shared Card/Prompt/artifact components | Completed foundation, Observe/shell and view-layout baseline | Individual inventory/detail/Prompt visual acceptance; existing registered Files and derived Prompt values are in-scope UI wiring |
| [006 — Query workbench](tasks/TASK-006-query-workbench.md) | Table catalog, SQL/typed parameters, bounded results/charts, cancellation, details and History | Completed foundation and Observe/query-inspector baseline | Separately approved History source/lifetime before its scenario; no invented browser/server persistence |
| [007 — Verification](tasks/TASK-007-verification.md) | Results/drawers/session/drift and exact Verifier definition; four shared verification components | 005 and completed Observe | Non-sensitive requirement-outcome contract for affected facets/detail; never elevate payload permission |
| [008 — Card workspaces](tasks/TASK-008-card-workspaces.md) | Agent, Model and Data with separate checkpoints; composition/interface/bindings components | 005, 007 and completed Observe | Existing required outcome projection; reuse 005 Files tab |
| [009 — Changes and Repositories](tasks/TASK-009-changes-and-repositories.md) | Change list/detail/verification/review and repository configuration, each independently inspected | 005, 007 and completed shell | REQ-029 approved/shipped platform contracts and actual SDK/MCP read/write projections; entire contract-dependent workflow is gated |
| [010 — Service workspace](tasks/TASK-010-service-workspace.md) | Service overview/charts/versions/YAML with signals, attention and chart authoring components | 005, 006, 007, 008, 009 | REQ-025 charts and REQ-029 Changes contracts; no fixture completion |
| [011 — Home dashboard](tasks/TASK-011-home-dashboard.md) | Inventory summary, attention, Services and inline Observe at tenant Home | 005, 007, 008, 010 | Applicable chart/attention/Changes prerequisites inherited from their owners |
| [012 — MCP App completion](tasks/TASK-012-mcp-app-completion.md) | Production resources/tools and complete connected-agent/browser investigation | 005–011 complete, retained foundation/Observe/layout proof | Complete library/actions; approved shared metrics/histogram projection; actual compatible connected host and local agent credentials |

These edges govern consumed outcomes; numbering alone is not a dependency.
A consumer may start once the specific shared components/routes it consumes
have passed their focused checks and individual human checkpoint, even if an
unrelated external gate keeps the producer task open. Record that accepted
producer outcome explicitly; do not relabel the entire producer complete. For
example, Verify consumes 005 exact Card routing/facts/relationships. Query can proceed independently of Cards. A conditional scenario cannot be activated by
an implementer's choice of a new public or persisted contract. Independent
already-specified work may proceed, but a task remains incomplete until every
required checkpoint, prerequisite and human inspection closes. Do not wait for
one missing platform contract to start an unrelated slice. Partial progress
must be reported by outcome; never report a conditional whole task complete.

## Shared delivery and proof rules

- Components receive typed props from generated current wire types. Shared
  portable loaders own request planning and shaping; executors own authorized
  IO, and hosts validate/execute typed action intents. Pages own route/layout
  and interaction state. No page SQL, new registry, graph store, authorization
  owner, transport, cache, orchestration framework or fixture fallback.
- Each task adds its first conventional consumer and a `dev:full` checkpoint.
  Extend the existing canonical setup through public operations only where
  representative data is missing. Record source/wheel/data identity and actual
  readback; neither successful startup nor the existing 500-request count proves
  that the new feature has populated data. Development data is ephemeral across
  launcher restarts, and setup failures cannot count as populated proof.
- Preserve T1 component states, T2 actual request/shape/cancellation assertions,
  T3 catalog/view validation, T4 actual built AppBridge artifact, T5 authorized
  Node/server/Postgres journeys, and T6 production-BFF real-browser page
  journeys. Lower tiers, fixture hosts and imported route handlers cannot
  replace the live proof. Ordinary runner selectors and existing managed setup
  remain the verification owners; each task contains its exact focused commands.
- Component-specific Node/MCP/catalog/action closure happens in the same slice,
  not in a final portability repair. Production MCP wiring may progress with a
  slice when supported; final 012 proves the complete resources/tools and live
  connected conversation. Earlier AppBridge fixtures do not prove production
  registration or a connected agent.
- Every remaining task directly links its exact approved `.dc.html` canvas
  files in its Approved Canvases section. A canvas-directory link or board name
  alone is insufficient. Follow those approved designs; implementation inspection
  verifies fidelity and does not reopen design approval.
  Missing or contradictory visual details require maintainer clarification;
  implementers must not guess layouts or invent replacement designs.
- Every catalog component and distinct page has an individual desktop/mobile,
  light/dark checkpoint with keyboard/focus, touch, long content and overflow.
  A grouped task never receives one blanket visual acceptance. Record human
  inspection separately from automated behavior and screenshot comparisons;
  wait before the next dependent slice. Uninspected work remains unaccepted.
- Use the twelve kinds in the architecture kind catalog. Policy and Source are removed; this is settled scope, not a new prerequisite. Detailed requirements/drift are currently
  sensitive `execution_outputs.output`; `results` is payload-free. Preserve
  exact result identity and viewer permission; no retired drift detail table.
- Ask, saved views and runtime A2UI stay deferred. Non-Svelte/custom applications
  keep their existing component/view entry points and regression obligations.
  This change creates no production agent service or new platform prerequisites.

## Completed and historical work retained

| Existing record | Disposition |
|---|---|
| [001 distribution/login](tasks/TASK-001-distribution-login.md), [002 deletion](tasks/TASK-002-remove-old-ui.md) and their [session remediation](review/tasks-001-002-843484f6c/TASK-001-R1-installation-browser-sessions.md) | Retain completed work and immutable evidence; final native/image/nginx proof remains a change-review duty |
| [003 foundation](tasks/TASK-003-shared-ui-foundation.md) | Complete; reuse Chart, CardHeader, primitives, package/catalog/view/executors and tests |
| [004 Observe](tasks/TASK-004-observe-components.md) and [revision 01](tasks/TASK-004-revision/TASK-01-mockserver.md), [02](tasks/TASK-004-revision/TASK-02-live-observe-workflow.md), [03](tasks/TASK-004-revision/TASK-03-local-users-personal-keys-and-observe.md) | Caller reports complete. Keep original evidence/status fields as historical, rather than retroactively editing them into new PASS claims. Includes former 012/020/021/022 shell/Traces/Logs/Metrics and personal Settings |
| [004 revision 04](tasks/TASK-004-revision/TASK-04-bifrost-bloom-pruning.md) | Separate bounded platform proposal; no execution authority inferred from consolidation and no new 005+ dependency |
| [027 view layout](tasks/TASK-027-view-layout.md) | Complete; preserve accepted placement/resizing and renderer behavior |
| Old 010 Ask/saved views | Deferred, no active task or dependency; scope retained in spec |

Previous task-review and readiness verdicts keep their reviewed revision and
source identity. Revision 28 does not rewrite them. Current tasks do not depend
on a temporary archive. Removed assignments remain recoverable from git using
the provenance and hashes in the transfer record.

## Obligation ownership

The acceptance tables inside each task map individual scenarios to proof. This
map establishes whole-change coverage, including retained/deferred scope.

| Specification obligations | Owner and proof |
|---|---|
| REQ-001–006/033; AC-001–005/012; INV-003/005 | Retained 001/session remediation: native install/lifecycle, opaque sessions and native/nginx login matrix |
| REQ-032 | Retained 002: deletion and login regression |
| REQ-007/015–017/021/028/037–044; AC-020–027 | Retained 004 revisions: shell, Observe, SQL predicate, canonical development workload, real humans/Settings/personal keys and individual browser/visual evidence |
| REQ-045–061; INV-008/009; AC-028–034/036–040 | Retained Observe revisions/current remediation evidence: provider failures, realistic logs/metrics, histograms, series identity/index and latency; AC-035 removed by approved revision 25. Revision 21 defers the shared MCP histogram projection to new 012 |
| REQ-008/009/011/024; AC-006/007 | 005: inventory, exact detail and Prompt; components/gallery plus actual registry and page journeys |
| REQ-014; AC-008/016 | 006: Query, discovery/descriptions, terminal/bounded result fidelity, cancellation, actions and History gate |
| REQ-018/026/027; AC-009/019 | 007: recorded/waiting distinction, results/requirements/session/drift/Verifier definition and exact citations; non-sensitive projection gate |
| REQ-010/012/024; AC-007/017/019 | 008: Agent/Model/Data definitions, bindings, exact baseline/subject attribution and truthful activity/drift |
| REQ-029 | Separately approved platform prerequisite; 009 consumes it for Changes/Repos and closes actual read/write host projections; 010/011 reuse required CR/attention regions |
| REQ-013/023–025; AC-007/017 | 010 consumes existing workload/subject attribution and external versioned chart contract; Service UI and shared signals/attention/editor |
| REQ-019; AC-009 | 011: all required Home regions and their separate summary checkpoint, no Home computation service |
| REQ-030; AC-011 | Retained foundation/Observe plus 005–011 deliver inventory, portable loaders, Node/MCP/custom app, catalog and actions; 012 proves full-library production MCP completion |
| REQ-034; AC-013 | Retained browser lifecycle/Observe and each assembled 005–011 page: real public login, navigation, interaction, range/version, authorization and cross-tenant proof |
| REQ-035; AC-014 | 012: same-conversation live-agent/browser MCP investigation and follow-up/range interaction; credentials/host absence blocks live acceptance |
| REQ-036; AC-015 | Retained 027 and regression consumers; view layout compatibility and no saved-view persistence |
| REQ-031 and Ask | Deferred; no shipping gate, shell control or runtime service here |
| REQ-020/022/062; INV-001/002/004/006/007; AC-010/041 | Every new consuming task: source-backed reuse, truthful/current typed data, credentials/tenant boundaries, individual gallery/page acceptance and normal dev:full usability |
| AC-018 | Retained Observe plus every signal-consuming task's narrow fidelity regressions; MCP histogram proof only at 012 under revision 21 |

## Verification and handoff

The shared [verification reference](references/verification.md) describes current
runner/setup ownership. Exact task selectors are planned additions unless
explicitly identified as existing; confirm actual selection before RED and
never count skipped/no-selected tests as proof. No application build/test run is
claimed by planning. No fake RED is required for unchanged evidence/config.

Final change review runs the integrated first-class surface journeys and
repository aggregate once against the candidate, plus native/image/visual and
paid live-agent evidence that is outside default CI. Task gates remain narrow.
Independent readiness and comprehension results belong to the new revision;
remaining platform gates and human implementation acceptance stay explicit.

## Revision 28 readiness result

Independent [plan readiness and comprehension review](../../../.dev/review/ui-consolidation-revision-28/review.md)
returned **Ready**, architecture PASS and executability PASS for the declared
independent work and accurately bounded conditional assignments. All eight tasks
were reviewed; both findings were corrected and independently rechecked. No open
Critical/Major findings remain. This is not a claim that every scenario can run
against current main: the named external gates still prevent affected whole-task
completion. No runtime/implementation or human visual PASS is implied.

The temporary task archive was removed only after independent outcome/component/
page/action/proof conservation and all 23 original task hashes were checked.
The transfer record retains provenance and current ownership. Completed records
were preserved, including pre-existing edits; no commit or push was performed.

## Caller correction to Task 005

The added artifact and derived-Prompt SDK/hydration gates were not approved.
They are removed from Task 005 and its consumers. Every Card page has a Files
tab displaying files already recorded at registration; PromptVariables displays
values already derived by the existing compiler. UI/shared-host wiring is part
of the task. Policy and Source remain removed. The earlier conditional readiness
report records the superseded draft; its artifact/Prompt gate conclusions are
not authority for this corrected assignment. The approved canvases and paired
component/gallery/conventional delivery remain unchanged.
