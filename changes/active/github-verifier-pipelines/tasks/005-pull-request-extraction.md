---
id: TASK-005
status: proposed
approved_spec_revision: 5
requirements: [REQ-006, REQ-007, REQ-016]
acceptance: [AC-004, AC-011]
invariants: [INV-001, INV-002, INV-004, INV-005, INV-007]
depends_on: [TASK-001, TASK-004, TASK-006, TASK-010]
---

# Extract accepted PR source into ordinary evidence

Carry **approved-code-review-support.md → Approved pull_request extraction
contract / Model input and automatic behavior / Successive commits and pipeline
ownership** into **contracts.md → Pull request extraction**. Although numbered
005, this task depends on 006's admitted source; it must not manufacture an
alternate manual source-injection endpoint to avoid that dependency. It also
requires task 010's memory execution runtime and spec revision 7 REQ-017.

## Input and existing owner

Input is task 006's actual admitted queued activation: exact tenant authority,
Repository/Verifier/Trigger pins, provider repository/PR base/head/source identity
and generation. Wyrd's retained completed execution history supplies the last
completed usable head for the same PR and exact policy; task 004 supplies the
extended library facade and production adapters. A caller supplies no installation
credentials. Missing accepted source identity refuses before acquisition.

`wyrd-spec` Verifier actions/validation and `vala-verify` block dispatch currently
support media `extract/from`; the server's hosted evidence/blocks bind tenant
reads and Agent execution. Extend the existing extract action with exactly one
of `from` or `extractor: pull_request`. Preserve existing formats and failure
semantics.

### Revision — registration-derived `queue_only`

When introducing `extractor: pull_request`, extend the server-owned registration
derivation introduced by [010-R1](010-R1-restore-direct-verification-baseline.md)
to set the Verifier's `queue_only=true` for this form. Preserve other supported
queue-only conditions; users cannot set or override the flag. This replaces the
former instruction to add a pre-execution graph guard. Direct verification
reuses the existing flag check on cold registry load before cache insertion;
do not add request-time extraction scans or repeat eligibility work on cache hits.
Verify registration classification, direct cold-load refusal before any step,
and that ordinary media extraction remains direct-eligible in the absence of
other queue-only features. Task 010-R1 implements publish detection only;
this task owns the extraction extension and its proof.

### Revision — metadata batching and group commit

Extend the existing `CodeReviewMetadata` owner to collect pending metadata writes
and group-commit independent operations in short tenant-scoped transactions.
Use verification `Completions` as the model for collection, flush and per-call
replies; metadata remains owned by `CodeReviewMetadata`.

Workers take available operations up to a bounded batch size as soon as they are
free. Do not wait for the batch to fill or add a collection timer: a lone write
commits immediately when a worker is available. Bound pending work and worker
concurrency. Return each operation's result only after its transaction commits;
dependent pipeline work still waits for that result. Preserve conditional pointer
publication, conflict handling, context timestamps, tenant isolation and retry
semantics. Failed batches report failure without falling back to a transaction
per operation. Reads remain available when the pipeline needs them.

Release the transaction before GitHub or object-storage IO. Do not hold a state
lock while waiting for admission, SQL or commit, or reuse the execution-completion
queue for metadata. No new database, generic batching service or configuration
surface is selected.

Verify concurrent independent writes share commits, low-traffic writes complete
without waiting for another arrival, replies follow commit, failures retain the
existing operation semantics, and tenant batches remain isolated. Exercise the
actual pipeline with real Postgres/object-store adapters; preserve pointer races,
context lifetime and cleanup coverage. Record batch sizes and transaction
boundaries through existing tracing/test seams; no benchmark or production hook
is added by this revision.

### Revision — extracted-source baseline eligibility

A completed execution is not proof that its source was extracted: a failed
pull_request extraction still settles `completed` with an `inconclusive`
verdict. Baseline eligibility is a source fact, never a judgment; the verdict
does not decide it.

Record `pr_source_extracted` on the execution row through a new immutable
migration (`BOOLEAN NOT NULL DEFAULT false`, true only for completed
`pull_request` executions). The runner derives it from the successful
pull_request extraction settlement of the admitted source (its server-assigned
extraction provenance), not from output JSON or the verdict, and carries it
through the completion transition. Write it atomically with `status`,
`result_id` and settlement timestamps in the existing lease-fenced completion
transaction, in both single and batched completion, preserving it on identical
replay and on recovery that reuses accepted extraction. Do not write it when
extraction finishes: cancelled or unfinished work must never become eligible.

Baseline selection, including the candidates that identify `policy_changed`,
requires the fact. A successful extraction remains eligible when later judgment
is failed or inconclusive; the complete PR remains in `review.full_pr` for the
Verifier to use. Existing rows default to false, giving full reviews until
eligible history accumulates.

Verify at Tier 2: a → unreadable d → b keeps a as `previous`; successful
extraction with a failed or inconclusive judgment advances; recovery keeps the
fact without reacquisition; cancelled work does not advance; policy change
falls back. Verify at the SQL owner: single/batched parity, rollback, stale
lease and replay.

### Revision — no client projection of the extraction

The extraction has no client consumer. Its consumers are server-side: the
same Verifier's Agent and code steps and requirements through ordinary
`steps.<id>.output` references, recovery from accepted state, the next head's
baseline selection over execution history, and task 009's report validation
against the retained pins. Without a defined client consumer, no client SDK
surface or type is created for it.

This drops from "Output, consumers and surfaces" and AC-011 the Rust, Python
and TypeScript typed read projections of the extraction output, and SDK
authoring helpers for `extractor: pull_request`. The action stays authorable in
Card YAML and JSON through the shared loader and declaration route, and the
regenerated TypeScript wire types carry only the action field. Inspection uses
the existing execution details on HTTP, MCP and CLI, where the step output is
ordinary JSON; this task adds no inspection-specific type or command.

## Agreed transformation and custody

Select the last completed usable same-PR/same-policy review from retained
execution history, freeze it with this logical extraction and call task 004's
public pipeline once for accepted evidence. Partial/cancelled work never advances
the baseline: P completed, A running, B admitted selects P→B. Without history,
with unavailable/rewritten history or changed policy, use the full PR comparison
and the exact approved fallback reason. An unavailable authoritative execution
state is a retryable failure, not permission to pretend no prior state exists.
Keep actual PR base and merge-base distinct from comparison_base.

Return the closed typed output in contracts: format, pr metadata/identities,
review mode/comparison_base/previous execution ref/fallback_reason/full_pr,
active changes with paths/status/blob IDs/hunks, and unchanged library context
including actual selected source text, omissions/budget/coverage. changes is
active-only; complete evidence remains review.full_pr in the same JSON and is
not implicitly added to the changes or context bindings. Authors may explicitly
bind the entire extraction output. Retain enough before/after source
to validate exact locations/replacements after restart and library cache expiry.
Acquire through the library; no Wyrd clone/index/rerank or filesystem discovery.

Validated pinned extraction enters live `RunState`, and dependent Prompt/workspace
consumers use it immediately, without waiting for checkpoint or Scribe ACK.
Only extraction actually persisted in an accepted revision is reusable recovery
evidence; queued recovery reuses that exact output with its exact source/context
and immutable object descriptors, and unpersisted extraction may repeat after a
crash. Library context-record lifetime cannot invalidate retained Wyrd evidence.
Normal Agent inputs may select the whole output or fields and Prompt placeholders
must match. Existing workspace JSON mapping materializes the same validated values;
it is neither a source-tree checkout nor implicit input to unrelated Agents.
Repository guidance comes from pinned base source; edited guidance is evidence,
while exact Agent/Prompt owns policy. Use existing model limits; an oversized
input fails honestly, with no automatic splitting service or clean-review claim.
Optional formatting uses ordinary input binding, without mandatory prep stage.

Amend **wyrd-design.md → Verifier / Agents, tools, and workspaces** and applicable
**bifrost-design.md → Durability and visibility / Ingest** for typed extraction
and source custody; preserve existing media extraction and accepted-state recovery.

## Output, consumers and surfaces

Dependent Agent/code steps consume the validated output through ordinary refs.
Task 009 validates report anchors/edits against the retained pins/source, and
later completed reviews can supply review.previous through ordinary execution
reads. Ship action/output schemas, stable errors, all SDK authoring/typed read
projections, CLI and HTTP/MCP inspection, and focused extraction/binding guidance
here. No report format, verdict or provider write is introduced here.

## Connected proof

**AC-004:** Use task 002's real connection, task 003's declared selected policy
and task 006's actual accepted PR activation; run extraction through the queue
and task 004's public pipeline. Have a normal Agent receive pr/active changes/
context as declared Prompt values and mount the entire same output as JSON.
Observe exact source text/pins in both, unchanged context, no implicit source
checkout and no second acquisition for another consumer. With persistence ACK
held, observe dependents consume live extraction. Recover after cache expiry and
observe the retained accepted evidence with exact source/context. Drive full/incremental and
fallback cases, including P/A/B, omissions/missing required source and model
input limit: none can turn missing/failed review into passed. Empty active diff
remains evidence for authored requirements. Verify legacy media extraction and
mutually exclusive action fields; direct invocation refuses before first step.

**AC-011 (extraction):** Follow the input/workspace guide and inspect the same
typed extraction/previous execution ref across SDK, CLI and HTTP/MCP.

Tier 1: corresponding SDK story journeys through WyrdTestServer/Postgres and a
local provider/model transport, existing Rust/Python/TypeScript/MCP runners in
README. Tier 2: hosted retained evidence/recovery in `wyrd-testing` server target
via `mise run test:bifrost:journey:server`; tier 3: spec action/reference/schema,
block binding and baseline decisions at their owners. Register new tests in
existing targets and record actual focused commands after adding them. Apply
README contract/type/codegen and dependency/tenant checks. A hand-built extraction
JSON fixture alone cannot prove any predecessor handoff in this journey.

## Obligations

| ID | Source | Observation | Planned proof |
|---|---|---|---|
| X1 | REQ-006; contracts → Pull request extraction ¶1; AC-004 | `extract` takes exactly one of `from` or `extractor: pull_request`; neither or both is refused at validation; media `extract/from` keeps its formats, output and failures | spec validation unit tests; existing media extract tests; legacy media step in the journey |
| X2 | Revision — registration-derived `queue_only` | Registration sets `queue_only=true` for a PR extractor step, preserves the publish condition, ignores an authored value; direct verify refuses it on cold load before any step; media extraction alone stays direct-eligible | server registration test; direct-verify refusal in the journey; media-only Verifier stays `queue_only=false` |
| X3 | Input and existing owner ¶1; INV-001 | The extractor reads only the queued activation's pins and tenant connection; a missing accepted source identity refuses before any acquisition; a caller supplies no credentials | engine/server unit test of a non-PR execution; source review: no credential field in the action |
| X4 | Agreed transformation ¶1; REQ-006 baseline; Revision — extracted-source baseline eligibility | Baseline is the last completed same-PR, same-policy execution whose pull_request source was extracted (`pr_source_extracted`, written with completion); a failed extraction never advances it, a failed or inconclusive judgment after a successful extraction does: P completed, A running, B admitted reviews P→B; first review, unavailable or rewritten history and changed policy run full with `first_review`, `history_unavailable`, `history_rewritten`, `policy_changed`; unreadable execution state is a retryable failure; PR base and merge base stay distinct from `comparison_base` | SQL history and single/batched completion tests; Tier 2 P/A/B, failed-extraction and fallback cases |
| X5 | Agreed transformation ¶1, ¶3; AC-004 | One call to task 004's public `ReviewPipeline::pinned` per extraction, through the connection's installation token; no Wyrd clone, index, rerank or filesystem discovery; a second consumer causes no second acquisition | Tier 2 provider call counts; source review |
| X6 | contracts table; Agreed transformation ¶2 | Closed typed output: `format`, `pr` metadata and identities, `review` mode, `comparison_base`, `previous`, `fallback_reason`, `full_pr`, active `changes` with paths, status, blob ids, hunks and before/after source or binary/unavailable/excluded identity, and the unchanged library `context`; an empty active diff is valid evidence, never a pass | spec schema test; Tier 2 output assertions incl. empty incremental diff |
| X7 | REQ-007; Agreed transformation ¶3 | Prompt inputs bind `pr`, active `changes` and `context`, with placeholders matching; the workspace mounts the same whole output as JSON; no source checkout and no implicit input to other Agents; with the persistence ACK held, dependents consume the live extraction | journey Agent sees exact source text in Prompt and workspace; held-ACK Tier 2 |
| X8 | REQ-007, REQ-017; Agreed transformation ¶3 | Recovery reuses the accepted extraction exactly, with source and context intact after library context expiry; unpersisted extraction may repeat | Tier 2 restart after cache expiry |
| X9 | REQ-007; AC-004 | Missing required source, omissions and model-input limit failures stay honest; none turn a missing or failed review into passed | Tier 2 failure cases |
| X10 | Revision — metadata batching | `CodeReviewMetadata` group-commits independent writes in short tenant-scoped transactions: concurrent writes share commits, a lone write commits at once, each reply follows its commit, failed batches fail their callers with existing semantics and no per-operation fallback, tenants never share a batch, pending work and workers are bounded, no transaction or lock spans GitHub/object IO; batch sizes recorded in tracing | `pg_code_review_cache` tests over real Postgres |
| X11 | Output, consumers and surfaces; AC-011; Revision — no client projection | Action schema with `extractor` in the regenerated wire types, stable errors; the extraction and `review.previous` are inspectable as ordinary step-output JSON through existing execution details on HTTP, MCP and CLI; no SDK typed read or authoring helper; focused input/workspace guide | `codegen:check`; inspection journey over existing details surfaces; docs review |
| X12 | Agreed transformation ¶4 | `wyrd-design.md` Verifier / Agents, tools and workspaces and `bifrost-design.md` Durability/Ingest state typed extraction and source custody | content review |
| X13 | Connected proof ¶3; INV-002 | New tests sit in existing targets; contract, dependency and tenant checks pass | `codegen:check`, `check:deps`, `check:tenant-isolation` |
