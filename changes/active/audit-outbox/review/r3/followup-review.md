# Focused follow-up review

## Immutable subject and question

- Base: `cf5ee4128ce0b842e00a0eb20770ab5c285dedd8`
- Candidate: `52e1144b5c186ccacd85d9c779a4e60c3cce5ac2`
- Reviewed range: `cf5ee4128ce0b842e00a0eb20770ab5c285dedd8..52e1144b5c186ccacd85d9c779a4e60c3cce5ac2`
- Approved authority: `changes/active/audit-outbox/spec.md`, revision 4
- Closure inputs: the r2 verdict and validation ledger, the bounded-remediation
  task, and the revision-4 commit-outcome remediation task

This fresh follow-up resolves only the conflicts routed by the orchestrator:
the remaining panic path, the proof obligation for the untested transaction
status branches, the range-local standards and documentation proposals, and
the three implementer-reported risks. It does not vote on the overall verdict
or reopen earlier passed code. The repository has no `.codegraph/` directory.
`HEAD` matched the immutable candidate before and after source inspection.

## Source and authority inspected

- `AGENTS.md` sections 2, 4-6, 11-12, 14-16
- `architecture/agent-rules.md`
- `architecture/references/languages/spec-driven-development.md`
- `architecture/references/languages/maintainer-style.md`
- `architecture/references/languages/rust-core.md`
- `architecture/references/languages/testing-workflows.md`
- `architecture/bifrost-design.md`, especially lines 587-649
- `architecture/operations/deployment-and-release.md`, especially the migration
  contract
- `changes/active/audit-outbox/spec.md`, revision 4, especially REQ-003,
  REQ-003a, REQ-008, REQ-009, AC-008, and AC-009
- `changes/active/audit-outbox/review/r2/findings-validation.md`
- `changes/active/audit-outbox/review/r2/verdict.md`
- both r2 remediation tasks and their recorded evidence
- every r3 discovery report supplied to this follow-up
- `crates/shared/wyrd-runtime/src/outbox.rs`
- `crates/vala/vala-sql/src/audit_outbox.rs`
- `crates/wyrd/wyrd-sql/src/error.rs`
- `crates/vala/vala-sql/src/queries/audit_staging.rs`
- `crates/vala/vala-sql/tests/pg_audit_outbox.rs`
- `crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs`
- `crates/wyrd/wyrd-server/src/audit/mod.rs`
- `crates/vala/vala-bifrost-redux/src/tables/audit/projection.rs`
- the deleted event-ID migration and its history in the immutable range
- the full committed range diff and file history needed to distinguish changed
  prose from earlier prose

## Resolution 1: panic containment and `FIND-AUDIT-OUTBOX-12`

### Reachable state trace

`OutboxWriter::dispatch` removes the only owned item vector from `waiting`,
spawns a child task, and records only `(tenant, count)` in `in_flight`
(`crates/shared/wyrd-runtime/src/outbox.rs:319-345`). The child catches unwind
while constructing `sink.write`, and `contain_panic` catches unwind from each
poll (`outbox.rs:331-337,435-454`). That closes the exact ordinary async-body
panic exercised by the new test.

The containment boundary nevertheless ends before all sink-controlled values
have been normalized and destroyed:

- On an ordinary sink error, `error.to_string()` executes after
  `contain_panic` returns (`outbox.rs:338-340`). `OutboxSink::Error` is generic
  and bounded only by `Display + Send + 'static` (`outbox.rs:55-77`); neither
  the trait nor Rust makes a `Display` implementation non-panicking. A
  conforming generic sink can therefore return an error whose `Display`
  panics. This is a direct child-task panic while the child still owns the sole
  vector.
- After a poll panic, the caught `Box<dyn Any + Send>` and the pinned sink
  future are dropped after the polling catch. A sink can use a panic payload
  with a panicking destructor, and a sink future can have a panicking
  destructor. Those drops occur after the narrow polling catch and can unwind
  the child. The current `finish` rustdoc itself records the panic-payload-drop
  example (`outbox.rs:368-375`).

The parent then receives `JoinError`. Because `in_flight` contains no items,
`finish` can only increment loss, release pending, and return
(`outbox.rs:382-388`). The writer and process continue, later same-tenant work
can dispatch, and the accepted decisions are permanently absent. This is not
an abrupt process stop, shutdown deadline, or revision-4 transaction-status
`NULL` terminal.

### Generic versus audit-sink reachability

The current production `AuditSink` uses `SqlError`, whose `Display` is derived
from fixed variants and whose source shows no custom panicking destructor. No
ordinary audit database error was found that is expected to trigger the
`Display` path above. That narrows immediate audit likelihood, but does not
close the finding: `FIND-AUDIT-OUTBOX-12` and REQ-008 govern the reusable
generic owner, and the trait admits this path without unsafe code, contract
violation, or test-only access. The r2 correction explicitly required a sink
task panic to retain the batch while the task owns it, not merely panics from
one poll site. The current generic module also promises that a write which
"returns an error or panics" is retried and that live-process loss does not
occur (`outbox.rs:11-23`).

### Resolution

`FIND-AUDIT-OUTBOX-12` remains open. The invariant and system reports identified
a reachable closure failure, not speculative hardening. Behavior, concurrency,
persistent-data, and security reviews proved the ordinary async-body panic but
did not account for the sink-owned normalization and destruction steps after
the catch.

## Resolution 2: direct proof for the transaction-status branches

### Approved proof contract

Revision-4 REQ-009 specifies four outcome classes: committed, aborted,
in-progress/unreachable, and status `NULL`. AC-009 chooses the required
Postgres integration proof: a landed commit whose reply is lost is not retried;
an aborted commit is retried and retained once; and repeated ambiguous commits
with publisher retirement between them leave exactly one retained row per
decision and a gap-free chain. The revision-4 remediation task repeats exactly
those proof scenarios. Neither AC-009 nor that task separately requires a
forced in-progress/unreachable state or an aged-out `NULL` state.

The journey at
`crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs:976-1054`
uses the production sink and publisher. Three `AckLost` rounds establish the
committed/no-resend outcome across retirement; the `CommitLost` round
establishes aborted/front-of-queue retry; the final retained population equals
the chain head. That is the direct AC-009 proof selected by the approved spec.

### Source closure of the unforced branches

`AuditSink::resolve_commit` keeps ownership inside one sink future
(`crates/vala/vala-sql/src/audit_outbox.rs:83-125`). `Ok(Some(_))` other than
the terminal strings and a query `Err` can only log, sleep with capped backoff,
and loop; there is no append call or return to the generic retry owner in that
branch (`audit_outbox.rs:112-122`). `Ok(None)` increments the audit loss metric
by the exact event count and returns success, so the generic owner releases
pending but cannot resend (`audit_outbox.rs:100-110`; generic release at
`outbox.rs:391-395`). These transitions contain no intervening stateful helper
or sibling consumer.

Repository testing guidance favors edge and stable-failure coverage, but the
applicable Rust reference says new core logic "should" cover those cases;
`AGENTS.md` requires directly testable code and the strongest applicable tier,
not exhaustive branch execution where the approved AC selected narrower proof.
A real `NULL` integration case would require aging a transaction status out of
Postgres; deterministic in-progress/unreachable injection would require more
proxy behavior or a new production seam. `maintainer-style.md` expressly warns
against building a test harness only to prove one branch. On this immutable
task, imposing those tests as closure requirements would strengthen approved
AC-009 after implementation.

### Resolution

The absent direct in-progress/unreachable and `NULL` tests are a material
verification limit to retain in the verdict, not a proposed finding and not a
reason that `FIND-AUDIT-OUTBOX-11` remains open. `STD-R3-002` is rejected as a
mandatory-proof claim. This conclusion does not claim those branches have
runtime proof; it distinguishes source-validated required behavior from the
three scenarios AC-009 explicitly requires as integration evidence.

## Resolution 3: standards, rustdoc, and prose proposals

### Import placement and declaration types

The new journey proxy violates two explicit, unconditional rules in
`architecture/agent-rules.md`:

- `CommitCutter::addr` uses `std::net::SocketAddr`
  (`audit_publication.rs:839-846`).
- `relay` uses `tokio::net::TcpStream` parameters and a
  `std::io::Result<()>` return (`audit_publication.rs:915-920`).
- `relay` puts `use tokio::io::{AsyncReadExt, AsyncWriteExt};` inside the
  function (`audit_publication.rs:921`). Neither allowed function-local import
  exception applies.

All of these lines were introduced by the immutable range. This is the exact
mandatory dependency-manifest rule previously tracked by
`FIND-AUDIT-OUTBOX-7`; fixing its older `MutexGuard` and `Uuid` sites while
adding fresh instances in the closure journey leaves the cumulative finding
unclosed and is an in-scope range regression.

### Rustdoc on changed test items

The range materially changes `MemorySink::write` to signal dispatch, optionally
wait, consume one-shot panic state, and then perform ordinary failure/write
behavior (`crates/shared/wyrd-runtime/src/outbox.rs:519-543`). It has no item
rustdoc, including no `# Errors`, `# Panics`, or async wait/cancellation
description. The range also adds all six methods in `impl Recorder for
TestMetrics` (`outbox.rs:614-629`) without item rustdoc, including the
intentional no-op histogram behavior.

`AGENTS.md` section 16 and `architecture/agent-rules.md` make substantive
rustdoc a hard acceptance criterion for every new or materially modified Rust
item, explicitly including private test helpers and methods. The behavior of
the changed sink is central to the `FIND-AUDIT-OUTBOX-12` closure proof, so this
is not documentation of untouched code or optional polish. It is an in-scope
range standards regression.

### Revision-4 loss prose

The changed owner-module paragraph at
`crates/vala/vala-sql/src/audit_outbox.rs:12-16` says a batch that fails to
commit is retried and "never dropped," but revision-4 REQ-009 and the same
module's `resolve_commit` documentation say a status Postgres no longer holds
is counted lost without resend (`audit_outbox.rs:65-78,100-110`). The new
qualification about consulting Postgres does not state that terminal exception
and leaves the owner overview making a broader durability promise than its
implementation. Because the paragraph was materially modified for revision 4
and describes the exact changed failure contract, the contradiction is an
in-scope documentation regression.

The similar sentence at `architecture/bifrost-design.md:589-595` says a failed
commit is retried until it lands. `git blame` and the exact range diff establish
that lines 593-595 predate the remediation base (`5a5542cbb9`) and were not
modified by `cf5ee4128..52e1144b5`; the range changes only the later, accurate
paragraph at lines 639-642. The earlier sentence is stale against revision 4,
but the user's closure boundary forbids reopening earlier passed prose unless
it makes audit results wrong. It does not alter the writer, publisher, reader,
or retained result, so it is recorded as an out-of-scope inconsistency rather
than folded into the range-local documentation proposal.

## Resolution 4: directed operational risks

### One unresolved batch holds one of four writer slots

Accepted, with a bounded availability cost. `AuditSink::outbox` fixes four
generic tenant-write slots (`crates/vala/vala-sql/src/audit_outbox.rs:27-32,
51-63`). `resolve_commit` owns one slot while its transaction outcome remains
uncertain but holds no checked-out database connection during its backoff
sleep: each `fetch_one(self.vala.pool())` completes before the branch sleeps
(`audit_outbox.rs:90-121`). One unresolved tenant therefore leaves three slots
and cannot be overtaken by later same-tenant work. Four simultaneously
unresolved transactions can occupy all four slots, but under complete Postgres
unreachability no released slot could commit another audit batch. Moving the
wait out of the sink would require a second unresolved-outcome scheduler and a
new concurrency owner not approved by revision 4. This is not a loss,
duplication, attribution, tenancy, or closure regression; capacity remains the
explicitly deferred benchmark obligation.

### Deleted unreleased migration

Accepted. The deleted
`crates/vala/vala-sql/migrations/20261003000001_audit_staging_event_id.sql` was
introduced by the superseded event-ID implementation and removed by the
revision-4 commit. The user stipulates it was unreleased, and revision 4
explicitly requires no event-ID column on staging or retained audit. Current
append, row, projection, and reader source consistently omit it. Retaining a
forward migration would leave a schema the final writer does not populate and
would contradict the approved contract. This conclusion is specific to the
stipulated unreleased migration; it does not weaken the repository's immutable
released-migration rule.

## Proposed findings for independent validation

### FOLLOW-R3-001 — `FIND-AUDIT-OUTBOX-12` remains open after partial containment

- **Classification:** `INCORRECT`
- **Violated obligation:** `FIND-AUDIT-OUTBOX-12`, REQ-003, REQ-003a, and
  REQ-008.
- **Location:** `crates/shared/wyrd-runtime/src/outbox.rs:328-342,382-388,
  435-454`.
- **Reachable consequence:** a generic sink error whose `Display` panics, or
  sink-controlled future/panic-payload cleanup that panics after polling, turns
  into `JoinError`; the sole accepted batch is counted lost and released while
  the process continues.
- **Minimum correction boundary:** preserve the owned batch while containing
  the complete sink-controlled result lifecycle needed to classify the write,
  then route every contained non-deadline panic through the existing failure,
  front-of-queue retry, and write-failure metric. Do not clone items, add a
  queue, or abort the process.
- **Focused proof:** a test sink with a panicking error formatter (and any
  separately relevant cleanup panic) proves the original batch commits once
  before later same-tenant work, the writer survives, failure increments, loss
  remains zero, and pending/gauge settle to zero.

### FOLLOW-R3-002 — the closure journey reintroduces `FIND-AUDIT-OUTBOX-7`

- **Classification:** `REGRESSION / VIOLATION`
- **Violated obligation:** `architecture/agent-rules.md` top-level-import and
  bare-declaration-type rules; prior `FIND-AUDIT-OUTBOX-7` closure.
- **Location:**
  `crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs:839-841,
  915-921`.
- **Consequence:** the range claims closure while its new proof code repeats
  the exact mandatory dependency-manifest violation.
- **Minimum correction boundary:** import the existing standard-library and
  Tokio types/traits in the module import block and use bare declaration and
  signature names; no wrapper or refactor is needed.

### FOLLOW-R3-003 — changed and new Rust test items lack mandatory rustdoc

- **Classification:** `VIOLATION`
- **Violated obligation:** `AGENTS.md` section 16 and
  `architecture/agent-rules.md` Rust documentation rule.
- **Location:** `crates/shared/wyrd-runtime/src/outbox.rs:519-543,614-629`.
- **Consequence:** the panic-closure fixture's ordering, errors, panics, wait
  behavior, and the recorder's deliberate metric model are not documented at
  the items that own them, despite the hard rule covering test helpers.
- **Minimum correction boundary:** document only the materially changed sink
  method and the new recorder methods with the required workflow, error/panic,
  async, and no-op semantics; do not document unrelated untouched items.

### FOLLOW-R3-004 — changed audit-sink overview overstates retry durability

- **Classification:** `REGRESSION / INCORRECT`
- **Violated obligation:** revision-4 REQ-009 and the maintainer requirement
  that durable failure and retry documentation match behavior.
- **Location:** `crates/vala/vala-sql/src/audit_outbox.rs:12-16`.
- **Consequence:** the owner overview promises a failed batch is never dropped
  while the approved and implemented `NULL` terminal counts it lost and sends
  nothing again, leaving incompatible guidance inside one module.
- **Minimum correction boundary:** make this changed overview name the same
  committed/aborted/wait/status-unavailable outcomes already documented by
  `resolve_commit`; no new check or architectural prose is needed.

`STD-R3-002` (mandatory direct tests for in-progress/unreachable and `NULL`) is
not retained. The Bifrost read-audit sentence at lines 593-595 is stale but is
not a proposed finding because it predates the range and does not make audit
results wrong.

## Status

**RESOLVED.** Source and authority resolve all routed conflicts. Four proposed
findings remain for the required independent Ponytail validation; the missing
transaction-status branch tests remain a verification limit, and the writer
slot and unreleased-migration risks are accepted within the approved boundary.
