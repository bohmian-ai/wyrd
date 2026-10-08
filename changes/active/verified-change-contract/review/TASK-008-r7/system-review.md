# System-resilience review

## Subject and scope

- Immutable subject: `345295d8e..54373c36856d41ffe3554c977c35e9d033918cdd`.
- Authority: approved `changes/active/verified-change-contract/spec.md` revision
  57, especially REQ-171; the original TASK-008 closeout; the round-six
  verdict and validated findings; and remediation task
  `TASK-008-CLOSEOUT-R5-bracket-audit-drain-and-place-pg-proof.md`.
- User-directed closure scope: only `FIND-TASK-008-CLOSEOUT-17`,
  `FIND-TASK-008-CLOSEOUT-18`, and regressions introduced by this range.
  Earlier accepted implementation was not reopened. The complete default
  `bench:capacity` qualification (`FIND-TASK-008-CLOSEOUT-13`) remains deferred.
- Review method: source and diff trace only. The orchestrator owns command
  execution. This report does not independently claim the implementation
  record's test results.

The range changes only the capacity benchmark's evidence polling and proof
placement, plus nextest scheduling for two fixed-port capacity tests. It does
not change a deployed `wyrd-server`, its audit writer, its publisher, its
request path, or any public or durable contract. Its runtime effect is on the
benchmark process: whether that process may declare the server-owned work
drained, and whether its tests may execute concurrently.

## Deployed-path evidence

### Capacity drain and audit ownership

1. Oracle authorization decisions are enqueued without waiting for their
   staging commit. `OracleQueryAudit::stage` increments the process-local
   pending owner and `audit_outbox_pending` gauge before `try_send`
   (`crates/wyrd/wyrd-server/src/oracle/query_audit.rs:101-116`).
2. The one background writer commits each received tenant batch into
   `vala.audit_staging`; only after `commit_batch` returns does it decrement
   the pending owner and gauge (`query_audit.rs:155-179`). A successful handoff
   is therefore ordered commit first, gauge decrement second.
3. `Queue::backlog` obtains one PostgreSQL statement snapshot containing
   unsettled/expected verifier runs, committed audit rows above each tenant's
   publication watermark, and Forge demand (`crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:261-305`).
4. New `Queue::poll` scrapes every serving replica, performs that durable
   query, combines the first scrape, and, only if the combined value is zero,
   takes a second scrape and returns the original durable result combined with
   that later scrape (`capacity/evidence.rs:307-341`).
5. `Deployment::drain` supplies the real replicas' `/metrics` calls to
   `Queue::poll` and immediately passes an empty result to `Drain::judge`; a
   drained result ends the loop, while a nonempty result sleeps and polls again
   (`crates/wyrd/wyrd-testing/src/bin/capacity/step.rs:383-423`).
6. `Drain::judge` still accepts the first empty read at or before 60 seconds
   and expires a nonempty read at the limit (`capacity/step.rs:239-261`). The
   range preserves this accepted edge.

The first scrape still safely covers a decision that was pending before that
scrape: even if its commit lands during the SQL query, the first scrape keeps
the result nonzero. Committed unpublished rows still remain durable across
process restart and publisher retry because `Queue::backlog` counts every row
above `published_seq`; the range does not alter that owner.

### Live proof placement and reachability

The environment-owned helpers and live proof now reside in the in-source
`#[cfg(test)] mod pg_tests`, while metrics arithmetic and percentile tests
remain in ordinary `mod tests`
(`capacity/evidence.rs:376-456,458-708`). This closes the source-classification
violation in `FIND-TASK-008-CLOSEOUT-18`; the proof retains its `#[ignore]`
environment gate.

The integrator-directed public Oracle read is a valid producer for testing the
pending-audit handoff. It is issued in the first scrape callback after that
scrape has been captured and before `Queue::backlog` runs
(`capacity/evidence.rs:637-656`). Whether Drift or the public Oracle client
caused the pending decision is immaterial to `Queue::poll` and the outbox.
However, the proof deliberately holds `vala.audit_chain_head` until after the
poll assertion (`capacity/evidence.rs:637-671`). It therefore proves only the
case where the decision remains pending through the second scrape. It cannot
prove the successful pending-to-staging transfer that occurs after the SQL
snapshot but before the second scrape; that is the remaining false-empty
interval described below.

### Fixed-port nextest scheduling

Both ignored capacity tests start replica ordinal zero on fixed port 8080:
`tests::a_stalled_tenant_setup_stops_the_run_by_its_deadline` and
`tests::a_slow_replica_stop_leaves_the_runtime_free`
(`crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:773-993`). The new exact
default-profile override assigns only those two tests to
`release-server-ports`, whose `max-threads = 1`
(`.config/nextest.toml:62-70`). The selectors match their binary-local full
test names, no other capacity test binds port 8080, and serialization changes
only test scheduling. It introduces no server or benchmark-result regression.

## Failure and recovery assessment

| Failure or transition | System behavior and recovery | Assessment |
|---|---|---|
| First replica scrape fails | `Queue::poll` returns the scrape error before SQL; `Deployment::drain` propagates it. The benchmark cannot silently pass, and server replicas remain available until existing cleanup. | Safe. |
| Durable backlog query fails | The error returns before any empty judgment. A later external rerun starts from durable server state; this benchmark invocation fails rather than inventing zero. | Safe. |
| Second replica scrape fails | The otherwise-empty candidate is refused because the error propagates; no stale first scrape is substituted. | Safe. |
| Decision is pending at the first scrape and commits during SQL | The first scrape remains in the combined backlog, so the poll conservatively repeats even if SQL also sees the row. | Safe, accepted over-count. |
| Decision is first enqueued after the first scrape and remains pending through the second scrape | The second gauge is nonzero, so the poll repeats. The held-chain proof directly exercises this ordering. | Safe and proven. |
| Decision is first enqueued after the first scrape, its staging commit occurs after SQL takes its snapshot, and the gauge decrement occurs before the second scrape | SQL cannot see the not-yet-committed row; the second scrape sees the post-commit zero gauge. `Queue::poll` reuses the first durable zero and returns empty, and `Drain::judge` can terminate. The committed unpublished row survives and would appear on a later query, but no later query occurs after `Drained`. | **Unsafe false PASS; finding SYS-R7-01.** |
| Audit publication/Scribe is unavailable after a committed row is visible | The row stays above the durable publication watermark; repeated polling remains nonzero until recovery or the 60-second failure edge. | Safe; unchanged durable retry ownership. |
| Capacity drain future is cancelled | Poll operations are read-only. Durable rows survive; process-local pending work remains owned by the serving replica and existing server shutdown handling. A later benchmark invocation can observe durable state, but the cancelled invocation produces no pass. | Safe within benchmark lifecycle. |
| Replica dies or is replaced while draining | A metrics request fails instead of supplying zero. The changed code propagates that failure, so unrelated production availability is not changed and the benchmark does not claim a valid capacity result. | Safe. |
| Repeated nonempty polling | Existing 100 ms sleep and exact 60-second judge remain; the new second scrape occurs only on an otherwise-empty candidate, so dependency load is bounded to one extra scrape at the termination edge. | No amplification regression. |
| Two fixed-port proofs are selected together | nextest schedules the two exact tests one at a time in the default profile. | Safe; closes the observed port collision. |

## Affected capabilities

- `bench:capacity` audit-backlog drain evidence and therefore its PASS/FAIL
  credibility under REQ-171.
- Capacity report final replica scrape: the new code correctly returns the
  second scrape when that scrape is the terminating observation.
- The ignored Postgres/live-server audit-handoff proof's classification and
  focused selector (`evidence::pg_tests::...`).
- Default-profile nextest scheduling for the two capacity lifecycle proofs.

No deployed Oracle, authorization, audit staging, audit publication, Scribe,
Forge, verification, SDK, HTTP, gRPC, or tenant-isolation behavior changes in
this range.

## Recovery and proof conclusion

`FIND-TASK-008-CLOSEOUT-18` is closed. The source boundary and environment gate
now match repository rules, and pure tests remain in the credential-free
module.

`FIND-TASK-008-CLOSEOUT-17` is not closed. The range detects the exact held
pending state built by its proof, but it still permits a successful ownership
handoff to cross the durable-query/second-scrape boundary unseen. Because the
benchmark stops polling on that empty result, ordinary durable recovery on the
next poll is unavailable. This affects the benchmark's truth value, so it is
within the user's closure scope rather than a reopening of earlier code.

## Material proposed finding

### `SYS-R7-01` — INCORRECT — the post-query scrape can miss a commit made after the query snapshot

- **Violated obligation / regression boundary:** REQ-171 and
  `FIND-TASK-008-CLOSEOUT-17` require an otherwise-empty capacity drain not to
  pass while a step-caused audit decision remains unpublished. The remediation
  specifically requires the observations to bracket a later producer.
- **Exact location:**
  `crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:331-340`, composed with
  the commit-before-decrement handoff at
  `crates/wyrd/wyrd-server/src/oracle/query_audit.rs:176-178` and the terminal
  decision at `crates/wyrd/wyrd-testing/src/bin/capacity/step.rs:417-420`.
- **Evidence:** after an initial zero scrape, `Queue::poll` retains a single
  durable snapshot. If a newly pending decision's transaction commits after
  that snapshot but before the second scrape, the snapshot has no row and the
  second gauge is already zero. The returned `Backlog` is empty even though
  `vala.audit_staging` now owns an unpublished row. The live proof cannot
  falsify this ordering because its chain-head lock prevents that commit until
  after the assertion (`capacity/evidence.rs:637-671`).
- **Observable system consequence:** `bench:capacity` can emit a false PASS for
  audit drain and report a final zero backlog while an unpublished audit row
  remains. Publication or a later poll would expose/clear it, but the empty
  result ends this drain loop.
- **Testable correction boundary:** keep the existing owner and evidence
  mechanisms, but require a durable observation after the terminating second
  scrape before accepting zero, so a handoff that completed after the first
  SQL snapshot is visible. Extend the existing held-commit harness to release
  the commit between the first durable snapshot and the terminating scrape and
  assert the poll still refuses zero until publication. The public Oracle read
  remains a sufficient producer; no Drift harness, blocking audit commit,
  cross-service barrier, or production change is required.

## Verification limits

- Source review confirms the new `pg_tests` boundary and exact nextest grouping.
- The implementation record reports the focused Postgres proof passing, 15
  default capacity tests passing with five skipped, three consecutive 20/20
  all-ignored capacity runs, the server journey, format, and lints. Those runs
  were not independently executed by this reviewer.
- A green held-commit proof does not cover `SYS-R7-01`, because its held lock
  removes the commit-between-query-and-scrape transition.
- The complete unmodified default `mise run bench:capacity` remains deferred
  as directed and is not a finding here.

## Overall result

**FAIL**

`FIND-TASK-008-CLOSEOUT-18` is closed and the fixed-port serialization is
correctly scoped, but `FIND-TASK-008-CLOSEOUT-17` remains open because the
range's two-observation poll still has a reachable false-empty audit handoff.
