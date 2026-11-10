# Research and existing owners

Research snapshot: Wyrd f4ca61126; local bohmian-code-review d8c772d. Read-only
source/authority inspection and primary GitHub documentation, not runtime or
throughput proof. Existing uncommitted work was preserved. This file supports
the revision 3 draft; requirements live in [spec.md](spec.md).

## Existing workflow and material gaps

| Owner inspected | Verified current behavior | Required extension |
|---|---|---|
| wyrd-cli/src/card.rs::declare_tree; wyrd-loader::{load,parse,resolve,graph}; wyrd-config::WyrdConfig | Offline YAML graph resolution precedes client registration; TOML is defaults-only | Inferred native Repository TOML root; selected graph only; preserve ordinary YAML loading |
| wyrd-client/src/cards/saga; server components/cards/service.rs; SQL cards/version_resolve.rs | Prepare/submit/upload/complete, exact reference resolution, normal hash/version reuse, Card-owned binding projection | Repository kind/root/binding owner; readiness-gated selected policy; replay-safe activation |
| SQL verification_bindings; card/trigger.rs | Exact owner/subject/Verifier binding and frozen schedule/group activation | Repository owner and closed PR activation; deferred-input matching |
| server verification/submissions.rs::SubmissionSink and AdmissionReply | Scribe ACK precedes queue writer; caller receives committed admission; bounded process-local outboxes are not durable queues | PR-source admission identity and complete fanout receipt; continuation admission for same execution |
| server verification/state.rs::RunState | Full accepted checkpoints; deterministic execution/revision batch fences; accepted-prefix recovery distinguishes missing fence from unavailable row | Pending dependency/approval state, accepted input/decision facts and parked restoration |
| SQL verifier_executions; runner::{VerifierRunner,RunnerClaim} | Queue claims/leases/retry, cancellation and owed Bifrost terminal delivery; result recording separated from computation | Waiting/continuation readiness and provider-delivery coordination on the same table |
| hosted/blocks.rs::HostedBlocks | Direct and queued engines share workspace/Python resources today | Separate direct/queued resource ownership and model admission; same engine |
| spec verification/execution.rs::EvidencePlanRequest; client verify::File | Digest/length planning, shared upload completion and exact File custody | Independent publish acceptance after upload, authorized deferred matching |
| Bifrost tables/verification/execution_inputs.rs | One row per execution, non-null execution_id and verifier_card_uid | Do not fake those IDs for pre-execution evidence; add standalone evidence custody at the existing Bifrost verification owner |
| AgentExecutor::execute/bind and Workspaces::open | Structured output is normal JSON; explicit Prompt variables; JSON workspace mounts | New named code_review schema and report consumer, no separate output store |

Architecture owns the durable contracts and dependency boundaries.
Current RunState uses generic Scribe batch fences. The older migration mentioning
scribe_state_heads is superseded by vala-sql migration
20261108000000_retire_scribe_state_heads.sql. Do not reintroduce that table.

## Existing-owner extensions

These are inspected reuse boundaries and missing behavior, not a selected SQL
schema or proof of implementation feasibility. The specification requires reuse
of existing queue/storage owners and no new PostgreSQL tables. It does not
prescribe columns, indexes, control-row layouts or an additional writer.

### App, repository and declaration control

Conventional App registration, verified tenant installation association,
repository access and revocation are the agreed behavior. Credentials use
existing secret providers; the callback reuses existing authentication and
short-lived authorization-transaction handling. No approval-specific login proof,
manual confirm step, separate disable/claim-management feature or new permission
framework is needed. Source inspection distinguishes App association from
existing Operator credentials; it does not authorize overloading either owner.

Native Repository declaration extends existing graph, Card versioning and
binding owners. Selected policy activation must preserve the prior selection on
failure and reject stale conflicting completion. These are behavioral
requirements; private storage/control layout belongs to implementation, not this
research record. PR-head TOML is not imported at admission.

### Evidence before execution

Existing execution_inputs requires execution and Verifier IDs. Evidence for an
existing subscription without an execution cannot be represented there by
inventing IDs. Bifrost's verification owner must retain accepted evidence and
File custody independently of execution, using existing storage/Scribe behavior.
The exact representation is not selected by this research.

Match authenticated tenant, stable repository identity, commit and input name.
Card and Verifier versions remain pinned on executions; the publisher does not
select policy versions. Existing authorized subscribers can receive evidence;
new subscribers do not automatically receive old evidence. Publication alone is
not activation. Do not add subscription-generation/reactivation API concepts.

Identical content must deduplicate and conflicting replacement must refuse.
Consequential receipts follow durable evidence acceptance. Initial execution and
recovery must discover accepted evidence even if a notification is lost. These
properties need owner-level implementation and crash-boundary proof; process
outbox staging or ordinary analytical query success alone does not prove them.
No active subscriber means an explicit unmatched result, no retained publication
for future subscribers; temporary upload bytes follow existing cleanup.

### Park, approve and resume

A wait is execution state, not an executing sleeping block. The current engine
needs a nonterminal parked outcome after its canonical checkpoint ACK; the queue
row stops being claimable computation and stops renewing a lease. Independent
branches keep running until all are blocked or complete. Record the original
execution deadline once using database coordination time.

Human decisions are accepted control facts for exact approval occurrences at the
existing canonical execution-event owner. Conditional acceptance enforces one
actor/decision/reason, source generation, expiry and current authority without a
second checkpoint writer racing live branches. The execution's sole state owner
consumes accepted facts; restore reads accepted checkpoint plus owed facts. A
receipt does not precede durable decision ACK.

Extend existing queue/SubmissionSink control for continuation readiness of the
same ExecutionId. Each genuine continuation has a stable internal delivery
identity; retries replay that delivery rather than create another execution or
replenish computation attempts. A crash after canonical ACK but before queue
readiness is repaired by the pending queue row's bounded canonical-state check.
Cancellation/supersession marks both running and parked work ineligible before
any later continuation. No new workflow service, input mirror, state-head table
or writer is necessary.

The user rejected fresh human-login proof and the draft's five-minute threshold.
Approval uses normal authentication and authorization. No approval-specific
proof/session extension is required. This supersedes the initial design's recent
interactive-login restriction; update authority if its owner carries that rule.

### Webhook admission and provider delivery

Stage a verified PR delivery's frozen binding inputs as a stable Scribe batch,
then use existing queue admission and return 2xx only when required admissions
commit or a durable refusal/obsolete disposition is established. Preserve the
first accepted policy/source across redelivery, including partial fanout. Stable
internal receipt/execution identities derive from provider delivery and frozen
binding identities; publisher-facing evidence needs no retry key. Existing
SubmissionSink batches/replies need this complete-handoff extension; a volatile
outbox reservation is not ingress acceptance.

Crash before committed admission produces no positive webhook receipt. GitHub
failed-delivery recovery can redeliver; accepted original batch/queue identities
must be read/reused, not reconstructed from a newer selected policy. The existing
App maintenance owner needs bounded failed-delivery recovery with progress in
existing connection control, because GitHub does not redeliver automatically.
Do not add another webhook inbox table. Out-of-order source events are checked
against current provider PR identity outside the HTTP request's computation path
before obsolete work can become current.

Existing terminal DeliveryClaim writes results to Bifrost; it does not publish
GitHub comments. Extend existing queue delivery coordination for that distinct
phase, retaining source/report references, provider IDs, attempts/readiness and
sanitized failure without copying report bodies into SQL. The existing runner
can claim owed delivery without computing another verdict. Persist/reconcile each
consequential remote effect; delayed retries recheck generation and authority.
Ambiguous create/append outcomes require provider readback before retry. If
readback cannot establish what happened, delivery stays visibly unresolved;
exactly-once HTTP side effects are not promised.

### Independent library

Current ReviewPipeline::context accepts GitHubClient, Identity and PR number;
its public facade does not accept a frozen PR/baseline or return the entire diff.
ReviewContext already contains selected source spans with path/blob/lines/text,
ranking metadata, omissions, budget and coverage. Its token budget excludes the
diff/Prompt. The new public pinned/baseline facade must return complete and active
comparisons while reusing selection, snapshot and overlay stages.

Current SnapshotStore/ContextStore require conditional pointer publication,
collection/expiry and tenant-safe identity. The Wyrd adapter must preserve those
existing library contracts using the agreed storage owners. Do not turn these
trait requirements into prescribed SQL columns, Card control metadata layouts or
a second repository indexer. No additional cache-coordination subsystem is
selected. Adapter storage feasibility still needs implementation validation.

Generic App/check/webhook helpers existed at local historical 9fdf6dc. They do
not imply the current detached library checkout includes all helpers or that a
historical dependency pin is approved. Resolve the dependency to the reviewed
published/committed library revision when implementing, preserving independence.

## Primary protocol checks

- [GitHub setup URL](https://docs.github.com/en/apps/creating-github-apps/registering-a-github-app/about-the-setup-url): installation hint is not proof; user access verification and separate callback are required.
- [Checks API](https://docs.github.com/en/rest/checks/runs): app writes, pinned SHA/external ID, 50-annotation request batches and append behavior constrain delivery; app status cannot use Actions-only waiting states.
- [PR reviews](https://docs.github.com/en/rest/pulls/reviews#create-a-review-for-a-pull-request): commit-pinned COMMENT review supports inline review delivery; source commits/merge approval are not required.
- [Failed webhook deliveries](https://docs.github.com/en/webhooks/using-webhooks/handling-failed-webhook-deliveries): no automatic redelivery; scheduled failed-delivery recovery is needed for ingress recovery after outages.

## Verification status

No production changes, SQL migrations, runtime/provider tests or benchmarks were
run for this draft. Static document checks and the independent comprehension
pass are recorded at handoff. Implementation must prove conditional acceptance,
exact recovery, cross-store handoffs and direct/queued isolation; source-level
ownership feasibility does not prove these properties already hold.


The initial independent comprehension pass reviewed revision 1. It identified
ambiguities but its proposed version/reactivation clarifications added policy
that the user subsequently rejected. It is not approval or validation of revision
3. A subsequent bounded read-only pass over revision 3 spec.md and contracts.md
found no material comprehension contradiction or explanation gap. It confirmed
source-based matching, ordinary approval authorization, single-report selection,
park/resume and independent provider delivery are explained without a public
subscription-management or private SQL-layout requirement. This is comprehension
review, not runtime/implementation feasibility proof.

## Complexity audit — revision 3

Audit compared every spec section and public contract with the approved journey,
its design records and the user's subsequent corrections. No production changes.

| Added complexity found | Correction |
|---|---|
| Fresh human proof, credential-type bans and five-minute approval window | Normal authentication and approval authorization only |
| Publisher selects Repository version to reach older review | Match source repository/commit/input; retain policy pins on execution |
| Public subscription occurrences, draining/reactivation and rollback protocol | Existing/new subscriber scenario table; internal tracking stays private |
| Separate manual confirm and disable/claim-management workflow | Conventional App connect/callback/access/disconnect flow |
| Fixed five-minute App connection transaction lifetime | Reuse existing authorization-transaction expiry handling |
| New evidence read/write and integration permission categories asserted as necessary | Reuse authorization owners; only the already agreed approval permission is explicit |
| Specific tenant/Card/binding columns, indexes, cache metadata and table layouts | Remove schema prescriptions; retain existing-owner and no-new-Postgres-table constraints |
| Complete PR diff nested inside active changes, inflating the approved normal Prompt binding | Keep changes active-only; retain full comparison separately in review.full_pr in the same extraction JSON |
| Agreed optional review helpers missing from the required behavior | Restore validation, aggregation/sorting, finding IDs and authored severity helper support without a new engine |
| Ownership suggestions asserted as proven feasible implementations | Distinguish inspected owners, necessary behavior and untested implementation choices |

Retained because they serve explicit agreed requirements: tenant isolation,
authorization, exact source and execution lineage, immutable accepted evidence,
no historical replay to new subscribers, capacity-free suspension, isolated
execution pools, restart-safe acceptance and independent provider retry. These
are observable guarantees, not permission for a new workflow/broker/storage
framework. Runtime recovery and saturation tests remain unexecuted.

### Design alignment checked

| Specification area | Approved basis |
|---|---|
| REQ-001 App | [Runtime/App record](design/approved-runtime-and-github.md); conventional onboarding, no extra management product |
| REQ-002–003 Repository/declaration | [Six declaration rules](design/approved-declaration.md) |
| REQ-004–005 PR admission/supersession | [Runtime](design/approved-runtime-and-github.md) and [baseline rules](design/approved-code-review-support.md#successive-commits-and-pipeline-ownership) |
| REQ-006–007 extraction/consumption | [Extraction contract](design/approved-code-review-support.md#approved-pull_request-extraction-contract) and approved context limits |
| REQ-008–009 evidence | [Deferred input and publication rules](design/approved-deferred-inputs.md), including latest source-based matching correction |
| REQ-010–011 approval/resumption | [Deferred/approval record](design/approved-deferred-inputs.md) and [resource rules](design/approved-runtime-and-github.md#durable-waits-and-resumption); latest authorization-only correction wins |
| REQ-012–014 report/delivery | [Report convention/presentation defaults](design/approved-report-delivery.md) and approved optional helpers |
| REQ-015 isolation | [Separate capacity](design/approved-runtime-and-github.md#direct-and-queued-capacity) |
| REQ-016 surfaces/docs | Repository agent-first/parity standards and the approved complete backend journey; no UI |

This audit restores approved behavior and removes added policy. Draft field and
endpoint spellings make the agreed behavior reviewable; they do not authorize
new workflow stages, SQL designs or security/lifecycle requirements. Approval is
still required for the specification itself.
