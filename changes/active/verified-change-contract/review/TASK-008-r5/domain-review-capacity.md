# Capacity and Performance-Evidence Domain Review

## Reviewed boundary

- **Immutable subject:** base `f6159606c5c959e8fcc3423574ab0e7e6c86ee13`, candidate `0973a03e5a389ea0fb3635c6d9175e25db0a6da0`. The candidate still matched `HEAD` after source inspection and focused verification.
- **Approved authority:** `changes/active/verified-change-contract/spec.md`, approved revision 57, specifically REQ-171, AC-040, AC-041, and the revision-57 history entry.
- **Task and prior hypotheses:** `changes/active/verified-change-contract/tasks/task-008-closeout.md`; the r1-r4 capacity reports, verdicts, validated ledgers, and remediation tasks; and `review/TASK-008-r4/TASK-008-CLOSEOUT-R3-process-boundary-and-proof.md` with its implementation evidence. Prior findings were treated as hypotheses and checked against the cumulative candidate.
- **Runtime path traced:** `mise.toml` command and release envelope; `capacity/main.rs` sequence, knee, lifecycle, cleanup, and report construction; `fixture.rs` reference workloads; `load.rs` open-loop arrivals and offered/accepted/completed accounting; `step.rs` measurement, resource, drain, reconciliation, and durable-run accounting; `evidence.rs` metric deltas and Postgres backlog probes; `judge.rs`; `profile.rs`; and `report.rs` SLO cells, shared table, and final verdict.
- **Caller authority applied:** `FIND-TASK-008-CLOSEOUT-13`, the full unmodified default benchmark, is deferred to integration after other workstreams merge. It is a verification limit, not a candidate blocker or evidence for an empirical capacity claim. The report intentionally precedes Postgres-wrapper teardown; this review does not require `Report::total_seconds` to include post-report teardown.

## Authority and source coverage

| Concern | Governing authority | Source and evidence inspected | Result |
|---|---|---|---|
| One server-capacity command and prescribed release envelope | REQ-171; revision-57 history | `mise.toml:507-556`; `crates/wyrd/wyrd-testing/Cargo.toml`; cumulative target/task scan | PASS |
| Four identical tenants and fixed operation rates | REQ-171 workload | `capacity/main.rs:65-79`; `capacity/load.rs:480-515`; `load::tests::mix_offers_the_required_rates` | PASS |
| Open-loop offered, started, accepted, completed, missed, refused, lost, and wrong-judgment meanings | REQ-171 Traffic and Errors | `capacity/load.rs:119-144,255-331,369-455`; `capacity/step.rs:125-143,423-485`; `capacity/report.rs:131-174` | PASS |
| Warmup, ramp to first miss, knee, sustained on one and two replicas, and `2K` scale-out | REQ-171 Steps | `capacity/main.rs:427-475`; `capacity/step.rs:95-123` | PASS |
| Per-operation traffic at least 95% and zero approved error categories | REQ-171 SLOs | `capacity/report.rs:131-174,266-342`; `report::tests::{every_slo_failure_fails_the_step,every_approved_error_category_fails_without_duplicate_judgment}` | PASS |
| Paired direct overhead, non-judge threshold, sample floor, and separate provider wait | REQ-171; AC-040 | `capacity/evidence.rs:51-85`; `capacity/report.rs:35-45,192-234`; production metric definition in `wyrd-server/src/app/metrics.rs:88-93`; focused report/evidence tests | PASS structurally |
| Default-queue ingest, `QUEUE_FULL`, 100-feature rate, and client drain | REQ-171; AC-041 | `capacity/load.rs:46-48,146-229,424-441,480-515`; `capacity/report.rs:236-242`; task-recorded real-server durability/flat-byte journey | PASS structurally; empirical default-run qualification deferred |
| Server backlogs and exact 60-second drain edge | REQ-171 Saturation | `capacity/evidence.rs:89-104,159-180,223-265`; `capacity/step.rs:239-261,359-420`; drain and report boundary tests | PASS |
| CPU and peak-memory values use one measured interval | REQ-171 Saturation | `capacity/step.rs:145-232,320-345`; `step::tests::resources_use_the_readings_interval` | PASS |
| One shared report table with step and operation rows | REQ-171 Report | `capacity/report.rs:344-369,444-564`; `report::tests::report_renders_one_shared_table` | PASS |
| Verdict requires one-replica sustained plus both two-replica steps | REQ-171 Verdict | `capacity/report.rs:395-424`; `report::tests::verdict_needs_every_verdict_step` | PASS |
| Latest remediation preserves workload/SLO semantics while closing process ownership, async cleanup, and exact proof commands | R3 AC-R3-1, AC-R3-3, AC-R3-4, AC-R3-5; caller rejection of AC-R3-2 | `mise.toml:510-556`; `capacity/main.rs::stop_replicas` and task-entry proofs; current revision-57 task matrix | PASS for the capacity boundary |

## Domain assessment

### Offered and achieved traffic

`mix` divides global `L` evenly over four tenants and then over the required kind/query shares. At `L = 200`, its executable check proves direct `100/s`, queued `100/s`, ingest `500 observations/s` (therefore `50,000 rows/s` at 100 features), and query `100/s`. Each lane schedules arrivals from the common step start independently of responses. A missed driver permit remains in `offered`, counts in `missed`, and lowers achieved traffic rather than silently reducing demand.

For direct, ingest, and query, `completed` counts accepted requests whose completion falls inside the arrival window. For queued work it counts durable runs settled inside that same window, while accepted-but-not-created work becomes `lost` only after the run backlog drains; an undrained queue already fails the backlog SLO. Refusals, wrong direct judgments, bad terminal run statuses, and lost work feed the error cell. These producer-to-report meanings are internally consistent and do not double-count the intentionally failing inputs whose correct outcome is a failed judgment.

### SLOs, sampling, drain, and resources

Every operation must independently reach 95% traffic and zero errors. The overhead calculation sums cumulative histogram bucket deltas over every serving replica for exact `kind` plus `mode="direct"`, takes the p95 bucket bound, applies `< 10 ms` to the four non-judge kinds, and requires at least 1,000 samples per kind only in the one-replica sustained step. Judge engine overhead and local-provider wait remain separately reported and unjudged.

The ingest SLO uses the longest tenant-client flush and fails above one second; `QUEUE_FULL` is already an error. The backlog probe covers current-step run work, Scribe persistence/immutable/staging work, audit staging above the publication watermark, and unsettled Forge demand. Empty observed at or before 60 seconds passes; empty first observed afterward fails. CPU delta and `memory.peak` are opened and closed by one `ResourceWindow`; CPU divides by the actual interval between those readings rather than a planned duration.

### Report, verdict, and evidence credibility

The Markdown uses one header and one common SLI schema, with each step row immediately followed by four operation rows. `Report::passed` refuses early failure or missing evidence and requires exactly the sustained one-replica, sustained two-replica, and scale-out two-replica records to pass. The accepted integrator decision leaves `total_seconds` as time to the pre-teardown report; that is the intended artifact boundary and is not a defect in this review.

The current task record now gives each named focused capacity test a complete repository-pinned command. The complete capacity target passed locally. The new task-entry process proof also passed locally. The normal-stop heartbeat proof could not be revalidated in this sandbox because `LocalServer::start` reached `systemd-run` and the environment has no permitted user bus; it failed during fixture boot before exercising the stop/heartbeat assertion. The implementation record reports that proof passing on the required delegating-systemd host, and source inspection confirms that normal stop is executed through `tokio::task::spawn_blocking`. This host limitation does not expose a capacity-semantics defect.

## Material proposed findings

None.

The cumulative capacity implementation satisfies the revision-57 structure and meanings reviewed here. No new or reopened capacity-domain finding is supported by the current source. In particular, the integrator-rejected post-report teardown requirement is not retained, and the caller-deferred default run is not converted into a candidate finding.

## Verification limits

- `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity`: **14 passed, 0 failed, 4 skipped**.
- `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity --run-ignored=only -E 'test(=tests::a_hung_setup_step_is_killed_with_its_descendants_by_the_deadline)'`: **1 passed**.
- `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity --run-ignored=only -E 'test(=tests::a_slow_replica_stop_leaves_the_runtime_free)'`: **fixture boot failed** because the sandbox denied the systemd user bus (`Failed to connect to bus: Operation not permitted`); the tested heartbeat/stop path was not reached. The implementation record reports **1 passed** on a compatible host.
- `git diff --check f6159606c5c959e8fcc3423574ab0e7e6c86ee13..0973a03e5a389ea0fb3635c6d9175e25db0a6da0`: passed.
- The full unmodified `mise run bench:capacity` was not run. Per caller direction, `FIND-TASK-008-CLOSEOUT-13` remains deferred to integration and supplies neither a blocker nor empirical AC-040/AC-041/scale-out evidence for this candidate.
- This review makes no measured capacity claim from source shape, unit tests, the historical reduced smoke, or the deferred run.

## Overall result

**PASS**

The workload and arrival accounting, SLO computation, AC-040 sampling, AC-041 ingest semantics, common resource interval, complete backlog/drain boundary, shared report table, and three-step verdict are coherent with approved revision 57. The latest delta preserves those semantics while closing the accepted process-group, async-cleanup, and exact-command remediation boundaries. Residual empirical qualification is explicitly deferred rather than misrepresented as completed evidence.
