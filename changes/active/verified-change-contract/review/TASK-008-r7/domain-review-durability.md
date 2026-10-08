# TASK-008 round-seven durability and persistent-state review

## Immutable subject and review boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `345295d8e`
- Candidate: `54373c36856d41ffe3554c977c35e9d033918cdd`
- Range: `345295d8e..54373c36856d41ffe3554c977c35e9d033918cdd`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 57
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior review: `changes/active/verified-change-contract/review/TASK-008-r6/`
- Remediation authority: `TASK-008-CLOSEOUT-R5-bracket-audit-drain-and-place-pg-proof.md`

The candidate remained at the named commit during this review. The checkout
has no `.codegraph/` directory, so navigation used Git, `rg`, and direct source
inspection. Per the user-directed closure scope, this report decides only
whether `FIND-TASK-008-CLOSEOUT-17` and
`FIND-TASK-008-CLOSEOUT-18` are closed and whether this range introduces a
durability or persistent-state regression. It does not reopen earlier accepted
code except where it affects the capacity benchmark's PASS/FAIL result.
`FIND-TASK-008-CLOSEOUT-13` remains deferred to integration.

## Reviewed boundary

This review traced one audit decision through the complete evidence chain:

1. `OracleQueryAudit::stage` increments the process-local pending owner and
   `audit_outbox_pending` before enqueue
   (`crates/wyrd/wyrd-server/src/oracle/query_audit.rs:101-117`).
2. `OracleAuditWriter` retains that ownership through the canonical tenant
   append and decrements it only after commit or counted loss
   (`query_audit.rs:155-215`).
3. `Queue::backlog` reads one PostgreSQL statement snapshot containing
   nonterminal/missing runs and every staged audit row above its tenant's
   publication watermark
   (`crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:261-305`).
4. `Backlog::with_replicas` adds replica-local pending decisions to that
   durable reading (`evidence.rs:180-205`).
5. The new `Queue::poll` composes pre-query replica evidence, the durable
   statement, and—only for an otherwise-empty result—a post-query replica
   scrape (`evidence.rs:307-341`), and `Deployment::drain` uses that result at
   the benchmark's 60-second decision boundary
   (`crates/wyrd/wyrd-testing/src/bin/capacity/step.rs:383-423`).
6. `AuditPublisher` freezes, appends, and settles the tenant range before the
   watermark makes staging no longer owed
   (`crates/wyrd/wyrd-server/src/audit/publication.rs:259-383`), consistent
   with `architecture/bifrost-design.md:587-646`.

I also reviewed the moved Postgres proof in
`capacity/evidence.rs:458-708`, the pure backlog arithmetic tests at
`evidence.rs:376-456`, and the new fixed-port nextest group at
`.config/nextest.toml:62-70` against both capacity tests' actual use of port
8080.

## Authority coverage

| Authority | Applied obligation | Result |
|---|---|---|
| `changes/active/verified-change-contract/spec.md` REQ-171 | A step may pass only after every audit-outbox backlog caused by the step has drained within 60 seconds. | **FAIL** — one pending-to-staging handoff can still be observed as zero. |
| R6 `FIND-TASK-008-CLOSEOUT-17` and R5 AC-R5-1/2 | Pre-query pending, durable staging, and post-query pending evidence must not admit a false-empty poll; the proof must exercise the false-empty interval. | **FAIL** — the implementation and proof cover a decision that stays pending through SQL, not one that commits after SQL fixes its snapshot and before the post-query scrape. |
| `architecture/bifrost-design.md` read-audit and publication contract | Pending ownership lasts until canonical staging commit or counted loss; publication is watermark-owned. | **PASS** — the range does not change or duplicate those owners. |
| `architecture/agent-rules.md` Postgres test placement | Live-server/Postgres proof belongs in `mod pg_tests` or a `pg_*` file. | **PASS** — environment-owned helpers and proof now live in `mod pg_tests`; pure tests remain in `mod tests`. |
| R5 AC-R5-4 and range regression boundary | Preserve workload, SLOs, report shape, audit semantics, and adjacent tests. | **PASS for reviewed durability surfaces** — no durable contract changed; the fixed-port nextest group serializes exactly the two tests that bind port 8080. |

## Persistent-state analysis

The new post-query scrape closes the exact held-pending schedule exercised by
the test: the first metrics value is captured, the public Oracle read stages a
decision, the held chain-head transaction prevents its commit, the durable
statement reads zero, and the second scrape sees the still-pending decision
(`evidence.rs:637-656`). The use of a public Oracle read instead of a queued
Drift run is valid for that state transition. The drain cannot distinguish
which audited surface produced the decision, and the public read uses the same
`OracleQueryAudit` owner and canonical staging path.

That held schedule does not cover the whole pending-to-durable handoff. A
PostgreSQL statement reads a fixed MVCC snapshot. The following reachable
ordering remains:

1. The first replica scrape reads pending zero.
2. A queued verifier stages its Oracle decision, finishes its query and result
   publication, and settles its run terminally. Its audit writer is delayed,
   so the decision remains process-local.
3. `Queue::backlog` fixes a statement snapshot. It sees the run terminal and
   no staging row, so runs and durable audit both read zero.
4. Before that statement returns and before the second metrics scrape, the
   audit transaction commits. The writer then decrements pending, as required
   by `query_audit.rs:176-178`. The already-fixed SQL snapshot cannot see this
   commit.
5. The post-query scrape reads pending zero. `Queue::poll` combines that scrape
   with the stale zero durable value and returns an empty backlog.

The producer is not speculative: it is the same queued-verifier path already
validated as reachable in R6. The range moved the false-zero window from
"created after the sole scrape and still pending" to "created after the first
scrape and committed after the SQL snapshot but before the final scrape." The
first durable zero establishes that no queued run was nonterminal at that
snapshot, so an additional durable observation after the post-query scrape
could close this particular handoff without changing the audit owner or making
audit commits blocking. The current implementation performs no such
observation.

The focused proof deliberately holds the audit commit until `Queue::poll`
returns (`evidence.rs:637-656`), so it cannot fail under the ordering above.
Its later handoff loop starts with two decisions and has durable staging from
the earlier commit while the later decision moves (`evidence.rs:671-697`);
that proves conservative nonzero continuity when another staged row already
exists, but not the zero-to-one handoff on which the benchmark verdict depends.

## Material proposed finding

### `DUR-R7-001` — INCORRECT — the final scrape can race a commit invisible to the durable snapshot

- **Violated obligation:** REQ-171 and R5 AC-R5-1 require a poll that reports
  zero audit backlog to cover a decision first created after the pre-query
  scrape, including its pending-to-staging handoff. R5 AC-R5-2 requires focused
  proof of that false-empty interval.
- **Exact location:**
  `crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:324-341` and
  `crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:637-656`.
- **Evidence:** `Queue::poll` reuses the first PostgreSQL backlog value after
  the post-query scrape. `OracleAuditWriter` removes the pending gauge only
  after commit (`query_audit.rs:176-178`), but a commit after the SQL
  statement's MVCC snapshot is invisible to that statement. If it completes
  before the post-query scrape, both values used for the decision are zero.
  The test holds the commit across the post-query scrape and therefore cannot
  exercise this ordering.
- **Observable consequence:** a judged capacity step can record audit backlog
  zero and stop its drain timer while a step-caused audit row has just entered
  canonical staging above the publication watermark. The benchmark can
  produce a false PASS for REQ-171 saturation.
- **Testable correction boundary:** keep the existing process-local owner,
  gauge, canonical staging query, non-blocking request semantics, and
  `Deployment::drain` owner. At the otherwise-empty decision boundary, ensure
  the post-query replica observation is paired with a durable observation that
  occurs after it before zero is accepted, so a handoff that committed after
  the first SQL snapshot is visible. Do not add an audit queue, ledger,
  transaction spanning metrics and SQL, blocking commit, endpoint, or public
  contract. Extend the held-commit proof to release the commit after the first
  durable snapshot has been fixed but before the final decision, and prove
  that the poll remains nonzero through staging and publication.

## Prior-finding closure

| Finding | Result |
|---|---|
| `FIND-TASK-008-CLOSEOUT-17` | **OPEN.** The range catches a decision held pending across the durable read, but can still miss its commit between that read's MVCC snapshot and the post-query scrape. |
| `FIND-TASK-008-CLOSEOUT-18` | **CLOSED.** The real Postgres/live-server proof and its helpers are under `#[cfg(test)] mod pg_tests`; pure arithmetic remains under `mod tests`, and the ignore/environment gate is retained. |
| `FIND-TASK-008-CLOSEOUT-13` | **DEFERRED** to integration by caller direction; no capacity qualification is inferred here. |

## Verification limits

This was a source-only durability review. Per orchestrator direction, I ran no
Cargo, `mise`, codegen, or database-backed commands. I inspected the range's
recorded verification evidence but did not independently execute it. The full
default `bench:capacity` run is explicitly deferred and supplies no evidence
for or against this finding.

## Result

**FAIL**

`FIND-TASK-008-CLOSEOUT-18` is closed and the nextest serialization introduces
no reviewed durability regression, but `DUR-R7-001` leaves
`FIND-TASK-008-CLOSEOUT-17` open because the benchmark can still accept a
false-empty audit backlog during the pending-to-staging handoff.
