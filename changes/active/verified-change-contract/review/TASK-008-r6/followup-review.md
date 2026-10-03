# TASK-008 round-six focused follow-up

## Conflict investigated

- Immutable subject:
  `1d05642bf2c4d824de1aec27286048ec79b37e25..f8d6945041467d52020024d311ce8831a45ff4a1`.
- Approved authority: `changes/active/verified-change-contract/spec.md`,
  revision 57, especially REQ-171.
- Closure boundary: `FIND-TASK-008-CLOSEOUT-17` and regressions introduced by
  the remediation range.
- Conflict: `domain-review-durability.md` proposed `DUR-R6-001`, asserting that
  a queued verifier can create a new pending Oracle audit decision after the
  drain's only replica scrape and settle before the later SQL snapshot. The
  behavior, invariant, system, and concurrency reviews treated the
  scrape-before-SQL order as a complete handoff, on the premise that all
  step-owned decisions already exist when request futures have joined.

The checkout has no `.codegraph/` directory. I used the immutable Git range,
`rg`, and direct source/caller inspection. Per the assigned boundary, I ran no
Cargo, mise, codegen, or database-backed command.

## Source path inspected

1. Capacity load completion and accepted queued work:
   `crates/wyrd/wyrd-testing/src/bin/capacity/load.rs:255-331,340-365,369-455`.
2. Step completion, activation accounting, and the drain observation order:
   `crates/wyrd/wyrd-testing/src/bin/capacity/step.rs:320-425`.
3. Run and audit fields in the durable backlog snapshot:
   `crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:159-200,257-300`.
4. Manual queued-run admission and request return:
   `crates/wyrd/wyrd-server/src/components/verification/service.rs:262-304,313-363`.
5. Queued Drift's scheduled Oracle read:
   `crates/wyrd/wyrd-server/src/verification/drift.rs:880-988` and
   `crates/wyrd/wyrd-server/src/query/scheduled.rs:129-196`.
6. Oracle read-decision staging before query execution:
   `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:2211-2239,2256-2353,2723-2745`.
7. Non-blocking audit ownership and commit release:
   `crates/wyrd/wyrd-server/src/oracle/query_audit.rs:101-117,155-214`.
8. Verification publication and terminal settlement:
   `crates/wyrd/wyrd-server/src/verification/runner.rs:241-297,372-436,449-527,628-672`
   and `crates/wyrd/wyrd-server/src/verification/publisher.rs:117-163`.
9. Candidate held-chain-head proof:
   `crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:354-588`.

## Evidence resolving the conflict

### Request completion is not queued-run completion

`Lane::drive` joins only the public request tasks. For queued Drift,
`Request::send` calls `Verification::start_run` and treats the enqueue response
as accepted (`load.rs:400-405`). The server commits the newly enqueued run and
returns its ID (`verification/service.rs:340-355`). The runner executes that
durable row later. Therefore `try_join_all` at `step.rs:343` establishes that
the enqueue acknowledgements returned; it does not establish that each queued
run has reached Oracle or that all audit decisions caused by those runs already
exist.

The durable backlog accounts for that asynchronous execution. It counts
accepted queued requests whose rows do not exist yet through
`activations - created`, and it counts existing rows only while their status is
`pending`, `running`, or `retrying` (`evidence.rs:280-299`). A row that becomes
terminal no longer holds `runs` nonzero.

This directly disproves the concurrency review's premise that every
step-owned decision is staged synchronously before its request returns. That
premise is true for the direct and public Oracle request lanes, but not for the
queued verifier lanes that REQ-171 explicitly includes.

### The queued Drift decision precedes run settlement but need not precede the scrape

A queued Drift attempt constructs an authenticated `ScheduledQueryCaller` and
awaits its Oracle stream (`drift.rs:951-988`). Oracle calls
`audit_read_decision` before it binds and executes the retained query plan
(`oracle/mod.rs:2211-2239,2256-2353`). `OracleQueryAudit::stage` increments the
process-local pending owner and gauge, then returns after the non-blocking
`try_send` (`query_audit.rs:101-117`). The writer releases that ownership only
after its independent staging transaction commits or is counted lost
(`query_audit.rs:155-214`).

After the Oracle stream finishes, the runner may publish the result and then
commit the run's terminal settlement (`runner.rs:241-297,372-436,628-672`).
Result publication returns only after every result batch is durably
acknowledged (`publisher.rs:117-163`). Neither result publication nor run
settlement waits for the separate Oracle audit writer. Consequently the run
can be terminal, and its Scribe result publication complete, while its Oracle
decision remains process-local and uncommitted.

### Reachable false-zero interleaving

Each drain poll performs all replica scrapes once, then issues one PostgreSQL
statement, then immediately accepts an all-zero `Backlog`
(`step.rs:406-423`). The following ordering is reachable for a queued Drift
run already represented by the run backlog at the start of the poll:

1. The relevant replica is scraped while the run has not yet reached its
   Oracle read, so `audit_outbox_pending` is zero. The run itself is not part
   of this metrics snapshot.
2. The runner reaches `audit_and_bind`; `OracleQueryAudit::stage` raises the
   pending gauge after that scrape. Its writer can remain uncommitted behind
   ordinary tenant chain-head serialization or a delayed database operation.
3. The Oracle query completes, result publication is acknowledged, and the
   runner commits the run terminally. The audit writer is independent and can
   still own the decision.
4. `Queue::backlog` begins its statement snapshot after that settlement. It
   sees the run terminal (`outstanding = 0`), the accepted run already created
   (`activations - created = 0`), and no audit staging row because the audit
   writer has not committed. With other work drained, `runs`, `scribe`,
   `audit`, and `forge` are all zero in this read.
5. `Backlog::with_replicas` adds the stale pre-SQL pending value of zero and
   `Drain::judge` accepts empty, although the replica still owns the audit
   decision.

The pre-SQL scrape correctly covers a decision that already existed at the
scrape and commits during the SQL read. It does not cover a decision first
created after the scrape. The durable query correctly covers a decision whose
staging commit is visible to its statement snapshot. It does not cover a newly
created decision whose run has already settled but whose independent audit
commit is still pending. No barrier in the traced path forbids the interval
between those two observations.

Sequential replica scraping does not remove the interval: a queued run may be
claimed by a replica after that replica's scrape, and no request-lane join
waits for the claim or engine. The durable run count prevents a false zero only
until the run's independent settlement becomes visible.

### Current proof does not exercise this ordering

`the_audit_backlog_holds_from_a_pending_decision_until_its_publication` issues
two public Oracle requests and waits for both requests to return before its
first `drain_read` (`evidence.rs:510-528`). Both decisions therefore exist
before the scrape. The proof covers pending-before-scrape, commit after the
captured stop timestamp, staged-above-watermark, and publication-to-zero. It
does not arrange for a step-owned queued run to stage its decision between the
replica scrape and the durable backlog snapshot, nor can its helper take a
second metrics observation before accepting zero.

## Proposed finding

### `FOLLOWUP-R6-001` — corroborates and narrows `DUR-R6-001`

- **Classification:** INCORRECT; prior finding
  `FIND-TASK-008-CLOSEOUT-17` remains open.
- **Violated obligation:** revision-57 REQ-171 requires every audit-outbox
  backlog caused by the step to drain within 60 seconds before the step
  passes. The R4 intended outcome likewise requires the audit cell to remain
  nonzero for the entire lifecycle of step-owned audit work.
- **Changed decision boundary:**
  `crates/wyrd/wyrd-testing/src/bin/capacity/step.rs:406-423`, composed with
  `crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:182-200,274-300`.
- **Producer path:** queued Drift enqueue in `capacity/load.rs:351-360,400-405`,
  later scheduled Oracle execution in `verification/drift.rs:951-988`,
  non-blocking staging in `oracle/query_audit.rs:101-117`, and independent run
  settlement in `verification/runner.rs:628-672`.
- **Observable consequence:** a capacity step can report its run and audit
  saturation cells drained and stop the 60-second timer while a serving
  replica still owns an uncommitted decision caused by that step. This is a
  false PASS in the exact evidence path `FIND-TASK-008-CLOSEOUT-17` required
  the range to close.
- **Smallest correction boundary:** retain the existing pending gauge,
  pre-SQL scrape, durable staging/watermark query, and exact deadline. Before
  accepting an otherwise empty durable snapshot, obtain post-query pending
  evidence from every replica and require it to be zero too. The pre-query
  observation continues to cover work handed off during SQL; the post-query
  observation covers work created after the first scrape. If the SQL snapshot
  still sees a queued run outstanding, that poll remains nonempty and repeats.
  Add a real-path proof that makes a queued verifier stage its Oracle decision
  after the first scrape, holds its audit commit while allowing the run to
  settle, and proves drain cannot accept zero until the decision commits and
  publishes. The current public-read held-chain test remains useful but is not
  closure proof for this interval.

No separate regression or additional finding was found while tracing this
conflict. The candidate's gauge ownership, pending-to-staging release point,
removal of the stop-time audit cut, and staged-above-watermark query are sound
for work observed by one of the two current snapshots.

## Resolution

**RESOLVED**

The conflict is resolved in favor of `DUR-R6-001`. The opposing reviews prove
only the handoff of audit work already present at the pre-SQL scrape. The
actual REQ-171 workload includes queued verifier execution that continues
after enqueue acknowledgement and can create a new Oracle audit decision
between the scrape and the SQL statement. Current source and proof do not
exclude or exercise that reachable false-zero interval.
