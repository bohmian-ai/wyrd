# Behavior Review: TASK-008 bench:capacity (revision 57)

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `ce5c09ef3b559c35e65d6a3e73beebe0a69ae5bd`
- Candidate: `852894689388124960993014a46934e73c0ed2a8`
- Cumulative diff: `ce5c09ef3..852894689`
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 57, especially REQ-171, AC-040, AC-041, and the revision 57 history entry
- Original task/evidence: `changes/active/verified-change-contract/tasks/task-008-closeout.md`, especially `bench:capacity implementation (revision 57)`
- Candidate identity was rechecked after review and remained `852894689388124960993014a46934e73c0ed2a8`.

## Navigation and caller trace

`mise run bench:capacity` builds release `wyrd-server` and runs the `capacity` binary. `main::benchmark` provisions four tenants, constructs a `Deployment`, runs warmup/ramp/sustained/scale-out plans, and hands each plan to `Deployment::run`. `Deployment::run` builds the per-tenant `load::mix`; each `Lane::drive` schedules public-client requests and returns a tally; `evidence::{Scrapes, Queue}` reads server metrics and PostgreSQL backlog/run state; `report::{step_row, Report::passed, Report::render}` produces the verdict and report. The realistic consumers are the operator invoking the mise task and the Markdown/JSON report readers.

The review also traced the capacity-task registry in `mise.toml` and `wyrd-testing/Cargo.toml`, the AC-041 real-server journey in `sdks/wyrd-sdk-rust/tests/observe_run.rs`, and the moved correctness/isolation proofs in `pg_verification_runtime.rs` and `drift_verification.rs`.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-171: `bench:capacity` replaces the verification and ingest capacity tasks and no other server capacity benchmark exists | `mise.toml:511-528` and `Cargo.toml:153-159` add the consolidated task/binary and the two superseded task/binary trees are removed; however `mise.toml:507-509` and `Cargo.toml:145-151` still register and describe `bench:bifrost:query-capacity` as a release-server capacity benchmark | Static source inspection; task evidence at `task-008-closeout.md:1565-1569` explicitly says it was not touched | **FAIL** (BHV-001) |
| REQ-171: default run completes within 30 minutes including setup | `main.rs:114-176` measures setup and `main.rs:257-276` records total time, but `report.rs:386-397` does not use `total_seconds` in the verdict and the run has no 30-minute deadline | Only a shortened smoke run is recorded at `task-008-closeout.md:1544-1554`; no default-run duration proof exists | **FAIL** (BHV-002, BHV-004) |
| REQ-171 workload: public Rust client, four identical tenants; direct and queued at `L/2` over five equal kinds; ingest at `2.5L` with 100 features/default queue; lookup and aggregate queries at `L/2` over five minutes | `main.rs:56-57`; `load.rs:146-212, 239-425, 427-483`; AC-040 fixtures in `fixture.rs` | `mix_offers_the_required_rates`, `queries_read_the_last_five_minutes`, and the recorded smoke path | **PASS** |
| REQ-171 step sequence and knee selection | `main.rs:188-246` implements warmup, ramp with first-failure stop, sustained `K` on one and two replicas, and scale-out `2K` | Recorded smoke traversed all steps | **PASS** |
| REQ-171 traffic/error/latency/saturation SLIs and verdict steps | `step.rs:136-330`; `evidence.rs:62-277`; `report.rs:129-349, 370-397` | Capacity binary unit tests recorded as six passing; the focused SLO and verdict tests are listed in task evidence | **PASS**, subject to the empirical-proof and sample-floor failures below |
| AC-040: the four non-judge kinds stay below 10 ms p95 in every judged step and the one-replica sustained step has at least 1,000 samples per kind | Workloads match AC-040 and `report.rs:184-210` enforces the threshold/floor, but the fixed `L=50` mix yields only 900 samples per kind in 180 seconds | Task evidence itself records the construction failure at `task-008-closeout.md:1561-1563`; no full default benchmark was run | **FAIL** (BHV-003, BHV-004) |
| AC-041 benchmark: at `L=200`, 500 observations/s × 100 rows, zero `QUEUE_FULL`, drain at most one second at or below knee | `load.rs:393-410, 449-483`; `Lane::drive` times flush at `load.rs:302-306`; refusals enter the zero-error cell | Only the `L=20`, 15-second smoke is recorded; it does not exercise the required `L=200` rate | **FAIL** (BHV-004) |
| AC-041 tests: real server, exactly 100 rows per `record_id`, exactly once, flat client-owned bytes under sustained emission | `observe_run.rs::sustained_hundred_feature_drift_lands_exactly_once_with_flat_client_bytes` drives public `WyrdState`, checks queue bytes, then groups durable rows by `record_id` | Exact ignored journey command and passing result are recorded at `task-008-closeout.md:1507-1508` | **PASS** |
| Correctness/isolation remain tests rather than benchmark judgments | Benchmark checks direct verdict correctness only to classify a wrong result as an error; fairness and queued claim correctness are outside its report. Dedicated tests cover two-runtime claiming, tenant claim order, direct verdicts, and AC-041 queue durability | Exact commands/results recorded at `task-008-closeout.md:1502-1508` | **PASS** |
| Report shape: step rows, then per-operation rows with the SLI columns; profiling fails on missing evidence | `report.rs:417-525`; `profile.rs:55-160`; `Report::passed` rejects recorded profile failures | Report/unit evidence recorded; shortened smoke rendered successfully | **PASS** |
| Non-goal: no production behavior change for the revision 57 consolidation | Diff is benchmark/test/harness wiring plus a documentation-only queue-default comment; no server or queue behavior is changed | Cumulative diff inspection | **PASS** |

## Proposed findings

### BHV-001 — VIOLATION: the repository still exposes another server capacity benchmark

- **Violated obligation:** REQ-171 requires `mise run bench:capacity` to be Wyrd's one capacity benchmark and states that no other server capacity benchmark exists; revision 57 says `bench:bifrost:query-capacity` is already deleted.
- **Exact location:** `mise.toml:507-509`; `crates/wyrd/wyrd-testing/Cargo.toml:145-151`; contrary task claim at `changes/active/verified-change-contract/tasks/task-008-closeout.md:1565-1569`.
- **Evidence:** `bench:bifrost:query-capacity` remains an operator-visible mise task. It builds a release `wyrd-server`, runs the `bifrost_query_capacity` binary in an 8-CPU/16-GiB scope, drives writes and query load, and fails on capacity rows. The binary remains registered. This is a reachable server capacity benchmark, not merely historical text or an external comparison artifact.
- **Observable consequence:** operators still have two authoritative-looking server capacity commands, directly defeating the revision 57 consolidation and its single readable capacity verdict.
- **Required testable correction:** remove the stale server-capacity mise task, binary registration, and owned binary source if it has no non-capacity consumer; then prove repository search exposes only `bench:capacity` as a server capacity benchmark while preserving any separately named external storage-engine comparison benchmarks the approved history excludes.

### BHV-002 — MISSING: a run can report PASS after exceeding the required 30-minute ceiling

- **Violated obligation:** REQ-171 says the default run MUST complete within 30 minutes including setup.
- **Exact location:** `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:114-176,257-279`; `crates/wyrd/wyrd-testing/src/bin/capacity/report.rs:386-397`.
- **Evidence:** the implementation records `setup_seconds` and `total_seconds`, but `Report::passed` considers only the three verdict steps and profile failures. There is no total-runtime condition or overall deadline. Therefore a default run whose steps pass after 1,801 seconds returns success.
- **Observable consequence:** automation and operators can accept a benchmark that violates the explicit operational bound, and the report presents the over-budget duration only as unjudged prose.
- **Required testable correction:** make the default-run result fail when end-to-end elapsed time exceeds 30 minutes, without changing the fixed step SLO table, and add a focused report/benchmark test proving an otherwise-passing report is rejected above the ceiling and accepted at or below it. Preserve custom short smoke arguments as diagnostic runs rather than misclassifying them as the default contract.

### BHV-003 — INCORRECT: the required `L=50` knee cannot satisfy AC-040's sample floor

- **Violated obligation:** REQ-171 fixes a possible knee of `K=50`, fixes the one-replica sustained duration at 180 seconds, and fixes direct traffic at `L/2` equally across five kinds; AC-040 requires at least 1,000 direct samples per non-judge kind in that step.
- **Exact location:** `crates/wyrd/wyrd-testing/src/bin/capacity/load.rs:449-483`; `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:219-226`; `crates/wyrd/wyrd-testing/src/bin/capacity/report.rs:184-210`; acknowledged at `changes/active/verified-change-contract/tasks/task-008-closeout.md:1561-1563`.
- **Evidence:** at `L=50`, direct rate is `25/s`; five equal kinds receive `5/s` in total; `5 × 180 = 900`. The implementation correctly enforces the 1,000 floor, so a deployment whose highest passing ramp step is 50 is forced to fail its required sustained verdict even if every measured request meets all performance SLOs.
- **Observable consequence:** the benchmark cannot produce a passing verdict for the lowest valid knee and thus does not implement the approved knee-to-verdict behavior over its full specified input domain.
- **Required testable correction:** this is an incompatibility among approved fixed rates, duration, knee levels, and sample floor, so do not silently oversample or extend a step. Obtain an approved specification resolution choosing which material quantity changes, then implement it and add a deterministic count test for every allowed knee, especially `K=50`.

### BHV-004 — MISSING: AC-040, AC-041, and the default-duration claim have no representative benchmark proof

- **Violated obligation:** acceptance requires credible evidence that the resulting repository satisfies the empirical AC-040/AC-041 SLOs and REQ-171's default-run duration.
- **Exact location:** `changes/active/verified-change-contract/tasks/task-008-closeout.md:1497-1501,1544-1563`.
- **Evidence:** the only revision 57 execution is explicitly a reduced smoke run at `L=20`, with 5/10/15-second windows. It exits 1, produces only 32 overhead samples per kind, and never exercises AC-041's specified `L=200` / 500 observations per second. Unit tests prove report arithmetic and workload construction; they cannot prove release-server throughput, overhead, queue drain, backlog drain, or the 30-minute end-to-end bound.
- **Observable consequence:** the task marks AC-040 and AC-041 PASS without evidence at the specified rate/duration, so capacity regressions or a benchmark that cannot finish its default contract remain acceptance-unknown.
- **Required testable correction:** after resolving BHV-003, run the unmodified default `mise run bench:capacity` in the required release peer/Postgres/RustFS/8-CPU/16-GiB envelope and record the complete report and duration. The evidence must show the required `L=200` ingest cell when that step is at or below the measured knee and all AC-040 samples/SLOs; a failing default run requires root-cause correction or an approved spec decision, not a shorter substitute.

## Verification assessment

The task records clean formatting, workspace lints, targeted Clippy, six capacity binary unit tests, exact Postgres/runtime tests, and the AC-041 real-server durability/memory journey. I reviewed those commands and their mapped source. I did not rerun Cargo-backed commands in this shared checkout. The recorded smoke run is useful plumbing evidence but is explicitly not the default benchmark and cannot close empirical capacity acceptance.

## Overall result

**FAIL**

The workload, step orchestration, SLO calculation, moved correctness tests, and report structure largely follow revision 57. Acceptance is nevertheless blocked by one retained server capacity benchmark, an unenforced 30-minute requirement, an approved-contract arithmetic conflict at `K=50`, and absent representative default-run evidence for AC-040/AC-041.
