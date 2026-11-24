# TASK-003-R1 remediation review: PASS

All five prior findings are closed at `0d8c3fed6`, with no new validated blocker.
This is acceptance of TASK-003-R1 and the pre-benchmark code remediation, not
completion of the original TASK-003. REQ-020 performance qualification remains
pending the maintainer's independently executed final benchmark.

## Subject and authority

- Original base: `74034459d468fd0adf277e3b51c32b1a676e8e55`.
- Prior reviewed candidate: `c0900bff523e83d3666658440daa81355067cb34`.
- Remediation candidate: `0d8c3fed690d51fac5b9c4eb51af951025927f5f`.
- Actual review delta: `git diff c0900bff5 0d8c3fed6`.
- Cumulative range: `git diff 74034459d...0d8c3fed6`.
- Approved authority: [spec revision 4](../../spec.md),
  [original task](../../tasks/TASK-003-bounded-write-pipeline.md),
  [prior verdict](../TASK-003-review-2026-11-23/verdict.md), and
  [TASK-003-R1 with evidence](../TASK-003-review-2026-11-23/TASK-003-R1-pipeline-scheduling-and-proof.md).

Direct ancestry was confirmed. HEAD and all packet hashes in [subject.md](subject.md)
remained unchanged. The packet is intentionally uncommitted; unrelated dirty
benchmark/capacity work was excluded. Governing AGENTS, spec-driven workflow,
maintainer style, TESTING and Bifrost ingest/lifecycle/custody authority apply.

## Independent closure and coverage

One fresh `r1_ponytail_rev` independently checked every prior finding, actual
assertions, the remediation diff and affected callers/consumers. The orchestrator
handled execution and report packaging only. This follows the skill's default
remediation procedure; no new blocker required a separate validation pass.

The **prior finding closure matrix** and **affected-path/carry-forward matrix** in
[findings-validation.md](findings-validation.md) are incorporated as this verdict's
acceptance record. Their source locations and executable proof establish:

- FIND-TASK-003-1: **CLOSED** — ready completion precedes new preparation;
  mailbox service is bounded, with retained futures and the real task waker.
- FIND-TASK-003-2: **CLOSED** — held identities do not count toward eligible
  collection; deadline, ready draining and predecessor dispatch are tested.
- FIND-TASK-003-3: **CLOSED** — architecture now distinguishes the 32-request
  waiting target from ready draining to existing byte/64-request limits.
- FIND-TASK-003-4: **CLOSED** — a real same-shard WAL/Postgres test repeatedly
  interleaves two tenants and two tables, checking independent ACKs, fence/row
  ownership, held-retry exclusion and actual request-accounting release.
- FIND-TASK-003-5: **CLOSED** — unavailable live COMMIT reconciliation reaches
  the production poison branch; both groups remain unanswered/charged, younger
  IO drains without fencing, and restart suppresses provisional rows.

Prior SQL, recovery/custody and collateral specialist evidence remains valid:
R1 changes the existing scheduling owner and adds tests/documentation, without
changing acceptance semantics, security boundaries, resource APIs or SDKs. The
reviewer traced control, pressure, rotation, shutdown and cancellation consumers.
Additional specialist discovery would duplicate established, unchanged coverage;
no required report or expertise is missing. No second remediation task is needed.

## Verification and limits

Review independently executed all five new tests with exact qualified selectors
through the repository PostgreSQL wrapper:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --no-fail-fast -E 'test(=scribe::shards::tests::a_ready_head_completion_is_acknowledged_before_the_next_group_prepares) | test(=scribe::shards::tests::held_retries_do_not_satisfy_the_collection_target) | test(=scribe::shards::tests::a_predecessor_completion_dispatches_a_short_eligible_group) | test(=scribe::shards::tests::pg_tests::tenants_and_tables_interleave_through_both_slots_with_independent_acks) | test(=scribe::shards::tests::pg_tests::an_unavailable_commit_reconciliation_retains_both_groups_for_restart)'
```

Result: **5/5 PASS**, nextest run `040fdc9a-2068-4622-864e-f0ffccbf13fa`;
1010 tests were not selected. Both `git diff --check c0900bff5 0d8c3fed6` and
working-tree `git diff --check` passed. The task's full lib 1015/1015, 11 focused
regressions, lints, formatting, behavioral RED/GREEN and mutation runs remain
implementation-record evidence; the reviewer independently inspected assertions
and connected production paths rather than recreating execution history.

The 64-command service chunk bounds how much mailbox input becomes visible to
the scheduler in one turn; it preserves FIFO and round-robin for routed input.
The shard admission fixture exposes pod-global counters, so resource settlement
uses actual transient bytes and pending-request counts; the four tenant/table
keys separately establish row ownership. No new counter API is required.

TDD: R1 scenarios 1–2 report **PASS**; scenarios 3–4 are evidence-only, strengthened
by mutation tests. The original TASK-003 **TDD VIOLATED** history remains recorded.
The first failed-lookup fixture's diagnosis is consistent with PostgreSQL's
empty-row policy evaluation; its correction was confined to the test.

Broader Redux integration and journey suites were not rerun in this remediation
review; they remain integrated-change checks under the packet. No benchmark was
run. After required integrated checks, the maintainer independently executes
`mise run benchmark:l1:bifrost-writes` as the final required execution step,
using only `design/l1-baseline-74034459d.md` for comparison. Performance and
original task completion remain pending that evidence.

Review changed reports only: no source/spec/task edits, implementation, commit,
merge, push or deployment. This task review does not invoke change completion.
