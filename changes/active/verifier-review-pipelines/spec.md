---
id: SPEC-verifier-review-pipelines
revision: 24
status: draft
approved_baseline_revision: 18
approved_github_amendment_revision: 24
---

> **Deprecated for future work — 2026-11-09.** The user requested a fresh design of the remaining capabilities. Continue in [GitHub Verifier pipelines design](../github-verifier-pipelines/design/notes.md). This packet is retained as history; its prior approvals and evidence do not approve the replacement.


# Build review pipelines with Verifier primitives

Revision 24 records the user's explicit agreement to the researched GitHub App
convention and instruction to replace TASK-004 on 2026-11-09. This GitHub
amendment is approved: one operator-configured App per deployment, many tenant
installations, GitHub repository selection, verified connection to an existing
Wyrd tenant, and ordinary PR Checks. SaaS and self-hosting use the same code with
different operator configuration. The independent library remains generic.
Unrelated wait/approval policy retains its separately recorded status; this
amendment does not choose it. The rejected TASK-004 is deprecated and its
completion evidence is historical only.

Revision 22 records the user's explicit approval on 2026-11-08 of the five
TASK-003 corrections: retain shared process and per-execution concurrency
controls without a new per-tenant quota; keep server fault injection in Rust;
remove acceptance-time limits reporting; fix cancelled waits in the shared
client; and force the competing cancellation writers in one deterministic
regression. This narrow amendment is approved. Revision 18 and subsequent
approved amendments remain the implementation baseline; unrelated wait/approval
decisions remain draft. This revision does not approve those open decisions.

Revision 21 records the explicitly approved terminal-settlement amendment in
[durability supplement revision 3](execution-state-and-durability.md#approved-terminal-settlement--revision-3).
Every queued terminal path records canonical state, receives the existing
Bifrost WAL/batch-commit ACK, then settles queue delivery. Cancellation stops
computation immediately and preserves durable delivery responsibility in the
existing queue. The active correction handoff is
[TASK-001-R5](review/task-001-r4-0d317ecdf-working/TASK-001-R5-single-terminal-settlement.md):
revert the rejected R4 implementation and rebuild its single terminal workflow.
[TASK-001-R4](review/task-001-r3-d89356985/TASK-001-R4-uniform-terminal-settlement.md)
retains the approved outcomes and remaining acceptance obligations;
implemented TASK-001 and R3 remain historical. This amendment is approved;
unrelated wait/approval decisions keep the overall packet draft.

Revision 20 records the user's approval of durable wait/approval suspension,
recent interactive human login, a separate Verifier approval grant, and
SubmissionSink admitting a new queue work item that resumes the same execution.
The user requested [TASK-007](tasks/TASK-007-durable-waits-and-approval.md).
Its detailed contract is in [wait/approval](wait-and-approval.md); unresolved
material policy remains explicitly draft rather than silently approved.
Revision 18 plus current user corrections remains the approved baseline for
existing work. TASK-001 is implemented; existing foundation remediation owns its
remaining corrections/proofs. TASK-007 is new capability work and does not
block foundation remediation.

## Problem and proposed behavior

A repository owner installs Wyrd just as they install another third-party
GitHub App: select their GitHub account or organization and repositories, connect
their Wyrd account, and receive PR Checks from configured Verifiers. Wyrd's
operator registers and operates the App once for the deployment. Self-hosted
operators do the same for their deployment; ordinary tenant users never create
their own App. Both installation from GitHub and connection started from Wyrd
must reach this same server-owned workflow.

A team must be able to declare one reusable Verifier, execute it against a
pull request, and have Wyrd acquire evidence, build code context, run concurrent
reviewers, consolidate findings, judge requirements, retain custom reports,
and recover interrupted work. The developer supplies Cards, declarations, optional fixed criteria
attachments and bounded transformations. The developer does not supply a scheduler,
checkpoint service, storage-transfer loop or application review worker.

A team can maintain many independently registered Verifiers in a dedicated
repository. Application repositories select exact shared Verifier versions or
their own local definitions in `wyrd.toml`. These are repository verification
bindings: no deployed Service or Agent Card must exist to review a repository.
Service YAML keeps its existing runtime verification bindings; it is not the
configuration owner for repository PR verification.

Today the verification engine supports a fixed preparation DAG, concurrent
independent steps, Agent and code actions, requirements and decisions. It can
pass JSON between steps and mount evidence for Agents. It cannot expand a
runtime-sized collection, commit and restore individual reviewer outputs,
materialize repository evidence, or complete the GitHub lifecycle. Hosted Agents
also lack the write/publication tools needed by these workflows.

This change expands those existing capabilities. **Code review is an example
configuration of a Verifier, never a Card kind, Verifier discriminator, check
kind, separate execution engine or privileged product mode.** The same mapping,
output, tooling and recovery behavior must work for document verification and
other bounded verification pipelines.

Repository contextualization uses the complete public `ReviewPipeline` from
[`bohmian-code-review`](https://github.com/bohmian-ai/bohmian-code-review).
Wyrd provides its intended consumer-owned object and metadata adapters. It
does not replace its incremental index, GitHub reader, ranking, packing,
publication or context readback with Wyrd implementations.

The agreed extraction contract is `kind: extract` with
`extractor: pull_request`. Selecting it automatically executes
`bohmian-code-review`'s public `ReviewPipeline` using the bound PR's server-pinned
evidence. This requires no authored `inputs`, `from`, repository-input mapping,
custom schema, inline code or user-managed pipeline call. See REQ-016–017.

Before: an author can list three reviewer steps, but must orchestrate variable
PR files and retain partial work elsewhere. After: the same Verifier can
derive work from the pinned PR, map its declared reviewers, checkpoint each
result, run a critic over collected outcomes, and expose a custom report.
The same exact Verifier can serve two projects through different bindings and
execution inputs.

## Approach, costs and limits

Extend the current engine and its hosted adapters; retain the existing action
and check owners. Add bounded fanout and sequential repetition to steps,
runtime file/directory/JSON publication and durable checkpoint recovery, with Bifrost
history and ongoing API/UI inspection. Add authenticated repository evidence and an extraction
mode backed by `ReviewPipeline`. Extend the existing hosted tool registry with Agent write/publication tools
and repository evidence reads.

Extend the existing `wyrd.toml` schema and declaration workflow to resolve
repository bindings from trusted Git configuration. Add repository subjects to
the existing verification binding/result contracts without introducing a
Repository Card. Every selected Verifier still executes through the same engine.

Durable acknowledgement adds storage/database work to execution acceptance and
step completion. GitHub App connections require credential rotation, repository
reconciliation and an independently retryable publication lifecycle. These costs are required to make the execution promise real.

Repository subjects extend public and persisted subject contracts. Git-backed
configuration imports require explicit declaration authority and immutable
version handling. Enforced reviews use target-branch configuration. PR changes cannot remove
or weaken their own enforced checks.

Fanout repeats an existing action over a frozen array; sequential repetition
runs another invocation only after its predecessor completes. Both are bounded.
Optional speculative execution starts an ordinary conditional step before its
decision is ready, but withholds its contribution until that decision allows it.
This trades bounded extra spend for latency. Publication-triggered scheduling,
live consumer subscriptions, recursive graphs, unbounded loops,
arbitrary host Python, subprocess execution, repository builds, code fixing,
automatic merge, inline PR comments and annotation delivery are outside this
change. Initial GitHub delivery is a Check Run summary and report links.

There is no claim of exactly-once model execution or guaranteed model finding
quality. A call whose response was not committed before a crash may repeat.
Correctness means the declared pipeline, evidence boundaries and policy run as
specified; it does not mean a reviewer discovers every defect.

## Execution ownership and durability

Revision 18 incorporates the explicitly approved decisions 1–5 in
[execution state and durability, revision 2](execution-state-and-durability.md).
That supplement is the detailed execution-state contract for this change.
It replaces revision 17's PostgreSQL checkpoints, request-owned direct-publication
state and active-to-retained handoff. This consolidation adds no new architecture.

Both `verify` and the submitted/scheduled/group queue path enter the same
engine. On entry the engine attempts authoritative recovery from Bifrost,
restores or initializes one in-memory run state, and executes eligible work
from that state. PostgreSQL owns queue admission, claims, renewal and retry
scheduling. It does not hold a competing recovery copy of invocation results,
accepted publications or final-result selection.

The existing Bifrost shared writer persists complete logical checkpoints at
consequential boundaries and returns Scribe WAL acknowledgements. Bifrost's
existing read and acceptance owners must be extended for authoritative exact
checkpoint loading, current-writer acceptance and rejection that survives WAL
replay. Recovery waits/fails when required state is unavailable; it never
substitutes older or empty state. No extra writer, lease scheduler, recovery
store, remote-publication wait or archive handoff is introduced.

Concurrent steps retain their existing execution concurrency. Checkpoint IO
must not hold an execution's memory lock: independent steps can record progress
while an earlier checkpoint awaits acceptance. Independent changes may share
one complete checkpoint, with each consequential boundary waiting until an
accepted snapshot covers its own change. A checkpoint revision identifies that
snapshot, not each memory mutation. This clarifies revision 18's existing
batching and engine-ownership contract; it changes neither durability barriers
nor the persisted state format.

The existing publication/tool naming, receipts, exact File descriptors,
permissions and reference semantics remain unchanged. Direct requests remain
queue-free and distinct; persistence does not turn disconnect into background
Agent execution. Existing input/result/output tables retain their meanings.
Original deadlines, budgets, settled failures, interrupted attempts and group
comparison behavior survive recovery.

[Implemented Task 001](tasks/TASK-001-bifrost-execution-state.md) supplied this
foundation; [current R4](review/task-001-r3-d89356985/TASK-001-R4-uniform-terminal-settlement.md)
owns all remaining corrections and new foundation proof before dependent
Tasks 002–006. The original revision-16 Task 001 and its remediation/review
remain historical implementation evidence, not proof of this architecture.

## Reading the control flow

The complete proposed control flow is shown in
[the standalone SVG](implementation-diagram.svg), with a text alternative and
branch details in [diagram notes](diagram-notes.md). It includes administration,
manual and webhook admission, library context construction/readback, reviewer
fixed sweeps, conditional/speculative review, durable commit/recovery, publication and
maintenance. The illustration is proposed behavior, not a claim of shipment.

Three self-contained alternatives live in [proposed-examples/](proposed-examples/README.md):

| Example | Behavior |
| --- | --- |
| [Repeated sweeps](proposed-examples/repeated-sweeps/verifier.yaml) | Run one reviewer three times, then consolidate. |
| [Conditional specialists](proposed-examples/conditional-specialists/verifier.yaml) | Run distinct security and concurrency Agents only when selected by a planning Agent. |
| [Speculative review](proposed-examples/speculative-review/verifier.yaml) | Start a third pass after either bootstrap succeeds; retain it only if the later comparison calls for it. |

Each folder contains its own root `wyrd.toml`, `verifier.yaml` and `agents/`
with inline Prompts. Agents normally read attached workspaces, with no injected
Prompt inputs. These are alternative proposed configurations, not one large
pipeline and not currently runnable contracts. [Example orchestration](example-orchestration.md)
defines the controls, [PR extraction](pull-request-extraction.md) defines the
complete evidence and [publication](output-publication.md) defines publication references.

```text
Team publishes subject-less Verifiers; tenant admin authorizes GitHub access
                                  |
PR webhook/manual request -> trusted target wyrd.toml -> exact repository binding
                                  |
Extract pull_request: metadata + complete diff/source + public ReviewPipeline
                                  |
                    Choose a Verifier configuration
           +----------------------+-----------------------+
           |                      |                       |
     Same reviewer x3       General + planning       Review A + Review B
           |                      |                       |
           |              Conditional security/     First success starts C
           |              concurrency Agents        Both finish -> compare
           |                      |                 agree: discard C
           |                      |                 disagree: retain C
           +----------------------+-----------------------+
                                  |
                 Consolidate settled workspace publications
                 (statuses distinguish success, skip and failure)
                                  |
                     Existing Agent requirement + decide
                                  |
                   Durable result/report -> exact-head GitHub check
```

Existing execution/storage owners checkpoint invocations, publications and
speculative decisions. Queued recovery restores committed work and budgets;
late replies cannot reverse cancellation/discard. API/UI inspection can show
pending/discarded work, but downstream Agents receive only settled admitted
content. Default-branch refresh, access reconciliation and library-context expiry
remain server-owned. CodePal-style third-pass speculation is supported; eager
successors on intermediate discoveries and live validation remain excluded.

## Decisions and required behavior

### Reuse and authoring

**REQ-001 — One execution owner.** Invoking a Verifier executes all declared
preparation and judgment through the existing verification engine. SDKs and
GitHub adapters submit and inspect executions; they never schedule review
stages or shuttle intermediate payloads. This prevents a hidden application
runtime requirement.

**REQ-002 — Reusable declarations.** Reuse named inputs, `extract`, `agent` and
`code` actions; assert/trace/agent/code/drift checks; Prompt schemas; exact
Agent/Prompt/Artifact references; requirements; and `decide`. A subject-less
Verifier receives PR evidence automatically from its repository binding when
using `pull_request`. Existing named inputs remain available for other
verification capabilities. Reviewers, concurrency and acceptance criteria belong
to the Verifier and its referenced declarations, not repository TOML inputs.
Fixed policy attachments remain exact-version pinned and cannot be replaced
by execution inputs. New capabilities have the same meaning outside code review.

**REQ-003 — Authoring validation.** Registration validates the DAG, exact references,
known output schemas, conditions, foreach/repeat bounds and tool dependencies.
Reject cycles, unknown fields, impossible known-schema selections and unknown
tool selections with field paths and remediation. Validate runtime arrays atomically
before dispatch. Reuse existing Card/path declaration rules.

### Steps, control flow and concurrent work

**REQ-004 — Bounded fanout and fixed sweeps.** Optional step `foreach` accepts exactly
one of positive integer `count` or `items` (a JSON-array reference). Count creates
that many independent calls to the same action; no injected Prompt input is
required. Items requires positive `max_items`; freeze and commit the complete
ordered array before dispatch and reject non-arrays/excess without starting any
child. Duplicate values are valid independent items. Item mode exposes `item`
and zero-based `item_index`; count exposes only `item_index`. Identity is
execution, step and original index, not a user-generated key. Optional positive
`concurrency` only lowers the effective execution limit; omitting it uses server
admission. Examples omit it. Nested fanout/dynamic graph generation is excluded.

**REQ-005 — Invocation identity and fanout collection.** Each logical Agent
invocation has durable identity `(execution_id, step_id, item_index,
iteration_index)`. Item/iteration indices are null when their construct is
absent; fanout and repetition indices are zero-based. A one-based attempt number
identifies a retry, not another independent review. Commit the logical identities
before dispatch and preserve them across recovery. Each attempt gets a separate
Agent conversation, tool history and private scratch; all three count-based
sweeps can mount the same immutable evidence at the same visible path and write
the same scratch filename without collision. Attribute every returned value,
publication, requirement and budget charge to its invocation/attempt.
A successful fanout output is the array of final values in original index order.
An empty items array succeeds with [] and an empty collection but passes no
requirement by itself. Failed items prevent successful aggregate output, while
all committed partial work and statuses remain inspectible. The consolidator
waits for settlement and receives frozen manifest paths, never mutable sibling
scratch. See [the concrete identity/layout explanation](example-orchestration.md#invocation-identity-and-workspace-layout).

**REQ-006 — Dependencies and conditions.** Preserve `.output` success requirements
and existing list-form `depends_on` as ordering-only. Add mutually exclusive
object form `depends_on: {any_succeeded: [step-ids]}`: a nonempty unique set of
known predecessors, satisfied once any has settled with existing StepStatus
`succeeded`; if all settle without success, do not start and preserve their
failure/intentional-skip cause. Ordinary action inputs/named mounts must still
be ready; any_succeeded is not permission to ignore another required binding.
Status remains readable after settlement. Collection `.publications` waits for
settlement, not success, as REQ-011 defines; a named publication still requires
successful admitted production and actual existence. Keep `when: {path,
operator, expected?}`. Normally resolve it before action binding/expansion;
false intentionally skips, while missing/wrong-type values, invalid comparisons
and failed decisions are evaluation errors, never implicit false. Only REQ-051
permits action work before an unresolved condition. Declaration checks all
possible dependency/condition edges for cycles, including any_succeeded and
speculative decisions. No `.result` or alternate result envelope is added.

**REQ-007 — Requirements and decisions.** Every fanout/repeat invocation runs its
existing step requirements before an iteration can continue or a dependent can
start. Instances are attributed by authored requirement ID, item index and
iteration index; all applicable instances participate in existing `decide`.
Intentional skips are excluded, failed/missing evidence is inconclusive, and no
applicable requirements remains inconclusive. Action success is distinct from a
requirement's verdict. Wyrd imposes no review-completeness or finding schema.

**REQ-008 — Resource and retry accounting.** Server admission bounds concurrency
across executions, tenants, fanout items and Agent tool loops through existing
shared process capacity and per-execution limits; fanout never multiplies a
configured cap. Preserve rotating tenant claim rounds and deferral on full
capacity. This introduces no per-tenant Agent quota, reserved capacity or
cluster-wide concurrency quota. Another tenant can wait while all shared
capacity is occupied and progresses as capacity becomes available. Optional `spec.limits` has positive `concurrency`,
`max_agent_invocations` and `timeout_seconds`; omitted values use configured server
defaults. Concurrency bounds active Agent invocations; the invocation ceiling
counts actions/checks and retries; timeout includes queue/retry waits. Foreach
concurrency defaults to the effective execution cap and may only lower it.
No limits snapshot is added to acceptance or inspection responses. Existing
item, concurrency, deadline, model-call/token and output-byte limits remain
enforced by their owners. Verifier limits only lower deployment limits. Persist cumulative budgets and attempt counts across
recovery; a restart does not replenish them. Deadline includes queue and retry
time. Busy admission defers without spending an attempt. Capacity and provider
retry waits release unrelated permits and honor cancellation. Conditional
fallbacks use explicit steps; retries do not silently change prompts or policy.

**REQ-009 — Existing transformations.** Reuse code actions returning JSON and Prompt
response schemas for structured Agent outputs. Do not add code-action
`response_format` or a second schema facility in this change. Known Agent and
extractor schemas validate references before execution; runtime code values are
checked at their consuming boundary. Authors may use bounded transformations
for routing, grouping and rendering. Code gains no host packages, credentials,
network or subprocess access.

### Intermediate evidence, checkpoints and recovery

**REQ-010 — Final values and retained content.** Keep `steps.<id>.output` for the
existing final text/JSON action value. Add `steps.<id>.publications` for named,
immutable file/directory/JSON publications and `.publications.<name>` for a member
of a plain step or the final successful iteration of a repeat. Named selection
on fanout is refused because several items can use the same name. No `.outputs` alias, `.result` alias, intermediate Card,
fixed report filename or finding type. Writing scratch is not publication. The
host retains content through existing verification File/storage owners with
producer, invocation/attempt, integrity and original-source lineage.
The complete contract is [output-publication.md](output-publication.md).

**REQ-011 — Settled workspace handoffs.** A collection reference
`steps.<id>.publications` waits until the producer and its requirements settle,
then mounts immutable retained work with the recorded step/invocation states.
Success includes committed content; intentional skip includes an empty collection
and reason; failure includes committed partial work and its failure reason.
A discarded/blocked speculative contribution exposes status/disposition but no
content to ordinary consumers. Thus a skipped specialist does not skip the
consolidator, and a failed reviewer cannot look like successful empty work.
The Verifier's own requirements decide whether partial evidence is adequate;
the engine invents no reviewer quorum. Exact `output`/named-publication bindings
retain missing-evidence behavior. Collection manifests expose actual paths and
producer provenance. Metadata grants no file access; generated opinions are not
original-source citations. Agents can read attached workspaces without any
Prompt input bindings. No live subscription or follow/read tool is introduced.

**REQ-012 — Commit before release.** Commit each action output independently,
including fanout siblings and sequential iterations, before releasing its consumers. Persist bytes and
completed inventory before the fenced checkpoint references them. Action and
requirement settlements are individually recoverable; recovery after an action
commit does not rerun that action just because its requirements were unfinished.
Step requirements settle before ordinary dependents run. A storage/checkpoint
failure cannot be ignored. Orphaned uncommitted bytes are not readable evidence
and remain subject to the existing storage recovery/retention owner.

**REQ-013 — Durable admission and execution outcomes.** Queued submission
acknowledges existing pinned input custody and recoverable PostgreSQL admission,
with matching-key replay and conflicting-key refusal. Execution checkpoints and
selected terminal outcomes are durably accepted by Bifrost through the existing
shared writer under the approved state contract. Direct and queued runs share
that execution path. Final success requires selected state and referenced result
records; process-memory staging is insufficient. Preserve cancellation-fence
acknowledgement and non-blocking audit; no active-to-retained transfer or new
cleanup mechanism is introduced.

**REQ-014 — Restore committed work.** Recover an execution on another replica
from its pinned Verifier/input/configuration identity, frozen fanout expansions and iteration positions,
committed outcomes, remaining budget and deadline. Fence checkpoint and final
writes with current execution authority and database-owned expiry; queued work uses its
execution lease. Recover queued work by loading block settlements and explicit control
state, not event replay. Dispatch only unfinished work; do not
re-resolve Card names or branch tips. Persist the pinned declaration snapshot
needed for recovery independently of an LRU cache. Deleting the Verifier or a
referenced Agent, Prompt or Artifact before unfinished work starts ends
the execution errored with the unavailable capability identified safely;
retained checkpoints remain readable under ordinary evidence permissions.
Disabling an execution capability or revoking its authority cancels affected
active work with a typed disabled/revoked reason and prevents recovery from
dispatching it. Pinned snapshots prevent
version substitution; they are not permission to bypass deletion or revocation.
Persist speculative readiness, condition resolution and admission/discard with
the checkpoint; a retry or new worker cannot reverse them or release withheld
content. No cross-execution output cache is introduced.

**REQ-015 — Partial and cancelled execution.** Expose committed progress,
per-item states and retained outputs while running and after cancellation,
timeout or error. Add idempotent authorized cancellation to all execution
surfaces. Cancellation fences subsequent commits and starts no new actions;
in-flight calls are cancelled where possible. It never deletes accepted
checkpoints. A terminal execution does not silently resume; an explicit rerun
creates a new execution. Uncommitted provider work may repeat after an
ambiguous crash, with attempts/spend recorded conservatively.

Direct `verify` stays synchronous and queue-free on the same engine state and
checkpoint path as submitted work. Accepted publications and partial evidence
remain in Bifrost state after failure. A disconnected request does not create
background Agent execution or a new resume API. Only the existing queue poller
automatically retries submitted work; every engine entry still loads its own
execution state. Window/Drift activation and evidence semantics are unchanged.

### Repository evidence and bohmian-code-review

**REQ-016 — Automatic pinned PR evidence.** A verified repository binding supplies
installation, host, repository and PR number automatically. Manual invocation
supplies a repository and PR number through the execution request, not Verifier
inputs; the server resolves and checks the same tenant authority/allowlist.
Missing or unauthorized identity refuses before library/source IO. Pin target
commit, merge base, head, head repository/tree and metadata snapshot. No authored
root PR input, credential, `from`, repository descriptor or implicit repository
fallback. Complete extraction behavior is fixed in
[pull-request-extraction.md](pull-request-extraction.md).

**REQ-017 — Automatic pull-request extraction.** Extend the existing `extract`
action with exactly `extractor: pull_request`. Selecting it automatically runs
the public `ReviewPipeline` from `bohmian-code-review`, plus its extended pinned
PR evidence acquisition; no callback or authored library call is needed. The
previous draft selector is replaced, with no alias. Refuse `from` for this
selector; preserve document extraction and its source contract. Reuse the whole
library's cold build, incremental snapshot/history, overlay, symbol resolution,
ranking, packing, publication and readback workflow. Wyrd implements its intended
object/metadata adapters under existing storage/SQL owners. Do not duplicate
library orchestration or add heavy dependencies to pure contracts/SDKs.

```yaml
- id: extract-pr
  action:
    kind: extract
    extractor: pull_request
```

**REQ-018 — Exact context and budgets.** Extend the library public pipeline facade
for a pinned PR rather than bypassing it with leaf calls. Pin and verify diff,
merge-base/head source and supplemental context identities before committing
one bundle. PR movement returns moved/superseded rather than substituting a
new head; retries are bounded by the accepted identity/deadline. Once committed,
read retained evidence without GitHub. Preserve original ContextId, digest,
format/extraction versions, spans, omissions, `not_analyzed`, budget and index
coverage. Its `context_tokens` limit is context-only; separately admit the full
model request. Never silently truncate diff, changed source or model requests.
Unobtainable complete changed evidence fails extraction; bounded supplemental
omissions remain explicit and do not imply truncated PR evidence.

**REQ-020 — Refresh and cleanup.** Default-branch pushes request bounded,
idempotent `ReviewPipeline::refresh`; PR `context` does not refresh an existing
snapshot. Wyrd schedules context expiration and snapshot collection through
the library's public owners, honors collection grace and nonoverlap, and
quiesces builds before repository offboarding. Persist maintenance work before
acknowledging a relevant delivery. Cache maintenance is server work, not a
developer-maintained cron dependency. It does not produce a verification verdict.

### Hosted Agent tooling

**REQ-021 — Hosted review tools.** Extend the existing hosted built-in catalog
and resolve tools through the existing `ToolResolver`/`ToolRegistry`.
Keep tools as runtime names, with no Tool Card. Preserve current built-in names,
permission identities and records.
Expose retained evidence and committed outputs through the existing mounted
filesystem tools. Reviewers read and search only their mounted immutable evidence;
no additional repository context, path/line or search tools are introduced.
The library's internal context selection remains part of extraction. A missing
file outside the retained bundle is unavailable to the reviewer; filesystem
operations never fetch additional repository source from GitHub. Large results
use existing bounded pages or durable output references, never an unbounded
model message.

**REQ-022 — Removed.** Remote MCP tools are outside this change. This ID is
retired; it must not be implemented or reused for another requirement.

**REQ-023 — Tool authority and provenance.** Every tool invocation requires its
own existing tool permission plus underlying resource/repository permission,
all bounded by the accepted execution authority and current connection state.
Recheck repository/connection disablement before each external operation.
Never forward Wyrd tokens or model-visible credentials to tools. Record tool
identity, schema version, producer item, sanitized inputs/output references,
digests, byte counts and failure categories under sensitive capture policy.
Apply per-call and cumulative limits, SSRF screening/pinned connections,
cancellation and bounded retries. Unknown tools fail before a model call;
denied tools do not leak content. Committed Agent steps are not replayed;
uncommitted read-tool calls may repeat during step recovery.

### Install Wyrd's GitHub App and connect repositories

**REQ-024 — Install the deployment's Wyrd App.** The SaaS operator registers
Wyrd's App once; a self-hosted operator registers their own App on github.com or
GHES and configures the deployment. Use one configured App/host per deployment
for this capability, with one shared webhook URL. Tenant users install that App,
choose their GitHub account/organization and repositories, and connect the
installation to an existing Wyrd tenant. Tenants never register an App or submit
App credentials. App ID, slug, host, private key, webhook secret and OAuth client
credentials belong to the existing server configuration/secret-provider boundary.
Operator rotation keeps installation and execution identities; tenant APIs cannot
rotate, read or remove these deployment credentials.

Support Connect GitHub from Wyrd and installation directly from the App's GitHub
page. A GitHub-first user signs into Wyrd and selects a tenant through existing
onboarding; installation never creates or selects a tenant implicitly. The Rust
server owns both entry points. CLI/SDK/MCP callers receive a browser URL and read
connection status; UI/BFF renders the same workflow rather than owning it.

Use GitHub's documented Setup URL followed by its separate user-authorization
flow. The Setup URL receives an untrusted installation hint, not an OAuth code.
After establishing the Wyrd user/tenant, start correlated GitHub authorization;
validate the returned state and exchange its code at the registered OAuth
callback. Use state and PKCE according to the configured host's supported
protocol. Do not enable GitHub's mutually exclusive automatic OAuth-on-install
setting alongside Setup URL. Both entry points converge on this same flow.
Transactions are bounded, one-use, expiring, and bound to the current browser,
Wyrd administrator, tenant, App/host and selected installation.

Before confirming, verify the installation with the App and the authenticated
GitHub user: the selected installation appears in paginated user-accessible
installations and the selected repositories are accessible through that user's
installation-repository API. Check App identity, required permissions and
suspension. Installation IDs, state and webhooks alone are not ownership proof.
User-accessible installation listing proves access, not organization ownership;
this change adds no organization-owner-only rule. Confirming connects the selected
repositories through the existing tenant allowlist and enables their PR checks.
Additional repository connections use the same verified selection flow; revoking
or disabling remains an ordinary tenant-authorized operation. Do not add a second
persistent verified-repository snapshot, grant model or user-token refresh service.
User tokens are transient setup credentials; background work uses narrowed
installation tokens. GitHub authorization does not create a Wyrd login or grant.

Retain one active installation per tenant and one tenant per installation.
Conflicts refuse without disclosure or silent reassignment. Pending organization
approval remains pending; no PR work starts until verification and confirmation
complete. Repeated completion is idempotent for the same successful transaction.
Disconnect disables only that tenant connection, stops affected active work and
retains evidence; it does not uninstall the shared App or erase operator keys.
GitHub uninstall, suspension and repository removal reduce current authority.
Verify every webhook before routing its installation to a connected tenant.

**REQ-025 — Repository authorization.** Effective access is the intersection of
GitHub installation access, tenant-admin allowlist, verified Wyrd permissions,
repository binding configuration and operation needs. New installations default to
no allowed repositories, including installations granted all repositories.
Use numeric IDs and host-qualified identities; names are display labels and
renames/transfers do not retarget evidence. Reconcile installation removals,
suspension and permission changes. Revocation blocks new acquisition/tool IO
and publication, and cancels affected active work. Existing retained evidence
still requires its normal read permission and is not implicitly deleted by revocation.
Tokens are narrowed to required repositories/permissions and refreshed per
operation rather than persisted in execution state.

**REQ-026 — One authorization plane.** Introduce closed typed integration and
repository permissions/scopes in the existing RBAC model: integration metadata
read, administration write, repository source read, PR execution invoke and
check publication write. Use integrations:read/write with tenant-scoped all, and repositories:read/run/write
with all or exact canonical host/repository_id scope. Read covers source/evidence;
run covers submit/cancel; write covers Check Runs, never source edits. Repository
run additionally requires existing exact-Verifier verifier:run. Scope never
supplies tenancy; integration identity is connection provenance, not repository
identity. Exact wire fields and read/write projections are fixed in
[public contracts](public-contracts.md). Built-in tenant-admin grants cover
management through ordinary permissions, not role-name bypasses. GitHub's
installation grant and the tenant connection allowlist constrain the external
resource; they do not create a second Wyrd policy engine. Each permission
decision audits non-blocking under current rules; checkpoints record lineage.

**REQ-027 — Tenant PR integration controls.** An administrator enables automatic
PR verification for an allowed repository and assigns publishing and execution
Service principals. Existing deployment admission and Verifier/execution budgets
remain the resource-limit owners; this connection adds no per-repository quota
or limits API. Activation applies to PRs targeting any
branch in that repository; there is no separate branch allowlist. A Service principal is
an authentication identity, not a required Service Card. `wyrd.toml` owns the
repository's selected Verifiers; integration
controls constrain permission and activation without maintaining a competing
list of Verifiers. Resolve that configuration into durable repository binding
revisions using the existing verification control owner. Validate exact
Verifier/input compatibility and pin binding/configuration revisions into each
execution. Principals require their own declaration, source, execution, tool
and delivery permissions; installation grants none. Record external GitHub
actors separately from Wyrd SUB/ACT. Ordinary manual invocation remains
available with its normal permissions. No schedule Trigger is required.

**REQ-028 — Durable webhook admission.** Verify HMAC-SHA256 over unchanged raw
body with constant-time comparison, validate host/App/installation/repository
association and persist accepted delivery work before a 2xx acknowledgement.
An unavailable durable store returns a retryable non-2xx. Bound parsing/body
size and acknowledge within GitHub's deadline without executing review inline.
Dedupe by host/App/delivery ID; redelivery resumes failed admission rather than
discarding it. The separate execution key includes repository binding revision,
configuration digest, repository, PR and requested
base/head, so distinct deliveries for one logical
revision do not duplicate work. A deliberately requested rerun gets a new
generation; it is not confused with webhook redelivery.

Automatic PR execution covers opened, reopened, synchronized and
ready-for-review PRs, plus PR edits that change the target branch. Retarget
deliveries resolve the new target before admitting work; other
metadata-only edits do not trigger a review. Drafts wait until ready-for-review.
Closed PRs stop
unfinished work. Check rerequests create a new generation after normal
repository binding authorization. Default-branch pushes refresh context indexes;
installation/repository events reconcile access. Unsupported events are
explicitly ignored after validation. A newer base/head supersedes and cancels
unfinished older work for that repository binding; completed historical evidence
remains exact. Process out-of-order deliveries against current PR identity
before admission, rather than cancelling newer work with an older event.

**REQ-029 — Independent library and generic GitHub protocol.** Extend the outer
`bohmian-code-review` pipeline crate with reusable typed provider operations
needed for user authorization-code exchange and user-accessible installation/
repository discovery, narrowed token requests, pinned
PR evidence and Check Run create/list/update/reconciliation, plus bounded
webhook validation/decoding helpers. Preserve its HTTP transport seam,
identity validation, cancellation and retry dispositions. Keep the generic
App/token/Check/webhook additions at library revision `9fdf6dc77a329f34d96b7ad4f397331d2d18216b`;
do not revert that independent branch. Extend its existing client only for
missing generic protocol operations, then adopt an independently verified
immutable descendant. It contains no JWT signing implementation: its App client
accepts a consumer-signed JWT. Wyrd owns keys/signing, App configuration,
installation/setup/OAuth routes, browser transactions, tenant association,
permissions, durable webhook/Check state, schedules, models and server listeners.
The library remains standalone, with no Wyrd dependency or Wyrd-specific routes,
records or concepts. Generic storage traits/implementations and review helpers
remain library functionality; Wyrd implements its own adapters where needed.
The six symbolic crates remain IO-free. This explicitly
amends the library's current read-only provider/comment-publication boundary;
the library owns provider protocol, Wyrd owns when/why effects occur. Do not
copy provider implementations into Wyrd. Apply the host-qualified format and
cold-build migration defined below before Wyrd adopts the delivered pin.

**REQ-030 — GitHub check lifecycle.** Create/update a Check Run on the reviewed
head with queued/running status and a stable execution association. Terminal
mapping is: passed → success; failed → failure; inconclusive → action_required;
cancelled/superseded → cancelled; timed_out → timed_out; errored → action_required.
Only a fully committed passed judgment can publish success. Each repository
binding has its own stable check name and execution; one passed check never
stands for all configured Verifiers. Configuration/import failures publish a
separate configuration check as `action_required`, never a clean empty suite.
Summaries include
coverage/completeness and authorized report links. Reports stay private;
publication is an explicit authorized export, not permission to publish raw
source or make Wyrd downloads anonymous. Fork PRs use the approved base
installation and exact head identities; never execute PR code or pass secrets
to a fork runner. Inaccessible fork evidence yields an explicit unavailable
outcome, never another source or a clean review.

Publication is durable and independently retryable. A crash after remote
creation must reconcile by stable association before creating another check;
an ambiguous write is not blindly replayed. Later retries use latest allowed
credentials but the original result/head. Out-of-order queued/running updates
cannot regress a terminal check. New-head work creates a distinct check;
old results never claim to judge the current head. A terminal delivery failure
is visible on execution status and does not change its verdict. The initial
release does not append annotations or post PR comments.

### Reports, public surfaces and documentation

**REQ-031 — Custom reports.** Only Agents have the `fs.write` and `publish`
tools. Agents may publish authored JSON, Markdown and arbitrary files/directories
through that tool contract. Standalone `code` actions return their ordinary JSON
`output`; they receive no write/publish tools or direct publication interface.
The engine automatically persists extraction evidence and exposes the existing
`steps.extract-pr.publications.evidence` reference; extractors do not call the
Agent tools. This does not add another authored publication mechanism.
Reports used by requirements are ordinary predecessor evidence.
Separate report content, execution status and judgment; impose no finding
schema, filename or severity policy. A report is required by an ordinary named consumer binding or authored check,
not inferred from Prompt prose. A missing required name prevents that consumer
from receiving evidence and cannot produce GitHub success; the producer may
still have a valid final output. A tool error alone follows normal Agent
error/recovery behavior and is not an invented requirement. Final requirement
Verdict/data uses existing limits. No new post-judgment report action or lifecycle
is added: final judgments are inspectible through existing result APIs alongside
the retained preparation reports.

**REQ-032 — Equivalent public surfaces.** Project new authored contracts and
execution status, paginated step/item inspection, output/report reads,
cancellation, GitHub administration and repository binding management through HTTP,
all three first-class SDKs, CLI and MCP. Reuse the existing Verification and
Cards handles and shared Rust client transport. Read tools remain discoverable;
write tools require explicit scopes plus domain permissions. Schema discovery,
OpenAPI and stable error/remediation payloads are part of acceptance. SDKs
contain no durable execution, policy or storage behavior. The UI is not required to
create, run or administer the capability; REQ-050 separately requires a UI
projection for ongoing inspection using these same APIs.

For a terminal cancelled execution, the shared Rust client's wait operation
raises `WYRD_SDK_409_VERIFICATION_CANCELLED` (status 409), preserving the
execution ID and `cancelled` status in error details. Rust, Python and
TypeScript project that same catalog error; the stored server error code
`cancelled` and status/cancel response shapes remain unchanged. This is distinct
from a caller's wait deadline expiring or an upstream service failing. The
catalog remediation directs callers to inspect retained evidence and submit a
new execution if further work is wanted; waiting never restarts cancelled work.

Server crash, lease expiry and replica takeover are driven by the existing Rust
real-server journeys. Python and TypeScript journeys cover their public submit,
read, publication, cancellation and wait/error contracts, without adding worker,
lease or replica controls to their test harnesses. Rust server tests must force
the cancellation checkpoint writer and terminal-delivery writer to contend and
prove that cancellation remains retained, pending terminal delivery completes,
and unrelated work continues. A Scribe batch-conflict unit test alone is not
proof of this workflow.

**REQ-033 — Teach developers to build reviews.** The planning handoff must include
a dedicated, outcome-complete docsite deliverable in the inspection and
developer-education task, dependent on shipped contracts and journey-tested
examples. This follows the explicitly requested six-area task decomposition.
This is required delivery, not optional cleanup:

- Teach operator App registration separately from team installation, verified tenant connection, repository selection,
  trusted target configuration, exact shared Verifier adoption and local YAML.
  Show one team-definition repository reused by two application repositories,
  without Service Cards; cover publication authority.
- Progress from one `pull_request` extraction and workspace-based reviewer to
  all three alternative examples: fixed repeated sweeps, distinct conditional
  specialists and speculative third-pass review. Also teach array fanout and
  optional sequential refinement. Keep Prompts inline; explain ready-step
  scheduling, effective server concurrency limits, settled collection joins,
  speculative retention/discard and how to customize without a scheduler.
- Explain the actual `pr.json`, `diff.patch`, `changes.json`, changed before/after
  source and unmodified `context.json` bundle. Show one mount and real file reads;
  distinguish authored PR description from an optional generated summary.
- Explain existing `output` versus new `publications`, arbitrary file/directory/
  JSON publication, logical names versus filenames, manifests, frozen handoffs,
  partial evidence, retry/restart/cancellation and exhausted loops. Do not teach
  removed release/cancel expressions or live subscription syntax.
- Provide procedural guides, concepts, complete field/error/permission reference,
  troubleshooting and safe API/UI/Bifrost debugging under `docs/AGENTS.md`.
  Explain inventory visibility lag and GitHub delivery retry.
- Provide equivalent journey-tested Rust, Python, TypeScript and CLI/MCP flows;
  embed tested files through existing docs components. Include the accessible
  non-Mermaid diagram and verify rendered links/accessibility.
- Update library README/API documentation for complete pinned PR acquisition
  and GitHub operations without claiming ownership of Wyrd tenant policy or
  verification scheduling.

### Independent Verifiers and repository bindings

**REQ-034 — Team-owned Verifier publication.** A definition repository may
contain any number of independently registered Verifiers and their dependency
Cards. Use the existing composite declaration and artifact publication workflow;
publication alone creates no subject binding or PR activation. Application
repositories reuse exact registered versions within the same tenant without
copying definitions, importing the definition repository during review, or
requiring its GitHub access. Publishing a new shared version does not retarget
existing application bindings. Adoption changes the application's trusted
configuration. No `latest`, version ranges, cross-tenant imports, new package
registry or Verifier ownership by a Service is introduced.

**REQ-035 — Root repository configuration.** Expand the existing strict
`wyrd.toml` schema with `[[verification.pull_requests]]` entries whose `verifier`
array selects exact Verifier CardRefs and/or repository-local YAML paths.
It selects which Verifiers run; it does not define reviewer counts, concurrency,
acceptance policy, input schemas, credentials or PR-input mappings.
The section activates its selections on PRs under administrator-approved integration
controls; it does not carry a Service subject or require a schedule. This is
the repository binding's pull-request activation, not a new check type or a
`trigger_on` field on Verifiers/Services. Preserve current metadata defaults
and unknown-key errors. Older config parsers require upgrade; they do not
silently ignore this table.

The approved minimal configuration selects the local Verifier:

```toml
[[verification.pull_requests]]
verifier = ["./verifier.yaml"]
```

Wyrd derives a stable selection/check key from the resolved Verifier kind/name
within its repository target branch; the exact version belongs to its binding
revision. Repeated selections of the same Verifier family, including conflicting
versions, refuse configuration before dispatch. Authors need no extra ID.
Check names identify the target branch and selected Verifier.
The GitHub integration resolves physical repository identity and automatically
supplies the pinned PR context to each selected Verifier.

**REQ-036 — Trusted configuration revision.** For enforced PR review, read the
root `wyrd.toml` from the exact pinned target-branch/base revision, not PR head,
merge-result revision or current default-branch tip. Resolve every local Card,
Prompt and policy path from that same trusted tree. Record the configuration
blob digest, target revision, resolved Card identities and dependency digests.
PR changes to configuration or definitions do not govern that PR's enforced
execution; after merge they govern subsequent PRs targeting that updated
branch. Each PR uses its actual target branch’s pinned configuration.
Resolve once per delivery generation and restore that resolution during retry.

For Git ingestion, the root file is literal `/wyrd.toml` within the repository;
do not ancestor-walk outside the repository, use a server CWD/home config, or
inherit a definition repository's config. Local CLI discovery keeps its current
bounded behavior. Shared schema/declaration semantics apply to both; the
server adapter parses Git-provided bytes rather than arbitrary host files.

**REQ-037 — Repository subject contract.** Registering a Verifier remains
subject-less. Extend the existing binding/execution/result subject contract to
support a repository alongside existing Card-backed subjects. A repository
binding identifies verified tenant, integration, canonical GitHub host, numeric
repository ID, target branch ref and stable check ID. Its revision
pins exact Verifier identity,
configuration origin/digest and activation. The execution's
repository subject additionally identifies PR number, target ref and exact base/head;
context/tree IDs remain evidence provenance;
binding identity never depends on a mutable owner/name label or deployed
application version. Expose this subject and binding provenance in inspection,
results, reports and retained evidence. Preserve existing Card-backed subject
payload shapes; add a distinguishable repository alternative to the typed
subject union, not a fabricated CardRef/UID or repository named as a Service.
Use the exact request/result/binding arms and ID serialization in
[public contracts](public-contracts.md): card_uid or repository in requests,
existing raw CardRef or repository wrapper in results. Wyrd pins revisions;
new public GitHub IDs are decimal strings.

Repository executions use repository permissions plus normal Verifier/tool
permissions; Card emit scopes never grant repository access. No application
Run or observation-target Card is fabricated. Existing Card lineage continues
to use real CardRefs, with repository revision recorded as typed execution
provenance rather than a fake Card relationship. A Service may retain source
provenance independently, but its runtime binding neither activates nor
substitutes for repository review. Old clients retain Card-subject behavior;
repository-subject operations require the upgraded contract.

**REQ-038 — Declare local definitions before execution.** Resolve a local
Verifier path as a full Card envelope and reuse existing transitive declaration,
reference-slot validation and heavy-artifact completion. Add a bounded Git-tree
byte source to that shared workflow; do not create another YAML loader or pass
authored paths into the engine. Resolve paths relative to the containing file,
within the selected repository tree; reject absolute paths, traversal outside
the tree, symlinks, submodules, arbitrary remote imports and over-limit graphs.
All imported Card envelopes must declare exact versions. Changed content under
an existing version refuses with the ordinary immutable-version conflict;
do not rewrite names, generate hidden versions or replace another Card.

Import is an authorized declarative publishing operation before verification
admission, never a preparation action that declares Cards. The configured
publishing principal requires normal Card/artifact write permissions and
source access; the execution principal's rights remain separate and cannot be
expanded by TOML or imported definitions. Registry/import writes retain that
publisher's attribution, not SYSTEM. Incomplete declarations/uploads block
affected execution admission and reconcile through existing publication owners.
Freeze the resolved exact references before binding commit. Subsequent engine
execution is identical for registry-selected and locally authored Verifiers.

**REQ-039 — Removed.** PR-head configuration preview is outside this change.
This ID is retired; no preview operation, candidate publication flow or separate
preview execution/check identity is required.

**REQ-040 — Binding reconciliation and failure visibility.** Once automatic PR
verification is enabled, missing, malformed or empty trusted PR configuration,
unknown refs, failed imports and incompatible inputs produce an inspectable
configuration failure and an `action_required` configuration check. They never
silently fall back to another branch or a stale binding set. Resolve and
validate the complete selection before admitting any selected execution; no
partially resolved suite can claim success. Independently published Cards may
remain after a later binding validation failure, but they activate no work.

Persist successful binding revisions and selection provenance before executing
their independent checks. For unchanged stable ID, configuration blob and
dependency digests, and resolved Verifier/activation, a later unrelated
target commit reuses the binding revision while execution provenance records
the new trusted commit. Changed configuration/dependency content or effective
selection creates a new binding revision;
removed IDs retire their old bindings for new submissions on that target branch
only. Reconciliation is scoped by repository and canonical target branch ref;
two target branches can select different versions under the same check ID
without revising or retiring each other's bindings. Branch deletion retires
that branch's bindings; a renamed target ref resolves its own pinned
configuration through the same branch-scoped reconciliation. Accepted executions
retain their frozen binding/configuration. A newer PR generation supersedes
unfinished older executions across that PR's affected binding revisions; a
replayed older configuration cannot retire newer bindings. Retargeting a PR
resolves the new target's configuration and supersedes unfinished
work for the old target; out-of-order deliveries cannot undo that retarget.
A disabled integration
or explicit authority revocation still cancels work as specified above.

No single aggregate pass is inferred from zero checks or one successful member.
Each selected Verifier yields its own execution, report and Check Run. Removing
a check via trusted configuration retires it without publishing a replacement
success; repository branch-protection policy remains the repository owner's
responsibility. A Service YAML, shared definition publication or metadata-only
`wyrd.toml` does not implicitly enable PR verification.

### Complete reviewer evidence and the canonical authoring example

**REQ-041 — One complete extraction bundle.** Successful extraction returns the
fixed PR metadata JSON as `output`, also retained as `pr.json`, plus one directory
`publications.evidence`: `pr.json`, complete `diff.patch`, `changes.json`,
`changed/before/`, `changed/after/`, and unmodified `context.json`. Define metadata,
file entries, binary/add/delete/rename behavior and integrity rules in
[pull-request-extraction.md](pull-request-extraction.md). The diff is part of
extraction's retained evidence, not hidden prompt injection or ReviewContext.
A generated summary is a separate authored Agent action, not provider metadata.

**REQ-042 — Retain once and share.** One extraction invokes ReviewPipeline once
logically and commits its exact evidence bundle. Every reviewer/validator/judge
mounts that same bundle. An ambiguous uncommitted acquisition may repeat; siblings
and recovery do not rebuild it. Preserve the library ContextId/readback contract,
but do not treat random publication IDs as deterministic cache keys or introduce
cross-execution output caching.

**REQ-043 — Separate completeness from context selection.** Preserve the library's
index coverage and omission lists without interpreting them as successful code
review. Complete changed-file acquisition is required for extraction success;
model review can still be incomplete and must say so. Request limits account
for diff/source, context, prompts, tools/conversation and response allowance.
No engine field named coverage, finding schema or reviewer acceptance policy is
introduced by this contract.

**REQ-044 — Three readable Verifier examples.** Replace the old single example with
three self-contained folders under `proposed-examples/`: repeated-sweeps,
conditional-specialists and speculative-review. Each has one root `wyrd.toml`,
one `verifier.yaml` and organized `agents/` with inline Prompts. They demonstrate
fixed same-reviewer calls, distinct conditionally selected Agents, and bounded
third-pass speculation respectively, followed by workspace-based consolidation
and one final requirement requiring a named report. Do not inject review text
through Prompt placeholders when attached files suffice; omit authored
concurrency settings and use effective server limits. Existing Prompt schemas
express only small author-defined routing/comparison values. No inline code,
custom named platform schema, root PR/policy input or extra config filename.
Explain each field, backend behavior and failure in the linked supplements.
These are proposed authoring targets, not current runnable APIs.

**REQ-045 — Bounded sequential repetition.** Optional `repeat` requires positive
`max_iterations` and `until` in the existing comparison shape. `iteration.output`
is the just-completed value available only to until; optional action bindings
may use `iteration.previous` (initially null) and zero-based `iteration.index`.
Invocations and their step requirements settle sequentially before the next;
true ends successfully, false continues within the bound. Invalid/missing until
data or invocation failure stops unsuccessfully; exhausted unfinished work never
succeeds. Successful step output is the final value; prior iteration values and
publications remain inspectible. With foreach, repeat independently per item
and collect final values in original order. Retries are attempts of an iteration,
not extra iterations. Freeze other inputs. Speculation under REQ-051 applies to
the whole bounded step and its publications, not independently admitted children.
No publication-triggered successors, DAG self-cycles or live consumer subscription.

**REQ-046 — Complete workspace snapshots.** The publication source may name a
file or a directory chosen at runtime. Freeze a complete consistent snapshot,
preserving nested files and empty directories, before releasing writers and
uploading. Reuse workspace path confinement/writer exclusion and File storage.
Strictly fail unreadable, unsupported, unsafe or over-limit trees; never publish
a truncated best-effort listing. Preserve safe UTF-8 names including spaces and
Unicode under the portable collision/path rules in
[output-publication.md](output-publication.md). A failed snapshot is not partially
visible. A later workspace edit cannot alter an accepted snapshot.

**REQ-047 — Recoverable publication acknowledgements.** `publish` accepts a logical
`name` and exactly one of workspace `path` or JSON `value`. Acknowledge only
completed immutable bytes plus a fenced durable manifest/checkpoint and
replayable Bifrost projection intent. Same-call retries return the same receipt;
a different call reusing a committed name in the same invocation attempt refuses.
The uniqueness scope includes execution, step, item, iteration, attempt and name.
A fresh Agent retry can publish the same name in its separate attempt; earlier
attempt content stays separately attributed, never merged into that retry.
Replaying the original tool call retains its original attempt identity. Iterations
and fanout items also have separate name scopes. Cancellation/stale leases prevent
later commits; previously accepted content remains inspectible. Receipt/read/
manifest details and strict snapshot rules are fixed in
[output-publication.md](output-publication.md).

**REQ-048 — Durable queryable history.** Extend the canonical Bifrost verification
schemas with `bifrost.verification.execution_events` for ongoing step/invocation
and publication history, including bounded JSON Variant values. Preserve existing
completed result/output table semantics. Store immutable file/tree bytes through
File storage and authoritative coordination/manifests through the execution
owner. Project stable event/sequence/producer/subject identities and content
references through the existing Scribe capture/outbox, Oracle and Forge owners;
never add direct Iceberg writes or a second ingest/outbox service. Persist
projection intent and retry identity with the checkpoint; expose pending/failed
projection position. Register/version the new built-in schema and extend the
restricted peer capture allowlist and architecture authority. Analytical lag
must not erase live execution metadata or be represented as deleted content.

**REQ-049 — Live and historical inspection across surfaces.** Extend existing
execution HTTP, Rust/Python/TypeScript SDK, CLI and MCP operations with bounded
progress/output pagination, immutable manifest reads and authorized content
read/download, usable while work is running and after failure/cancellation.
Use ordinary Bifrost query APIs for historical searches and authored JSON-field
queries. Active publication reads use the exact verified File descriptor committed with
acceptance and can reuse it in the running ledger; retained publication reads use that
descriptor from the Bifrost manifest record. Neither rediscovers accepted publication
content through inventory by digest. Ordinary input-File discovery and actual
storage/Bifrost unavailability keep their existing typed bounded failures; they are
never false absence. Add verifier:read scoped to the
execution’s exact Verifier UID or all for dedicated status/progress/publication/
download inspection, including discarded work; repository reads also need
repositories:read. Keep verifier:run for execute/enqueue/cancel, with repository
run when applicable. cards:read applies to explicit Card fetches, not internal
engine loading or execution inspection. Dedicated endpoints use fixed scoped
server reads after feature authorization; arbitrary Bifrost queries retain
caller table permissions. No inspector role or cards:read fallback.
[Public contracts](public-contracts.md) fixes the exact execution-inspection
methods/paths, producer selectors, response envelopes, scope and errors. Both
existing execute/enqueue endpoints accept the same repository target arm.
GitHub administration and repository activation/binding/check operations share
their typed HTTP/client/SDK contracts.

**REQ-050 — Generic execution UI inspection.** Deliver an ongoing execution timeline,
concurrent fanout and sequential iteration/attempt status, publications/trees,
pending/retained/discarded/blocked speculative disposition,
source provenance, cancellation/error/exhaustion reasons and projection state.
Authorized users inspect JSON, safely preview Markdown/text and download files.
Partial publications remain visible after failure/cancellation, grouped by their
invocation; previous immutable content cannot be overwritten. UI uses the same
public APIs and owns no scheduling/storage logic. Include rendered accessibility
and active-workflow journeys; headless operation retains the whole capability.

**REQ-051 — Optional speculative conditional steps.** A step with
`speculative: true` must declare `when`. Once its non-condition prerequisites
are ready, it may execute while that condition is unresolved. If the condition
is already true, run normally; already false, skip without calling the action.
A later true condition admits retained work; false cancels unfinished calls and
fences their later commits, recording the step as skipped/discarded. A condition
that cannot be evaluated records failure/blocked and admits no contribution.
An action that finishes before the condition stays pending for downstream use;
its final output/publications are withheld until that decision settles. Only
condition dependencies are deferred: action inputs/mounts cannot read unavailable
values or bypass normal authority. No downstream invocation starts before both
action settlement and admission decision. Completed speculative content remains
immutable and inspectible with its disposition under normal execution evidence
permissions; ordinary collections exclude discarded/blocked bytes. If false,
a discarded action failure does not become mandatory failed review evidence;
if true and action failed, retain its explicit failure/partial-evidence state.
Apply to all bounded foreach/repeat invocations in the step. Persist the decision,
spent budgets and cancellation fence so commit/decision races, retry and recovery
cannot leak or resurrect excluded work. No arbitrary speculative descendants,
new event scheduler or separate release/cancel expressions are introduced.

### Execution lifecycle requirements

Detailed data flow and approved decisions are in
[execution state and durability](execution-state-and-durability.md).

**REQ-052 — Engine-owned recoverable state.** One engine/walker/ledger owns
complete logical run state. Reconstruct it from versioned Bifrost checkpoints,
including pinned inputs, complete settlements, decisions, budgets, publications
and selected outcome. Restore settled work and conserve interrupted attempts;
no engine serialization, instruction/conversation replay or exactly-once promise.

**REQ-053 — Shared execution boundary.** Direct verify remains queue-free;
submit, schedule and group activation use existing PostgreSQL queue owners.
Both enter the same load/restore/execute/checkpoint flow. Fresh direct calls
have distinct identities; no concurrent direct-resume API or background
resumption is added. Existing admission input custody is not a run checkpoint.

**REQ-054 — Single durable execution source.** Canonical Bifrost verification
data owns recoverable progress, accepted publications and exact descriptors,
selected result/full sanitized error, lineage and inspection metadata for every
outcome. Preserve existing result table meanings. Use memory during execution;
no PostgreSQL payload mirror or result selection by timestamp.

**REQ-055 — Durable boundaries and stable delivery.** Use reserved existing
shared-writer capacity and WAL acknowledgement before external dispatch,
dependent release, publication success and terminal success. Preserve stable
write/result identities through partial delivery and ambiguous replies; no
object-publication/Iceberg wait or separate finalization service. Direct verify
returns its final input, output and result without waiting for their
acknowledgement; progress and publication boundaries and queued delivery keep
theirs. Every queued terminal path, including errors, timeouts, exhaustion,
cancellation/revocation and admission/pre-engine refusal, records its canonical
terminal state and receives the existing WAL/batch-commit ACK before its queue
work is fully settled. Immediate cancellation removes computation eligibility
while retaining durable pending terminal delivery in the existing queue; the
approved narrow coordination, retry and identity contract is in supplement3.

**REQ-056 — Authoritative reads and public continuity.** Recovery returns the
exact accepted compatible state, authoritative absence, or explicit
unavailability/incompleteness. Never infer absence from an incomplete ordinary
query. Existing execution reads preserve permissions, selectors, page cuts,
content, full errors and recorded identities independently of obsolete
PostgreSQL payload rows or source Card deletion. No public backend selector.

**REQ-057 — One accepted execution writer.** Bifrost acceptance and takeover
have authoritative ordering. A replacement shuts out the previous writer
before loading/continuing; late or rejected progress cannot become recoverable,
including after WAL replay. Preserve expiry/cancellation/deadline protection.
PostgreSQL remains the queued-work lease owner; no second lease scheduler. Ordering is each revision's Scribe batch fence: a revision's batch identity is derived from the execution and revision, so the first accepted write of a revision wins and any other write of it is refused. Each queued revision confirms the PostgreSQL lease before it freezes. No PostgreSQL execution-head, ownership or state table exists; `vala.scribe_state_heads` is retired. One rare interval is accepted: a revision the previous writer froze under a confirmed lease just before takeover may still be accepted ahead of the replacement's revision, which then refuses as stale, and the next claim inherits it.

**REQ-058 — Idempotent recovery and replay.** Queue redelivery uses the original
execution identity; terminal state returns its recorded outcome. Preserve
request-scope/key/digest conflict/replay, attempt-scoped publication receipt
identity and stable cursor membership under existing lifetimes. Unavailable
lookup never permits duplicate work. Explicit new execution remains distinct.

**REQ-059 — Existing consumers and control lifetimes.** Adapt group dedup and
previous-verdict comparisons, binding summaries, inspection and Operator/Check
delivery to the single execution-state source without changing behavior.
Schedule/group activation and independent control/library/delivery lifetimes
remain owned as before. No new tombstones, whole-group serialization, archival
handoff, evidence TTL or cleanup mechanism is authorized as a shortcut.

### Wait, approval and resumed admission — revision 20

The [wait/approval supplement](wait-and-approval.md) defines the proposed
agreed capability intent, inherited boundaries and open material decisions for
these requirements. SubmissionSink ownership and queue-delivery identity are selected; remaining
entry-mode, approval-outcome and public-contract decisions are explicit below. The new scope would
narrow the baseline no-public-resume exclusion only after explicit approval;
it adds no new Trigger/Operator kind or general workflow service.

**REQ-060 — Durable timer waits.** A Verifier can declare a timer wait
whose absolute wake time survives recovery without extending the original
execution deadline. Supported entry modes remain a material decision.

**REQ-061 — Action-driven waits.** A Verifier can suspend a logical
step until an authorized command identifies its exact pending wait instance.
Unrelated ready steps continue; parked work retains no worker lease, execution
permit, provider connection or sleeping future solely for that wait.

**REQ-062 — Human approval.** A Verifier can require human approval with recent
interactive login and a separate approval grant scoped to its exact Verifier.
API keys and delegated automation cannot satisfy human presence. Rejection and
client/public-contract decisions remain explicit until selected; no self-approval
ban is inferred from the grant.

**REQ-063 — Stable decision resolution.** Matching command retries return the
recorded result; continuation cannot target a different logical wait or revive
terminal work. Conflict resolution must be specified under the existing
controls and accepted rare old-worker limitation, without a new SQL fence.

**REQ-064 — Recoverable suspension and wake-up.** Canonical Bifrost records
retain wait/decision state. The server passes acknowledged continuation work
to SubmissionSink, which commits resume admission through the existing queue.
Bifrost never writes PostgreSQL or schedules workers.
Crashes between accepted transitions and queue handoff cannot strand accepted
decisions or require repeating settled computation or human approval.

**REQ-065 — Unchanged execution bounds and authority.** Waiting/resumption never
refreshes original deadlines, budgets, accepted execution authority or pinned
repository evidence. Expiry, revocation, cancellation and supersession retain
their existing force.

**REQ-066 — Shared control surfaces.** HTTP, shared client, Rust/Python/TypeScript
SDKs, CLI and MCP expose equivalent inspection/control and a server-owned means
of obtaining the required interactive human proof. Exact routes and login flow
remain undecided; no optional UI or automation token becomes approval authority.

**REQ-067 — Inspectable waiting.** Inspection distinguishes waiting steps and
nonterminal suspended executions. Code-review delivery cannot report terminal
success while required approval is pending. Exact status projections and
examples follow the chosen contract for approval, expiry and restart.

**REQ-068 — New delivery, same execution.** SubmissionSink owns initial and
resume admission. An approval/action or due timer causes a new queue work item
referencing the original ExecutionId. Delivery retries deduplicate independently
of initial execution admission; restore retains settled work, authority,
budgets, original deadline and result identity. Queue coordination is outside
the engine's execution loop, and active lease renewal remains a separate owner.

## Security and compatibility invariants

| ID | Invariant and failure prevented |
| --- | --- |
| INV-001 | One Verifier/check vocabulary and one engine across deployment modes; no code-review discriminator, Workflow Card or separate orchestration service. |
| INV-002 | Tenant identity derives from verified authority; all control rows, context keys, outputs, caches and delivery state are tenant-qualified and object references cannot cross tenants. |
| INV-003 | Accepted authority never expands during recovery, tool resolution or integration refresh; revocation can reduce it. SYSTEM performs work but grants no extra permission. |
| INV-004 | A published output is immutable, integrity checked and linked only by a current fenced commit; consumers never read partial writes or mutable sibling scratch. |
| INV-005 | Original evidence, reviewer versions, tool schemas, fanout/iteration identity and requirement instances remain attributable through collection and critic stages. A retry cannot count as another independent supporting reviewer. |
| INV-006 | Source, diffs and tool output are untrusted data; no repository instructions can grant tools, replace fixed policy, select credentials or cause host code execution. |
| INV-007 | Preparation reads and computes; only the existing delivery/effect boundary mutates GitHub. Agent publication retains execution-owned work and grants no external write authority. |
| INV-009 | Audit remains non-blocking and uses the existing Scribe outbox. Stronger execution durability must not strengthen audit acknowledgement or create parallel ingestion. |
| INV-010 | Identical capability on macOS arm64/Linux x86_64/Linux arm64 and shared SaaS/self-hosted code; no platform-specific sandbox mechanisms or private enterprise execution path. |
| INV-011 | Existing unmapped Verifiers and current document extraction retain behavior; new fields are optional and old persisted records decode without fabricated checkpoints. |
| INV-012 | Verifiers register independently; a repository binding and revision subject never require a Service/Agent Card, fake CardRef or extra Card kind. |
| INV-013 | Enforced configuration comes from the pinned PR target tree; PR-head changes cannot remove, satisfy or replace enforced checks or publish with automatic importer credentials. |
| INV-014 | A complete reviewer input links changed evidence and supplemental spans from the same pinned revisions; neither an opaque context ID nor full index coverage can stand in for that complete input. |
| INV-015 | Publication acknowledgement preserves immutable content, exact completed File association, manifest and accepted run state. Accepted content is read from that association; analytical inventory rediscovery cannot consume a model attempt or become false deletion. |
| INV-016 | Speculative work is excluded until its condition admits it; false/error, stale workers and recovery can never release discarded/blocked content to ordinary consumers. Authorized debugging is distinct from downstream evidence. |
| INV-017 | One engine-owned in-memory state and Bifrost recovery source serve every execution path; PostgreSQL queue coordination is not a second execution-state store. |
| INV-018 | Recovery loads the exact accepted checkpoint or waits/fails; current-writer acceptance and WAL replay never resurrect rejected progress or lose accepted File associations. A revision frozen under a confirmed lease just before takeover may be accepted ahead of the replacement and is then inherited (REQ-057). |
| INV-027 | Queued terminal delivery cannot retire before Bifrost ACK; stopping computation immediately does not retire pending delivery. A crash or unavailable source preserves delivery eligibility without authorizing more computation. |
| INV-019 | Direct verification remains queue-free but uses the common state lifecycle; disconnect never silently becomes background Agent execution. |
| INV-020 | Proposed waits release their permits and fully parked executions release renewable worker leases; ordinary ready peers remain concurrent. Durable records and bounded coordinator work still consume resources. |
| INV-021 | Proposed approval requires current exact authority and recent interactive human login; the approver's identity never replaces the execution's accepted authority. |
| INV-022 | Continuation is bound to one tenant, execution and logical wait occurrence; it cannot approve a later PR head. Exact conflicting-command semantics must be selected without silently strengthening the accepted old-worker limitation. |
| INV-023 | Proposed wait/decision payload authority remains in Bifrost, with replayable existing-queue activation; no SQL checkpoint head or competing recovery payload is introduced. |
| INV-024 | Proposed suspension never replenishes computation budgets or deadlines and cannot revive cancelled, expired, revoked or superseded work. |
| INV-025 | Logical ExecutionId survives suspension and all queue deliveries; duplicate delivery cannot count as a new execution or reset execution-wide bounds. |
| INV-026 | Server-owned SubmissionSink commits initial/resume queue admission; Bifrost persists canonical data and the existing engine executes claimed work. Neither Bifrost nor an admission callback executes the Verifier. |

## Required ownership and architecture amendments

| Existing owner | Required extension and boundary |
| --- | --- |
| `wyrd-spec` | Authored foreach/repeat/publication/repository/report contracts, result identities and stable errors. Remains IO/async/SQL/PyO3-free. |
| `vala-verify` | Existing planner/walker/ledger, references, action/check execution, settled collection joins, fixed/array fanout, speculative admission, restore and decisions. Storage is supplied through host interfaces; Vala imports no server state. |
| Server hosted execution | Accepted-authority adapters for repository/library calls, storage, tools and gateway. Reuse current Agent loop/ToolRegistry, not another Agent runner. |
| `wyrd-sql` / `vala-sql` | Existing tenant-bound coordination and tier re-exports: queue coordination/fences, integration/binding/delivery metadata and library store adapters; no competing invocation/publication recovery payload. Payload bytes stay out of control tables. |
| `wyrd-storage` / existing inventory | Complete immutable files and manifests; output readback/integrity. No duplicate object-store subsystem. |
| Bifrost / Scribe | Persist versioned complete run checkpoints, final selection and exact publication custody through the existing shared writer; extend accepted-writer ordering, authoritative recovery reads and WAL replay. Memory feeds dependencies. |
| Existing dispatch owner | GitHub adapter delivery reuses durable claim/retry/fencing/security machinery. It is an integration operation over committed execution states; existing failure Operators keep their semantics and gain no raw report payload. |
| `bohmian-code-review` outer pipeline | Whole context workflow plus reusable GitHub provider additions. Independent library, no Wyrd dependency, server listener or tenant administration. |
| Shared Rust client and SDKs | Thin equivalent authoring/control/read projections. Heavy extraction and storage dependencies stay server-side. |
| Existing `wyrd.toml` config/declaration owners | Shared strict PR configuration schema and Git-tree-backed reference loading; imports reuse composite declaration before execution. Root repository selection and durable repository bindings remain server-owned. |

Approval authorizes these amendments to owning documentation. Revision 18
consolidates approved durability supplement revision 2; implementation and
verification remain task-owned:

1. Wyrd Doctrine/Verifier/Agent: distinguish bounded verification preparation
   and report assembly from a general application runtime; add bounded fanout/repeat, settled publication collections, speculative conditional steps, derived evidence, hosted write/publication tools and repository extraction.
   Retain the twelve-kind catalog. Do not build on the unapproved Source Card
   scaffolding currently present in code; repository evidence is a Verifier
   input, not a new Source kind. Reconcile affected schemas/examples accordingly.
   Expand binding/execution/result subject contracts for repository revisions;
   independent Verifier registration and Card-backed runtime bindings remain.
2. Verifier submission/settlement and Bifrost integration: replace best-effort
   staging acknowledgements for queued verification with recoverable admission,
   checkpoint/output commit and final readback. Retain Scribe's actual durable
   acknowledgement and the existing non-blocking audit contract. Do not store
   analytical payloads in PostgreSQL to achieve this.
3. GitHub security/operations: define deployment-owned App secrets and verified tenant installation connections,
   host-qualified identity, repository-scoped permissions and delivery recovery. GHES addresses obey the
   existing SSRF/network policy; this change adds no private-network bypass.
4. Library authority: authorize provider writes and webhook helpers in the
   outer pipeline while leaving effect policy and execution with its consumer.
   Host identity must participate in persisted identity/object-key scope; a
   format bump never silently interprets old records as another host. Old
   single-host snapshots can be cold-built under the new scope; no mixed-format
   cache reuse is required. Preserve documented context lifetime and budgeting.
5. Workspace config and declaration: extend `wyrd.toml` beyond metadata defaults
   with repository PR bindings. Replace the blanket statement that the server
   never reads `wyrd.toml` with a precise Git-integration exception: parse pinned
   repository bytes through shared schema/loading, never the server filesystem
   or home defaults. Keep authored paths off durable wire/engine contracts and
   keep publishing outside Verifier preparation. Extend public subject unions
   without changing existing Card-subject payloads. Pull-request activation
   belongs to repository bindings, not a Verifier discriminator or a required
   Service YAML field. Team repository publication does not create bindings.

6. Verification lifecycle, storage and operations: use one engine-owned state
   recovered from Bifrost through existing shared-writer WAL acknowledgement;
   extend authoritative accepted-state reads, writer takeover/fencing and WAL
   replay. Keep PostgreSQL queue coordination, exact publication custody,
   independent delivery and existing retention. Direct stays queue-free and
   does not become background execution. Remove duplicate PostgreSQL recovery
   payload ownership without a terminal archive handoff.

Existing declarations are additive-compatible. New capabilities require a
supporting server; older servers refuse unknown fields rather than ignore
execution requirements. Rolling deployment must not admit new-contract work
until every eligible worker supports its persisted version. Drain pre-change
queued work before enabling stronger admission semantics; legacy records retain
their honest historical durability status and are never labelled checkpointed.
Rollback cannot dispatch new-format work to an old worker; drain or pause its
admission first. No compatibility route, alias or alternate SDK contract.

## Acceptance scenarios

These are completion obligations for the whole change, not an implementation
task list. Each journey uses real SDK/HTTP/MCP calls through `WyrdTestServer`
and repository-managed PostgreSQL. Scripted GitHub/model servers control
external responses; extraction/index/ranking/storage/checkpoint logic is real.
External mocks do not replace Wyrd or the library. Live GitHub/GHES smoke
evidence supplements, never substitutes for deterministic journeys.

| ID | Situation exercised | Required observation |
| --- | --- | --- |
| AC-001 | A tenant admin installs the deployment's Wyrd App, selects repositories on GitHub and completes verified connection to the tenant. Its trusted root TOML selects a registered Verifier. A PR opens without any Service/Agent subject Card. | Wyrd resolves a repository binding and one durable execution acquires exact diff/source and a real library context, runs reviewers/critic/requirements, retains JSON/Markdown and publishes that binding's check. No client stages work or moves outputs. |
| AC-002 | Execute the same exact Verifier on two repositories through their PR bindings, using fixed review-criteria attachments. | Each receives its own automatic pinned PR evidence and separate provenance while sharing the same Verifier-owned review configuration. TOML cannot replace fixed attachments or change reviewer/acceptance settings. |
| AC-003 | Use foreach count 1/3, array items 0/1/many, duplicates, invalid/excess arrays and invalid count/both modes. | Count creates independent calls without injected inputs. Arrays preserve frozen indices/order; invalid declarations/expansions dispatch nothing; empty array alone is not a clean verdict. |
| AC-004 | Select distinct security/concurrency Agents with true/false planning flags, fail one reviewer, and consolidate their publication collections. | False skips before action binding; consolidation still runs and receives manifests distinguishing successful, intentionally skipped and failed partial work. Output/named bindings retain missing-evidence behavior; failed work cannot masquerade as clean empty review. |
| AC-005 | An Agent reads a predecessor's large persisted JSON/file through mounted inputs, paged reads/search and a derived citation. | Bytes/digests match committed outputs; citation identifies its producer and original lineage. Mutable scratch, foreign files and forged JSON descriptors cannot become evidence. |
| AC-006 | Stop after a fanout child commits, after its action but before its requirements, and between repeated iterations; recover elsewhere. | Committed calls do not repeat; unfinished work resumes with the same indices, previous value, attempt counts, remaining iteration budget and pinned declarations; stale workers cannot commit. |
| AC-007 | Kill during upload, between output completion/checkpoint commit, after provider reply/before commit, and after final commit/before response. | No partial output is readable; safe orphan cleanup occurs; ambiguous uncommitted calls may repeat without duplicate accepted results; committed final results are found without rerunning. |
| AC-008 | Fail durable queued admission/publication/result commit and delay Scribe projection or File inventory visibility. | No uncommitted work is acknowledged/released; accepted work recovers. Live metadata remains inspectible and pending content visibility is typed/retryable, never absence. Audit failure alone never blocks. |
| AC-009 | Cancel or time out during concurrent/repeated work after one report was committed. | No new invocation starts and late commits refuse. Retained reports and values remain inspectible; no terminal execution resumes implicitly and no incomplete clean verdict publishes. |
| AC-010 | Cold, warm and default-branch-update reviews use the library. Later read a context after GitHub is unavailable. | Cold build occurs only when absent; warm context does not refresh; push maintenance updates incrementally. Read-by-ID returns exact context without GitHub. Wyrd does not duplicate ranking/indexing or clone a checkout. |
| AC-011 | PR moves during acquisition; an old webhook arrives after a newer one; base-only and head changes occur. | No mixed-revision execution evidence commits. Older work cannot cancel/retarget newer work; supersession is visible and old evidence remains attributed to its original base/head. |
| AC-012 | Exercise additions, deletions, renames/copies, binary files, absent patches, unsupported context languages and evidence/model limits. | changes.json represents every change and both available sides. Missing patches are reconstructed or extraction fails. Complete changed evidence is never silently truncated; context omissions remain unchanged and full request limits are enforced separately. |
| AC-014 | Two tenants and two hosts have equal numeric installation/repository IDs; a repo is renamed/transferred. Attempt cross-tenant reads and incompatible host-cache reuse. | Host/tenant scopes remain separate; foreign objects are unavailable; labels do not retarget identity; incompatible cache formats cold-build. |
| AC-015 | Install Wyrd from GitHub while signed out, then sign in and connect selected repositories; separately start from Wyrd or its CLI. Try a missing grant, foreign installation and a repository outside the user's GitHub access. | Both entry points complete the same server-owned connection without user-created Apps or App keys. Only proven repository access can be allowed. Revocation stops affected active work and later IO; retained evidence keeps normal read permissions. |
| AC-016 | Forge installation hints, replay/expire/swap setup and OAuth state across tenants/admins/hosts, withhold GitHub consent, or request an installation pending organization approval. Send signed shared-App webhooks before and after association, including duplicates and failed persistence. | No unverified association or tenant work; pending approval stays pending. Valid completion proves Wyrd and GitHub authority. Shared ingress routes only verified installation ownership; accepted delivery is durable before 2xx and deduplicates independently of logical work. |
| AC-017 | A fork PR is reviewed without executing source; inaccessible fork data and source-injected tool instructions are supplied. | Approved evidence is read as data; no fork runner gets credentials; unavailable evidence is explicit; repository text cannot widen tools, credentials or policy. |
| AC-018 | A reviewer uses selected hosted tools. Unknown tool names, provider-name collisions and revoked permissions are tried. | Selected tools work through the existing loop with recorded provenance; invalid selections fail before model use and denied calls expose no content. Existing built-in declarations and permissions retain their behavior. No remote MCP support is required. |
| AC-019 | Each judgment/terminal status publishes, a GitHub write response is lost, retries arrive out of order, and credentials rotate. | Exact check mapping is observed, remote creation is reconciled, terminal state never regresses, and publication retry never reruns review. Delivery failure is visible and verdict stays unchanged. |
| AC-020 | Return schema-invalid Agent JSON, malformed code data consumed by fanout, oversized Verdict data, and a missing named report required by a consumer. | Existing Prompt and Verdict validation applies; runtime arrays reject before dispatch; no invented code response_format is needed. A missing named member blocks its consumer and clean success without pretending the producer failed; committed partial work remains inspectible. |
| AC-021 | Exercise authoring, submission, progress/item pagination, cancellation, report read and integration administration on every shipped surface. | Rust/Python/TypeScript, HTTP, CLI and MCP agree on identities, permissions, result/error semantics and discoverable schemas. No surface owns a missing server workflow. |
| AC-022 | Run the published docs examples and give a reader only the rendered guides. | The reader connects a permitted repo, authors/customizes a Verifier, obtains reports/checks and diagnoses a documented partial failure without writing an orchestrator. All language examples execute the same scenario. |
| AC-023 | Run an existing document Verifier and a non-repository document pipeline using fanout, conditions, repeat and publications. | Existing document behavior remains; all new primitives work without GitHub or a code-review mode. |
| AC-024 | Recover after a deployment version change and inspect cleanup/offboarding under active work. | Only compatible workers claim versioned records; retained evidence roots survive cleanup; offboarding quiesces work before library removal; rollback never reinterprets records. |
| AC-025 | Delete a referenced Agent/Prompt/Verifier after one item commits, then recover; separately disable a tool capability or revoke execution authority during work. | Unfinished work never uses the deleted/disabled capability or a newer version. Deletion ends errored with retained checkpoints; revocation stops work. Existing evidence reads retain their ordinary permissions. |
| AC-026 | Publish multiple shared Verifiers from a team repository, then select the same exact versions in two application repositories; publish a newer shared version and deny application access to the definition repository. | Registration needs no subject. Both repositories complete their own bindings/checks without copying or fetching definitions; neither retargets to the newer version until its trusted TOML adopts it. |
| AC-027 | Trusted TOML selects one registered Verifier and one local YAML with transitive Prompt/policy/artifact dependencies. Replay after import interruption and try a changed payload under an existing exact version. | Existing declaration/upload recovery completes the local graph before execution; both selected Verifiers use the same engine. Import is attributed to the publisher; version conflict does not rename/overwrite or run affected work. |
| AC-028 | A PR deletes its enforced check, changes a local reviewer, adds a new TOML, targets a non-default branch, or supplies traversal/symlink/remote paths. | Target configuration remains authoritative; new head config cannot weaken checks. Non-default targets use their own pinned configuration; unsafe paths refuse without reading outside that tree. Enabled-but-unconfigured repos show configuration failure, not a clean review. |
| AC-029 | Removed with REQ-039. | No PR-head configuration preview capability is required. |
| AC-030 | Trusted config selects several Verifiers, repeated/conflicting versions of a Verifier family, unknown refs, an empty list or a removed selection; PR deliveries replay out of order while the target advances. | Valid selections produce independently derived check identities and per-binding execution provenance. Invalid selections admit no partial suite and expose configuration failure. Unrelated target commits preserve unchanged revisions; changed content revises/retires bindings honestly, and stale deliveries cannot retire newer selections. |
| AC-031 | Inspect repository bindings/results through every surface, alongside an existing Card-backed Service binding, and recover a repository execution with no application Run. | Repository identity, base/head/tree, trusted configuration are typed and retained without fake Card identity. Card-backed payload behavior remains intact; repository access requires its own permission and no synthetic Run. |
| AC-032 | Two target branches select different versions under the same check ID; one removes that ID, a PR retargets, and stale deliveries arrive. | Bindings, revisions, check identities and retirement stay target-branch scoped. Retargeting selects the new target's trusted tree and supersedes old-target work; stale deliveries cannot undo it. A renamed target resolves its pinned configuration without administrator branch approval. |
| AC-033 | Introduce a defect requiring one changed file plus an unchanged supporting declaration. Read the extractor bundle and metadata through Agents. | The reviewer reads the complete pinned diff/source and real selected context span, with valid original-source citations. pr.json matches extraction output; ReviewContext remains supplemental. |
| AC-034 | Fan out concurrent reviewers over one extraction, interrupt after commit and recover with GitHub unavailable. | All mounts use identical digests and only one accepted bundle; no sibling/recovery rebuild. Live library readback matches retained context before library TTL expires. |
| AC-035 | Pair context from another head with a diff; remove required changed source; exceed the full request budget despite fitting context_tokens. | Revision mismatch and incomplete changed evidence prevent extraction success/dispatch. Model request refuses without truncation; context omissions/index coverage are preserved and cannot certify complete review. |
| AC-036 | Load the proposed files through shipped schemas and run on a bound PR, then manually selected PR, then absent/unauthorized PR identity. | extractor: pull_request automatically executes the real public ReviewPipeline and complete pinned evidence acquisition. No authored PR inputs/from/callback. Missing identity refuses before source IO; document extraction is unchanged. |
| AC-037 | Execute all three example Verifiers on clean, defect and insufficient-evidence PRs; vary selected specialists and agreement/disagreement. | Workspace-only Agents acquire identical evidence, publish retained review material, consolidate all configured branches and return the ordinary final Verdict. Supported clean passes, substantiated introduced defect fails, insufficient evidence is inconclusive; named final report is required without external orchestration. |
| AC-038 | Repeat until true, exhaust the bound, fail/return invalid until data, combine foreach and repeat, and restart at a committed boundary. | No overlapping iterations per item, true stops, exhaustion/failure cannot succeed, previous/index are correct, input-order final array is stable, retries do not create extra iterations, and recovery preserves budgets. |
| AC-039 | Publish runtime-named files, nested Unicode/space directories including empty directories, JSON including null, and final structured output without a file. Try duplicate names and mutate scratch afterward. | Snapshots are immutable/all-or-nothing; logical names are independent of filenames and scoped per invocation attempt; same-attempt name conflict refuses. A fresh Agent retry can reuse the name with separate attribution. Consumers discover files via manifests. Final JSON uses existing response formats; unsafe/unreadable/over-limit trees fail. |
| AC-040 | Lose a publish acknowledgement, retry the same call elsewhere, use a different call with the same name, cancel during upload and replay stale attempts. | Same-call response is recoverable without duplication, distinct same-attempt name reuse conflicts, incomplete trees never appear, accepted snapshots survive, and cancellation fences later commits. |
| AC-041 | Pause Bifrost projection/inventory visibility while an execution publishes; inspect live APIs/UI, restart the projector, then query historical metadata and an authored JSON field. | Live authoritative progress/output identities remain inspectable; pending visibility is explicit; stable events recover through existing Scribe ownership; eventual query returns the same identities and Variant values, with ordinary query permissions. Existing completed output rows keep their meaning. |
| AC-042 | Inspect active and terminal fanout/repeat/speculative workflows across API/SDK/CLI/MCP/UI, then cross access boundaries. | Users see named publications and status/disposition, including withheld/discarded work only under verifier:read and applicable repository read permissions. Ordinary consumers cannot read excluded bytes. Safe previews, provenance, bounded visibility wait and normal content availability states agree across surfaces. |
| AC-043 | Run speculative C after the first bootstrap success, with its action finishing before/after agreement; separately resolve the condition before C becomes eligible. | Pending starts only when ordinary prerequisites are ready; true admits, false cancels/excludes, known false makes no call. Consolidation waits for action plus decision, and true-condition action failure remains visible. |
| AC-044 | Both bootstraps fail, comparison returns null/fails, cycle declarations occur, or a worker crashes at speculative commit/decision/cancel boundaries. | No-success dependency never dispatches; unresolved/invalid decisions admit no bytes. Cycles reject before execution. Durable decisions/budgets recover without duplicate accepted invocation, resurrected work, late commit or leaked provisional content. |
| AC-045 | Mount publications from succeeded-empty, skipped, failed-partial and discarded steps; separately request a missing named report. | Every settled collection has a typed status/reason manifest; skipped/failed states are not fabricated success. Discarded content is absent from ordinary entries. A missing named member still blocks its consumer; final requirements inspect completeness rather than treating emptiness as success. |
| AC-046 | Restore a run with settled, interrupted, failed and control-decision state from Bifrost; repeat with incompatible format. | Reuse complete settlements and pinned decisions; conserve attempts/deadline; incompatible state fails without dispatch, alteration or fresh-state fallback. |
| AC-047 | Run direct verify and submit/poller through the shared engine, including Agent/extraction publications and disconnect. | Direct has no queue admission; both persist/checkpoint through the existing writer; successors use accepted references, and direct disconnect starts no background Agent work. |
| AC-048 | Inspect active and terminal success/error/cancel/timeout, partial/discarded publications and exact downloads without old PostgreSQL payload rows. | Same authorized responses, exact selected result/full errors, page membership and File bytes; source Card deletion does not erase retained identity. |
| AC-049 | Race old checkpoint acceptance with takeover, delay exact accepted state, reject a stale write, restart WAL, and lose terminal delivery replies. | Only current accepted progress survives; prior accepted progress is inherited, rejected progress never resurrects, unavailable exact state waits/fails, and final delivery never reruns settled work; a revision frozen under the old lease before takeover is either refused or inherited, never merged (REQ-057). |
| AC-050 | Continue an existing page cut; replay matching/conflicting admission and publication calls, including unavailable Bifrost lookup. | Original identity/receipt/cursor membership and conflict behavior survive; uncertain lookup does not create a second run. |
| AC-051 | Retry scheduled/group work and Operator/Check delivery while old execution-state dependencies are removed. | Original windows/inputs, group comparison/dedup, binding summaries and independent control/delivery behavior remain; no provider retry re-enters verification and no evidence lifetime changes. |
| AC-052 | Execute a timer wait, restart and cross its fixed wake/deadline. | Original wake time and bounds survive; entry-mode and composition negatives follow the selected contract, not an assumed direct/speculation ban. |
| AC-053 | Pending action wait with independent work, then a fully parked execution and targeted resume. | Peers continue, parked resources release, normal bounded re-admission restores the same execution without rerunning settled work. |
| AC-054 | Human approval plus credential, delegation, stale-login, authorization and tenant negatives. | Recent interactive human proof is required; eligibility follows the separate grant without an added initiator ban; rejection cases await the pending policy decision. |
| AC-055 | Retry continuation delivery and address a prior iteration or PR head; exercise conflict/expiry/cancel under the chosen transition contract. | Accepted decisions are recovered without another human decision; the wrong instance/head cannot be approved, terminal work cannot revive, and no stronger distributed fence is silently required. |
| AC-056 | Crash between suspension/decision acceptance and queue coordination; restart with retained WAL or unavailable source. | Canonical waiting/runnable state converges without another decision; no false absence, stranded accepted wake-up or repeated settled work. |
| AC-057 | Cancel, revoke or supersede an execution while waiting; exhaust its original deadline. | No continuation under changed head/authority or refreshed budget; accepted history remains attributable. |
| AC-058 | Exercise every shipped client/control surface and the chosen interactive human-proof flow. | Current authorization, recent login and applicable browser protections apply; optional UI and bearer possession cannot bypass them. |
| AC-059 | Inspect mixed running/waiting state, GitHub status and complete examples, including old Verifiers. | Waiting is nonterminal and consistent; old declarations and direct fast-return behavior retain their contract. |
| AC-060 | Admit initial work, suspend, then admit a resume delivery; retry after queue COMMIT reply loss. | SubmissionSink creates distinct queue work under the same ExecutionId; matching delivery retries deduplicate, settled work/bounds survive, and source admission/group/schedule effects do not repeat. |

| AC-061 | Interrupt each queued terminal producer before enqueue, after acceptance/reply loss, and before queue settlement; restart and remove source dependencies after delivery. | Immediate cancellation/revocation holds, the same full canonical terminal decision is delivered without recomputation, stable retries deduplicate, and queue delivery retires only after Bifrost ACK. TASK-007 also proves ACK before retiring a suspended delivery. |

### Requirement-to-proof mapping

| Obligations | Primary acceptance evidence |
| --- | --- |
| REQ-001–003, INV-001/011 | AC-001–002, AC-021, AC-023 |
| REQ-004–009, INV-005 | AC-003–004, AC-006, AC-020, AC-023 |
| REQ-045 | AC-038 |
| REQ-051, INV-016 | AC-043–044, AC-042 |
| REQ-006/011 | AC-004, AC-045 |
| REQ-046–047 | AC-039–040 |
| REQ-048–050, INV-015 | AC-041–042 |
| REQ-010–015, INV-004/009 | AC-005–009, AC-024–025 |
| REQ-016–018/020 | AC-010–012/014, AC-024 |
| REQ-021/023, INV-003/006/007 | AC-005, AC-015, AC-017–018 |
| REQ-024–030, INV-002 | AC-001, AC-011, AC-014–019 |
| REQ-031–033 | AC-020–022 |
| REQ-034–038/040, INV-012/013 | AC-026–028/030–032 |
| REQ-041–044, INV-014 | AC-033–037 |
| REQ-052–059; INV-017–019; DUR-001–010 | AC-046–051; implemented Task 001 is the baseline, existing R4 owns every remaining foundation correction/proof, Tasks 002–006 close their consumer seams |
| REQ-055; INV-027; AC-061 | R4 closes every current queued terminal producer under approved supplement3; TASK-007 applies the ACK ordering to its suspended delivery. |
| REQ-060–068; INV-020–026 | AC-052–060; TASK-007 owns the new capability and its consumer closure; wait/approval supplement records approved mechanism and any remaining draft contract decisions |
| INV-010 | Platform CI/build/journeys on all supported targets; shared-mode deployment evidence |

Supporting evidence includes focused pure-engine tests, real PostgreSQL/storage
integration, library pipeline/provider journeys, fault-injected multi-replica
recovery, exact schema/error contract tests, dependency/tenant-isolation gates,
and rendered documentation checks. Named tests and exact focused commands
belong to the later task packet. At minimum the eventual affected lanes include
`fmt`, `lints`, `codegen:check`, `check:deps`, `check:tenant-isolation`, the
owning verification/storage/SDK journeys, `check:examples` and `docs:check`.
Library changes must pass their owning pipeline/provider journeys and gate.
No live credentials or synthetic host load are required for deterministic proof.

## Approval scope and separately deferred decisions

The GitHub amendment is settled by the user's explicit agreement and ordered
replanning instruction on 2026-11-09. The selected connection protocol and public
contract are in REQ-024 and [public contracts](public-contracts.md). TASK-004 is
replaced in full; TASK-005 consumes its verified installation authority and
TASK-006 teaches and tests that actual onboarding. The generic library additions
are kept and extended, not reverted; no Wyrd-specific behavior moves into them.
No GitHub product, authorization, ownership, API or dependency decision is left
to the implementer. Private code shape remains implementation-owned.

The user approved all five TASK-003 recommendations on 2026-11-08 and explicitly
requested the revisions through wyrd-plan. Revision 22 records that scoped
approval; it does not reopen tenant admission or require a limits-response
contract. The cancelled-wait error is specified under equivalent public surfaces
and in the shared public contracts. Implementation and independent task review
remain required; the amendment is not an implementation PASS.

Revision 20 records the explicit request to update this specification and
create TASK-007 after the user agreed that SubmissionSink admits a new queue
record resuming the same suspended execution. Recent interactive login and a
separate Verifier approval grant are also explicitly selected. The remaining
material decisions are identified in [wait/approval](wait-and-approval.md), not
left for an implementer to invent. Existing R4 can proceed independently under
its baseline and already supplied user corrections. TASK-007 exists as a
proposed task; it is not ready while its contract_status is pending.

The decisions below record explicit user approvals and subsequent corrections.
The user explicitly agreed to workspace-based Agent calls, fixed same-reviewer
sweeps, distinct conditional specialists, settled publication joins and optional
third-pass speculation using any_succeeded plus ordinary when. Revision 9
records that agreement and supplies three alternative examples. The user
approved planning this agreed revision on 2026-11-04 by explicitly
requesting six implementation-complete tasks. Revision 9 was approved for that
planning handoff. Revision 10 records the user's explicit correction that only
Agents have write/publish tools; engine-persisted extraction evidence is separate
from those tool calls. Revision 11 applies the user’s explicit instruction to
remove remote MCP tools from this change, including their connection, identity
and transport work. Existing Wyrd MCP client-facing APIs remain in scope. Revision 12 removes PR-head configuration preview at the user’s direction.
The user also approves the PR event rules and configuration failure check, and
requires one active GitHub App installation per tenant. Revision 13 records
the user’s approval to reuse existing Wyrd retention, retain GHES support, and
allow private workspace files to be overwritten before or after publication.
Published snapshots remain immutable. Revision 14 records the user’s approval
of repository-level activation for every PR target branch, using that target’s
pinned configuration without a separate branch allowlist. Revision 15 records
the agreed Task 004 wire/permission contracts and the permitted scoped
verifier:read addition for Task 001; verifier:run continues to cover cancellation.
Revision 16 records approval of the API additions and their public proof. On 2026-11-05
the user explicitly approved all discussed lifecycle and block-checkpoint
recommendations and requested the spec, current remediation and Tasks 002–006 be
revised; revision 17 records that approval. It authorizes active execution metadata
cleanup after full handoff, distinct from the unchanged evidence-retention decision in
revision 13, and direct Agent publication without queued recovery.
The revised contracts are not claimed to be implemented. Earlier exclusions of all speculation and
success-only collection mounts are superseded, not compatibility alternatives.

The revision-17 approval recorded above is historical; its durability architecture was subsequently rejected. Revision 18 incorporates the explicitly approved durability supplement revision 2; it supersedes the earlier lifecycle requirements without changing publication authoring or public API contracts.

The current delivery boundaries are recorded in [tasks/README.md](tasks/README.md):
implemented TASK-001 is the baseline, existing R4 owns every remaining foundation
correction/proof, and Tasks 002–006 consume that foundation. Original task and
review evidence remain historical acceptance context.
They include cross-repository library delivery, owning architecture amendments
and the dedicated in-depth docsite deliverable within the requested inspection
and developer-education task. Task proposal is not task approval or implementation.
Approval is not a claim that unimplemented interfaces are runnable.

## Required implementation boundaries for publication

[Output publication](output-publication.md#required-implementation-ownership-and-reuse)
fixes the reuse owners and missing behavior. In plain terms: Wyrd must remember
acknowledged outputs across retry/restart, and it must authorize and mount those
actual outputs for another step. Neither requires report-specific naming or
code-review logic. Reuse hosted workspace locking/path checks, existing non-Card
File upload/completion, inventory metadata conventions, execution lease fencing,
Scribe/Oracle/Forge and public execution surfaces. Extend their contracts for
strict tree snapshots, durable publication manifests/events, typed publication
resolution and live inspection. Existing best-effort walkers, fresh upload
sessions, stage barriers and in-memory ledgers do not satisfy those guarantees.

Amend the owning architecture sections before implementation of the expanded
contracts: Verifier references/citations and hosted tool/workspace behavior;
Bifrost verification history, canonical table registration and restricted capture
allowlist; existing execution/publication authority under the approved durability supplement.
No architecture file is silently superseded by this specification. The dedicated docsite
deliverable must teach file/directory/JSON publication, actual-name discovery,
plain retry/recovery guarantees, live API/UI debugging and Bifrost query/visibility
behavior with journey-tested examples. Implementation tasks belong in the linked tasks directory.

## Inspected reuse evidence and authority

Original Wyrd inspection baseline: `f4e79ea00523af5cd9212fdacde64f16c97ad554`.
Revision-17 source inspection: `e36474189c2cf74408af6e9c918a1efd84446b92`; original
Task-001 review candidate remains `365bfc94bfef1a153f213462f7ef104afa823741`. Existing
engine/checkpoint, File completion, inventory, terminal reader and former PostgreSQL projector gaps are
recorded in the remediation reuse map; no new framework is justified.
Library inspection baseline: `d8c772de4248ce0130f79ec3b5fefda518b1431a`.
The implementation dependency must pin the delivered library revision, not a
mutable branch or a developer filesystem path. Inspection pins are not a
requirement to retain those old revisions after upstream changes ship.

- [Wyrd doctrine, Verifier and tools](../../../architecture/wyrd-design.md):
  bounded preparation/judgment, exact references, existing evidence/results,
  registration, bindings, Trigger and Operator separation.
- [Security posture](../../../architecture/wyrd-security-posture.md): typed
  RBAC, tenant binding, outbound credentials, SSRF and non-blocking audit.
- [Bifrost durability](../../../architecture/bifrost-design.md#durability-and-visibility)
  and [operations](../../../architecture/operations/README.md): actual Scribe
  acknowledgement, recovery, retention and readiness.
- [Spec workflow](../../spec-driven-development.md),
  [agent surfaces](../../../agent_docs/agent-surfaces.md), and
  [docsite rules](../../../docs/AGENTS.md).
- `crates/wyrd-spec/src/card/verifier/mod.rs`: existing Step/Action/Check,
  inputs, code sources and workspaces; `validate.rs`: output/reference checks.
- `crates/vala/vala-verify/src/walk.rs` and `blocks/{engine,scope,tools}.rs`:
  static concurrent walker, in-memory ledger and hosted tool-session seam.
- `crates/wyrd/wyrd-server/src/verification/hosted/{blocks,evidence,workspace,tools}.rs`:
  tenant-bound executors, document extraction, output materialization gap.
- `crates/wyrd/wyrd-storage/src/service.rs` and server storage inventory:
  non-Card File completion/readback; SQL verifier execution queries: lease fences.
- `crates/wyrd/wyrd-config/src/config.rs`: strict metadata-only TOML schema;
  `crates/shared/wyrd-client/src/cards/saga/build_submission.rs`: existing
  declaration projection and artifact-source validation. Architecture workspace
  config/reference/composite-declaration sections establish the boundary being
  expanded for pinned Git configuration.
- [Library README](https://github.com/bohmian-ai/bohmian-code-review#readme),
  architecture and public `review.rs`, `pull_request.rs`, `context.rs`,
  `github.rs`, `storage.rs`, `persistence.rs`: intended consumer adapters,
  context/diff distinction, 24-hour lifetime, exact reads and provider seam.
- [GitHub webhook guidance](https://docs.github.com/en/webhooks/using-webhooks/best-practices-for-using-webhooks),
  [Checks API](https://docs.github.com/en/rest/checks/runs), and
  [installation authentication](https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/generating-an-installation-access-token-for-a-github-app):
  primary provider constraints inspected 2026-11-04. Provider API versions and
  GHES support must be pinned and tested by the library implementation.

## Revision history

| Revision | Status | Change |
| --- | --- | --- |
| 1 | draft | Full engine-executed reusable pipelines, whole-library extraction, hosted tooling, durable outputs/recovery, tenant GitHub administration/delivery and developer education. |
| 2 | draft | Independent team Verifier publication; repository subjects and trusted root TOML PR bindings; existing local declaration reuse, explicit head preview, binding reconciliation and cross-repository acceptance/docsite coverage. |
| 3 | superseded | Paired evidence handoff; larger example and associated authoring choices subsequently rejected and removed. |
| 4 | draft | Records the earlier automatic PR-context selector automatically executing public ReviewPipeline with automatic pinned PR evidence; aligns minimal TOML/example and removes conflicting authored-input/larger-example requirements. Overall change remains under incremental review. |
| 5 | draft | Proposes one extraction-attached Agent-check requirement, with its Agent in agents/ and Prompt inline; preserves the approved extraction and TOML contracts and adds no orchestration or report schema. |
| 6 | draft | Replaces the single-review proposal with a canonical CodePal-style multi-stage authoring target; specifies proposed generic event/repeat/speculation/channel controls, inline Agent prompts, durable validation/report handoff and recovery scenarios. |
| 7 | superseded | Replaces streams/key routing and guessed report paths with generic runtime file/directory/JSON snapshots; records strict tree semantics, durable receipts, storage/Bifrost history, API/UI debugging and reuse boundaries. |
| 8 | superseded | Records the agreed simpler design: pull_request evidence bundle, output/publications, named publish, ordinary conditions, bounded foreach and sequential repeat; removes speculation/live subscriptions and replaces all examples/diagrams. |
| 9 | superseded | Approved for the six-task planning handoff on 2026-11-04. Records agreed workspace-first Agents, distinct conditional specialists, fixed-count sweeps, settled publication joins and bounded speculative third pass. Replaces one example with three self-contained alternatives and updates supplements/diagrams/proof. Clarifies invocation identity, private attempt workspaces, attempt-scoped publication names and manifest attribution without changing the approved semantics. |
| 10 | superseded | Records the user's explicit correction: only Agents have fs.write/publish tools; standalone code returns JSON and has no direct publication API. Extraction evidence remains automatically persisted by the engine under the agreed evidence reference. Removes the unsupported direct-code promise and resolves the planning blocker; six tasks target this revision. |
| 11 | superseded | Applies the user’s explicit instruction to remove remote MCP support. Retires REQ-022 and removes remote tool declarations, connections, transport and permission identity expansion from Task 003. Keeps hosted Agent tools, repository evidence reads and Wyrd’s existing MCP API surface. |
| 12 | superseded | Applies the user’s explicit instruction to remove additional Agent-facing repository read/search tools. Reviewers use existing filesystem tools over the retained evidence bundle; library context selection/readback remains extraction-owned. Removes Task 004 scenario/proof 11 and repository-tool obligations from Task 003 and the task map. |
| 12 | superseded | Removes PR-head configuration preview and its APIs, publication flow, checks and tests at the user’s explicit direction. Records approval of existing PR event rules and configuration failure checks, and clarifies one active GitHub App installation per tenant. Other queried policy choices remain under discussion. |
| 13 | superseded | Removes the invented retention changes and their redundant replacement requirements/tests; evidence uses the existing File/Bifrost owners. Retires REQ-019, INV-008 and AC-013 without renumbering other IDs. Allows fs.write to overwrite private workspace files while published exports remain immutable. Keeps GHES and specifies local protocol/journey plus appliance compatibility evidence. Branch activation recommendation is still under discussion. |
| 14 | superseded | Applies the user-approved branch behavior: repository-level activation covers every PR target branch, with configuration read from that target’s pinned tree. Removes branch allowlists, default-branch-only activation and branch-rename approval requirements; synchronizes task proof and diagrams. |
| 15 | superseded | Fixes Task 005 repository request/result/binding arms, integration views and permissions. Adds scoped verifier:read for dedicated execution inspection, retaining verifier:run for invoke/cancel and cards:read for explicit Card fetches. Dedicated reads use narrowly scoped internal data access; public Bifrost queries retain table grants. |
| 16 | superseded | Records the user-approved API changes in Tasks 001/005: execution inspection/read/download/cancel operations, shared repository target on execute/enqueue, and GitHub integration/repository administration projections. Fixes producer selection, response envelopes and operation-specific proof without new workflow/tool semantics. |
| 17 | superseded | Explicit user approval on 2026-11-05: disposable per-attempt engine and logical block-checkpoint recovery; lightweight direct path and request-scoped direct publications; complete canonical terminal handoff to Bifrost; one phase-aware reader, retained replay/cursors, bounded Postgres execution cleanup and independent delivery/control lifetimes. Revises Task 001-R1 foundation and Tasks 002–006 without reclassifying the original findings or changing evidence TTL. |
| 18 | approved | Consolidates explicitly approved durability supplement revision 2: engine-owned logical state, Bifrost checkpoints through existing WAL writer, authoritative recovery, fenced takeover/replay, unchanged publication contracts and queue activation; Task 001 replaces the rejected lifecycle foundation before Tasks 002–006. Clarified on 2026-11-05: preserve concurrent memory progress during checkpoint IO and allow independent changes to share a complete checkpoint; existing durability and format contracts remain unchanged. |
| 19 | draft | Records the requested wait/human-approval capability and explicit recent-login requirement. Agreement audit removed unagreed entry, approval, wire/permission/login-flow and race-policy defaults from requirements. Material decisions remain open; TASK-001 follow-ups belong exclusively to R3 and new capability tasks follow contract approval. |
| 20 | draft | Records explicit approval of SubmissionSink-owned resume admission, separate queue-delivery identity with stable ExecutionId, and a separate human approval grant; creates TASK-007 at user request. Keeps unresolved policy/public contracts visible rather than marking them approved. |
| 21 | draft; terminal amendment approved | Records the user-approved uniform terminal ACK-before-queue-settlement mechanism and narrow durable terminal-delivery responsibility in the existing queue (supplement3). Creates consolidated R4 from the validated review; new wait/approval policy remains draft and TASK-007 retains suspension ownership. |
| 22 | TASK-003 amendment approved; unrelated wait/approval remains draft | User approval on 2026-11-08: shared process/per-execution admission without new tenant quotas; Rust-owned recovery fault injection; retire acceptance-time limits reporting; shared SDK cancelled-wait error; deterministic cancellation-writer race proof. |
| 23 | draft; GitHub correction requested | User rejects TASK-004's per-tenant App model and implementation. Replace it with installing the deployment-owned Wyrd App, GitHub- and Wyrd-initiated connection, server-owned OAuth proof, tenant association and shared webhook routing. No optional per-tenant App mode. Earlier approved work remains unaffected. |
| 24 | GitHub amendment approved; unrelated waits retain prior status | User agrees to the researched deployment-owned App convention and orders complete TASK-004 replacement and downstream alignment. Fixes one App per deployment, verified browser onboarding, shared webhook routing and keep/extend of the independent generic library branch; removes the proposed second repository-snapshot policy. |
