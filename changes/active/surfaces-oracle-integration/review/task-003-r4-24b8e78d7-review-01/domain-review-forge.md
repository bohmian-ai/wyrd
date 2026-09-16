# Forge durability and recovery domain review — PASS

## Immutable subject and authority

Base `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`; candidate `24b8e78d7420321eb8fe936b3045fcbd3bc1d26b` (tree `6a3ec967ab094a42256b44ce908abb080360e466`). The last source commit is `d6de890d83f269eab834fa323f3ebe31f1adc573` (tree `d8a727b5846f24bac7b55bbc786bcb391c55cb03`); the later commit records evidence only. I reviewed the cumulative Forge source diff, the original TASK-003 and R1–R4 remediation obligations, approved spec revision 9 (REQ-010, REQ-051, REQ-052, REQ-064, INV-025, AC-014, AC-022), `AGENTS.md` testing and code rules, `architecture/agent-rules.md`, `architecture/bifrost-design.md` maintenance authority, and `architecture/operations/reliability-and-recovery.md` Forge failure boundaries. This is the Forge domain review, not a whole-repository verdict.

## Source and boundary coverage

| Obligation | Source path and check | Result |
|---|---|---|
| Do not queue a rewrite the worker's table policy must refuse, while retaining other maintenance strategies | `forge/planning_scheduler.rs:883` calls the existing `ForgeTablePolicy::extract` before forming a small-files candidate. A failed policy withholds that rewrite only; promotion and other candidate construction continue. No second geometry owner was introduced. | PASS |
| Bound nonterminal orphan cleanup to one task per tenant-qualified table without losing demand/cursor atomicity | `vala-sql/migrations/20260910000010_forge_tasks.sql:46` adds a partial unique index over the full table identity for `ready`, `retryable`, `claimed`, `running`, and `prepared`. `ForgeTasks::enqueue_and_acknowledge` uses `ON CONFLICT DO NOTHING` inside the existing fenced transaction, returns only inserted strategies, then acknowledges demand and advances the cursor. Terminal rows permit the next cutoff. `pg_forge_tasks.rs` tests coalescing, concurrent insertion, and successor admission. | PASS |
| Preserve ordinary settlement bookkeeping during startup cleanup recovery | `forge/worker.rs:2253` routes recoverable cleanup through the same `record_settled_claim` owner used by live attempts, so completion observation and error propagation no longer diverge. The path remains fenced by the existing recovery claim and does not introduce a new effect owner. | PASS |
| Retain shutdown-only recovery assertion without changing production publication/cancellation | `compaction_admission.rs:2578–2605` waits until the successor has reconciled the exact operation to `reset`, cancels worker stop while the successor is held before new planning, then requires returned errors to contain only `shut down`. The hold is inserted after `rewrite_settlement_barrier` and before `managed_rewrite` at `forge/worker.rs:5183–5189`. The observer fields, methods, and call are all `cfg(feature = "test-support")`; they do not enter a default production build. | PASS |
| Judge production-geometry ownership after all roles drain, rather than while audit-log maintenance can still start | `production_closeout.rs:1416–1424` retains the telemetry registry through cluster shutdown and checks every Forge active-task sample is zero afterward. It does not remove the ownership assertion. | PASS |

The new table-targeted returned-attempt observer is also test-support-only. Its task-ID and recorded `Planned` table binding filter at `forge/worker.rs:1678–1712` prevents unrelated audit-log work from consuming the hold; the scheduler records `Planned` only for actually inserted strategies at `forge/planning_scheduler.rs:656–674`. These changes do not alter lease, catalog, tenant, or object-store authority in production.

## Verification limits and findings

The R4 evidence records a passing exact Postgres-backed `acceptance_unknown_recovers_from_durable_state`, the production-geometry lane (one test), the Forge journey group (13 tests), SQL and Redux integration suites, and a completed same-tree gate. I inspected source and the recorded command/selection evidence but did not rerun expensive suites. The user's explicit gate-equivalence override is sufficient for this domain; the recorded full gate also exited zero. `git diff --check` for the cumulative range is clean. The observer's unfiltered post-settlement hold is a test fixture seam, not a production behavior or a separate recovery authority.

No material Forge-domain findings. **PASS.**
