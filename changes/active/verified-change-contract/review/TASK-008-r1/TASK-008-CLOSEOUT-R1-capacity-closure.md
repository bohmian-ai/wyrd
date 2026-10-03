---
id: TASK-008-CLOSEOUT-R1
kind: remediation
status: ready
spec: changes/active/verified-change-contract/spec.md
spec_revision: 57
original_task: changes/active/verified-change-contract/tasks/task-008-closeout.md
base: ce5c09ef3b559c35e65d6a3e73beebe0a69ae5bd
reviewed_candidate: 852894689388124960993014a46934e73c0ed2a8
requirements: [REQ-171, AC-040, AC-041]
findings: [FIND-TASK-008-CLOSEOUT-1, FIND-TASK-008-CLOSEOUT-2, FIND-TASK-008-CLOSEOUT-3, FIND-TASK-008-CLOSEOUT-4, FIND-TASK-008-CLOSEOUT-5, FIND-TASK-008-CLOSEOUT-6, FIND-TASK-008-CLOSEOUT-7, FIND-TASK-008-CLOSEOUT-8, FIND-TASK-008-CLOSEOUT-9, FIND-TASK-008-CLOSEOUT-10, FIND-TASK-008-CLOSEOUT-11, FIND-TASK-008-CLOSEOUT-12, FIND-TASK-008-CLOSEOUT-13]
route_to: wyrd-implement
---

# Capacity benchmark revision 57 closure

## Outcome

Make `bench:capacity` the single, bounded, maintainer-readable server-capacity
benchmark approved by revision 57; make every reported SLI faithful to its
measurement boundary; retain correctness and durability proof in the focused
tests that own it; and record one authoritative default execution that proves
REQ-171, AC-040, and AC-041.

This is a bounded remediation of the cumulative candidate
`ce5c09ef3..852894689`. Implement the smallest corrections at the existing
benchmark, test, task-record, and command owners. Do not redesign production
runtime behavior.

## Issue diagnoses and required outcomes

### FIND-TASK-008-CLOSEOUT-1 — second server-capacity entry point

REQ-171 requires one server-capacity benchmark. The candidate adds
`bench:capacity` but leaves `mise.toml`'s `bench:bifrost:query-capacity`, its
`wyrd-testing` Cargo target, and `src/bin/bifrost_query_capacity/` reachable.
That binary launches release `wyrd-server` and produces judged capacity rows,
so it is not one of revision 57's excluded external engine-comparison tools.
Operators therefore see two competing server-capacity authorities.

Delete the stale mise task, Cargo binary registration, and owned binary source.
Preserve separately named ClickBench and observability comparison tooling.
Closure requires repository search/task listing to show only `bench:capacity`
and the `capacity` binary as release-server capacity entry points.

### FIND-TASK-008-CLOSEOUT-2 — no benchmark-wide 30-minute boundary

REQ-171 bounds the complete default command, including setup, to 30 minutes.
`main.rs` only records elapsed time. Sequential tenant provisioning can enter
multiple five-minute waits, and each lane awaits every issued request after its
arrival window; setup, tails, and cleanup share no absolute deadline. A slow
dependency can therefore keep the operator command alive beyond the contract.

The benchmark lifecycle owner must apply one absolute 30-minute deadline from
before server/tenant setup through measured steps and bounded cleanup. Expiry
must stop later work, retain diagnostics, return failure, and use the existing
owned client/process cleanup. Do not change production request deadlines and do
not turn total duration into another per-step SLI. Prove the failure path with
a controllable shortened lifetime that cannot complete, including bounded
cleanup; the unmodified default run under FIND-13 proves the real ceiling.

### FIND-TASK-008-CLOSEOUT-3 — judge provider wait is not reported

AC-040 requires judge engine overhead and provider waits as separate,
unthresholded diagnostics. The candidate collects the engine-overhead
histogram and a judge call count, but neither Markdown nor JSON contains a
provider-wait duration. A count cannot separate the local provider's fixed
delay from Wyrd work.

Reuse the benchmark-owned local TLS `Judge` as the measurement owner and retain
per-step response-wait evidence. Render that evidence separately from judge
engine overhead in both Markdown and JSON, without adding a production metric
or a provider-wait threshold. A focused evidence/report test must use known
judge timing and show distinct fields; missing wait evidence must not masquerade
as the required diagnostic.

### FIND-TASK-008-CLOSEOUT-4 — task authority remains revision 49

The changed task body records revision 57 as implemented, while its operative
frontmatter remains `status: proposed`, `spec_revision: 49`, and
`planning_result: SPEC_REVISION_REQUIRED`, and omits REQ-171/AC-040 from its
mapping. Later workflow agents therefore receive an obsolete current contract.

Update the task frontmatter to the revision 57 review state, explicitly map
REQ-171, AC-040, and AC-041, and remove the obsolete planning-result blocker.
Keep historical planning and execution material in the body. Static inspection
must yield one unambiguous current revision 57 task contract.

### FIND-TASK-008-CLOSEOUT-5 — lifecycle ownership is split across free workflows

The benchmark setup-to-report sequence is dependency-backed, stateful,
multi-step orchestration, but free `benchmark` owns it. Free `connect` drains
and rebuilds `Deployment::clients` from deployment replicas and tenants. This
violates the repository's struct-centered rule and splits the client/replica
alignment invariant away from its owner.

Introduce one meaningful concrete benchmark lifecycle owner for configuration
and setup-to-report state. Keep measured-step execution on `Deployment`, and
place reconnection on the concrete owner that holds the deployment and auth
pacing state. Keep process `main` thin. Do not introduce a trait, generic layer,
or zero-sized utility wrapper. Source inspection must find no free helper that
mutates `Deployment`; unit tests and the reduced two-replica smoke must remain
green.

### FIND-TASK-008-CLOSEOUT-6 — async effect contracts are undocumented

Changed async workflows can be cancelled after release processes start, Cards
register, observations are admitted, request tasks reach the server, queues
flush, or partial evidence is collected. Their rustdoc describes errors but
not surviving effects, cleanup ownership, retry meaning, or whether partial
evidence is usable. This is a hard AGENTS.md documentation violation.

Add substantive cancellation and partial-progress documentation to every
applicable changed async lifecycle, provisioning, driver, deployment,
reconnection, flush, and shutdown boundary. Describe owned cleanup, possible
durable residue, retry safety, and partial-evidence disposition. Do not change
runtime behavior merely to satisfy documentation. Close with a complete source
audit plus rustdoc/Clippy and capacity tests.

### FIND-TASK-008-CLOSEOUT-7 — verdict step identity is stringly typed

`Plan::name` accepts arbitrary strings, while verdict selection and profile
paths depend on exact `"sustained"` and `"scale-out"` literals. A typo compiles,
runs load, and later appears as missing verdict evidence. The four step kinds
are a closed set where exhaustiveness matters.

Replace the string with one private four-variant step-kind enum. Derive the
serialized/rendered/profile label in one owner and match variants for verdict
selection. Preserve external labels. The verdict test must construct only valid
variants, find all three verdict identities, and retain report/profile names.

### FIND-TASK-008-CLOSEOUT-8 — Scribe backlog omits durable staged work

The Scribe saturation cell sums persistence queue depth and immutable
generation count. Both can reach zero after persistence hands work to durable
staging while `bifrost_scribe_staging_live_members` still reports ready or
claimed unpublished members. The benchmark can therefore pass and stop while
Scribe owns recovery-relevant work.

Reuse the production `bifrost_scribe_staging_live_members` gauge in the Scribe
zero-backlog decision alongside the existing signals. Do not create another
ledger or separately sum the subset claim gauge. Add a focused evidence test
where persistence/immutable are zero and one staged live member keeps the cell
failing until it clears.

### FIND-TASK-008-CLOSEOUT-9 — backlog can first clear after the deadline and pass

`Deployment::drain` checks emptiness before comparing elapsed time with the
60-second limit, and the report treats every `Some(seconds)` as PASS. Because
lane tails and client flushes precede the first poll while timing begins at
nominal load stop, the first empty observation can occur after 60 seconds and
still select a knee or final PASS.

Use the existing `DRAIN_LIMIT` so emptiness passes only when observed at or
before the deadline, and make report judgment preserve that boundary. Add no
grace or retry setting. Focused tests must cover below, exactly at, and above
60 seconds; only the above-limit case fails.

### FIND-TASK-008-CLOSEOUT-10 — report violates the single-table contract

REQ-171 calls for one table: step rows followed by per-operation rows using the
same SLI columns. `Report::render` closes a Steps table and opens a differently
headed Per operation table, changing meanings such as backlog/resource versus
driver columns. The result is not the approved directly comparable artifact.

Render aggregate and operation rows consecutively under one shared SLI schema.
Use an explicit non-applicable value where an operation has no value, preserve
the existing step verdict and JSON evidence, and keep cell meanings stable. A
render test must prove one header, equal column counts/meanings, and each step
immediately followed by its operation rows.

### FIND-TASK-008-CLOSEOUT-11 — CPU and memory use incompatible windows

CPU accumulation continues through lane/request and client-flush tails but is
divided by the planned arrival-window duration. Peak memory is read later,
after backlog drain. The two saturation signals cover different intervals and
CPU's denominator excludes part of its numerator interval, producing biased,
non-comparable evidence.

Define one explicit resource measurement interval for a step, sample both
signals over that interval, and divide CPU delta by its measured duration.
Preserve the existing cgroup envelope and reported-only status. A pure
resource-window test must prove both sampling boundaries and the denominator
use the same interval; the report must retain per-replica cores and memory.

### FIND-TASK-008-CLOSEOUT-12 — duplicate-run correctness remains a capacity SLO

Revision 57 moves exactly-once claim proof to deterministic tests. Nevertheless,
`Deployment::ops` turns `runs.created > tally.accepted` into a benchmark-failing
`duplicate run` error and the report advertises it. Duplication is not named in
REQ-171's Errors SLI, while the dedicated two-replica integration test already
owns the claim invariant.

Delete duplicate-run judgment and advertising from the benchmark. Preserve the
durable-run reads used for queued traffic/backlog and preserve refusals,
failures, lost requests, wrong judgments, and failed terminal statuses, which
REQ-171 explicitly treats as errors. A focused report test must show no
duplicate category while every approved error category still fails; the
cross-replica exactly-once integration test must stay green.

### FIND-TASK-008-CLOSEOUT-13 — no authoritative default execution

The task records only a modified `L = 20`, 5/10/15-second smoke. It exits 1,
produces 32 samples per kind, and never offers the AC-041 `L = 200` rate. Unit
tests prove construction and arithmetic, not release-server capacity. The task
therefore claims empirical PASS without evidence for the deployment, AC-040,
AC-041, the three verdict steps, or the 30-minute ceiling.

After closing the source findings, run the unmodified
`mise run bench:capacity` in the prescribed release/Postgres/RustFS,
8-CPU/16-GiB-per-replica envelope. Append the exact command, complete Markdown
and JSON report identity, and wall time to the task evidence. The run must reach
`L = 200`, expose the AC-040 sample/latency and judge-wait diagnostics, expose
AC-041 refusal/drain evidence, pass all three verdict steps, and finish inside
30 minutes. A failing run requires root-cause correction within the approved
behavior or an approved spec revision; another shortened run is not closure.

## Constraints and preserved behavior

- Keep the approved four-tenant workload, rates, reference fixtures, step
  durations, knee rule, SLO thresholds, and three verdict steps unchanged.
- Preserve public Rust-client traffic through real release server replicas,
  Postgres, RustFS, Scribe, Oracle, verification engines, auth, audit, tenancy,
  deadlines, replay fences, durability, and tracked shutdown.
- Preserve the existing queue default and the dedicated AC-041 durability/
  flat-byte journey.
- Keep wrong judgments and lost requests in benchmark error accounting.
- Keep correctness, fairness, and exactly-once claim assertions in their
  focused tests.
- Do not add a production metric when benchmark-owned judge timing suffices.
- Do not add dependencies, public contracts, configuration knobs, retry/grace
  controls, concurrency limits, storage formats, or compatibility paths.
- Do not weaken, ignore, delete, or bypass a test or repository gate.

## Explicit non-goals

- No production server, Scribe, Oracle, Forge, verification-engine, SDK, or
  queue behavior redesign.
- No new admission controller, capacity policy, stage-specific permit, or
  autoscaling decision.
- No benchmark-owned fairness, exactly-once, or tenant-isolation assertion.
- No Grafana, external-provider capacity result, or new benchmark family.
- No change to the approved workload, numeric SLOs, or default step sequence.

## Acceptance criteria mapped to findings

| Criterion | Findings | Required observable result |
|---|---|---|
| R1 Single authority | FIND-1 | Only `bench:capacity` remains as a release-server capacity benchmark. |
| R2 Bounded lifecycle | FIND-2 | Whole default command is deadline-bound from setup through cleanup and fails cleanly on expiry. |
| R3 Complete AC-040 diagnostic | FIND-3 | Judge overhead and provider wait appear separately in Markdown and JSON. |
| R4 Current task contract | FIND-4 | Frontmatter identifies revision 57 review and REQ-171/AC-040/AC-041 without obsolete blocker. |
| R5 Discoverable ownership | FIND-5 | One concrete lifecycle owner; no free deployment-mutating reconnection workflow. |
| R6 Effect documentation | FIND-6 | Every applicable changed async boundary documents cancellation and partial progress. |
| R7 Exhaustive step identity | FIND-7 | A private enum owns all step identities and labels. |
| R8 Complete Scribe saturation | FIND-8 | Staged live members prevent a zero-backlog result. |
| R9 Exact drain deadline | FIND-9 | Backlog clearing after 60 seconds fails. |
| R10 One report table | FIND-10 | Step and operation rows share one header/schema in one table. |
| R11 Comparable resources | FIND-11 | CPU and memory share one measured interval and CPU uses its actual duration. |
| R12 Test-owned duplication | FIND-12 | Duplicate-run judgment is absent from capacity; exactly-once test remains green. |
| R13 Empirical acceptance | FIND-13 | Unmodified default run passes required steps/SLOs at the required envelope within 30 minutes. |

## Focused and broader proof

Run all verification through `mise` and record exact results.

1. Add/run exact capacity binary tests for:
   - lifecycle deadline expiry and cleanup;
   - separate judge provider-wait rendering;
   - staged-live-member backlog accounting;
   - drain boundary below/at/above 60 seconds;
   - one-table shared-column rendering;
   - common resource interval arithmetic;
   - exhaustive verdict-step selection; and
   - approved error categories without duplicate-run judgment.
2. Run the complete capacity binary unit target:
   `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity`.
3. Re-run the exact two-replica claim and tenant-fairness integration tests
   through the repository-managed Postgres wrapper.
4. Re-run the exact AC-041 sustained 100-feature real-server journey and the
   affected direct/queued judgment journey.
5. Run the narrowest owning `wyrd-testing`, server-test, and Rust SDK lanes
   required by `mise.toml`; run `mise run fmt`, `mise run lints`, and
   `git diff --check`.
6. Run the unmodified `mise run bench:capacity` last and record its complete
   report, configuration identity, verdict, and wall time. This performance
   run is required evidence and is not replaced by unit/integration lanes.

Route this remediation directly to `$wyrd-implement`. A later
`$wyrd-task-review` must review the complete new cumulative candidate against
the original task and revision 57 specification.

## Integrator finding validation

The integrator checked each finding against revision 57 and AGENTS.md.

- FIND-1 through FIND-12: **valid, accepted.**
  - FIND-1 is valid because REQ-171 says no other server capacity benchmark exists, and the `opitimization-and-benchmarks` change that would delete `bench:bifrost:query-capacity` has not been implemented.
- FIND-13: **valid, but sequenced to integration.** The unmodified default run is a pass/fail proof against the integrated server. It cannot pass until the approved `audit-outbox` and `forge-concurrent-planning` changes merge: capacity run 3 showed that two replicas lose throughput on the synchronous audit chain-head lock. This remediation closes FIND-1 through FIND-12. The integrator runs the default `mise run bench:capacity` after merging, records it in the task evidence, and the next task review judges FIND-13 on that evidence.

## Implementation evidence

Base `852894689`. Owner: `crates/wyrd/wyrd-testing/src/bin/capacity/`;
`release_server.rs` only exposes `STOP_GRACE` and `Metrics::parse`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| R1 Single authority (FIND-1) | Deleted `[tasks."bench:bifrost:query-capacity"]` (`mise.toml`), its `[[bin]]` (`wyrd-testing/Cargo.toml`), and `src/bin/bifrost_query_capacity/`. ClickBench/observability comparison tooling untouched | `git grep -n "bifrost_query_capacity\|query-capacity" -- ':!changes'`: no hits. `mise tasks info bench:capacity` sources this checkout's `mise.toml`; `bench:bifrost:query-capacity` now resolves only from the parent checkout's `mise.toml` until merge | PASS |
| R2 Bounded lifecycle (FIND-2) | `main.rs::Lifetime` (30-minute `LIMIT` from `Benchmark::prepare`, before any server starts; reserves `CLIENT_SHUTDOWN` and `REPLICAS × (STOP_GRACE + 5 s)`); `Benchmark::run` bounds `measure` then `clean_up`; expiry or error is recorded in `Report::failure`, fails the verdict, keeps finished step records, server logs, and the written report | `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=tests::an_unfinished_benchmark_fails_and_cleans_up_by_its_deadline)'` (paused clock: pending measure fails at 45 s of a 60 s lifetime, pending cleanup stops at 50 s); `-E 'test(=report::tests::verdict_needs_every_verdict_step)'` (a failure fails and renders `Stopped early`). The real 30-minute ceiling is FIND-13 | PASS |
| R3 Judge provider wait (FIND-3) | `judge.rs::Judge` records each completion's wait; `Record::judge_wait_ms` (JSON) and `report.rs::judge_wait` (Markdown, separate from `eval_llm_judge` overhead; `not measured` when absent); no threshold, no production metric | `-E 'test(=report::tests::judge_provider_wait_is_reported_apart_from_overhead)'`; smoke shows `judge provider wait 201.0/201.5/201.7 ms (n 40)` beside `eval_llm_judge ≤0.5 ms` | PASS |
| R4 Current task contract (FIND-4) | `tasks/task-008-closeout.md` frontmatter: `status: review`, `spec_revision: 57`, REQ-171/AC-040/AC-041 mapped, `planning_result` removed, remediation linked; a one-paragraph note marks revision 49 planning as history | Static inspection of lines 1-18 | PASS |
| R5 Discoverable ownership (FIND-5) | `main.rs::Benchmark` owns settings, lifetime, fixtures, deployment, auth pacing, records, knee; `prepare/run/measure/provision/scale_out/reconnect/step/clean_up` are inherent methods; `main` only prepares and runs. Free `benchmark` and `connect` deleted; measured steps stay on `Deployment` | `grep -n "^async fn\|^fn" main.rs`: only `identity`, `release_binary`, `install_tracing`, `main`; capacity unit target; reduced smoke below | PASS |
| R6 Effect documentation (FIND-6) | `# Cancellation` on every async fn in the binary: `Lifetime::{measure, shut_down}`, `Benchmark::{prepare, run, measure, provision, scale_out, reconnect, step, clean_up}`, `Deployment::{run, drain}`, `Lane::drive`, `Request::send`, `TenantClients::{connect, flush, shutdown}`, `Tenant::{provision, seed, count}`, `await_fit`, `Judge::start`, `Queue::{connect, now, backlog, runs}` | `grep -n "async fn" src/bin/capacity/*.rs` cross-checked; `mise exec -- cargo clippy --locked -p wyrd-testing --all-targets --all-features -- -D warnings` clean | PASS |
| R7 Exhaustive step identity (FIND-7) | `step.rs::StepKind` (four variants, one `label`, `Serialize` via `label`); `Plan::new`; `Report::verdict_steps` selects by variant; profile paths and report use `label()` | `-E 'test(=report::tests::verdict_needs_every_verdict_step)'` (all three found, JSON `name` stays `scale-out`) | PASS |
| R8 Complete Scribe saturation (FIND-8) | `evidence.rs::scribe_backlog` adds production `bifrost_scribe_staging_live_members` | `-E 'test(=evidence::tests::staged_members_hold_the_scribe_backlog)'` | PASS |
| R9 Exact drain deadline (FIND-9) | `step.rs::Drain::judge` (empty at or before `DRAIN_LIMIT` drains; otherwise expires at it); `report.rs::backlog` passes only `seconds <= DRAIN_LIMIT` | `-E 'test(=step::tests::a_backlog_drains_only_within_the_limit)'`, `-E 'test(=report::tests::backlog_passes_only_within_sixty_seconds)'` (59.9 and 60.0 pass, 60.1 fails) | PASS |
| R10 One report table (FIND-10) | `report.rs::{HEADER, table_row}`: step row (operation `all`) followed by its operation rows under one header; `n/a` where a column does not apply; driver misses moved into the traffic cell so CPU / memory keeps one meaning | `-E 'test(=report::tests::report_renders_one_shared_table)'`; smoke report | PASS |
| R11 Comparable resources (FIND-11) | `step.rs::ResourceWindow` opens CPU readings and `memory.peak` together before arrivals and closes both at one instant after every lane and flush; `Resources::between` divides by the readings' interval (`seconds` in JSON) | `-E 'test(=step::tests::resources_use_the_readings_interval)'` | PASS |
| R12 Test-owned duplication (FIND-12) | `duplicate run` removed from `Deployment::ops`, `OpRecord` docs, module docs, and rendered SLOs; lost, refusal, wrong judgment, and failed-run statuses kept | `-E 'test(=report::tests::every_approved_error_category_fails_without_duplicate_judgment)'`; `WYRD_LOG=info scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test pg_verification_runtime -E 'test(=two_replicas_claim_each_queued_run_exactly_once) \| test(=a_flooding_tenant_does_not_delay_another_tenants_run)'`: 2 passed | PASS |
| R13 Empirical acceptance (FIND-13) | Sequenced to the integrator after `audit-outbox` and `forge-concurrent-planning` merge | Not run here, per the integrator finding validation | NOT RUN (integrator) |

Commands:

- `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity`: 14 passed.
- `scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-testing --lib`: 58 passed. Without the wrapper, `bifrost::cluster::tests::process_pool_lifecycle_isolated_across_restart` refuses to run because `WYRD_TEST_DATABASE_ADMIN_URL` is unset (environment, not a defect).
- `scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-sdk-rust --test observe_run -P journey --run-ignored=all -E 'test(=sustained_hundred_feature_drift_lands_exactly_once_with_flat_client_bytes)'`: 1 passed.
- `scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-sdk-rust --test drift_verification -P journey --run-ignored=all -E 'test(=direct_execution_judges_supplied_input_through_the_sdk) | test(=drift_methods_fit_score_persist_and_dispatch)'`: 2 passed.
- `mise run fmt`: clean. `mise run lints`: exit 0. `mise exec -- cargo clippy --locked -p wyrd-testing --all-targets --all-features -- -D warnings`: clean. `git diff --check`: clean.
- Reduced two-replica smoke (plumbing only, not FIND-13):
  `WYRD_LOG=info mise run bench:capacity -- --levels 20 --warmup-seconds 5 --ramp-seconds 10 --sustained-seconds 15`:
  exit 1 as expected at these windows, wall 136 s including build, report
  setup 14 s / total 102 s. Every step ran; one table; zero errors; every
  backlog drained ≤ 5.0 s; provider wait ≈ 201 ms reported apart from
  overhead; both replicas stopped cleanly. The failures are the known
  short-window ones: the sample floor (n 32 < 1,000) and queued traffic
  (87.5–88.8%).

Limits:

- The deadline expiry is proven in-process with a paused clock rather than
  a shortened real run, because a lifetime override would be a new
  configuration knob, which this task forbids.
- `LocalServer::stop` remains synchronous; the cleanup reserve covers its
  `STOP_GRACE` bound per replica.

Non-goals stayed excluded: no production server, Scribe, Oracle, Forge,
engine, SDK, or queue change; no new metric, knob, dependency, or public
contract; workload, rates, SLOs, and step sequence unchanged.

IMPLEMENTED
