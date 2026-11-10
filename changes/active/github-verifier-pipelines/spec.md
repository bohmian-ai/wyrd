---
id: SPEC-github-verifier-pipelines
revision: 3
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
completed work. Results must reach GitHub without making GitHub the execution
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
integration contracts at existing owners. There is no new execution engine,
Postgres table, general event bus, generic wait framework or UI workflow.

## Scope and fixed boundaries

- Rust server owns durable behavior; Rust, Python, TypeScript, CLI and HTTP/MCP
  expose equivalent operations. Schemas/errors come from wyrd-spec. No UI work.
- The independent library owns generic GitHub acquisition, pinned comparisons,
  indexing and context selection. Wyrd owns tenant access, Cards, bindings,
  accepted execution state, storage adapters and GitHub delivery policy.
- Existing Card registry, storage/upload, Bifrost/Scribe, verification queue,
  SubmissionSink, runner, checkpoints and engine remain the owners. No new
  PostgreSQL tables.
- This is the same implementation for SaaS, self-hosted and enterprise cloud.
  Conventional configured GitHub host/App support is retained; no new host or
  installation-management product is introduced.
- No source commits/autofix, merge approval, automatic thread resolution,
  general signal buffer, timer wait action or CI-provider-specific integration.

## Required behavior

### Connect and declare

**REQ-001 — App connection.** The operator configures one App for the deployment
through existing configuration and secret providers. A tenant completes verified
installation association and repository selection. Enforce one connected
installation per tenant and one tenant per installation. Disconnect stops
connection use and affected unfinished work before releasing its association.
GitHub's setup URL is not its OAuth callback, and an installation_id is not
association proof.

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

The last completed usable review of the same PR and policy is the incremental
baseline. Partial/cancelled work never advances it. P completed, A running, B
pushed means cancel A and review P→B. Without usable history, after rewritten
history or changed policy, use full PR review. Keep PR base/merge-base distinct
from comparison baseline. The library computes comparisons/context; Wyrd does
not duplicate selection or indexing.

**REQ-007 — Normal evidence consumption.** Entire extraction output or individual
fields bind through ordinary Agent inputs and workspace mappings. Prompt binding
injects actual selected source text; workspace JSON holds the same accepted
values. No filesystem-discovery step is required, no implicit injection into
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

**REQ-011 — Park and resume.** Acknowledge the canonical pending checkpoint before
retiring its current queue delivery. No runnable/active branches means release
runner capacity, renewable execution lease and invocation-owned workspace,
Python/model resources. Queue continuation restores the same accepted execution
without replaying settled predecessors or replenishing retries/spend/deadline.
Missing evidence and approval expiry are bounded by the original execution
deadline; operator execution ceilings must support intended approval periods.
Cancellation, revocation and supersession also apply while parked.

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

## Durability and security invariants

| ID | Must remain true | Failure prevented |
|---|---|---|
| INV-001 | Authenticated tenant and current permission constrain all operations; evidence matches stable repository/source identity while execution lineage retains exact Card/policy versions | Cross-tenant evidence or credentials becoming authority |
| INV-002 | Bifrost owns evidence/decisions/checkpoints; existing Postgres rows coordinate claims, activation and owed delivery | Competing execution truth or a second payload store |
| INV-003 | A positive consequential receipt follows durable acceptance; every accepted handoff can be rediscovered after restart | Lost acknowledged evidence, approval or queue work |
| INV-004 | Recovery returns exact accepted state, authoritative absence or explicit unavailability | Replaying settled work or treating unavailable evidence as fresh state |
| INV-005 | Existing source/policy, publication identity, budgets and deadline survive retry/resumption | A retry judging different code or silently replenishing authority/spend |
| INV-006 | Stale/cancelled/superseded actors cannot release new work or publish as the current review | Late writes reviving an obsolete generation |
| INV-007 | No new Postgres tables or generic broker/workflow engine; no secrets in Cards/TOML/results | Unnecessary infrastructure or credential disclosure |

Use existing upload completion/exact File custody, Scribe acknowledgement/batch
fences and tenant queue transactions. There is no Bifrost/Postgres atomic
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
| AC-001 | Operator config, install from either entry point, verified repository selection, spoof/replay/foreign tenant, disconnect | Only verified association activates; no tenant keys; revocation stops only affected work | 001,016 |
| AC-002 | Minimal TOML with local/exact refs, offline check, unchanged/changed/pinned content, failed/racing activation, rollback, empty list | Ordinary exact Card receipts; correct selected policy; no unrelated import or stale activation | 002,003 |
| AC-003 | 41 PR burst, duplicate/out-of-order events, queued/running A superseded by B, draft/close/access loss | Bounded durable admissions, latest source only, old evidence retained and no obsolete continuation | 004,005,015 |
| AC-004 | Cold/warm pinned extraction, full/incremental and unusable baseline, Prompt plus workspace consumption, context omissions and model limit | Exact source/context preserved and reused; missing/incomplete review never a clean pass | 006,007 |
| AC-005 | Publish before consumer starts/while waiting/while running/no execution/no subscription; identical/conflicting retry; wrong tenant/Card/commit/type | Repository/commit/input matching independent of policy version; existing subscribers receive evidence, later subscribers get no historical replay; retries deduplicate and conflicts refuse | 008,009 |
| AC-006 | Parallel branch plus deferred input/approval, all blocked, authorized/rejected/expired decision, stale occurrence and unauthorized caller | Peers continue, blocked work releases capacity; same execution resumes once under original authority | 010,011 |
| AC-007 | Controlled crash before/after evidence/decision ACK, park settlement, queue commit and recovery reads | Accepted work survives and progresses; unavailable state is not absence; no second approval or computation replay | 009,010,011 |
| AC-008 | No/one/multiple/malformed report; every severity, absent location, valid/invalid suggested edit | Exact report convention; honest verdict/conclusion; no invented source anchor or applied source change | 012,013 |
| AC-009 | GitHub timeout after remote success, annotation batch retry, restart, new head/access removal during delivery | Reconciliation avoids duplicates; provider retry does not rerun verification or claim stale success | 005,014 |
| AC-010 | Hold queued execution/workspace/Python/model admission saturated while calling direct-eligible Verifier | Direct reserved resources still execute; queued-only direct call refuses before work | 015 |
| AC-011 | Fresh reader follows CLI/SDK docs and uses execution, evidence, approval and delivery inspection | Complete journey without UI or hidden manual orchestration; schemas/errors match served operations | 016 |

## Authority reconciliation and approval

The user explicitly approved revision 3 and [contracts](contracts.md) for planning
on 2026-11-10. Contracts define the public shapes;
[research](research.md) records inspected owners, necessary extensions and
verification limits. The [standard-report example](design/worked-examples/standard-report/README.md)
illustrates the declaration-to-delivery flow; its future contracts have not been
executed against the server.

Specification approval covers the behavior and public contracts in this packet.
It does not grant implementation readiness or select private storage layouts.

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

Authority: [Wyrd doctrine/Verifier](../../../architecture/wyrd-design.md),
[Bifrost](../../../architecture/bifrost-design.md),
[security](../../../architecture/wyrd-security-posture.md),
[workflow](../../spec-driven-development.md).
Design: [current index](design/notes.md), [declaration](design/approved-declaration.md),
[extraction](design/approved-code-review-support.md),
[publication/approval](design/approved-deferred-inputs.md),
[reports](design/approved-report-delivery.md),
[App/runtime](design/approved-runtime-and-github.md).
