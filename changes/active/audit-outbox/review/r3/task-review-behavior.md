# Audit outbox r3 behavior review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-aad682fbca5074900`
- Base: `cf5ee4128ce0b842e00a0eb20770ab5c285dedd8`
- Candidate: `52e1144b5c186ccacd85d9c779a4e60c3cce5ac2`
- Reviewed range: `cf5ee4128..52e1144b5`
- Approved authority: `changes/active/audit-outbox/spec.md`, revision 4
- Closure inputs: `review/r2/verdict.md`, `review/r2/findings-validation.md`,
  `review/r2/TASK-AUDIT-OUTBOX-R2-bounded-remediation.md`, and
  `review/r2/TASK-AUDIT-OUTBOX-R3-commit-outcome.md`

The repository has no `.codegraph/` directory. I reviewed the committed range,
not the abandoned committed r3 reports or their working-tree deletions. `HEAD`
remained the candidate throughout this review.

## Caller-to-result traces

### Unknown commit outcome

An audited surface calls `Outbox::stage`, which increments pending state and
places the tenant decision on the process queue
(`crates/shared/wyrd-runtime/src/outbox.rs:143-162`). The writer gives one
tenant's ordered backlog to `AuditSink::write` while retaining ownership of the
items (`outbox.rs:304-345`). `AuditSink::write` opens a tenant-bound
transaction, obtains its `xid8`, appends the whole batch through the one
canonical RLS-bound append, then commits
(`crates/vala/vala-sql/src/audit_outbox.rs:133-157` and
`crates/vala/vala-sql/src/queries/audit_staging.rs:50-175`).

If commit acknowledgement fails, the sink does not return to the generic retry
owner. It asks `pg_xact_status` on a fresh pool connection until Postgres says
`committed`, `aborted`, or no longer knows the transaction
(`audit_outbox.rs:65-125`). A committed answer becomes success, so the same
items cannot re-enter dispatch; an aborted answer becomes the original error,
so `OutboxWriter::finish` restores the items ahead of later same-tenant work and
backs off (`outbox.rs:368-423`); an in-progress status or unavailable status
connection stays inside the resolver and sends nothing; `NULL` counts the
batch lost and returns success so it is released without resend. The production
publisher continues to project the sequence range without an event-ID column
(`crates/vala/vala-bifrost-redux/src/tables/audit/projection.rs:164-264`).

The journey at
`crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs:820-1055`
uses the production writer and server publisher. Three acknowledgement-loss
rounds each commit, publish, retire, and remain one retained row. A commit cut
before Postgres receives it is retried and retained once. The final retained
count equals the chain head, proving the gap-free, no-duplicate result through
the public query path.

### Generic panic and shutdown closure

`dispatch` catches both sink-future construction and polling unwind while the
child task still owns the batch, converts the contained panic into the same
failure result as an ordinary sink error, and therefore uses the existing
front-of-queue retry path (`outbox.rs:304-345,368-423,435-463`). The focused
panic test proves one later item cannot overtake the recovered batch and that
failure/loss/pending metrics settle correctly (`outbox.rs:764-788`).

`Outbox::shutdown` takes and drops the only sender under the queue's write lock
before its first await. A concurrent `stage` either completes under the read
lock before that fence or observes no sender and is counted lost without
entering pending state. Deadline abandonment joins the writer, atomically takes
the remaining pending count once, decrements the gauge by that exact count, and
wakes idle waiters (`outbox.rs:189-227`). The race and deadline tests exercise
both results (`outbox.rs:732-823`).

## Closure and regression matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-AUDIT-OUTBOX-11`; revision-4 REQ-009/AC-009: resolve commit outcome before retry; committed is not resent, aborted retries once, repeated ambiguous commits separated by retirement retain one decision and a gap-free chain | Transaction ID capture and status resolution at `audit_outbox.rs:65-157`; event-ID staging/dedup removed at `audit_staging.rs:50-175`; retained projection has the canonical 15 columns including managed event time and no event ID at `projection.rs:164-264` | Production-writer/publisher journey at `audit_publication.rs:820-1055`; implementer reports the focused journey and the full server journey passed. My attempted focused rerun was blocked before test execution by unavailable Docker-socket permission. | PASS |
| `FIND-AUDIT-OUTBOX-12`: a sink panic keeps the accepted batch and retries it ahead of later tenant items | Construction/poll containment and ordinary retry path at `outbox.rs:304-345,368-463` | `mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(/^outbox::/)'`: 7/7 passed, including `a_panicking_write_is_retried_once_in_order_without_loss` | PASS |
| `FIND-AUDIT-OUTBOX-13`: shutdown is a one-way admission fence; deadline loss is exact and terminal pending/gauge state is zero | Sender ownership and fence at `outbox.rs:83-162,189-227`; writer terminates after channel close or abandonment at `outbox.rs:266-301` | Same 7/7 outbox run passed, including both shutdown closure tests | PASS |
| `FIND-AUDIT-OUTBOX-14`: a production `tests.rs` is not exempted by basename | Four explicit cfg-test paths at `scripts/check_unwrap_audit.py:31-55`; production scan at `scripts/check_unwrap_audit.py:218-265` | `mise exec -- python3 scripts/test_check_unwrap_audit.py`: passed; `mise run check:unwrap-audit` with a writable temporary uv cache: passed; each allowlisted owner was inspected and has `#[cfg(test)] mod tests;` | PASS |
| `FIND-AUDIT-OUTBOX-7`: changed declarations use module imports and bare type names | `MutexGuard` import/use at `outbox.rs:475-480,546-552`; `Uuid` import and bare `AuditRows` fields at `audit_staging.rs:14-20,181-220` | Focused source inspection; implementer reports `mise run fmt` and `mise run lints` passed | PASS |
| `FIND-AUDIT-OUTBOX-3`: live security authority names the shared owner | `architecture/wyrd-security-posture.md:420-426` names “audit outbox write failure,” not Oracle commit ownership | Focused source search found no stale phrase; implementer reports `mise run docs:check` passed | PASS |
| Regression boundary: event-ID revision 2/3 machinery is removed without changing retained audit meaning or introducing a second writer/reader dedup path | `AuditEvent` is again the sink item at `audit_outbox.rs:128-158`; append and projection contain no audit event ID; harness retained reads no longer collapse by event ID; only the existing publisher remains | Focused production-source search found no audit `StagedAuditEvent` or audit event-ID consumer. `mise exec -- cargo check --locked -p vala-sql`: passed. | PASS |
| Regression boundary: closure edits do not weaken formatting/static integrity or enter unrelated behavior | Range inspected across runtime, SQL, projection, harness, scripts, architecture, docs, and migration registry | `git diff --check cf5ee4128..52e1144b5`: passed | PASS |
| Explicit deferrals | FIND-5 / `bench:capacity` and `mise run gate` are integration-owned by user direction | Not run | DEFERRED |

## Reported risk judgments

- **No direct in-progress/unreachable or `NULL` test:** this is a residual proof
  limit, not a closure finding. The approved AC-009 proof specifically requires
  committed, aborted, and repeated ambiguous commits across retirement; the
  journey exercises those results. The untested branches are direct outcomes
  of the same resolver: `Ok(Some(_)) | Err(_)` sleeps and loops without
  returning the batch, while `Ok(None)` counts exactly `events` lost and
  returns success, which prevents resend (`audit_outbox.rs:97-123`). No
  alternate producer or resend path was found. A direct `NULL` integration
  fixture would require aging a real transaction status out of Postgres, and
  the candidate does not add a new abstraction solely to simulate that state.
- **A waiting batch holds one of four writer slots:** accepted. The slot is the
  existing bounded per-tenant workflow slot, not a database connection held
  across sleep; each status lookup acquires a fresh pooled connection only for
  its query. One unresolved tenant leaves three audit writer slots available,
  preserving the stated single-contended-tenant isolation. Releasing that slot
  would require a second owner for unresolved commit state and is neither
  required nor safer than revision-4's instruction to wait and resend nothing.
- **Deleted unreleased migration:** accepted. The removed migration introduced
  only the superseded event-ID shape, while revision 4 explicitly requires no
  staging or retained event ID. Git history shows it was introduced on this
  active change before release, and the candidate's ordered source registry now
  ends at `20261003000000_audit_publication_progress.sql`. A database that
  locally applied the abandoned active-change migration must be reprovisioned,
  but there is no shipped schema to forward-migrate and retaining or replacing
  the migration would violate revision 4.

## Proposed findings

None. I found no `MISSING`, `INCORRECT`, `DRIFT`, `VIOLATION`, or `REGRESSION`
within the user-directed closure scope.

## Verification limits

- The focused Postgres journey could not be independently rerun in this review
  environment because the repository Postgres wrapper could not access its
  Docker socket. This is an environment limitation, not a source failure. The
  journey source was traced end to end and the implementer recorded a passing
  exact command plus passing `test:bifrost:journey:server` and
  `test:bifrost:integration:sql` lanes.
- `bench:capacity` and `mise run gate` remain explicitly deferred to
  integration.

## Overall result

**PASS** — all six scoped findings are closed against revision 4, and the
reviewed range introduces no behavior regression within the directed boundary.
