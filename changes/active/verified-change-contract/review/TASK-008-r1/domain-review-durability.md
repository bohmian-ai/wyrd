# Durability, concurrency, and tenant-fairness review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `ce5c09ef3b559c35e65d6a3e73beebe0a69ae5bd`
- Candidate: `852894689388124960993014a46934e73c0ed2a8`
- Range reviewed: `ce5c09ef3b559c35e65d6a3e73beebe0a69ae5bd..852894689388124960993014a46934e73c0ed2a8`
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 57, specifically REQ-171, AC-040, AC-041, and the revision 57 history entry
- Task record: `changes/active/verified-change-contract/tasks/task-008-closeout.md`, “bench:capacity implementation (revision 57)”

`HEAD` resolved to the candidate before and after source inspection. Commit `f6159606c` was treated as the caller-approved harness state rather than as a finding source.

## Reviewed boundary

This pass was limited to the sensitive concurrency and persistent-data boundary:

- cross-replica queued-run claiming and fenced settlement;
- per-tenant claim fairness under a flooding tenant;
- default-queue Drift admission, draining, client-owned-byte accounting, durable row grouping, and exactly-once read-back;
- the separation revision 57 requires between capacity SLIs and test-owned correctness, fairness, exactly-once, and queue-durability judgments.

The candidate makes no material production change in this boundary. `crates/shared/wyrd-queue/src/config.rs` only updates the default queue's benchmark reference. The behavioral additions are benchmark and test-harness code.

## Authority and source coverage

| Boundary | Authority | Producer-to-sink/source coverage | Assessment |
|---|---|---|---|
| Cross-replica claims | REQ-171 revision-57 separation; durable queue and lease rules in `architecture/wyrd-design.md` and `architecture/bifrost-design.md` | `crates/wyrd/wyrd-server/tests/pg_verification_runtime.rs::two_replicas_claim_each_queued_run_exactly_once`; `verification/claims.rs::ClaimLoop`; `verification/runner.rs::VerifierRunner`; `wyrd-sql/src/queries/verifier_runs.rs::{RUNNABLE_TENANTS_SQL, CLAIM_RUN_SQL, VerifierRuns::claim}`; result publication through the bound server and `Harness::durable_rows` | PASS. Two independently scripted runtimes race the shared PostgreSQL queue, both must claim work, all 100 executions remain held until every claim exists, each run settles on attempt one, and exactly 100 durable summaries remain. The production claim is tenant-scoped, committed before execution, lease-token fenced, and uses `FOR UPDATE SKIP LOCKED`. |
| Tenant fairness | REQ-171 revision-57 test ownership; repository tenant-isolation rules | `pg_verification_runtime.rs::a_flooding_tenant_does_not_delay_another_tenants_run`; `ClaimLoop::claim_round`; `VerifierRuns::tenants_with_runnable_runs`; `RUNNABLE_TENANTS_SQL` | PASS. The integration test places a quiet tenant behind a 60-run flood, holds every execution, and proves from PostgreSQL lease deadlines that at most one flood claim precedes the quiet claim. This follows the production one-item-per-due-tenant claim round rather than a substitute test scheduler. |
| AC-041 durable ingest | AC-041; AGENTS.md test taxonomy; Bifrost ingest/durability authority | `sdks/wyrd-sdk-rust/tests/observe_run.rs::{emit_with_resubmit,sustained_hundred_feature_drift_lands_exactly_once_with_flat_client_bytes}`; public `WyrdState`/`Run::observe().drift`; default `QueueConfig`; bound Gate/Scribe server; public Oracle read-back of `vala.drift.observations` | PASS. The journey emits 1,500 logical observations with 100 tall rows each, uses all-or-none resubmission, flushes the client and server, and requires exactly 1,500 `record_id` groups of exactly 100 rows. Its periodic owner-byte samples compare the late and early thirds within one maximum-message bound and require zero owned bytes after flush. |
| Capacity/test separation | REQ-171 lines 1751-1756; revision-57 history lines 2438-2451; caller's explicit review constraint | `capacity/load.rs::{Tally,Lane::drive,Request::send}`; `capacity/step.rs::{OpRecord,Deployment::ops}`; `capacity/report.rs::{step_row,Report::render}` | FAIL. See DUR-001. |

## Material finding

### DUR-001 — VIOLATION: `bench:capacity` still judges test-owned correctness and durability

- **Violated obligation:** Revision 57 says the report judges only the stated capacity SLIs, “nothing else,” and moves judgment correctness and exactly-once queued claims/durability to tests (`spec.md:1751-1756`, `spec.md:2450-2451`). The caller explicitly resolved this review boundary as: the benchmark must not judge correctness, fairness, or durability.
- **Location:** `crates/wyrd/wyrd-testing/src/bin/capacity/load.rs:133-134,291-295,351-364`; `crates/wyrd/wyrd-testing/src/bin/capacity/step.rs:79-81,288-319`; `crates/wyrd/wyrd-testing/src/bin/capacity/report.rs:4-8,286-293,447`.
- **Evidence:** Direct requests compare every response verdict with an expected verdict and accumulate `wrong_verdicts`. `Deployment::ops` then promotes those mismatches to the judged error cell. The same method compares accepted queued activations with durable run-row counts and adds `lost` or `duplicate run` errors. `step_row` makes any such entry fail the step, and the rendered SLO contract explicitly advertises wrong judgments, loss, and duplication as benchmark verdict inputs.
- **Observable consequence:** A capacity run can fail because of correctness or exactly-once assertions that revision 57 deliberately assigned to deterministic integration/journey tests. That restores the prior benchmark's mixed acceptance role, makes the capacity verdict harder to interpret, and duplicates the new passing proofs rather than leaving the benchmark to measure traffic, runtime failures/refusals, latency, saturation, and resource use.
- **Required correction:** Keep durable run reads only where needed to calculate queued traffic completion and run backlog. Remove direct verdict-accuracy accounting and the `wrong judgment` pass/fail input from the benchmark. Remove accepted-versus-created `lost`/`duplicate run` pass/fail accounting from the benchmark; retain request refusals/failures and terminal failed-run statuses as the error SLI, and retain the run-row status/count data needed for traffic and backlog. Continue to prove verdict correctness in the direct/queued SDK tests and cross-replica exactly-once behavior in `two_replicas_claim_each_queued_run_exactly_once`. Add or adjust focused report/load tests so a benchmark record has no correctness/durability verdict categories while real request/runtime errors still fail its error cell.

## Verification evidence and limits

Executed on candidate `852894689388124960993014a46934e73c0ed2a8`:

- `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity` — 6 passed.
- `WYRD_LOG=info scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test pg_verification_runtime -E 'test(=two_replicas_claim_each_queued_run_exactly_once) | test(=a_flooding_tenant_does_not_delay_another_tenants_run)'` — 2 passed.
- `scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-sdk-rust --test observe_run -P journey --run-ignored=all -E 'test(=sustained_hundred_feature_drift_lands_exactly_once_with_flat_client_bytes)'` — 1 passed.

The full default 30-minute `mise run bench:capacity` was not rerun in this review. The task records only a reduced-duration smoke run, so final scale/saturation evidence remains limited to the implementation record. That limit does not affect DUR-001, which is established statically by the benchmark's pass/fail data flow.

## Overall result

**FAIL**

The cross-replica claim, tenant-fairness, and AC-041 durable-ingest proofs are credible and passed focused execution. The candidate nevertheless violates revision 57's required ownership split by retaining correctness and exactly-once durability as `bench:capacity` verdict inputs.
