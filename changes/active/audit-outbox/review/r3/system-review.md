# Audit outbox r3 system-resilience review

## Immutable subject and scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-aad682fbca5074900`
- Base: `cf5ee4128ce0b842e00a0eb20770ab5c285dedd8`
- Candidate: `52e1144b5c186ccacd85d9c779a4e60c3cce5ac2`
- Reviewed range: `cf5ee4128ce0b842e00a0eb20770ab5c285dedd8..52e1144b5c186ccacd85d9c779a4e60c3cce5ac2`
- Authority: approved `changes/active/audit-outbox/spec.md` revision 4; repository rules; Bifrost design, security posture, operations runbooks, and the r2 findings and two remediation tasks named by the caller.
- Closure scope: `FIND-AUDIT-OUTBOX-11`, `-12`, `-13`, `-14`, `-7`, and `-3`, plus regressions introduced by the range. `FIND-5`, `bench:capacity`, and `mise run gate` remain deferred by user direction.

`HEAD` was the stated candidate immediately before this report was written. The committed but abandoned prior `review/r3/` reports were not used as evidence.

## Deployed runtime path

| Path stage | Source evidence | Runtime and recovery assessment |
|---|---|---|
| Request decision and process staging | Audited request owners call `AppState::audit_outbox.stage`; boot constructs exactly one process outbox at `crates/wyrd/wyrd-server/src/boot/mod.rs:1021-1023`. `Outbox::stage` fences queue admission under its handle-owned lock at `crates/shared/wyrd-runtime/src/outbox.rs:143-162`. | Requests enqueue without awaiting Postgres. Shutdown takes the sender before its first await, so accepted pre-fence work drains and later stages are refused and counted. This closes the admission race in `FIND-13` for the generic owner. |
| Generic writer and tenant isolation | `OutboxWriter::dispatch` owns one batch per tenant and at most the configured number of tenant tasks at `outbox.rs:304-345`; `AuditSink::outbox` fixes audit concurrency at four at `crates/vala/vala-sql/src/audit_outbox.rs:27-32,51-63`. | Same-tenant later work cannot overtake the in-flight batch. Independent tenants retain the remaining writer slots. Returned errors and ordinary sink panics are placed back at the tenant front with bounded backoff. One residual live-process loss path remains (`SYS-R3-001`). |
| Tenant transaction and commit-outcome resolution | `AuditSink::write` acquires a `TenantConn`, records `pg_current_xact_id()`, appends through the one private canonical append, and resolves a failed commit before returning at `audit_outbox.rs:128-158`. `resolve_commit` queries `pg_xact_status` through the pool at `audit_outbox.rs:65-125`. | RLS remains the tenant boundary for the append. A confirmed commit returns success and cannot be resent; a confirmed abort returns the original error to the generic retry; in-progress, unexpected, or unreachable status is polled with 50 ms-to-5 s backoff without returning the batch; `NULL` counts the batch lost and returns success so it cannot be resent. This closes revision-4 `FIND-11` by construction. |
| Staging, publication, and retained history | `append_audit_events` serializes the tenant chain head and atomically inserts the batch at `crates/vala/vala-sql/src/queries/audit_staging.rs:50-175`. `AuditPublisher` freezes a durable range, sends it through local Scribe, and settles only after durable append at `crates/wyrd/wyrd-server/src/audit/publication.rs:179-209,259-353`. | A publisher may retire a committed row while the writer resolves the acknowledgement, but the writer no longer retries a committed transaction, so retirement cannot recreate the revision-2/3 duplicate. Publisher crash/cancellation reuses the persisted range and Scribe batch identity. Retained readers need no event-ID collapse. |
| Graceful shutdown and restart | Server transport/task/Bifrost owners drain first, then call the audit outbox last at `crates/wyrd/wyrd-server/src/app/server.rs:744-884`. Generic shutdown fences admission, waits to the shared deadline, cancels unresolved writes at the deadline, counts the exact remaining pending total, and returns pending/gauge to zero at `outbox.rs:189-227`. | Queued decisions commit while time remains. A resolution wait is cancelled only at the approved shutdown deadline and then counted lost. Staged rows already committed survive process replacement and publish after restart; process-local queued or outcome-unresolved rows can be lost only at abrupt stop or the deadline, except for `SYS-R3-001`. |

## Failure and recovery assessment

| Failure | What stops / remains available | Recovery, amplification, and proof |
|---|---|---|
| Tenant append or confirmed-aborted commit | That tenant batch waits at its queue front. Requests and other tenant writers remain available. | Generic backoff is bounded at 5 s; the batch absorbs later same-tenant work and retries in order. Existing generic and Postgres integration evidence covers returned-error recovery and cross-tenant progress. |
| Commit acknowledgement lost after Postgres committed | That tenant writer remains occupied only while it establishes status; it sends no second append. Requests and publisher work remain independent. | The production proxy journey performs three acknowledgement-loss rounds, lets the server publisher retain and retire each decision between rounds, and observes each once. This is credible direct proof for committed ambiguity and AC-009. |
| Commit never reached Postgres | The tenant writer resolves the transaction as aborted and hands the same batch back to generic retry. | The production proxy journey's `CommitLost` round proves one retry, one retained decision, and a gap-free chain. |
| `pg_xact_status` reports in progress or Postgres is unreachable | One of four audit writer slots remains occupied; no database connection is held during the backoff sleep. Three other tenants can progress unless more unresolved transactions consume their slots. Four simultaneous unresolved outcomes can occupy all audit slots, but during a total Postgres outage no audit append could progress anyway. | Status probes are bounded by the same 50 ms-to-5 s backoff, so retry traffic is at most four probes per interval per process after saturation. On recovery, each slot releases as its status resolves. Holding the slot is required to prevent a resend and does not warrant weaker integrity. There is no direct branch-specific test, but the source keeps the batch inside the sink loop and AC-009 does not require a separate availability harness for this branch. No finding. |
| Transaction status is `NULL` | The affected decisions are not resent; other tenant work continues when the slot returns. | `resolve_commit` increments `outbox_events_lost_total{outbox="audit"}`, logs tenant/xact/count, and returns success. This is the explicit revision-4 terminal for status retention loss. It lacks a direct injected test, but the branch is small, source-local, and not part of AC-009's required committed/aborted ambiguity proof. No finding. |
| Publisher or Scribe failure | Retained history lags; request staging and committed Postgres history continue. | Frozen bounds survive and later sweeps/restarts reuse the same range and Scribe batch-id fence. Publisher concurrency is separately bounded at eight tenant cycles. |
| Graceful deadline | Admission is already fenced; the writer and any status-resolution wait are cancelled at the deadline. | All remaining accepted events are counted once, pending and gauge settle to zero, and server shutdown reports the remainder. Focused shutdown tests pass. |
| Abrupt process/pod loss | Process-local queued decisions disappear. Committed staging, publication watermark, and frozen publisher bound survive. | This is the approved REQ-003a boundary. A replacement publisher resumes retained publication. |
| Sink task panic | Panics during sink-future construction or polling are converted to ordinary retries. A panic after that containment boundary still reaches `JoinError`. | The normal sink-body panic test passes, but the changed `JoinError` branch deliberately releases and loses the batch in the live process. See `SYS-R3-001`. |

## User-directed risk judgments

- **No direct in-progress/unreachable or `NULL`-loss test:** accepted verification limit, not a finding. The production journey directly proves the commit/abort ambiguity outcomes AC-009 requires. Source inspection establishes that unresolved status never returns the batch to the retry owner and that `NULL` is terminal, counted, and non-resending. A dedicated branch injection seam would add test-only mechanism without changing the present deployed conclusion.
- **A resolution wait holds one of four writer slots:** accepted integrity-preserving bound, not a regression. The wait holds a logical writer slot, not a checked-out pool connection during sleep. It limits uncertainty to four batches and avoids resend; remaining slots continue independent tenants. If all four are unresolved because Postgres is unavailable, additional audit writes could not commit against that dependency in any event.
- **Deleted unreleased migration:** accepted and required by revision 4. The event-ID migration belonged only to the superseded, unreleased revisions 2/3 design; the approved revision explicitly removes that column and migration. Keeping it would leave schema drift. This judgment does not authorize deleting a released/applied deployment migration; such a deployment is outside the stated immutable subject. Repository-managed test databases must be recreated if they were previously pointed at the transient active-change commit.

## Prior-finding closure

| Finding | System-resilience result |
|---|---|
| `FIND-AUDIT-OUTBOX-11` | **CLOSED for revision 4.** Commit status is resolved before the generic owner can retry; committed acknowledgement loss plus publisher retirement is proven exactly once. |
| `FIND-AUDIT-OUTBOX-12` | **OPEN.** Ordinary sink construction/poll panics retry correctly, but the generic child still has an admitted panic-to-`JoinError` path that loses accepted work outside the approved loss boundaries (`SYS-R3-001`). |
| `FIND-AUDIT-OUTBOX-13` | **CLOSED.** Shutdown owns a one-way admission fence and terminal pending/gauge settlement; the three focused panic/shutdown tests passed in this review. |
| `FIND-AUDIT-OUTBOX-14` | **CLOSED from this lens.** The checker now uses an exact cfg-test module allowlist, and its standalone fixture passed. The canonical `mise run check:unwrap-audit` could not be executed because its package runner could not create a cache temporary file on the read-only host cache. |
| `FIND-AUDIT-OUTBOX-7` | **CLOSED.** The changed declarations use top-level imports and bare names. |
| `FIND-AUDIT-OUTBOX-3` | **CLOSED.** Live operations/security prose assigns failures to the shared audit outbox rather than Oracle. |

## Material proposed finding

### SYS-R3-001 — A panic outside sink polling still drops an accepted batch in the live process

- **Classification:** `INCORRECT`; closure failure for `FIND-AUDIT-OUTBOX-12`.
- **Violated obligation:** revision-4 REQ-003a permits loss only on abrupt process stop, an expired graceful-shutdown deadline, or the explicit REQ-009 transaction-status `NULL` terminal. The r2 remediation requires a panicking sink task to return its still-owned batch through the existing front-of-queue retry rather than lose it.
- **Exact location:** `crates/shared/wyrd-runtime/src/outbox.rs:328-342,368-388,435-463`.
- **Evidence:** the child catches panic only while constructing or polling `sink.write`. After that boundary it calls `error.to_string()` and drops the returned error/panic payload before returning `(tenant, items, result)`. A panic from an error's `Display` implementation or from cleanup therefore escapes the child. The source itself documents an escaping-panic example at lines 373-375. `finish` then receives `JoinError`, has only `(tenant, count)` in `in_flight`, increments loss, releases pending, and permanently discards the sole batch ownership. This happens while the process is live and before any shutdown deadline.
- **Observable system consequence:** the originating request has already succeeded, but its authorization decision is absent from staging and retained audit forever. The metric records a loss, yet no restart or publisher replay can recover it. Later same-tenant decisions can commit, so the missing decision is not repaired by ordering or hash-chain checks. Today the deployed sink is audit; the generic owner also promises this behavior to its intended second Eval use.
- **Required testable correction:** keep sole batch ownership recoverable until every panic-capable sink-owned step needed to classify the result—including error rendering and cleanup—has crossed the containment boundary. Any such panic must return the original items to the existing `OutboxWriter::finish` failure/retry path, increment `outbox_write_failures_total`, and preserve front-of-tenant order. Do not clone the batch, add another queue, or turn a sink failure into a process crash. Reserve loss counting for the approved deadline/abrupt-stop/status-`NULL` boundaries.
- **Focused closure proof:** use a test sink that returns an error whose `Display` panics (and, if the implementation separately owns panic-payload cleanup, a payload with panicking cleanup). Stage that batch and a later same-tenant item, then prove the writer stays alive, reports one write failure and zero lost events, commits both exactly once in order after recovery, and returns pending/gauge to zero. Re-run the existing ordinary-panic and shutdown tests unchanged.

## Verification notes

- Passed in this review: exact `wyrd-runtime` tests
  `a_panicking_write_is_retried_once_in_order_without_loss`,
  `shutdown_refuses_items_staged_after_it_begins`, and
  `shutdown_counts_items_unwritten_at_the_deadline_as_lost` (3/3).
- Passed in this review: `mise exec -- python3 scripts/test_check_unwrap_audit.py`.
- Not rerun: the Postgres proxy journey and broader SQL/server lanes. Their implementer evidence records the exact production journey, `test:bifrost:integration:sql`, `test:bifrost:journey:server`, and `test:wyrd` green. Source review found the proxy exercises the production `AuditSink`, generic outbox, real publisher, and retained public read.
- Blocked by host filesystem, not a code failure: `mise run check:unwrap-audit` could not create a temporary cache file in the read-only package-runner cache.
- Deferred by user direction: `bench:capacity` / `FIND-5` and `mise run gate`.

## Overall result

**FAIL** — revision-4 commit-outcome reconciliation and the stated operational risks are acceptable, and findings 11, 13, 14, 7, and 3 close. `SYS-R3-001` shows that `FIND-AUDIT-OUTBOX-12` remains open: a reachable child-task panic after the narrow polling catch still loses accepted audit work outside every approved loss boundary.
