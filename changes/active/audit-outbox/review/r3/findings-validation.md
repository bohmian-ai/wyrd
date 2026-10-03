# Structured Ponytail finding validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-aad682fbca5074900`
- Base: `cf5ee4128ce0b842e00a0eb20770ab5c285dedd8`
- Candidate: `52e1144b5c186ccacd85d9c779a4e60c3cce5ac2`
- Reviewed range: `cf5ee4128ce0b842e00a0eb20770ab5c285dedd8..52e1144b5c186ccacd85d9c779a4e60c3cce5ac2`
- Approved authority: `changes/active/audit-outbox/spec.md`, revision 4
- Closure authority: `review/r2/verdict.md`,
  `review/r2/findings-validation.md`,
  `review/r2/TASK-AUDIT-OUTBOX-R2-bounded-remediation.md`, and
  `review/r2/TASK-AUDIT-OUTBOX-R3-commit-outcome.md`

The repository has no `.codegraph/` directory. I inspected the complete
committed range, all required r3 discovery and follow-up reports, the cited
owners and callers, the prior r2 ledger and remediation tasks, and the
applicable repository authority. The committed abandoned r3 report content was
not treated as evidence; the working review reports supplied for this pass were
validated against source. `HEAD` matched the candidate before this report was
written.

## Validation status

**COMPLETE — three bounded findings remain.** Revision-4
`FIND-AUDIT-OUTBOX-11` and prior findings `-13`, `-14`, and `-3` are closed.
`FIND-AUDIT-OUTBOX-12` remains narrowly open at a new error-normalization panic
site, and the range repeats `FIND-AUDIT-OUTBOX-7` in its closure journey. One
new hard documentation violation is retained as `FIND-AUDIT-OUTBOX-15`.

## Producer-to-consumer validation

### Generic outbox ownership

`Outbox::stage` increments pending state and sends while holding the queue read
lock; `Outbox::shutdown` takes the only sender under the write lock before its
first await (`crates/shared/wyrd-runtime/src/outbox.rs:143-227`). A racing stage
therefore linearizes before or after the shutdown fence. Deadline abandonment
stops the writer before atomically taking the remaining pending count, settling
the gauge, and waking waiters. The two new shutdown tests directly exercise the
admission and accounting outcomes, so `FIND-AUDIT-OUTBOX-13` is closed.

`OutboxWriter::dispatch` removes the sole item vector from `waiting` and moves
it into a spawned child (`outbox.rs:304-345`). The new containment catches a
panic while constructing or polling `sink.write`; those outcomes return the
same vector and enter the established front-of-queue retry at
`outbox.rs:391-422`. The focused ordinary-panic test proves that path.

The child nevertheless performs one newly introduced sink-controlled step
after containment: `error.to_string()` at `outbox.rs:338-340`.
`OutboxSink::Error` is constrained only by `Display + Send + 'static`; a valid
implementation can return an error whose `Display` panics. That panic escapes
the child while it still owns the only vector. The parent receives `JoinError`
and has only `(tenant, count)`, so `finish` counts/releases the batch as lost
at `outbox.rs:382-388`. This is a live-process loss outside REQ-003a's approved
boundaries and keeps `FIND-AUDIT-OUTBOX-12` open. The broader proposal about
arbitrary panicking destructors is not retained: destructor panics are not
needed to prove the defect and demanding containment of adversarial cleanup
would widen a small correction into a speculative panic-safety framework.

### Audit transaction through retained history

`AuditSink::write` obtains `pg_current_xact_id()` inside the same tenant-bound
transaction, appends through the sole canonical writer, and commits
(`crates/vala/vala-sql/src/audit_outbox.rs:128-158`). Only a returned commit
error enters `resolve_commit` (`audit_outbox.rs:65-125`): `committed` becomes
success, `aborted` returns the original error to the generic ordered retry,
unresolved or unreachable status stays inside the same sink future without an
append, and `NULL` increments the audit loss counter and returns success so the
generic owner cannot resend. The transaction-status input is the internally
produced xid, while the append remains under the original `TenantConn`; no
tenant, attribution, or writer authority crosses that seam.

The production journey at
`crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs:976-1054`
cuts three landed commit acknowledgements, waits for the production publisher
to retain and retire each row, then cuts one commit before Postgres receives
it. It asserts one retained decision per round and a retained row population
equal to the tenant chain head. The staging append, row projection, retained
schema, harness reads, and migration history contain no remaining audit event
ID or reader collapse. This closes `FIND-AUDIT-OUTBOX-11` under revision 4 and
shows no range regression in ordering, attribution, or tenant isolation.

### Static controls and authority

The unwrap checker now exempts only four explicit cfg-test module paths and its
fixtures distinguish an allowlisted module from a production `tests.rs`,
closing `FIND-AUDIT-OUTBOX-14`. `architecture/wyrd-security-posture.md:421-426`
names the shared audit-outbox write failure, closing `FIND-AUDIT-OUTBOX-3`.
The earlier `MutexGuard` and `Uuid` declaration sites are corrected, but the
new revision-4 journey repeats the same unconditional import/declaration rule
at `audit_publication.rs:839-841,915-921`, so `FIND-AUDIT-OUTBOX-7` is not
closed in the cumulative candidate.

## Proposal disposition

| Discovery proposal(s) | Result | Validation |
|---|---|---|
| `INV-R3-001`, `SYS-R3-001`, `FOLLOW-R3-001` | **REVISED** | The ordinary construction/poll panic is correctly contained, but the new `error.to_string()` remains outside containment and can panic for an admitted `OutboxSink::Error`, after which `JoinError` loses the sole vector. Preserve `FIND-AUDIT-OUTBOX-12`. The destructor-panic expansion is rejected as unnecessary and speculative. |
| `STD-R3-001`, `MAINT-R3-03`, `FOLLOW-R3-002` | **CONFIRMED** | The new journey uses fully qualified types in a field and function signature and a function-scoped Tokio IO import, repeating the exact rule tracked by `FIND-AUDIT-OUTBOX-7`. |
| `MAINT-R3-02`, `FOLLOW-R3-003` | **CONFIRMED** | The materially changed `MemorySink::write` and six newly added `Recorder` methods have no rustdoc despite the unconditional hard rule covering private test helpers and methods. New `FIND-AUDIT-OUTBOX-15`. |
| `MAINT-R3-01`, `FOLLOW-R3-004` | **REJECTED** | The changed `AuditSink` overview says a batch that *fails to commit* is retried only after Postgres confirms it did not commit. A `NULL` result does not confirm failure, so the sentence does not promise retry for that branch; `resolve_commit` immediately below documents the terminal counted loss. The stale Bifrost sentence predates the range and does not change audit results, so user scope excludes it. |
| `STD-R3-002` | **REJECTED as a mandatory closure finding** | Revision-4 AC-009 and its remediation task select landed-but-unacknowledged, aborted, and repeated ambiguity across retirement as the required production integration proof; the journey supplies those cases. The wait and `NULL` branches are direct, source-local transitions with no sibling resend path. Adding a production seam or database-aging harness only for those branches would exceed the approved proof contract. Their lack of direct execution remains a verification limit. |
| Behavior, concurrency, persistent-data, and security empty-ledger claims | **REVISED** | Their audit commit-outcome, shutdown, unwrap-check, authority, migration, slot-ownership, attribution, and tenancy conclusions are supported. Their conclusion that `FIND-AUDIT-OUTBOX-12` is fully closed omits the new post-containment error-rendering path. |

## Final deduplicated finding ledger

### FIND-AUDIT-OUTBOX-12 — error rendering can still lose the child-owned batch

- **Discovery sources:** `INV-R3-001`, `SYS-R3-001`, `FOLLOW-R3-001`
- **Status:** `REVISED`
- **Classification:** `INCORRECT`
- **Violated obligation:** REQ-003, REQ-003a, REQ-008, AC-008, and the r2
  remediation require a non-deadline sink-task panic to preserve the accepted
  batch and route it through ordered retry; accepted loss is limited to abrupt
  stop, graceful-shutdown deadline, and revision-4's explicit transaction-
  status `NULL` terminal.
- **Exact location:**
  `crates/shared/wyrd-runtime/src/outbox.rs:328-342,382-388`.
- **Evidence:** construction and polling are contained, but converting
  `S::Error: Display` with `error.to_string()` is not. A panicking `Display`
  unwinds the child before `(tenant, items, result)` is returned. `in_flight`
  retained only tenant and count, so the `JoinError` branch cannot restore the
  vector and instead counts/releases it as lost.
- **Observable consequence:** an accepted batch disappears while the writer
  and process continue, later same-tenant work may commit, the loss counter
  increments outside an approved boundary, and `pending()` reports completion.
- **Decision-complete minimum correction:** keep item ownership outside the
  existing containment boundary but normalize both the sink result and any
  ordinary panic raised while rendering its error inside that boundary. Route
  every contained construction, poll, or error-rendering panic to the existing
  failed-write result so `finish` performs the same front-of-queue retry and
  write-failure accounting. Reuse `catch_unwind`, the existing writer, and the
  existing retry path; do not clone items, add a queue, add a panic abstraction,
  or attempt to guarantee behavior for adversarial panicking destructors.
- **Focused closure proof:** a test sink returns an error whose `Display`
  panics once, then succeeds. Prove the writer remains alive, the original
  batch commits once ahead of a later same-tenant item, write failures increment
  once, lost remains zero, and pending/gauge settle to zero. Re-run the existing
  ordinary-panic and shutdown tests unchanged.

### FIND-AUDIT-OUTBOX-7 — the closure journey repeats the declaration/import violation

- **Discovery sources:** `STD-R3-001`, `MAINT-R3-03`, `FOLLOW-R3-002`
- **Status:** `REVISED`
- **Classification:** `REGRESSION / VIOLATION`
- **Violated obligation:** `architecture/agent-rules.md` requires module-level
  imports and bare names in struct fields and function signatures; this is the
  same repository rule tracked by prior `FIND-AUDIT-OUTBOX-7`.
- **Exact location:**
  `crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs:839-841,915-921`.
- **Evidence:** `CommitCutter::addr` is declared as `std::net::SocketAddr`;
  `relay` declares `tokio::net::TcpStream` parameters and a
  `std::io::Result<()>` return; and the function contains
  `use tokio::io::{AsyncReadExt, AsyncWriteExt};`. None matches an allowed
  function-local-import exception.
- **Observable consequence:** the remediation range claims closure while its
  required proof code repeats the same hard dependency-manifest violation.
- **Decision-complete minimum correction:** extend the existing module import
  block with `SocketAddr`, an unambiguous alias for the IO `Result`,
  `TcpStream`, `AsyncReadExt`, and `AsyncWriteExt`; use the bare names in the
  field and signature and delete the function-scoped import. Do not extract or
  generalize the proxy.
- **Focused closure proof:** focused inspection of the changed proxy plus
  `mise run fmt`, `mise run lints`, and the exact ambiguous-commit journey.

### FIND-AUDIT-OUTBOX-15 — changed and new outbox test methods lack mandatory rustdoc

- **Discovery sources:** `MAINT-R3-02`, `FOLLOW-R3-003`
- **Status:** `CONFIRMED`
- **Classification:** `VIOLATION`
- **Violated obligation:** `AGENTS.md` section 16 and
  `architecture/agent-rules.md` require substantive rustdoc for every new or
  materially modified Rust item, including private test helpers and trait
  implementation methods; fallible, panicking, and async behavior must be
  documented where relevant.
- **Exact location:**
  `crates/shared/wyrd-runtime/src/outbox.rs:519-543,614-629`.
- **Evidence:** `MemorySink::write` was materially changed to signal dispatch,
  wait for an injected hang, consume an injected one-shot panic, and then
  fail or record the batch, but has no item rustdoc. All six methods in the new
  `Recorder for TestMetrics` implementation likewise have no item rustdoc,
  including the intentional no-op descriptor and histogram behavior.
- **Observable consequence:** the exact injection order that makes the panic
  and shutdown proofs meaningful, and the deliberately narrow metric model
  their assertions depend on, are not stated at the owning items; the
  cumulative candidate violates a hard merge rule even though tests pass.
- **Decision-complete minimum correction:** document only the materially
  changed sink method and six new recorder methods. State the sink's signal /
  hang / one-shot panic / failure / write order, its `# Errors`, `# Panics`,
  and cancellation behavior; state for each recorder method whether it records
  by metric name or intentionally does nothing. Do not document unrelated
  untouched code or add a documentation check.
- **Focused closure proof:** source inspection, `mise run fmt`, and
  `mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E
  'test(/^outbox::tests::/)'`.

## Prior-finding closure

| Finding | Validated result |
|---|---|
| `FIND-AUDIT-OUTBOX-11` | **CLOSED under revision 4.** Transaction status gates retry; the production writer/publisher journey proves committed ambiguity, confirmed abort, retirement between repeated ambiguities, one retained decision per round, and a gap-free retained prefix. |
| `FIND-AUDIT-OUTBOX-12` | **OPEN, revised as above.** Construction/poll panics are fixed, but new error rendering can still reach the same live-process loss owner. |
| `FIND-AUDIT-OUTBOX-13` | **CLOSED.** Shutdown owns a one-way admission fence and terminal loss/pending/gauge settlement. |
| `FIND-AUDIT-OUTBOX-14` | **CLOSED.** The basename-wide exclusion is deleted and direct fixtures distinguish cfg-test and production `tests.rs`. |
| `FIND-AUDIT-OUTBOX-7` | **OPEN, revised as above.** The earlier cited declarations are fixed, but the range repeats the same rule in the new closure journey. |
| `FIND-AUDIT-OUTBOX-3` | **CLOSED.** Live security authority names the shared audit-outbox owner. |

## Accepted limits and rejected expansion

- The in-progress/unreachable status wait and `NULL` loss branch have no direct
  test. This remains a material verification limit, not a new closure
  obligation: AC-009's selected production proof is committed ambiguity,
  confirmed abort, and repetition across publisher retirement, all of which the
  journey covers. Source tracing found no append or return-to-retry transition
  in the wait branch and exactly one counted, non-resending terminal in the
  `NULL` branch.
- One unresolved transaction occupies one of the four existing tenant writer
  slots but no pool connection during backoff. It preserves sole ownership and
  same-tenant ordering, leaves three slots for the stated single-contended-
  tenant case, and requires no second scheduler. Four simultaneous unresolved
  transactions can occupy all four slots; that bounded availability tradeoff
  follows revision-4's required wait-without-resend semantics. Capacity remains
  the explicitly deferred integration obligation.
- The deleted event-ID migration belonged only to the superseded, user-
  stipulated unreleased revision-2/3 implementation. Removing it leaves writer,
  staging schema, projection, and readers consistent with revision 4. Keeping
  or replacing it would preserve a forbidden column; this judgment does not
  authorize editing or deleting a released migration.
- The stale Bifrost read-audit sentence at
  `architecture/bifrost-design.md:589-595` predates the reviewed range and does
  not make audit results lost, duplicated, misattributed, or cross-tenant, so
  user scope excludes it.
- `FIND-AUDIT-OUTBOX-5`, `mise run bench:capacity`, and `mise run gate` remain
  deferred to integration by explicit user direction.
- No event-ID restoration, second table, ledger, WAL, relay, retirement delay,
  new public contract, test-only production seam, generic proxy abstraction, or
  panic-safety framework is justified by the retained ledger.

## Verification evidence and limits

- Independently passed:
  `mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E
  'test(/^outbox::tests::/)'` — 7/7.
- Other discovery passes independently passed the unwrap checker fixtures,
  `check:unwrap-audit` with a writable temporary cache, and `git diff --check`.
- The implementer records the exact production commit-outcome journey,
  Bifrost SQL integration, Bifrost server journey, `test:wyrd`, format, lints,
  docs, unwrap audit, and diff check as green. The Postgres-backed journey was
  source-traced but not independently rerun by this validator because the
  review environment does not provide the required Docker-backed Postgres
  lane.
- Green existing tests do not exercise the retained `Display` panic or satisfy
  the source-static import and rustdoc rules.
