# Capacity and Performance-Evidence Domain Review

## Reviewed boundary

- **Immutable subject:** base `f6159606c5c959e8fcc3423574ab0e7e6c86ee13`, candidate `8022436387f3a9a9499527ebf8b8b8140c6559cb`. The candidate still matched `HEAD` after inspection.
- **Approved authority:** `changes/active/verified-change-contract/spec.md`, approved revision 57, especially REQ-171, AC-040, AC-041, and the revision-57 history entry.
- **Task and prior evidence:** `changes/active/verified-change-contract/tasks/task-008-closeout.md`; r1-r3 capacity reports, verdicts, and validated ledgers; and `review/TASK-008-r3/TASK-008-CLOSEOUT-R2-command-lifetime.md` with its implementation evidence.
- **Runtime path traced:** `mise.toml` command entry and deadline propagation; the `capacity` Cargo target; `capacity/main.rs` command lifetime and benchmark lifecycle; `fixture.rs` reference workloads; `load.rs` open-loop operation mix; `step.rs` measurement, drain, and resource windows; `evidence.rs` exporter/SQL evidence; `judge.rs`; `profile.rs`; `report.rs` SLO cells and verdict; and `release_server.rs` migration, setup, replica, cgroup, and cleanup ownership.
- **Explicit sequencing constraint:** `FIND-TASK-008-CLOSEOUT-13`, the unmodified full default benchmark, is deferred to integration after the other workstreams merge. It is recorded below as a verification limit, not a finding or candidate blocker.

## Authority and source coverage

| Concern | Governing authority | Source and evidence inspected | Result |
|---|---|---|---|
| One server-capacity entry point | REQ-171; revision-57 history | `mise.toml:507-549`; `crates/wyrd/wyrd-testing/Cargo.toml:148-150`; repository scan of non-historical task/target names | PASS |
| Four identical tenants and the fixed direct/queued/ingest/query mix | REQ-171 workload | `capacity/main.rs:65-79`; `capacity/load.rs:255-515`; recorded `mix_offers_the_required_rates` and query-window evidence | PASS structurally |
| Warmup, ramp to knee, one-replica sustained, two-replica sustained, and `2K` scale-out | REQ-171 steps | `capacity/main.rs:427-475`; `step.rs:95-123`; recorded verdict/sequence smoke evidence | PASS structurally |
| Traffic, errors, paired overhead, queue drain, backlog, latency, CPU, and memory meanings | REQ-171 SLOs; AC-040; AC-041; analytical reliability qualification rules | `capacity/load.rs:119-144,255-455`; `step.rs:125-485`; `evidence.rs:20-297`; `report.rs:32-369` | PASS structurally |
| Reference workload and sample-floor judgment | AC-040 | `capacity/fixture.rs`; `capacity/evidence.rs:51-85`; `capacity/report.rs:35-45,192-233`; recorded focused tests | PASS structurally |
| Default-queue ingest rate, refusal/drain judgment, and separate durability proof | AC-041; REQ-172-177; INV-019 | `capacity/load.rs:28-30,146-229,424-441,480-515`; `step.rs:423-485`; `report.rs:236-264`; recorded real-server sustained-ingest journey | PASS structurally; empirical default-run proof deferred |
| One shared report table and three required verdict steps | REQ-171 report/verdict | `capacity/report.rs:371-563`; recorded report/verdict unit evidence | PASS |
| Fixed release topology and per-replica resource envelope | REQ-171 environment | `capacity/main.rs:277-375,478-635`; `release_server.rs:112-485`; `mise.toml:507-549` | PASS structurally |
| One absolute 30-minute setup-to-exit command boundary | REQ-171; R2 AC-R2-1 and AC-R2-2 | `mise.toml:510-549`; `capacity/main.rs:81-275,379-425,581-613`; `release_server.rs:130-207,365-405,636-732`; R2 focused evidence | **FAIL — CAP-R4-001** |
| Cancellation and surviving-effect contract | R2 AC-R2-3; `AGENTS.md` async/documentation rules | `capacity/main.rs:206-265,313-391,427-491`; `release_server.rs:130-207,636-732` | PASS |
| No workload, SLO, production timeout, or public-surface drift | R2 AC-R2-4 | remediation delta and complete cumulative diff | PASS |

## Test Coverage Analysis

### Current Coverage

- The R2 implementation record reports `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity`: 14 passed and the process proof skipped by default. Those tests cover workload arithmetic, query windows, percentile and histogram calculations, Scribe staged backlog, the exact drain boundary, common resource intervals, SLO failures, error categories, report shape, verdict selection, judge-wait separation, and cooperative lifetime arithmetic.
- The R2 record reports the ignored process test `tests::a_stalled_tenant_setup_stops_the_run_by_its_deadline` passing in 10 seconds. It exercises an already-serving replica plus a non-terminating tenant-setup child through `Benchmark::run`, and proves failed-report creation, child/replica reaping, diagnostic retention, and prevention of later in-binary work.
- The R2 record reports two `release_server` unit tests and clean formatting/lints. Prior task evidence separately records real-server queued-claim/fairness, judgment, and sustained-ingestion durability coverage.
- Static inspection confirms that the prior r1 capacity defects are closed: the legacy capacity targets are absent, the 60-second drain edge is preserved, CPU and memory share one measured interval, provider wait is separate from engine overhead, one report table is used, and the verdict selects the three required steps.

### Gaps

- `[mise.toml:526-529]` The first two command-owned setup processes are guarded by `timeout --foreground` without `--kill-after` and without a surrounding hard deadline that can escalate. GNU `timeout` returns only after a TERM-ignoring command eventually exits when no kill escalation is configured. A bounded local probe (`timeout --foreground 0.1 sh -c 'trap "" TERM; sleep 1'`) returned 124 only after about 1,002 ms, ten times its nominal bound. A stuck RustFS `docker compose up` or `docker compose run` can therefore hold `mise run bench:capacity` beyond the absolute 30-minute deadline before the Postgres wrapper or `Benchmark` owner exists.
- `[mise.toml:510-549]` The outer task body does not enable fail-fast shell behavior. A nonzero early RustFS setup command can fall through to later setup, and its status can be replaced by the last command's status. This compounds the command-owner gap: setup failure is not guaranteed to stop later phases or remain the command result.
- `[R2 process proof]` The controlled stalled-child test begins inside `Benchmark::prepare`/`LocalServer::start`; the recorded shell shim only checked deadline propagation and budget refusal. Neither proof drives a TERM-ignoring process in the two pre-Postgres RustFS slots, so the reachable command-entry defect is not covered.

## Material proposed finding

### CAP-R4-001 — INCORRECT: the absolute command deadline cannot stop the earliest setup processes

- **Prior finding:** `FIND-TASK-008-CLOSEOUT-2` is only partially closed. The candidate fixes deadline origin and propagation, makes migration/setup children cooperatively cancellable, reserves cleanup/report time, and records pre-binary elapsed time, but it does not enforce the same boundary over the first two mandatory command-owned setup actions.
- **Violated obligation:** REQ-171 requires the default `mise run bench:capacity` command, including setup, to complete within 30 minutes. R2 AC-R2-1 requires every phase to consume one absolute remaining budget through command exit; AC-R2-2 requires a stalled operator child to be terminated and reaped, with no later work starting after expiry.
- **Exact location:** `mise.toml:510-549`, especially `mise.toml:526-529`.
- **Evidence:** `WYRD_CAPACITY_DEADLINE` is established before RustFS setup, but both RustFS commands use `timeout --foreground "$seconds"` without a kill escalation. `timeout` sends TERM at expiry and waits for a command that ignores TERM; the bounded probe above demonstrated that the nominal timeout is not a hard ceiling. These commands run before the Postgres wrapper's separate `--kill-after=5` bound and before the Rust `Lifetime`/`Benchmark` lifecycle exists. The task body also lacks `set -e`, so an early nonzero setup action does not itself prevent later actions.
- **Observable consequence:** a wedged or TERM-insensitive Docker client can keep the operator command alive after the approved 30-minute ceiling, with no failed benchmark report because no report owner was initialized. An early RustFS failure can also permit later setup to start and can be masked by a later command's status. The benchmark therefore cannot yet serve as trustworthy bounded capacity evidence.
- **Required testable correction:** complete the existing command-level owner rather than adding another benchmark timeout abstraction. Make every pre-`Benchmark` setup process obey the same absolute deadline with bounded escalation and make the task fail immediately on a failed setup phase, while preserving Ctrl-C behavior, the approved workload/SLOs, the Postgres wrapper teardown reserve, and the in-binary cleanup/report reserves. Add a shortened command-entry proof whose first or second RustFS stand-in ignores TERM; prove bounded nonzero exit, escalation/reaping, no later setup/build/measurement start, and retained diagnostics. Re-run the existing stalled-tenant-setup proof to preserve the already-correct in-binary path.

## Recommended Verification

- `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity --run-ignored=only -E 'test(=tests::a_stalled_tenant_setup_stops_the_run_by_its_deadline)'` — preserve the fixed migration/setup and serving-replica lifecycle proof.
- `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity` — preserve workload, evidence, report, verdict, and lifetime unit coverage.
- A focused shortened command-entry test for a TERM-ignoring first/second setup process — directly close CAP-R4-001; a shell shim that merely observes propagated variables is insufficient.
- `mise run fmt`, `mise run lints`, and `git diff --check` — required Rust/task-file verification after the correction.
- The unmodified `mise run bench:capacity` remains the separately sequenced integration qualification for `FIND-TASK-008-CLOSEOUT-13`, not remediation proof for CAP-R4-001.

## Verification limits and residual risk

- Per orchestrator direction, I did not start Cargo, mise, or the opt-in benchmark while shared review work was active. Current runtime results above are the recorded R2 evidence; this pass adds source inspection and the bounded native `timeout` semantics probe only.
- The full default benchmark remains explicitly deferred. No empirical AC-040, AC-041, one-to-two-replica scale-out, or real 30-minute wall-time claim is made from source shape, unit tests, or the reduced smoke.
- Until CAP-R4-001 is corrected, even a later passing capacity report would not prove that the command boundary is universally bounded, because the uncovered failure occurs before report ownership.

## Overall result

**FAIL**

The cumulative candidate faithfully implements the approved workload, step sequence, SLO/report/verdict meanings, evidence limits, and benchmark consolidation, and it closes the in-binary stalled migration/setup path. The earliest mandatory setup commands remain outside a hard, fail-fast absolute command owner, so `FIND-TASK-008-CLOSEOUT-2` remains reachable. `FIND-TASK-008-CLOSEOUT-13` is separately deferred by explicit caller sequencing and does not contribute to this result.
