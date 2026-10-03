# TASK-008 round-six findings validation

## Immutable subject and validation boundary

- Repository:
  `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `1d05642bf2c4d824de1aec27286048ec79b37e25`
- Candidate: `f8d6945041467d52020024d311ce8831a45ff4a1`
- Reviewed range:
  `1d05642bf2c4d824de1aec27286048ec79b37e25..f8d6945041467d52020024d311ce8831a45ff4a1`
- Approved authority:
  `changes/active/verified-change-contract/spec.md`, revision 57
- Original task:
  `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior decision and remediation:
  `changes/active/verified-change-contract/review/TASK-008-r5/`

The candidate remained at the named commit throughout validation. The checkout
has no `.codegraph/` directory, so navigation used Git, `rg`, and direct source
inspection. I read the complete remediation diff, the applicable repository
authorities, the revision-57 capacity contract, the original task, the prior
verdict and finding ledger, the R4 remediation task, and every round-six
discovery and follow-up report.

This validation follows the caller's closure boundary. It decides only whether
`FIND-TASK-008-CLOSEOUT-16` and `FIND-TASK-008-CLOSEOUT-17` are closed and
whether the remediation range introduces a regression. Earlier accepted code
is not reopened except where it makes the benchmark's PASS/FAIL result false.
`FIND-TASK-008-CLOSEOUT-13` remains deferred to integration and supplies no
capacity qualification. The intermittent
`verification_runtime::two_bindings_share_one_client_observation` failure is
on an untouched path, remains separately tracked for the integrated branch,
and is not charged to this range.

No Cargo, `mise`, codegen, or database-backed command was run in this
validation pass.

## Producer-to-consumer validation

### Replica shutdown owner

`Benchmark::clean_up` still owns client-before-replica cleanup and supplies the
output destination (`capacity/main.rs:581-604`). `Deployment` owns the live
replica collection and now owns its collection shutdown as the inherent
`Deployment::stop_replicas` operation (`capacity/step.rs:427-454`). The former
module-level workflow is deleted. The method preserves `mem::take`, newest-
first traversal, per-ordinal log names, `spawn_blocking` around synchronous
`LocalServer::stop`, join/process error conversion, ordinal result order, and
the previously documented cancellation boundary. Its sole production caller
is the owning benchmark cleanup transition; the focused slow-stop proof now
reaches that production owner path.

No helper type, trait, manager, second process owner, or behavior knob was
introduced. Deletion and reuse of the existing `Benchmark`, `Deployment`,
`LocalServer`, and Tokio owners is the minimum repository-native correction.
`FIND-TASK-008-CLOSEOUT-16` is therefore closed, with no range-introduced
process-lifecycle regression.

### Audit ownership and the drain decision

The new producer-side gauge is sound for work it observes.
`OracleQueryAudit::stage` increments the existing atomic pending owner and the
`audit_outbox_pending` gauge before enqueue, reverses both on full/closed queue,
and the writer decrements both only after the complete received batch commits
or is counted lost (`query_audit.rs:101-117,155-215`). This reuses the actual
process-local owner; it adds no queue, ledger, request wait, publisher, label,
or configuration.

The durable half is also sound. `Queue::backlog` counts every
`vala.audit_staging` row above its tenant's publication watermark and no longer
excludes audit rows committed after the captured stop timestamp
(`capacity/evidence.rs:257-300`). This preserves the canonical staging table,
tenant-scoped chain, publisher, and watermark as the sole durable owners.

The composition does not cover the entire approved workload, however. Each
drain poll scrapes all replicas once, executes one PostgreSQL snapshot, and can
immediately accept the combined result as empty
(`capacity/step.rs:406-423`). `Backlog::with_replicas` can add only the pending
values captured by that earlier scrape (`capacity/evidence.rs:182-200`). The
scrape-before-SQL order prevents a miss only for decisions already present at
the scrape.

The revision-57 mix has a reachable later producer. A queued Drift request
returns when `start_run` has committed the run row
(`capacity/load.rs:351-360,400-405`;
`components/verification/service.rs:313-355`), not when its runner has
finished. The runner later executes Drift's authenticated scheduled Oracle
query (`verification/drift.rs:951-988`). Oracle stages its non-blocking read
decision before query execution (`vala-bifrost-redux/src/oracle/mod.rs:
2211-2239,2310-2353,2723-2745`). Only after that query and result publication
can the runner durably settle the run terminally
(`verification/runner.rs:241-297,372-437,628-672`). The Oracle audit writer is
independent of that settlement.

The following producer-to-consumer interleaving is therefore reachable:

1. The drain scrapes a replica while a queued run has not yet reached its
   Oracle read; the pending gauge is zero.
2. The run reaches Oracle after that scrape and stages its decision. The
   audit writer remains blocked or delayed before commit.
3. The Oracle query and result publication finish, and the run settles
   terminally while the independent audit decision remains process-local.
4. `Queue::backlog` takes its statement snapshot. It sees all accepted runs
   created, the producer run terminal, and no committed audit staging row, so
   both `runs` and durable `audit` can be zero.
5. The drain combines that snapshot with the stale pre-query gauge and accepts
   empty while the replica still owns the decision.

The current held-chain-head proof starts both public Oracle decisions before
its first drain read (`capacity/evidence.rs:510-528`). It proves pending-before-
scrape, late durable commit, watermark retention, and publication-to-zero, but
it cannot exercise a queued verifier first producing its decision between the
sole scrape and the SQL snapshot. Thus the range closes the original
timestamp exclusion and the already-pending handoff, but not the complete
step-owned lifecycle required by REQ-171. `FIND-TASK-008-CLOSEOUT-17` remains
open.

## Proposed-finding decisions

| Proposal | Decision | Validation |
|---|---|---|
| Behavior review's empty proposal set | **REVISED** | Its accepted owner and ordinary handoff traces stand, but it did not exclude the queued-verifier producer that can stage after the sole scrape. |
| Invariant review's empty proposal set | **REVISED** | Atomic/gauge and durable-watermark invariants are correct locally; the consumer composes snapshots that do not bracket a later producer. |
| `REPO-R6-001` | **REJECTED** | `Deployment::stop_replicas` is total at its outer boundary: it always returns a vector and represents per-replica stop/join failures as values. Its rustdoc already names both the returned per-replica errors and panic-to-error conversion. A `# Errors` section would conventionally describe an outer `Result`/failure the method does not have. The adjacent, existing `Benchmark::clean_up` uses the same outcome-as-data shape. No missing error contract or range regression is established. |
| `REPO-R6-002` | **CONFIRMED** as new `FIND-TASK-008-CLOSEOUT-18` | The range adds a real Postgres/live-server proof under ordinary `mod tests`, contrary to the explicit `mod pg_tests`/`pg_*` structural rule. `#[ignore]` controls execution but does not satisfy the required source classification. |
| Maintainer review's empty proposal set | **CONFIRMED as empty for that lens** | Owner shape, naming, local documentation, and reuse are maintainable; no separate maintainer finding remains after rejecting `REPO-R6-001`. |
| System review's empty proposal set | **REVISED** | No crash, shutdown, or availability regression is present, but its PASS depends on the incomplete single-scrape audit-drain conclusion addressed by retained finding 17. |
| `DUR-R6-001` | **REVISED** and retained as prior `FIND-TASK-008-CLOSEOUT-17` | The reachable queued-Drift path establishes the false-zero interval. The smallest correction is a second pending observation only before accepting an otherwise empty durable snapshot, not a new owner or redesign. |
| Concurrency review's empty proposal set | **REVISED** | Its claim that all step-owned decisions stage before request futures join is true for public Oracle/direct requests but false for queued verifier execution, whose request returns after enqueue. |
| Process-lifecycle review's empty proposal set | **CONFIRMED as empty** | FIND-16 is closed and no process-stop regression is introduced. |
| `FOLLOWUP-R6-001` | **REVISED** into prior `FIND-TASK-008-CLOSEOUT-17` | It correctly resolves the discovery conflict and identifies the same consumer-snapshot defect as `DUR-R6-001`; it is not a second finding. |

## Ponytail correction analysis

For `FIND-TASK-008-CLOSEOUT-17`, deleting the new gauge or reverting to the
old SQL-only evidence would recreate the original miss. Existing behavior does
not supply a barrier between queued-run completion and the audit writer. The
standard library, native platform, and installed dependencies do not replace
the two existing evidence owners. The minimum correction is therefore at the
existing `Deployment::drain` decision boundary: retain the pre-query pending
scrape and durable snapshot, but before accepting an otherwise empty snapshot,
obtain current pending evidence from every serving replica and require that
post-query observation to be zero. The first scrape covers a decision handed
off during SQL; durable staging covers committed unpublished work; the second
scrape covers a decision first created after the first scrape. A queued run
that has not yet reached Oracle remains nonterminal in the SQL snapshot, so the
poll repeats. No extra endpoint, queue, ledger, wait on requests, transaction,
configuration, or public contract is justified.

For `FIND-TASK-008-CLOSEOUT-18`, an external integration target would add an
unnecessary test binary and dependency cone. Reuse the required in-source
`pg_tests` organization: place only the Postgres/live-server helpers and real
handoff proof under `#[cfg(test)] mod pg_tests`, keep pure arithmetic tests in
the ordinary test module, retain the existing environment gate, and update the
exact focused selector. This is a source-organization correction only.

## Final deduplicated finding ledger

### `FIND-TASK-008-CLOSEOUT-17` — REVISED — INCORRECT

- **Discovery source IDs:** `DUR-R6-001`, `FOLLOWUP-R6-001`.
- **Violated obligation:** Revision-57 REQ-171 requires every audit-outbox
  backlog caused by the step to drain within 60 seconds before the step
  passes. R4 AC-R4-2 requires the audit cell to remain nonzero while any
  serving replica owns an uncommitted step-caused decision.
- **Exact location:**
  `crates/wyrd/wyrd-testing/src/bin/capacity/step.rs:406-423`, composed with
  `crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:182-200,274-300`.
- **Producer evidence:** queued Drift enqueue returns at
  `capacity/load.rs:351-360,400-405` and
  `components/verification/service.rs:313-355`; later runner execution reaches
  audited Oracle at `verification/drift.rs:951-988` and
  `vala-bifrost-redux/src/oracle/mod.rs:2211-2239,2723-2745`; run publication
  and settlement follow at `verification/runner.rs:241-297,372-437,628-672`.
- **Evidence:** the drain takes only one pending snapshot before its SQL
  statement. That statement can see the queued run terminal and no staging row
  while an audit decision produced after the scrape is still owned by the
  independent writer. The existing proof creates decisions before the scrape
  and does not cover this ordering.
- **Observable consequence:** a judged capacity step can stop its 60-second
  drain timer and record an empty run/audit backlog while a serving replica
  still owns a step-caused, uncommitted audit decision. This is a false PASS in
  the exact saturation evidence path the prior finding required the range to
  close.
- **Decision-complete correction:** preserve the producer-side pending gauge,
  the pre-query scrape, the durable staging/watermark query, non-blocking audit
  semantics, and the exact deadline. At the existing drain decision boundary,
  before accepting an otherwise empty durable snapshot, scrape every serving
  replica again and require the post-query `audit_outbox_pending` total to be
  zero. If it is nonzero, retain it in the audit backlog and continue polling.
  Use the post-query scrape as the final returned evidence for a zero decision.
  Do not add a queue, ledger, request wait, transaction, endpoint,
  configuration, or new owner.
- **Focused closure proof:** through the real queued-verifier path, arrange for
  a Drift run to stage its Oracle decision after the first replica observation,
  hold that audit commit while allowing the run to settle, and prove the drain
  cannot accept zero from the intervening SQL snapshot. Then release the
  commit and prove the audit cell stays nonzero through durable publication and
  reaches zero only after the watermark advances. Retain the existing
  pending-before-scrape and late-created staging assertions, the exact
  60-second boundary proof, and the broader capacity target.

### `FIND-TASK-008-CLOSEOUT-18` — CONFIRMED — VIOLATION

- **Discovery source ID:** `REPO-R6-002`.
- **Violated obligation:** `architecture/agent-rules.md` requires tests needing
  Postgres, Docker, or a live server to live in `mod pg_tests` or a `pg_*`
  file, never the ordinary fast-test module.
- **Exact location:**
  `crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:336-566`, especially
  `the_audit_backlog_holds_from_a_pending_decision_until_its_publication` at
  lines 420-566.
- **Evidence:** the test starts `WyrdTestServer`, uses its fixture Postgres,
  holds a tenant chain-head transaction, and drives `AuditPublisher`, but the
  range places it and its database helpers in `#[cfg(test)] mod tests`.
  `#[ignore]` prevents default execution; it does not provide the mandated
  `pg_tests`/`pg_*` source classification.
- **Observable consequence:** the newly added environment-owned proof is not
  discoverable at the repository's required structural boundary, and source-
  based test selection or maintenance can mistake it for an ordinary
  credential-free unit test. This is a standards regression introduced by the
  remediation range.
- **Decision-complete correction:** reuse an in-source `#[cfg(test)] mod
  pg_tests` in `capacity/evidence.rs` for the Postgres/live-server helpers and
  real audit-handoff test. Leave the pure metrics/backlog arithmetic tests in
  the ordinary test module, retain the environment gate, and change only the
  exact test selector needed by the module path. Do not create an external test
  target or duplicate helpers.
- **Focused closure proof:** run the renamed exact `pg_tests` selector through
  the existing repository Postgres wrapper and migrations, confirm the default
  credential-free capacity test selection does not execute it, then run the
  complete capacity target and repository format/lint checks.

## Prior-finding closure

| Prior finding | Validation result |
|---|---|
| `FIND-TASK-008-CLOSEOUT-16` | **CLOSED.** Replica-set shutdown is now an inherent operation on the existing `Deployment` owner, invoked by `Benchmark::clean_up` after client shutdown, while `LocalServer::stop` and Tokio's blocking pool retain their existing responsibilities. |
| `FIND-TASK-008-CLOSEOUT-17` | **OPEN.** The range correctly covers pending work already present at the pre-query scrape and every committed unpublished row, but can miss a queued verifier's decision first created between that scrape and the SQL snapshot. |
| `FIND-TASK-008-CLOSEOUT-13` | **DEFERRED** by caller direction to integration. It is not counted against this remediation and provides no empirical AC-040/AC-041 result here. |

## Validation result

The final ledger contains retained `FIND-TASK-008-CLOSEOUT-17` and new
`FIND-TASK-008-CLOSEOUT-18`. Both corrections are bounded within the existing
capacity drain and test-organization owners. Neither requires a new product,
public API, architecture, security, compatibility, cross-service,
concurrency-semantics, resource-ownership, or persistent-data decision.
