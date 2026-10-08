# TASK-008 round-eight findings validation

## Immutable subject and validation boundary

- Repository:
  `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `ea0ed46fad8aa9ffa38c29c1b2d093d9d06177e8`
- Candidate: `c4bc77a5c877c508191dc606b3cd3bb78047dc29`
- Reviewed range:
  `ea0ed46fad8aa9ffa38c29c1b2d093d9d06177e8..c4bc77a5c877c508191dc606b3cd3bb78047dc29`
- Approved authority:
  `changes/active/verified-change-contract/spec.md`, revision 57
- Original task:
  `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior review and validation:
  `changes/active/verified-change-contract/review/TASK-008-r7/`
- Remediation task:
  `changes/active/verified-change-contract/review/TASK-008-r7/TASK-008-CLOSEOUT-R6-close-final-audit-handoff.md`

The candidate remained at the named commit throughout validation. The checkout
has no `.codegraph/` directory, so navigation used Git, `rg`, and direct source
inspection. I read the complete two-file remediation diff, the applicable
repository and Bifrost authorities, the revision-57 capacity contract, the
original task, the round-seven verdict and validation, the R6 remediation, and
every round-eight discovery and follow-up report.

This validation follows the user-directed closure boundary. It decides only
whether `FIND-TASK-008-CLOSEOUT-17` is closed and whether the remediation range
introduces a regression. Earlier accepted source was inspected only where it
can make the benchmark report a false PASS or FAIL. The changed remediation
record itself is part of the range and remains subject to the repository's
task-lifecycle contract. `FIND-TASK-008-CLOSEOUT-13` and the complete default
`bench:capacity` qualification remain deferred to integration; no AC-040 or
AC-041 empirical qualification is inferred here.

No Cargo, `mise`, database-backed, or benchmark command was run in this
validation pass. `git diff --check` for the immutable remediation range passed.

## Source validation

### Pending-to-durable handoff and terminal consumer

The unchanged production producer increments its process-local pending count
and `audit_outbox_pending` before enqueueing an Oracle read decision
(`crates/wyrd/wyrd-server/src/oracle/query_audit.rs:101-117`). The writer awaits
the canonical tenant staging transaction, then decrements the pending owner
only after commit or counted loss (`query_audit.rs:155-215`). A successful
decision therefore transfers in this order:

1. the serving process owns the pending decision;
2. `vala.audit_staging` commits it;
3. the process releases the pending gauge; and
4. the row remains owed until `AuditPublisher` advances the tenant watermark.

`Queue::backlog` reads outstanding or absent expected runs, staged audit rows
above their tenant watermarks, and Forge demand in one PostgreSQL statement
snapshot (`crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:261-305`).
`Backlog::with_replicas` adds every replica's pending audit gauge and Scribe
state (`evidence.rs:163-205`). `Deployment::drain` is the sole runtime consumer
of `Queue::poll`; it passes the returned value directly to the unchanged exact
60-second `Drain::judge` boundary
(`crates/wyrd/wyrd-testing/src/bin/capacity/step.rs:383-423`).

The range corrects the previously stale observation. `Queue::poll` now takes
`S1 -> Q1`, preserves the established early return for a nonempty first pair,
and otherwise takes `S2 -> Q2` and returns the latest pair
(`evidence.rs:307-353`). In the round-seven failure schedule, the writer's
commit may cross `Q1` and release pending before `S2`; the later `Q2` necessarily
sees the committed staging row unless publication already completed. A decision
still pending at `S2` remains conservatively nonzero even if it commits before
`Q2`. A zero after publication or counted loss is correct. The empty `Q1` also
establishes that every expected queued activation exists and is terminal, while
the driver lanes have joined before drain begins, so the reviewed workload has
no later step-owned producer that can first appear after that boundary.

The added public Oracle phase forces the exact missed schedule
(`evidence.rs:725-787`). It captures zero at `S1`, creates the real decision
while a held chain-head transaction blocks its commit, enters the second scrape
only after the real `Q1`, confirms durable zero while the commit remains held,
then releases and completes the commit and waits for pending to reach zero
before returning `S2`. The assertion that the poll returns audit backlog one
can then be satisfied only by `Q2`. The proof continues through the real
publisher and demonstrates that the row remains owed above the watermark and
clears after publication. Removing `Q2` makes the recorded assertion fail at
the intended boundary. The prior still-pending phase remains intact.

The correction is reachable, uses the existing owners, and changes no
production audit, persistence, deadline, report, workload, public contract, or
test-placement behavior. The complete caller and sibling-consumer trace
supports closing `FIND-TASK-008-CLOSEOUT-17`; no discovery proposal identified
another executable regression in the range.

### Remediation task lifecycle state

The same range changes the R6 task header from `status: ready` to
`status: implemented` and appends completed implementation evidence
(`TASK-008-CLOSEOUT-R6-close-final-audit-handoff.md:1-14,181-213`). The
authoritative task contract defines the exhaustive progression as `proposed`,
`ready`, `in_progress`, `review`, and `approved`, with `superseded` for an
invalidated task
(`architecture/references/languages/spec-driven-development.md:132-179`). The
implementation skill returns `IMPLEMENTED` as an execution result that routes
the immutable candidate to independent task review; it explicitly does not
complete the task (`.agents/skills/wyrd-implement/SKILL.md:96-100`).

The invalid metadata is introduced by the reviewed range, so it is inside the
directed regression boundary. It does not invalidate the benchmark fix, but it
leaves the active remediation record in a state no workflow consumer can
interpret under the closed task lifecycle. Existing artifacts that also use
`implemented` are implementation drift, not an authority or reusable native
mechanism. The nearest independently validated precedent is
`FIND-TASK-011-13`, whose metadata-only correction set implemented remediation
tasks to `review`; the following review accepted closure after those headers
used the defined state.

## Proposed-finding decisions

| Proposal or conclusion | Decision | Independent validation |
|---|---|---|
| Behavior review's empty proposal set and FIND-17 closure | **CONFIRMED** | The producer-to-consumer trace proves that `S2 -> Q2` closes the exact successful commit/decrement crossing while preserving the nonempty early path and outer deadline. |
| Invariant review's empty proposal set and FIND-17 closure | **CONFIRMED** | The writer's commit-before-decrement ordering and PostgreSQL statement visibility make every successful decision visible at `S2` or `Q2` until publication. |
| Maintainer review's empty proposal set | **CONFIRMED** | The correction stays on the dependency-owning `Queue`; repetition remains on `Deployment::drain`; the expanded proof remains in the existing environment-owned test. |
| System review's empty proposal set | **CONFIRMED** | Scrape or SQL failure still propagates, cancellation creates no result, publication delay stays visible, and the additional conditional read adds no retry or production failure path. |
| Concurrency review's empty proposal set and FIND-17 closure | **CONFIRMED** | The source-ordered `S1 -> Q1 -> S2 -> Q2` schedule and held-commit proof close the only retained ownership-transfer gap without a new barrier. |
| Durability review's empty proposal set and FIND-17 closure | **CONFIRMED** | `Q2` is a fresh read-committed statement after `S2`; staging remains visible above the monotonic watermark until durable publication settlement. |
| `STD-R8-001` | **CONFIRMED** as new `FIND-TASK-008-CLOSEOUT-19` | `status: implemented` is outside the authoritative task-state vocabulary, and this range introduced it on a remediation already handed to independent review. |
| `FOLLOWUP-R8-001` | **CONFIRMED** and deduplicated into `FIND-TASK-008-CLOSEOUT-19` | The follow-up correctly resolves the task-state conflict from authority and the implementation handoff contract; this is the same metadata violation as `STD-R8-001`. |
| Discovery conclusion that FIND-13 stays deferred | **CONFIRMED as a limit** | The focused handoff proof does not empirically qualify the unmodified default capacity run, AC-040, or AC-041. |

No proposal supports another finding. The runtime reviewers' agreement is
corroboration only; the closure and metadata decisions above come from the
independent source and authority traces.

## Ponytail correction analysis

### FIND-17 closure

Deleting the added `Q2` would restore the proven false-zero interval. Existing
repository behavior already supplies the correct owners: the pending gauge,
canonical staging query, tenant watermark, `Queue::poll`, outer drain loop, and
public Oracle held-commit harness. No standard-library primitive or installed
dependency can replace the required second cross-owner observation. The range
therefore uses the minimum safe correction: one existing durable read only on
the otherwise-empty path, at the consumer that decides whether all owners are
empty. It does not duplicate lifecycle, add a barrier, or change production
audit semantics.

### Retained lifecycle finding

The invalid task status cannot be deleted with the front matter because the
active remediation task contract requires a status. Existing repository
authority already supplies the correct value; neither a new state, parser,
checker, dependency, nor compatibility convention is justified. The minimum
correction is the one-word metadata replacement `implemented` -> `review` on
the existing task owner. No runtime test or production edit belongs in this
correction. Static inspection plus `git diff --check` is the smallest credible
proof.

## Final deduplicated finding ledger

### `FIND-TASK-008-CLOSEOUT-19` — CONFIRMED — VIOLATION

- **Discovery source IDs:** `STD-R8-001`, `FOLLOWUP-R8-001`.
- **Violated obligation:** `AGENTS.md` section 14 and
  `architecture/references/languages/spec-driven-development.md:175-179`
  require an implemented task submitted to independent review to use the
  defined `review` lifecycle state. `IMPLEMENTED` is the implementation
  agent's return result, not a task-front-matter state.
- **Exact location:**
  `changes/active/verified-change-contract/review/TASK-008-r7/TASK-008-CLOSEOUT-R6-close-final-audit-handoff.md:4`.
- **Evidence:** the remediation range replaces `status: ready` with
  `status: implemented`, appends a completed implementation-evidence section,
  and hands the immutable candidate to this review. `implemented` is absent
  from the authoritative closed task-state vocabulary.
- **Observable consequence:** task-selection, review, and completion consumers
  cannot classify the active remediation record as ready, under review, or
  approved using the repository contract. The packet therefore represents the
  completed handoff ambiguously even though its executable correction is valid.
- **Decision-complete correction:** change only the R6 remediation header to
  `status: review`. Preserve its complete diagnosis and implementation
  evidence, the `Queue::poll` and proof changes, all earlier accepted behavior,
  and the integration deferral of `FIND-TASK-008-CLOSEOUT-13`. Do not add a new
  lifecycle state, parser, checker, alias, or runtime change.
- **Focused closure proof:** inspect the R6 header and confirm the exact value
  is `status: review`; run `git diff --check`. No executable test is warranted
  for the metadata-only correction.

## Prior-finding closure

| Prior finding | Validation result |
|---|---|
| `FIND-TASK-008-CLOSEOUT-17` | **CLOSED.** The terminal poll pairs its final replica scrape with a fresh durable observation, and the public held-commit proof forces the formerly missed `Q1 -> commit/decrement -> S2 -> Q2` schedule through the real producer, staging owner, and publisher. |
| `FIND-TASK-008-CLOSEOUT-13` | **DEFERRED TO INTEGRATION** by user direction. It is non-blocking in this closure review and supplies no AC-040/AC-041 empirical qualification. |

## Verification limits and explicit validation result

- The implementation record reports the focused Postgres proof passing and a
  deliberate no-`Q2` red run failing at the expected assertion. Discovery
  reviewers could not independently start that repository-managed proof in
  this sandbox because Docker API access was denied; the test body did not run
  in those attempts.
- Discovery records report the exact pure pending-plus-staged and drain-edge
  selection passing 2/2, plus format, targeted Clippy, and range diff checks.
  This validation independently ran only the range `git diff --check`, which
  passed.
- The recorded broader evidence includes the complete ignored-inclusive
  capacity target, fixed-port pair, release-server tests, and server journey.
  Those commands were not rerun in this validation pass.
- The complete unmodified default `mise run bench:capacity` remains deferred
  as `FIND-TASK-008-CLOSEOUT-13`.

**Validation result: one retained bounded finding.**
`FIND-TASK-008-CLOSEOUT-17` is closed and the executable remediation range
introduces no validated benchmark regression. The range does introduce the
metadata-only `FIND-TASK-008-CLOSEOUT-19`, whose correction is confined to the
existing R6 task header and requires no specification or architecture decision.
