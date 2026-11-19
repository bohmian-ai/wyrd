# Review orchestration: authoring and backend behavior

Revision 18 preserves the agreed primitives and three alternative
[Verifier examples](proposed-examples/README.md). They are proposed contracts,
not currently runnable APIs. All three use ordinary Agent actions and a final
Agent-check requirement; Prompts live inline in each example's agents/ folder.

## Workspace-based Agents

An Agent can be told to review attached files, with no injected Prompt inputs:

```yaml
action:
  kind: agent
  agent: ./agents/reviewer.yaml
  workspace:
    /inputs/pr: steps.extract-pr.publications.evidence
```

The mount contains pr.json, diff.patch, changes.json, changed/before and after
source, and supplemental context.json. The Agent reads them with fs tools.
[Extraction](pull-request-extraction.md) specifies exact evidence and failure
rules. Inputs remain useful for actual Prompt placeholders; they are optional,
not mandatory transport for review accounts or file contents.

## Example 1: repeat the same reviewer, then consolidate

[Repeated sweeps](proposed-examples/repeated-sweeps/verifier.yaml) adds:

```yaml
foreach:
  count: 3
```

The backend freezes three invocation identities, gives each private scratch and
the same immutable PR mount, and dispatches within existing capacity. These are
independent calls, not one conversation and not retries. Successful output is
an ordered three-element array. A failed invocation prevents a successful
aggregate output, while its status and committed partial publications survive.

The consolidator mounts the complete settled review publication collection. It
reads manifest.json to distinguish each invocation/attempt and discover actual
paths, validates claims against source, and publishes its report. The final
requirement mounts the named report and judges it against the original evidence.

`foreach` also retains dynamic `items: <array reference>` plus `max_items` for
other pipelines. Exactly one of count/items is allowed; count must be positive,
while an empty valid items array makes zero calls. Duplicate array values are
independent original indices. Validate/freeze before dispatch; never truncate.
No Prompt input is needed merely to distinguish calls. Item/index bindings are
available only if the author actually wants them.

### Invocation identity and workspace layout

The durable logical identity is `(execution_id, step_id, item_index,
iteration_index)`. The last two fields are null when foreach/repeat is absent;
otherwise they are zero-based. For count 3, no sequential repeat:

| Logical invocation | Evidence visible inside the Agent | Scratch/conversation |
| --- | --- | --- |
| execution E, review, item 0, iteration null | /inputs/pr: the same committed PR evidence | Private to this attempt. |
| execution E, review, item 1, iteration null | /inputs/pr: the same committed PR evidence | Private to this attempt. |
| execution E, review, item 2, iteration null | /inputs/pr: the same committed PR evidence | Private to this attempt. |

Attempt 1 and attempt 2 of item 1 are retries of one logical invocation, not a
fourth independent reviewer. Recover the same logical identity and committed
work. An ambiguous uncommitted call can repeat with its attempt recorded;
committed calls are not dispatched again. Each attempt has a fresh Agent session
and tool history, potentially containing several model requests. No sweep index
is injected into a Prompt unless explicitly bound by the author.

All three Agents may write /work/review.md without collision. The path is local
to each attempt, not a shared directory. The evidence is logically the same
immutable content; this does not prescribe whether hosts copy or share read-only
bytes locally. Publications are scoped by invocation/attempt, so logical name
review can also be reused safely by all three calls.

After settlement, /inputs/reviews contains manifest.json and content grouped
beneath opaque host-generated invocation segments. Its entries link every actual
mounted path to item_index, iteration_index and attempt. Consumers use those
paths, never guess segments or derive storage keys. The manifest includes all
invocation/attempt statuses and reasons; two successes plus one failed partial
review remain explicitly different from three successful reviews.

Final returned values are retained as output; they do not automatically become
publications. The reviewer Prompts explicitly request publishing the accounts
that the consolidator will read. A missing expected publication is missing
review evidence, not an invented clean result.

## Example 2: distinct conditional specialist Agents

[Conditional specialists](proposed-examples/conditional-specialists/verifier.yaml)
has one general reviewer and a planner. The planner's existing Prompt schema
returns two booleans, security_review and concurrency_review. Each specialist
is a distinct Agent with its own inline Prompt and tool selection:

```yaml
when:
  path: steps.plan.output.security_review
  operator: equals
  expected: true
```

The backend evaluates the condition before resolving that branch's action
bindings. True runs, false intentionally skips, missing/wrong-type decision or
failed planner records error rather than inventing false. General review and
planning can run concurrently; selected specialists can run as capacity allows.

Consolidation mounts all three collections, regardless of terminal success.
Skipped specialists supply status/reason and no content; failed reviewers supply
failed status and accepted partial content. The consolidator therefore runs
without pretending every branch succeeded. The final requirement determines
whether the stated review is complete; Wyrd supplies no specialist quorum.
A successful response with missing expected review material is not evidence of
a complete review simply because no tool reported a defect.

## Example 3: a speculative third pass

[Speculative review](proposed-examples/speculative-review/verifier.yaml) starts
two independent bootstrap Agents with different sampling temperatures. A
comparison Agent waits for both settled collections and compares supported
claims. The extra pass has:

```yaml
depends_on:
  any_succeeded: [review-a, review-b]
when:
  path: steps.agreement.output.agree
  operator: equals
  expected: false
speculative: true
```

`any_succeeded` means at least one listed step has fully settled with existing
status succeeded. If all settle without success, the extra pass never starts;
record their failure/intentional-skip cause. Other required action bindings must
still be ready. The old list-form depends_on remains ordering-only. The object
form cannot be combined with that list in one field.

`speculative: true` permits work before the when decision, not before source
availability or authorization. It requires when; without the flag, a conditional
step waits normally. Only condition dependencies are deferred. A normal action
binding to the comparison's output would still wait for that value.

| Decision/action ordering | Backend observation |
| --- | --- |
| Decision already false before dispatch | Skip; make no extra Agent call. Recheck while waiting for capacity. |
| Decision already true | Run normally, subject to ordinary prerequisites. |
| Decision pending, action ready | Start when capacity permits; retain content as pending/withheld. |
| Decision later true | Admit completed/ongoing work; consumers still wait for action settlement. |
| Decision later false | Cancel unfinished work, fence later commits, exclude its contribution; accepted bytes remain inspectible. |
| Action finishes before decision | Persist it, but keep ordinary consumers waiting. |
| Decision fails, is absent or not boolean | Mark failed/blocked; admit no contribution, including already completed content. |
| True decision, action fails | Failed/partial work is visible as failed in the settled collection. |

The example comparison returns agree true/false only for a complete comparison;
null means unable to compare. For this boolean condition, null is wrong-type
rather than an implicit false/true decision, so speculative admission blocks.
The reason field explains the comparison; it is not a built-in agreement type.

```text
PR evidence -> Review A --------+-> compare both -> keep/discard extra work
             Review B --------+                    |
                 |                                 |
                 +-> first success -> extra review -+
                                                   |
                                    settled consolidation -> final requirement
```

This supports the third-pass optimization motivated by
[CodePal](https://eng.snap.com/codepal). It does not reproduce intermediate-
discovery successor launches or a live long-running validator. Those require
an event/read contract excluded here. No automatic speculative descendants,
separate release/cancel conditions or semantic finding deduplication is added.

## Publications, status and named requirements

| Reference | Contract |
| --- | --- |
| steps.extract-pr.output | Captured PR metadata JSON, not the diff or an injected prompt. |
| steps.extract-pr.publications.evidence | Strict successful extraction directory mount. |
| steps.review.output | Successful final value; fanout returns ordered values, until-repeat the final value. Failure is not null. |
| steps.review.publications | Immutable settled collection with statuses, reasons and admitted committed work. Skip/failure does not prevent collection mounting. |
| steps.consolidate.publications.report | Required named content from successful admitted consolidation. Absence blocks the consumer; no filename is assumed. |

[Publication](output-publication.md) defines exact publish arguments, collection
manifest fields, directory snapshots and retention. Agents choose report format
and actual filenames. Writing scratch alone retains nothing. A publish receipt
means durable bytes/manifest/projection intent, not speculative admission or
immediate analytical visibility. Generated opinions do not replace source
citations. No `.outputs`, `.result` envelope or read/follow tool is introduced.

## Optional sequential refinement and limits

The three examples do not need sequential refinement, but the agreed repeat
primitive remains available: max_iterations plus until comparison. Each invocation
waits for the prior action/requirements; iteration.previous is initially null,
then the previous value, and iteration.index is zero-based. Until alone can
read iteration.output. True stops, false continues, failure/invalid data stops,
and exhaustion is unsuccessful. Repeated scratch is not shared; carry cumulative
state explicitly when needed. The final value is output; a named publication
selects only the final successful iteration, with no earlier fallback. Combined
with foreach, each item repeats independently.

Concurrency means how many Agent calls may run simultaneously, not how many
sweeps exist. Every call consumes existing tenant/deployment/execution capacity.
Optional authored limits only lower those ceilings; omission uses exposed server
defaults. The examples deliberately omit concurrency settings. Without retries, repeated
sweeps uses five Agent invocations; conditional specialists uses four to six;
speculative review uses five or six depending on whether the extra pass starts.
Model/tool turns are bounded separately inside each Agent. Retry/restart
never replenishes budgets; waiting for capacity/visibility spends no model call.

## Required backend support and recovery

| Existing owner | Required extension |
| --- | --- |
| Planner/walker/ledger | Ready-step dispatch without unrelated stage barriers; fixed/array expansion; condition-before-binding; any-success readiness; settled publication joins; action and decision tracked separately for speculation. |
| Engine state / Bifrost | Frozen invocation identities, progress/requirements, budgets, condition decisions and admission/discard checkpoints under lease/cancellation fences. Recover unfinished work, not committed calls. PostgreSQL owns coordination time. |
| Hosted workspace/File storage | Private scratch; immutable consistent snapshots; idempotent named publish; authorized frozen mounts with status manifests. Excluded speculative bytes remain inspectible through authorized execution reads, not downstream mounts. |
| Existing Agent/tool runtime | Reuse the same Agent loop, exact Prompt/tool schemas and accepted authority. No reviewer worker or alternative orchestration engine. |
| Bifrost/public execution APIs/UI | Project stable history through existing Scribe; inspect active/failed/discarded content with proper access, safe previews, projection lag. Preserve existing completed output-table meaning. |

Persist the speculative decision and spent budget before releasing its effects.
An action-completion/decision race cannot release pending content. A late success
after discard cannot reverse it. Restore the original decision after worker loss;
ambiguous uncommitted model calls may repeat, but accepted invocations/receipts
must not duplicate. Cancellation retains acknowledged evidence for inspection.
A terminal execution never silently resumes. No new scheduler/outbox/object
store or client-owned durable behavior is introduced.

Execution storage, acknowledgement and recovery follow [the approved durability supplement](execution-state-and-durability.md); no PostgreSQL run-state mirror or terminal archive handoff is implied by these authoring examples.
