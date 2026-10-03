# Concurrency and resource-ownership domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-aad682fbca5074900`
- Base: `cf5ee4128ce0b842e00a0eb20770ab5c285dedd8`
- Candidate: `52e1144b5c186ccacd85d9c779a4e60c3cce5ac2`
- Reviewed range: `cf5ee4128ce0b842e00a0eb20770ab5c285dedd8..52e1144b5c186ccacd85d9c779a4e60c3cce5ac2`
- Approved authority: `changes/active/audit-outbox/spec.md`, revision 4
- Prior findings and remediation authority: `review/r2/verdict.md`, `review/r2/findings-validation.md`, `review/r2/TASK-AUDIT-OUTBOX-R2-bounded-remediation.md`, and `review/r2/TASK-AUDIT-OUTBOX-R3-commit-outcome.md`

`HEAD` matched the immutable candidate before source review, focused verification,
and report writing. The repository has no `.codegraph/` directory. The abandoned
review artifacts from the earlier superseded r3 attempt were not used as
evidence.

## Reviewed boundary

This review traced the concurrency and lifecycle boundary relevant to the
user-directed closure scope:

- the `Outbox` handle's `RwLock<Option<UnboundedSender<_>>>` admission fence,
  pending atomic, gauge, idle notification, abandonment token, and tracked
  writer lifecycle in `crates/shared/wyrd-runtime/src/outbox.rs:83-227`;
- writer dispatch, one-in-flight-write-per-tenant enforcement, the four-slot
  concurrency bound, retry ordering/backoff, panic containment, `JoinSet`
  completion, and terminal release accounting in
  `crates/shared/wyrd-runtime/src/outbox.rs:230-468`;
- the audit sink's fixed four-writer topology and failed-commit resolution on a
  fresh pool checkout in `crates/vala/vala-sql/src/audit_outbox.rs:27-158`;
- tenant-bound transaction acquisition and consuming commit ownership in
  `crates/wyrd/wyrd-sql/src/tenant_conn.rs:25-73`;
- server ownership and shutdown ordering: one shared process outbox is composed
  into `AppState` (`crates/wyrd/wyrd-server/src/state.rs:1571-1641,2176-2222`),
  request/Bifrost producers drain before the outbox fence, and the same process
  deadline controls final outbox abandonment
  (`crates/wyrd/wyrd-server/src/app/server.rs:853-887`);
- the production-writer/publisher ambiguity journey and its commit-cut proxy in
  `crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs:843-1054`;
- the generic panic, retry, cross-tenant, shutdown-fence, deadline-loss, pending,
  and gauge tests in `crates/shared/wyrd-runtime/src/outbox.rs:640-823`.

Governing authority included `AGENTS.md` sections 2, 5, 6, 11, and 12;
`architecture/agent-rules.md` audit, tenancy, async, and verification rules;
`architecture/bifrost-design.md` audit durability/publication and lifecycle
authority; and the approved revision-4 REQ-002, REQ-003, REQ-003a, REQ-007,
REQ-008, REQ-009, AC-007, AC-008, and AC-009.

## Closure assessment

| Closure item | Concurrency/resource evidence | Result |
|---|---|---|
| `FIND-AUDIT-OUTBOX-11` under revision 4 | `AuditSink::write` obtains the transaction ID before append and does not return the batch to the generic retry owner after a failed commit until `resolve_commit` reports `aborted` (`audit_outbox.rs:83-124,144-157`). `committed` becomes success, `aborted` becomes the ordinary front-of-queue retry, unresolved status stays inside the same sink future without re-send, and `NULL` records terminal loss and returns success so the batch cannot be reissued. The journey performs three acknowledgement-loss rounds with publication/retirement between them, then one pre-commit loss, and verifies one retained row per decision plus a gap-free retained prefix (`audit_publication.rs:976-1054`). | **CLOSED** |
| `FIND-AUDIT-OUTBOX-12` | Future construction and every poll occur under `catch_unwind` while the spawned child still owns `items`; a recovered panic is returned with the vector and follows the same retry/backoff/front-of-queue path as a sink error (`outbox.rs:304-345,368-423,435-454`). The focused test proves the original batch stays pending, precedes a later same-tenant item, commits once, increments failure once, loses nothing, and settles both pending and gauge (`outbox.rs:764-788`). The remaining `JoinError` branch covers task failures outside the specifically required sink-future construction/poll containment and is not the former reachable sink-panic loss path. | **CLOSED** |
| `FIND-AUDIT-OUTBOX-13` | `stage` holds the queue read lock through count, gauge, and send; `shutdown` takes and drops the only sender under the write lock before its first await (`outbox.rs:143-161,189-227`). Therefore a racing stage linearizes wholly before or wholly after the fence. Deadline abandonment first stops and drops the writer/its `JoinSet`, then atomically swaps pending to zero, counts the exact remainder once, decrements the gauge, and wakes settlers. The focused tests cover pre-fence drain/post-fence refusal and exact deadline loss with zero terminal pending/gauge (`outbox.rs:732-762,790-823`). | **CLOSED** |
| Same-tenant ordering and cross-tenant dispatch | `in_flight` prevents a second write for the same tenant; failure prepends its returned vector ahead of later queued work; elapsed backoff and slot availability control dispatch (`outbox.rs:311-365,391-423`). Existing focused tests exercise order and a failing tenant with concurrency one (`outbox.rs:640-693`). | **PASS — no regression** |
| Cancellation and shutdown ownership | The outer tracked writer is the only owner of the receiver, waiting maps, retry state, and `JoinSet`. Deadline cancellation selects the biased abandon branch; dropping that owner cancels in-flight children before `shutdown` settles the shared pending count. Production shuts down request and Bifrost producers before invoking this fence (`server.rs:853-878`). | **PASS — no regression** |
| `FIND-AUDIT-OUTBOX-7` concurrency-adjacent declarations | The changed generic test declaration imports and uses bare `MutexGuard`, and the audit row arrays import and use bare `Uuid`; no concurrency owner/type regression was introduced (`outbox.rs:475-490,546-552`; `audit_staging.rs:14-20,181-210`). | **CLOSED in the reviewed boundary** |

## Directed risk judgments

### Unresolved-status and `NULL` branches lack direct tests

This is a real verification limit, but not a material closure finding. The
revision-4 acceptance proof explicitly requires the committed, aborted, and
repeated ambiguity-with-retirement outcomes; the production journey exercises
those through the real writer and publisher. The unasserted branches are small
and source-complete: every nonterminal status/query error takes only the bounded
50 ms-to-5 s polling backoff without returning the batch, while `NULL` performs
the required loss increment and returns success so neither the generic retry
path nor publisher can create a duplicate (`audit_outbox.rs:90-124`). There is
no downstream state transition between the status result and those actions.
The absence of deterministic direct injection for `in progress`, connection
unreachability, and `NULL` should remain recorded as a verification limit; it
does not invalidate AC-009's existing direct proof.

### An ambiguous batch occupies one of the four writer slots

This is acceptable bounded behavior under the approved topology, not a
correctness or regression issue. `AUDIT_WRITER_CONNECTIONS` is four
(`audit_outbox.rs:27-32`), and an unresolved transaction remains the one
in-flight write for its tenant, occupying one generic dispatch slot while
`pg_xact_status` is retried. That preserves the more important REQ-009 rule that
neither the batch nor later same-tenant work is re-sent before its outcome is
known. One ambiguous tenant leaves three slots available, satisfying the
specified one-contended-tenant isolation. Four simultaneous unresolved commits
can consume all four slots, but that is the direct bounded consequence of four
concurrent uncertain writes; during database unreachability no tenant could
commit through another slot, and after recovery each resolver uses the shared
Vala pool (default 16 connections) and releases its slot as soon as the status
is known (`vala-sql/src/postgres.rs:64-159`). Freeing a slot while its sink call
remains unresolved would require a second concurrency/state owner and would
weaken the current one-write-per-tenant ordering unless the specification chose
new concurrency semantics.

### Deleted unreleased migration

Deletion is the required revision-4 rollback of the superseded event-ID model,
not a persistent-data or lifecycle regression. The migration was introduced
only by the still-active, unreleased change and revision 4 explicitly requires
no event-ID column. Keeping it would make the candidate violate REQ-009's wire
and retained-state contract. No compatibility migration is warranted for an
unreleased schema; previously initialized development databases may need their
normal test/database reset, but no shipped deployment contract is being
removed.

## Material proposed findings

None. The concurrency/resource-ownership finding ledger is empty.

## Verification limits

- Independently run: `mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(/^outbox::tests::/)'` — **7/7 passed**.
- Independently run: `git diff --check cf5ee4128ce0b842e00a0eb20770ab5c285dedd8..52e1144b5c186ccacd85d9c779a4e60c3cce5ac2` — **passed**.
- The remediation record reports the production commit-outcome journey, the
  SQL integration lane, Bifrost journey lane, `test:wyrd`, format, lints,
  docs, and unwrap-audit checks green. This domain reviewer did not rerun the
  Postgres journey or broad lanes.
- The in-progress/unreachable status loop and `NULL` loss branch have no
  deterministic direct assertion, as assessed above.
- `mise run bench:capacity`, `FIND-AUDIT-OUTBOX-5`, and `mise run gate` remain
  deferred to integration by explicit user direction and were not treated as
  review failures.

## Overall result

**PASS.** Within the user-directed closure scope, the range closes the
concurrency/lifecycle portions of `FIND-AUDIT-OUTBOX-11`, `-12`, `-13`, and
`-7`, introduces no concurrency, cancellation, accounting, tenant-dispatch, or
resource-ownership regression, and the three directed risks are either bounded
approved behavior or explicit non-blocking verification limits rather than
material defects.
