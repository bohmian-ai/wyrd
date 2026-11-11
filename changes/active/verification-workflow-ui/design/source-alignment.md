# Source alignment and delivery boundaries

Inspected 2026-11-10. This is design research, not runtime verification or an
implementation assignment. Main checkout supplies the existing UI owners;
`/home/thorrester/Documents/GitHub/wyrd-worktrees/code-review-workflow` at
`ec87d12e1` supplies execution inspection and the approved GitHub pipeline
contract. Worktree changes are not evidence of delivery to main.

## Existing owners and precise extensions

### Specification revalidation, 2026-11-10

The UI [specification revision 1](../spec.md) revalidated the newer worktree
`/home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-002`
at HEAD `5d6fb4116`. Its `github-verifier-pipelines/spec.md` and `contracts.md`
are approved revision 4. Revision 4 changes installation storage only: the
backend owns the approved `wyrd.github_installations` table. Public revision-3
shapes remain unchanged. The UI adds no table or installation lifecycle owner.

The current verification route registration still provides exact execution,
invocation/publication/content/download and cancel operations; no all-execution
list or review-set read appears in that route owner. `get_execution` still
reads selected terminal result identity and pinned group membership. The
discovery/review projections below remain required extensions, not delivered
APIs. Native Repository, approval and connection behavior remain approved
backend dependencies; this inspection does not establish their full delivery.
Main's CodeGraph was consulted for the shared UI read/loader owners; the task
worktree has no CodeGraph index, so its exact source was inspected directly.

| Capability | Inspected owner | Selected extension and limit |
|---|---|---|
| Shared reads and props | UI `packages/ui/src/lib/contract.ts::WireRequest/WireResponses/Loaded`; `loaders.ts::LoadContext/loadPanel` | Add typed domain read projections and pure verification/repository loaders to this owner. Existing states already distinguish loading/populated/empty/denied/error. Do not add page-local transport or a second loader framework. |
| SDK and MCP host reads | UI `packages/ui/src/lib/node.ts::nodeExecutor`; `mcp.ts::tool/mcpExecutor` | Extend their closed operations for the same authorized server reads. Currently they expose `verification.get_execution`, Cards and Bifrost operations, not the full proposed viewer. Keep credentials with hosts. |
| Host actions | UI `contract.ts::ActionIntent/ActionState/ActionProps`; current server BFF/session owners | Use typed action intents for cancel, approval and integration actions. The read executor stays read-only. Extend existing host action dispatch and session forwarding; components never hold authority. |
| Exact Card content | UI `loaders.ts::loadCardHeader`, `cards.ts`, CardHeader/CardFacts and exact Card routes | Repository is another Card projection. Reuse version navigation and bindings; viewing an old version is read-only. No second Repository registry or inline declaration engine. |
| Exact execution/result | Worktree `components/verification/service.rs::VerificationControl::get_execution`; `service/inspection.rs::retained` | Reuse the accepted execution state, exact selected result and current scoped authorization, including retained reads after queue/Card removal. A result not visible yet is not an invented verdict. |
| Invocation/publication inspection | Worktree `service/inspection.rs::invocations/publications/publication/download`; shared client `verify/execution.rs::Execution` | Reuse bounded metadata pages, fixed listing cuts and exact Producer selection. Public file content/download routes already exist. Extend UI adapters rather than build a browser event ledger or query protected payload tables directly. |
| Cancel | Worktree `service/inspection.rs::cancel`, route `cancel_execution`, client `Execution::cancel` | Use the existing server cancellation fence and read back status. Already-settled results remain settled. Cancellation neither deletes accepted publications nor guarantees a provider call already sent disappears. |
| Repository/GitHub/wait/approval | Approved `changes/active/github-verifier-pipelines/spec.md` and `contracts.md`, revision 3 in the worktree | Consume its Repository declaration/selection, singular integration connection, exact PR admission, waiting inspection, approval action and delivery state. These are approved contract dependencies, not all implemented methods in the inspected source. |
| All-status list and review grouping | Worktree verification routes list exact execution reads and child/publication lists; no aggregate execution-list route was found. Current TASK-007 lists recorded results only. | Extend the existing VerificationControl read boundary with bounded summary/filter/review-set projections needed by the agreed UI. This is missing public behavior and needs inclusion in the new spec with SDK/MCP projections, not a browser join over every execution or a new runtime owner. |
| Sensitive detail | `architecture/bifrost-design.md` verification tables; worktree `inspection.rs::retained` | Domain execution reads already authorize exact Verifier read before internal retained-state access. Reuse that boundary. Public SQL retains its table grants and sensitive columns remain sensitive. No ordinary-viewer Bifrost payload bypass or payload reclassification. |

The existing TypeScript client exposes `verify.Execution.invocations`,
`publications`, publication reads and cancellation. Its integration tests include
`verify-in-real-time.test.ts`, `verifier-building-blocks.test.ts`,
`verifier-orchestration.test.ts` and `durable-verifier-work.test.ts`.
The current UI host integration test exercises `verification.get_execution`.
These are reuse/verification owners, not claims that their journeys prove the
new UI or future approval/GitHub contract.

## Necessary public read behavior

The approved all-status Verify page necessarily needs a summary read that covers
accepted queued/running/waiting/terminal work, including executions with no
recorded result. A result-table query alone cannot provide it.

The summary projection must carry exact execution, subject and Verifier
identity, lifecycle status, nullable recorded verdict/result selection, existing
correlation/filter fields, and repository/PR/review-set association when present.
It must support the approved list filters, bounded pagination, matching chart
and facet populations, and exact execution navigation. Unfinished work has no
verdict. Verdict filters apply to recorded verdicts; status and verdict filters
combine normally, so an incompatible combination produces no matches.

Review-set membership is server-recorded admission context: the exact selected
Repository policy, PR source and review occurrence identify the Verifier
executions requested together. Preserve that context in the existing admission/
accepted-record owners; do not add a new Card kind, orchestration service or
Postgres table. Expose it to typed list/history reads rather than infer it from
timestamps, display names, commit SHA alone, or session `group_id` text.
The specification records these public read obligations at the existing owner.
Exact schema and operation spellings must follow that owner's conventions when
the contract extension is implemented; this design does not claim an existing
API for them.

The general PR link resolves the current review set for that PR, across source
updates; earlier sets remain explicitly selectable. Exact execution and selected
historical-set links stay pinned. Live list refresh can show current work but
must not replace an open exact execution/selected historical inspection.

Summary reads use the same current tenant/domain access as execution inspection,
with per-execution Verifier scope applied before pagination/counts. Repository
and Card reads retain their own authorization. No follow-up sensitive read is
issued after a denied prerequisite. Unknown/foreign IDs follow the owning
not-found policy; denied, unavailable and genuinely empty remain distinct.

Connection projections supply provider-access repositories; Card reads supply
registered Repository versions and selected bindings. A registered Card remains
visible through authorized Card reads even when provider access is gone.
The UI may combine those bounded projections by stable provider identity for
display, but registration, selection, access and readiness are server facts.
Unavailable access state must never become a guessed active/inactive state.

## Authority and scope reconciliation

- Backend revision 3 explicitly excludes UI. It owns Repository kind/declaration,
  GitHub admission, evidence custody, waits, approval semantics and provider
  delivery, including necessary architecture amendments. This UI change consumes
  that work rather than silently reimplementing its engine or install protocol.
- This packet owns the new summary/review-set read projection required by its
  discovery UI, in existing contract/server/client owners. Coordinate accepted
  admission fields with the backend owner. This is a necessary consequence of
  the user-approved list/grouping, not permission for a new backend subsystem.
- Existing UI TASK-007 must transfer verification components/Verify/definition
  graph scope here. Preserve session/drift, exact selected results, trace evidence
  navigation, gallery and portable-host coverage. Its results-only list and
  prohibition on inspector attempt identity need explicit replacement.
- TASK-009 transfers its repository/connection/Card projection scope here.
  Its broader Changes/collaboration scope remains in the original UI packet.
  Do not pull those workflows into this change or silently supersede the whole
  task. The old folder-scan/default-branch configuration canvas is not the new
  native declaration contract.
- Native Repository is approved design input and backend revision-3 behavior;
  the current main architecture/Card-kind list still needs its owning update.
  Do not add an alias or implement contradictory definitions in the UI.
- Standard code-review report delivery in the backend does not contradict equal
  UI output treatment. The generic viewer exposes every authorized output
  equally; provider delivery can consume its declared report convention without
  the UI inventing a primary-output selector.

## Verification evidence and remaining private work

The sole centralized mock is built and checked with:

```bash
mise exec -- node changes/active/verification-workflow-ui/design/mock/build.mjs
mise exec -- node changes/active/verification-workflow-ui/design/mock/selfcheck.mjs
```

Its 28 browser checks passed and the user approved the centralized UI.
Superseded prototypes were removed with authorization.

These prove prototype behavior only. No production server/browser journey was
run for this design-completion pass. Implementation must use the existing real
server, browser, SDK and MCP test owners for the connected observations in the
design notes. Exact future test selectors belong to planning after tests/owners
are revalidated; do not manufacture them here.

Private implementation work includes layout sizing, graph grouping mechanics,
refresh scheduling, component decomposition, cursor encoding and fixing the
prototype zoom-control anomaly. The graph uses already-installed Svelte Flow;
Dagre is proven in the preview and is the starting layout choice. These do not
reopen the approved appearance, read-only interaction or status stability.
