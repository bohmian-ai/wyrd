# Capacity and Performance-Evidence Domain Review

## Reviewed boundary

- **Immutable subject:** base `7d96c30066425e0cde2290842d5801307843283d`, candidate `5c3bb79b3598abd88a3a234611fc400096adc975`. Both identities matched the requested subject while this report was written.
- **Requested authority:** `changes/active/verified-change-contract/spec.md`, approved revision 58.
- **Authority actually present in the candidate:** the same file declares approved revision 57 (`spec.md:1-5`), the task declares `spec_revision: 57` (`task-008-closeout.md:1-18`), and repository search finds no revision-58 entry for this change. The requested approved authority therefore cannot be obtained from the immutable subject.
- **Provisional authority used only to avoid losing source evidence:** revision 57 REQ-171, AC-040, AC-041, the revision-57 history entry, the original task, the prior r1 verdict, and `TASK-008-CLOSEOUT-R1-capacity-closure.md`. This provisional comparison is not a substitute for revision 58.
- **Runtime path traced:** `mise.toml` and the `wyrd-testing` binary registration; `capacity/main.rs` lifecycle and step sequence; `load.rs` open-loop mix and accounting; `fixture.rs` AC-040 fixtures; `step.rs` timing, resource, reconciliation, and backlog boundaries; `evidence.rs` metrics and SQL evidence; `judge.rs` provider-wait evidence; `report.rs` SLO cells and verdict; `profile.rs`; and the release-server process owner.
- **Consumers traced:** operators invoking `mise run bench:capacity`, the Markdown/JSON report readers that judge REQ-171, AC-040, and AC-041, and the focused correctness/durability journeys retained outside the benchmark.

## Authority and source coverage

| Concern | Governing authority | Source/evidence inspected | Provisional revision-57 result |
|---|---|---|---|
| Fixed representative workload and envelope | REQ-171; AC-040; AC-041; `analytical-operations-reliability.md` performance-evidence rule | `capacity/load.rs`, `fixture.rs`, `main.rs`, `release_server.rs`, `mise.toml` | PASS structurally |
| Warmup, ramp, knee, sustained, and scale-out sequence | REQ-171 | `capacity/main.rs:346-394`, `step.rs:303-381` | PASS structurally |
| Traffic, error, overhead, provider-wait, ingest-drain, backlog, CPU, and memory evidence | REQ-171; AC-040; AC-041; Bifrost measurement meanings | `capacity/load.rs`, `step.rs`, `evidence.rs`, `judge.rs`, `report.rs`; r1 remediation closure tests | PASS structurally |
| Durable Scribe/audit/Forge backlog fidelity | REQ-171; `bifrost-design.md` Scribe staging and audit publication authority | `evidence.rs:94-102,223-297`, `step.rs:383-420`; production metric definitions | PASS structurally |
| One report table and three-step verdict | REQ-171 | `report.rs:394-563` | PASS structurally |
| One server-capacity entry point | REQ-171; revision-57 history | `mise.toml:507-524`, `wyrd-testing/Cargo.toml:145-151`; repository scan | PASS structurally |
| Whole-command 30-minute ceiling | REQ-171; prior FIND-TASK-008-CLOSEOUT-2 remediation | `capacity/main.rs:79-188,227-344,500-532,621-637`; `release_server.rs:147-190,347-387,618-635`; lifetime unit test | FAIL: CAP-R2-001 |
| Authoritative default performance qualification | REQ-171; AC-040; AC-041; prior FIND-TASK-008-CLOSEOUT-13; analytical reliability qualification rule | task evidence at `task-008-closeout.md:1490-1568`; remediation evidence at `TASK-008-CLOSEOUT-R1-capacity-closure.md:293-344` | FAIL: CAP-R2-002 |

## Test Coverage Analysis

### Current Coverage

- `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity` passed all 14 tests in this review. These prove the mix arithmetic, query window, Scribe staged-member accounting, histogram/percentile math, drain boundary, common resource denominator, error categories, one-table rendering, verdict-step selection, and separate judge-wait rendering.
- The r1 remediation records green focused real-server proof for two-replica claim/fairness behavior, the AC-041 100-feature exactly-once/flat-byte journey, and direct/queued judgment journeys. Those are supporting correctness proofs, not performance qualification.
- `git diff --check` passed for both the cumulative base-to-candidate range and the remediation delta.
- The recorded shortened smoke exercises the release-process topology and report, but deliberately changes levels and durations, exits 1, supplies only 32 samples per kind, and never reaches `L = 200`.

### Gaps

- `[capacity/main.rs:159-187,240-295,623-628; release_server.rs:147-190,624-635]` The deadline test covers a cooperatively pending async future, not the real setup path's blocking `Command::output()` calls. A stalled `migrate` or tenant `setup` child blocks inside a poll and cannot be preempted by `tokio::time::timeout_at`, so the claimed whole-command bound is unproven and not enforced. Add a process-level reproduction with a deliberately non-terminating setup child and prove the command terminates, cleans up, reports failure, and stays inside its absolute deadline.
- `[task-008-closeout.md:1498-1568; TASK-008-CLOSEOUT-R1-capacity-closure.md:293-344]` No unmodified default execution proves the actual release deployment. Run exactly `mise run bench:capacity` after source closure and retain the complete Markdown/JSON identity and wall time; it must reach `L = 200`, satisfy the AC-040 sample/latency evidence, expose AC-041 refusal/drain evidence, pass all three verdict steps, and finish within 30 minutes.
- `[spec.md:1-5; task-008-closeout.md:1-18]` Revision 58 is unavailable, so this reviewer cannot determine whether its requirements changed the workload, evidence, or acceptance boundary. Supply the approved immutable revision-58 specification before treating any revision-57 matrix as acceptance evidence.

## Verification limits

- I did not run the unmodified default benchmark. It is the missing acceptance artifact identified by prior FIND-TASK-008-CLOSEOUT-13, not a unit-test substitute.
- I did not treat configuration, source shape, the reduced smoke, or green unit/integration tests as empirical capacity evidence. Repository authority explicitly says performance claims require a fixed workload, environment, concurrency, dataset, query mix, and promoted measurements.
- The immutable candidate contains no approved revision 58 to review. Findings below are source-valid against the available revision-57 authority, but the domain result must remain blocked until the requested authority is supplied.

## Material proposed findings

### CAP-R2-001 — MISSING: the absolute lifetime cannot preempt blocking setup subprocesses

- **Violated obligation:** REQ-171 and prior FIND-TASK-008-CLOSEOUT-2 require one absolute 30-minute boundary from setup through cleanup, with expiry stopping work and retaining diagnostics.
- **Exact location:** `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:159-187,240-295,623-628`; `crates/wyrd/wyrd-testing/src/release_server.rs:147-190,618-635`.
- **Evidence:** `Lifetime` is created inside `Benchmark::prepare`, but `main` awaits `prepare` outside any timeout. More importantly, measured provisioning calls `LocalServer::start`, whose migration and per-tenant setup use synchronous `Command::output()`. A Tokio timeout cannot cancel a future while its poll is blocked in that call. The focused lifetime test uses `std::future::pending`, which is cooperatively cancellable and does not exercise this reachable subprocess path.
- **Observable consequence:** an unavailable or wedged migration/setup dependency can keep the operator's default command alive beyond 30 minutes without producing the required failure report and bounded cleanup. The candidate therefore does not close prior FIND-TASK-008-CLOSEOUT-2 on the real path.
- **Required testable correction:** make setup subprocess ownership obey the existing absolute benchmark deadline and terminate/reap a still-running child on expiry, while preserving the current report and cleanup behavior. Add a process-level stalled-setup proof; do not add a new timeout knob or change production request deadlines.

### CAP-R2-002 — MISSING: prior FIND-TASK-008-CLOSEOUT-13 has no default-run closure evidence

- **Violated obligation:** REQ-171 requires the default production-shaped run to complete within 30 minutes and pass its verdict; AC-040 and AC-041 require evidence that exists only at the prescribed sustained load and sample count. The r1 remediation explicitly requires one unmodified default execution.
- **Exact location:** `changes/active/verified-change-contract/tasks/task-008-closeout.md:1498-1568`; `changes/active/verified-change-contract/review/TASK-008-r1/TASK-008-CLOSEOUT-R1-capacity-closure.md:293-344`.
- **Evidence:** the task retains only an intentionally failing `L = 20`, 5/10/15-second smoke. The remediation record explicitly marks R13 `NOT RUN (integrator)` and repeats the shortened failing smoke. No recorded artifact reaches `L = 200`, proves at least 1,000 samples per non-judge kind in the one-replica sustained step, passes the one-replica/two-replica/scale-out verdict steps, or records a passing wall time below 30 minutes.
- **Observable consequence:** operators have plausible benchmark code but no promoted measurement supporting the capacity claim. Source inspection and unit tests cannot establish that the integrated audit-outbox/Forge candidate sustains the required workload or scales from one to two replicas.
- **Required testable correction:** after closing CAP-R2-001, run the unmodified `mise run bench:capacity` in the prescribed release/Postgres/RustFS and per-replica cgroup envelope, and append the exact command, report identities, complete verdict evidence, and wall time to the task record. A failed run requires root-cause correction within approved behavior; another reduced smoke does not close the finding.

## Overall result

**BLOCKED**

The requested approved revision-58 specification is absent from the immutable candidate, so this domain cannot issue an acceptance result against the user's stated authority. Against the available revision-57 authority, the provisional result is **FAIL** because CAP-R2-001 remains reachable and prior `FIND-TASK-008-CLOSEOUT-13` remains open as CAP-R2-002.
