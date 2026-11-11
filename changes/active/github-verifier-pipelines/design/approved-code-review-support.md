# Approved code-review support

Status: approved design decisions; not an implementation-ready specification.

The user explicitly approved the preceding proposal and requested that these
decisions be locked. Follow-up API changes and structures not presented in that
proposal remain subject to approval before recording.

## Library output and retained evidence

Use the independent library's existing ReviewPipeline, not a duplicate Wyrd
indexing/ranking pipeline. Its PrContextResult contains record, lifetime and
context. ReviewContext carries pinned identity/PR/base/head/tree, selected spans,
omissions, not_analyzed, budget and coverage. Each selected span carries path,
blob ID, line range, exact source text, rank score, matched names and estimated
tokens. Rank score is not review severity or model confidence.

Retain PrContextResult.context unchanged in the extraction output, including
selected source text, not just context metadata. It can also be mounted as
context.json through an Agent workspace binding. Preserve identity/provenance
alongside extraction. Acquire once and share accepted evidence among reviewer
steps. Wyrd's accepted evidence must remain usable independently of expiration
of the library's published context record.

The library does not itself return review findings, remediations, the complete
changed-source bundle or full PR diff. Those responsibilities must be explicit
in the proposed extractor/library extension, never attributed to today's output.

## Model input and automatic behavior

- Executing extraction automatically acquires and retains its evidence, including
  context.json with the library's selected snippets.
- Agent workspace attachment requires its declared workspace mapping.
- Prompt injection requires declared inputs matching Prompt placeholders.
- Agent file reads enter the conversation through its tools.
- Retaining full evidence does not automatically inject it into every model call.

The standard review flow binds the extraction output or its fields directly to
Prompt placeholders and/or workspace files. It requires no separate input
preparation step or tool-driven file discovery. Optional formatting helpers may
ease authoring, but do not replace the library's ranking with another Wyrd
selector. Its context budget covers selected snippets; Wyrd remains responsible
for the whole model input, including diff/instructions. The subsequently approved
limit behavior below replaces the earlier open oversized-diff question.

### Approved context limits and instruction source

Reuse the library's bounded context and existing engine/model limits. Do not add
another selector, automatic splitting service or large-PR scheduler. Inject the
selected context and active changes as agreed. Report model-input limit failures
honestly through normal failure semantics, never as clean reviews. Authors may
split review through existing DAG/fanout primitives. The context budget does not
bound diff and Prompt overhead; no new context allocation subsystem is selected.

The pinned Verifier's Agent/Prompt owns policy. Supply repository guidance from
the pinned PR base revision. Changes to guidance in the PR are review evidence,
not an automatic policy replacement.

Keep pr/review/changes/context. Source anchors identify commit, path and line
range. Represent previous review/report through retained execution references.
Exact nested schemas remain to specify; no new evidence framework.

## Approved pull_request extraction contract

The user explicitly approved this contract after reviewing both the returned
data and its workspace/Prompt bindings. This supersedes the earlier metadata-only
output proposal. These are required additions, not claims of existing PR support.

For one pinned PR and Wyrd-selected comparison baseline, extraction returns one
structured step output with these fields:

| Field | Contents | Owner |
|---|---|---|
| format | pull_request discriminator | Wyrd extraction contract |
| pr | Repository identity, PR number, URL, title, body, base and head commits | GitHub acquisition through the independent library |
| review | Full/incremental mode, comparison commit and fallback reason when applicable | Wyrd selects the baseline |
| changes | Active-comparison changed-file paths, change kinds, diff hunks and source-line coordinates | Independent library extension |
| context | Existing PrContextResult.context unchanged, including selected source snippets, provenance, omissions, budget and coverage | Existing library ReviewContext; baseline-aware pipeline extension required |

First review uses PR merge base → current head. Incremental review uses the last
completed usable reviewed head → current head. Supporting context is selected
for that active comparison by the independent library. Extraction produces no
review findings, severity, fixes or verdict; those belong to the reviewing DAG.

The output and each of its fields are ordinary execution-scope JSON values.
They can feed either or both existing AgentCall.inputs and AgentCall.workspace:

```yaml
inputs:
  pr: steps.extract-pr.output.pr
  changes: steps.extract-pr.output.changes
  context: steps.extract-pr.output.context

workspace:
  /inputs/pr.json: steps.extract-pr.output
```

The Prompt declares matching pr, changes and context placeholders. Their values
are rendered and injected directly. The declared workspace mapping materializes
the same accepted output as a JSON file for tools. Authors may instead mount
individual fields, including context as /inputs/context.json. Both mechanisms
can be used together without re-extraction. Workspace JSON bindings do not
implicitly expand into a source tree. No special publications.evidence name or
mandatory directory layout is introduced.

This locks the field responsibilities and consumption mechanism, not unseen
nested wire schemas. The earlier agreement to retain complete current PR
evidence and provide previous completed-review evidence still applies; their
exact representation remains to specify. The approved context-limit rule above
applies when active changes exceed the model-input budget.

## Successive commits and pipeline ownership

The user approved and requested locking these follow-up decisions:

- Wyrd owns supersession/cancellation and selection of the last completed usable
  review under the same policy, because Wyrd owns execution and review history.
- The independent library's existing ReviewPipeline owns source acquisition,
  comparison, indexing, changed-line analysis and supporting-context selection.
  Extend that public facade to accept the pinned PR identity and an optional
  comparison baseline; do not create a parallel Wyrd acquisition/ranking pipeline.
- The Wyrd input helper formats/binds returned evidence into bounded model input.
  It does not refetch source, build another index or rerank supporting files.
- Keep the actual PR base/merge base distinct from the review comparison baseline.
  First review or unusable baseline uses PR merge base → current head. Incremental
  review uses last completed usable reviewed head → current head. Source pins and
  the chosen baseline remain fixed for that logical review's retry/recovery.
- The library extension returns complete current PR evidence and the active
  review diff, with supporting context selected for that active diff. Keep full
  evidence available; normal reviewer input focuses on active changes and selected
  snippets. Today's public context call accepts a PR number and does not yet
  implement this baseline-aware API. Exact names/types remain specification work.

If A is still running when B arrives, request cancellation of A and queue B.
A's partial work never advances the review baseline. Use the last completed
usable review before A; without one, B receives a full PR review. For example:

```text
P: completed review
A: still running
B: pushed

Cancel A; review P → B.
Without P, review PR merge base → B.
```

If C arrives before B starts, skip B and review C from the same completed
baseline. Preserve accepted old evidence and completed history. Cancellation may
take time, but a superseded execution cannot publish as the current review.
An already sent remote request can still finish; do not promise instantaneous
remote interruption. Do not reuse partial findings as if they were a completed
review or inherit an old approval for a new head.

## Optional standard review findings

Standard severities: critical, high, medium, low. Confidence is separate.
The standard finding carries:

- id;
- severity;
- confidence;
- title;
- explanation/impact;
- source locations and supporting evidence;
- remediation explanation when available;
- suggested replacement when available.

Do not require the reviewer to invent a fix. Validate locations and suggested
replacements against retained pinned evidence. Exact serialization, bounds,
missing-value representation and matching rules require later specification.
The standard is opt-in; users can retain custom reviewer outputs and translate
them into the standard through normal Agent/code steps.

## Wyrd helper and engine support

Provide input preparation, typed report schemas, validation of locations and
suggested replacements, aggregation/sorting/identity-preservation utilities,
requirement helpers for authored severity thresholds, and consistent GitHub
rendering of severity, explanation and remediation.

Use the existing verification engine's declared structured-output and publication
mechanisms to validate and retain reports. Review strategy stays in the DAG;
there is no parallel code-review scheduler or engine. A severity threshold yields
a normal Wyrd verdict; severity does not replace passed/failed/inconclusive.
Concrete new engine behavior and helper surfaces must be designed explicitly.

The user subsequently approved [standard report delivery](approved-report-delivery.md):
the final reporting step returns the code_review envelope, recognized from
accepted step outputs without a Verifier/binding selector. Intermediate reviewers
reuse Finding schemas; multiple final envelopes produce an ambiguity error.

GitHub annotation levels (failure/warning/notice) are a delivery projection, not
our stored severity vocabulary. Preserve the original severity in the report.
The user subsequently approved both Check output and PR review comments,
including suggested edits, in [runtime and GitHub decisions](approved-runtime-and-github.md).
Implement the required provider protocol/permissions; do not turn suggested
edits into automatic source commits.

## Task 004 storage amendment

The user directed this correction on 2026-11-11. Task 004 implements the
library's storage protocols over Wyrd's existing PostgreSQL and object-store
deployment. Its required metadata tables are an explicit exception to the
no-new-PostgreSQL-tables constraint, in addition to Task 002's installation table.
No new database service or cache-coordination service is selected.

The blob trait is `ObjectStorage`, distinct from `object_store::ObjectStore`.
`ReviewPipeline::new` takes that adapter and one metadata owner implementing
both `SnapshotStore` and `ContextStore`.

| Implementation | Persistence and required behavior |
|---|---|
| `ObjectStorage` | Wrap Wyrd's configured `object_store` backend. Store immutable source-free snapshots and contexts containing selected source spans. Preserve library-scoped keys, atomic create-only writes (`AlreadyExists`), exact reads (`NotFound`), idempotent deletion and backend failures. |
| `SnapshotStore` | PostgreSQL snapshot pointers and records. Preserve version/format/size scope, tree-based compare-and-swap, insert-if-absent cold publication, record touches, grace periods, atomic collection claims against upserts/pointer publication, and retryable idempotent cleanup. |
| `ContextStore` | PostgreSQL context records and publication/expiry metadata. Repeated upsert preserves original timestamps. Lookup distinguishes absent, expired and live; bounded expiry enumeration and repository removal support object cleanup. |

The README's three reference tables are `bcr_snapshot_pointers`,
`bcr_snapshot_records` and `bcr_contexts`. Task 004 owns equivalent Wyrd metadata
tables and migrations through `wyrd-sql`; exact Wyrd names and private adapter
placement remain implementation-owned. PostgreSQL holds pointers, records and
lifecycle metadata, never snapshot/context object bytes or execution payloads.

Scope metadata by tenant, installation and stable numeric repository ID, with
snapshot compatibility fields where required. Owner/name are labels; renames
preserve metadata. Adapt the README's raw-pool examples to Wyrd's `TenantConn`
and forced tenant RLS, with a narrow `OperatorPool` capability for cross-tenant
expiry. Use PostgreSQL `statement_timestamp()` for lifecycle timestamps and
bind durations for expiry/grace deadlines. Never hold a transaction across
GitHub or object-store IO. Follow the [SQL owner rules](../../../../crates/wyrd/wyrd-sql/AGENTS.md).

The flow is authorized identity and source pins → configured blob adapter plus
PostgreSQL metadata adapter → public `ReviewPipeline` → returned source/context
→ Task 005's accepted Bifrost evidence. Library metadata commits and blob
publication satisfy library protocols, not Wyrd execution acceptance.
Bifrost/Scribe still owns accepted execution evidence, decisions and checkpoints;
cache expiry must not remove that evidence. Do not substitute Bifrost/Scribe for
these PostgreSQL traits or fabricate execution IDs for library cache records.

Proof uses the actual pipeline with real Wyrd storage/PostgreSQL adapters:
cold publication and warm reuse, competing pointer publications, collection
versus upsert/publication, interrupted cleanup/retry, non-extending context
lifetimes, absent/expired/backend-failure distinctions, identity isolation and
rename continuity. Task 005 proves retained execution evidence survives cache
expiry. No runtime verification is claimed by this amendment.

Evidence: [library README](/home/thorrester/Documents/GitHub/bohmian-code-review/README.md),
“Implement ObjectStorage” and “Implement the metadata store (Postgres)”, plus
[persistence traits](/home/thorrester/Documents/GitHub/bohmian-code-review/crates/bohmian-code-review-pipeline/src/persistence.rs),
inspected at `d8c772de4248ce0130f79ec3b5fefda518b1431a`. This research revision is
not an approved dependency pin.

## Sources inspected

- bohmian-code-review-pipeline/src/context.rs::ReviewContext/SelectedSpan,
  pull_request.rs::PrContextResult and PrContextLimits, review.rs::ReviewPipeline.
- vala-verify/src/blocks/agent.rs::AgentExecutor::bind: explicit Prompt bindings.
- mitari-platform/app/worker/pr_v4_scan.py and utils/finding_presentation.py:
  severity, rationale, proposed fixes and GitHub projection.
- [Devin](https://docs.devin.ai/work-with-devin/devin-review),
  [CodeRabbit](https://docs.coderabbit.ai/guides/code-review-overview),
  [Greptile](https://www.greptile.com/docs/code-review/first-pr-review).

No production implementation or runtime verification is claimed by this record.
