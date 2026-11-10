---
id: TASK-005
status: proposed
approved_spec_revision: 3
requirements: [REQ-006, REQ-007, REQ-016]
acceptance: [AC-004, AC-011]
invariants: [INV-001, INV-002, INV-004, INV-005, INV-007]
depends_on: [TASK-001, TASK-004, TASK-006]
---

# Extract accepted PR source into ordinary evidence

Carry **approved-code-review-support.md → Approved pull_request extraction
contract / Model input and automatic behavior / Successive commits and pipeline
ownership** into **contracts.md → Pull request extraction**. Although numbered
005, this task depends on 006's admitted source; it must not manufacture an
alternate manual source-injection endpoint to avoid that dependency.

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
semantics. Attach the new form to task 001's pre-execution queued-only check,
including manual direct invocation; never execute a prefix then refuse.

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

Accept extraction through existing `RunState`/checkpoint/Scribe evidence custody
before dependent work consumes it. Recovery reuses the exact accepted output;
library context-record lifetime cannot invalidate retained Wyrd evidence. Normal
Agent inputs may select the whole output or fields and Prompt placeholders must
match. Existing workspace JSON mapping materializes the same accepted values;
it is neither a source-tree checkout nor implicit input to unrelated Agents.
Repository guidance comes from pinned base source; edited guidance is evidence,
while exact Agent/Prompt owns policy. Use existing model limits; an oversized
input fails honestly, with no automatic splitting service or clean-review claim.
Optional formatting uses ordinary input binding, without mandatory prep stage.

Amend **wyrd-design.md → Verifier / Agents, tools, and workspaces** and applicable
**bifrost-design.md → Durability and visibility / Ingest** for typed extraction
and source custody; preserve existing media extraction and checkpoints.

## Output, consumers and surfaces

Dependent Agent/code steps consume the accepted output through ordinary refs.
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
checkout and no second acquisition for another consumer. Recover after cache
expiry and observe the retained accepted evidence. Drive full/incremental and
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
