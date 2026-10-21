---
id: TASK-003a-R2
kind: remediation
status: proposed
spec: SPEC-long-horizon-verification
spec_revision: 20
parent_task: TASK-003a-R1
remediates: [FIND-TASK-003a-2, FIND-TASK-003a-6, FIND-TASK-003a-7]
---

# Close grouped verification duration, terminal-record and documentation gaps

## Outcome and authority

Implement this single bounded remediation with `$wyrd-implement`. Three findings
remain: exact elapsed duration arithmetic, the existing terminal-error database
constraint, and missing rustdoc. Reuse their existing owners; add no polling,
retry, authorization or recovery machinery.

- Worktree: `/home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-001`.
- Branch: `change/long-horizon-verification-task-001`.
- Implementation starting candidate: `8376bb2aa71be18b9c7269cd9f2a601509137dc7`.
- Historical reviewed candidate: `3d0b301c51cfe6e50cbbde41b8b2ee3dfc2e9fb4`.
- Original cumulative base: `22335c2c40a5ad24c27bfa0d4686f4da49f922c5`.
- Prior candidate: `7cda4e93159ab328de2a74068d5e1978a2590818`.
- Approved spec: `changes/active/long-horizon-verification/spec.md`, revision 20.
- Original task: `changes/active/long-horizon-verification/tasks/TASK-003a-simplify-grouped-verification.md`.
- Prior remediation: `changes/active/long-horizon-verification/review/task-003a-20261018-7cda4e931/TASK-003a-R1-retain-pending-work-and-remove-stale-code.md`.
- Historical evidence: `verdict.md`, `findings-validation.md`, and `verification.md` in this directory, concerning revision 18 at the historical candidate.

This task was amended on 2026-10-19 after inspecting source through 8376bb2aa
and discussing findings 1, 7 and 8 with the owner. This is a remediation-scope
update, not a new cumulative review or a claim that tests passed at the new HEAD.
Preserve original review reports as historical evidence.

## Findings removed from implementation scope

| Finding | Current disposition | Source-backed reason |
|---|---|---|
| FIND-TASK-003a-1 | Resolved in source | Revision 19, commit e0d0c2206, removed `Denied` and the blanket HTTP-403 classification. `GroupPollError::from` now makes query failures retryable `Read` errors. Do not restore typed owner-query denial classification. |
| FIND-TASK-003a-8 | Superseded by approved revision 19 | Snapshot reads use the tenant's internal read principal, not owner Bifrost grants. The old owner-grant journey was intentionally removed; existing server coverage now removes owner roles and verifies successful snapshot admission. Do not recreate its obsolete denial phases or require its deleted selector. |

FIND-3/4/5 and HUMAN-CLEANUP 1–3 remain closed as previously recorded. Their
whole journeys need not be rerun for an unaffected write set.

## Diagnosis and selected minimal corrections

### FIND-TASK-003a-2 — Add fixed elapsed seconds rather than calendar days

**CONFIRMED residual; INCORRECT; Important.** Discovery DATA-2. Location: `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs:202`. Obligations: current REQ-004 exact accepted duration (retained from revision 18), original task locked decision 2, R1's fixed-year/exact-seconds replacement, and `architecture/wyrd-design.md:1283–1286`.

**Source trace and consequence.** TriggerDuration::parse (`trigger.rs:103–132`) scales `d` to 86,400 seconds and keeps authored text. Binding projection freezes those settings; the only production activity writer is ScribeSink::schedule_groups (`scribe_outbox.rs:766–802`) calling `record_group_activity`. Its SQL adds the authored interval directly to timestamptz; pending claim later consumes that due_at unchanged. `pool.rs:194–219` retains DSN/default settings and `TenantConn` binds the tenant without changing TimeZone. No UTC-only production policy supplies the missing invariant.

The data review's authorized managed-PG fixed-date proof used the actual operation and showed, at `2026-03-07 12:00:00-05`, New York `+ '1d'::interval` advances 82,800 elapsed seconds versus 86,400 for `24h`; UTC advances 86,400 for both. A supported database setting therefore makes equivalent accepted values schedule differently across DST. Existing P2's current-date/default-zone year assertion passes without excluding this case. No claim is made that its specific `365d` sample differed from `8760h`.

**Selected minimum correction.** Correct the one SQL activity owner to turn accepted duration into an interval of elapsed seconds before timestamp addition, e.g. extract epoch from the accepted interval and multiply by `INTERVAL '1 second'`. The accepted whole-second/year bound makes this an exact finite quantity. Preserve authored wire/storage text, statement-time coordination, tuple matching, batched upsert, RLS and ordinary SQL-error/outbox behavior. Do not introduce a global timezone setting/policy, new duration abstraction, alternate maximum or clock API.

**Closure.** Extend existing P2 with fixed-date DST-observing-session assertions for `1d` and `24h` using the corrected production arithmetic, including an exact 86,400-second advance. Preserve current one-year, normal-hour, debounce, claim, digest and settlement checks. No wall-clock wait or new journey is required for this SQL arithmetic seam. Run P2 below; the historical parser-only proof is recorded in verification.md and is not fresh evidence at this HEAD.

### FIND-TASK-003a-6 — Document the newly added slot visitor accurately

**REVISED; VIOLATION; mandatory rustdoc gate.** Discovery STD-001/MREV-R1-01. Location: `crates/wyrd-spec/src/card/verifier/mod.rs:322–341`. Authority: AGENTS §16 (`:734–748`) and `agent-rules.md:32`, explicitly applicable to private associated types and every fallible method. The task-review skill preserves missing rustdoc as a blocker.

Original Firsts, Visible::input/result and Support::activity sites are now documented. The new Slot visitor still has undocumented `type Value = InputSlot`; `expecting`, `visit_str` and `visit_map` have no Errors sections. The outer deserialize's Errors section does not cover these separate new items. Full method bodies were read: expecting propagates the formatter write error; visit_map forwards the tagged remote decoder's missing/unknown tag, field and value errors. SourceText (`card/verifier/reference.rs:177–200`) is a transparent string wrapper deliberately accepting any string; its grammar is validated later. STD-001's proposed source-grammar refusal inside visit_str is therefore rejected.

**Selected minimum correction.** Add concise substantive rustdoc to the associated output and hooks, with Errors sections describing actual behavior: no practical decoding error for an already supplied string and deferred source-grammar validation, tagged map decode failures, and formatter write failure. Keep behavior, schema and public types unchanged. Do not manufacture validation or a new test to support prose. Inspect these exact items and run affected Rust format/lint checks; batch this with the behavioral corrections.

### FIND-TASK-003a-7 — Constrain the new authority-free exception to its selected shape

**REVISED; VIOLATION; Important.** Discovery DATA-1 and follow-up B. Location: `crates/wyrd/wyrd-sql/migrations/20261018000000_verification_group_pending.sql:46–75`. Explicit authority: current revision-20 REQ-004 (retaining the revision-18 terminal-record requirement) says the pre-admission error has no source inputs/snapshot and only this terminal shape may omit authority (`spec.md:384–394,424–435`); R1's selected persistence boundary expressly lists absent inputs/routes/snapshot and directs SQL constraints narrowly to permit that shape (`R1:66–79`).

**Producer, writers and consumers.** Enumerated all four production execution INSERTs: submitted, scheduled, pinned group, and refused group. `refuse_group:1824–1843`, used by the scheduler for owner refusals and, since revision 20, expired incomplete snapshots, correctly uses INSERT_GROUP_ERROR_SQL and omits authority, all input fields and snapshots. Manual/scheduled/group writers still provide their existing authority and proper input shapes. CLAIM_SQL admits only pending/retrying/expired-running; retry/release/termination require a running lease; completion requires lease fencing. Proper refusal records are never claimed or dispatched. StatusRow/execution_status (`verifier_executions.rs:2185–2235,2635–2658`) need no authority but decode any non-NULL group_sources as a BTreeMap. Public status authorization remains in VerificationControl::get_binding/get_execution.

The new refused_shape CHECK restricts terminal metadata but omits input/snapshot absence. Its input_shape ordinary object arm can admit authority-NULL object inputs/routes. Its group_shape accepts false equality for a one-byte non-NULL digest or array-valued non-NULL sources as well as NULL. The data review's migrated CHECK/NOT NULL clone admitted both counterexamples; array sources then conflict with StatusRow's map decoder. This proves a schema-admitted invalid exception shape, not an emitted application state or public exploit. The invariant review's broader arbitrary-corruption rejection does not override the explicitly required constraint boundary. The existing production writer is already correct.

**Selected minimum correction.** Strengthen the existing authority-NULL terminal alternative itself to require window_start/window_end, input_record_id/input_event_time, inputs/route_overrides, group_snapshot_sha256 and group_sources all SQL NULL, in addition to its present terminal metadata. Keep inherited no-errorless-terminal/no-retry-deadline/no-running-lease checks and ordinary authority-present shapes. One authoritative condition prevents falling through any ordinary input arm or malformed false-equivalence; no decoder fallback or new writer is needed. Reject DATA-1's broader totalization of old authority-present ordinary-input predicates: no newly unsafe writer or separate required correction was established there.

**Closure.** Extend existing P2 with database-rejected updates of an otherwise valid refusal to object inputs/routes and malformed non-NULL digest/source fields, while retaining valid refusal insert/readback/attribution, no claim/dispatch, and rejection of authority-free pending status. Use the existing fixture/transaction pattern. Run P2; no new table, lifecycle state or public security decision is required. Apply the constraint correction through the existing migration workflow; do not change an already-applied migration checksum or reset a database to make it pass.


## Scope and preserved behavior

| Required correction | Existing owner | Consumer and proof closure |
|---|---|---|
| FIND-2: fixed elapsed due time | `wyrd-sql` RECORD_GROUP_ACTIVITY_SQL / record_group_activity | Scribe scheduling notifications and existing SQL snapshot/settlement test |
| FIND-7: narrow terminal record | Existing verifier_executions refused-shape constraint | refuse_group, status decoding and the same SQL test |
| FIND-6: accurate rustdoc | Existing InputSlot Slot visitor | Unchanged decoder behavior; source inspection and Rust static checks |

Preserve these approved decisions:

- `group_end.after` is positive and at most 365 fixed days, rejected at write
  time above that bound. Keep exact authored text; no SQL clamp or new maximum.
- Snapshot reads use the tenant's credentialless internal read authority,
  limited to verification input/result tables and existing owner/source filters.
  Owner activity/accepted authority still govern execution work. Query failures
  retain pending work; do not restore the retired snapshot-read denial branch.
- Revision 20's result-without-input check pins nothing and retries while the
  group is no more than five minutes past due. Beyond that PostgreSQL-clock
  deadline it records one `WYRD_VERIFICATION_409_SNAPSHOT_INCOMPLETE` execution
  and consumes pending atomically. An input without a result remains normal.
- Owner refusal and expired incomplete snapshots share the same pre-execution
  terminal shape: error/attribution, no accepted authority, inputs or snapshot,
  no evaluation, verdict, result pointer or Operator. Error-write failure rolls
  back and retains pending. FIND-7 constrains this shape; it does not add or
  alter the missing-input retry/deadline.
- Keep incomplete execution IDs in the stored error message under the existing
  code/message contract. Do not add stored structured details or another error.
- Preserve PostgreSQL hot-path absence, tenant isolation/RLS, exact sources,
  existing claims/leases, normal schedules, manual idempotency, outbox admission,
  non-blocking audit, and existing accepted loss/backlog/overlap/postponement limits.

Do not harden unrelated authority-present SQL shapes, add decoder fallbacks,
change grants, add a timezone policy, duration type, queue, clock, setting,
dependency, history endpoint or recovery protocol. The current refusal writer
already emits correct NULLs; no current writer defect or public exploit is
claimed for FIND-7. Benchmarking REQ-015/AC-009 remains subsequent work.

## Ordered implementation scenarios

### Scenario 1 — Equivalent fixed durations schedule equal elapsed time

**Behavior.** Accepted `1d` and `24h` mean exactly 86,400 elapsed seconds even
across a DST transition in a DST-observing PostgreSQL session.

**RED.** Extend P2 below with a fixed-date, session-local timezone regression
using the activity SQL's actual arithmetic. At 2026-03-07 noon in
America/New_York, current direct calendar-day addition differs from 24 hours.
Prove the production expression is covered; a test of an unrelated corrected
expression alone is insufficient. No wall-clock wait is needed.

**GREEN.** Normalize the accepted interval to elapsed seconds in the existing
activity SQL. Preserve statement-time coordination, exact source matching,
upsert/debounce behavior, authored duration text and the one-year write bound.
Run P2 with existing year/hour, claim, digest and settlement assertions intact.

**REFACTOR.** Use native PostgreSQL interval arithmetic and the existing owner.
No global timezone change or new duration/clock abstraction.

### Scenario 2 — A pre-execution error cannot also contain execution inputs

**Behavior.** An authority-free terminal group error remains attributable,
readable and non-runnable, with every input/window/record/snapshot field NULL.
The same shape supports owner refusal and revision 20's incomplete error.

**RED.** Extend existing P2 using its transaction pattern: from an otherwise
valid refusal, attempt non-NULL input/route/window/record/snapshot values,
including object inputs/routes and malformed non-NULL digest/source values.
Require database rejection. Existing checks admit the diagnosed combinations.
Keep valid insertion/readback and authority-free pending rejection assertions.

**GREEN.** Add the explicit NULL requirements to the existing authority-free
terminal constraint branch through the repository migration workflow. Leave
ordinary authority-present shapes and the already-correct writer unchanged.
Run P2 after this change and Scenario 1.

**REFACTOR.** Enforce this once in the database constraint. No consumer guards,
new execution states, snapshot defaults or wider legacy constraint rewrite.

FIND-6 is documentation-only: accurately document the associated visitor output
and each fallible hook. No manufactured RED or decoding behavior change.

## Acceptance criteria

| Finding | Required outcome | Proof |
|---|---|---|
| FIND-TASK-003a-2 | Equivalent accepted durations add exactly the same elapsed seconds across DST; write-time 365-day limit and existing queue behavior stay intact. | P2 fixed-date regression and existing assertions |
| FIND-TASK-003a-6 | Visitor output and cited fallible hooks have substantive accurate rustdoc; source strings remain verbatim and grammar validation remains later. | Source inspection; fmt/lints |
| FIND-TASK-003a-7 | Valid authority-free terminal records read correctly and cannot run; non-NULL execution inputs/windows/record/snapshot fields are rejected; authority-present rows stay valid. | P2 rejected updates, valid readback and existing execution assertions |

## Focused verification

Commands below are implementation obligations, not fresh passing results.
Use nonzero exact test selections. Run PostgreSQL-backed commands sequentially.

**P2 — existing SQL duration and terminal-record owner**

```bash
WYRD_LOG=info,wyrd_server::verification=debug mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sql --test integration -E "test(=pg_verifier_executions::group_snapshots_are_idempotent_and_react_in_settlement_order)"'
```

**P1 — two existing scheduler regressions: revision-19 internal reads and
revision-20 incomplete-snapshot handling**

```bash
WYRD_LOG=info,wyrd_server::verification=debug mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration -E "test(=pg_verification_groups::pending_groups_debounce_and_stop_after_handoff) | test(=pg_verification_groups::incomplete_snapshots_retry_then_error)"'
```

Run `mise run fmt`, `mise run lints`, `mise run check:tenant-isolation`, and
`git diff --check` for the bounded Rust/SQL write set. No Python/TypeScript
runtime changes, generated contract changes, full SDK suites, broad gate or
benchmarks are selected by this remediation. Broader integrated qualification
belongs to change review.

## Completion evidence and handoff

Append source/acceptance evidence for the three remaining stable finding IDs,
exact commands/features and nonzero counts, RED/GREEN evidence for the two SQL
corrections, migration results and the final scope audit. Keep historical
revision-18 failures clearly identified as historical. Record FIND-1 as resolved
in source and FIND-8 as superseded by revision 19, not as newly implemented R2
fixes. Do not claim that the removed owner-grant journey ran successfully.

Trace unexpected failures before diagnosis. Do not weaken checks/assertions,
ignore tests or extend waits to clear a failure. If a new material contract or
architecture decision is actually required, report the concrete conflict.
Otherwise implement the bounded corrections without redesigning grouping.

Return the cumulative candidate to `$wyrd-task-review` against the original
TASK-003a plus R1/R2 and approved revision 20. This task amendment is not a new
formal verdict, final integrated approval or performance qualification.

## Implementation evidence (2026-10-19, from 8376bb2aa)

Status: `IMPLEMENTED`. FIND-TASK-003a-1 remains resolved in source (e0d0c2206)
and FIND-TASK-003a-8 superseded by revision 19; neither was re-implemented, and
the removed owner-grant journey was not run. Revision-18 failures in
`verification.md` remain historical.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-2: `1d` and `24h` add identical elapsed seconds across DST; 365-day bound and queue behavior intact | `RECORD_GROUP_ACTIVITY_SQL` adds `extract(epoch FROM after::interval) * INTERVAL '1 second'` (`verifier_executions.rs`); authored text, upsert, statement time unchanged | P2 DST block: RED `left: (false, false)` at the `1d`/`24h` assertion before the SQL change, GREEN after; existing hour/year/claim/digest/settlement assertions retained | PASS |
| FIND-6: Slot visitor output and fallible hooks documented accurately | `type Value`, `expecting`, `visit_str`, `visit_map` rustdoc with `# Errors` (`card/verifier/mod.rs`); behavior unchanged | Source inspection; `mise run fmt`, `mise run lints` exit 0 | PASS |
| FIND-7: authority-free terminal records reject every input/window/record/snapshot field and runnable status; valid refusal reads back; authority-present rows unchanged | New migration `20261019000000_verification_refused_shape.sql` replaces `verifier_executions_refused_shape`, requiring those fields NULL in the authority-free branch only (applied migration left unchanged) | P2 rejection loop (pending status, object inputs/routes, window, record, 1-byte digest, array sources), each in a sqlx nested transaction: RED `rows_affected: 1` before the migration, GREEN after; existing refusal readback/attribution/no-claim assertions retained | PASS |

Deviation, recorded: the DST regression does not use the fixed date
2026-03-07 New York, because `statement_timestamp()` cannot be pinned and a
fixed-date test would exercise a copy of the expression rather than the
production statement. P2 instead sets a session-local POSIX zone
(`XST0XDT,<today>/<next hour>,<today>/<+49h>`) whose daylight time starts within
the hour, asserts a calendar day in that session is 23 hours, then drives the
real `record_group_activity` and requires `due(1d) = due(24h)` and
`due(1d) - due(1h) = 23 hours`.

Commands (PostgreSQL lanes run sequentially):

- P2: `WYRD_LOG=info,wyrd_server::verification=debug mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sql --test integration -E "test(=pg_verifier_executions::group_snapshots_are_idempotent_and_react_in_settlement_order)"'`. Migrations (wyrd-sql 1, vala-sql 1) passed; 1 passed, 103 skipped.
- P1: same wrapper with `-p wyrd-server --features test-support --test integration -E "test(=pg_verification_groups::pending_groups_debounce_and_stop_after_handoff) | test(=pg_verification_groups::incomplete_snapshots_retry_then_error)"`. 2 passed, 275 skipped.
- `mise run fmt`, `mise run lints`, `mise run check:tenant-isolation`, `git diff --check`: all pass.

Scope audit: changed files are `card/verifier/mod.rs` (rustdoc only),
`verifier_executions.rs` (activity SQL and its doc), the P2 test, and the new
migration. No timezone policy, duration type, clock, decoder fallback, grant,
or generated-contract change. The untracked historical review files in this
directory were not edited apart from this evidence section.
