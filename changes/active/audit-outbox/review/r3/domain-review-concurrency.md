# Concurrency and lifecycle domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-aad682fbca5074900`
- Base: `cf5ee4128ce0b842e00a0eb20770ab5c285dedd8`
- Candidate: `e54b1244f32950d1ab251dae6530c4e1694c78d5`
- Approved specification: `changes/active/audit-outbox/spec.md`, revision 3
- Prior review: `changes/active/audit-outbox/review/r2/`
- Remediation task: `changes/active/audit-outbox/review/r2/TASK-AUDIT-OUTBOX-R2-bounded-remediation.md`

The repository has no `.codegraph/` directory. `HEAD` matched the candidate at
the start of this review. This report is limited to closure of
`FIND-AUDIT-OUTBOX-12` and `FIND-AUDIT-OUTBOX-13`, their concurrency
interaction with revision 3's `FIND-AUDIT-OUTBOX-11` outcome, and regressions
introduced by the remediation range. It does not reopen earlier passed code.

## Reviewed boundary

The review traced:

- `Outbox::stage`, `settle`, and `shutdown`, including the sender `RwLock`,
  the admission fence, concurrent stage/shutdown linearization, concurrent
  shutdown calls, cancellation, the common deadline, pending accounting, the
  `outbox_pending` gauge, and terminal loss accounting;
- `OutboxWriter::run`, receive/dispatch order, one in-flight write per tenant,
  retry-front insertion, backoff, `JoinSet` completion, item ownership, unwind
  containment, and abandonment;
- the production `AuditSink`, the only shipped `OutboxSink`, and the
  production-shaped unknown-commit test sink;
- the actual server shutdown order, which drains request/Bifrost producers
  before the process audit outbox; and
- the focused generic tests plus the unknown-outcome publication journey.

No production Eval outbox consumer exists in this candidate. The approved
verified-change authority explicitly says the initial Eval change has no Eval
outbox, so the generic owner and its tests are currently the only evidence for
that future consumer.

## Authority coverage

| Boundary | Governing authority inspected | Result |
|---|---|---|
| Accepted loss, panic retry, order, and backoff | Spec rev. 3 REQ-003, REQ-003a, REQ-008, AC-008; r2 `FIND-AUDIT-OUTBOX-12` and its remediation | Ordinary sink polling panic is retried in order; one remaining escaped-unwind path still loses the batch (`CONC-R3-001`) |
| Shutdown fence and terminal accounting | Spec rev. 3 REQ-007, REQ-008, AC-007, AC-008; r2 `FIND-AUDIT-OUTBOX-13` and its remediation | PASS |
| Audit unknown-outcome interaction | Spec rev. 3 REQ-009 and AC-009; AGENTS.md and `architecture/agent-rules.md` one-path/non-blocking audit rules | PASS for concurrency/lifecycle: the same owned event stays ahead of later tenant work, and post-retirement duplication remains identifiable as revision 3 permits |
| Async ownership and lifecycle | AGENTS.md §§5-6, 11-12; `architecture/references/languages/maintainer-style.md`; `architecture/references/languages/spec-driven-development.md` | One finding below |
| Server composition | `architecture/wyrd-design.md`, `architecture/bifrost-design.md`, `crates/wyrd/wyrd-server/src/app/server.rs` | PASS: producers drain before the final audit-outbox fence and wait |

## Source coverage

| Area | Source and tests inspected | Assessment |
|---|---|---|
| Generic outbox | `crates/shared/wyrd-runtime/src/outbox.rs` in full and its base-to-candidate diff | FIND-13 closes; FIND-12 closes the tested polling-panic path but not every unwind the child task admits |
| Audit sink | `crates/vala/vala-sql/src/audit_outbox.rs` | The sink uses the generic owner and canonical append; no second queue or lifecycle owner was introduced |
| Unknown outcome | `crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs:821-1030` | Failed acknowledgement keeps the original items owned, blocks retry deterministically, preserves same-tenant order, and settles to zero |
| Server shutdown | `crates/wyrd/wyrd-server/src/app/server.rs:744-878` | Request, MCP, gateway, and Bifrost producers drain before `audit_outbox.shutdown(deadline)` |
| Eval consumer | `changes/active/verified-change-contract/architecture/verifier/eval.md:90-111`; repository-wide `OutboxSink` implementation search | No production Eval outbox exists in the candidate |

## Closure assessment

| Obligation | Evidence | Result |
|---|---|---|
| FIND-12: a sink panic retains its batch, retries at the tenant front with backoff/write-failure accounting, and records no live-process loss | `dispatch` catches future construction and polling panics while it still owns `items`; `finish` requeues the returned vector ahead of later items. `a_panicking_write_is_retried_once_in_order_without_loss` passes. However, teardown and error/panic formatting occur outside the unwind boundary, and `finish` still deliberately loses the batch for the resulting `JoinError`. | **FAIL — `CONC-R3-001`** |
| FIND-13: shutdown fences intake before waiting | `Outbox::shutdown` takes and drops the sole sender under the write lock before its first await; `stage` holds the read lock through send. Thus every send linearizes before the fence or is refused after it. | PASS |
| FIND-13: post-fence stages are refused and counted without entering pending | `stage` increments pending/gauge only while a sender exists and reverses both on a closed receiver; the no-sender path increments only loss. `shutdown_refuses_items_staged_after_it_begins` proves the late item does not reach the sink. | PASS |
| FIND-13: deadline abandonment is exact and clears terminal pending/gauge | Shutdown cancels abandonment, waits for writer destruction (including in-flight cancellation), then atomically swaps pending to zero, decrements the gauge by exactly that value, counts it once, and notifies settlers. Concurrent shutdown callers share the writer wait and only one can receive the nonzero swap. | PASS |
| FIND-11 interaction: retry ownership/order survives unknown commit and publisher retirement | The child task returns the same `StagedAuditEvent` vector on an ordinary reported error; `finish` prepends it to later same-tenant work. The journey proves both the still-staged and retired-row cases and ends with `pending() == 0`. | PASS within this domain; audit-read surface breadth is outside this concurrency report |
| Range regression boundary | The sender change preserves non-blocking staging, one in-flight batch per tenant, bounded tenant concurrency, healthy-tenant progress, retry timing, and server drain order. No additional lifecycle regression was found. | PASS apart from `CONC-R3-001` |

## Material finding

### CONC-R3-001 — the panic boundary still permits a child-task unwind to destroy its owned batch

- **Classification:** INCORRECT; `FIND-AUDIT-OUTBOX-12` remains partially open.
- **Violated obligation:** REQ-003, REQ-003a, REQ-008, AC-008, and the r2
  correction require a sink-task panic to preserve item ownership and enter the
  existing retry path. Accepted items may be lost only on abrupt process stop
  or the graceful-shutdown deadline.
- **Exact location:** `crates/shared/wyrd-runtime/src/outbox.rs:328-342,
  368-389,435-453`.
- **Evidence:** `contain_panic` catches only calls to `Future::poll`. After a
  caught poll panic, the pinned sink future is dropped when `contain_panic`
  returns, outside `catch_unwind`. Converting an ordinary sink error with
  `error.to_string()` and formatting/dropping the caught panic payload also
  occur outside any unwind boundary. Each is code executed by or over types
  supplied by the public `OutboxSink` contract and can panic. Such a panic
  escapes the child task, so `JoinSet` returns `JoinError`; `finish` then uses
  only the `(tenant, count)` side record, increments
  `outbox_events_lost_total`, releases pending, and discards the sole item
  vector. The source documentation itself acknowledges this reachable loss
  path for an escaped panic.
- **Observable consequence:** a legal generic sink whose future panics while
  being dropped, whose error `Display` panics, or whose panic payload panics on
  drop can lose accepted audit/evaluation work while the process and writer
  continue. A later same-tenant batch can pass the lost decision and
  `settle()` can report zero, recreating the exact result FIND-12 prohibited.
  The current `AuditSink` does not intentionally define any of those panics,
  but REQ-008 makes the generic owner—not each sink—the owner of this
  guarantee, and the future Eval sink is not yet present to narrow it.
- **Testable correction:** keep the existing writer, `JoinSet`, tenant queues,
  and retry path, but make the child-task unwind boundary cover the complete
  sink attempt lifecycle, including future teardown and conversion of a sink
  error or panic into the writer's non-panicking failure value, before the
  task relinquishes the item vector. Then delete the live-process loss behavior
  for a sink-originated `JoinError` or make that state unreachable by
  construction. Add focused generic cases whose sink future panics during
  teardown and whose error formatting panics; prove the original batch remains
  pending, increments the write-failure metric rather than the loss metric,
  retries once ahead of a later same-tenant item, and returns pending/gauge to
  zero. Preserve deadline cancellation as the terminal loss boundary.

## Verification limits

- Independently ran `mise exec -- cargo nextest run --locked -p wyrd-runtime
  --lib -E 'test(/^outbox::/)'`: all 7 focused tests passed.
- The passing panic test covers a conventional panic during future polling.
  It does not enter the teardown or formatting unwind paths described in
  `CONC-R3-001`.
- The Postgres-backed audit-publication journey and recorded broader lanes were
  inspected from source and implementer evidence but not rerun by this domain
  reviewer.
- `mise run bench:capacity` and `mise run gate` remain explicitly deferred to
  integration and are not treated as domain-review failures.
- Schema fingerprint compatibility and shipped audit-reader deduplication are
  assigned to the persistent-data and surface reviewers, respectively; this
  report assessed only their concurrency/lifecycle interactions.

## Overall result

**FAIL**

`FIND-AUDIT-OUTBOX-13` is closed: shutdown has a one-way admission fence,
drains only pre-fence work to the deadline, and settles pending/gauge/loss
accounting coherently. The normal unknown-outcome retry continues to own and
order the same events, including revision 3's allowed post-retirement retained
duplicate. `FIND-AUDIT-OUTBOX-12` is not fully closed because sink-controlled
teardown and formatting can still unwind beyond the containment boundary and
the remaining `JoinError` branch intentionally records live-process loss.
