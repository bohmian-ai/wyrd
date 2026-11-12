---
id: SPEC-github-verifier-pipelines
revision: 7
status: approved
---

# GitHub Verifier pipelines

A connected repository declares its review policy once. Pull requests then run
that policy through Wyrd's existing verification queue and engine, with exact
source evidence, optional later inputs and ordinary Wyrd verdicts. GitHub shows
the result; it does not own execution or review policy.

## Problem and proposed change

Repository review needs one declared policy, automatic PR admission, pinned
source evidence and durable execution. Later evidence and human decisions must
allow the same review to continue without occupying workers or restarting
durably accepted completed work. Unpersisted execution progress may be lost on
process death as specified in REQ-017. Results must reach GitHub without making GitHub the execution
owner or requiring a UI.

This change connects repository declaration, GitHub events and evidence
publication to Wyrd's existing verification queue and engine.

The operator registers/configures the deployment's GitHub App. A tenant installs
it and selects repositories. In an authorized repository the user writes:

```toml
[repository.verified_by]
verifier = ["./review/verifier.yaml"]
runs_on = { kind = "pull_request" }
```

`wyrd declare wyrd.toml` infers a native Repository Card, resolves the chosen
Verifier graph, completes registration and selects that exact policy. Opening or
updating an eligible PR queues its bound Verifiers. Extraction returns pinned
changes and bohmian-code-review's actual bounded source context. A step may
consume evidence published later or require a human decision; parked work holds
no worker capacity. One optional standard report supplies GitHub findings and
suggestions; authored requirements still determine the verification verdict.

The cost is new Repository, PR extraction, deferred-input, approval and provider
integration contracts at existing owners, one dedicated installation association
table and the independent library's required snapshot/context metadata tables.
There is no new execution engine, general event bus, generic wait framework
or UI workflow.

## Scope and fixed boundaries

- Rust server owns durable behavior; Rust, Python, TypeScript, CLI and HTTP/MCP
  expose equivalent operations. Schemas/errors come from wyrd-spec. No UI work.
- The independent library owns generic GitHub acquisition, pinned comparisons,
  indexing and context selection. Wyrd owns tenant access, Cards, bindings,
  accepted execution state, storage adapters and GitHub delivery policy.
- Existing Card registry, storage/upload, Bifrost/Scribe, verification queue,
  SubmissionSink, runner, checkpoints and engine remain the owners. No new
  PostgreSQL tables except `wyrd.github_installations` for GitHub associations
  and Task 004's required library snapshot/context metadata tables. Reuse the
  existing PostgreSQL deployment.
- This is the same implementation for SaaS, self-hosted and enterprise cloud.
  The provider is GitHub.com using its documented GitHub App installation
  and OAuth flow. Development uses local protocol mocks. Enterprise Server
  support and provider capability negotiation are outside this change.
- No source commits/autofix, merge approval, automatic thread resolution,
  general signal buffer, timer wait action or CI-provider-specific integration.

## Required behavior

### Connect and declare

**REQ-001 — App connection.** The operator configures one App for the deployment
through existing configuration and secret providers. A tenant completes verified
installation association and repository selection. Enforce one connected
installation per tenant and one tenant per installation. Disconnect stops
connection use and affected unfinished work before releasing its association.
Use GitHub.com's documented setup and OAuth callback flow, including its
standard state and PKCE protections and installation confirmation. The tenant
completes this ordinary flow once to connect the App.

`wyrd-sql` owns one dedicated `wyrd.github_installations` table for tenant identity
(`data_tenant_id`), configured host, verified installation ID, selected accessible
repositories, suspension and revision, with connection identity and safe timestamps
as needed. Postgres enforces one associated installation per tenant and unique
`(host, installation_id)` across tenants, including concurrent claims. Suspension
retains ownership. Enable and force standard tenant RLS. Keep
`auth_human_connections`, its OIDC constraints and queries unchanged. An
installation grants repository access, while that table defines tenant OIDC login
trust; sharing it would require nullable OIDC fields and provider filtering in
security-sensitive login queries. The dedicated table avoids that coupling. Reuse
`auth_login_state` for temporary actor/tenant/browser-bound connection transactions
with explicit flow separation: neither callback may consume the other's state,
and stale callbacks cannot restore a disconnected or replaced connection. One
narrow SECURITY DEFINER installation-to-tenant lookup routes verified webhooks,
with pinned search_path and body-scoped operator elevation; subsequent work uses
TenantConn and normal authorization. Routing grants no permission. Store no App
keys or user/installation tokens in these rows. Private columns and generation
mechanics remain implementation-owned.

App permissions are Contents read, Pull requests write and Checks write, with
required PR/source and installation-access events. Tenant users never upload App
keys. Missing optional App configuration makes only GitHub integration unavailable;
invalid configured values use normal typed configuration validation. Background
work uses installation tokens and narrowly authorized tenant execution authority, never a
persisted personal GitHub OAuth token. Connection alone does not register Cards.

**REQ-002 — Native declaration.** TOML is an inferred Repository root, not a
second binding command. Origin remote is a hint; the server verifies stable host
and repository ID against the tenant connection. Infer the first Card name;
verified overrides win. Missing/ambiguous identity refuses clearly. Provider
rename preserves the established Card family. Resolve only selected Verifiers
and transitive dependencies, not unrelated YAML. Normal YAML Repository Cards
use the same lifecycle. Existing YAML/directory behavior remains supported.

**REQ-003 — Selected version.** Normal Card hashing/version rules apply. Changed
content cannot overwrite an exact pin. Success means graph/artifacts ready and
that Repository policy selected. Failure preserves the old selection without
deleting already registered dependencies. Distinct concurrent activations from
the same prior selection conflict; identical targets may reuse. Retry or delayed
completion cannot undo a newer selection. Intentional new declaration of an old
exact version supports rollback. The binding list is complete desired state;
`verifier = []` removes future automatic review without disconnecting access.
Offline `--check` validates locally and identifies deferred remote checks. Return
normal Card receipts and server-managed binding status.

### Admit, extract and review

**REQ-004 — PR admission.** A verified PR event resolves the connected repository
and selected registered bindings, then stages ordinary queued verification.
Freeze exact Repository, Verifier, Trigger, authority, PR base/head and reviewed
source identity. Do not import policy from the PR branch or execute a DAG in the
webhook request. Open/reopen/ready and source head/base changes are eligible;
drafts do not start. Conversion to draft, closure or access loss cancels affected
unfinished work. Metadata-only edits do not run. Policy changes affect future
admissions, not every existing open PR.

**REQ-005 — Supersession.** New PR source supersedes obsolete unfinished reviews:
coalesce replaced queued heads and cancel running/parked older work. Delayed
events cannot restore an obsolete generation. Preserve historical accepted work,
but reject obsolete continuation and current-result delivery. A provider request
already sent may finish; pin its source and reconcile rather than promise atomic
cancellation across GitHub and Wyrd.

**REQ-006 — Pinned extraction.** Add `action: {kind: extract, extractor:
pull_request}` while retaining existing `extract/from` media behavior. Queued
activation supplies the authorized pinned PR; missing source identity refuses
before acquisition. Use the independent library's public pipeline extended for
pinned PRs and optional previous-reviewed head. Return the typed
pr/review/changes/context output in [contracts](contracts.md#pull-request-extraction).
Retain the actual library ReviewContext unchanged and enough source/diff evidence
to verify findings and edits after recovery or library cache expiry.

Implement the library's `ObjectStorage` over Wyrd's configured `object_store`
backend and a PostgreSQL-backed metadata owner implementing both `SnapshotStore`
and `ContextStore`, including required metadata tables/migrations. Preserve
identity scoping, conditional publication and collection, context lifetime and
cleanup contracts as detailed in the
[Task 004 storage amendment](design/approved-code-review-support.md#task-004-storage-amendment).
PostgreSQL stores pointers/records/lifecycle metadata; object bytes remain in
object storage. Library cache publication is separate from accepted execution
evidence in Bifrost, which survives cache expiry.

The last completed usable review of the same PR and policy is the incremental
baseline. Partial/cancelled work never advances it. P completed, A running, B
pushed means cancel A and review P→B. Without usable history, after rewritten
history or changed policy, use full PR review. Keep PR base/merge-base distinct
from comparison baseline. The library computes comparisons/context; Wyrd does
not duplicate selection or indexing.

**REQ-007 — Normal evidence consumption.** Entire extraction output or individual
fields bind through ordinary Agent inputs and workspace mappings. Prompt binding
injects actual selected source text; workspace JSON holds the same validated live
values. Dependent consumption does not wait for checkpoint/Scribe acceptance;
recovery reuses only actually persisted exact extraction evidence (REQ-017). No filesystem-discovery step is required, no implicit injection into
unrelated Agents, no invented evidence publication name and no required source
checkout. Pinned Agent/Prompt owns policy; repository guidance comes from pinned
base source, while PR edits to guidance are evidence. Reuse library context and
existing model limits; oversized input is an honest failure, never a clean review.
Authors may use existing DAG/fanout primitives; no automatic splitting subsystem.

### Supply later evidence and human decisions

**REQ-008 — Deferred subscription.** `deferred: true` on a typed input subscribes
that name when its binding becomes active. References establish dependent steps.
Missing deferred evidence parks its consumers; independent work continues.
Publication alone does not start a run. A binding subscription exists even when
no execution is running. This is an input dependency, not a new Trigger kind.

**REQ-009 — Publish and match.** `wyrd evidence publish ci_results --file
results.json` and equivalent SDK/HTTP operations publish evidence without a
Verifier/execution ID. Infer repository from Git remote and commit from Git,
with explicit identity/commit overrides where necessary. Match authenticated
tenant, stable repository identity, commit tag and input name. Evidence concerns
source, not the Repository Card's review-policy version; executions retain their
exact policy pins. Never repurpose group_id
or require a publisher idempotency key. Authorize subject publication and validate
subscriber type/format. Identical accepted identity/content replays its receipt;
conflicting replacement refuses. Evidence cannot cross tenants or source commits,
and new subscribers do not receive historical evidence automatically. No public
subscription-reactivation or rollback protocol is introduced.

| At publication | Required observation |
|---|---|
| Waiting execution | Accept input; queue continuation of the same execution |
| Running/queued execution | Accept input; normal scheduling releases consumers |
| Active subscription, no execution | Retain for that subscription under normal retention; later matching execution can consume it |
| No active matching subscription | Explicit unmatched result; no retained publication or historical replay for future subscriptions |
| Completed/cancelled/superseded execution | Do not reopen it |

Execution initialization checks accepted matching evidence as well as processing
new notifications. Publication acknowledgement confirms durable evidence
acceptance, not review completion. Partial uploaded bytes are not publication.

**REQ-010 — Human approval.** A dedicated approval action declares message and
expiry. Pending occurrences have exact server-issued identities. An authorized
user may approve/reject through the shared execution surfaces using normal
authentication and authorization. Require the approval permission and current
subject authority. Do not require recent interactive proof, a special login
flow or an authentication-age threshold. Record approved/rejected/expired with actor,
time and reason. An identical authenticated retry replays; a conflicting or stale
occurrence refuses. No authority expansion, quorum or silent approval of a new
source head. Rejection is data, not a crash; consumers requiring approval must
explicitly require approved. A standard helper fails rejection/expiry.

**REQ-011 — Park and resume.** When no branches remain active/runnable, release
execution, workspace, Python and model capacity immediately. Bounded background
custody retains/renews the claim and acknowledges the canonical pending checkpoint
before retiring its current queue delivery and releasing the renewing lease.
No execution permit remains attached to that ACK. Continuation restores the same
execution and accepted state without replaying durably settled predecessors or
replenishing accepted spend/retries or the original deadline. A crash before
checkpoint acceptance can repeat unpersisted work under REQ-017. Missing evidence
and approval expiry remain bounded by the original execution deadline; operator
execution ceilings must support intended approval periods. Cancellation,
revocation and supersession apply while parked.

### Deliver and operate

**REQ-012 — Report convention.** `response_format: code_review` resolves the
optional standard report schema; normal code may produce the same envelope.
Intermediate reviewers can reuse Finding schemas, and a normal consolidator
returns the final envelope. Inspect accepted successful outputs per execution:
zero means normal summary, one means render that report, multiple means explicit
ambiguity. Invalid/unsupported envelopes are visible delivery errors. Failed
attempts, discarded speculation and intermediate repeat outputs do not count.
No Verifier/binding selector or automatic concatenation. Report selection never
changes an accepted verdict. Provide the agreed optional helpers for report
validation, aggregation/sorting, finding-ID preservation and authored severity
thresholds through ordinary review outputs/checks. They do not introduce a
mandatory preparation stage or another review engine.

**REQ-013 — GitHub presentation.** Publish a Check and valid report findings as
PR comments/suggestions, pinned to reviewed source. Severity and confidence stay
separate from verdict. Critical/high map to failure annotations, medium warning,
low notice; unlocated findings stay in summary. Passed→success, failed→failure,
inconclusive→action_required, cancelled→cancelled, timed_out→timed_out,
errored→failure. Pending evidence/approval uses in_progress with explanation.
Preserve finding identity and reuse comments where possible. Do not auto-resolve
threads because a finding is absent from an incremental report. No automatic
APPROVE, REQUEST_CHANGES or source modification is authorized.

**REQ-014 — Recoverable delivery.** Retry GitHub delivery from retained report and
progress independently of computation. Reconcile ambiguous Check/review/comment
writes before retrying; never blindly append annotations or duplicate comments.
Respect provider cooldowns, bound retries, and expose permanent/ambiguous failure
rather than pretend success. Recheck access and current generation at each
write. Existing execution inspection exposes delivery state separately from
execution verdict. No new result mirror or finalizer service.

**REQ-015 — Resource isolation.** Direct and queued execution, workspace, Python
and model-call admission use separate capacity without borrowing. PR extraction,
deferred inputs and approval are queued-only, including manual calls; refuse a
direct request before any step. Ordinary direct-eligible Verifiers retain their
path. Same engine and tenant/evidence rules apply. A burst of 41 PRs is durably
accepted under bounded ingress and progresses through the existing fair queue;
there is no completion-time SLA for arbitrary DAGs.

**REQ-016 — Surfaces and operation.** Ship typed Rust/Python/TypeScript, CLI and
HTTP/MCP operations, stable errors, schemas, inspection and focused user guides
for the introduced capability. New writes require domain permission and MCP
write scopes; reads remain discoverable. Expose isolated capacities and original
execution ceiling through normal operator configuration. Documentation teaches
App setup, declaration, publication, approval and diagnosis without a UI.

**REQ-017 — Memory execution and bounded background completion.** Direct and
queued execution use the same stateful engine. Preserve existing request/claim
preparation, authorization and necessary cold loads; do not add a cold-cache
refusal/retry contract. Once prepared, engine transitions validate and mutate
live state, release dependents and select one terminal result without SQL lease
confirmation, outbox reservation/staging or persistence ACK. Actual model/tool,
Python and validated physical file IO remain work subject to existing limits;
model-response time counts against its per-call budget.

Background persistence observes/coalesces live progress, freezes consistent
snapshots, accounts retained memory and uses existing bounded encoding/outbox
owners. Do not serialize/count/copy the whole growing state per mutation or hold
its lock across encoding/IO. Direct returns its selected result and releases
compute independently of persistence; its evidence publication remains bounded
and best effort. VerificationResult remains compact and retained
VerificationDetail(s) may be absent even after direct success.

Queued terminal selection releases execution/workspace/Python/model resources.
Separate bounded background tracking retains finished state, renews its claim,
retries persistence and performs token-fenced queue settlement after canonical
terminal evidence acceptance. A shed terminal offer is not queue completion.
Account for admitted running claims so every one can finish without waiting for
tracking space. Full tracking pauses new claims rather than running computation;
isolated direct capacity remains available. This is live memory custody, not a
new durable result mirror or stronger crash guarantee.

The existing background lease owner renews claims and updates local cancellation;
execution checks local tokens and original deadlines. Preserve cancellation
notification responsiveness. Admission, claim, renewal, cancellation and settlement
use short tenant-scoped group transactions, set-based batches and bounded retry
work. Release connections after commit; never hold them over compute/provider,
Bifrost, file or encoding waits. A low-traffic batch can contain one row; do not
build a transaction/connection per row by construction.

Preserve resource-scaling gateway snapshots, in-memory quota accounting and
background settlement/capture. Remove capture-capacity waiting only for
verification-originated calls, without weaker authorization or changes to
standalone gateway behavior. Server-produced publication files retain validated
object IO and exact descriptors but use background inventory persistence rather
than external client upload-session SQL/inventory ACK on engine publication.
External upload completion contracts remain unchanged. Bifrost implementation,
query semantics, catalog and durable-write/WAL/fence contracts remain unchanged;
Agent access continues through Agent tools, with no new Verifier query surface.

A crash can lose unaccepted progress/results and direct requests do not resume.
Queued recovery retains ExecutionId, exact accepted input/source/policy, accepted
spend and original deadline; it can repeat later work/effects/spend. Unavailable
recovery never means absence. Accepted terminal decisions are reused without
computation. Preserve existing revision identity/conflict/replay behavior and
allowed old-frozen-revision race; queue/source authority controls current
continuation, settlement and provider writes. Do not add synchronous per-step
SQL or invent instantaneous cross-replica revocation.

**REQ-018 — Runtime baseline and schema correction.** Preserve resource-scaling
latency, throughput, server CPU efficiency and memory behavior using matched full
verification benchmarks and existing targets, including direct p99 overhead below
500 microseconds at 64 clients and L2 backlog at most one second of offered work,
drain at most 60 seconds and recovery within 30 seconds. No new 5% regression
allowance is selected. Correct verdicts and healthy-load evidence reconciliation
remain necessary; benchmark validity or exit zero alone is insufficient.

Remove the unreleased generic execution-state/terminal-delivery SQL scaffolding
identified in task 010 and its dependent runtime references. Preserve existing
queue coordination, role grants, tenant isolation, background cancellation
responsiveness and approved GitHub/PR, Task 004 metadata and resource-scaling
schema. No replacement generic execution tables/columns or SQL payload mirror.
This authorizes cleanup of unreleased branch-only migrations, never rewriting
released/applied deployment history or resetting a user database.

## Durability and security invariants

| ID | Must remain true | Failure prevented |
|---|---|---|
| INV-001 | Authenticated tenant and current permission constrain all operations; evidence matches stable repository/source identity while execution lineage retains exact Card/policy versions | Cross-tenant evidence or credentials becoming authority |
| INV-002 | Bifrost owns accepted execution evidence/decisions/checkpoints; existing Postgres rows coordinate claims, activation and delivery, with bounded live terminal custody under REQ-017; Task 004 metadata tables serve only the library snapshot/context protocols | Competing execution truth or a second payload store |
| INV-003 | Positive durable control receipts follow acceptance and survive restart; memory execution/publication success and ordinary staged submission are not durability certificates (REQ-017) | Lost acknowledged evidence, approval or queue work |
| INV-004 | Recovery returns exact accepted state, authoritative absence or explicit unavailability | Replaying settled work or treating unavailable evidence as fresh state |
| INV-005 | Accepted source/policy, publication identity, spent budgets and original deadline survive retry/resumption; unpersisted progress/spend can be lost under REQ-017 | A retry judging different code or silently replenishing authority/spend |
| INV-006 | Local cancellation stops further engine work when observed; current token/generation fences protect continuation/settlement and provider writes; already-issued IO and the existing old-frozen-revision interval remain possible | Late writes reviving an obsolete generation |
| INV-007 | New Postgres tables limited to wyrd.github_installations and Task 004 library snapshot/context metadata; no new database service or generic broker/workflow engine; no secrets in Cards/TOML/results | Unnecessary infrastructure or credential disclosure |

Use existing external upload completion/exact File custody, background Scribe
acknowledgement/batch fences and tenant queue transactions. Server-produced
publication inventory follows REQ-017. There is no Bifrost/Postgres atomic
transaction. [Recovery research](research.md#existing-owner-extensions) identifies
specific extension points and ACK ordering; implementation may choose private
mechanics that preserve these guarantees.

## Acceptance and proof

These are required observations, not claims of tests already run. Each shipped
first-class surface needs the corresponding real SDK→WyrdTestServer/Postgres→SDK
journey. Use unit tests for pure matching/schema rules and integration tests for
actual provider/storage/queue boundaries; mocked provider transport is sufficient
without production GitHub credentials.

| ID | Situation exercised | Expected observation | Requirements |
|---|---|---|---|
| AC-001 | Operator config, install from either entry point, verified repository selection, spoof/replay/foreign tenant, concurrent claims in both directions, cross-flow/stale callbacks, suspension/disconnect | Verified association only; database uniqueness and RLS hold; OIDC login stays unchanged; suspension retains ownership; revocation stops only affected work | 001,016 |
| AC-002 | Minimal TOML with local/exact refs, offline check, unchanged/changed/pinned content, failed/racing activation, rollback, empty list | Ordinary exact Card receipts; correct selected policy; no unrelated import or stale activation | 002,003 |
| AC-003 | 41 PR burst, duplicate/out-of-order events, queued/running A superseded by B, draft/close/access loss | Bounded durable admissions, latest source only, old evidence retained and no obsolete continuation | 004,005,015 |
| AC-004 | Cold/warm pinned extraction, full/incremental and unusable baseline, Prompt plus workspace consumption, context omissions and model limit | Exact source/context preserved and reused; missing/incomplete review never a clean pass | 006,007 |
| AC-005 | Publish before consumer starts/while waiting/while running/no execution/no subscription; identical/conflicting retry; wrong tenant/Card/commit/type | Repository/commit/input matching independent of policy version; existing subscribers receive evidence, later subscribers get no historical replay; retries deduplicate and conflicts refuse | 008,009 |
| AC-006 | Parallel branch plus deferred input/approval, all blocked, authorized/rejected/expired decision, stale occurrence and unauthorized caller | Peers continue, blocked work releases capacity; same execution resumes once under original authority | 010,011 |
| AC-007 | Controlled crash before/after evidence/decision ACK, park settlement, queue commit and recovery reads | Accepted work survives; unavailable state is not absence; no second accepted approval or replay of durably settled predecessors; unpersisted work can repeat under REQ-017 | 009,010,011 |
| AC-008 | No/one/multiple/malformed report; every severity, absent location, valid/invalid suggested edit | Exact report convention; honest verdict/conclusion; no invented source anchor or applied source change | 012,013 |
| AC-009 | GitHub timeout after remote success, annotation batch retry, restart, new head/access removal during delivery | Reconciliation avoids duplicates; provider retry does not rerun verification or claim stale success | 005,014 |
| AC-010 | Hold queued execution/workspace/Python/model admission saturated while calling direct-eligible Verifier | Direct reserved resources still execute; queued-only direct call refuses before work | 015 |
| AC-011 | Fresh reader follows CLI/SDK docs and uses execution, evidence, approval and delivery inspection | Complete journey without UI or hidden manual orchestration; schemas/errors match served operations | 016 |
| AC-012 | Hold Scribe/outbox and background SQL replies while direct/queued graphs advance; fill terminal tracking, then restore persistence; crash/reclaim before/after acceptance and race cancellation/takeover | Engine progresses from memory, compute permits free, tracking bounded with room for running claims, new claims back off, direct remains isolated; restored persistence yields the same terminal evidence then batched fenced settlement; only unpersisted work may repeat | 011,015,017 |
| AC-013 | Matched full immutable resource-scaling/candidate verification benchmarks with healthy evidence reconciliation | Existing latency/L2 targets met and RS throughput/CPU/memory baseline preserved; no invented tolerance or exit-zero-only performance claim | 018 |
| AC-014 | Fresh isolated migration comparison and affected consumer/SQL tests | Only permitted GitHub/PR/Task 004 deltas beyond main plus RS; no generic execution payload schema, preserved grants/RLS/cancellation responsiveness and no Bifrost runtime redesign | 018 |

## Authority reconciliation and approval

The user explicitly approved revision 3 and [contracts](contracts.md) for planning
on 2026-11-10, then approved and requested the dedicated installation storage
amendment as revision 4. The user then directed removal of the Enterprise
Server compatibility decision and use of the conventional GitHub App flow as
revision 5. GitHub.com is the provider for this change. Public shapes are
unchanged. Contracts define those shapes;
[research](research.md) records inspected owners, necessary extensions and
verification limits. The [standard-report example](design/worked-examples/standard-report/README.md)
illustrates the declaration-to-delivery flow; its future contracts have not been
executed against the server.

Specification approval covers the behavior and public contracts in this packet.
It does not claim implementation readiness. Revision 4 selects the installation
table; private column layout and query mechanics remain implementation-owned.
Revision 5 uses GitHub.com’s documented installation and authorization flow.
The user’s direction to simplify this scope authorized revision 5. The user
also directed Task 004 storage (revision 6), agreed the finalized
[nonblocking execution design](design/approved-nonblocking-execution.md), requested
task 010 revision and explicitly approved the runtime spec revision in this
conversation. Revision 7 records that agreement without adding new decisions.

Owning architecture still needs the approved amendments for Repository/Card
roots, closed PR activation, TOML roots, deferred publication/approval, scoped
permissions and Bifrost evidence custody. The corresponding implementation
tasks include those owner updates; approval does not claim they have already
landed. Neither spec approval nor a comprehension pass proves implementation
readiness. Implementation tasks derive from this approved revision.

## Revision history

| Revision | Status | Change |
|---|---|---|
| 1 | draft | Initial backend journey and public contracts from the approved design |
| 2 | draft | User correction: approval uses normal authorization only; remove authentication-age and interactive-proof requirements |
| 3 | approved 2026-11-10 | Apply approved source-based evidence matching; remove publication Card-version overrides, public reactivation rules, extra App management workflow and prescribed permission/layout additions; audit supporting contracts. User explicitly approved this revision and its contracts for planning. |
| 4 | approved 2026-11-10 | User approved and requested the storage amendment: allow only wyrd.github_installations, preserve OIDC trust, reuse flow-separated login state and revise task 002 proof. Public contracts and other task behavior are unchanged. |
| 5 | approved 2026-11-10 | User directed the conventional GitHub App flow and removal of the Enterprise Server compatibility decision. Scope the provider to GitHub.com with its documented installation and OAuth protections. |
| 6 | approved storage direction | Task 004 ObjectStorage over existing object storage and PostgreSQL SnapshotStore/ContextStore with their required metadata-table exception; reconcile the previously stale draft frontmatter. |
| 7 | approved in current conversation | Shared memory execution, preserved preparation/Bifrost, bounded background queued completion and leases/group commit, explicit crash loss, compute-free park handoffs, resource-scaling proof and unreleased execution-schema cleanup. |

Authority: [Wyrd doctrine/Verifier](../../../architecture/wyrd-design.md),
[Bifrost](../../../architecture/bifrost-design.md),
[security](../../../architecture/wyrd-security-posture.md),
[workflow](../../spec-driven-development.md).
Design: [current index](design/notes.md), [declaration](design/approved-declaration.md),
[extraction](design/approved-code-review-support.md),
[publication/approval](design/approved-deferred-inputs.md),
[reports](design/approved-report-delivery.md),
[App/runtime](design/approved-runtime-and-github.md),
[nonblocking execution](design/approved-nonblocking-execution.md).

Revision 7 supersedes conflicting earlier execution ACK/SQL requirements for this
change. Required owner-document updates remain task work before affected runtime
changes; approval does not claim they have landed or that runtime tests passed.

The maintainer reaffirmed revision 7 approval after task 010 was clarified to
reuse the existing ClaimLoop permit through background persistence/settlement
and retain the existing cancellation-notification migration, trigger and listener.
These reuse clarifications preserve this revision's behavior and add no subsystem.
