---
id: TASK-009
status: proposed
approved_spec_revision: 5
requirements: [REQ-005, REQ-012, REQ-013, REQ-014, REQ-016]
acceptance: [AC-008, AC-009, AC-011]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007]
depends_on: [TASK-002, TASK-005, TASK-006, TASK-007, TASK-008]
---

# Render retained reports and recover GitHub delivery

Implement **approved-report-delivery.md → Output convention / GitHub consumption /
Subsequently approved presentation defaults**, **approved-code-review-support.md
→ Optional standard review findings / Wyrd helper and engine support**, and
**approved-runtime-and-github.md → GitHub results**. The standard-report worked
example is the selected complete authoring flow; the three other worked examples
illustrate existing repeat/fanout/speculation, not extra review strategies to ship.

## Actual inputs and report production

Task 006 supplies exact accepted binding/execution/source generation; task 005
supplies retained source/diff and unchanged library context; task 002 supplies
current access and installation tokens. Tasks 007/008 supply nonterminal waiting
reasons and accepted continuation progress. Existing engine accepted output/state
and result assembly supply the verdict and successful logical step outputs; do
not create a report/result mirror or recompute them during delivery.

Add the exact CodeReviewReport/Finding/SourceAnchor schema from contracts to the
pure `wyrd-spec` owner. Extend the existing named Prompt response-format resolver
(currently `verdict`) with `code_review`. AgentExecutor already returns normal
schema-validated JSON step output; normal code may emit the same envelope.
Intermediate reviewers may reuse Finding schemas in ordinary outputs; a normal
consolidator emits the final envelope. Supply the approved optional validation,
aggregation/sorting, ID-preservation and authored severity-threshold helpers
through existing authoring/helper owners, using ordinary report outputs/checks.
No mandatory prep step, report selector, new Verifier kind or review engine.

Select per execution from accepted successful final logical outputs: retained
fanout items each count; only final retained repeat output counts; failed attempts
and discarded speculation do not. Zero envelopes gives ordinary summary, one
renders it, multiple is explicit ambiguity even if a consolidator consumed the
earlier envelope. Unsupported/malformed envelopes are visible delivery errors.
Report selection/errors never change an accepted verdict. Keep report under
ordinary execution/provider output budgets, separate from 64-KiB Verdict.data;
never truncate a large report into a clean review.

## Exact-source rendering

Validate every supplied anchor against task 005's retained canonical relative
path/commit/inclusive line range and source. Suggestions replace precisely that
range with supplied replacement. Invalid edit/source content yields safe visible
delivery error, never invented line 1 or a modified finding. Empty location arrays
stay summary-only. Preserve ID, severity and confidence independently; map
critical/high→failure annotation, medium→warning, low→notice.

Use a stable Check name per binding, external_id for exact execution and reviewed
SHA. Map passed→success, failed→failure, inconclusive→action_required,
cancelled→cancelled, timed_out→timed_out, errored→failure. Waiting evidence or
approval remains in_progress with explanation. Publish review as COMMENT pinned
to reviewed commit, with valid inline findings and suggestions. Reuse comments by
retained finding/provider identity where possible; moved anchors preserve history
and link replacement rather than silently relocating old comments. Absence from
an incremental report does not auto-resolve threads. No APPROVE, REQUEST_CHANGES,
merge decision, source change or autofix is authorized.

## Existing delivery workflow and consequential handoffs

Current `runner.rs::DeliveryClaim` and SQL `VerifierExecutionQueue` coordinate
owed terminal **Bifrost** result delivery; they are not GitHub publishing. Extend
that existing runner/queue delivery phase to claim provider progress independently
of computation, preserving canonical result delivery. Keep report/source/progress
in canonical Bifrost verification state and provider IDs/attempt/readiness/
sanitized failure coordination in existing queue/control owners without storing
report payloads in SQL. No finalizer service, second payload store or table.

Retain enough accepted progress for every consequential Check/review/comment/
annotation batch effect to be reconciled after timeout/restart. Stable IDs and
provider readback must distinguish remote success from absence before retrying
an unknown create/append; annotation requests respect the provider limit (50)
and append semantics. Never blindly append again. If readback cannot establish
outcome, expose unresolved/ambiguous delivery rather than claim success. Respect
provider cooldown, bound retry and expose permanent failure. Recheck task 002's
current access and task 006's source generation at **each** remote write; a
provider request already sent may finish and must be reconciled with its pins.
Superseded work never claims current-result delivery or starts a continuation.

Provider retry reads retained outputs/progress and consumes no new verification
attempt, approval or model computation. Record acknowledged provider progress
before retiring its owed queue delivery; recovery rediscovers the remaining
obligation. Execution inspection exposes delivery status/error separately from
verdict, through existing read authorization, even after computation completes.
Amend **wyrd-design.md → Prompt / Verifier / queued delivery** and appropriate
**bifrost-design.md → canonical evidence / Durability and visibility**; delivery
policy remains Wyrd-owned, generic HTTP provider calls may reuse the library.

## Surfaces and connected proof

Ship report/schema/helper authoring, typed delivery inspection and safe errors in
shared Rust client, all SDKs, CLI and HTTP/MCP here. Update the standard-report
example into a runnable public story with actual declaration→queued extraction→
Prompt/workspace→normal requirement→Check/comment; docs explain optional reports,
severity versus verdict, waiting, ambiguous delivery and retry diagnosis. Earlier
tasks already own their introduced operations and docs; this is not deferred
surface implementation for the whole change.

**AC-008:** Through actual task 002 connection, 003 declared policy, 006 admission
and 005 extraction, run ordinary Agent/code outputs for zero/one/multiple/
malformed reports; retained fanout/repeat and discarded speculation exercise real
engine output selection. Assert provider Check/conclusion plus every severity,
unlocated summary, valid suggestion and invalid exact-source edit; authored
requirements alone determine verdict. One successful model output or empty
findings never proves an otherwise incomplete review passed. Helpers preserve
finding IDs through authored aggregation and threshold checks.

**AC-009:** On that same completed/waiting review, have local provider transport
commit a remote write then time out; interrupt annotation batches and restart.
Inspect reconciled IDs/comments/batches and independent delivery state, with no
review recomputation or duplicate append. Push a new head through 006 and remove
access through 002 during delivery; no later stale write is authorized, and
already-sent effects remain pinned/visibly reconciled. Also exercise cooldown,
permanent failure and ambiguous readback without pretending completion.

**AC-011 (complete journey):** Fresh reader follows the combined CLI/SDK guides
from App setup to declare, PR admission, deferred publish, approval and result/
delivery inspection, without UI or private manual orchestration. Use predecessor
receipts/accepted states at every seam, not seeded report/activation fixtures.

Tier 1: matched Rust/Python/TypeScript stories and `wyrd-mcp` journeys with real
WyrdTestServer/Postgres and local protocol/model transports, using README runners;
server verification runtime journeys cover restart. Tier 2: queue claims/owed
progress and canonical reads against real dependencies; tier 3: schema, selection,
anchor/rendering and helper branches. Record actual new focused commands when
tests exist. Apply README surface/contract/type/boundary checks; integrated change
review owns full shipped-surface lanes and required broad aggregate. Planning
neither runs tests nor promises exactly-once remote HTTP effects.
