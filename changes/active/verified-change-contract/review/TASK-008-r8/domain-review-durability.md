# TASK-008 round-eight durability and PostgreSQL visibility review

## Immutable subject and review boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `ea0ed46fa`
- Candidate: `c4bc77a5c877c508191dc606b3cd3bb78047dc29`
- Range: `ea0ed46fa..c4bc77a5c877c508191dc606b3cd3bb78047dc29`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 57
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior review: `changes/active/verified-change-contract/review/TASK-008-r7/`
- Remediation authority: `TASK-008-CLOSEOUT-R6-close-final-audit-handoff.md`

The candidate remained at the named commit during this review. The checkout
has no `.codegraph/` directory, so navigation used Git, `rg`, and direct source
inspection. Per the user-directed closure scope, this report decides only
whether `FIND-TASK-008-CLOSEOUT-17` is closed and whether this remediation
range introduces a durability, PostgreSQL-visibility, or persistent-state
regression. Earlier accepted code is not reopened except where it determines
whether the capacity benchmark can report a false PASS or FAIL.
`FIND-TASK-008-CLOSEOUT-13` and the full default `bench:capacity` run remain
deferred to integration.

## Reviewed boundary and source coverage

This review traced the corrected terminating poll and its proof through the
complete ownership handoff:

1. `OracleQueryAudit::stage` increments the process-local owner and
   `audit_outbox_pending` before enqueue
   (`crates/wyrd/wyrd-server/src/oracle/query_audit.rs:101-117`).
2. `OracleAuditWriter::run` waits for the canonical tenant transaction to
   commit, or counts the batch lost, before decrementing that owner and gauge
   (`query_audit.rs:155-178`, `187-215`).
3. `Queue::backlog` reads `vala.audit_staging` rows whose sequence is above
   each tenant's `published_seq` in one PostgreSQL statement snapshot
   (`crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:261-305`).
4. The changed `Queue::poll` now performs `S1 -> Q1`, returns a nonempty
   result immediately, and otherwise performs a complete second
   `S2 -> Q2` observation before accepting zero
   (`evidence.rs:307-353`).
5. `Deployment::drain` gives that result directly to the 60-second terminal
   decision (`crates/wyrd/wyrd-testing/src/bin/capacity/step.rs:383-423`).
6. `AuditPublisher::publish_tenant` publishes the frozen range durably before
   settlement advances `published_seq` and deletes staged rows
   (`crates/wyrd/wyrd-server/src/audit/publication.rs:259-285` and
   `crates/vala/vala-sql/src/queries/audit_staging.rs:402-448`).
7. The extended public Oracle proof holds the chain head, creates a decision
   after `S1`, lets the actual `Q1` complete while the commit remains blocked,
   releases the commit inside the `S2` callback, waits until pending is zero,
   and then requires `Q2` to report the committed row until publication
   (`evidence.rs:725-786`).

## Authority coverage

| Authority | Applied obligation | Result |
|---|---|---|
| `spec.md` revision 57, REQ-171 | A judged step may pass only after every audit-outbox backlog caused by the step drains within 60 seconds. | **PASS** — a terminating empty result is now based on the post-scrape durable statement, so the known pending-to-staging handoff cannot be missed. |
| R7 `FIND-TASK-008-CLOSEOUT-17` and R6 AC-R6-1 | Pair the final replica observation with a fresh durable observation before accepting zero. | **PASS** — `Queue::poll` executes `Q2` after `S2` and returns `S2 + Q2`, rather than reusing `Q1`. |
| R6 AC-R6-2 | The public Oracle proof must force commit and pending decrement after `Q1` but before `S2`, then prove the row remains owed until publication. | **PASS** — the held chain-head schedule does so at `evidence.rs:725-786`; removing `Q2` would return the stale zero `Q1`. |
| `architecture/bifrost-design.md` read-audit and publication contract | Pending ownership lasts until canonical staging commit or counted loss; staging remains owed above the monotonic publication watermark. | **PASS** — the remediation observes those existing owners without changing them. |
| R6 AC-R6-3 and user regression boundary | Preserve the audit writer, publisher, watermark, 60-second drain owner, workload, and report semantics. | **PASS for the reviewed domain** — the range changes only the empty poll's second durable observation and its focused proof; no production audit or persistent-state contract changes. |

## PostgreSQL visibility and ownership-transfer analysis

The prior false-zero ordering was `S1 = 0`, `Q1 = 0`, commit/decrement,
`S2 = 0`. PostgreSQL's `READ COMMITTED` statement visibility made the newly
committed row invisible to the already-completed `Q1`, while the writer's
commit-before-decrement order made it absent from `S2`. The changed poll does
not reuse either stale owner. It issues `Q2` only after `S2` returns. Therefore:

- if the decision is still process-owned at `S2`, the pending gauge makes the
  second result nonzero;
- if it commits before or during `S2`, commit completes before pending falls,
  and the later `Q2` statement sees the staging row;
- if it commits after `S2` but before `Q2`, the same later statement sees it;
- if it remains uncommitted through `Q2`, it was necessarily pending at the
  earlier `S2` under the writer's ordering;
- if publication races between `S2` and `Q2`, a zero durable value is correct
  only after the publisher has durably appended the frozen range and advanced
  the watermark in settlement; and
- a failed commit may clear pending without a row only through the existing
  counted-loss path, which is already an error signal rather than hidden
  persistent work.

The focused proof is faithful to this visibility boundary. Entry into its
second scrape callback establishes that the poll's real first combined reading
was empty. The chain-head lock prevents the staged insert transaction from
committing during `Q1`; the additional zero query inside the callback confirms
the same durable precondition. Releasing the lock and waiting for the public
pending metric to reach zero orders the canonical commit before the returned
`S2` snapshot. The poll can then return audit backlog one only by executing
the new `Q2`. A later read remains one above `published_seq`, and manual
publication makes it zero only after the publisher's normal durable path.

The extra durable read is confined to an otherwise-empty poll. It cannot make
a still-owed staged row disappear: each statement observes a later database
state, and the only reviewed transition from owed to not owed is watermark
settlement after durable publication. It can conservatively require another
outer drain iteration, but it does not create a false PASS or false FAIL.

## Material proposed findings

None.

## Prior-finding closure

| Finding | Result |
|---|---|
| `FIND-TASK-008-CLOSEOUT-17` | **CLOSED.** `S2` is paired with a new `Q2`; the held-commit proof forces the previously missed commit/decrement interval and demonstrates nonzero staging ownership until publication. |
| `FIND-TASK-008-CLOSEOUT-13` | **DEFERRED** to integration by user direction; no AC-040/AC-041 empirical qualification is inferred here. |

## Verification limits

- `git diff --check ea0ed46fa..c4bc77a5c877c508191dc606b3cd3bb78047dc29`: passed.
- Exact pure pending-plus-staged arithmetic and 60-second drain-edge selection:
  2 passed.
- The exact Postgres held-commit proof could not start in this sandbox because
  access to the configured Docker API socket was denied. The remediation
  record reports that proof passing, but this review treats that as recorded
  evidence rather than an independent execution. Its ordering and assertions
  were independently validated against the Oracle writer, PostgreSQL query,
  publisher, and settlement source above.
- The full default `mise run bench:capacity` qualification was not run and
  remains explicitly deferred.

## Result

**PASS**

`FIND-TASK-008-CLOSEOUT-17` is closed for the reviewed durability and
PostgreSQL-visibility boundary. The range introduces no material regression
in ownership transfer, canonical staging, publication, or the capacity
benchmark's saturation verdict.
