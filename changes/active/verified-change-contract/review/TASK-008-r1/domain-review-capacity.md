# Capacity and Load-Methodology Domain Review

## Reviewed boundary

- **Immutable subject:** base `ce5c09ef3b559c35e65d6a3e73beebe0a69ae5bd`, candidate `852894689388124960993014a46934e73c0ed2a8`; candidate was still `HEAD` when this report was written. Commit `f6159606c` was treated as pre-approved harness state as directed.
- **Approved authority:** `changes/active/verified-change-contract/spec.md` revision 57, specifically REQ-171, AC-040, AC-041, and the revision 57 history entry.
- **Original task/evidence:** `changes/active/verified-change-contract/tasks/task-008-closeout.md`, including the revision 57 implementation record and its reduced smoke-run evidence.
- **Runtime path traced:** `main.rs` default plan and knee selection → `load.rs` open-loop lanes and public-client requests → `step.rs` step boundaries, reconciliation, resource sampling, and backlog drain → `evidence.rs` Prometheus/SQL evidence → `report.rs` SLO cells and final verdict. I also inspected `profile.rs`, the release-server cgroup owner, `mise.toml`, the benchmark targets in `wyrd-testing/Cargo.toml`, and the remaining query-capacity binary entry point.

## Authority and source coverage

| Concern | Authority | Source/evidence inspected | Result |
|---|---|---|---|
| One production-shaped open-loop mix over four identical tenants | REQ-171 workload | `capacity/load.rs:239-484`, `capacity/main.rs:56-57,160-245` | PASS |
| Warmup, ramp, knee, sustained, and scale-out sequence | REQ-171 steps | `capacity/main.rs:188-245` | PASS |
| Traffic, error, overhead, ingest-drain, backlog, latency, CPU, and memory evidence | REQ-171 SLOs; AC-040; AC-041 | `capacity/step.rs`, `capacity/evidence.rs`, `capacity/report.rs` | FAIL: CAP-001, CAP-004 |
| One-table diagnostic report and verdict semantics | REQ-171 report/verdict | `capacity/report.rs:240-537` | FAIL: CAP-002 |
| One Wyrd server-capacity benchmark | REQ-171; revision 57 history | `mise.toml:507-527`, `wyrd-testing/Cargo.toml:145-159`, `bifrost_query_capacity/main.rs` | FAIL: CAP-003 |
| Default-run performance and 30-minute qualification | REQ-171; AC-040; AC-041; analytical reliability qualification rule | `task-008-closeout.md:1493-1501,1535-1563` | FAIL: CAP-005 |
| Per-step, per-replica symbolized profiles | REQ-171 `--profile` | `capacity/main.rs:117-120,181-185,257-279`; `capacity/profile.rs` | PASS |

## Material findings

### CAP-001 — INCORRECT: a backlog observed empty after the 60-second deadline is reported as passing

- **Violated obligation:** REQ-171 requires every backlog after load stops to drain within 60 seconds; a step passes only when that SLO holds.
- **Location:** `crates/wyrd/wyrd-testing/src/bin/capacity/step.rs:173-195,244-259`; `crates/wyrd/wyrd-testing/src/bin/capacity/report.rs:221-237`.
- **Evidence:** `Deployment::run` waits for every lane, including outstanding request tasks and client flushes, before it first calls `drain`, although the drain clock starts at the nominal `start + window`. In `drain`, the first observation of an empty backlog returns `Some(elapsed)` before checking whether `elapsed >= DRAIN_LIMIT`. `report::backlog` treats every `Some(seconds)` as `PASS`, without comparing it with 60 seconds. Therefore an empty first poll at, for example, 61 seconds produces `PASS 61.0 s`.
- **Observable consequence:** a ramp or verdict step can claim that the backlog-drain SLO passed even though the only evidence proves that emptiness was observed after its deadline. This can also select a false knee and permit a false final PASS when the other SLOs hold.
- **Required testable correction:** make the drain result distinguish “observed empty within the deadline” from “empty only when first observed after the deadline,” and have report judgment enforce `<= DRAIN_LIMIT`. Add a focused boundary test covering a drained duration above 60 seconds (and the at/below-boundary passing case).

### CAP-002 — INCORRECT: the Markdown report emits two differently shaped tables instead of the required single SLI table

- **Violated obligation:** REQ-171 requires one table containing the step rows followed by one per-operation row per step, with the same SLI columns; every SLI cell is marked PASS or FAIL.
- **Location:** `crates/wyrd/wyrd-testing/src/bin/capacity/report.rs:449-477`.
- **Evidence:** `Report::render` closes a `## Steps` table at line 462, then creates a separate `## Per operation` table at lines 463-477. The second table changes `backlogs drained` to `backlog left` and `CPU / memory` to `driver`; `op_row` also renders non-applicable cells as unmarked `-` and backlog cells as reported-only text (`report.rs:318-348`).
- **Observable consequence:** the human-readable artifact does not provide the revision 57 comparison surface: an operator cannot scan one consistent set of SLI columns from the aggregate step into its operation breakdown, and operation cells do not carry the required PASS/FAIL status.
- **Required testable correction:** render the aggregate and operation rows in one table with one shared column schema, using an explicit non-applicable representation only where an SLI genuinely has no operation-level value, and add a rendering test that proves there is one header and that each step's operation rows follow it with the same number and meaning of columns.

### CAP-003 — VIOLATION: the old Bifrost query server-capacity benchmark remains runnable

- **Violated obligation:** REQ-171 says `bench:capacity` is Wyrd's one capacity benchmark and that no other server-capacity benchmark exists; the revision 57 history says the former Bifrost query-capacity entry point is already deleted and only its external engine-comparison benchmarks remain.
- **Location:** `mise.toml:507-509`; `crates/wyrd/wyrd-testing/Cargo.toml:145-151`; `crates/wyrd/wyrd-testing/src/bin/bifrost_query_capacity/main.rs:1-12`.
- **Evidence:** the candidate still registers and documents `mise run bench:bifrost:query-capacity`, still registers the `bifrost_query_capacity` binary, and retains its server-driving implementation. The task record itself acknowledges that it was intentionally not touched (`task-008-closeout.md:1565-1569`), which conflicts with the approved revision 57 authority.
- **Observable consequence:** operators still have two runnable server-capacity commands with different workload and verdict models, preserving the ambiguity revision 57 was approved to remove.
- **Required testable correction:** retire the `bench:bifrost:query-capacity` task, its Cargo binary target, and the server-capacity binary sources while preserving separately named ClickBench/observability engine-comparison tooling. Prove by repository scan that `bench:capacity` is the only server-capacity entry point.

### CAP-004 — INCORRECT: CPU and peak-memory values do not share the declared step measurement window

- **Violated obligation:** REQ-171 requires per-step replica CPU cores and peak memory to be reported as the saturation evidence; `Resources` explicitly defines both as use “over the arrival window.”
- **Location:** `crates/wyrd/wyrd-testing/src/bin/capacity/step.rs:89-98,143-205`.
- **Evidence:** CPU begins before the scheduled start and is read only after all lanes have awaited their outstanding requests and flushed the ingest clients, but is divided by the planned arrival-window duration. Peak memory is reset at the same beginning and read still later, after the server backlog drain completes. Thus CPU includes a variable request/client-drain tail with no corresponding time in its denominator, while memory includes up to another 60 seconds of server drain that CPU excludes.
- **Observable consequence:** the reported CPU value can exceed the actual average cores used during the arrival window, and CPU and memory describe different intervals. The saturation diagnosis for the knee and scale-out rows is therefore not comparable or reliably attributable to the offered load.
- **Required testable correction:** sample both resource signals over one explicitly defined step interval, and divide CPU delta by that interval's measured duration. Add a focused test around the resource-window calculation; retain the cgroup-enforced 8-CPU/16-GiB envelope.

### CAP-005 — MISSING: no authoritative default run proves the performance acceptance criteria or 30-minute ceiling

- **Violated obligation:** REQ-171 requires the default benchmark to finish within 30 minutes; AC-040 and AC-041 require measured default-run overhead, sample-count, 50,000-row/s, refusal, and drain evidence. The analytical reliability authority states that performance claims require fixed workload, environment, concurrency, dataset, query mix, and promoted measurements; configuration is not performance evidence.
- **Location:** `changes/active/verified-change-contract/tasks/task-008-closeout.md:1493-1501,1535-1563`.
- **Evidence:** the recorded run is explicitly a reduced smoke with `--levels 20`, 5/10/15-second windows, not the default run. It exits 1, records only 32 AC-040 samples per kind, offers neither `L = 200` nor 500 observations/s, and therefore proves neither AC-040 nor AC-041. Nevertheless the acceptance table marks AC-040 and AC-041 PASS using that smoke report. No recorded default-run wall time or passing default report exists for this candidate.
- **Observable consequence:** there is no credible evidence that the candidate completes inside the required budget, finds a valid knee, supplies 1,000 samples per non-judge kind, carries 50,000 ingest rows/s, or passes the three verdict steps on the prescribed environment.
- **Required testable correction:** after the bounded implementation defects are corrected, run unmodified `mise run bench:capacity` in the prescribed release/Postgres/RustFS/cgroup environment and preserve the report and wall time as task evidence. The report must exercise the default levels and durations, reach `L = 200`, and directly show the AC-040/AC-041 and final-verdict cells; a shortened smoke remains useful only as harness evidence.

## Verification limits

- `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity` passed all 6 unit tests on the immutable candidate.
- Those tests cover mix totals, query windows, raw and histogram percentile calculations, representative SLO failures, and missing verdict steps. They do not cover a backlog first observed empty after 60 seconds, report-table structure, resource-window scope, legacy benchmark retirement, or default-run performance.
- I did not rerun the opt-in capacity workload. The task record supplies only the reduced, intentionally failing smoke run described in CAP-005; no full default report was available to validate.
- AC-041's durable exactly-once and flat-client-byte journey is recorded separately in the task. This review did not re-run it because the domain gap is the missing prescribed benchmark measurement, not that journey's queue-property proof.

## Overall result

**FAIL**

The load mix, default step sequence, metric-delta math, raw-percentile math, knee control flow, and profile capture are present, but the candidate can pass a late backlog drain, does not emit the required report shape, retains a second server-capacity benchmark, reports mismatched resource windows, and lacks the default-run evidence needed to accept the performance contract.
