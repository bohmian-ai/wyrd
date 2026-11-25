# Existing verification data for the UI

Status: finalized for task planning on 2026-11-12 after user agreement. Boundary: UI adaptation only; no new underlying server
contracts or logic. This replaces the earlier overbroad task-revision proposal.

## Verified source baseline

UI checkout `9c38fdfc3`; resource-scaling `443a79337`; newer integrated backend
source `54aa8f4a9` in the github-verifier-pipelines-task-004 worktree. That newer
source includes the compact/details split and existing retained inspection APIs.
This is source inspection, not proof those changes are merged into the UI branch.

`architecture/bifrost-design.md:422–456` defines summary/output/input storage.
Newer backend architecture additionally defines execution_events at line 458.
Resource-scaling `architecture/wyrd-design.md:1876–1906` explicitly says the compact
result drops fields at the response, never in storage.

## Field-to-existing-read map

| UI content | Existing source | UI responsibility |
|---|---|---|
| Declared nodes, dependencies, description and version | Bifrost execution_inputs.verifier_card_uid (or retained execution_events metadata) for the execution, then existing HTTP cards/by-uid/Verifier/{uid} or MCP cards.get(kind=Verifier, card_uid); TASK-001 projection | Add a UI adapter for the existing UID Card read; render/reuse shared graph and current Step controls |
| Lifecycle and terminal error | verification.get_execution / SDK Execution.status | Render returned status separately from verdict; preserve unknown/denied/unavailable states |
| Current recorded judgment, reasons, actual, citations, custom data and drift feature detail | Compact state.result / VerificationResult requirements | Render these fields without assuming steps/check/provenance/evidence are present |
| Completed steps, authored checks, provenance and evidence declarations | Existing verification.get_execution_details / SDK Execution.details; backed by Bifrost outputs/inputs | Explicit inspector load; preserve readable judgment when details are absent or refused |
| Accepted invocation instances and progress | Existing invocation pages | Preserve exact step/item/iteration/attempt and speculation; do not infer execution from graph topology |
| Retained output names and producers | Existing publication pages | List metadata without opening every body |
| Opened JSON/file/directory output | Existing exact publication, manifest/content and File/download operations | Bounded safe rendering and existing host-held access |
| Recorded result list, verdict charts, correlations and result timing | Existing bounded bifrost.query over verification.results | Parameterized UI queries; matching list/chart/filter predicates and existing limits |
| Historical requirement/step/drift details | Existing permitted queries over verification.results and execution_outputs, or existing details operation | Select recorded values; current permissions still apply |
| Evidence declarations and accepted input context | Existing permitted execution_inputs reads or domain projections | Display only what the selected authorized read returns |
| Retained nonterminal execution facts | Existing status/invocation reads and newer execution_events | Exact all-status discovery query coverage still needs verification; absence of a final result is not absence of work |
| Cancel, approval and delivery | Existing domain operations as delivered | Host action/read adapters only; preserve their existing acceptance/authorization semantics |

The existing UI contract/executors already support bounded bifrost.query with
parameters and row/byte limits, plus verification.get_execution. Other existing
backend operations need UI adapters; adding an adapter is not adding a server API.
No code/runtime experiment was performed during this research.

## Specific UI task direction

- TASK-001 stays complete. Extend its shared graph for newly integrated Step
  forms in TASK-002, preserving the definition view.
- TASK-002: create the missing runtime panels in the existing catalog family;
  separate compact judgment props from explicit details and invocation/publication
  props; preserve exact selection and independently unavailable reads. Reuse the
  current query/domain operations rather than add execution-context/drift APIs.
- TASK-003: replace proposed new server discovery endpoints with existing bounded
  Bifrost queries where the required authorized records exist. Verify all-status
  coverage and permission semantics before declaring the task ready. Do not use
  completed results alone to claim an all-execution list.
- TASK-004–007: consume shared UI reads/presentation and existing external
  capabilities. Amend only wording, adapters and tests actually affected by the
  integrated contract; do not invent new server capabilities or change task order.

## Capability checks before affected task activation

1. Verify the exact retained columns/permissions needed for all-status rows and
   counts; distinguish available Bifrost queries from requirements they cannot
   satisfy today. Client filters never grant authorization.
2. Running definition lookup is source-backed: execution_inputs.rs:59–67 and
   execution_events.rs:64–81 expose verifier_card_uid as scalar metadata; existing
   HTTP /v1/cards/by-uid/{kind}/{card_uid} and MCP GetCardArgs support
   kind=Verifier plus card_uid. The current TypeScript Cards.get accepts a
   versioned CardRef/string, not a bare UID; its Node host path therefore needs
   an adapter to that existing endpoint or an exact retained CardRef. This is an
   adapter gap, not a missing server capability. Use an authorized execution-ID-
   filtered bounded metadata lookup, then the existing Card read. Until the first record is visible, show
   definition not yet available. Retained result selection still comes from the
   existing domain status/details owner, not latest-result SQL.
3. Confirm supported-host file preview/download reuse and already delivered
   wait/approval/delivery projections. Do not replace missing inputs with fixtures.

The user rejected three earlier proposals: new execution-status context fields,
a drift-context API and server discovery/filter/aggregate APIs. They are not
current recommendations. Specification revision 3 and the revised proposed tasks remove that broader
read-extension assignment. Capability checks remain explicit external inputs;
no production source was changed.

## TASK-002 concrete user journey

Open `/t/[tenantKey]/verify/executions/[executionId]`. Read existing execution
status; resolve the exact Verifier UID from authorized retained metadata and load
that Card through the existing UID operation. Draw its declared graph. Read
bounded invocation/publication pages for current accepted progress and output
names. Selecting a node/instance displays its recorded status/judgment; explicit
completed details provide check/provenance/evidence when needed. Opening an
output fetches only that exact producer's publication/manifest/file. Refresh
rereads existing state while preserving topology, viewport and producer selection.
Session membership comes from status.group; drift feature judgments from compact
requirement detail, executed window/check context from details, and historical
scores from permitted bounded Bifrost result/output queries. Cancel uses existing
Execution.cancel through a host action, then status readback. Trace citations use
existing exact trace reads. Unknown/denied/not-yet-retained data stays explicit.

The implementation is the route, shared rendering, typed UI operation adapters,
Bifrost lookup/history queries, URL/selection state and real host/browser proof.
No new server data, fields, endpoints, execution decisions or storage are needed
for this mapping. This source mapping is not a runtime integration pass.
