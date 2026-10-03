# TASK-008 round-six durability domain review

## Reviewed boundary

- Immutable subject:
  `1d05642bf2c4d824de1aec27286048ec79b37e25..f8d6945041467d52020024d311ce8831a45ff4a1`.
- Approved authority: `changes/active/verified-change-contract/spec.md`,
  revision 57.
- Original task:
  `changes/active/verified-change-contract/tasks/task-008-closeout.md`.
- Prior review and remediation:
  `changes/active/verified-change-contract/review/TASK-008-r5/`, including
  `FIND-TASK-008-CLOSEOUT-17` and
  `TASK-008-CLOSEOUT-R4-audit-handoff-and-owner-shape.md`.
- User-narrowed scope: whether the remediation range closes the audit
  pending-to-staging-to-publication evidence gap and whether that range
  introduces a durability or persistent-audit-state regression. Earlier
  accepted code was not reopened except where it participates in the changed
  benchmark's PASS/FAIL decision.

The candidate remained at the named commit during this review. This checkout
has no `.codegraph/` directory, so navigation used Git, `rg`, and direct source
inspection.

## Authority and source coverage

| Boundary | Authority and source inspected | Result |
|---|---|---|
| Required saturation result | Revision-57 `REQ-171`: every audit-outbox backlog caused by a step must drain within 60 seconds before the step passes | The changed evidence path still has one reachable false-zero interval. |
| Non-blocking pre-commit ownership | `AGENTS.md` audit decisions; `architecture/agent-rules.md`; `architecture/references/architecture/patterns.md`; `architecture/bifrost-design.md` read-audit contract; `oracle/query_audit.rs:101-178` | The new `audit_outbox_pending` gauge accurately follows the existing process-local owner: it rises before enqueue and falls only after commit or counted loss. Queue-full and commit-failure semantics remain non-blocking and counted. |
| Durable staging and tenant publication state | `vala-sql/src/queries/audit_staging.rs` append, freeze, range-read, and settlement paths; audit-staging migration; `wyrd-server/src/audit/publication.rs::publish_tenant` | The changed SQL correctly treats `(data_tenant_id, seq)` staging rows above each tenant's `published_seq` as owed, and removal of the `created_at <= stopped` cut closes the prior post-stop-commit exclusion. The range does not alter RLS, tenant identity, frozen-bound replay, Scribe deduplication, watermark advance, or garbage collection. |
| Capacity consumer and poll ordering | `capacity/step.rs:320-420`; `capacity/evidence.rs:160-300`; `capacity/load.rs` request completion; queued Drift Oracle read and run settlement paths | The pre-query scrape protects a decision that is already pending when the poll starts, but it does not protect a decision produced after that scrape by step-owned work that was still running. |
| Focused proof | `capacity/evidence.rs:350-589` | The held-chain-head proof covers pending-before-scrape, commit after the stop timestamp, staging above the watermark, and publication to zero. It does not exercise a decision becoming pending between the scrape and durable backlog snapshot. |

Applicable focused references were read completely: analytical operations and
reliability, OLAP serving, Vala architecture, and architecture patterns. The
governing audit and publication sections of `architecture/bifrost-design.md`,
the relevant revision-57 spec and task sections, the prior verdict and finding
validation, and the complete R4 remediation task were inspected. No
`.codegraph/` index was available.

## End-to-end durability trace

`OracleQueryAudit::stage` increments both its atomic `pending` owner and the
process-local gauge before `try_send`. A full or closed queue decrements both
before recording loss. The writer retains both counts while a batch is queued
or committing and decrements them only after every tenant group in that batch
has either committed through `append_audit_batch` or been counted lost. This is
a conservative gauge: a multi-tenant batch can temporarily over-count an
already committed tenant while a later tenant finishes, but it cannot falsely
report zero for work it already observed.

After commit, `Queue::backlog` joins `vala.audit_staging` to
`vala.audit_chain_head` by `data_tenant_id` and counts every row with
`seq > published_seq`. `AuditPublisher::publish_tenant` freezes or reuses one
tenant range, writes it through Scribe, then advances the same tenant's
watermark and deletes through it in one settlement transaction. The changed
query therefore remains nonzero across a post-stop commit and clears only at
the existing durable publication settlement boundary.

The remaining gap is at the consumer snapshot. Each `Deployment::drain` poll
scrapes the replicas once, then executes one PostgreSQL statement that reads
run and durable-audit state, then immediately accepts an all-zero result. The
comment at `capacity/step.rs:407-409` proves only the handoff for decisions
that existed at the scrape. It does not establish that no new decision can be
produced before the database snapshot.

That production is reachable in the approved workload. A queued Drift request
returns after enqueue, while its run remains in the database. The runner later
uses `ScheduledQueryCaller` and ordinary Oracle; Oracle stages the read
decision synchronously before its query completes, and only afterwards can the
runner publish and settle the run terminally. Consequently this interleaving
is possible:

1. the drain scrape reads `audit_outbox_pending = 0` while a queued run is
   still executing before its Oracle read decision;
2. the run reaches Oracle, which stages a decision and raises the gauge;
3. the Oracle query returns and the run settles terminally, while the audit
   writer remains delayed before its staging commit;
4. the following `Queue::backlog` statement snapshots the run as terminal and
   no staged audit row, so its run and audit values are both zero;
5. `Drain::judge` accepts the stale pre-query gauge plus the later SQL snapshot
   as empty and ends the step while the replica still owns the decision.

The current held-chain-head test creates both decisions before its first
scrape, so it cannot falsify this interleaving. The removed timestamp cut and
publisher proof are otherwise correct.

## Material finding

### `DUR-R6-001` — FIND-TASK-008-CLOSEOUT-17 remains open because one scrape cannot cover a newly produced decision

- **Classification:** INCORRECT.
- **Violated obligation:** revision-57 `REQ-171` and R4 AC-R4-2 require the
  drain to remain nonzero for the entire life of every step-caused audit
  decision and forbid accepting zero while a request-completed decision is
  still process-local and uncommitted.
- **Exact changed location:**
  `crates/wyrd/wyrd-testing/src/bin/capacity/step.rs:406-420`, with the
  combined evidence semantics at
  `crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:182-200` and the
  single-statement snapshot at `evidence.rs:274-300`.
- **Producer and reachable sibling path:**
  `crates/wyrd/wyrd-server/src/oracle/query_audit.rs:101-178`;
  queued Drift runs enter through `capacity/load.rs:351-401`, execute an
  ordinary audited Oracle query through
  `verification/drift.rs:922-988`, and settle only after that query in the
  verification runner.
- **Evidence:** `Deployment::drain` obtains only one replica snapshot before
  `Queue::backlog`. `Backlog::with_replicas` can add only that earlier
  snapshot. `Queue::backlog` can simultaneously observe the producer run
  terminal and the audit row not yet committed. No barrier prevents the
  gauge from rising between those observations. The focused integration test
  starts its decisions before the first scrape and therefore does not cover
  this producer race.
- **Observable consequence:** a capacity step can falsely PASS its audit and
  run backlog cells, ending the 60-second drain while a serving replica still
  owns an uncommitted audit decision. This is the exact false-PASS class
  `FIND-TASK-008-CLOSEOUT-17` was meant to close.
- **Smallest testable correction:** keep the existing pending gauge, durable
  query, watermark semantics, and pre-query scrape that protects a pending
  decision committing during the SQL read. Before accepting empty, also
  obtain post-query replica pending evidence and include it in the audit
  decision, so a decision created after the first scrape is observed while a
  decision handed off during the query remains protected by the first scrape
  or durable row. Preserve the other backlog metrics and the exact deadline
  rule. Add a focused real-path proof that holds a queued verifier's Oracle
  audit commit, arranges for its decision to become pending after the first
  scrape, lets the run settle, and proves the drain cannot return zero until
  that decision commits and its staged row is published. A parser/arithmetic
  test is supporting evidence only.

## Verification limits

- Per orchestrator coordination, this domain pass was static/source-only and
  did not run Cargo, `mise`, codegen, or database-backed commands.
- The remediation task records the focused held-chain-head test, complete
  capacity target, audit journey, formatting, and lints as passing. Those
  results establish the implemented pre-existing-pending handoff, not the
  newly identified between-observations race.
- `FIND-TASK-008-CLOSEOUT-13` remains deferred to integration and was not used
  as evidence here.
- The known intermittent
  `verification_runtime::two_bindings_share_one_client_observation` failure is
  outside this range and was not counted against it.

## Result

**FAIL**

The range correctly adds process-local pending evidence, removes the invalid
stop-time cut, and preserves the canonical tenant-scoped staging and
publication protocol. It does not fully close `FIND-TASK-008-CLOSEOUT-17`
because its one pre-query metrics snapshot can miss a decision created by
still-running step work before the SQL snapshot accepts the producer as
terminal. No separate durability regression was found in the changed audit
owner, staging, watermark, tenant, loss, or publisher semantics.
