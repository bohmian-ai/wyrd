---
id: SPEC-verification-workflow-ui
revision: 3
status: approved
---

# One verification UI

## Problem and agreed outcome

The original verification design explains definitions and recorded judgments.
It does not give engineers a complete way to discover unfinished workflows,
inspect their intermediate outputs, or follow GitHub-triggered verification.
This change combines that foundation with the approved workflow and Repository
design in one UI, preserving session and drift investigation.

For example, an engineer follows a PR link while two authored Verifiers are
running, opens either execution, inspects a retained output, and returns later
to its recorded judgment. The same execution page serves ordinary, session,
drift and approval-wait workflows. Code review is an authored workflow, not a
verification type.

The [finalized design](design/notes.md), [composition](design/consolidation.md)
and [centralized mock](design/mock/README.md) record the agreement. The mock is
the approved visual target; its sample IDs, hash routes and simulated responses
are illustrative, not production contracts. The preserved original
[canvases](references/original-canvas/README.md) explain the retained foundation.
Where the visual references differ, the centralized mock and the decisions
recorded in this specification govern. This specification defines required
behavior; sample interactions do not add obligations.

The approach builds shared UI components and adapters over existing Card,
Bifrost and verification operations, then mounts them in the conventional UI
and supported hosts. The server continues to own durable behavior. This packet
adds no server fields, endpoints, query services, storage or execution logic.
Costs are UI queries, adapters, presentation and real-server visual/journey proof;
the prototype supplies none of that production evidence.

## Required behavior

### Discover and open verification

**REQ-001 — One discovery workspace.** Verify retains the approved dense,
Observe-like layout: WHERE, time range, Live, two compact linked charts,
counted facets, grouping and execution table. One chart shows executions
grouped by verdict over time; the other groups them by Verifier over time.
Dragging across either chart selects the time range for both. Include accepted
unfinished and terminal executions, including those with no result. Preserve
general Verifier, subject/Card, Service/Workload, application Run, session and
failed-requirement context, and add status and applicable repository/PR context.
Both charts and facets describe the same authorized filtered population as
the list, across its pages. Recorded verdicts are passed/failed/inconclusive;
the neutral no-recorded-verdict chart bucket is not another verdict. Status
and Verdict have separate columns and filters. All the listed filters combine;
an incompatible combination is empty. Grouping
changes navigation, never creates a judgment.

**REQ-002 — Exact navigation and return.** Selecting a row opens its workflow
page directly. Back restores discovery filters, range, grouping and position.
Copyable execution links reopen that exact execution and selected result,
without silently following later work. Preserve application Run, execution,
result, subject, exact Verifier and binding identity as separate facts.
Refresh may update recorded progress but must not replace the inspected
execution or selected historical judgment. Use the existing tenant shell and
exact Card routing; do not create a second Cards browser.

**REQ-003 — Existing reads for discovery.** Use existing authorized Bifrost
queries and delivered domain reads for bounded execution rows, matching
aggregates/facets and PR review-set/history presentation. Reuse the typed UI
read adapters; do not add server endpoints, response fields or query services.
Rows carry exact execution, Verifier and subject identity, lifecycle status,
nullable selected result and recorded verdict, the discovery correlations above,
and repository/PR/review-set association when recorded. Completed-result rows
alone do not represent unfinished executions. Current tenant and exact Verifier
read restrictions must be enforced by existing read/query authority before
pagination and counts; a browser filter is never authorization. If an existing
surface cannot supply a required authorized projection, record that precise
external dependency and leave the affected work proposed rather than changing
the server, relaxing access or fabricating records. Discovery must not enumerate
every execution in the browser or infer review membership.

### Inspect one workflow

**REQ-004 — Shared graph and definition.** Use Svelte Flow with Wyrd styling
and automatic layered layout for the graph above the contextual inspector.
Render exact declared inputs, dependency edges, steps, attached requirements
and whole-Verifier requirements; show independent branches in parallel. Keep
readable edge direction, arrowheads and dependency labels, avoiding edges
through nodes and the tangled connections shown in the rejected drafts.
Label the graph's stages clearly and keep simple workflows compact. A failed
session requirement must not color passing members or their connections as
though those members caused the failure.
Disable authoring gestures. Runtime changes preserve node positions, viewport
and user selection; layout follows topology rather than progress. Repeated or
fanout work uses authored step nodes with exact item/iteration/attempt selection
in the inspector. Support pan/zoom and usable keyboard selection. The exact
Verifier Card reuses the graph without runtime coloring, with description,
version, Graph/YAML, where-it-runs and Results in Verify filtered to that exact
Verifier; no separate Results tab.

**REQ-005 — Recorded state and evidence.** The inspector shows the selected
step's recorded state/reason and instances, or the requirement's recorded
verdict, reason, actual, citations and judge. Inputs start collapsed. Distinguish
action success, requirement verdict, speculative/discarded disposition and
execution status. A fresh recorded judgment initially selects its first failed
requirement when present; subsequent updates do not steal selection. Read the
execution's selected result before treating retained output as authoritative.
Load the graph from the exact Verifier Card through existing registry APIs.
Render compact result judgments without assuming they contain steps, checks,
provenance or evidence; obtain those recorded fields through the existing
explicit details read or authorized Bifrost queries. Use existing invocation
and publication reads for accepted progress and exact producers. Status,
judgment, details and file access can be independently available; an optional
read failure must not erase other authorized content. A live result does not
certify that retained records are already visible. Show truthful availability
and normal refresh without waiting inside a page load or resubmitting work.
Exact trace/span citations use current viewer permissions independently of the
originating list's time/workload/subject filters. Keep return context and expose
bounded-read failures. Current retained spans do not claim to reproduce a past
verification's exact span collection; late spans never revise its judgment.

**REQ-006 — Equal retained outputs.** List all authorized retained publications
equally by their actual names, attributed to exact execution, step, item,
iteration and attempt. Changing instance selects only that producer's outputs.
There is no primary-output selector, filename heuristic or final-step shortcut.
Read bounded publication metadata first and bodies only when opened. Provide
bounded JSON, safe text/Markdown and directory-manifest previews; use authorized
domain downloads for other files. Do not execute supplied HTML or download a
whole directory merely to list it. Retained intermediate and discarded work
remains attributable and is not presented as a successful judgment.

**REQ-007 — Session and drift context.** A session judgment shows its exact
pinned chronological member executions and recorded session requirements;
members and citations open their exact evidence. Returning from a member
restores the session's selected member or requirement. Membership comes from the
selected execution's recorded snapshot, scoped to its exact owner, never from
matching caller group text. Later reviews remain distinct. A drift judgment
shows its exact window/baseline/method, previous/next windows, every recorded
feature with drifted features first, scores, thresholds, verdicts and recorded
score history with a dashed threshold and the viewed window highlighted.
Do not calculate new verdicts or invent
baseline distributions. These are contextual views in the shared inspector.

### Wait, decide and cancel

**REQ-008 — Waiting and approval.** Distinguish missing-evidence inputs from
pending approval, including running peers; waiting is nonterminal and supplies
no final verdict. An approval identifies the exact server-issued occurrence,
step/item/iteration, message, expiry and recorded decision. Authorized users
can Approve or Reject with an optional reason through the existing domain
action. Show pending, accepted, denied and failed outcomes. Refresh and explain
stale, expired or conflicting decisions; never apply them to a newer occurrence.
Resolve ambiguous submission through recorded decision readback rather than
automatically issuing a new decision. Continuation belongs to the same server
execution under its accepted authority, with normal scoped authorization and
no additional login ceremony.

**REQ-009 — Cancellation and honest failure.** Authorized users can cancel
unfinished queued/running/waiting work through the existing domain action.
Guard duplicate pending submissions and refresh authoritative state. If
completion wins the race, preserve its terminal result. Cancellation retains
accepted progress/publications and does not promise to retract provider IO
already sent. Missing, denied, unavailable, empty, not-yet-visible, skipped,
expired and terminal-error states remain distinguishable. GitHub delivery
status is separate from verification verdict; delivery retries are not
verification retries. The UI never resubmits work to repair inspection.

### Repositories, PR links and GitHub settings

**REQ-010 — Separate Repositories workspace.** Repositories is a visible
primary navigation destination beside Cards. Keep the existing Cards UI.
The repository index presents registered Repository Cards and accessible
provider repositories, including accessible-but-undeclared instructions.
Join bounded projections by stable provider identity for display only. Show
registration, selected policy, provider access and automatic-verification
readiness as distinct server facts. Card and integration reads retain their
independent permissions; unavailable access does not imply inactive or ready.
Preserve index search/filter state on return from detail.

**REQ-011 — Exact Repository policy and activity.** Repository detail reuses
normal Card header/version navigation and shows selected policy separately
from the version being viewed, exact Verifier bindings/activation, read-only
policy and verification activity/PR context. YAML/TOML authoring remains in
normal declaration/SDK workflows; label reconstructed TOML as a projection,
not the original source file. Viewing an old version never selects it. Empty
verified_by means no future automatic verification, not disconnection.
Authorized Card reads remain usable after provider access is removed. Activity
opens exact executions or the same filtered Verify workspace.

**REQ-012 — Current PR review set with pinned history.** A review set is the
server-recorded collection of Verifier executions admitted together for a PR
source/review occurrence and exact selected Repository policy. The general
copyable PR link opens the latest set, with earlier sets selectable. It can
contain multiple Verifier executions, each independently openable; do not
invent a combined verdict. Explicit historical-set and execution links stay
pinned as the PR changes. Distinguish a new source head from a rerequest on
the same head. Membership must come from accepted admission context, never
timestamps, display names, commit SHA alone or session/group text. Reuse the
same workflow page for inspection.

**REQ-013 — GitHub connection UI.** Settings → Integrations → GitHub supports
Wyrd-first and GitHub-first onboarding through the existing singular flow:
Connect → provider installation/authorization → server callback-completed
association. Show configured host/App/account and safe access state, provider
manage-access navigation and authorized disconnect with confirmation. Explain
affected unfinished-work cancellation while Cards/evidence remain retained.
Connecting or adding access alone neither declares Cards nor activates
Verifiers. Use the backend's normal authorization and redacted projections;
no App-secret forms, second connection-confirmation step or new management
workflow. Missing optional App configuration makes this integration visibly
unavailable without disabling general verification.

### Shared delivery and limits

**REQ-014 — Shared components and supported hosts.** Extend the existing
pure-prop component/loader catalog, closed typed read contract, Node/MCP
executors and host action intents. Conventional pages, gallery, custom
consumers and the supported MCP App artifact use the same implementation.
Credentials/transport remain with hosts and writes with authorized domain
actions, not read executors. Superseded loads cannot overwrite new selection;
denied prerequisites cause no dependent sensitive reads. Preserve individual
component/page state, visual and accessibility evidence from the original UI
agreement, including light/dark, desktop/mobile, keyboard/focus, long content
and internal scrolling/pan/zoom. Fixtures do not close live dependencies.

No visual workflow editor, new verification kind, finding schema, separate PR
dashboard, general Cards redesign, activation/pause/rollback editor or broader
Changes/collaboration product is included. No new server API, response field, query service, authorization behavior,
runtime, scheduler, registry, graph store, browser execution ledger, event bus
or PostgreSQL table is authorized by this UI packet. Internal layout algorithms, cursor encoding,
refresh scheduling and component decomposition remain implementation-owned.

## Dependencies, authority and scope transfer

The governing owners are [Wyrd doctrine/Cards/Verifier](../../../architecture/wyrd-design.md#verifier),
[Bifrost evidence](../../../architecture/bifrost-design.md) and
[security](../../../architecture/wyrd-security-posture.md). Domain reads keep
sensitive payload classifications and current tenant/scope checks. The UI never
widens accepted execution authority or implements durable workflow behavior.

External dependencies are capabilities, not task numbers from another packet.
Existing Card APIs load registry declarations; Bifrost retains verification
summary/output/input data and accepted progress; existing verification APIs
supply status/details/invocations/publications and authorized actions. The
[finalized read map](design/contract-alignment-research.md) records inspected
sources and the [source alignment](design/source-alignment.md) distinguishes
source presence from integration into this UI checkout. Revision 3 records the
user's 2026-11-12 agreement to this UI-only approach.

Native Repository, connection, admission, deferred input, approval and delivery
remain external backend capabilities with their owning authority amendments.
The newest inspected backend contract is revision 7 at source `54aa8f4a9`; its
approved nonblocking-execution semantics are not proof of implementation.
Positive approval receipts retain durable decision acceptance; direct/live
results do not certify retained visibility. This packet neither implements those
semantics nor changes their storage, credential or permission owners.

Approval of revision 2 enacts the following bounded transfer. The original
packet records it beside the affected requirements and tasks:

| Original scope | Replacement and retained responsibility |
|---|---|
| [UI spec revision 28](../wyrd-ui-main-integration/spec.md), REQ-018/026/027; [TASK-007](../wyrd-ui-main-integration/tasks/TASK-007-verification.md) | This packet owns verification components, discovery, workflow/definition pages, session/drift and evidence navigation. Replace results-only and drawer-first navigation restrictions and the prohibition on exact inspector attempt identity; revision 3 preserves the no-new-server-API boundary. Preserve recorded judgment, scoped membership, citation, host and real-journey obligations. Internal retry/backoff controls remain excluded. |
| Original REQ-029 repository portion; [TASK-009](../wyrd-ui-main-integration/tasks/TASK-009-changes-and-repositories.md) | This packet owns Repository presentation and GitHub connection UI. Native declaration/access/policy replaces folder-scan/configuration-path editing. Change Requests, discussion/review, revisions and their unrelated components remain in the original packet. |
| Original shared delivery obligations, including REQ-030/034/062 | Continue to apply to these components and pages; ownership transfer removes neither supported-host proof nor paired component/conventional delivery. |

This section is the scope-reconciliation record. The transfer is fully
enumerated above and was synchronized into the original packet on approval
(2026-11-11): its REQ-018/026/027/029 entries, plan, TASK-007 and TASK-009 point
here, and its retained Changes obligations are unchanged. Do not execute conflicting original tasks
or mark the entire original Changes task superseded. Discovery/review-set UI queries and adapters belong to this packet over existing
read surfaces. Server contract, engine and installation changes remain outside
this packet; a missing input is an external dependency, not an implementation
assignment here.

The following backend-owned authority amendments are dependencies of the tasks
that consume them. They are not a prerequisite for approving the UI behavior
or starting unrelated work:

| Owning document | Required backend amendment and current boundary |
|---|---|
| `architecture/wyrd-design.md` and root contributor Card inventory | Native Repository Card/declaration and TOML root, closed PR activation, deferred-input/approval and public inspection contracts. Main still describes 12 Card kinds and the older verification surface; this specification does not amend them. |
| `architecture/wyrd-security-posture.md` | Backend-owned connection authorization, exact approve scope and accepted-authority rules for PR/deferred/approval behavior. Consume its approved amendments; do not derive authority from this UI. |
| `architecture/bifrost-design.md` | Backend-owned evidence custody/admission changes, retaining sensitive-payload protections. This packet consumes existing authorized queries/reads; no new domain projection or payload reclassification is authorized. |

Specification approval fixes the agreed UI behavior and bounded scope transfer;
it does not certify backend delivery. Synchronize the affected original-packet
scope references when the transfer is approved. Before activating each task,
record only the integrated contract/authority revisions and real dependency
proof that task consumes. No task may implement behavior that conflicts with
its applicable current authority.

| UI task | Existing capability or predecessor input required |
|---|---|
| 001 — Definition graph | Existing exact Card/Verifier definitions; complete against revision 2. |
| 002 — Execution inspection | 001 graph; existing status, compact result, explicit details, invocation/publication/file/cancel operations and permitted Bifrost context queries. |
| 003 — Verify discovery | 002 inspector; existing authorized accepted-execution/result data for bounded queries, counts and filters. Missing read coverage remains an external dependency; no server extension. |
| 004 — Repositories/settings | 001/003 plus delivered connection/access and native Repository/selected-policy operations with owning authority. |
| 005 — PR review sets | 002/003/004 plus actual accepted admission membership/history and confirmed same-commit request semantics. |
| 006 — Waiting/approval | 002 plus delivered occurrence/wait/decision/readback/continuation operations and owning authority. |
| 007 — Delivery status | 002/005 plus delivered authorized provider delivery projection. |

For 005, a distinct explicit request at an unchanged commit must produce a new
server-recorded review occurrence; retry/redelivery of that same request must
reuse its accepted occurrence and frozen membership. The backend admission
owner must identify the supported request input, durable occurrence identity
and replay behavior. The inspected backend admission contract does
not establish that request path; confirmation remains a named dependency of
005, not permission for the UI to invent an endpoint, synthesize membership or
add a rerun control. Unrelated UI tasks can proceed; tasks that depend on 005
still require its completed output. Exact public schema spellings follow owner
conventions.

## Acceptance observations

Use existing public server/Postgres/Bifrost reads for real UI input journeys on
the supported hosts, and real browser journeys for conventional UI behavior. UI fixtures
supplement rendering/error coverage. The mock's 28 browser checks establish
prototype behavior only. Each proof below must assert the stated observation,
not merely the existence of a route/component or a successful command.

| Proof | Situation and expected observation | Requirements (REQ-*) |
|---|---|---|
| AC-001 Discovery and return | Create unfinished and completed ordinary/session/drift/PR executions. Combine status/verdict/general/repository filters, brush both charts, page/group, open a row and return. Counts and rows describe the same authorized population; no-result work has no verdict; discovery context survives. | 001–003 |
| AC-002 Exact graph and publications | Follow an execution link during parallel/repeated work, choose item/iteration/attempt and open intermediate outputs; refresh as work progresses and reopen after completion. Producer/result identity, selection and viewport remain stable, all outputs are peers and unsafe/oversized previews fail visibly without executing content. Check ordinary and wide graphs against the approved mock. | 002,004–006 |
| AC-003 Session, drift and citations | Open distinct owner-scoped session reviews, pinned members and an authorized cross-workload/untagged trace beyond list range; inspect drift windows/features/history. Exact recorded membership/judgments remain unchanged after later activity/spans; denied/truncated evidence is visible. | 005,007 |
| AC-004 Waiting and decisions | Exercise a blocked branch with a running peer, approved/rejected/expired decisions, stale/conflicting occurrence and denied action. Read back an ambiguous submission and continuation. No decision affects newer source/occurrence; accepted continuation stays on the same execution and accepted authority. | 008 |
| AC-005 Cancel and retain | Cancel unfinished work, race completion and reopen evidence; inspect provider delivery failure/retry independently. Authoritative terminal state wins, publications remain readable and no inspection/delivery action starts replacement verification. | 006,009 |
| AC-006 Repository onboarding | Complete Wyrd-first and GitHub-first connection through public server operations; inspect accessible-but-undeclared, declared-with-empty-bindings, ready and access-lost repositories. View an old policy, follow activity to Verify, return and disconnect. No connection/view selects or creates policy; access loss retains authorized Cards/evidence and reflects backend cancellation. | 010,011,013 |
| AC-007 PR navigation | Admit multiple Verifiers together, rerequest on the same commit, then update the head. The general PR link follows the current set; explicitly selected history/execution stays exact. Membership and source/policy are server-recorded and no combined verdict appears. | 003,012 |
| AC-008 Authorization and hosts | Exercise foreign IDs, restricted Verifier scopes, denied prerequisites and unavailable reads through the real supported HTTP/SDK/MCP and browser adapters. No foreign content/counts or follow-on sensitive reads appear; denial/unavailability never becomes empty success. Repeat affected reads/actions in supported host consumers. | 003,005,008–014 |
| AC-009 Visual and development journey | Produce representative ordinary, session, drift, PR and approval executions through normal public development workflows; navigate Verify, Repositories and Settings in the real browser. Individually compare components/pages with the approved mock in both themes, narrow/wide layouts and keyboard/touch interaction. No inert fixture action or absent prerequisite is accepted as production completion. | 001,004,007,010–014 |

## Approval and revision history

Revision 3 records the user's explicit agreement on 2026-11-12 to adapt the
approved UI to existing reads and resource-scaling's compact/details split,
with no underlying server changes, and the request to revise the tasks.
The visual behavior and revision-2 scope transfer remain approved. External
capability readiness is still checked per task; unavailable data is not invented.

| Revision | Status | Change |
|---|---|---|
| 1 | draft | Convert the finalized centralized design into one verification UI contract; align the dependency with backend revision 4 and explicitly bound the original UI scope transfer. Clarify the agreed charts, graph presentation, session return and drift-window highlight. |
| 2 | approved (2026-11-11) | Separate UI specification approval from task-specific backend delivery; record the unconfirmed same-commit review-request input as a direct dependency of TASK-005. |
| 3 | approved (2026-11-12) | Record the user-agreed UI-only implementation boundary: existing Card/Bifrost/domain reads, compact judgments and explicit details, unchanged task order, no new server APIs/fields/query services or runtime behavior. |
