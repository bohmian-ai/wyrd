# Capacity and Performance-Evidence Domain Review

## Reviewed boundary

- **Immutable subject:** base `f6159606c5c959e8fcc3423574ab0e7e6c86ee13`, candidate `5c3bb79b3598abd88a3a234611fc400096adc975`. The candidate remained `HEAD` throughout this review.
- **Approved authority:** `changes/active/verified-change-contract/spec.md`, approved revision 57, especially REQ-171, AC-040, AC-041, and the revision-57 history entry. I also applied `architecture/references/domain/analytical-operations-reliability.md` to the validity of capacity claims and the repository testing rules to the supplied proof.
- **Task and prior evidence:** `tasks/task-008-closeout.md`; the r1 verdict, capacity-domain review, validated ledger, and remediation; and the r2 capacity review. The r2 authority blocker does not apply here because the caller explicitly selected candidate revision 57.
- **Runtime path traced:** `mise.toml` command entry and environment; `wyrd-testing` target registration; `capacity/main.rs` lifecycle and step sequence; `fixture.rs` reference inputs; `load.rs` open-loop scheduling and sample accounting; `step.rs` measurement windows, durable reconciliation, resources, and drain; `evidence.rs` metric/SQL meanings; `judge.rs` provider-wait measurement; `report.rs` cell and verdict semantics; `profile.rs`; and `release_server.rs` process, cgroup, migration, setup, shutdown, and storage environment handling.
- **Supporting boundaries traced:** `QueueConfig::default`, the AC-041 sustained-ingestion journey, the two-replica queued-run/fairness tests, deletion of the former capacity binaries/tasks, and the task's recorded unit, integration, lint, and reduced-smoke evidence.

## Authority and source coverage

| Concern | Authority | Source and evidence inspected | Result |
|---|---|---|---|
| One server-capacity entry point and removal of legacy capacity harnesses | REQ-171; revision-57 history | `mise.toml:507-524`; `wyrd-testing/Cargo.toml:145-151`; full base-to-candidate deletion/rename diff; repository search excluding historical `changes/` records | PASS |
| Four identical tenants and fixed open-loop workload rates | REQ-171 workload | `capacity/main.rs:63-67`; `capacity/load.rs:255-515`; `load::tests::mix_offers_the_required_rates` | PASS |
| AC-040 reference inputs and five-kind coexistence | AC-040 | `capacity/fixture.rs:39-176,202-302`; direct/queued request construction in `load.rs`; metric-kind mapping in `evidence.rs` | PASS structurally |
| Warmup, ramp, knee, one-replica sustained, two-replica sustained, and `2K` scale-out | REQ-171 steps and verdict | `capacity/main.rs:346-394`; `report.rs:394-423`; verdict tests | PASS |
| Offered/started/accepted/completed/error accounting and operation traffic cells | REQ-171 traffic/errors; analytical measurement fidelity | `capacity/load.rs:119-144,255-331,369-455`; `capacity/step.rs:320-485`; `capacity/report.rs:131-174,266-342` | PASS |
| Direct paired engine-overhead sample count and p95 semantics | AC-040 | `capacity/evidence.rs:51-145`; `capacity/report.rs:192-220`; histogram-delta and SLO tests | PASS structurally |
| Judge engine overhead and provider wait remain separate | AC-040 | `capacity/judge.rs:31-153`; `step.rs:326-378`; `report.rs:192-233`; focused report test | PASS |
| AC-041 default queue, `L = 200` ingest rate, refusal/drain semantics, and supporting durability proof | AC-041; REQ-172-REQ-177; INV-019 | `load.rs:46-49,424-441,480-515`; `QueueConfig::default`; `step.rs:423-485`; `report.rs:236-264`; recorded `sustained_hundred_feature_drift_lands_exactly_once_with_flat_client_bytes` journey | PASS structurally; empirical default-run proof deferred below |
| Backlog meanings and 60-second observation boundary | REQ-171 saturation; analytical operations authority | `evidence.rs:89-104,159-297`; `step.rs:239-261,383-420`; drain and staged-member tests | PASS |
| Comparable CPU and memory measurement window | REQ-171 saturation | `step.rs:145-232,320-380`; resource-window test | PASS |
| One shared report table, cell meanings, JSON evidence, and three-step verdict | REQ-171 report/verdict | `report.rs:47-129,266-369,371-563`; render/verdict/SLO tests | PASS |
| Fixed environment and artifact identity | REQ-171 envelope; analytical performance-evidence authority | `mise.toml:507-524`; `capacity/main.rs:318-339,576-605`; `release_server.rs:130-260,576-616` | PASS for declared server binary, resource envelope, workload, replicas, Postgres/RustFS command environment, and report artifacts; FAIL for the command-wide elapsed boundary (CAP-R3-001) |
| Complete default command within 30 minutes, including setup | REQ-171 | `mise.toml:507-524`; `capacity/main.rs:124-188,227-344,397-430,500-532,621-637`; `release_server.rs:130-190,347-388,618-635`; lifetime unit test; reduced-smoke wall/report times | FAIL: CAP-R3-001 |

## Verification performed and limits

- `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity`: **14 passed, 0 failed**. This directly covers rate arithmetic, query window, percentile math, staged-Scribe accounting, drain boundaries, resource-window arithmetic, report shape, error categories, judge-wait separation, verdict selection, and a cooperatively cancellable lifetime future.
- The r1 remediation's focused real-server results were inspected: two-replica claim/fairness, AC-041 100-feature exactly-once/flat-byte ingestion, direct/queued judgment journeys, formatting, lints, and Clippy were recorded green. They are supporting correctness evidence rather than a substitute for a capacity run.
- The shortened two-replica smoke proves topology and report plumbing only. Its command wall time was 136 seconds while the report recorded 102 seconds, directly illustrating that the report's lifetime begins after wrapper setup/build work has already occurred.
- The unit lifetime test uses `std::future::pending`, which yields cooperatively. It does not exercise the reachable synchronous `Command::output()` migration/setup path, and it cannot prove that `timeout_at` regains control from a blocked poll.
- Per the caller's explicit sequencing decision, **FIND-TASK-008-CLOSEOUT-13 is deferred to integration and is not a blocker for this candidate**. The unmodified `mise run bench:capacity` has not been run or promoted here; the integrator must run it after the other workstreams merge and preserve the complete Markdown/JSON report, configuration identity, verdict, and command wall time. Consequently this review makes no empirical throughput or scale-out claim from source shape, unit tests, or the reduced smoke.

## Material proposed finding

### CAP-R3-001 — INCORRECT: the advertised 30-minute boundary does not cover the complete default command or preempt blocking setup children

- **Prior finding:** continuation and narrowing of `FIND-TASK-008-CLOSEOUT-2`; the r1 remediation closed cooperative in-binary timing but not the reachable command/process boundary identified in r2.
- **Violated obligation:** REQ-171 requires the default `mise run bench:capacity` command, including setup, to complete within 30 minutes. Expiry must yield a failed diagnostic artifact rather than an indefinitely running command.
- **Exact location:** `mise.toml:507-524`; `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:124-188,240-344,621-637`; `crates/wyrd/wyrd-testing/src/release_server.rs:147-190,618-635`.
- **Evidence:** Docker/RustFS startup, the Postgres wrapper, release-server build, and capacity-binary build all execute before `Benchmark::prepare` creates `Lifetime`; none contributes to `Report::total_seconds`. Once the timer exists, `LocalServer::start` executes `migrate` and four `setup` children through synchronous `Command::output()`. Those calls block the Tokio worker inside a future poll, so `tokio::time::timeout_at` cannot observe its deadline until the child exits. The focused test proves only a yielding future. The recorded smoke's 136-second command wall time versus 102-second report total confirms the artifact omits a material part of the command.
- **Observable consequence:** a slow build/emulator/Postgres setup can make the command exceed 30 minutes while a later report still records less than 30 minutes, and a wedged migration or setup child can prevent timeout, cleanup, and report creation entirely. Operators therefore cannot rely on the claimed command bound or the report's elapsed value as faithful REQ-171 evidence.
- **Required testable correction:** place the existing benchmark command's complete setup-to-report process tree under one absolute 30-minute owner, propagate its remaining deadline into the benchmark lifecycle, and ensure migration/setup children are terminated and reaped on expiry while retaining logs and writing a failed report when the report owner can run. Preserve the approved workload, SLOs, server defaults, and absence of a new public timeout knob. Add a process-level focused proof with a deliberately stalled setup child that demonstrates bounded termination, cleanup, retained diagnostics, and elapsed reporting from command entry.

## Deferred integration evidence

`FIND-TASK-008-CLOSEOUT-13` remains real but is deliberately sequenced after the approved audit-outbox and Forge workstreams merge. It is recorded here as **DEFERRED**, not as a proposed blocker and not as evidence that the candidate passes AC-040/AC-041 empirically. Integration must close it with the unmodified default command; a shortened smoke or configuration inspection cannot do so.

## Overall result

**FAIL**

The candidate structurally implements the approved workload, sample accounting, SLO cells, measurement windows, report/verdict semantics, environment envelope, AC-040/AC-041 fixture meanings, and removal of legacy benchmarks. The independently reachable whole-command deadline defect remains material under REQ-171. The absent full default run is separately recorded as caller-directed deferred integration evidence and does not contribute to this FAIL result.
