# Focused follow-up — audit-outbox r3 conflicts

## Immutable subject and purpose

- Base: `cf5ee4128ce0b842e00a0eb20770ab5c285dedd8`
- Candidate: `e54b1244f32950d1ab251dae6530c4e1694c78d5`
- Reviewed range: `cf5ee4128..e54b1244f`
- Approved authority: `changes/active/audit-outbox/spec.md`, revision 3
- Prior closure authority: `review/r2/verdict.md`,
  `review/r2/findings-validation.md`, and
  `review/r2/TASK-AUDIT-OUTBOX-R2-bounded-remediation.md`

This pass resolves three conflicts among the completed r3 discovery reports.
It is not a vote or a verdict. I read all eight discovery reports in this
directory, the complete applicable workflow and architecture authority, the
owners and callers identified below, and the relevant history. The repository
has no `.codegraph/` directory, so navigation used direct source and history.
The unanimous shipped-reader-collapse finding is not re-reviewed here.

## Uncertainty 1 — retained-schema compatibility

### Source paths and history inspected

- `architecture/references/languages/spec-driven-development.md`
- `architecture/operations/deployment-and-release.md:151-218`
- `architecture/references/domain/iceberg.md:22-29`
- `changes/active/audit-outbox/spec.md`, revision 3, especially REQ-009,
  AC-009, expensive-to-reverse decisions, and open material decisions
- `crates/vala/vala-bifrost-redux/src/tables/audit/audit_log.rs`
- `crates/vala/vala-bifrost-redux/src/scribe/ingress.rs:183-204`
- `crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs:1016-1042,
  1072-1135,1238-1261`
- Commit `b7185d0ee0a812949efb74fe232b2b10a94c625d` and the audit-specific
  compatibility implementation it deleted
- The current branch/tag history around the immutable base and candidate

### Evidence

The mechanical risk reported by both sides is real. `ensure_builtin` sends the
current declaration to `create_table_locked`; an existing control row whose
fingerprint differs returns `FingerprintMismatch` before the physical table is
accepted (`bifrost_catalog.rs:1110-1113`), and an exact physical-shape mismatch
would also fail (`:1238-1261`). Adding required `event_id` therefore cannot
publish into a table materialized under the preceding declaration. The range
contains no evolution path.

That fact is not, by itself, a supported-state regression for this task. The
authority order in `spec-driven-development.md` places repository architecture
above the approved spec and both above implementation evidence. The applicable
release authority expressly states that no Wyrd image has yet been published
and that the next artifact is the first release
(`deployment-and-release.md:209-213`). Its expand-and-contract requirement is
conditioned on an old/new release overlap (`:163-168`), and its compatibility
interval is established by published release manifests (`:175-218`). There is
no supported predecessor image or manifest in that interval here. Revision 3
then expressly approves `event_id` on retained `audit_log` as an
expensive-to-reverse schema change and records no open material decision.

Repository history corroborates that interpretation rather than creating it:
`b7185d0ee` deliberately removed the only audit-specific Iceberg evolution,
legacy fingerprint, field-ID reconciliation, and upgrade proof because the
schema had not shipped. The current strict `ensure_builtin` path is the chosen
pre-release replacement behavior, not an accidentally omitted compatibility
branch.

The Iceberg reference still controls released/retained schema evolution: once
an earlier table shape belongs to a supported release interval, a required
field change needs explicit compatibility. It does not turn every developer or
test catalog materialized from an unpublished intermediate commit into a
supported upgrade state. Under the checked-in deployment authority, an
existing materialized *pre-release* catalog is disposable integration/developer
state unless the user separately designates it as data that the first release
must preserve. No such designation exists in this task. If integration supplies
that new external fact, the current code will refuse publication and the
compatibility decision must be reopened before release.

### Resolution

**RESOLVED — no source-local finding.** `STD-R3-001` and `SEC-TEN-R3-002`
correctly identify what would happen to an old physical table, but they apply a
released-upgrade obligation to a state the controlling deployment authority
does not support. The maintainer, system, behavior, invariant, and
persistent-data dispositions are the authority-consistent result for this
immutable task. This conclusion is narrow to the first-release boundary and
must not be reused after a Wyrd artifact is published or if the user explicitly
requires preservation of a pre-release catalog.

## Uncertainty 2 — escaped sink panics and `FIND-AUDIT-OUTBOX-12`

### Source paths inspected

- Revision-3 REQ-003, REQ-003a, REQ-008, and AC-008 in
  `changes/active/audit-outbox/spec.md`
- `review/r2/findings-validation.md` and `review/r2/verdict.md`, exact
  `FIND-AUDIT-OUTBOX-12` diagnosis and correction
- `review/r2/TASK-AUDIT-OUTBOX-R2-bounded-remediation.md`, item 1 and its
  focused closure proof
- `crates/shared/wyrd-runtime/src/outbox.rs:50-78,304-453`
- `crates/vala/vala-sql/src/audit_outbox.rs:90-110`
- All repository `OutboxSink` implementations and call sites, including the
  test-only `UnknownOutcomeSink`
- The current and planned-consumer references found by the discovery reports

### Evidence

The original defect was concrete: the child task owned the only item vector,
an ordinary panic from `OutboxSink::write` escaped as `JoinError`, and
`finish` counted that batch lost while the process continued. The independently
validated correction was specific: contain sink-future construction and
polling unwind while the task still owns the vector, then use the existing
front-of-queue retry path. The remediation task repeats exactly that boundary
and asks for a sink that panics once and then succeeds.

The candidate implements that correction at `outbox.rs:328-342`:
`sink.write(...)` construction is inside `catch_unwind`, every poll is inside
`contain_panic`'s `catch_unwind`, and the child returns `(tenant, items,
result)`. `finish` then treats the contained panic like an ordinary failure and
prepends the original vector (`:368-423`). The focused proof exercises this
path. The production `AuditSink` is the only shipped implementation; its
compiler-generated future awaits tenant acquisition, append, and commit and
returns `SqlError`. No custom `Drop`, panic payload, or panicking `Display`
implementation is present in that path. No production Eval sink exists in this
candidate.

`CONC-R3-001` identifies type-system-possible escapes outside that boundary:
a deliberately panicking future destructor after a caught poll panic, an error
whose `Display` panics, or a panic payload whose formatting/destruction panics.
The public trait bounds do not make those programs ill-typed. They are not,
however, a reachable behavior of a current production consumer, and they are
not the construction/poll panic that `FIND-12` and its approved correction
specified. They require a custom sink type whose teardown or formatting itself
panics. Treating every user-supplied `Drop`/`Display` implementation as an
outbox failure mode would extend the task into general adversarial Rust panic
containment; even `Item::drop` can be made to panic, so the proposed boundary is
not a complete generic no-unwind guarantee.

The remaining `JoinError` branch is therefore defensive runtime containment,
not evidence that the reported production/audit path is still wrong. It also
does not handle deadline cancellation in normal operation: on deadline the
writer and its `JoinSet` are dropped by the shutdown path, and shutdown owns the
resulting accepted loss.

### Resolution

**RESOLVED — no source-local finding.** `FIND-AUDIT-OUTBOX-12` is closed at
the exact approved correction boundary. `CONC-R3-001` is rejected as
speculative hardening over hypothetical custom malicious/broken
`Future::drop`, `Display`, or panic-payload behavior, with no current production
consumer path. A future sink that introduces such behavior must be reviewed at
that sink; it is not a regression introduced by this range.

## Uncertainty 3 — repeated unknown commit outcomes

### Source paths inspected

- Revision-3 REQ-003, REQ-008, REQ-009, and AC-009 in
  `changes/active/audit-outbox/spec.md`
- `crates/vala/vala-sql/src/audit_outbox.rs:90-110`
- `crates/vala/vala-sql/src/queries/audit_staging.rs:57-209,472-522`
- `crates/shared/wyrd-runtime/src/outbox.rs:368-423`
- `crates/wyrd/wyrd-server/src/audit/publication.rs:272-348`
- `crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs:821-870,
  909-1054`
- `architecture/bifrost-design.md` and the revision-3 audit wording in
  `architecture/wyrd-security-posture.md`

### Evidence

This path is reachable without a malicious type. `AuditSink::write` returns
the result of `TenantConn::commit` directly. A connection loss after Postgres
commits is an ordinary ambiguous result: the durable row exists, but the
outbox receives `Err`. `OutboxWriter::finish` must retry every reported failure
indefinitely and retains the same event ID at the tenant front. On each retry,
`append_audit_events` checks only the current `vala.audit_staging` rows
(`audit_staging.rs:113-130`). `settle_publication` deletes those rows after
durable publication (`:491-522`). Once deleted, the next attempt allocates a
new sequence and creates a new range-derived Scribe batch identity. Retained
`event_id` makes that copy detectable but does not fence another restage.

Consequently the following cycle has no guard and can repeat:

1. commit succeeds and its acknowledgement is lost;
2. the publisher retains and retires that staged row during retry delay;
3. retry restages the same event under a new sequence;
4. that commit also succeeds and loses its acknowledgement;
5. publication retires the second row before the next retry;
6. the next retry creates a third retained copy, and later cycles can create
   more.

The test fixture deliberately excludes this path. `UnknownOutcomeSink` uses
`lose_next_ack.swap(false, ...)` and makes every later attempt commit normally
(`audit_publication.rs:821-870`). Its phase-two assertion therefore proves one
ambiguous outcome plus one successful retry, not the claimed physical bound.

The approved wording is not limited to a single retry. REQ-003 requires a
failed write to remain queued and retry with backoff, and REQ-008 assigns that
retry behavior to the generic owner without an attempt limit. REQ-009 describes
the one-retirement example in the singular, but AC-009 supplies the normative
bound: a retry after retirement produces **at most one extra retained row**.
Read together, that is a global per-event physical multiplicity cap, not merely
a statement about the first retry. If the intended contract were ordinary
at-least-once delivery with any number of detectable physical copies, AC-009
would not say “at most one.” Logical read collapse does not repair the retained
row bound.

No source-local bounded correction is available under revision 3's simultaneous
constraints. Staging is the sole durable event-ID fence and is deliberately
deleted after publication; retries are indefinite; and the spec prohibits a
second ledger/table/WAL and a retirement delay. Preventing a third copy needs a
durable identity lifetime or coordination decision, or the approved bound must
change to allow any number of detectable copies. That is a persistent-data and
delivery-semantics decision, not a local test or consumer guard.

### Resolution

**RESOLVED — proposed new source-local finding `FOLLOWUP-R3-001`.** The
`INV-R3-001` / `PDATA-R3-002` claim is supported and should enter independent
validation. The reports that accepted the one-ambiguity journey did not trace
the indefinite retry loop through a second publication/retirement cycle.

#### FOLLOWUP-R3-001 — repeated ambiguous commits exceed AC-009's retained-copy bound

- **Classification:** `INCORRECT`
- **Violated obligation:** revision-3 REQ-003/REQ-008 indefinite retry together
  with AC-009's “at most one extra retained row” requirement.
- **Exact location:**
  `crates/vala/vala-sql/src/audit_outbox.rs:102-109`;
  `crates/vala/vala-sql/src/queries/audit_staging.rs:113-130,491-522`;
  `crates/shared/wyrd-runtime/src/outbox.rs:391-423`; insufficient proof at
  `crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs:821-870,
  909-1007`.
- **Observable consequence:** two successive commit-success/acknowledgement-loss
  cycles with publication and retirement between them produce three retained,
  gap-free rows for one authorization decision; the cycle can continue, so
  physical retained duplication is not bounded at one extra row.
- **Correction boundary:** this requires specification revision unless the
  approved constraints permit a durable/reconcilable event-ID lifetime or
  publication coordination mechanism. A focused proof must inject at least two
  successive commit-success/unknown-result attempts, publish and retire between
  them, and establish the newly approved physical bound. Harness or shipped-read
  deduplication alone is not closure proof for this retained-data obligation.

## Follow-up disposition

All three conflicts are **RESOLVED** from checked-in authority and reachable
source paths:

1. the pre-release schema mismatch is real but not a supported compatibility
   state in this task;
2. the construction/poll panic defect from `FIND-12` is closed, while the
   proposed destructor/formatting cases are hypothetical custom-sink hardening;
3. repeated real commit ambiguities can exceed AC-009's explicit physical-copy
   cap and support `FOLLOWUP-R3-001` for independent validation.

No conclusion here changes the separate, unanimous finding that shipped audit
count/list reads do not collapse retained duplicates.
