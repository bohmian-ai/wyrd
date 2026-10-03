---
id: TASK-AUDIT-OUTBOX-R3-COMMIT-OUTCOME
kind: remediation
status: ready
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
