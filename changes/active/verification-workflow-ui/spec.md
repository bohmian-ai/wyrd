---
id: SPEC-verification-workflow-ui
revision: 8
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
and [centralized mock](design/mock/README.md) record the agreement. The
[tasks 005–007 resumption design](design/resume-005-007.md) records revision 8's
authorization alignment, presentation decisions and retained-read correction.
The mock is
the approved visual target; its sample IDs, hash routes and simulated responses
are illustrative, not production contracts. The preserved original
[canvases](references/original-canvas/README.md) explain the retained foundation.
Where the visual references differ, the centralized mock and the decisions
recorded in this specification govern. This specification defines required
behavior; sample interactions do not add obligations.

The approach builds shared UI components and adapters over existing Card,
Bifrost and verification operations, then mounts them in the conventional UI
and supported hosts. The server continues to own durable behavior. This packet
adds no server fields, endpoints, query services, storage or execution logic
apart from REQ-015's status recording and revision 5's bounded verdict/citation
correction under REQ-016/017, and revision 6's public EXPLAIN support under
REQ-018, and revision 7's PR rerequest and retained-membership prerequisite under
REQ-019/020, and revision 8's existing retained-status read correction under
REQ-021. TASK-004A owns the verdict/citation correction and its development
journey; TASK-004B owns explanation and UI query consolidation; TASK-005A owns
the bounded admission extension before TASK-005. TASK-007 owns REQ-021's
backend read correction before consuming that response in the UI. All other
server exclusions remain unchanged.
Costs are UI queries, adapters, presentation, status-event writes and real-server visual/journey proof;
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
alone do not represent unfinished executions. Current tenant, trusted retained
parent visibility and sensitive-content controls are enforced by existing
read/query authority before user expressions, pagination and counts; a browser
filter is never authorization. The parent is the subject Card for ordinary
verification, the Verifier for subjectless direct work and the Repository for
PR work and its members. Repository Card read covers its retained PR sets,
executions and evidence, including custom roles, without a separate referenced
Verifier read or GitHub-configuration grant. Opening another Card's definition
checks that Card independently. These are the existing authorization-model
REQ-006/007 boundaries, not new UI permission behavior. If an existing
surface cannot supply a required authorized projection, record that precise
external dependency and leave the affected work proposed rather than changing
the server beyond the explicit REQ-015–021 exceptions, relaxing access or fabricating records. Discovery must not enumerate
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
Verifier; no separate Results tab. The authored graph and YAML require that
definition's own read access. If the definition is denied, missing or deleted,
show its truthful unavailability while keeping independently readable recorded
facts selectable in the same inspector. Do not fabricate declared nodes or
edges from partial execution records or implicitly grant definition access.

**REQ-005 — Recorded state and evidence.** The inspector shows the selected
step's recorded state/reason and instances, or the requirement's recorded
verdict, reason, actual, citations and judge. Inputs start collapsed. Distinguish
action success, requirement verdict, speculative/discarded disposition and
execution status. A fresh recorded judgment initially selects its first failed
requirement when present; subsequent updates do not steal selection. Read the
execution's selected result before treating retained output as authoritative.
Load an authorized graph from the exact Verifier Card through existing registry APIs.
Without that definition, allow selection of recorded requirements, bounded
invocations and retained publications through the existing exact node/producer
selection. Missing definition access must not suppress completed recorded
progress needed for that inspection; only recorded, authorized facts appear.
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
step/item/iteration, message, recorded state and decision. Explain that expiry
follows the execution's original deadline, cancellation or supersession; display
the server-recorded expired state, not an invented requested-at/expiry timestamp
or browser countdown. Occurrences remain inspectable without completed details
or a readable Verifier definition. Approve/Reject requires both exact-Verifier
`verifier:approve` and read on the trusted retained parent; neither grant implies
the other. Submit the exact occurrence with an optional reason through the
existing domain action. Host support and current coarse operation hints may
inform read-only/unavailable presentation but never establish target authority;
Rust checks current grants on submission. A supported host without a reliable
hint can submit and display the refusal. Unsupported hosts issue no write.
No new permission preflight API or role-name gate is introduced.
Show pending, accepted, denied and failed outcomes. Refresh and explain
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
verification retries. Read GitHub delivery from the execution's existing
authorized status projection, displaying its pending/delivered/skipped/failed
state, reason and sanitized problems independently of verdict. No GitHub
configuration grant, provider request, separate delivery read or UI retry
workflow is required. A failed status read is not a provider outcome; a recorded
failed delivery is not failed verification. Non-PR executions have no delivery
section. The UI never resubmits work to repair inspection.

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

**REQ-015 — Retained queued lifecycle status.** The existing owner of each
successfully committed queued execution transition stages one row to
`bifrost.verification.execution_events` through the process Scribe outbox.
Record claim as `running`, scheduled retry as `retrying`, deferred-input or
approval park as `waiting`, completion as `completed`, terminal failure as
`timed_out` or `errored`, and cancellation as `cancelled`. Principal cancellation
records one row per affected execution, including the existing admission-time
cancellation path. Initial pending work needs no event; ordinary release/defer
after a claim records the resulting `pending` or `retrying` status. A rejected
stale lease, rollback or repeated cancellation without a change records no event.

Each row has `event = 'status'`, `status` exactly as
`VerificationExecutionStatus` serializes, the affected execution/Verifier
identities, event UUID and occurrence time. Preserve event identity and time
through outbox retries. The existing required `sequence` is `0` and has no
checkpoint meaning. All other nullable columns, including `value`, are null;
normal managed tenant/subject/actor attribution remains. Discovery reads the
newest status event per execution ordered by `occurred_at`, ignoring checkpoint
and GitHub delivery rows. Equal-time selection is consumer-owned, without a
new causal-order guarantee. Direct executions retain their existing behavior.

Preserve completion's existing `complete_many` group commit and Scribe's
ordinary shared-frame batching. Extend existing UPDATE results with `RETURNING`
where needed for event identity/attribution; add no PostgreSQL statement, read
query or round trip. Staging never waits for or fails the operation because of
the status write. Checkpoints, revision fencing, queue/lease behavior and
recovery are unchanged; recovery ignores status events. A row lost with its
process may leave the list stale. No migration, table, column, endpoint,
reconciliation or crash-durability guarantee is added. The
[finalized status-event design](design/verify-status-events.md) records the
selected implementation flow; TASK-003A supplies the server writes and TASK-003
owns UI consumption. Failed-requirement IDs and new scoped-viewer machinery are
outside this server supplement. Current managed-row visibility applies to the
implemented discovery queries; revision 8 does not restart TASK-003 or introduce
a separate scoped dashboard product.

**REQ-016 — Preserve valid check verdicts.** When an Agent or code check
returns a structurally valid verdict, the engine preserves its authored
`passed`, `failed` or `inconclusive` outcome. An empty citations list alone is
valid and never causes a pass to be downgraded. The ordinary `decide` rule
continues to aggregate requirement outcomes; it does not rewrite them to enforce
an implicit grounding policy. Malformed check output or an invalid supplied
citation is rejected with an explicit validation reason through the existing
check-error path, rather than dropping citations or accepting and replacing the
authored verdict. The existing check-error path may record an inconclusive error
result with that validation reason; it has not accepted and rewritten the
check's authored verdict. Existing schema/declared-data validation,
provider/infrastructure error handling and aggregation are otherwise unchanged. A requirement needing particular evidence remains an
explicit authored rule; no implicit citation-count rule is added.

**REQ-017 — Cite pinned PR evidence through the review workflow.** A successful
`pull_request` extraction makes its retained, accepted PR source an original
citable source using the existing slot/typed-locator citation shape. The source
identifies the exact provider repository and pinned comparison/head commits;
a citation into its extraction JSON resolves only within that execution's
retained evidence. Downstream Agent inputs, derived outputs and workspaces
preserve reachability to that source. Check authors can identify and cite the
source without fabricating a caller input; the source slot must be documented
and usable in the authored example. It cannot collide with or be replaced by
a caller-controlled slot. Out-of-source, wrong-execution or invalid-locator
citations fail validation under REQ-016. Existing tenant/access and retained
source boundaries remain authoritative. Use retained extraction and current
custody/recovery paths; no new storage owner, PostgreSQL evidence, migration or
extra hot-path external wait is authorized.

Update the shipped `examples/review-a-pull-request` workflow and the pinned
`wyrd-example-py` demo to cite this PR evidence in the final requirement, including
clean reviews with no findings. Preserve their decision rule: incomplete review
is inconclusive, critical/high findings fail, otherwise pass. A requirement
citation is separate from finding source anchors: a clean report can have no
finding locations and still cite the PR reviewed. The closed `CodeReviewReport`
format and GitHub rendering contracts do not change. The
[finalized correction design](design/dev-repository-review.md#verdict-and-pr-citation-correction)
records the agreed flow; TASK-004A supplies runtime correction, authoring and
connected proof.

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

**REQ-018 — Explain authorized Bifrost queries.** TASK-004B adds
`EXPLAIN SELECT` and `EXPLAIN ANALYZE SELECT` through the existing public query
operation, including supported CTEs, joins and parameter bindings. Plain
EXPLAIN returns optimized logical/physical plans without executing the data
query; ANALYZE executes the underlying SELECT through its normal selected path
and returns its executed plan with available operator metrics. Both return
DataFusion's tabular plan output through existing query streams and terminals.
They preserve underlying table/tenant/payload authorization, read audit,
admission, resource/output limits, deadlines, cancellation and typed errors.
Explain does not authorize writes, multiple statements, new query endpoints,
credential disclosure or private infrastructure diagnostics. Existing shipped
query clients and the query editor can use the same SQL capability.
[Verification decisions](design/query-verification.md) define the selected
integration and evidence collection. Consolidation covers every existing UI
page/route and uses existing tests, without benchmark runs or `dev:full`.

### PR admission prerequisite

**REQ-019 — Explicit PR-wide fresh review.** Add one tenant-scoped Wyrd write
operation through HTTP, Rust/Python/TypeScript SDKs and MCP. It accepts provider
host, stable repository ID, PR number and a caller-supplied idempotency key;
tenant and caller come from verified authentication. It requests the entire
selected Repository policy's `pull_request` binding set, not a single Check.
Require current `cards:read`, `verifier:read` and `verifier:run` for each
selected Verifier before acceptance, and the existing explicit MCP write scope. Do not let a caller supply
policy, member IDs, source pins or execution authority.

The existing admission owner resolves connected repository access and current
open, non-draft provider source under its per-PR lock and connection fence.
An accepted distinct key creates a new generation even at the same head, freezes
the selected policy and members, and uses the existing PR binding declaration's
accountable execution authority. Existing authority refusals remain terminal
member failures, never a fallback to the requesting caller or SYSTEM. An
unauthorized request is refused before accepting work or disclosing membership.
Closed/draft/inaccessible PRs or a selected policy with no PR bindings accept no
fresh set; unavailable provider/authority state is a typed retryable failure.

The response identifies the accepted generation and its exact member execution
IDs after normal admission handoff/refusal settlement. A retry of the same
tenant/PR/key reuses that occurrence and its frozen policy/members across
concurrency and restart; it never starts another set, even after policy or head
changes. It cannot reactivate superseded work. Ordinary GitHub source-event
redelivery keeps its existing source-replay behavior. New generations retain
existing supersession and lease fencing. GitHub Re-run hooks and a UI rerun
button are excluded. Public names and wire spellings follow owner conventions.

**REQ-020 — Retain complete review membership.** Through the existing verification
input/event and Scribe owners, retain each accepted generation's exact frozen
member execution identities, PR source and selected Repository policy, including
terminal admission refusals. Retain each member's actual status without making
refused members runnable or manufacturing a verdict. A reader must distinguish
incomplete retained visibility from complete membership during interrupted
handoff; a visible subset cannot be presented as the full set. Retrying/recovering
handoff preserves the same identities and does not insert duplicate members.

Existing authorized Bifrost/domain reads supply bounded current/history and
exact membership to TASK-005, independent of its default Verify time range;
delayed older records cannot replace a newer current set. Apply REQ-003's
existing trusted-parent and sensitive-content boundary before counts or member
disclosure. Retained metadata may extend existing verification input/event
records only; no new table, storage family, history endpoint or payload
reclassification is authorized. Persist accepted rerequest identity with its
generation in those existing records for replay. If the existing coordination
schema cannot implement this safely, a named migration requires a further
approved specification revision before implementation.

**Implementation boundary for REQ-019/020.** Extend existing admission and
retention owners; do not change verification execution or other hot paths.

### Retained PR execution inspection

**REQ-021 — Retain PR context and delivery after queue cleanup.** For a PR
execution whose accepted inputs, state, selected result and delivery outcome
are retained in Bifrost, the existing execution-status read returns the same
PR source/generation/member association, selected judgment and recorded delivery
after its terminal PostgreSQL scheduling row is removed. Extend only the
existing verification retained-status and GitHub delivery readers, using their
canonical retained records and existing response fields. Reopening must not
recreate a scheduling row, execute verification or contact/write GitHub.

Apply current caller authorization on the retained parent before disclosure;
inaccessible/unknown exact executions remain indistinguishable. Preserve
independently readable progress/publications and ordinary direct/non-PR status
behavior. An unavailable required retained source is an explicit read failure,
not absent context or fabricated pending delivery. Unprovable parents stay hidden.
The correction restores retained inspection; it does not move registry/auth/live
coordination out of PostgreSQL or promise a PostgreSQL-free request. File bytes
remain with their storage owner. No new endpoint, wire field, migration, storage
family, query service, permission model or execution hot-path change is authorized.
TASK-007 proves this backend output before its UI consumes it; TASK-005/006 do
not depend on this correction.

No visual workflow editor, new verification kind, finding schema, separate PR
dashboard, general Cards redesign, activation/pause/rollback editor or broader
Changes/collaboration product is included. No new server API, response field, query service, authorization behavior,
runtime redesign, scheduler, registry, graph store, browser execution ledger,
event bus or PostgreSQL table is authorized. REQ-016/017 authorize only the
existing runtime verdict validation and PR evidence/citation correction; they
do not authorize unrelated server behavior or a new endpoint. REQ-018 is the
additional bounded exception for explanation through the existing query API;
it does not authorize storage/execution redesign or a separate query service.
REQ-019/020 are the additional bounded exception for one explicit PR-wide write
operation, its normal client projections and complete retained admission records;
they authorize no other server extension or migration. REQ-021 is the bounded
exception for correcting the existing retained-status projection; it adds no
public operation, response field, durable behavior or migration. Internal layout algorithms, cursor encoding,
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
user's 2026-11-12 agreement to this UI-only approach. Revision 4 adds only
REQ-015's approved server status-event supplement. Revision 5 records the
maintainer's agreement to preserve valid verdicts and make pinned PR extraction
citable, with the shipped example/demo corrected and the original passing PR
acceptance retained.

Native Repository, connection, admission, deferred input, approval and delivery
remain backend-owned capabilities with their owning authority amendments.
The earlier backend revision-7 inspection at `54aa8f4a9` is historical.
Revision 8 consumes authorization-model approved revision 9 and the current
retained-parent architecture; the resumption design records inspected source
and historical proof separately. Contract approval is not delivery evidence.
Positive approval receipts retain durable decision acceptance; direct/live
results do not certify retained visibility. This packet neither implements those
semantics nor changes their storage, credential or permission owners, apart from
REQ-019/020's explicit admission extension and REQ-021's read correction under
those same existing owners.

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
read surfaces. Apart from REQ-015's status recording and REQ-016/017's verdict/PR citation
correction, REQ-018's query explanation, REQ-019/020's admission prerequisite
and REQ-021's retained-status projection repair,
server contract, engine and installation changes remain outside this packet; a missing input is an external dependency, not an implementation
assignment here.

The following backend-owned authority amendments are dependencies of the tasks
that consume them. They are not a prerequisite for approving the UI behavior
or starting unrelated work:

| Owning document | Required backend amendment and current boundary |
|---|---|
| `architecture/wyrd-design.md` and root contributor Card inventory | Current native Repository/Card identity, closed PR activation, deferred-input/approval and retained inspection/delivery contracts. Consume them without changing Card kinds or backend ownership. |
| `architecture/wyrd-security-posture.md` | Current trusted-parent visibility, exact-Verifier approve plus parent read and accepted-authority rules. Definitions and GitHub configuration remain independently authorized; no UI-created authority. |
| `architecture/bifrost-design.md` | Current managed-row source visibility and evidence custody. Consume existing authorized queries/reads; REQ-021 repairs the existing status projection without new fields or payload reclassification. |

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
| 003 — Verify discovery | Implemented discovery consumes 002 inspector, 003A retained status events and existing authorized bounded reads. Reuse it; no restart is assigned. |
| 003A — Queued status events | Existing queue transition owners, completion group commit and process Scribe outbox; implements only REQ-015. |
| 004A — Dev repository review and citation correction | Implemented 004 views; existing check/extraction/evidence owners; portable upstream demo files and available pinned commit. Implements REQ-016/017 before completing the passing/failing live PR journey. |
| 004 — Repositories/settings | 001/003 plus delivered connection/access and native Repository/selected-policy operations with owning authority. |
| 005A — PR admission prerequisite | Existing GitHub admission, queue and Scribe owners; implements REQ-019/020 and proves the retained input consumed by 005. |
| 005 — PR review sets | Existing 002/003/004 and 005A admission/retention outputs under current parent visibility; extend shared recorded-selection navigation without requiring a readable definition. |
| 006 — Waiting/approval | 002 inspector and existing occurrence/decision/readback/continuation operations. Reuse 005's shared fallback if available; PR-set UI and 007's correction are not prerequisites. |
| 007 — Delivery status | 002 inspector and 005 PR context; first correct/prove the existing backend retained-status response under REQ-021, then consume it in the UI. |

TASK-005A delivers the explicit Wyrd request and retained membership required by
REQ-019/020 before TASK-005 activates. Specification approval is not delivery
proof. TASK-005 consumes actual integrated operation/query journeys; it still
adds no endpoint, synthesized membership or rerun control. Unrelated UI tasks
can proceed; tasks that depend on 005 still require its completed output.

## Acceptance observations

Use existing public server/Postgres/Bifrost reads for real UI input journeys on
the supported hosts, and real browser journeys for conventional UI behavior. UI fixtures
supplement rendering/error coverage. The mock's 28 browser checks establish
prototype behavior only. Each proof below must assert the stated observation,
not merely the existence of a route/component or a successful command.

| Proof | Situation and expected observation | Requirements (REQ-*) |
|---|---|---|
| AC-001 Discovery and return | Create unfinished and completed ordinary/session/drift/PR executions. Combine status/verdict/general/repository filters, brush both charts, page/group, open a row and return. Counts and rows describe the same authorized population; no-result work has no verdict; discovery context survives. | 001–003 |
| AC-002 Exact graph and publications | Follow an execution link during parallel/repeated work, choose item/iteration/attempt and open intermediate outputs; refresh as work progresses and reopen after completion. Producer/result identity, selection and viewport remain stable, all outputs are peers and unsafe/oversized previews fail visibly without executing content. With no definition grant or a deleted definition, recorded facts remain selectable and graph/YAML remains unavailable without invented topology. Check authorized ordinary/wide graphs and the fallback against the approved design. | 002,004–006 |
| AC-003 Session, drift and citations | Open distinct owner-scoped session reviews, pinned members and an authorized cross-workload/untagged trace beyond list range; inspect drift windows/features/history. Exact recorded membership/judgments remain unchanged after later activity/spans; denied/truncated evidence is visible. | 005,007 |
| AC-004 Waiting and decisions | Exercise a blocked branch with a running peer, exact repeated/fanout occurrences, approved/rejected/expired decisions, stale/conflicting occurrence and denied action. Occurrences remain reachable without completed details/definition access and show lifecycle-based expiry without invented timestamps. Compare parent-read-only, approve-without-parent and both-grant callers; hints never bypass target checks and unsupported hosts issue no write. Read back an ambiguous submission before explicit retry. No decision affects newer source/occurrence; accepted continuation stays on the same execution and accepted authority. | 008 |
| AC-005 Cancel and retain | Cancel unfinished work, race completion and reopen evidence; inspect provider delivery failure/retry independently. Authoritative terminal state wins, publications remain readable and no inspection/delivery action starts replacement verification. | 006,009 |
| AC-006 Repository onboarding | Complete Wyrd-first and GitHub-first connection through public server operations; inspect accessible-but-undeclared, declared-with-empty-bindings, ready and access-lost repositories. View an old policy, follow activity to Verify, return and disconnect. No connection/view selects or creates policy; access loss retains authorized Cards/evidence and reflects backend cancellation. | 010,011,013 |
| AC-007 PR navigation | Admit multiple Verifiers together, rerequest on the same commit, then update the head. The general PR link follows the current set; explicitly selected history/execution stays exact. Membership and source/policy are server-recorded and no combined verdict appears. | 003,012 |
| AC-008 Authorization and hosts | Exercise foreign IDs, Repository family/exact-version and custom-role readers, two Repositories sharing a Verifier, restricted Verifier actions, denied prerequisites and unavailable reads through real HTTP/SDK/MCP/browser adapters. Matching scoped rows/counts/pages/facets expose no other Repository; a table grant alone unlocks no hidden managed row. Independent Verifier-definition/GitHub-configuration denial does not hide readable retained execution facts. Grant/removal affects subsequent authoritative reads in the existing session; denial/unavailability never becomes empty success. Repeat affected reads/actions in supported hosts. | 003,005,008–014,020,021 |
| AC-009 Visual and development journey | Produce representative ordinary, session, drift, PR and approval executions through normal public development workflows; navigate Verify, Repositories and Settings in the real browser. Individually compare components/pages with the approved mock in both themes, narrow/wide layouts and keyboard/touch interaction. No inert fixture action or absent prerequisite is accepted as production completion. | 001,004,007,010–014 |
| AC-010 Retained lifecycle events | Through real public SDK/server operations, cancel pending and running work, revoke a principal with multiple unfinished executions, and hold other work at running/retrying/input-wait/approval-wait before completion or timeout/error. Public Bifrost queries over `event = 'status'` return the expected newest words and specified row fields. Exercise ordinary release/defer returning to pending/retrying. Repeated cancellation, stale settlement and rollback add no event. Focused owner proof preserves grouped completion, shared status-row batching, attribution and retry identity; existing checkpoint recovery and tenant-isolation proof remains passing. | 015 |
| AC-011 Verdict and PR evidence integrity | Exercise Agent/code checks returning valid passed/failed/inconclusive outcomes, including an uncited pass; outcomes remain authored. Malformed output and invalid supplied citations produce explicit validation failures, never silently filtered evidence. Run the shipped PR example and pinned demo through real extraction, review and final code check: a clean review passes with a valid citation to the retained pinned PR source, a high finding fails, and incomplete coverage remains inconclusive. Preserve source reachability through workspace/derived output and existing retained recovery; wrong-execution/tenant/locator citations are rejected. Live demo proof still includes actual GitHub delivery and existing UI visibility under AC-009. | 016,017 |
| AC-012 Public query explanation | Through existing SDK/HTTP/MCP query paths and the query editor, explain a bound SELECT and analyze its real execution. Plain EXPLAIN performs no underlying data execution; ANALYZE retains the SELECT's execution selection and governs its work. Both preserve tenant/table/payload checks and complete-stream semantics. Invalid statements, denied access, resource/output exhaustion, deadline and cancellation remain typed failures without successful partial plans. | 018 |
| AC-013 PR admission prerequisite | Through the real Wyrd HTTP/SDK/MCP operation, rerequest a multi-member PR at the same head, retry concurrently and after restart, then change policy/head and retry the older request. Accepted identities stay stable and obsolete work is never reactivated. Query mixed/all-refused sets and interrupt/recover handoff: every authorized member is retained with exact source/policy/status, incomplete visibility is distinguishable, and denied/foreign callers learn no membership/counts. Refused members never run; ordinary source-event replay remains unchanged. | 019,020 |
| AC-014 Queue-free PR inspection | Complete actual admitted PR work and record delivery, capture the public status, then clean up its terminal scheduling row while Postgres and retained records remain available. A Repository-scoped reader obtains the same PR/generation/members, selected result/verdict and recorded delivery through HTTP, Rust/Python/TypeScript SDKs, MCP and the connected UI. Another Repository/shared-Verifier-only/foreign reader remains refused or not-found as applicable. The browser reopens a pinned set/member after cleanup; provider failure/skip/retry stays separate from the judgment, and no queue row, execution or provider write is created by inspection. | 002,005,006,009,014,020,021 |

## Approval and revision history

Revision 8 records the maintainer's explicit 2026-11-23 instruction to revise,
approve and commit the selected spec/task amendments. It aligns reads with
authorization-model revision 9 and current retained-parent authority, preserves
independent definition/configuration checks, and records the agreed fallback,
approval lifecycle/action presentation and existing delivery projection.
REQ-021/AC-014 assign TASK-007 the bounded backend retained-status repair before
UI consumption; TASK-003 stays implemented and 005/006 do not depend on that
repair. No new endpoint, wire field, migration, permission model or PostgreSQL
outage guarantee is approved. Revision 7 is preserved in
[the prior revision](spec-revisions/revision-7.md). Approval establishes scope,
not implementation or independent acceptance.

Revision 7 records the maintainer's 2026-11-19 approval of the backend
prerequisite and subsequent selection of the recommended Wyrd API/SDK/MCP
entry point. REQ-019/020 and AC-013 authorize TASK-005A before TASK-005: one
PR-wide idempotent fresh-review operation and complete retained membership,
including refused members, through existing admission/Scribe/storage owners.
Trigger, evidence-custody and security authority are reconciled in the same
documentation change before runtime implementation. Revision 6 is preserved in
[the prior revision](spec-revisions/revision-6.md). No migration, GitHub Re-run
hook, UI rerun action or implementation claim is approved by this revision.

Revision 6 records the maintainer's explicit 2026-11-18 direction to add public
EXPLAIN to TASK-004B rather than use a diagnostics workaround. REQ-018/AC-012
extend the existing query SQL surface; the selected design includes ANALYZE
for executed-work verification. TASK-004B audits and optimizes all UI routes
using existing tests without benchmark runs or `dev:full`. Other tasks retain
their approved scope and evidence. Revision 5 is retained in
[the prior revision](spec-revisions/revision-5.md). This authorizes planning,
not a claim of implementation or permission to commit/push.


Revision 5 records the maintainer's explicit agreement to the verdict/PR citation
correction and request to revise the spec and tasks. It authorizes REQ-016/017
and AC-011 within TASK-004A, retaining its clean-PR passed/success expectation.
The owning Verifier citation paragraph in `architecture/wyrd-design.md` is
amended with this documentation revision before runtime implementation.
Revision 4 is preserved in [the prior revision](spec-revisions/revision-4.md).
This approval does not certify implementation, change other tasks' acceptance,
or authorize commits or pushes.

Revision 4 records the user's explicit approval on 2026-11-16 of the status-event
supplement and its proposed release/defer correction. It authorizes REQ-015 and
AC-010 only as the exception to the earlier UI-only implementation boundary;
checkpoints and recovery remain unchanged. TASK-003A implements the server part,
and TASK-003 consumes its recorded rows.

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
| 4 | approved (2026-11-16) | Add REQ-015/AC-010: queued lifecycle status rows through the existing Scribe outbox, retaining completion group commit, shared batching and unchanged checkpoints/recovery. Include release/defer status recording; no migration, new endpoint, additional PostgreSQL statement or crash-durability guarantee. |
| 5 | approved (2026-11-17) | Preserve valid Agent/code check verdicts; reject invalid supplied citations explicitly; make retained pinned PR extraction citable and carry provenance through the review/check flow; correct shipped example/demo and retain clean-PR passed/success. TASK-004A owns this bounded runtime and development supplement. |
| 6 | approved scope (2026-11-18) | Add maintainer-requested public EXPLAIN and selected EXPLAIN ANALYZE in TASK-004B; all-route consolidation verified with existing tests and query plans, without benchmarks or dev:full. |
| 7 | approved (2026-11-19) | Add TASK-005A under REQ-019/020 and AC-013: explicit Wyrd PR-wide rerequest via HTTP/SDKs/MCP, replay identity and complete retained membership including refusals. Preserve UI exclusions and existing admission/security owners; no migration authorized. |
| 8 | approved (2026-11-23) | Align tasks 005–007 with current Repository-parent reads, definition-independent recorded selection and exact approval permissions/lifecycle; add REQ-021/AC-014 for the existing queue-free PR/delivery status correction in TASK-007. Preserve implemented TASK-003 and existing wire/storage boundaries. |
