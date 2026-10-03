# Structured Ponytail finding validation

## Immutable subject

- Base: `6714ae35d732814240fdcc42fc226c079b14d3f0`
- Candidate: `5a5542cbb965af99e92a3983a2ddf586412cea73`
- Approved specification: `changes/active/audit-outbox/spec.md`, revision 2
- Original tasks: `changes/active/audit-outbox/tasks/*.md`
- Remediation authority: `review/r1/TASK-AUDIT-OUTBOX-R1-remediation.md`

`HEAD` matched the candidate before validation. The repository has no
`.codegraph/` directory. Validation covered the complete cumulative diff, all
required r2 reports, the changed owners and callers, the publisher and retained
history consumers, and the applicable repository and architecture authority.

## Validation status

**COMPLETE — SPEC_REVISION_REQUIRED.** The retained ledger contains six
findings. `FIND-AUDIT-OUTBOX-11` cannot be corrected safely without choosing a
new persistent identity owner or a new append/publication coordination
contract. The other five findings are bounded corrections after that decision.

## Proposal disposition

| Discovery proposal(s) | Result | Validation |
|---|---|---|
| `BEH-R2-001`, `INV-R2-001`, `STD-R2-001`, `MAINT-R2-01`, `SYS-R2-001`, `SEC-TEN-R2-001`, `CONC-R2-001`, `DUR-PDATA-001` | **CONFIRMED** | Staging retirement deletes the only event-ID fence while the retry owner can still hold the same event. New `FIND-AUDIT-OUTBOX-11`; correction is `SPEC_REVISION_REQUIRED`. |
| `BEH-R2-002`, `INV-R2-002`, `STD-R2-002`, `CONC-R2-002` and the maintainer uncertainty | **CONFIRMED** | The explicitly handled `JoinError` path loses a batch while the writer and process continue. New `FIND-AUDIT-OUTBOX-12`. |
| `MAINT-R2-02`, `MAINT-R2-03`, `CONC-R2-003`, follow-up uncertainties 1-2 | **REVISED** | One owner-level shutdown transition is incomplete: admission is not closed at invocation and deadline-abandoned work remains reported pending. Consolidated as new `FIND-AUDIT-OUTBOX-13`. The current server drains producers first, but REQ-008 and AC-008 require the generic owner itself to implement the contract. |
| `BEH-R2-003`, `INV-R2-003`, `STD-R2-004`, `MAINT-R2-04` | **CONFIRMED** | A basename-wide `tests.rs` exclusion weakens the production unwrap audit. New `FIND-AUDIT-OUTBOX-14`. |
| `STD-R2-003`, follow-up uncertainty 3 | **REJECTED** | REQ-008 requires a shared SQL-free runtime primitive and the approved remediation explicitly selected `wyrd-runtime`. `metrics`, `tokio-util`, and `tracing` are installed, directly used capabilities; no SQL/cloud/engine dependency entered the client tier and the boundary check passes. Splitting one cohesive, two-sink primitive into a new crate solely to avoid a lightweight `metrics` edge adds a package and ownership seam without a demonstrated task failure. |
| `STD-R2-005` | **REVISED** | The original locations were fixed, but the same exact r1 bare-type-path violation remains in newly added declarations. Preserve `FIND-AUDIT-OUTBOX-7`. |
| `STD-R2-006` | **REVISED** | Most r1 prose drift is fixed, but the same exact r1 live-authority finding remains at one security-operations sentence. Preserve `FIND-AUDIT-OUTBOX-3`. |
| `SYS-R2-002` | **REJECTED IN THIS REVIEW** | The user explicitly deferred `FIND-AUDIT-OUTBOX-5` and `mise run bench:capacity` to integration. Its absence cannot be reopened as an r2 task defect; it remains an integration acceptance obligation. |

## Prior-finding closure

- `FIND-AUDIT-OUTBOX-1` is closed for ordinary acquire, append, and commit
  errors: the same batch is restored ahead of later tenant work and retried.
  The retirement race is a new revision-2 identity defect, not the old
  drop-on-error implementation.
- `FIND-AUDIT-OUTBOX-2`, `-4`, `-6`, `-9`, and `-10` are closed.
- `FIND-AUDIT-OUTBOX-3` and `-7` remain narrowly unclosed as recorded below.
- `FIND-AUDIT-OUTBOX-8` is closed by the recorded TypeScript install, build,
  N-API declaration, typecheck, unit, and 29/29 integration lanes.
- `FIND-AUDIT-OUTBOX-5` remains explicitly deferred to integration and is not
  part of this ledger.

## Final deduplicated finding ledger

### FIND-AUDIT-OUTBOX-11 — publication retirement erases the unknown-commit dedup identity

- **Discovery sources:** `BEH-R2-001`, `INV-R2-001`, `STD-R2-001`,
  `MAINT-R2-01`, `SYS-R2-001`, `SEC-TEN-R2-001`, `CONC-R2-001`,
  `DUR-PDATA-001`
- **Status:** `CONFIRMED`
- **Classification:** `INCORRECT`
- **Violated obligation:** REQ-009, AC-009, AC-002's exactly-once recovery,
  and the audit-cardinality authority.
- **Exact location:**
  `crates/vala/vala-sql/src/queries/audit_staging.rs:111-130,449-467,489-520`;
  `crates/vala/vala-sql/migrations/20261003000001_audit_staging_event_id.sql:12-15`;
  insufficient proof at
  `crates/vala/vala-sql/tests/pg_audit_outbox.rs:185-215`.
- **Evidence:** `AuditSink::write` can commit in Postgres while its client sees
  an error. The generic writer then retains the same `StagedAuditEvent` and
  backs off. Meanwhile any publisher may retain the committed range and
  `settle_publication` deletes its staging row. Neither the publication row nor
  retained audit projection carries `event_id`. The retry therefore sees a
  fresh ID, consumes another sequence, and retains the same permission decision
  twice. Scribe's batch fence cannot absorb it because the second append has a
  new sequence range. The current test retries before retirement.
- **Observable consequence:** one authorization decision appears twice in
  authoritative retained history under two valid, gap-free chain entries.
- **Ponytail correction:** `SPEC_REVISION_REQUIRED`. Deletion, an existing
  fence, Postgres uniqueness on transient staging, and Scribe's range identity
  cannot preserve the event identity after retirement. Human approval must
  choose where the identity survives, its retention/garbage-collection rule,
  or the coordination that prevents retirement before reconciliation, while
  preserving one append path, one publisher, RLS, and chain ordering. Do not
  add a timing grace period, second writer, WAL, relay, or ad hoc downstream
  guard as bounded remediation.
- **Focused closure proof:** force a durable commit whose client outcome is
  unknown, publish and retire that exact row through the production path, retry
  the original event with a later same-tenant event behind it, and prove one
  retained decision, no second sequence allocation, intact ordering, and zero
  pending work.

### FIND-AUDIT-OUTBOX-12 — a sink-task panic loses accepted work while the process remains alive

- **Discovery sources:** `BEH-R2-002`, `INV-R2-002`, `STD-R2-002`,
  `CONC-R2-002`
- **Status:** `CONFIRMED`
- **Classification:** `VIOLATION`
- **Violated obligation:** REQ-003, REQ-003a, REQ-008, and AC-008 permit loss
  only at abrupt process stop or the graceful-shutdown deadline.
- **Exact location:** `crates/shared/wyrd-runtime/src/outbox.rs:288-297,321-340`.
- **Evidence:** the spawned task owns the only item vector. On `JoinError`, the
  writer retains only tenant and count, increments the loss metric, releases
  pending, and continues. This branch is part of the production generic owner;
  a panicking sink is therefore an implemented live-process loss path, not a
  shutdown or process-stop path.
- **Observable consequence:** audit, and the required second generic sink, can
  silently skip an accepted tenant batch and allow later work to pass it while
  `pending()` reports completion.
- **Ponytail correction:** keep the existing writer, queue, and retry path.
  Contain sink future construction and polling unwind inside the child task
  while that task still owns and can return the item vector, then route the
  recovered batch through the ordinary front-of-queue retry/backoff and write-
  failure metric. The already-installed async utilities may be reused; do not
  add cloning requirements, a second queue, or a process-abort policy.
- **Focused closure proof:** a generic sink panics once, then succeeds; the
  original batch remains pending, commits once before a later same-tenant item,
  the writer remains alive, the loss counter does not increment, and pending
  returns to zero.

### FIND-AUDIT-OUTBOX-13 — shutdown neither fences admission at invocation nor clears terminal pending state

- **Discovery sources:** `MAINT-R2-02`, `MAINT-R2-03`, `CONC-R2-003`,
  `followup-review.md` uncertainties 1-2
- **Status:** `REVISED`
- **Classification:** `INCORRECT`
- **Violated obligation:** REQ-007 requires shutdown to stop accepting new
  events; REQ-008 and AC-008 require coherent shutdown/loss accounting and
  pending to return to zero; Bifrost telemetry requires active gauges to
  decrement on cancellation and failure.
- **Exact location:** `crates/shared/wyrd-runtime/src/outbox.rs:136-148,175-190,234-269,385-399`.
- **Evidence:** `shutdown` cancels a token, but `stage` can still send until the
  writer later selects the stop branch and closes the receiver. At deadline,
  `abandon_remaining` counts the atomic total lost but never releases it or
  decrements the gauge; after the writer exits, `pending()` permanently reports
  work that has no owner. The server currently drains its producers before
  calling shutdown, which mitigates the audit composition but does not satisfy
  the public generic contract assigned by REQ-008.
- **Observable consequence:** a concurrent caller can enlarge the shutdown
  drain set after shutdown begins, and deadline-abandoned work remains exposed
  forever as live backlog.
- **Ponytail correction:** implement one handle-owned, one-way shutdown
  transition. Atomically close/drop the staging sender at shutdown invocation
  before waiting, so pre-fence items drain and post-fence stages are refused and
  counted without entering the queue. When deadline abandonment ends writer
  ownership, preserve the exact returned loss count separately while releasing
  that count from the pending atomic and gauge. Reuse the existing channel,
  counter, gauge, and writer; do not add another lifecycle service.
- **Focused closure proof:** with one write blocked, begin shutdown and race a
  later stage; only the pre-fence item reaches the sink. A persistent-failure
  deadline returns the exact loss count, increments loss once, and leaves both
  `pending()` and `outbox_pending{outbox}` at zero.

### FIND-AUDIT-OUTBOX-14 — the unwrap audit exempts every file named `tests.rs`

- **Discovery sources:** `BEH-R2-003`, `INV-R2-003`, `STD-R2-004`,
  `MAINT-R2-04`
- **Status:** `CONFIRMED`
- **Classification:** `REGRESSION / VIOLATION`
- **Violated obligation:** the repository prohibition on broadening a gate
  exclusion and the check's promise to scan production Rust.
- **Exact location:** `scripts/check_unwrap_audit.py:31-38,227-233`.
- **Evidence:** Rust gives `tests.rs` no test-only semantics. The four current
  files are genuinely declared by `#[cfg(test)] mod tests;`, but the checker
  ignores the basename without checking those owners. A production
  `mod tests;` or `#[path = "tests.rs"]` module now bypasses the audit.
- **Observable consequence:** production `.unwrap()` or invalid `.expect()`
  calls can pass `check:unwrap-audit` solely because of a filename.
- **Ponytail correction:** delete the basename rule. Use the smallest explicit
  allowlist of the four currently verified cfg-test module files rather than
  adding a Rust parser; keep each entry documented with its owning declaration.
  A future entry must prove the same cfg-test ownership.
- **Focused closure proof:** checker fixtures with identical `tests.rs` bodies
  prove an allowlisted cfg-test module is skipped and a production-included
  `tests.rs` is scanned and rejected; the existing upload wiremock assertion
  remains accepted.

### FIND-AUDIT-OUTBOX-7 — newly added declarations still hide type ownership

- **Discovery source:** `STD-R2-005`
- **Status:** `REVISED`
- **Classification:** `VIOLATION`
- **Violated obligation:** `architecture/agent-rules.md` requires top-level
  imports and bare names in fields and signatures.
- **Exact location:** `crates/shared/wyrd-runtime/src/outbox.rs:467-473` and
  `crates/vala/vala-sql/src/queries/audit_staging.rs:111-124,216-240`.
- **Evidence:** `MemorySink::lock` returns `std::sync::MutexGuard`; `AuditRows`
  fields use `uuid::Uuid`. These are new declarations in the remediation range.
  The prior `state.rs` and Oracle locations are fixed.
- **Observable consequence:** the remediation claims closure while repeating
  the same mandatory dependency-manifest violation in its new owner code.
- **Ponytail correction:** import `MutexGuard` and `Uuid` at their module tops
  and use bare names in the return type, fields, and matching local collection
  declarations. No wrapper, alias type, or refactor is needed.
- **Focused closure proof:** focused search of the changed declarations plus
  `mise run fmt` and `mise run lints`.

### FIND-AUDIT-OUTBOX-3 — security operations still names an Oracle audit commit owner

- **Discovery source:** `STD-R2-006`
- **Status:** `REVISED`
- **Classification:** `INCORRECT`
- **Violated obligation:** the r1 live-authority correction, REQ-003/REQ-004,
  and the one-outbox ownership rule.
- **Exact location:** `architecture/wyrd-security-posture.md:418-427`.
- **Evidence:** the same changed authority correctly states that Oracle only
  stages on the shared outbox, but its security-event list still says “Oracle
  audit commit failure.” Oracle has no commit path or surface metric; the
  generic audit outbox owns writes and `outbox_write_failures_total`.
- **Observable consequence:** incident handling attributes a shared audit
  persistence failure to the wrong component and contradicts the corrected
  architecture in the same document.
- **Ponytail correction:** replace only that stale phrase with the shared audit
  outbox write/publication vocabulary already used by the surrounding authority
  and runbook. Do not add an Oracle-specific metric, alias, or compatibility
  prose.
- **Focused closure proof:** a focused live-doc search leaves no Oracle audit
  commit owner, followed by `mise run docs:check`.

## Accepted limits and rejected expansion

- `mise run bench:capacity` and the aggregate gate remain integration-owned by
  explicit user direction; no substitute lane is requested here.
- Unbounded in-memory growth during prolonged Postgres outage and abrupt-
  process/deadline loss are approved revision-2 behavior.
- No per-tenant admission quota, second audit table, WAL, relay, publisher,
  compatibility path, or new client-visible contract is proposed.
