# GitHub Verifier pipelines

Status: exploring

## Problem

Users need one declared repository review policy that starts Verifier executions
on eligible PRs, accepts later evidence and approval without occupying workers,
and presents supported findings in GitHub. The declaration, activation and
execution contracts must form one complete backend journey through existing
Wyrd owners, without a UI or a separate workflow system.

## Desired outcome

1. An operator configures the deployment's GitHub App.
2. A tenant connects it and selects repositories, granting Wyrd access.
3. `wyrd declare wyrd.toml` infers/registers a native Repository Card, resolves
   its Verifiers and dependencies, and activates its exact review bindings.
4. An eligible PR stages ordinary queued verification through the existing
   admission, queue, runner and DAG engine.
5. Extraction supplies pinned PR changes and the independent library's actual
   source context, available both as Prompt inputs and workspace JSON files.
6. Any CI provider can supply declared deferred inputs through Wyrd uploads.
   Human approval and missing deferred inputs suspend without occupying workers.
7. A completed execution's standard report supplies GitHub review content;
   ordinary Wyrd requirements and decide determine the verdict.

## Constraints and non-goals

- No UI work here; it ships on another branch.
- No new PostgreSQL tables, second verification queue, general event bus,
  generic timer/external wait actions or replacement checkpoint store.
- Reuse existing storage, Bifrost verification records, queue tables, Scribe
  outbox and SubmissionSink. Bifrost owns accepted execution state; PostgreSQL
  coordinates work. These constraints do not imply missing behavior exists.
- bohmian-code-review stays independent. Its pipeline owns generic acquisition,
  comparisons and context selection; Wyrd owns tenancy, authorization, Cards,
  policy, execution history, adapters and integration.
- Direct and queued work have isolated execution, workspace, Python and
  model-admission capacity, with no initial borrowing. PR extraction, deferred
  input suspension and human approval use queued execution.
- Support durable acceptance and bounded progress for bursts such as 41 PRs.
  Arbitrary DAGs have no fixed completion-time promise. No speculative cache
  coordination infrastructure without measured need.
- User approval is required before recording new design proposals. Approved
  decisions below are design input, not an approved specification.
- Do not create specs/tasks, change production code, stage, commit, switch
  branches or finalize design without the corresponding user instruction.

## Proposed implementation

The following direction is approved; links own the detailed decisions.

```toml
[repository.verified_by]
verifier = ["./review/code-review.yaml"]
runs_on = { kind = "pull_request" }
```

| Boundary | Approved behavior | Detailed record |
|---|---|---|
| Repository and declaration | Native Repository Card; infer identity/envelope, allow verified overrides, resolve selected graph, reuse normal Card versions, activate only after readiness; preserve prior selection on failure | [Declaration](approved-declaration.md) |
| App and activation | Operator configures App; one active installation per tenant and one tenant per installation; repository access is separate from Card declaration; webhook stages queued verification | [Runtime and GitHub](approved-runtime-and-github.md) |
| PR lifecycle | Skip drafts; opened/reopened/ready and source changes run; draft/close/access removal stop affected unfinished work; new heads supersede unfinished old reviews | [Repository contract](repository-card-contract.md), [code-review support](approved-code-review-support.md) |
| Extraction and incremental review | Return pr/review/changes/context; context is the existing library ReviewContext unchanged; select baseline from the last completed usable review under the same policy; preserve complete/previous evidence | [Code-review support](approved-code-review-support.md) |
| CI and approval | CI publishes evidence through CLI/SDK; Wyrd matches subscribed deferred inputs and resumes through the queue without CI discovering executions; separate approval permission using normal authentication/authorization | [Deferred inputs](approved-deferred-inputs.md), [runtime rules](approved-runtime-and-github.md#durable-waits-and-resumption) |
| Report and delivery | Optional standard code_review step output; zero means ordinary summary, one supplies report, multiple is an error; no selector on Verifier or binding; Checks and PR comments/suggestions | [Report delivery](approved-report-delivery.md) |

New source uses the last completed usable baseline, never partial work. If P
completed, A is running and B arrives, cancel A and review P→B; without P, review
the full PR. Retries retain exact source/policy identities. Policy changes affect
future admissions and do not automatically rerun every open PR.

Parked work holds no active execution, workspace, Python or provider capacity.
Independent runnable branches continue. Resumption preserves the same execution,
accepted work, original deadlines, retries and spent budgets.

The report convention uses the existing step-output mechanism. Intermediate
reviewers can reuse Finding schemas; an ordinary consolidator emits the final
code_review envelope. Severity and confidence remain separate from the verdict.
A provider-delivery retry never recomputes the review.

Evidence publication matches stable repository identity, commit tag and input name within the
tenant. Infer repository/commit from Git where available; do not repurpose
group_id or require a publisher idempotency key. An active declared binding owns
the subscription even with no running execution. Retain matching evidence for
that subscription under normal retention; reject unmatched delivery explicitly
without historical replay for later-created subscriptions. Publication alone
never starts a run. Identical content replays acceptance; conflicting replacement
is refused. The [deferred-input record](approved-deferred-inputs.md#approved-publication-matching-and-subscription-lifecycle)
owns the approved CLI shape and complete scenario table.

## Evidence and rationale

Research inspected Wyrd at f4ca61126, bohmian-code-review at d8c772d and locally
available generic App helpers at historical 9fdf6dc. These are research snapshots,
not a final dependency pin or runtime proof.

- CLI/loader resolve Card graphs offline before shared declaration; current TOML
  is defaults-only. Client declaration stages submit/upload/complete; registry
  resolves exact versions and projects Card-owned bindings. These existing owners
  support extending declaration with a native Repository root.
- The library's current ReviewPipeline returns PrContextResult with record,
  lifetime and context. Context contains selected source text, provenance,
  omissions and coverage. Pinned baseline-aware complete/active diff output is
  new library work, not existing public API behavior.
- AgentExecutor::execute already returns schema-validated JSON as ordinary step
  output; Agent inputs bind Prompt placeholders, and workspace mappings serialize
  JSON to files. The named code_review schema and GitHub consumer are new work.
- Existing SubmissionSink stages acknowledged inputs into queue admission and
  reports committed admission. Existing terminal delivery records Bifrost results;
  it does not already deliver GitHub reviews. Detailed gap/constraint verification
  remains necessary before task planning.
- [Bursts and successive commits](bursts-and-successive-commits.md) retains queue,
  cache, Mitari and provider research. Ordinary PRs reuse repository snapshots;
  cold first-use duplication is not evidence for a new coordination subsystem.
- [Snap CodePal](https://eng.snap.com/codepal) supports bounded shared context and
  incremental review. [Devin](https://docs.devin.ai/work-with-devin/devin-review)
  and [CodeRabbit](https://docs.coderabbit.ai/configuration/auto-review) informed
  incremental review practices. CodeRabbit documents in-flight cancellation;
  inspected Devin/CodePal sources do not establish their cancellation protocols.

[Worked examples](worked-examples/README.md) distinguish the approved standard
report flow from three orchestration design examples. They are design
artifacts, not engine-tested examples. The standard example's TOML, local links
and five pure policy cases were checked; future engine contracts were not run.

## Decisions and alternatives

Read approved records by boundary, not as a chronological transcript. Their
explicit later corrections supersede earlier proposals.

Rejected or superseded directions:

- Non-Card repository root and synthetic Service subject: use native Repository.
- Separate binding/import command: extend `wyrd declare` with inferred TOML root.
- Mandatory metadata/spec boilerplate: infer it; user values are overrides.
- Metadata-only extraction and invented evidence publication: return actual
  context in normal output, available to both Prompt and workspace mappings.
- Verifier/binding report selectors: recognize the single standard envelope.
- Generic timer/external waits and GitHub Actions-specific CI acquisition:
  use deferred Wyrd inputs and separate human approval.
- New PostgreSQL tables or a separate workflow/outbox framework: extend existing
  owners. Source/storage feasibility must be established, not assumed.

[Original Repository proposal](repository-card-contract.md) and
[superseded non-Card proposal](repository-declaration.md) retain research only
where they agree with current approved records. They do not reopen decisions.

## Standards conflicts

Repository authority still needs updating before specification approval:

- Native Card catalog/root/composition/reference and binding rules gain Repository.
- Trigger rules gain closed pull_request activation; old rules exclude pushes.
- wyrd.toml gains an inferred declaration root beyond defaults configuration.
- Input/engine contracts gain deferred evidence and human approval suspension;
  Prompt schema resolution gains code_review.

No authority files have been changed. Standard Card identity, exact references,
server ownership, tenant isolation and audit behavior remain required. Current
source uses UUID declaration-operation keys while architecture describes a
content-derived key; revalidate this mismatch at its owner before relying on it.

## Open questions

Latest user correction: approval depends on normal authorization, without a
fresh-login requirement or five-minute authentication window. The user approved
replacing the draft's publication-version overrides and reactivation rules with
repository/commit/input matching and the existing/new subscriber scenario table. Execution policy pins remain exact; the publisher
need not choose them. The first comprehension pass did not authorize the
rejected additions.

### Remaining specification work

The user approved the remaining recommendations, with a correction to evidence
publication and a reminder that conventional App deployment was already settled.
Large-PR context strategy, trusted base guidance, severity meanings, Check
conclusions, manual thread resolution and deadline behavior are no longer open
product questions. Their detailed owners are the approved records above.

The remaining contract work is concrete specification drafting and validation:

- Publication matching, inferred repository/commit, input dependency and subscription
  lifecycle are approved, including no unmatched retention/replay and no required
  caller retry key. Specify exact SDK/wire forms and retained evidence custody
  for subscriptions without active executions, then validate restart-safe
  continuation. CI does not query executions or use provide_input/group_id.
- Complete the nested extraction/report/edit schemas and human-approval request,
  result and permission surfaces using the approved behavior. Present unseen
  fields for approval; do not invent new policy, services or configuration.
- Specify restart-safe admission, supersession, continuation and GitHub delivery
  at the existing owner boundaries, with no new PostgreSQL tables.

Do not reopen conventional App deployment or introduce a GitHub.com-only scope
restriction. Exact conventional configuration and callback details belong in the
specification, not another product-design round.

### Research needed before implementation tasks

- Prove webhook deduplication/partial fanout, supersession, parked continuation
  and GitHub delivery retry/reconciliation fit existing tables/state/outboxes.
  Name the precise owner extensions and consequential ACK boundaries; do not
  assume cross-store atomicity or equate process-local staging with durable ACK.
- Establish operator configuration for isolated capacity and suitable execution
  deadlines, plus proof of direct-path availability during queued saturation.
- Revalidate the independent library facade, provider helpers and version pin;
  preserve its independence and avoid duplicate acquisition/ranking pipelines.

Private function names, helper placement and reversible local implementation
choices do not need another design decision. They belong to implementation.

## Handoff

The user's requested [complexity audit](../research.md#complexity-audit--revision-3)
produced spec revision 3 draft and reconciled contracts. It removes the added
approval proof, version-targeting/reactivation protocol, extra App management
workflow, prescribed permission categories and SQL layouts. The remaining
requirements are mapped to their approved design basis in that audit. A bounded
independent comprehension pass found no material contradiction in the revised
spec and contracts. This does not approve the specification or prove runtime
recovery/capacity behavior.

Spec revision 3 is drafted and audited for unnecessary complexity. It is not an
approved spec or task-ready plan. The user still owns design completion.
Stay exploring until asked to finalize; do not invoke spec/plan automatically.

Next: review the corrected draft, validate the necessary existing-owner
extensions, update authority through the approved
workflow, then produce a small outcome-based specification and task packet.
