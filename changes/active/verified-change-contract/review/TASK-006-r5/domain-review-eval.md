# R5 continuous Eval domain review

## Immutable subject and authority

- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`; candidate: `6ce9f9bb7e2dea288a1b78081346551d896cb7a8` (HEAD checked at review).
- Approved `spec.md` revision 43, original `tasks/TASK-006-continuous-eval-verifier.md`, R1–R4 remediation and prior finding ledgers, `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, `architecture/bifrost-design.md`, `architecture/references/domain/evaluation.md`, and the packet's `architecture/verifier/eval.md` govern this boundary.
- Reviewed the cumulative changed Eval engine, server adapter, post-ACK hook, queue and Bifrost observation table, result projection/publisher, and real server journey. R4 changed only import style in `verification/mod.rs` within this domain; it did not change Eval execution behavior.

## Obligation coverage

| Obligation | Source evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-083/130, INV-012: one existing Eval scoring/judge path, with sampling before trace or tasks | `verification/eval.rs:134-270` calls `ScenarioScoring::score_record`; `verification/runner.rs:488` dispatches to that arm. | `continuous_eval_runs_the_terminal_matrix`; focused `vala-eval` regressions recorded in TASK-006. | PASS |
| REQ-084, INV-004: only returned assertions attest; gate, skipped, errors, and capture map correctly | `verification/engines.rs:60-77` applies capture before verdict; `verification/eval.rs:254-278` retains execution failures as retry/terminal outcomes. | Server journey checks pass, fail, ungated, all-skipped, sampled-out, errored, timed-out, redacted evidence, and dispatches; `eval_reports_map_to_the_common_verdict_after_capture`. | PASS |
| REQ-077/152, AC-014: first Scribe commit activates best-effort run with exact record and managed event day | `gate/mod.rs:975-1002` invokes the hook only after ACK and on `first_commit`; `verification/observations.rs:60-120` spawns bounded tracked work; `tables/eval/observations.rs:78-125` derives committed keys; `verifier_runs.rs:1320-1370` enqueues under tenant RLS. | Server journey forces post-ACK SQL insert failure, checks acknowledged row/no run, checks frozen timestamp, and proves `created_at` differs in UTC day; `sealed_replay_on_a_later_day_activates_once` covers later-day replay. | PASS |
| REQ-083/085, AC-016: trace wait/retry/restart and canonical summary/items before settlement or dispatch | `verification/eval.rs:170-215,566-676`; `verification/results.rs:210-248,368-440`; `verification/runner.rs:490-550`. | Server journey covers terminal matrix and restart; SQL/runtime tests cover retry, trace deadline, partial ACK, and lease recovery as recorded in TASK-006 and R4 evidence. | PASS |
| REQ-131, AC-027: named tenant-authorized media reaches native Skald input; refusals stay execution errors | `verification/eval.rs` `TenantMedia` resolver; `vala-eval` scoring/judge media path; server journey lines 697-763, 832-843, 918-934. | Provider capture sees base64 native image and no private URI; unbound, foreign-tenant, unsupported MIME, and oversized records error with no result, dispatch, or provider call. | PASS |
| Prior Eval findings `FIND-TASK-006-1` through `-12` remain closed | R2/R3 Eval reviews traced first-commit replay, frozen ordinal/day, bounded media and trace reads, System authority, error redaction, cancellation, and the media refusal matrix. The R4 diff does not alter these owners. | R4 evidence records `test:sql`, `test:server:startup`, `test:server:kind`, and `test:server:peer` passing; prior server/Oracle journey evidence is preserved. | PASS |
| Non-goals: no second engine, outbox, fabricated failed assertion, offline dataset route, or provider file lifecycle | Cumulative source and changed-file search show the existing engine and best-effort hook, with no new route or queue for these behaviors. | Static source review. | PASS |

## Material proposed findings

None.

## Verification limits

This is a static review; I did not rerun the environment-owning journeys. The R4 evidence does not claim a fresh full Eval server journey at candidate `6ce9f9bb7`, but the later R4 diff leaves Eval behavior unchanged except import style. Peer transport, SQL readiness, image startup, and production guide correctness are outside this domain and require their assigned reviews. The current cross-day journey uses a test receipt-clock step; it checks the persisted row and frozen run value, so it exercises the production read selector rather than only a pure helper.

## Overall result

**PASS** — no material continuous Eval implementation or evidence gap identified in the cumulative candidate.
