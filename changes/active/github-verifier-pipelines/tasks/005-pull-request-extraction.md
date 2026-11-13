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
the existing execution details on HTTP and MCP, where the step output is
ordinary JSON; this task adds no inspection-specific type or command. The CLI
has no execution-details read (`wyrd execution` reads status, invocations and
publications), so CLI inspection of step output is not part of this task;
AC-011's CLI wording for the extraction routes to the spec owner.

The output shape is server-owned. `wyrd-spec` holds only what authors write:
the `extractor` field of the `extract` action and its closed `Extractor`
values. The extraction's output types live with the server's extractor, and
client-side Verifier validation knows no output schema for
`extractor: pull_request`, exactly as for code steps: a reference path into
`steps.<id>.output` is not type-checked at authoring or declaration and
resolves at execution.

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
| X2 | Revision — registration-derived `queue_only` | Registration sets `queue_only=true` for a PR extractor step, preserves the publish condition, ignores an authored value; direct verify refuses it before any step, reusing the registered flag without request-time scans or repeated work on cache hits; media extraction alone stays direct-eligible | server registration test; direct-verify refusal in the journey; source review of the direct eligibility rule |
| X3 | Input and existing owner ¶1; INV-001 | The extractor reads only the queued activation's pins and tenant connection; a missing accepted source identity refuses before any acquisition; a caller supplies no credentials | engine/server unit test of a non-PR execution; source review: no credential field in the action |
| X4 | Agreed transformation ¶1; REQ-006 baseline; Revision — extracted-source baseline eligibility | Baseline is the last completed same-PR, same-policy execution whose pull_request source was extracted (`pr_source_extracted`, recorded with the lease-fenced completion, single and batched alike, preserved on identical replay, never on stale or rolled-back completion); `policy_changed` likewise requires an extracted review; a failed extraction never advances it, a failed or inconclusive judgment after a successful extraction does: P completed, A running, B admitted reviews P→B; first review, unavailable or rewritten history and changed policy run full with `first_review`, `history_unavailable`, `history_rewritten`, `policy_changed`; unreadable execution state is a retryable failure, never a first review; PR base and merge base stay distinct from `comparison_base` | SQL history and single/batched completion tests; Tier 2 P/A/B, failed-extraction, fallback and unreadable-history cases |
| X5 | Agreed transformation ¶1, ¶3; AC-004 | Extraction acquires once through task 004's public pipeline under the connection's installation; no Wyrd clone, index, rerank or filesystem discovery; settling acquires nothing again | Tier 2 provider read counts; source review |
| X6 | contracts table; Agreed transformation ¶2 | Closed typed output: `format`, `pr` metadata and identities, `review` mode, `comparison_base`, `previous`, `fallback_reason`, `full_pr`, active `changes` with paths, status, blob ids, hunks and before/after source or binary/unavailable/excluded identity, and the unchanged library `context`; an empty active diff is valid evidence, never a pass | spec schema test; Tier 2 output assertions incl. empty incremental diff |
| X7 | REQ-007; Agreed transformation ¶3 | Prompt inputs bind `pr`, active `changes` and `context` directly, with placeholders matching and no prep stage; the workspace mounts the same whole output as JSON and nothing else (no source checkout); an Agent that binds nothing receives none of it; every consumer shares one acquisition; policy is the pinned Agent/Prompt, and an edited repository file is before/after evidence, never policy; with the persistence ACK held, dependents consume the live extraction | journey Agent sees exact source text in Prompt and workspace, lists `/inputs`, bystander Agent; held-ACK Tier 2 |
| X8 | REQ-007 (persisted evidence per REQ-017); Agreed transformation ¶3; Revision — extracted-source baseline eligibility | Recovery reuses the accepted extraction exactly, with source and context intact after library context expiry, and still records the extracted-source fact without reacquisition; unpersisted extraction may repeat | Tier 2 restart after cache expiry |
| X9 | REQ-007; AC-004 | Missing required source, omissions and model-input limit failures stay honest; none turn a missing or failed review into passed | Tier 2 failure cases |
| X10 | Revision — metadata batching | `CodeReviewMetadata` group-commits independent writes per tenant: concurrent writes share commits, a lone write commits at once, each reply follows its commit, a failed batch fails each caller and applies none of its writes, tenants never share a batch, pending work and workers are bounded, no transaction spans GitHub/object IO; the actual pipeline's pointer races, context lifetime and cleanup still hold over the batched owner | `pg_code_review_cache` tests over real Postgres and object storage; source review of bounds and transaction scope |
| X11 | REQ-016; Output, consumers and surfaces; AC-011; Revision — no client projection | Action field `extractor` in the regenerated wire types and stable validation errors; `wyrd-spec` holds only the authored field and knows no output schema; the extraction and `review.previous` are inspectable as ordinary step-output JSON through the existing execution details on HTTP and MCP (no CLI details read exists; see the revision); no SDK typed read or authoring helper; focused input/workspace guide | `codegen:check`; spec binding test; inspection over HTTP and MCP details in the journey; docs review |
| X12 | Agreed transformation ¶4 | `wyrd-design.md` Verifier / Agents, tools and workspaces and `bifrost-design.md` Durability/Ingest state typed extraction and source custody | content review |
| X13 | Connected proof ¶3; INV-002 | New tests sit in existing targets; dependency and tenant checks pass | `check:deps`, `check:tenant-isolation` |

## Evidence

`J` = `crates/wyrd/wyrd-server/tests/integration/pg_github_pull_request_extraction.rs`;
`Q` = `crates/wyrd/wyrd-sql/tests/integration/pg_verifier_executions.rs`;
`C` = `crates/wyrd/wyrd-server/tests/integration/pg_code_review_cache.rs`.

| Obligation and required cases | Implementation evidence | Proof location | Verification evidence | Result |
|---|---|---|---|---|
| X1: exactly one of `from`/`extractor`; neither or both refused; media extract keeps formats, output and failures | `wyrd-spec/src/card/verifier/validate.rs:492` (`extract_source`); `vala-verify/src/blocks/engine.rs:1951` binds `from` only when present | `validate.rs:1680` `verifier_spec_refuses_invalid_graphs_and_references` (`extractor` beside `from`; `from: null`); over HTTP `pg_card_declaration_route.rs:1734` (both and neither: 400 `WYRD_SPEC_400_VALIDATION`, rule `extract_source`, nothing written); media extract over the changed binding: `engine/tests.rs:1022` and the full vala-verify lib suite, `pg_hosted_files.rs:199` `extract_action_returns_locked_shape` | spec 24 passed; vala-verify lib 77 passed; server integration passed. A PR-admitted execution has no input slots, so the legacy media step runs in its own journeys rather than in `J` | PASS |
| X2: registration derivation; authored value ignored; publish condition kept; media-only direct; direct refusal before any step from the registered flag | `cards/service.rs:1933` `queue_only` (once, at registration); `verification/service.rs:162` `direct_eligibility` reads `spec.queue_only` only on a direct cache miss, pure | `pg_card_declaration_route.rs:1734` (PR form with and without an authored `queue_only: false` stores `true`; media-only stores none; publishing Agent); `J:975` (422 `INPUT_UNSUPPORTED`, no repository read) | server integration passed; source review (no scan on the request path) | PASS |
| X3: only the queued activation's pins and tenant connection; no accepted source refuses before acquisition; no caller credentials | `hosted/blocks.rs` `ExecutionBinding::pull_request` (terminal without a `PullRequestSource`); `extraction.rs:73` reads only the execution's tenant and source; the action has no credential field | `J:1043` (enqueued, not admitted: `pr` failed with the refusal, inconclusive, no repository read); every `J` journey reads only the admitted PR's pins through the tenant's installation | server integration passed; source review | PASS |
| X4: extracted-source baseline; single/batched/replay/stale/rollback; `policy_changed` requires extraction; failed extraction never advances, failed judgment does; P/A/B; four fallbacks; unreadable history retryable, never a first review; bases distinct | migration `20261115000000_pull_request_source_extracted.sql`; `pull_requests.rs:203`; `engines.rs:99`; `runner.rs` `Transition::Complete`; `extraction.rs:150` | `Q:4628` (latest extracted, P/A/B, `policy_changed`, and `FirstReview` once the other version is unextracted); `Q:4781`; `Q:4853` (single and batched parity, identical replay of both keeps each fact); `Q:4913` (stale lease and rollback, single and batched; completion after cancellation is fenced single and batched and records nothing); `Q:5014`; `J:541`, `J:808`, `J:830`, `J:846`, `J:907`, `J:940`, `J:1500`; `J:1120` (column withheld: two attempts, `execution history is unavailable now`, no output, no comparison read, not eligible) | SQL lane passed; server integration passed. RED for `J:1120` by mutation: mapping the read failure to `FirstReview` made it pass with one attempt and a full review | PASS |
| X5: one acquisition through task 004's pipeline under the installation; no clone/index/rerank/discovery; settling acquires nothing again | `extraction.rs:73-122` (one `review()`; `installation_access`; no filesystem) | `J:690` (one `/compare/` read for every consumer); `J:1246` (settling acquires nothing again) | server integration passed; source review | PASS |
| X6: closed typed output; empty active diff evidence, never a pass | `components/github/extraction/output.rs` (`deny_unknown_fields`) | `output.rs:299` round trip and unknown-field refusal; field values in `J:541`; library `context` in `J:1716` (omissions, selected spans, budget accounting exactly for them, coverage carried); `J:873`; `J:1541` (an added image is `binary`, a file over the 1 MiB ceiling is `unavailable` with `limit_bytes` and its blob is never fetched, `vendor/` and a symlink are `excluded` with `vendored_segment` and `symlink`; each carries its blob, no `before` and no hunks; the extraction succeeds and the review passes) | server lib passed; server integration passed. `J:1541` bites by mutation: projecting binary as excluded fails it | PASS |
| X7: direct Prompt binding, no prep stage; workspace holds only the extraction; bystander receives none; one acquisition; Card policy, edited files as evidence; live consumption under held ACK | engine bindings; `blocks.rs` extractor route; Wyrd reads no repository guidance | `J:690` (exact source in Prompt and `/inputs/pr.json`; `fs.list /inputs` is exactly `/inputs/pr.json`; bystander's request carries no source, path, head or `pr.json` and it has no workspace; Prompt is the Card's); `J:541` (edited file carries base `before` and head `after`); `J:1541` (an edited `AGENTS.md` telling reviewers to approve everything is carried as base `before` and head `after` text, and the outcome judges only the Card's `changes` requirement); `J:1246` | server integration passed | PASS |
| X8: recovery reuses accepted extraction after library context expiry | ordinary step output in accepted revisions | `J:1343` (no repository read after restart; recovered Prompt and mount equal the retained extraction; `pr_source_extracted` true after recovery) | server integration passed | PASS |
| X9: missing source, omissions, model input limit never pass | existing step, requirement and model limits | `J:1684`, `J:1716`, `J:1818` | server integration passed | PASS |
| X10: group commit per tenant; lone write immediate; reply after commit; failed batch applies none; bounded; no IO in transactions; pipeline still correct over the batched owner | `review_cache.rs:142-152` (`WORKERS`, `BATCH`, bounded `PENDING` channel), `:342-357` one transaction per batch with no IO, `:346` span field `batch` | `C:820`, `C:847`, `C:913`; the actual pipeline over the batched owner with real Postgres and object storage: `C:298`, `C:376`, `C:436`, `C:543`, `C:664` | server integration passed; source review for bounds and transaction scope | PASS |
| X11: `extractor` in wire types; stable errors; no `wyrd-spec` output schema; HTTP and MCP inspection; no SDK projection; guide | `card-types.ts`; `validate.rs` `output_schema` returns `None`; `reference/verifier-card.md`, `declare-a-repository.md` | `validate.rs:1844` `pull_request_extraction_output_binds_without_a_client_schema`; HTTP `J:443`; MCP `J:461` `mcp_details`, used in `J:541` (`verification.get_execution_details` over `/mcp` serves the same extraction and `review.previous`) | `codegen:check` 0; `docs:check` 0; content review | PASS |
| X12: design authorities | `wyrd-design.md` action table, references paragraph, Agents/tools/workspaces paragraph; `bifrost-design.md` checkpoint custody bullet | commit 9d875a178 | content review | PASS |
| X13: tests in existing targets; dependency and tenant checks; INV-001 baseline never crosses tenants | new tests only in existing `integration` targets and inline modules; `BASELINE_SQL` runs on `TenantConn` under RLS | `Q:4731` (tenant B, same repository id, PR and Verifier name, reads `FirstReview` after tenant A completed an extracted review) | `check:deps` 0, `check:tenant-isolation` 0; SQL lane passed. `Q:4731` bites by mutation: with RLS disabled on executions and cards it reads `PolicyChanged` | PASS |

Commands: `mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(/verifier::/)'`;
`-p vala-verify --lib`; `-p wyrd-server --lib -E 'test(/extraction|engines/)'`;
under `scripts/postgres/with-test-postgres.sh`:
`-p wyrd-server --test integration -E "test(/pg_github_pull_request_extraction|pg_code_review_cache|pg_github_pull_requests|pg_hosted_files|registration_derives_queue_only/)"`
and `-p wyrd-sql --test integration -E "test(/pg_verifier_executions/)"`;
`cargo clippy --locked -p wyrd-server -p wyrd-sql --tests -- -D warnings`; `git diff --check`.

Process exceptions: spec types and metadata batching were written before their
tests; the server extractor (e4e6c3f79) was committed without RED. These
journeys passed on first run because the behavior already existed: the direct
refusal, held ACK, P/A/B, non-admitted, MCP, workspace listing, bystander and
X9 journeys, the SQL `policy_changed` and replay additions, the
unreviewable-file journey `J:1541`, and the cross-tenant baseline `Q:4731`;
the last two were proven to bite by mutation. RED was proven
for the extracted-source baseline (removing the predicate made `J:907` fall
back with `history_unavailable`) and, by mutation, for the unreadable-history
journey.

Mechanical deviations: the Tier 2 journeys live in the `wyrd-server`
integration target instead of `wyrd-testing` via
`mise run test:bifrost:journey:server`. HTTP and MCP inspection is proven over
`/mcp` in that target.

Open contract item (spec owner): REQ-016 and AC-011 still name typed
Rust/Python/TypeScript and CLI inspection, plus Tier 1 SDK runner journeys.
The revisions above drop the client projection and the CLI read, by decision,
so no SDK or CLI surface exists to journey. The spec text needs the same
revision.

Limits: recovered step output serializes object keys sorted, while the live
value keeps struct order. The values are equal, and `J:1343` compares by
value. This is outside the task's write set.

Non-goals: no SDK typed read, builder or CLI command, and no `wyrd-spec`
feature gating (a separate design change).
