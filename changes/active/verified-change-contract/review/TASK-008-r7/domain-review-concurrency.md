# TASK-008 round-seven concurrency and test-scheduling domain review

## Immutable subject and reviewed boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`.
- Base: `345295d8e`.
- Candidate: `54373c36856d41ffe3554c977c35e9d033918cdd`.
- Range: `345295d8e..54373c36856d41ffe3554c977c35e9d033918cdd`.
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 57, especially `REQ-171`.
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`.
- Prior review: `changes/active/verified-change-contract/review/TASK-008-r6/`.
- Remediation authority: `TASK-008-CLOSEOUT-R5-bracket-audit-drain-and-place-pg-proof.md`, with the integrator-approved public-Oracle proof substitution recorded in its implementation evidence.

This review is limited by caller direction to closure of
`FIND-TASK-008-CLOSEOUT-17` and `FIND-TASK-008-CLOSEOUT-18`, plus regressions
introduced by the range. It does not reopen code accepted in earlier rounds
except where that code participates in a potentially false benchmark result.
`FIND-TASK-008-CLOSEOUT-13` remains deferred to integration and is not a
failure here.

The candidate remained at the stated commit during this review. The checkout
has no `.codegraph/` directory, so navigation used Git, `rg`, and direct source
inspection.

## Authority and source coverage

| Boundary | Authority | Source and caller coverage | Result |
|---|---|---|---|
| Otherwise-empty audit drain must be bracketed | Revision-57 `REQ-171`; R6 `FIND-TASK-008-CLOSEOUT-17`; R5 AC-R5-1/2 and approved proof substitution | `capacity/evidence.rs:163-205,211-341,458-707`; `capacity/step.rs:239-261,307-423`; `oracle/query_audit.rs:101-215`; queued runner ordering in `verification/runner.rs:234-297,628-702` | PASS |
| Environment-owned proof placement | `architecture/agent-rules.md` Postgres/live-server test boundary; R6 `FIND-TASK-008-CLOSEOUT-18`; R5 AC-R5-3 | `capacity/evidence.rs:376-456,458-707` | PASS |
| Multi-replica collection, cancellation, and error behavior | `REQ-171`; R5 AC-R5-1/4; existing drain contract | `capacity/step.rs:383-423`; `capacity/evidence.rs:307-341`; `release_server.rs::LocalServer::metrics` | PASS |
| Exact fixed-port test scheduling | R5 AC-R5-4 and diagnosed range regression; repository nextest conventions | `.config/nextest.toml:62-70`; `capacity/main.rs:773-993`; `release_server.rs:31-51,125-127,157-232` | PASS |

Applicable repository authorities inspected include `AGENTS.md`,
`architecture/agent-rules.md`,
`architecture/references/languages/spec-driven-development.md`,
`architecture/references/languages/maintainer-style.md`, `TESTING.md`, and the
relevant Bifrost audit/query boundary in `architecture/bifrost-design.md`.

## Concurrency trace

### Poll bracketing and `Drain::judge`

`Deployment::drain` now delegates one whole observation cycle to
`Queue::poll` (`capacity/step.rs:405-418`). The callback scrapes every serving
replica in ordinal order. `Queue::poll` then reads one durable PostgreSQL
snapshot and combines it with that first scrape (`capacity/evidence.rs:324-335`).

If any run, Scribe, audit, or Forge backlog is nonzero, the poll returns that
evidence immediately. It cannot be judged drained, so omitting the second
scrape on this branch cannot create a false PASS. When the combined result is
otherwise empty, `Queue::poll` scrapes every replica again and adds the new
process-local evidence before returning (`evidence.rs:336-340`). Because the
first combined `Backlog` is known to be all-zero on this path, reusing it for
the second `with_replicas` call neither double-counts Scribe nor audit work.

`Drain::judge` receives only the bracketed result. The returned scrape vector
is the second vector that permitted the empty decision, so the final overhead
evidence also corresponds to the final observation rather than the stale
pre-query one. Elapsed time is sampled after both the durable read and required
second scrape; an observation that finishes after 60 seconds fails
conservatively instead of retroactively passing the exact boundary
(`capacity/step.rs:250-260,415-420`).

### Pending-to-durable timing

The pre-existing audit owner raises `audit_outbox_pending` before enqueue and
does not release it until the writer has committed or counted the decision
lost (`oracle/query_audit.rs:101-117,155-215`). The resulting handoff cases are:

1. A decision pending before the first scrape is counted there. A commit or
   publication during the later SQL read can only over-count that poll.
2. A decision that commits before the durable snapshot is counted above its
   tenant publication watermark unless it is already genuinely published.
3. A decision first staged after the first scrape but still process-local
   after the durable snapshot is counted by the second scrape.

The queued producer that motivated R6 cannot escape after the final scrape.
If its run remains nonterminal at the SQL snapshot, the run cell is nonzero and
the poll cannot reach the second-scrape/empty branch. If the SQL snapshot sees
the run terminal, the ordinary Oracle read and its synchronous audit staging
have already occurred before result publication and runner settlement, so the
decision is either durable/published or already reflected in the pending
gauge that the second scrape reads. Direct and query load futures have also
joined before drain begins; there is no new external step arrival after that
final observation.

Sequential multi-replica scrapes preserve this reasoning. A decision first
appearing on a replica already visited in the first sweep is covered by the
durable snapshot or the complete second sweep. If durable or another backlog
is nonzero, the iteration cannot pass and a later iteration retries. The
algorithm does not assume which replica or which audited surface produced the
pending decision.

### Error and cancellation behavior

An error from any replica scrape, the durable query, or the conditional second
scrape propagates through `Queue::poll` and `Deployment::drain`; the benchmark
fails rather than treating missing evidence as zero. Production scrape
callbacks and the SQL query are read-only. Cancellation while awaiting any of
them leaves no new owner or partial benchmark record and preserves the
existing drain cancellation contract. The extra scrape adds no task, lock,
queue, retry loop, or shutdown owner.

## Test Coverage Analysis

### Current Coverage

- `evidence::pg_tests::the_audit_backlog_holds_from_a_pending_decision_until_its_publication` uses a real public Oracle request and the existing held-chain-head harness. Its callback renders the first metrics snapshot, then issues the Oracle read and waits until the audit writer is blocked, all before `Queue::poll` starts its durable read (`capacity/evidence.rs:637-656`). Thus the decision is created strictly after the first scrape and before SQL; only the conditional second scrape can make the returned audit value one. This directly proves the false-empty interval regardless of whether Drift or the public Oracle surface originated the pending decision.
- The same test retains the pending-before-scrape, pending-to-staging, post-stop row, watermark-retention, and publication-to-zero assertions (`evidence.rs:657-704`).
- `evidence::tests::pending_decisions_add_to_staged_audit_rows` continues to prove replica aggregation and durable-plus-process-local arithmetic, while `step::tests::a_backlog_drains_only_within_the_limit` retains the exact 60-second boundary proof.
- The real server/Postgres helpers and proof now live in `#[cfg(test)] mod pg_tests` (`evidence.rs:458-707`); pure arithmetic and percentile tests remain in ordinary `mod tests` (`evidence.rs:376-456`). This closes `FIND-TASK-008-CLOSEOUT-18` without adding another test binary or changing the environment gate.

### Fixed-port scheduling

The new `release-server-ports` group has `max-threads = 1`, and its default-profile
override names exactly:

- `tests::a_stalled_tenant_setup_stops_the_run_by_its_deadline`; and
- `tests::a_slow_replica_stop_leaves_the_runtime_free`.

Both are in package `wyrd-testing`, binary `capacity`, and both start the same
stand-in through replica ordinal zero, whose fixed HTTP port is 8080
(`capacity/main.rs:773-792,829-860,919-950`; `release_server.rs:31-51,125-127`).
No other test in the capacity binary invokes `LocalServer::start` or the
stand-in's 8080 listener. The other ignored task-deadline tests execute shell
stand-ins and do not bind that port. The filter therefore serializes the exact
conflicting pair without reducing concurrency for unrelated capacity tests.

### Gaps

No material closure gap was found. A separate full queued-Drift fixture would
repeat the same pending owner and drain consumer already exercised by the
public Oracle proof; the caller explicitly approved the smaller existing
harness. A dedicated multi-replica race test is not required for closure: the
changed callback is invoked over the complete replica vector, the aggregation
logic already has two-replica-shaped unit coverage, and the producer identity
does not alter `Queue::poll`'s decision.

## Material findings

No Critical, Important, or Suggestion finding is proposed within the
user-directed scope. `FIND-TASK-008-CLOSEOUT-17` and
`FIND-TASK-008-CLOSEOUT-18` are closed, and the range introduces no concurrency
or test-scheduling regression.

## Verification limits

- Per orchestrator direction, this reviewer ran no Cargo, mise, codegen, or
  database-backed command. Runtime results are owned by the orchestrator's
  sequential verification pass.
- Static `git diff --check 345295d8e..54373c36856d41ffe3554c977c35e9d033918cdd`
  passed.
- Source inspection confirms the focused proof reaches the real Oracle,
  process-local audit owner, Postgres staging path, and publisher, but this
  reviewer did not execute it.
- The nextest filter and target names were validated statically against the
  current module paths and fixed-port callers; runtime group assignment was
  not invoked by this reviewer.
- The complete unmodified default `mise run bench:capacity` remains deferred
  as `FIND-TASK-008-CLOSEOUT-13`; no AC-040/AC-041 qualification is inferred.

## Overall result

**PASS**

The bracketed `Queue::poll` prevents the R6 false-empty interval, returns the
scrape that actually permits `Drain::judge` to accept zero, remains
conservative under multi-replica timing, and fails closed on scrape/query
errors. The live proof now occupies the required `pg_tests` boundary. The
new one-thread nextest group matches precisely the two capacity tests that
compete for release-server port 8080.
