---
task: TASK-009-R1
source_review: 009-reports-github-delivery-r1-4d001e6d9
approved_spec_revision: 11
---

# TASK-009-R1 — Report delivery corrections

Remediate the round-1 review of TASK-009 (candidate 4d001e6d9). The contract is
[task 009](../../tasks/009-reports-and-github-delivery.md), spec revision 11 and
the validated findings in [findings-validation.md](findings-validation.md).

In scope: FIND-009-1, -3, -4, -5, -7, -8 and -10.

Deferred by the user as accepted limits in spec revision 11, with no work here:
FIND-009-2 (extraction-shaped decoy), FIND-009-6 (recheck after lookup),
FIND-009-9 (recovery shutdown under cooldown).

Write set: `crates/wyrd/wyrd-server/src/components/github/{delivery,presentation,app}.rs`,
`crates/wyrd/wyrd-server/src/verification/inputs.rs`,
`crates/wyrd/wyrd-server/src/components/verification/service.rs` and
`service/inspection.rs`, `crates/wyrd-spec/src/verification/report.rs` (docs
only), their tests and `pg_github_delivery.rs`, plus the user guide.

## Obligations

| ID | Promise | Contract source | Planned proof |
|---|---|---|---|
| FIND-009-1 | A report renders without a retained extraction: summary, unlocated findings, invalid/ambiguous reasons; supplied anchors refused `anchor_source_unavailable`; verdict unchanged | REQ-012/013, RENDER | presentation units: one/malformed/multiple without extraction |
| FIND-009-3 | An anchor validates against either retained side of a changed file (base under `previous_path` on rename) or a selected context span; only head-side hunk locations are inline; unretained ranges still refuse | RENDER, wyrd-design report delivery | presentation units: base evidence, context evidence, rename side, out-of-span refusal |
| FIND-009-4 | Every valid rendered report gets one `COMMENT` review, also with zero inline comments; retry/restart finds it by marker | REQ-013, wyrd-design | presentation unit plus existing PG empty-report journey updated |
| FIND-009-5 | Cooldown honors the full `Retry-After` and the primary rate-limit reset; attempts stay bounded | REQ-014 | app units on the classifier |
| FIND-009-7 | A waiting delivery never updates an existing Check; terminal delivery updates in place | REQ-013, wyrd-design | PG journey: terminal Check first, late waiting delivery leaves it completed |
| FIND-009-8 | A cancelled execution delivers once more under its revision, separate from that revision's outcome; the Check is updated in place to `cancelled`; inspection reads the cancelled outcome; public cancel starts it; restart recovers it | REQ-014 rev 11, bifrost-design | PG journey: park → waiting delivered → public cancel → same Check `cancelled`, inspection `delivered` |
| FIND-009-10 | Rustdoc with `# Errors`/`# Panics` on every changed item | AGENTS.md Code | diff audit, lints |

## Evidence

Candidate range `16abc095e..1315240f9`.

| Obligation and required cases | Implementation evidence | Proof location | Verification evidence | Result |
|---|---|---|---|---|
| FIND-009-1: report without extraction — summary, unlocated finding, refused anchor `anchor_source_unavailable`, ambiguous/invalid reasons, verdict unchanged | `presentation.rs` `Completed::extraction: Option`, free `place`; `delivery.rs` `render` always passes the selection | `presentation.rs` `reports_render_without_an_extraction` | `-p wyrd-server --lib -E 'test(/components::github::presentation::/)'` 9/9 | PASS |
| FIND-009-3: base side, renamed base under `previous_path`, selected context valid; head-hunk only inline; wrong commit, out-of-range base/context, head path on base side refused | `presentation.rs` `Source`, `Retained`, `sides`, `Context` | `evidence_in_base_or_context_source_is_valid`, `anchors_outside_the_retained_source_are_problems`; real extracted context: `pg_github_delivery::a_code_review_report_becomes_one_check_and_one_comment_review` (F-2 in `src/user.ts` context valid, F-6 unknown path refused) | presentation units 9/9; `pg_github_delivery::` 10/10 | PASS |
| FIND-009-4: empty and unlocated-only reports reviewed with zero comments; restart/retry reuse via marker | `presentation.rs` `render`: review follows `ReportSelection::One` | `every_rendered_report_has_a_review`; PG `a_code_review_report…` (incomplete empty report reviewed), `each_execution_selects…` (repeat, speculation reviewed; none/ambiguous/invalid not); marker reuse unchanged in `a_restart_before_the_outcome_finds_what_already_landed` | units 9/9; PG 10/10 | PASS |
| FIND-009-5: full `Retry-After` beyond 15 min; primary reset when remaining is 0; 60 s fallback; bounded attempts kept | `app.rs` `GithubApp::cooldown`; `COOLDOWN_MAX` removed in `delivery.rs` | `app.rs` `cooldowns_last_as_long_as_github_asks`; short cooldown journey `a_lost_answer_is_found_again_and_a_cooldown_is_waited_out` | `-E 'test(/components::github::app::/)'` 3/3; PG 10/10 | PASS |
| FIND-009-7: waiting never updates an existing Check; terminal updates in place | `delivery.rs` `Write::send` skips an existing Check for nonterminal bodies | `pg_github_delivery::a_waiting_delivery_never_updates_an_existing_check` (second wait sends no PATCH; terminal PATCH completes same id) | PG 10/10 | PASS |
| FIND-009-8: public cancel of a waiting execution delivers once more; same Check `cancelled`; inspection serves the cancelled outcome; restart writes nothing again; principal revocation via restart recovery (spec rev 11) | `delivery.rs` `event`, `key(event, …)`, `recorded(.., status)`; `inputs.rs` `delivery(.., event)`; `service.rs` inspection by status; `inspection.rs` `cancel` spawns delivery | `pg_github_delivery::cancelling_a_waiting_execution_completes_its_check_cancelled`; `delivery.rs` `outcome_identity_is_per_event_execution_and_revision` | PG 10/10; github units 16/16 | PASS |
| FIND-009-10: rustdoc, `# Errors`, `# Panics`, partial-progress notes on changed items | `report.rs` `ReportVersion` impls and tests; `presentation.rs` NOTE, HEAD, tests; `delivery.rs` `spawn`, `send`, `attempt`, `record`; `app.rs` write methods; test helpers | diff `16abc095e..1315240f9` | `mise run lints` clean | PASS |

Also run: `pg_github_pull_request_extraction::` 21/21, CLI `github_journey::` 5/5,
`wyrd-spec` report units 4/4, `docs:check`, `git diff --check`.

Process exceptions: tests were written alongside the fixes, so no separate RED
run is recorded. The context view `Context`/`Span` reads only `head` and span
lines from the library's serialized `ReviewContext` instead of decoding the
whole type, so a fixture needs no full library context. Deferred by spec
revision 11, untouched: FIND-009-2, FIND-009-6, FIND-009-9.
