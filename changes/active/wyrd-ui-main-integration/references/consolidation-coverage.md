# Revision 28 task transfer and coverage

The caller approved consolidation on 2026-11-06. The full `tasks/` tree (31 files) was
copied to temporary `tasks-archive/` before task editing. On 2026-11-06 the
independent readiness/comprehension review and a final hash check confirmed
all 23 original 005–027 task identities and their transferred outcomes. The
temporary archive was then removed; no current task relies on it. This record preserves the transfer
and original byte identities without keeping a second executable packet.

Original checkout: `4f6bc10d4967b1cd19b5c2b314d9fd9d0b42d2d6`. Recover an original with
`git show 4f6bc10d4967b1cd19b5c2b314d9fd9d0b42d2d6:changes/active/wyrd-ui-main-integration/tasks/<filename>`.
The hashes below identify the reviewed pre-consolidation task bytes; existing
completed-task evidence is retained in place. Immutable review IDs/numbers
always refer to their reviewed filenames/revision, not the new reused numbers.

## Assignment transfer

| Original assignment | Current owner | Required outcome retained | Original SHA-256 |
|---|---|---|---|
| `TASK-005-card-components.md` | 005 / 008 / 011 | CardInventory, CardFacts, RelationshipList, ArtifactTable and Prompt members → 005; CompositionGraph/ModelInterface → 008; CardInventorySummary → 011. | `86b45f4fb053cfac634c32c7691abf6b23b068cf835170797babe12ad5926ed0` |
| `TASK-006-query-components.md` | 006 | QueryEditor, TableCatalog, QueryDetails; discovery/description, typed query execution, terminal results, cancellation and unresolved History. | `93f2d5ee36e5860c7c431bfc3849ab0682043e6fd02747267031417c1ebe1a5e` |
| `TASK-007-verify-components.md` | 007 / 008 / 010 | VerifierGraph, VerificationResult, RequirementDetail, DriftFeatureTable → 007; VerificationBindings → 008; AttentionList → 010. | `48335548987e5ed481f79d1fe7125e590f5008f3b8bb62949f9816514fb8b0d9` |
| `TASK-008-service-components.md` | 010 | SignalBreakdownTable and ChartEditor, real Service-scoped data and versioned chart-save prerequisite. | `de86fa278f1b8a34996b21e8b80811b8c3d5849a820ddd7bae0bdfc60884f021` |
| `TASK-009-change-repository-components.md` | 009 | All twelve Change/Repository components, real supported host writes and external REQ-029 gate. | `f1dd631986a6568ae4224c74cc0c57ec5a07b986c0f2c13f79d7c5ed10a3b7a1` |
| `TASK-010-ask-component.md` | Deferred | Ask, conversation UI and saved-view authoring remain excluded from delivery and dependencies. | `1aa3d5311d0657862684e0e1abe7099ac1eeefb97ddf17a3c9022ff740df1c1a` |
| `TASK-011-mcp-app.md` | 012 | Production ui.catalog/ui.show_view/ui://wyrd/view, caller-bound reads/actions, sandbox/security and connected live-agent/browser follow-up proof. | `72b1d23c30df455e01d0d7b09f3ea95c75b1e53eccb07240727b40b267d8ebe0` |
| `TASK-012-canvas-shell.md` | Retained 004 revisions | Already absorbed shell/browser workflow; no replacement assignment or new shell implementation. | `1cfc7c6e43318b3f4cf060b43bafdd3732f4087877ce35846b805dbcd7c45995` |
| `TASK-013-card-browsing.md` | 005 | Inventory and generic exact detail, versions/relationships/artifacts/YAML; separate checkpoints, current twelve native kinds. | `c438a1aec1b37927890a6d0e0932f734a30f1ecce1c8b088570aec7a5b07e6b3` |
| `TASK-014-agent-page.md` | 008 | Agent definitions/composition/bindings and actual subject-attributed invoke_agent/gateway activity. | `59999addc7017d61844bdd6fe4dd21f0a4e6421e5f6764d65e0e13af02d656b2` |
| `TASK-015-prompt-page.md` | 005 | Prompt native/portable messages/variables/response-format/UsedBy/version diff, no telemetry strip. | `34aebeb7f8c30c5b785faa9d659d74b6c3c9d0d54eb2672be3df52f41d1189ea` |
| `TASK-016-model-page.md` | 008 | Model interface/signature/artifacts/relationships, actually attributed signals and bound-Verifier drift. | `792c5de2e6d93c8f1b24f10707192c2bcfdfa92398c7e1079601cdf29c80c06e` |
| `TASK-017-data-page.md` | 008 | Data schema/stats/splits/target_columns/sql and exact baseline/Verifier/requirement drift; current authorized detail owner. | `d9a778a6e93ea2a380df109b5211af70f7eb72ff3e005f2546c5b9dd586ad9d2` |
| `TASK-018-service-page.md` | 010 | Service overview/composition/signals/component breakdown/attention/bindings/Changes/charts/versions/YAML; external025/029 gates. | `20b468d26bd593c6bd101713472a1a88fc1fdb9ecb4f3563ddd9fce6fa7c9a05` |
| `TASK-019-query-page.md` | 006 | Full workbench beyond existing Observe inspector: SQL/binds/catalog/results/chart/details/History and live browser workflow. | `d15e98a204863cbf8b9f1d9365d253bc1f97d9dbdcf12512372ae24f2c1b7ea9` |
| `TASK-020-traces-page.md` | Retained 004 revisions | Already absorbed full Traces/Spans investigation; no replacement assignment. | `509780a96340a4ff62a5a2c37a7f5aa55dce1925e6ed254aabf47f9702632314` |
| `TASK-021-logs-page.md` | Retained 004 revisions | Already absorbed full Logs investigation; no replacement assignment. | `52ecfb6249739b1dd62eb00375f454e232ef76577ce03236722b66febd92493a` |
| `TASK-022-metrics-page.md` | Retained 004 revisions | Already absorbed Metrics investigation; preserve later approved metrics revisions and MCP histogram deferral. | `5da28bc9a2deef7a830df31e5d2b5b24abebd7c32df0bee7b2ca17af7ba6a32f` |
| `TASK-023-verify-pages.md` | 007 | Verify list/facets/charts/drawers/session/drift/requirement citations and exact Verifier definition. | `716842286ffa4a93a535939ae8dcb87604620c0f9e093fe5596a71cbb69436f8` |
| `TASK-024-home-page.md` | 011 | Home greeting/count/Attention/Services/inline Observe/inventory/recent Cards, one separately accepted Home checkpoint. | `a944d439b3025ab970f03f0b6146360aa11c1dbf979b916d7632c6c9ce5c25d3` |
| `TASK-025-change-pages.md` | 009 | Changes list, revision overview/subjects/activity/reviews, Verification, Audit, Files changed; separate page checkpoints. | `a7089f43b7c3ff41470d2736b1372f2c6396fce117770982fd240cc21596e765` |
| `TASK-026-repositories-page.md` | 009 | Repository connections, webhook health/configuration, wyrd.toml path and Verifier folder with authorized writes. | `c1d4d0b4a9be15dc6d7248ef3001ee96f49b6906bdb5a89671e5066df798a023` |
| `TASK-027-view-layout.md` | Retained 027 | Completed placed/resizable view layout; regression-only where consuming it. | `9dca8a485740c1a14a9217f521a40a0966e3b2cc795285d53600f810dc19a091` |

The numbering now runs 005–012 for eight active delivery assignments. Old
010/012/020/021/022 receive no executable replacement: their scope is explicitly
deferred or already transferred to completed Observe. Completed 001–004/027
and nested revision records are retained byte-for-byte by this consolidation.
This is ownership accounting, not a new acceptance claim for those records.

## Components and first conventional consumers

| Owner | New components | First real consumer and checkpoint |
|---|---|---|
| 005 | CardInventory, CardFacts, RelationshipList, ArtifactTable, PromptMessages, PromptVariables, UsedBy, VersionDiff | Cards inventory; exact generic detail; Prompt definition. Inventory/detail/Prompt are separately inspected |
| 006 | QueryEditor, TableCatalog, QueryDetails | Query workbench; preserve existing exact-query inspector |
| 007 | VerifierGraph, VerificationResult, RequirementDetail, DriftFeatureTable | Verify results/session/drift and Verifier definition, each named board |
| 008 | CompositionGraph, ModelInterface, VerificationBindings | Agent, Model and Data workspaces, each separately inspected; Service later reuses composition/bindings |
| 009 | ChangeRequestList, ChangeHeader, SubjectsTable, RevisionList, ActivityFeed, ChangeVerifierList, RequestVerificationForm, ReviewSubmission, Participants, RepositoryList, WyrdTomlView, VerifierFolderTable | Changes list/detail/Verification/Audit/Files and Repositories, separately inspected |
| 010 | SignalBreakdownTable, ChartEditor, AttentionList | Service signals/component breakdown, charts and attention; Home later reuses them |
| 011 | CardInventorySummary | Home Cards/inventory/recent Cards plus all required reused summary regions |
| 012 | No replacement component family; close approved production MCP host and deferred shared histogram projection | Real MCP resource/connected conversation/browser; all affected catalog members retain individual proof |

Retained Chart, CardHeader, StatStrip, FacetRail, RecordTable, TraceWaterfall,
RecordDetail, primitives and WyrdView keep completed owners. No component is
reimplemented just to change its delivery task. A component may serve multiple
pages without creating another loader or host boundary.

## Proof and board conservation

Every original component-family promise retains T1 states, T2 actual
requests/shaping and negative sequencing, T3 view/catalog validation, T4 actual
built AppBridge artifact and applicable T5 authorized Node/server/Postgres
proof. Every original page promise retains real production-browser T6, public
login, interaction, exact range/version identity, stable errors and cross-tenant
negatives. Each new consuming slice additionally owns REQ-062/AC-041 normal
`dev:full` representative data and a reachable live route.

All approved canvas boards remain: Cards inventory (approved `Cards.dc.html`), Agent, Prompt, Model, Data, Service/ServiceCharts,
Query/QueryChart, Verify/VerifySession/VerifyDrift/Verifier, Home,
Changes/ChangeDetail/ChangeVerification and Repos, plus retained
Main/Spans/TraceDrawer/Logs/Metrics. Audit/Files follow the approved ChangeDetail canvas; their data contracts
still come from the server, never from mock-derived payloads. Grouped tasks retain
individual component and page desktop/mobile light/dark acceptance, keyboard,
focus, touch, long content and overflow. No human PASS is manufactured by this
planning transfer.

## Deliberate corrections and remaining prerequisites

- Current architecture has twelve registrable kinds; old fourteen-kind text
  does not authorize Policy/Source aliases. Policy and Source are removed; no separate kind-reconciliation project is
  introduced.
- Verification summary is payload-free. Requirements/steps/drift detail live
  only in sensitive `verification.execution_outputs.output`, resolved with
  exact result identity and current viewer permissions. Retired
  `drift.result_features`/`results.outcome` references are corrected, not copied.
- Query History source/lifetime,
  non-sensitive requirement detail, Service charts and Change/repository
  contracts retain their existing external gates. The consolidated tasks do
  not invent missing shapes, storage, permissions or durable owners.
- Revision 21 deferred shared MCP metrics/histogram projection until the MCP
  delivery. Task012 retains that approval prerequisite and proof; conventional
  Metrics acceptance alone does not satisfy it.
- Full-library production shipment and connected MCP proof still wait for all
  required components/actions/platform contracts. Remaining conventional pages
  no longer wait for that final gate; they ship with their own components.

The [plan](../plan.md) maps every REQ/INV/AC group to retained, deferred or new
owners. The independent [readiness/comprehension review](../../../../.dev/review/ui-consolidation-revision-28/review.md)
checked all eight tasks against this transfer and the original archive before
its removal. It returned Ready for declared independent work with zero open
Critical/Major findings; conditional whole-task completion still waits for
external approvals/projections. The installed CLI proxy resource forwarding scope was corrected and re-reviewed.
The derived-Prompt gate in that reviewed draft was subsequently rejected by the
caller and removed, together with the added artifact SDK gate. Task005 instead
displays registered files in a Files tab and existing compiler-derived Prompt
values; Policy and Source remain removed. The old review does not approve those
rejected additions.
Static checks also confirmed complete scenario blocks, task links/verification
anchors, paired-delivery mappings and preserved completed-task bytes. No
application tests, services or human visual acceptance were run by planning.

All eight remaining tasks now directly link their exact approved `.dc.html`
canvases in an Approved Canvases section. The existing design approval is
preserved; human checkpoints accept implementation fidelity to those designs.
