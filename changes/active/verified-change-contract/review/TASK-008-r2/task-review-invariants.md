# TASK-008 invariant review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `5c3bb79b3598abd88a3a234611fc400096adc975`
- Cumulative range: `7d96c30066425e0cde2290842d5801307843283d..5c3bb79b3598abd88a3a234611fc400096adc975`
- Requested authority: `changes/active/verified-change-contract/spec.md`, approved revision 58
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior review and remediation: `changes/active/verified-change-contract/review/TASK-008-r1/`

The candidate remained at `5c3bb79b3598abd88a3a234611fc400096adc975`
during this review. `.codegraph/` is absent, so navigation used repository
source and `rg`.

The requested approved specification cannot be obtained from the immutable
subject. Candidate `spec.md:1-4` is approved revision 57, the original task
frontmatter names revision 57, and the remediation task names revision 57.
There is no revision-58 entry or artifact in the change packet. Under the
spec-driven authority order, revision 57 cannot silently substitute for the
caller-named revision 58. The result is therefore `BLOCKED`. The source audit
below records what can be established conditionally against committed revision
57; it is not an acceptance decision for unknown revision 58.

## Navigation and invariant trace

`mise.toml:507-524` is the operator entry point. It starts RustFS, enters the
repository Postgres wrapper, builds release `wyrd-server`, and only then starts
the `capacity` binary. In the binary, `main.rs::Benchmark::prepare` creates a
`Lifetime`, local judge, certificates, and key. `Benchmark::run` places
`Benchmark::measure` under that lifetime. `Benchmark::provision` then calls
`LocalServer::start`, which synchronously runs migration and four setup
subcommands before constructing the measured deployment. `Benchmark::measure`
drives typed `StepKind` plans through `Deployment::run`; lanes produce tallies,
the queue and replica metrics produce durable/backlog evidence, and
`report.rs::{step_row, Report::passed}` consume the resulting records.

The remediation's producer-to-sink values are coherent except for the lifetime
boundary. Judge request completion times are produced by `Judge`, sliced at
step marks into `Record::judge_wait_ms`, and rendered separately from engine
overhead. Scribe persistence, immutable-generation, and staging-live gauges are
summed into the Scribe backlog, polled through the exact 60-second boundary,
and consumed by the report. CPU and memory share one `ResourceWindow` and CPU
uses that window's measured duration. `StepKind` owns all external labels and
verdict selection. Aggregate and operation records render under one header.

The lifetime value, however, is produced after the `mise` command has already
performed RustFS startup and the release build, and it only governs the future
passed to `Lifetime::measure`. `LocalServer::start` calls blocking
`std::process::Command::output` for migration and tenant setup; while any such
call is blocked, the future cannot yield to `timeout_at`. The paused-time test
passes only a cooperative pending future directly to `Lifetime` and therefore
does not exercise either uncovered producer path.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| The review uses the exact caller-approved revision 58 authority | Candidate `spec.md:1-4` says approved revision 57; task and remediation metadata also name 57 | Immutable `git show` plus change-packet search found no revision 58 | **BLOCKED** |
| REQ-171 exposes one server-capacity benchmark | `mise.toml:507-524`; only the `capacity` Cargo target remains; the legacy `bifrost_query_capacity` task, target, and source are deleted | Candidate search outside `changes/` found no legacy entry point | PASS against revision 57 |
| REQ-171 workload and typed step topology remain exact | `load.rs::mix`; `Benchmark::measure`; `StepKind`; `Plan::new` | Capacity unit target, including mix and verdict tests | PASS against revision 57 |
| FIND-2: one absolute 30-minute boundary covers the complete default command, including setup and cleanup | `main.rs:240-245` starts the clock inside the binary; `mise.toml:511-523` performs storage startup and release build first; `release_server.rs:155-169,624-625` executes migration/setup with blocking, unbounded `Command::output` | The paused-time unit test covers only cooperative futures already inside `Lifetime`; no subprocess or full-command deadline proof exists | **FAIL (INV-R2-001)** |
| FIND-3: judge provider wait and engine overhead remain separate, reported-only evidence | `judge.rs::Judge::{calls,waits_since}` -> `step.rs::Record::judge_wait_ms` -> `report.rs::judge_wait` | `judge_provider_wait_is_reported_apart_from_overhead`; reduced smoke report | PASS against revision 57 |
| FIND-4: task metadata identifies the current approved contract | Task frontmatter says revision 57 and review state | Static inspection | **BLOCKED** for requested revision 58; corrected relative to the prior revision-49 defect |
| FIND-5/FIND-7: lifecycle ownership is struct-centered and step identity is exhaustive | `main.rs::Benchmark`; `step.rs::StepKind`; verdict matches variants | Source/caller trace; capacity unit target | PASS against revision 57 |
| FIND-6: changed async effect boundaries document cancellation and partial progress | `# Cancellation` contracts on benchmark, deployment, lane, request, fixture, judge, and queue async paths | Source audit plus recorded clean Clippy | PASS against revision 57, except the runtime deadline claim is behaviorally false under INV-R2-001 |
| FIND-8/FIND-9: all Scribe-owned durable staged work participates in the exact 60-second drain boundary | `evidence.rs::scribe_backlog`; `step.rs::Drain::judge`; `report.rs::backlog` | `staged_members_hold_the_scribe_backlog`; below/at/above-limit tests | PASS against revision 57 |
| FIND-10: one shared-column report table has each step immediately followed by its operations | `report.rs::{HEADER,table_row,Report::render}` | `report_renders_one_shared_table` | PASS against revision 57 |
| FIND-11: CPU and peak memory cover one interval and CPU divides by its measured duration | `step.rs::{ResourceWindow,Resources::between}` | `resources_use_the_readings_interval` | PASS against revision 57 |
| FIND-12: duplicate-run capacity judgment is absent while approved error categories remain | `Deployment::ops`; report error text | `every_approved_error_category_fails_without_duplicate_judgment`; recorded two-replica exactly-once test | PASS against revision 57 |
| FIND-13: one unmodified default run proves the release deployment, AC-040, AC-041, the verdict steps, and the 30-minute ceiling | Remediation evidence explicitly leaves R13 to integration (`TASK-008-CLOSEOUT-R1...:320`); task evidence contains only a modified, failing short smoke (`task-008-closeout.md:1549-1559`) | No committed or recorded unmodified default `mise run bench:capacity` result; local `target/capacity` is the same `L=20`, shortened FAIL report and is not immutable evidence | **FAIL (INV-R2-002)** |
| Revision-57 non-goals remain excluded | Remediation delta is confined to benchmark/test support, task evidence, and deletion of the obsolete benchmark; no new public contract, production limiter, or storage format | Cumulative and remediation diff inspection | PASS against revision 57 |

The original task maps many inherited verification-contract obligations beyond
the capacity remediation. Their implementation and recorded focused journeys
remain present in the cumulative candidate, but this reviewer cannot reconcile
them to revision 58 because that authority is missing. Green revision-57 tests
cannot define the absent revision-58 contract.

## Proposed findings

### INV-R2-001 — INCORRECT — the 30-minute lifetime does not bound the complete default command

- **Violated obligation:** committed revision-57 REQ-171 requires the default
  `mise run bench:capacity`, including setup, to complete within 30 minutes;
  prior `FIND-TASK-008-CLOSEOUT-2` requires one absolute setup-through-cleanup
  deadline whose expiry fails cleanly.
- **Location:** `mise.toml:507-524`;
  `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:240-245,311-317,411-428`;
  `crates/wyrd/wyrd-testing/src/release_server.rs:155-169,618-625`.
- **Evidence:** RustFS startup, Postgres-wrapper startup, and the release build
  execute before `Benchmark::prepare` creates `Lifetime`. Within measurement,
  migration and each tenant setup call blocking `Command::output` without a
  deadline. Tokio's `timeout_at` cannot cancel a future while that same poll is
  blocked in the subprocess wait. Thus the command can exceed 30 minutes or
  hang even though `Lifetime` and its cooperative-pending unit test exist.
- **Observable consequence:** an unavailable container service, stalled build,
  migration, or setup process can hold the operator command past its approved
  ceiling without producing the promised failed report and bounded cleanup.
- **Required testable correction:** place the complete default command's setup,
  measurement, and teardown—including external service/bootstrap and operator
  subprocess waits—under one enforceable wall-clock boundary. Deadline expiry
  must stop owned subprocesses/replicas, retain diagnostics, and fail the
  command. Prove the real blocking-subprocess path, not only a cooperative
  future passed directly to `Lifetime`.

### INV-R2-002 — MISSING — the authoritative default execution is still absent

- **Violated obligation:** prior `FIND-TASK-008-CLOSEOUT-13` requires an
  unmodified default execution as empirical proof of REQ-171, AC-040, AC-041,
  all three verdict steps, and the 30-minute ceiling.
- **Location:**
  `changes/active/verified-change-contract/review/TASK-008-r1/TASK-008-CLOSEOUT-R1-capacity-closure.md:320,329-344`;
  `changes/active/verified-change-contract/tasks/task-008-closeout.md:1549-1559`.
- **Evidence:** the remediation record explicitly marks R13 `NOT RUN
  (integrator)`. The only capacity execution changes the level and every
  duration, exits 1, never reaches `L=200`, and fails the sample floor and
  queued-traffic SLO. Unit, integration, and short smoke checks establish
  mechanics, not release capacity.
- **Observable consequence:** the candidate has no credible evidence that its
  default topology sustains the approved workload, satisfies AC-040/AC-041,
  passes the three verdict steps, or completes within the required ceiling.
- **Required testable correction:** after closing INV-R2-001, run the
  unmodified `mise run bench:capacity` in the prescribed release/Postgres/
  RustFS and 8-CPU/16-GiB-per-replica envelope. Preserve the complete report
  identity, verdict, `L=200` evidence, and end-to-end wall time. A failing run
  requires a diagnosed root-cause correction within the approved contract.

## Prior-finding closure

Against committed revision 57, prior findings 1 and 3 through 12 are closed by
the remediation source and focused proof. Prior finding 2 remains open in
revised form as `INV-R2-001`: the new lifetime bounds only cooperative work
after part of the command has already run. Prior finding 13 remains open as
`INV-R2-002`; its own implementation evidence defers the required execution.

## Verification notes

- `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity`:
  14 passed, 0 failed in this review.
- `git diff --check 852894689388124960993014a46934e73c0ed2a8..5c3bb79b3598abd88a3a234611fc400096adc975`:
  clean.
- The remediation records passing focused Postgres-backed exactly-once,
  fairness, AC-041 durability/memory, direct/queued judgment, formatting,
  lint, Clippy, and a reduced two-replica smoke. Those results were treated as
  available evidence, not rerun by this invariant reviewer.
- No unmodified default benchmark run or blocking-subprocess deadline test is
  available. No profile run is recorded.

## Overall result

**BLOCKED**

The caller-required approved revision 58 is absent from the immutable
candidate, so the repository cannot be accepted or rejected against that
unknown contract. Conditional on committed revision 57, the invariant audit
would be `FAIL`: the lifetime boundary is not enforceable over the complete
default command, and the required default execution remains missing.
