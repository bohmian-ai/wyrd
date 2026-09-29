# TASK-006 Cumulative Task Implementation Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t006`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `3593bbc31273673f87159315fbf66a73562d3c99`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 36
- Original task: `changes/active/verified-change-contract/tasks/TASK-006-continuous-eval-verifier.md`, written against revision 35
- Remediation task: `changes/active/verified-change-contract/review/TASK-006-r1/TASK-006-R1-continuous-eval-closure.md`, written against revision 36
- Prior review: `changes/active/verified-change-contract/review/TASK-006-r1/`

The candidate remained the checked-out `HEAD` throughout this review. I reviewed the complete base-to-candidate range and used the prior reports only to identify closure obligations; conclusions below come from the candidate source, tests, and supplied verification record.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-077, REQ-079, INV-004, AC-014, Scenario 4: only the acknowledgement that first committed an observation activates best-effort enqueue with the committed managed event time; replay converges without joining the Scribe transaction | `vala-bifrost-redux/src/contracts.rs` carries `FrameAdmission.first_commit`; `scribe/shards.rs` derives it from the existing durable fence/insertion owner; `gate/mod.rs` calls the existing observation hook only on first commit; `verification/observations.rs` retains asynchronous post-ACK enqueue | `sealed_replay_on_a_later_day_activates_once`; retained Scribe retry tests; supplied `test:bifrost:journey:server` and `journey:scribe` passes | PASS |
| REQ-083, INV-010, AC-016, Scenario 5: sampling occurs before trace/tasks and one run's `every_nth` decision is stable across retry/restart | Migration `20260601000032_verifier_run_observation_ordinal.sql` stores a positive per-binding ordinal; `VerifierRunQueue::enqueue` locks the binding and assigns it once; `ClaimedRun` returns the stored value; `eval.rs::sampled` performs no recount | `pg_verifier_runs::observation_ordinal_is_fixed_at_serialized_enqueue`; terminal-matrix and restart journey evidence | PASS |
| REQ-084 and Scenario 1: successful false assertions attest through the authored gate; execution/input/provider errors stay errors | `vala-eval/src/executor.rs::fan_out_bucket` propagates executor/join errors; `EvalEngine::score` maps scoring errors to retry and returned reports through the common verdict mapping | Six focused `vala-eval` regressions rerun in this review: 6/6 PASS, including executor-error propagation and the false-comparison case | PASS |
| REQ-085, AC-020, Scenarios 2 and 6: completed runs persist the canonical summary and every Ran/Skipped item; sampled-out writes zero-count summary/no items; errored/timed-out write no results; only completed failed gates dispatch | `VerifierReport::eval`, result payload construction, acknowledged publication, and generic fenced settlement remain the single projection/dispatch path | `continuous_eval_runs_the_terminal_matrix`; supplied Postgres and server journey evidence | PASS |
| REQ-111, REQ-130, INV-012: continuous Eval reuses the existing Vala `ScenarioScoring`/`SkaldJudgeInvoker` path and preserves authored trace assertions | `EvalEngine::score` calls `ScenarioScoring::score_record`; `BifrostReader::spans` reconstructs persisted attributes, events, links, and dropped counts into the existing `SpanRecord`; no second engine or judge exists | `continuous_eval_reads_ordered_bounded_trace_evidence`; focused existing trace regression rerun PASS | PASS |
| REQ-083 reproducibility and bounded analytical reads: trace evidence has a closed time range, deterministic total order, and fixed row ceiling before decoding/provider work | `BifrostReader::spans` applies lower/upper `wyrd_event_time` predicates, `ORDER BY start_time_unix_nano, span_id`, and `LIMIT TRACE_SPAN_LIMIT + 1`, refusing overflow before decode | `continuous_eval_reads_ordered_bounded_trace_evidence`; `continuous_eval_refuses_a_trace_over_the_span_ceiling` | PASS |
| AC-014 Scenario 5: a real SDK-to-server path proves lookup by frozen managed day when client `created_at` is on another UTC day | Existing SDK journey shifts Scribe receipt time by a day, asserts the authored creation day differs, and completes/readbacks through frozen `wyrd_event_time` | `continuous_eval_runs_the_terminal_matrix`, including recorded mutation proof against creation-day lookup | PASS |
| REQ-131, AC-027, Scenario 3: named tenant-authorized supported media is bounded by the effective body, reaches the existing native provider input, and private locators do not escape | `TenantMedia::resolve` keeps scheme/tenant/MIME/kind checks and uses `StorageHandle::get_object_bounded` before base64/provider invocation; storage reads stop after the limit-plus-one sentinel | `get_object_bounded_holds_at_most_one_byte_past_the_limit` rerun PASS; storage matrix and media/provider/refusal tests supplied PASS | PASS |
| Revision-36 REQ-086 and security authority: Eval reads use the stable persisted per-tenant SYSTEM principal with separate table-scoped read authority; missing/wrong-tenant/under-scoped authority fails closed and Oracle audits decisions | `EvalReadAuthority::resolve` loads the tenant SYSTEM UUIDv7, grants only existing `vala.eval.observations` and `vala.traces.spans` table UIDs, and builds the ordinary `AuthorizedQueryContext`; `ScheduledQueryCaller` records object denials through the canonical authority path | `continuous_eval_reads_ordered_bounded_trace_evidence`; `continuous_eval_read_authority_fails_closed`; supplied principal integration/unit lanes | PASS |
| Stable, secret-free public verification errors | Eval and runner boundaries log dependency causes separately and persist fixed code/operation text; media failures log only binding/category/backend kind, never locator or backend `Display` | `continuous_eval_failures_publish_only_stable_errors`; unit log/redaction tests and mutation evidence | PASS |
| Repository async documentation obligation for changed fan-out behavior | `vala-eval/src/executor.rs::fan_out_bucket` documents role, first-error propagation, sibling cancellation, discarded partial outputs, and non-rollback of prior side effects | Focused executor regression rerun PASS; supplied `mise run lints` PASS | PASS |
| INV-015: tenant isolation across enqueue, input reads, media, results, and audits | TenantConn/RLS owns control reads and writes; System authority is tenant-bound and table-scoped; object paths validate the run tenant; result publication retains exact-Verifier signed scope | Negative System-authority/media journeys and supplied SQL/principal/Bifrost lanes | PASS |
| AC-016 complete retry/restart/terminal behavior | Generic queue owns claims, releases, retries, trace waits, and settlement; admission refusal uses the existing attempt-refunding release with a poll delay rather than consuming the run budget | `deferred_engine_admission_requeues_without_spending_attempts`; terminal matrix; restart evidence; supplied `pg_verification_runtime` pass | PASS |
| REQ-152: PostgreSQL remains the coordination clock | Run insertion, ordinal serialization, claims, release delays, retry deadlines, trace deadline, and settlement remain Postgres-owned; producer event time remains evidence, not coordination | SQL concurrency and runtime integration evidence | PASS |
| AC-033 and task verification set | Candidate records passing focused regressions, `fmt`, `lints`, `codegen:check`, `test:vala`, `test:sql`, `test:wyrd`, every Bifrost unit/integration/journey leaf, `test:wyrdstate:journey`, and boundary checks | This review reran 7 focused tests successfully and `git diff --check`; broad lanes accepted as supplied evidence due the review time budget | PASS |
| Explicit non-goals and prohibited changes | No outbox, atomic Scribe/run transaction, Bifrost queue poller, synthetic failed assertion, second Eval engine/judge, offline dataset execution, provider-file lifecycle, public auth surface, broad System grant, or runtime configuration knob was added | Complete cumulative diff inspection | PASS |
| No unrelated implementation drift | The skill-documentation commit was explicitly accepted in the prior review. The audit-publisher correction changes no Eval contract; it is the diagnosed root-cause correction required to make the task's mandated server journey lane credible under the repository rule that a red required gate blocks completion. Its existing multi-tenant invariant and focused proof remain within that verification closure. | Supplied before/after trace, focused audit-publisher test, and green server journey lane | PASS |

## Prior-finding closure

| Stable finding | Candidate closure | Result |
|---|---|---|
| `FIND-TASK-006-1` replay can activate with a different receipt instant | Durable first-commit disposition reaches Gate and gates the activation hook | CLOSED |
| `FIND-TASK-006-2` sampling rank changes with MVCC visibility | Immutable serialized observation ordinal stored on the run | CLOSED |
| `FIND-TASK-006-3` metadata/body race bypasses media ceiling | Limit-plus-one effective-body read and post-read refusal | CLOSED |
| `FIND-TASK-006-4` trace events/links fabricated empty | Canonical event/link/dropped-count projection and decoder | CLOSED |
| `FIND-TASK-006-5` positional span evidence unordered | Total SQL order by start timestamp and span ID | CLOSED |
| `FIND-TASK-006-6` cross-day journey absent | SDK journey separates client creation and managed receipt days | CLOSED |
| `FIND-TASK-006-7` fabricated user for internal reads | Stable tenant SYSTEM identity with narrow table scope and canonical audit | CLOSED |
| `FIND-TASK-006-8` trace read unbounded | Closed time interval plus fixed sentinel row ceiling | CLOSED |
| `FIND-TASK-006-9` raw dependency errors public | Fixed public errors with causes confined to structured diagnostics | CLOSED |
| `FIND-TASK-006-10` changed fan-out undocumented | Required error and cancellation rustdoc added on the existing owner | CLOSED |

## Proposed findings

None. I found no reachable MISSING, INCORRECT, DRIFT, VIOLATION, or REGRESSION acceptance defect in the immutable candidate.

## Verification limits

- I did not rerun the Postgres-, emulator-, or real-server-dependent broad lanes inside the 20-minute sub-review budget. Their exact commands, counts, failure diagnosis, and successful reruns are recorded in the remediation task and were checked against the candidate's named tests and source.
- I independently reran the six exact `vala-eval` regressions and the focused bounded-storage test: 7/7 passed. `git diff --check` passed.
- No live provider or cloud-storage service was used; the task's local provider journey and repository-managed storage emulators are the applicable evidence.

## Overall result

**PASS**

The cumulative candidate satisfies the original TASK-006 obligations and closes all ten prior stable findings without changing the approved terminal model or prohibited boundaries.
