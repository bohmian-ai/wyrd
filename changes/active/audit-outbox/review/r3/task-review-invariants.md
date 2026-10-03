# Audit outbox r3 invariant review

## Immutable subject and scope

- Base: `cf5ee4128ce0b842e00a0eb20770ab5c285dedd8`
- Candidate: `52e1144b5c186ccacd85d9c779a4e60c3cce5ac2`
- Reviewed range: `cf5ee4128..52e1144b5`
- Approved authority: `changes/active/audit-outbox/spec.md`, revision 4
- Closure authority: `review/r2/verdict.md`, `review/r2/findings-validation.md`,
  `review/r2/TASK-AUDIT-OUTBOX-R2-bounded-remediation.md`, and
  `review/r2/TASK-AUDIT-OUTBOX-R3-commit-outcome.md`

This review is restricted to closure of `FIND-AUDIT-OUTBOX-11`, `-12`, `-13`,
`-14`, `-7`, and `-3`, plus regressions introduced by the reviewed range. It
does not reassess earlier accepted code except where the range could lose,
duplicate, misattribute, or cross tenants with an audit decision. The committed
abandoned `review/r3/` reports were not used as review evidence. `HEAD` remained
the candidate throughout this review.

## Producer-to-sink and lifecycle trace

### Generic outbox state

`Outbox::stage` increments the pending count and gauge before sending under the
queue read lock (`crates/shared/wyrd-runtime/src/outbox.rs:143-162`). Shutdown
takes and drops the sender under the write lock before its first await
(`outbox.rs:189-227`), so a staged item is either inside the pre-fence drain set
or refused and counted after the fence. The writer groups by tenant, removes one
tenant backlog into a child task, and records that task's tenant and count
(`outbox.rs:304-345`). Ordinary errors and ordinary panics while constructing or
polling `sink.write` return the owned items to the parent; `finish` restores
them ahead of later same-tenant work (`outbox.rs:391-422`). Success releases the
pending count; deadline abandonment leaves the count for `shutdown` to exchange
to zero and count once.

The panic invariant still has one reachable hole. Panic containment ends before
the child task normalizes the sink error and before caught values are destroyed:
`contain_panic(write).await` is followed by `error.to_string()` and panic-payload
destruction in the child task (`outbox.rs:328-342`). `OutboxSink::Error` is only
bounded by `Display`; a sink error whose `Display` panics therefore escapes the
child task. Likewise, a caught panic payload or sink future with a panicking
destructor can escape when dropped outside the inner polling catch. The parent
then receives `JoinError` without the child-owned items and explicitly counts
and releases them as lost (`outbox.rs:382-387`). That loss is neither abrupt
process loss nor deadline abandonment. This is the same ownership source as
`FIND-AUDIT-OUTBOX-12`, not a new unrelated hardening concern.

### Audit transaction and retained result

`AuditSink::write` acquires a tenant-bound `TenantConn`, obtains the xid8 in the
same transaction, performs the sole canonical append, and commits
(`crates/vala/vala-sql/src/audit_outbox.rs:128-158`). A normal commit returns
success. If commit acknowledgement fails, `resolve_commit` asks
`pg_xact_status` through a fresh pool checkout without calling the append again
(`audit_outbox.rs:65-125`):

- `committed` returns success, so the generic writer releases the batch without
  resending it;
- `aborted` returns the original error, so the generic writer restores the
  batch to the front of only that tenant's queue;
- in-progress, an unexpected status, or an unavailable Postgres repeats only
  the status query after 50 ms-to-5 s backoff; and
- `NULL` increments the audit lost counter and returns success, so no resend is
  possible and the generic pending count is released.

The canonical append derives sequence and hashes after locking the current
tenant's chain head and inserts under the same `TenantConn`
(`crates/vala/vala-sql/src/queries/audit_staging.rs:69-175`). The publisher still
reads the staged sequence range and projects it once under its frozen range and
stable batch identity. Event-ID staging, projection, harness collapse, and the
event-ID migration are absent from the final candidate. Thus the revision-4
source path has neither a producer of duplicate IDs nor a sibling reader that
silently hides duplicate retained decisions.

The production-writer/publisher journey cuts three commit acknowledgements,
waits for publication and retirement between them, then cuts a commit before
Postgres sees it (`crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs:820-1054`).
It observes one retained decision for each committed transaction, one retained
decision after the aborted transaction is retried, and a retained row count
equal to the chain head. This exercises the producer, writer, publisher, and
retained sink rather than a substitute sink.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-AUDIT-OUTBOX-11`; rev-4 REQ-009 / AC-009: never resend a committed batch; retry an aborted batch; repeated ambiguous commits remain exactly once after retirement | Xid is read in the append transaction and `resolve_commit` gates every retry on `pg_xact_status` (`audit_outbox.rs:65-158`); event-ID producer, projection, collapse, and migration are removed | Journey `ambiguous_audit_commits_retain_each_decision_exactly_once` covers three committed/lost acknowledgements across retirement plus one aborted commit (`audit_publication.rs:976-1054`) | **PASS** |
| REQ-009 wait and `NULL` states | The status loop contains no append or resend; `NULL` counts the batch lost and returns success (`audit_outbox.rs:90-124`) | No direct branch-specific test; the `CommitLost` round may transiently observe in-progress but does not assert it | **PASS by source trace; residual proof limit** |
| `FIND-AUDIT-OUTBOX-12`; REQ-003 / REQ-003a / REQ-008: a sink-task panic preserves and retries the batch; accepted work is lost only at approved boundaries | Poll-time sink panics are caught, but error formatting and destruction remain outside containment; `JoinError` counts/releases the child-owned batch (`outbox.rs:328-342,382-387`) | `a_panicking_write_is_retried_once_in_order_without_loss` proves an ordinary async-body panic only; the seven focused outbox tests passed in this review | **FAIL — `INV-R3-001`** |
| `FIND-AUDIT-OUTBOX-13`; REQ-007 / AC-008: one-way shutdown admission fence and terminal pending/gauge settlement | Queue sender is taken before the first await; post-fence staging is refused; deadline abandonment swaps pending to zero and settles the gauge (`outbox.rs:143-162,189-227`) | Focused tests cover the stage/shutdown race and exact deadline loss; all seven outbox tests passed | **PASS** |
| `FIND-AUDIT-OUTBOX-14`: `tests.rs` basename cannot bypass the unwrap gate | Only four explicit cfg-test module paths are exempt (`scripts/check_unwrap_audit.py:31-55`) | Identical allowlisted and production `tests.rs` fixtures pass; direct checker execution passed | **PASS** |
| `FIND-AUDIT-OUTBOX-7`: changed declarations use imported bare names | `MutexGuard` is imported in the outbox test module and `Uuid` at audit-staging module scope (`outbox.rs:475-490`; `audit_staging.rs:14-20,181-210`) | Source inspection; recorded lint evidence was not independently rerun here | **PASS** |
| `FIND-AUDIT-OUTBOX-3`: live security authority names the shared owner | Security operations names `audit outbox write failure` (`architecture/wyrd-security-posture.md:420-426`) | Focused source inspection | **PASS** |
| No range regression in audit identity, ordering, tenancy, or retention | Tenant id selects `TenantConn`; retry stays on the same tenant; append locks and advances one tenant chain; publisher projection retains the same canonical fields without event-ID collapse | Two-replica/chain and tenant-independence Postgres tests remain; exact-once journey covers publisher retirement | **PASS**, subject to `INV-R3-001`'s loss path |

## Requested risk judgments

- **Untested in-progress/unreachable and `NULL` branches:** not a separate
  implementation finding. The loop is local and has no path back to
  `append_audit_events`; `NULL` has a single counted-loss terminal. The absence
  of direct proof is a real verification limit, especially for recovery after
  status-query unavailability, but AC-009's required committed, aborted, and
  repeated-retirement cases are exercised through the production writer and
  publisher.
- **One waiting batch occupies one of four writer slots:** accepted by the
  approved bounded-concurrency shape. It occupies a generic tenant-write slot,
  not a Postgres connection while sleeping. One unresolved tenant leaves three
  slots for other tenants. If four transactions are simultaneously unresolved,
  no fifth dispatches until an outcome resolves; when Postgres is unreachable,
  a fifth write could not commit either. This does not introduce duplicate,
  reordered, or cross-tenant audit state.
- **Deleted migration:** the removed event-ID migration is unreleased and exists
  only to implement the revision-2/3 design that revision 4 explicitly rejects.
  Removing it with all event-ID producers and consumers leaves a coherent fresh
  migration history and avoids retaining a forbidden schema column. No rollback
  or compatibility migration is required for an unreleased schema.

## Proposed findings

### INV-R3-001 — `FIND-AUDIT-OUTBOX-12` remains open after partial panic containment

- **Classification:** `INCORRECT`
- **Violated obligation:** `FIND-AUDIT-OUTBOX-12`; REQ-003, REQ-003a, and
  REQ-008 require a panicking sink batch to remain queued and limit accepted
  loss to abrupt process stop or an expired graceful-shutdown deadline.
- **Exact location:** `crates/shared/wyrd-runtime/src/outbox.rs:328-342` and
  `:382-387`.
- **Evidence:** only construction and polling of `sink.write` occur inside
  `catch_unwind`. Sink-error `Display` conversion and destruction of the sink
  future/caught panic payload can still panic after containment. The resulting
  `JoinError` no longer carries `items`; `finish` counts and releases their
  count instead of restoring the batch.
- **Observable consequence:** a reachable sink-task panic outside the narrow
  poll catch loses every accepted item in that tenant batch while the process
  continues, increments the loss metric outside an approved loss boundary, and
  permits later same-tenant decisions to commit without the lost decisions.
- **Required testable correction:** keep the batch owned inside the child while
  containing the complete sink-controlled outcome lifecycle, including error
  normalization and destruction, so every non-deadline sink panic returns the
  original items as an ordinary failed write for front-of-queue retry. A focused
  test sink whose returned error panics from `Display` (or whose caught panic
  payload panics on drop) must prove the writer survives, the original batch
  commits once ahead of later same-tenant items, the failure counter increments,
  and the loss counter remains zero. Preserve deadline abandonment and ordinary
  retry behavior.

## Verification notes

- `mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(/^outbox::tests::/)'`:
  7 passed.
- `mise exec -- python3 scripts/test_check_unwrap_audit.py`: passed.
- `python3 scripts/check_unwrap_audit.py`: passed.
- `git diff --check cf5ee4128..52e1144b5`: passed.
- The canonical `mise run check:unwrap-audit` wrapper could not acquire its uv
  cache lock because that cache path is read-only in this review sandbox; the
  same checker was run directly and passed.
- The Postgres journey and broader recorded lanes were inspected in source but
  not independently rerun by this reviewer. `bench:capacity` and `mise run gate`
  remain deferred by user direction.

## Overall result

**FAIL** — revision-4 `FIND-AUDIT-OUTBOX-11` and findings `-13`, `-14`, `-7`,
and `-3` are closed, but `FIND-AUDIT-OUTBOX-12` remains open as
`INV-R3-001`.
