# Verification workflows and repository UI

Status: finalized

The centralized visual design remains approved and `design/mock/` remains the
sole approved mock. The user finalized the existing-read, UI-only approach on
2026-11-12 and requested revised tasks. Specification revision 3 records that
agreement; TASK-001 stays complete and TASK-002 through TASK-007 remain proposed.

Verify discovery opens exact workflow pages. Repositories is a separate visible
workspace; the existing Cards UI is not redesigned. Repository identity remains
a native Card. Ordinary, session, drift, PR and approval-wait examples share the
same graph and contextual inspector. The original canvases remain references.

## Problem

The original verification UI explains declared Verifiers and completed judgments.
Generic long-running workflows also need discoverable active executions,
inspectable parallel/repeated work, retained intermediate outputs and human
approval. Repository onboarding must connect those same verification primitives
to GitHub without treating every connected repository as a registered or active
Repository Card.

This is a separate change packet: `verification-workflow-ui`. The original UI spec/tasks and canvases remain linked references until their
overlapping scope is explicitly transferred; they were not silently revised.
Superseded mock folders were removed with user authorization after approval of
the centralized mock.

## Desired outcome

One Verify workspace combines the original dashboard with the shared execution
inspector. A check's recorded content determines its contextual detail; code
review, drift and session inspection do not become competing verification apps.
See the [consolidation proposal](consolidation.md) and
[reference index](README.md) for the combined visual plan and working prototype.

An administrator connects the deployment's GitHub App. A team declares a normal
versioned Repository policy outside the UI. An engineer finds its queued,
running, waiting or completed verification in Verify or follows a PR link,
understands the workflow, inspects exact retained outputs, makes an authorized
approval decision or cancels unfinished work, and reopens the same evidence
later. All verification workflows use the same viewer.

## Reopened UI-only contract alignment (2026-11-12)

The user agreed this approach and requested task revisions on 2026-11-12.
The direction is to adapt the approved UI to
existing verification state and stored data, accounting for the resource-scaling
contract changes. Do not redesign the server to support the UI.

Fixed boundary: no new server fields, endpoints, domain query services, storage,
execution logic or authorization changes. The earlier proposed status-context,
drift-context and discovery API extensions are rejected. This finalized boundary
supersedes those earlier implementation proposals. Do not remove approved visual
features merely to simplify this investigation.

Retained data already lives in Bifrost. Compact VerificationResult is a response
projection, not deletion of detail from storage. Reuse existing Bifrost query
operations and existing domain status/details/invocation/publication/action
operations under their current permissions. UI responsibilities are field
projection, loading/availability, graph/inspector rendering, selection/navigation
and supported-host adapters.

The field-to-existing-read map carries this agreement into the revised tasks.
Record unsupported or undelivered inputs as specific external dependencies; do
not turn them into backend implementation assignments or invent state. [Source research](contract-alignment-research.md) is the current
map and supersedes the discarded backend-expansion proposal.

TASK-001 remains complete. TASK-002 needs result/details/live-record adapter work,
missing runtime presentation and shared graph adaptation for current Step types.
TASK-003 should use existing permitted Bifrost queries instead of its proposed
new server discovery APIs. Other tasks consume those same components/read owners;
existing external capabilities remain dependencies, not work for this UI packet.
The task revisions preserve IDs/order and expose remaining input dependencies
without changing their server owners.

Specification revision 3 records the user-agreed boundary. TASK-002 through
TASK-007 are revised as proposed work; TASK-001 keeps its historical completion
and revision-2 proof. No production code or branch integration is part of this
planning pass.

## Agreed decisions

- User direction: consolidate all verification into one decisive UI, including
  the original dashboard, Verifier Card, drift and session views. Preserve their
  content rather than treating the workflow-only prototype as their replacement.
- Preserve the approved Wyrd visual system and existing verification canvases.
  The workflow mock was approved on 2026-11-09; repository and real Svelte Flow
  previews were approved on 2026-11-10.
- Use Svelte Flow with custom Wyrd nodes/edges and automatic layered layout.
  Keep the graph above its contextual inspector. Status updates preserve graph
  position, viewport, selected node and exact output producer.
- Author workflows through YAML/SDK and Repository policy through normal
  TOML/YAML declaration. The viewer is read-only apart from authorized domain
  actions; it is not a visual workflow editor.
- Verify lists active and completed executions, with lifecycle-status filtering
  alongside verdict and existing filters. Each execution is individually openable
  and has a copyable exact link.
- A combined PR link shows the latest review set by default, with earlier sets
  selectable. Each set can include several Verifier executions. Exact execution
  and explicitly selected historical-set links stay pinned.
- All outputs appear equally, using their actual names. No author-selected
  primary output, filename heuristic or final-step shortcut.
- Authorized users can cancel unfinished work and Approve/Reject a pending
  human-approval occurrence in the existing inspector. Accepted progress and
  publications remain inspectable after cancellation.
- Manage repositories as native Cards with ordinary exact versions and selected
  policy. Registration, selected policy, GitHub access and automatic verification
  readiness are distinct. Viewing an old version never activates it.
- Provide admin GitHub integration settings, a repository index and Repository
  Card detail with verification activity/PR context. Reuse Verify for execution
  investigation; no separate PR dashboard or repository-specific DAG viewer.
- The Rust server owns durable behavior, permissions, verdicts, membership,
  approval, cancellation, recovery and provider delivery. UI components and
  loaders share the existing browser/SDK/MCP host boundaries.

## Constraints and non-goals

Code review is an authored workflow, not a Verifier kind. The generic UI adds no
finding/report schema or preferred-output contract. The backend's optional
standard report for GitHub delivery does not make that output primary in the UI.

No new workflow runtime, registry, scheduler, browser execution ledger, approval
dashboard, event bus or PostgreSQL table. No App-key upload, tenant App creation,
second manual connection-confirmation feature or separate integration-disable
product. No UI-only permissions or fresh-login policy. Do not reclassify sensitive
Bifrost payloads or use public SQL to bypass domain read permissions.

The selected native Repository/deferred-input/approval/GitHub semantics remain
external backend contracts. The refreshed source map records revision 7 and
distinguishes approved semantics from actual delivered operations. Its dedicated installation table is backend-owned;
this UI packet adds no table. That work is a delivery and
authority-update dependency; design approval does not imply it is implemented.

## Implementation proposal

### Shared Verify discovery and navigation

Use existing bounded Bifrost queries and delivered domain operations for the
summary/filter and review-set presentation required by the approved list. The current results-only
list cannot discover executions that have no result yet. Reuse accepted execution
and admission records; do not have the browser enumerate status for every ID or
assemble review membership from timestamps, names, commit SHA alone or caller
session/group text.

The list carries exact execution/Verifier/subject identity, status, nullable
recorded verdict/result selection, existing correlation fields and repository/
PR/review-set association where applicable. Filters, facets and charts describe
the same authorized population. Status and verdict filters combine normally;
running/waiting work has no verdict, and incompatible combinations are empty.

Review membership comes from the server's exact selected Repository policy,
source and review occurrence. The general PR link follows its current review
set across source updates. History selection and individual execution links
remain exact. Live list refresh never replaces an open exact execution or
selected historical inspection. Consume that association only when an existing authorized surface supplies it;
missing accepted membership remains an external dependency. No review engine,
server projection or Card kind is added here.

Latest user direction: selecting a list execution opens its workflow page,
preserving list filters/range/position on return. The current mock has no quick
drawer; the earlier drawer remains a reference, not a required extra click.
Reuse the approved simple, session and drift
inspection modes. The same definition graph can render without runtime coloring;
only recorded state for the selected execution supplies runtime status/verdicts.

### Svelte Flow graph and contextual inspector

Render the exact declared inputs, dependency edges, steps, attached requirements
and whole-Verifier requirements. Independent branches stay visibly parallel.
Use collapsed authored step nodes for fanout/repeat, with item/iteration/attempt
selection in the inspector instead of expanding every instance globally.

Dagre is the demonstrated starting layout choice. Compute layout from topology
and dimensions, not progress. Use Svelte Flow for viewport and interaction;
disable dragging, reconnecting, deleting and other authoring gestures. Keep
keyboard/focus behavior and a usable mobile pan/zoom surface.

The inspector shows a selected step's recorded state/reason and instances, or a
requirement's verdict, reason, actual, citations and judge. Inputs start collapsed.
Action success, requirement verdict, speculative disposition and overall execution
status are separate facts. Selecting another instance pins outputs to that exact
producer. New failures or publications do not steal the user's selection.

### Outputs, waiting and decisions

List retained publications as metadata; fetch bodies only when opened. Preview
bounded JSON, safe text/Markdown and directory manifests; download other files
through authorized domain operations. No raw HTML execution, whole-tree download
to paint a list, workspace-edit stream or token stream is implied. Partial and
discarded work is attributable and never presented as a successful judgment.

Show waiting-for-evidence input names and waiting-for-approval messages separately.
A blocked branch can coexist with running peers; waiting is nonterminal and has
no final verdict. Approval inspection identifies the exact server-issued occurrence,
step/item/iteration, expiry and current decision state.

For authorized pending approvals, show Approve and Reject with an optional reason.
Submit the existing server action with that occurrence identity; show pending,
denied/error, and authoritative accepted decision states. Normal current scoped
authorization applies, with no new login ceremony. A stale/expired/conflicting
occurrence is refreshed and explained, never silently applied to a newer occurrence.
An ambiguous submission is resolved through the recorded decision/readback rather
than automatically issuing a new decision. The same execution resumes under its
existing accepted authority; the UI never starts replacement work.

Cancel uses the existing domain action for unfinished queued/running/waiting work.
Disable duplicate pending submissions and read back server state. If the execution
finishes first, preserve its terminal result. Cancellation retains progress and
publications and does not promise to retract provider IO already sent.

Denied, unavailable, missing, empty, not-yet-visible, skipped, expired and terminal
error states stay distinguishable. GitHub delivery status is separate from the
verification verdict; delivery retry never becomes execution retry.

### GitHub settings and Repository Cards

Settings → Integrations → GitHub consumes the existing singular connection flow:
Connect → provider install/authorization → server callback-completed association.
Support both Wyrd-first and GitHub-first entry. Show safe connection/access state,
manage-access navigation and authorized disconnect. No second manual confirmation
or App-secret forms. Disconnect/access removal explains affected cancellation
while evidence and registered Cards remain retained.

The repository index displays registered Cards and accessible provider repositories,
with clear no-Card states. Stable provider identity joins bounded display projections;
it grants no authority. Card read permission and integration read permission remain
independent. If access state is unavailable, do not guess readiness.

Repository detail reuses the Card header/version navigation and shows selected
policy, provider access, declared exact Verifier bindings and activation, read-only
policy, and recent verification activity. A disconnected or access-revoked Card
remains inspectable through authorized Card reads. Empty verified_by means no
future automatic verification, not disconnection. Viewing another version changes
only the displayed policy. TOML reconstructed from a Card is labeled as a projection,
not presented as the original source file.

### Connected flow, inputs and consumers

| Actor and input source | Existing owner produces | UI consumer and observable outcome |
|---|---|---|
| Administrator starts Wyrd/GitHub onboarding | Server-verified tenant connection and repository access | Settings shows authoritative access; no Cards/bindings appear merely from connecting. |
| Team declares TOML/YAML through public authoring | Registered exact Repository graph, selected policy and bindings | Repository Card shows separate registration/access/readiness and exact versions. |
| GitHub sends an eligible PR event | Ordinary queued Verifier executions with frozen source/policy and membership | Verify discovers unfinished work; combined PR link opens the current set. |
| Engineer selects an execution | Authorized exact state, invocations and selected result | Graph/inspector show recorded progress without inventing a verdict. |
| Engineer opens an output or citation | Exact producer publication or authorized evidence | Bounded readable content with original identity; list predicates do not restrict exact citation reads. |
| Authorized user approves/rejects or cancels | Server-accepted decision/cancellation receipt and updated state | Pending/race/refusal handled locally; no duplicate work and retained evidence remains readable. |
| Work completes or GitHub delivery retries | Selected terminal judgment and separate delivery state | Final verdict stays stable; reopening shows the same accepted evidence. |

### Ownership and reuse

Use one component/loader implementation across the gallery, conventional UI,
custom consumers and supported MCP App artifact. Extend the existing closed read
contract, Node/MCP executors and host action intents. Existing server operations and
normal SDK/MCP projections remain the read/action owners; this packet adds UI
adapters and no server contract or durable workflow.

This packet owns the shared verification viewer, summary/review-set
UI queries/adapters over existing surfaces, Repository presentation and GitHub
connection UI. The backend
pipeline packet owns its engine/integration semantics and authority amendments.
Transfer overlapping verification scope from original TASK-007 and repository
scope from TASK-009 through explicit spec/task reconciliation. Retain their
session/drift, exact evidence, host and journey obligations. Broader Changes/
collaboration work stays in the original UI packet.

## Evidence and rationale

Approved visual references:

- [Centralized mock](mock/dist/index.html) and [build/check instructions](mock/README.md)
  are the sole approved combined visual target.
- Original [Verifier](../references/original-canvas/Verifier.dc.html),
  [Verify](../references/original-canvas/Verify.dc.html),
  [session](../references/original-canvas/VerifySession.dc.html)
  and [drift](../references/original-canvas/VerifyDrift.dc.html) canvases preserve
  the original foundation. Copies are byte-identical to the old packet's sources.
- [Composition](consolidation.md) records the connected experience;
  [retained rationale](mock-brief.md) explains the corrections that prevented
  chart/layout drift and incorrect Repository navigation.

[Source alignment](source-alignment.md) records inspected owners, callers/tests,
missing projections, authority boundaries and exact prototype check commands.
The refreshed source map records UI `9c38fdfc3`, resource-scaling `443a79337`
and newer integrated backend `54aa8f4a9`. Earlier 2026-11-10 worktree/revision
references are historical evidence, not current dependency-delivery claims.

The graph-plus-inspector pattern was informed by GitHub Actions and Airflow;
local Mitari and official CodeRabbit/Devin documentation informed the separation
of provider access, configuration and activity. Wyrd retains its own Card and
verification contracts. The earlier recommendation to pin every combined PR link
was rejected in favor of latest-by-default with history. All outputs remain equal,
even where backend GitHub delivery understands an optional standard review report.

The centralized mock passed 28 browser checks and independent visible-click
inspection. These establish prototype behavior only; user approval establishes
the visual direction. No production UI/backend journey has been run.

## Observable success

Implementation must prove the following through the real public server and
supported browser/SDK/MCP paths, using existing test owners:

- Connect a repository, declare its policy, trigger a PR and discover actual
  queued/running verification through the same normal development workflow.
- Open intermediate output, change item/iteration/attempt, refresh and observe
  exact identity preserved without node/viewport jumps or invented verdicts.
- Exercise waiting with a running peer; approve/reject an exact occurrence,
  include expired/stale/denied decisions, and observe authoritative continuation
  of the same execution. No decision may affect a replacement source/occurrence.
- Cancel unfinished work and race completion; reopen retained progress/outputs.
- Navigate current and previous review sets, including rerequest on the same
  commit and a new PR head. Membership is exact; historical selection stays pinned.
- Inspect a registered-but-inaccessible Card and a connected-but-undeclared
  repository. Viewing a historical Card never selects it. Disconnect changes
  access/unfinished work without deleting Cards or evidence.
- Denied/foreign/unavailable reads and unsafe or oversized previews fail visibly
  without disclosure, follow-on sensitive reads, invented emptiness or verdicts.
- Preserve existing session/drift judgments, exact trace citations and host parity.
  Verify keyboard, mobile and light/dark behavior against the approved references.

These observations are proof requirements for the agreed flow, not claims that
those tests already exist or pass. Planning fixes exact selectors after owner
revalidation; no prototype replaces a real user journey.

## Standards conflicts and dependencies

No departure from the client/server, tenancy or sensitive-payload standards is
approved. Native Repository and approval permissions need their owning approved
backend authority amendments. The original UI task's results-only list and older
repository configuration must be explicitly superseded by the new spec. A mock
never overrides those contracts by itself.

## Remaining implementation-owned choices

The combined mock and behavior are approved. There are no remaining product
decisions for this design. The spec records UI behavior over existing public read/action
owners. Missing input capability is a dependency, not a new backend assignment.
Exact internal query/layout/cursor mechanics remain implementation-owned. Do not
add another storage or orchestration owner.

Resolve the prototype's zoomIn/zoomOut anomaly during production integration
rather than blindly copying its setViewport workaround. Off-screen keyboard
selection needs usable viewport behavior. Node sizing, grouping mechanics,
refresh scheduling, component decomposition and supported-host packaging remain
ordinary implementation work within the accepted visual contract.

## Handoff

The [specification](../spec.md), revision 3, records the user's 2026-11-12
agreement: adapt the approved UI to existing Card/Bifrost/verification reads and
the compact/details split, with no underlying server changes. The visual design
and original scope transfer remain intact. Revised TASK-002 through TASK-007
carry the existing-read map, inputs, consumers and proof; TASK-001 stays complete.

The external admission owner still needs to confirm the supported same-commit
review-request input and replay identity and expose accepted membership to an
existing authorized read. That dependency remains on TASK-005, without adding a
UI rerun action or a new server read. Other tasks depend only on their named
inputs. Integration and real input proof, not another packet's task numbers,
establish readiness.

The connected flow is Verify → exact workflow → contextual evidence → return;
Repositories → policy/activity/PR review set → the same execution; Settings →
GitHub connection. The approved mock passed 28 real-browser checks, including
visible navigation from default Verify through Repository detail, Settings and
no-Card instructions. Production proof remains the real journeys listed above,
not simulated fixture checks. Wide tables scroll internally and wide workflows
support zoom/pan. Production viewport/keyboard behavior needs real-host validation.

The original canvases remain unchanged. The three superseded mock folders were
removed with user authorization; `mock/` is the sole mock. No production code,
commits, staging or pushes are part of this cleanup.
