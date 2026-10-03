# TASK-008 invariant review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `ce5c09ef3b559c35e65d6a3e73beebe0a69ae5bd`
- Candidate: `852894689388124960993014a46934e73c0ed2a8`
- Cumulative range: `ce5c09ef3..852894689`
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 57, specifically REQ-171, AC-040, AC-041, and the revision 57 history entry
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`, section `bench:capacity implementation (revision 57)`
- Pre-approved harness work in the cumulative range: `f6159606c`

The candidate stayed at `852894689388124960993014a46934e73c0ed2a8` during this review. `.codegraph/` is absent, so source navigation used the repository files and `rg`.

## Navigation and invariant trace

The executable entry point is `mise.toml`'s `bench:capacity`, which builds a release cloud server and runs `wyrd-testing`'s `capacity` binary. `main.rs::benchmark` creates four tenants, starts one release replica, provisions the AC-040 fixtures, and drives warmup, ramp, one-replica sustained, two-replica sustained, and two-replica scale-out records through `Deployment::run`. `load.rs::mix` produces equal per-tenant direct, queued, ingest, and query lanes. Each lane produces a `Tally`; `step.rs::Deployment::ops` combines those client values with durable `verifier_runs`; `evidence.rs` derives histogram deltas and polls the run, Scribe, audit, and Forge backlogs; `report.rs` converts those values into step and operation cells and the final verdict.

The relevant sibling consumers are the still-registered `bifrost_query_capacity` binary/task, the default `QueueConfig` documented against the new benchmark, the release-server harness used by both benchmark families, and the real-server correctness journeys added to `pg_verification_runtime.rs`, `drift_verification.rs`, and `observe_run.rs`.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-171 exposes `mise run bench:capacity` and retires the replaced verification and ingest capacity commands/binaries | `mise.toml:511-528`; `crates/wyrd/wyrd-testing/Cargo.toml:153-159`; old verification and ingest binary trees deleted | Candidate name search; capacity unit binary compiles and runs | PASS |
| REQ-171 makes `bench:capacity` Wyrd's one server capacity benchmark, with no other server capacity benchmark | `mise.toml:507-509` still registers `bench:bifrost:query-capacity`; `Cargo.toml:145-150` still registers `bifrost_query_capacity`; its `main.rs:1-14` identifies it as a release-server capacity benchmark | Direct source inspection | **FAIL (INV-001)** |
| REQ-171 workload is spread evenly over four identical tenants: direct and queued `L/2` over five equal kinds, ingest `2.5L` observations of 100 features through default-queue `WyrdState`, and Oracle `L/2` split across the two five-minute shapes | `main.rs:56-57`; `load.rs:146-187, 345-447, 449-483`; `fixture.rs:202-294` | `mix_offers_the_required_rates`; `queries_read_the_last_five_minutes` | PASS |
| REQ-171 step topology is warmup 30 s at 50, ramp 60 s at 50/100/200/400 through first miss, sustained 180 s at K on one and two replicas, then 180 s at 2K on two | `main.rs:77-89, 188-245`; `step.rs:136-223` | Source trace; shortened smoke run recorded by the task, not a default-duration proof | PASS |
| REQ-171 default run completes within 30 minutes including setup | `main.rs:113-279` records elapsed time but applies no overall deadline; provisioning is sequential and independently permits up to 300 s for each fit and seed wait (`fixture.rs:57-58, 282-292, 356-423`); operator setup commands are also outside a benchmark-wide deadline | Only a reduced-duration smoke run is recorded; no default run or deadline-path test | **FAIL (INV-003)** |
| Traffic is achieved/offered per operation at least 95%; zero refusals, losses, duplicate runs, wrong direct judgments, or failed runs | `load.rs:119-143, 239-307`; `step.rs:264-330`; `report.rs:129-166, 240-315` | `every_slo_failure_fails_the_step` plus source trace of producer-to-report accounting | PASS |
| AC-040 reference workloads are exact: four assertions/2 KiB context; Custom 1 metric/1,000 samples; PSI 8×1,000/10 bins/10,000 baseline; SPC 4×1,000/subgroup 5/10,000 baseline; judge plus assertion against a 200 ms local TLS mock | `fixture.rs:39-58, 109-170, 447-617`; `judge.rs:27-109` | Source inspection; no full default benchmark execution | PASS |
| AC-040 four non-judge direct kinds have paired overhead p95 below 10 ms in every judged step, with at least 1,000 samples each in the one-replica sustained step | `evidence.rs:62-86`; `report.rs:33-43, 184-211`; `main.rs:219-227` | `quantile_reads_bucket_deltas`; `every_slo_failure_fails_the_step` | PASS |
| AC-040 judge cases report paired overhead and provider waits separately, without a threshold | `evidence.rs:71-86` collects only `wyrd_verification_engine_overhead_seconds`; `report.rs:184-211, 445-448` prints only that overhead (plus a call count elsewhere), and never collects or renders provider-wait evidence | Candidate-wide search under `bin/capacity`; capacity unit tests contain no provider-wait assertion | **FAIL (INV-002)** |
| AC-041 carries 500 observations/s × 100 rows at L=200 with default queue configuration; steps at/below K judge zero `QUEUE_FULL` and queue drain no more than one second | `load.rs:46-49, 393-410, 449-483`; `TenantClients::connect` uses `QueueConfig::default`; `load.rs:302-305`; `report.rs:39-40, 150-166, 213-219` | Mix and SLO unit tests; reduced smoke only, not L=200/default duration | PASS with runtime verification limit |
| AC-041 tests prove real-server exactly-once durability with 100 rows per record and flat client-owned bytes under sustained emission | `sdks/wyrd-sdk-rust/tests/observe_run.rs:1033-1155` | Task records the exact ignored journey command and a passing result; not rerun in this invariant pass because it owns Postgres lifecycle | PASS |
| Correctness/isolation moved out of the benchmark: queued cross-replica exactly-once, tenant fairness, and judgment correctness stay test-owned | `pg_verification_runtime.rs:1267-1405`; `drift_verification.rs` direct LLM-judge failure addition; benchmark report does not judge those properties | Task records exact focused commands and passing results | PASS |
| Verdict requires one-replica sustained and both two-replica steps; missing steps fail | `report.rs:370-397`; `main.rs:207-245` | `verdict_needs_every_verdict_step` | PASS |
| Report contains step rows followed by per-operation rows, with client latency and replica CPU/memory reported | `report.rs:417-524`; `step.rs:176-220` | Render path compiled by capacity unit binary; smoke report recorded by task | PASS |
| Every server-owned backlog is polled from load stop and must drain in 60 s | `evidence.rs:155-247`; `step.rs:187-207, 225-262`; report backlog cell at `report.rs:221-237` | SLO unit test covers a non-draining result; full runtime path only smoke-tested | PASS with runtime verification limit |
| `--profile` produces per-step/per-replica symbolized evidence and fails on missing or unusable captures | `main.rs:90-97, 117-126`; `step.rs:332-355`; `profile.rs:55-171`; report includes profile failures in verdict | Source inspection; no profile run recorded | PASS with runtime verification limit |
| Revision 57 non-goal: do not judge tenant fairness, claim uniqueness, or judgment correctness in the benchmark itself | Those checks are absent from `capacity/report.rs` and live in the cited tests | Source inspection | PASS |

## Proposed findings

### INV-001 — VIOLATION — a second release-server capacity benchmark remains reachable

- **Violated obligation:** REQ-171 makes `mise run bench:capacity` Wyrd's one capacity benchmark and says no other server capacity benchmark exists; revision 57 says the old Bifrost query capacity benchmark is already deleted.
- **Location:** `mise.toml:507-509`; `crates/wyrd/wyrd-testing/Cargo.toml:145-150`; `crates/wyrd/wyrd-testing/src/bin/bifrost_query_capacity/main.rs:1-14`.
- **Evidence:** the candidate still exposes `mise run bench:bifrost:query-capacity`, builds the `bifrost_query_capacity` binary, starts release `wyrd-server`, applies capacity thresholds, and exits on failed rows. This is not merely ClickBench or an external engine-comparison benchmark.
- **Observable consequence:** operators have two authoritative server-capacity commands and two incompatible workload/report contracts, directly defeating the approved single-benchmark outcome.
- **Required testable correction:** remove the legacy Bifrost query capacity task, binary registration, and owned source tree while preserving non-capacity storage comparison benchmarks; prove repository-wide that `bench:capacity`/the `capacity` binary are the only server capacity entry points.

### INV-002 — MISSING — AC-040 judge provider waits are not reported

- **Violated obligation:** AC-040 requires judge cases to report overhead and provider waits separately, with no provider-wait threshold.
- **Location:** `crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:62-86`; `crates/wyrd/wyrd-testing/src/bin/capacity/report.rs:184-211,445-448`.
- **Evidence:** `Scrapes::overhead` reads only `wyrd_verification_engine_overhead_seconds`; the report prints that family for the judge alongside the other kinds. No capacity evidence value or report column/field represents the mock provider wait. `judge_calls` is a count, not a duration.
- **Observable consequence:** the report cannot distinguish time spent waiting on the fixed-delay provider from Wyrd's local judge overhead, so the required diagnostic half of the judge workload is absent.
- **Required testable correction:** use the execution telemetry owner or another already-owned, paired measurement to capture judge provider-wait duration separately from engine overhead, render both as reported-only evidence, and add a focused report/evidence test that would fail when the provider-wait value is missing.

### INV-003 — INCORRECT — the default run has no 30-minute completion bound

- **Violated obligation:** REQ-171 requires a default run, including setup, to complete within 30 minutes.
- **Location:** `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:113-279`; setup waits at `crates/wyrd/wyrd-testing/src/bin/capacity/fixture.rs:57-58,282-292,356-423`; unbounded operator subcommands enter through `crates/wyrd/wyrd-testing/src/release_server.rs:146-188,615-625`.
- **Evidence:** `benchmark` records `started.elapsed()` but never turns 30 minutes into a deadline or cancellation boundary. Four tenants provision sequentially; each tenant can independently spend up to 300 seconds on each of two sequential baseline-fit waits and another 300 seconds seeding, before the fixed step windows and their 60-second drains. Migration/setup subprocesses have no timeout at all. Therefore elapsed time can exceed 30 minutes without the run completing.
- **Observable consequence:** a slow fit, storage dependency, or stuck setup command can make the supposedly bounded operator benchmark run past its approved wall-clock budget or hang indefinitely.
- **Required testable correction:** make one 30-minute wall-clock budget own setup, measured steps, evidence drain, and teardown; when exhausted, stop further work and exit explicitly as a failed benchmark while preserving diagnostic evidence and process cleanup. Add a focused controllable-time proof of deadline expiry and record one full default run completing inside the bound.

## Verification notes

- `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity`: 6 passed, 0 failed.
- `git diff --check ce5c09ef3..852894689`: clean.
- The task records clean formatting, `wyrd-testing` and Rust SDK clippy, workspace lints, the focused Postgres-backed correctness journeys, and a shortened smoke benchmark. Those results were considered as available evidence.
- No full default-duration `bench:capacity` run, L=200 AC-041 report, or `--profile` run is available. The shortened smoke run cannot prove the 30-minute default-run requirement, AC-041 at its reference rate, or profile evidence behavior.

## Overall result

**FAIL**

The workload, step, traffic/error, overhead-SLO, backlog, and verdict value flows are largely coherent, but the cumulative candidate violates the one-benchmark invariant, omits required judge provider-wait reporting, and does not enforce the approved 30-minute completion bound.
