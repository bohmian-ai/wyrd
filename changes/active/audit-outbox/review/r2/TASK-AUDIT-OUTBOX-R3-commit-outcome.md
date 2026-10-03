---
id: TASK-AUDIT-OUTBOX-R3-COMMIT-OUTCOME
kind: remediation
status: review
spec: SPEC-audit-outbox
spec_revision: 4
requirements: [REQ-009, AC-009]
remediates: [FIND-AUDIT-OUTBOX-11]
route_to: wyrd-implement
---

# Resolve the commit outcome before retrying an audit write

Approved spec revision 4, REQ-009 and AC-009, replaces the event-ID approach
from revisions 2 and 3. The root cause: the writer retried without knowing
whether its earlier commit had succeeded.

## Change

1. `AuditSink::write` (`crates/vala/vala-sql/src/audit_outbox.rs`):
   - Inside the transaction, read `pg_current_xact_id()` before the append.
   - If `commit` returns an error, resolve the outcome with `pg_xact_status`
     on a fresh connection:
     - committed: return `Ok`, so there is no retry;
     - aborted: return the error, and the existing outbox retry handles it;
     - in progress, or Postgres unreachable: wait with the outbox's existing
       backoff and ask again; never re-send while waiting;
     - `NULL`: count the events as lost and do not re-send.
   - Keep this inside the sink. The generic outbox does not change.
2. Delete the event-ID machinery:
   - the event-ID uniqueness and skip in the staging append;
   - migration `20261003000001_audit_staging_event_id.sql`;
   - `StagedAuditEvent`, if only the event ID justifies it (the outbox item
     goes back to `AuditEvent`);
   - the `event_id` column on retained `vala.system.audit_log` and its
     projection and publisher plumbing;
   - the duplicate collapse in harness reads;
   - the docs prose describing at-least-once delivery or event-ID
     deduplication.
3. Keep FIND-12, 13, 14, 7 and 3 from `e54b1244f` exactly as they are.

## Proof (AC-009)

A Postgres integration test through the production writer and publisher:
- A commit that succeeds in Postgres but returns an error to the writer is
  not re-sent.
- An aborted commit is retried and commits once.
- Repeated ambiguous commits, with the publisher retiring rows in between,
  leave exactly one retained row per decision and a gap-free chain.

Replace the journey `unknown_outcome_retries_retain_each_decision_at_most_twice_and_read_once`
with this exactly-once proof. Use the smallest real injection point, for
example failing the commit acknowledgement after it is sent. Do not use
sleeps.

## Verification

- Run the focused tests with their exact `mise exec -- cargo nextest run`
  commands.
- Run `pg_audit_outbox`, `test:bifrost:integration:sql`,
  `test:bifrost:journey:server`, `test:wyrd`, `fmt`, `lints`, `docs:check`,
  `check:unwrap-audit` and `git diff --check`.
- Append the evidence table.

## Evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Commit that lands but errors is not re-sent (AC-009) | `crates/vala/vala-sql/src/audit_outbox.rs` `AuditSink::write` reads `pg_current_xact_id()`; `resolve_commit` asks `pg_xact_status` on a fresh pool connection, returns `Ok` on committed | journey `audit_publication::ambiguous_audit_commits_retain_each_decision_exactly_once` (proxy `Cut::AckLost`, 3 rounds); red run with resolution disabled failed: decision retained `Some(2)` times | PASS |
| Aborted commit retried and commits once (AC-009) | `resolve_commit` returns the commit error on `aborted`; in progress / unreachable waits with `INITIAL_BACKOFF`..`MAX_BACKOFF` and re-asks; `NULL` counts `outbox_events_lost_total{outbox="audit"}`, no re-send | same journey, `Cut::CommitLost` round: retained once | PASS |
| Repeated ambiguous commits with publisher retiring in between: one retained row per decision, gap-free chain (AC-009) | publisher drained (`await_audit_published` + `await_drained`) between rounds | same journey: each decision retained 1x; retained `seq <= chain head` count equals head | PASS |
| Event-ID machinery deleted | `StagedAuditEvent` removed (item is `AuditEvent`); staging skip/column removed in `queries/audit_staging.rs`, `row_types/audit_staging.rs`; migration `20261003000001_audit_staging_event_id.sql` deleted; `EVENT_ID` column and projection removed in `vala-bifrost-redux/src/tables/audit`; harness collapse removed in `wyrd-testing/src/server.rs`; prose updated in `architecture/*`, `docs/.../bifrost/architecture.svx`, `wyrd-server/src/audit/mod.rs` | `git grep event_id` has no audit hits; `test:wyrd`, `docs:check` | PASS |
| Generic outbox unchanged | `crates/shared/wyrd-runtime/src/outbox.rs` untouched | diff | PASS |
| FIND-12, 13, 14, 7, 3 kept | no changes to outbox panic/shutdown, unwrap-audit script, or posture wording beyond event-ID prose | `check:unwrap-audit`, `test:wyrd` | PASS |

Commands (all exit 0):
- `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=audit_publication::ambiguous_audit_commits_retain_each_decision_exactly_once)'"` (via `mise exec --`): 1 passed
- `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=tables::audit::projection::tests::projects_canonical_content_and_derives_missing_batch_id)'`: passed
- `mise run test:bifrost:integration:sql`: 118 passed (includes the 3 `pg_audit_outbox` tests)
- `mise run test:bifrost:journey:server`: 31 passed
- `mise run test:wyrd`: 2329 passed
- `mise run fmt`, `mise run lints`, `mise run docs:check`, `mise run check:unwrap-audit`, `git diff --check`: clean

Removed test: `pg_audit_outbox::rewriting_a_committed_batch_stages_each_event_once` asserted event-ID skipping, which no longer exists. The journey above replaces it, along with `unknown_outcome_retries_retain_each_decision_at_most_twice_and_read_once`.
Not covered by a test: the in-progress/unreachable wait loop and the `NULL` loss branch. The `CommitLost` round may hit the in-progress branch, but no test asserts it.
