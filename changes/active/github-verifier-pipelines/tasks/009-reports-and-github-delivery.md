---
id: TASK-009
status: proposed
approved_spec_revision: 10
requirements: [REQ-005, REQ-012, REQ-013, REQ-014, REQ-016]
acceptance: [AC-008, AC-009, AC-011]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007]
depends_on: [TASK-002, TASK-005, TASK-006, TASK-007, TASK-008, TASK-010]
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
consolidator emits the final envelope. Severity thresholds and aggregation stay ordinary authored outputs/checks (spec
revision 9 drops the helper library). No mandatory prep step, report selector,
new Verifier kind or review engine.

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
range with supplied replacement. Invalid edit/source content yields a safe visible
delivery error, never invented line 1 or a modified finding. Empty location arrays
stay summary-only. Show ID, severity and confidence with each finding.

One Check per execution: stable name per binding, external_id = execution,
head_sha = reviewed commit. Map passed→success, failed→failure,
inconclusive→action_required, cancelled→cancelled, timed_out→timed_out,
errored→failure. Waiting evidence or approval stays in_progress with explanation;
a completed Check is never set back to in_progress. No Check annotations. One
`COMMENT` review pinned to the reviewed commit carries the execution marker and
the valid inline findings and suggestions. No auto-resolve, APPROVE,
REQUEST_CHANGES, merge decision, source change or autofix.

## Delivery (spec revision 9)

GitHub REST has no idempotency key, so delivery is lookup-before-create: find the
Check by name/head/external_id and the review by its execution marker; update the
Check in place, skip an existing review, create only what is absent. After a
provider timeout, look up again before creating. A rare duplicate after a lost
answer is accepted. Respect Retry-After/rate-limit cooldown and bound retries.
Before each write, recheck task 002's current access and task 006's current
source generation; a superseded or access-removed execution is recorded as
skipped and never writes again.

Delivery runs in the background after settlement or park, reads only retained
accepted outputs (`select_report`) and retained source, and never recomputes
verification or blocks the verdict. One outcome per execution is recorded in
Bifrost; on restart, delivery re-runs for PR executions without one. No progress
ledger, new SQL table or column, result mirror or finalizer service. Execution
inspection exposes the outcome and safe error separately from the verdict,
through existing read authorization. Amend **wyrd-design.md → Prompt / Verifier /
queued delivery** and **bifrost-design.md → canonical evidence** accordingly.

## Surfaces and connected proof

Ship report schema authoring here. The delivery outcome and safe error are
read through existing execution inspection over HTTP/MCP only; spec revision 10
drops its Rust client, SDK and CLI projection. Update the standard-report
example into a runnable public story with actual declaration→queued extraction→
Prompt/workspace→normal requirement→Check/comment; docs explain optional reports,
severity versus verdict, waiting, delivery outcome and retry diagnosis. Earlier
tasks already own their introduced operations and docs; this is not deferred
surface implementation for the whole change.

**AC-008:** Through actual task 002 connection, 003 declared policy, 006 admission
and 005 extraction, run ordinary Agent/code outputs for zero/one/multiple/
malformed reports; retained fanout/repeat and discarded speculation exercise real
engine output selection. Assert provider Check/conclusion plus every severity,
unlocated summary, valid suggestion and invalid exact-source edit; authored
requirements alone determine verdict. One successful model output or empty
findings never proves an otherwise incomplete review passed.

**AC-009:** On that same completed/waiting review, have local provider transport
commit a Check/review write then time out: the lookup finds it and nothing is
created twice. Restart before an outcome is recorded: delivery runs again from the
retained report without recomputation and finds what already landed. Push a new
head through 006 and remove access through 002 during delivery: no later write,
outcome skipped. Rate-limit cooldown is honored; permanent refusal is a visible
failed outcome.

**AC-011 (complete journey):** Fresh reader follows the combined CLI/SDK guides
from App setup to declare, PR admission, deferred publish, approval and result/
delivery inspection, without UI or private manual orchestration. Use predecessor
receipts/accepted states at every seam, not seeded report/activation fixtures.

Tier 1: matched Rust/Python/TypeScript stories and `wyrd-mcp` journeys with real
WyrdTestServer/Postgres and local protocol/model transports, using README runners;
the local GitHub is the existing `wyrd-testing` `GithubAppMock`, extended with
Check run and pull-request review endpoints with readback, and scripted faults:
commit a write then lose its answer, rate-limit cooldown and permanent refusal;
server verification runtime journeys cover restart. Tier 2: delivery outcome
record and canonical reads against real dependencies; tier 3: schema, selection
and anchor/rendering branches. Record actual new focused commands when
tests exist. Apply README surface/contract/type/boundary checks; integrated change
review owns full shipped-surface lanes and required broad aggregate. Exactly-once
remote HTTP effects are not promised (spec revision 9).

## Obligations

Contract: this task, spec REQ-005/012/013/014/016, AC-008/009/011, INV-001–007,
contracts → Standard code review report / Errors, inspection and delivery.

| ID | Promise and required cases | Planned proof |
|---|---|---|
| REPORT (REQ-012, AC-008) | Exact closed report schema in `wyrd-spec`; `response_format: code_review` beside `verdict`; normal code may emit it. Per execution, from accepted successful final outputs: each fanout item counts, only the final repeat output, never failed attempts or discarded speculation; 0 → summary, 1 → render, >1 → visible ambiguity even if consumed, malformed → visible error. Never changes the verdict; never truncated into a clean review | `wyrd-spec` unit tests; AC-008 journey |
| RENDER (REQ-013, AC-008) | Anchors checked against task 005 retained path/commit/range/source; suggestion replaces exactly that range; invalid → visible delivery error, never line 1 or altered finding; unlocated stays in summary; ID, severity, confidence shown per finding; no annotations. Check: stable name per binding, `external_id` = execution, reviewed `head_sha`, verdict → conclusion mapping, waiting → `in_progress` with explanation, never back from completed. Review: one `COMMENT` on reviewed commit with execution marker, no auto-resolve, no APPROVE/REQUEST_CHANGES/source change | rendering unit tests; AC-008 journey over the mock's Check/review |
| DELIVERY (REQ-014, REQ-005, INV-001–006, AC-009) | Background after settlement/park from retained state only; no recomputation, no verdict wait, no new SQL table/column or report in SQL. Lookup-before-create for Check and review, also after a timeout; rare duplicate accepted. Cooldown, bounded retry, visible failure. Access (002) and generation (006) rechecked before each write; superseded/revoked → skipped. One Bifrost outcome per execution; restart re-runs delivery for executions without one | delivery unit tests; AC-009 journey (lost answer, restart, new head, access loss, rate limit, refusal) |
| SURFACES (REQ-016, AC-011) | Delivery outcome/error in execution inspection, separate from verdict, under existing read authorization on HTTP and MCP only (no Rust client, SDK or CLI projection, spec revision 10); report schema authoring on the SDKs; runnable standard-report example; docs (optional reports, severity vs verdict, waiting, delivery outcome, retry diagnosis); wyrd-design and bifrost-design amended. A fresh reader completes App setup → declare → PR → evidence → approval → result/delivery from the guides | HTTP/MCP inspection journeys; codegen/docs/examples checks; content review |

## Evidence

PG journeys run through
`mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner >/dev/null && cargo nextest run --locked -p wyrd-server --test integration -E '<filter>'"`;
`test(/pg_github_delivery::/) | test(/pg_github_pull_request_extraction::/)` ran 29/29 passed.

| Obligation and required cases | Implementation evidence | Proof location | Verification evidence | Result |
|---|---|---|---|---|
| REPORT: closed schema, `code_review` beside `verdict`, code may emit it, selection (fanout items, final repeat only, no failed attempts or discarded speculation), 0/1/>1/malformed, verdict untouched, visible cut not clean truncation | `wyrd-spec/src/verification/report.rs` (`CodeReviewReport`, `resolve_response_format`, `select_report`); SDK authoring `skald-prompt/src/response_format.rs::code_review`, Python `ResponseFormat.code_review()`, TS `{type: "code_review"}` | `report.rs` tests; `pg_github_delivery.rs::each_execution_selects_its_report_from_its_accepted_final_outputs`, `::a_code_review_report_becomes_one_check_and_one_comment_review`; `presentation.rs::long_summaries_are_cut_visibly`; `skald-prompt builder::code_review_format_resolves_to_the_report_schema`; `test_prompt_authoring.py`; `authoring.test.ts` | `cargo nextest -p wyrd-spec --lib -E 'test(/verification::report::/)'` 4/4; skald-prompt 2/2; pytest 31/31; vitest authoring 5/5; PG journeys | PASS |
| RENDER: anchors against retained source, exact suggestion range, invalid → named problem never moved, unlocated in summary, ID/severity/confidence, no annotations; Check name/external_id/head_sha/conclusion map/in_progress never back; one COMMENT review with marker, no APPROVE/REQUEST_CHANGES | `components/github/presentation.rs` | `presentation.rs` tests (6); `pg_github_delivery.rs::a_code_review_report_becomes_one_check_and_one_comment_review`, `::each_execution_selects_…` (waiting `in_progress`), `::the_review_example_fails_on_a_high_finding_and_comments_its_fix` | `cargo nextest -p wyrd-server --lib -E 'test(/components::github::(presentation\|delivery)::/)'` 7/7; PG journeys | PASS |
| DELIVERY: background after settle/park from retained state, no recomputation or SQL table/column, lookup-before-create also after timeout, cooldown and bounded retry, visible failure, access and generation rechecked → skipped, one Bifrost outcome per execution revision, restart redelivers | `components/github/delivery.rs`; `verification/inputs.rs::delivery`; mock Check/review endpoints and faults in `wyrd-testing/src/github.rs` | `delivery.rs::outcome_identity_is_per_execution_and_revision`; `pg_github_delivery.rs::a_lost_answer_is_found_again_and_a_cooldown_is_waited_out`, `::a_restart_before_the_outcome_finds_what_already_landed`, `::a_new_head_during_delivery_skips_every_later_write`, `::removed_access_during_delivery_skips_every_later_write`, `::a_permanent_refusal_is_a_visible_failed_outcome` | PG journeys | PASS |
| SURFACES: outcome in execution inspection over HTTP and MCP (rev 10), report authoring on SDKs, runnable example, docs, architecture amended | HTTP `GET /v1/verification/executions/{id}` and MCP `verification.get_execution` serve `github_delivery`; tool description updated; `examples/review-a-pull-request/`; `docs/.../verify/review-a-pull-request.md`; `reference/verifier-card.md`; `architecture/wyrd-design.md` (Prompt, pull_request delivery), `architecture/bifrost-design.md` (`github_delivery` event) | `pg_github_delivery.rs::a_code_review_report_becomes_…` (HTTP == MCP), `::the_review_example_…` (example files as published) | `mise run docs:check`, `codegen:check`, `ts:napi:check`, `ts:typecheck`, `py:lints`, `lints`; content review against `presentation.rs`/`delivery.rs` | PASS |
| SURFACES / AC-011 connected journey: one fresh-reader journey from App setup through declare, PR, evidence, approval to result/delivery inspection on Rust/Python/TypeScript/MCP | Seams are proven separately: `wyrd-cli/tests/github_journey.rs` (connect, declare, PR, publish, approve through the CLI), `sdks/wyrd-sdk-rust/tests/integration/deferred_evidence.rs`, the PG delivery journeys (HTTP/MCP delivery) | — | No single connected journey exists | FAIL |

Process exceptions:

- Presentation and delivery production code was written before its failing tests (no RED recorded for those scenarios).
- `components/github/extraction.rs` (outside the write set) retries a pinned review once on the library's documented `ColdBuildError::PointerConflict`, after a read-only diagnosis and maintainer approval; multi-Verifier PRs otherwise errored.
- Test model renamed `gpt-test` → `gpt-4o` in the GitHub integration tests so the public example's Prompt runs unchanged.
- `card-types.ts` regenerated for the `github_delivery` wire type; no hand-written SDK or CLI projection (spec revision 10).
