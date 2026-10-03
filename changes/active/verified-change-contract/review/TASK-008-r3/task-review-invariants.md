# TASK-008 invariant review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13`
- Candidate: `5c3bb79b3598abd88a3a234611fc400096adc975`
- Cumulative range: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13..5c3bb79b3598abd88a3a234611fc400096adc975`
- Approved authority: `changes/active/verified-change-contract/spec.md`, approved revision 57
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior review and remediation: `changes/active/verified-change-contract/review/TASK-008-r1/`
- Prior blocked review used as hypotheses: `changes/active/verified-change-contract/review/TASK-008-r2/`

The candidate remained at `5c3bb79b3598abd88a3a234611fc400096adc975`
throughout this review. Revision 57 is present and agrees with the original
task and r1 remediation, so the authority mismatch that blocked r2 does not
apply. `.codegraph/` is absent; navigation used repository source and `rg`.

The caller explicitly sequenced `FIND-TASK-008-CLOSEOUT-13`, the unmodified
default benchmark execution, to integration after the other workstreams merge.
This report records that evidence as `DEFERRED`; it is not a finding or blocker
for this candidate.

## Navigation and invariant trace

`mise.toml:507-524` is the operator entry point. It starts RustFS, enters the
repository Postgres lifecycle, builds release `wyrd-server`, and launches the
`capacity` binary. `main.rs::Benchmark::prepare` creates the in-binary
`Lifetime`, local TLS judge, peer material, and signing key. `Benchmark::run`
places `Benchmark::measure` under the measuring deadline, then shuts clients
down and stops replicas. `Benchmark::provision` calls `LocalServer::start`,
which runs migration and four tenant setup commands before returning the first
replica. `Benchmark::measure` drives the typed warmup/ramp/sustained/scale-out
sequence through `Deployment::run`.

For each step, `load.rs::mix` produces four tenants' direct, queued, ingest,
and query lanes. Those lanes produce `Tally` values; durable verifier-run rows,
Postgres backlog state, and replica metric scrapes are produced by
`evidence.rs`; the local judge produces provider-wait samples; and the cgroup
owner produces CPU and peak-memory readings. `step.rs::Deployment::run` joins
those sources into one `Record`. `report.rs::{step_row,op_row,Report::passed}`
is the sink that selects the knee, applies each SLO, renders one shared table,
and requires the three verdict steps.

Sibling consumers were traced as well. `QueueConfig::default` remains the
queue used by the benchmark and the real-server AC-041 journey. The
cross-replica claim and tenant-round tests exercise the same
`VerificationRuntime`/Postgres queue used by release replicas. The Rust SDK
direct and observation journeys retain failed-judgment, exactly-once row-group,
and bounded-client-byte proof after those concerns were removed from the
capacity verdict.

The producer-to-sink values introduced by r1 remediation otherwise remain
coherent: judge waits are recorded separately from engine overhead; Scribe
backlog includes live staging members; drain emptiness is accepted only at or
before 60 seconds; CPU and peak memory share one measured interval; `StepKind`
owns external labels and verdict selection; and step/operation rows share one
schema. The remaining runtime defect is at the lifetime producer, before those
records exist.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Revision-57 authority and task metadata agree | `spec.md:1-4`; task frontmatter `spec_revision: 57`, `status: review`; r1 remediation names revision 57 | Immutable candidate inspection | PASS |
| REQ-171 exposes one server-capacity command and retires the three predecessor binaries/tasks | `mise.toml:507-524`; sole `capacity` Cargo target; predecessor source trees deleted | Candidate-wide name search outside historical change records | PASS |
| REQ-171 exact four-tenant mix: direct and queued `L/2`, ingest `2.5L` with 100 features/default queue, Oracle `L/2` over two five-minute shapes | `load.rs:480-515`; `TenantClients::connect`; `Request::{send,Query}` | `load::tests::{mix_offers_the_required_rates,queries_read_the_last_five_minutes}` | PASS |
| REQ-171 typed warmup/ramp/knee/one-replica sustained/two-replica sustained/scale-out topology and three-step verdict | `main.rs:346-394`; `step.rs::StepKind`; `report.rs:394-423` | `report::tests::verdict_needs_every_verdict_step` | PASS |
| REQ-171 one absolute 30-minute setup-through-cleanup boundary | `Lifetime` starts inside `Benchmark::prepare`; `Benchmark::run` bounds only `self.measure()`; `LocalServer::start` performs migration/setup through blocking `Command::output`; RustFS startup, Postgres bootstrap, and release build precede the binary | Cooperative paused-time unit test only; no blocking-child/process-level deadline proof | **FAIL (INV-R3-001)** |
| REQ-171 traffic and error accounting retains refusals, missed/lost requests, wrong judgments, and failed terminal states while leaving duplicate-claim proof to tests | `load.rs::Tally`; `step.rs::Deployment::ops`; `report.rs::{traffic,errors}` | `report::tests::every_approved_error_category_fails_without_duplicate_judgment`; recorded cross-replica claim test | PASS |
| AC-040 exact five reference workloads and four non-judge direct overhead SLOs/sample floor | `fixture.rs::Kind::{input,context}` and generated Cards; `evidence.rs::Scrapes::overhead`; `report.rs::overhead` | Capacity unit target; reference workload source audit | PASS, with default empirical proof deferred under FIND-13 |
| AC-040 judge overhead and provider wait are separate and provider wait is reported-only | `judge.rs::Judge::{calls,waits_since}` -> `Record::judge_wait_ms` -> `report.rs::judge_wait` | `report::tests::judge_provider_wait_is_reported_apart_from_overhead`; reduced smoke evidence | PASS |
| AC-041 ingest queue drain/refusal values flow from the default client queue into the capacity verdict | `TenantClients::connect`; ingest `Lane::drive`; `OpRecord::drain_seconds`; `report.rs::ingest_drain` | Mix/SLO unit tests and reduced smoke | PASS, with `L=200` empirical proof deferred under FIND-13 |
| AC-041/AC-042 real-server exactly-once 100-feature durability and flat client-owned bytes remain outside the benchmark | `observe_run.rs::sustained_hundred_feature_drift_lands_exactly_once_with_flat_client_bytes`; default queue remains unchanged except its benchmark reference | Task records the exact Postgres-backed journey passing | PASS |
| Correctness/isolation stays test-owned: queued cross-replica exactly-once, tenant rotation, direct failed LLM-judge and SPC verdicts, queued Custom failure | `pg_verification_runtime.rs`; `drift_verification.rs`; no corresponding benchmark verdict categories | Task records exact focused passing commands | PASS |
| Every server-owned backlog is observed through the exact 60-second boundary, including durable staged Scribe work | `evidence.rs::{scribe_backlog,Queue::backlog}`; `step.rs::{Drain::judge,Deployment::drain}`; `report.rs::backlog` | `staged_members_hold_the_scribe_backlog`; step/report below-at-above boundary tests | PASS |
| REQ-171 report uses one table with stable SLI meanings and step rows immediately followed by operation rows | `report.rs::{HEADER,table_row,Report::render}` | `report::tests::report_renders_one_shared_table` | PASS |
| Replica CPU and peak memory share one explicit interval and CPU uses that interval's duration | `step.rs::{ResourceWindow,Resources::between}` | `step::tests::resources_use_the_readings_interval` | PASS |
| `--profile` remains external, per-step/per-replica, attaches to serving PIDs, rejects missing/empty/unresolved evidence, and does not affect ordinary runs | `main.rs` profile selection; `step.rs::start_captures`; `profile.rs`; separate profiling target dir in `mise.toml` | Source/caller trace; no real profile execution was recorded | PASS with runtime verification limit |
| Prior FIND-5/FIND-7: one cohesive lifecycle owner and exhaustive step identity | `main.rs::Benchmark`; `step.rs::StepKind` and variant matching | Source/caller trace and capacity unit target | PASS |
| Prior FIND-6 and AGENTS.md section 16: every materially changed async boundary documents cancellation and partial progress | Most async owners have `# Cancellation`, but `main.rs:174-188` makes `Lifetime::bounded` the actual `timeout_at`/future-drop owner without documenting that cancellation contract | Static async-item audit | **FAIL (INV-R3-002)** |
| Prior FIND-13: one unmodified default execution proves the integrated release deployment and required SLOs | r1 remediation and task evidence explicitly leave the run to integration; only a shortened failing smoke is recorded here | Caller explicitly directs this review to record it as deferred, not blocking | **DEFERRED — integration, not a candidate failure** |
| Inherited REQ-089/101/114/115/135-137/145/146/151/152, INV-015, and AC-017/020-024/030/032/033 verification-runtime, contract, tenancy, audit, SDK, schema, and coordination closure remain intact | The cumulative diff changes benchmark/test owners, one queue-doc reference, and retained regression tests; it does not alter the production owners of these contracts | Task's recorded focused and aggregate evidence; cumulative source/diff inspection | PASS with recorded-evidence limit |
| Inherited REQ-172-177 and INV-019 shared client-queue invariants remain intact | Production queue change is documentation-only; real-server observation regression is added | Task's recorded queue/journey evidence; changed-source inspection | PASS |
| Inherited REQ-178-180, INV-020, and AC-043 gateway capture invariants remain intact | No gateway capture production or test owner changed in this cumulative range | Cumulative diff inspection and inherited task evidence | PASS |
| Non-goals: no production limiter, admission policy, telemetry service, storage format, public contract, compatibility route, or benchmark-owned fairness/claim judgment | Cumulative diff is confined to benchmark/test support, regression tests, task/review records, and a queue-doc reference | Complete changed-path inspection | PASS |

## Proposed findings

### INV-R3-001 — INCORRECT — the 30-minute lifetime does not bound the complete default command

- **Prior finding:** `FIND-TASK-008-CLOSEOUT-2`, revised by the r2 source audit.
- **Violated obligation:** revision-57 REQ-171 and r1 remediation R2 require one
  absolute wall-clock boundary covering the default command from setup through
  cleanup, with expiry stopping later work, retaining diagnostics, and failing
  cleanly.
- **Exact location:** `mise.toml:507-524`;
  `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:124-188,227-245,311-317,397-428`;
  `crates/wyrd/wyrd-testing/src/release_server.rs:147-189,618-635`.
- **Evidence:** RustFS startup, its setup container, the repository Postgres
  wrapper, and the release build all run before `Benchmark::prepare` creates
  `Lifetime`. Inside the binary, `Benchmark::prepare` itself is not passed to
  `Lifetime::measure`. Later, migration and each tenant setup use synchronous
  `Command::output()` inside the async measurement future. While one of those
  waits blocks its runtime thread, `timeout_at` cannot regain control to drop
  the future. The paused-time test supplies a cooperative pending future
  directly to `Lifetime`; it exercises neither uncovered path.
- **Observable consequence:** a stalled storage/bootstrap/build, judge
  preparation, migration, or tenant-setup operation can hold
  `mise run bench:capacity` beyond 30 minutes. A stalled operator child also
  prevents the normal failed report and bounded cleanup path from running.
- **Required testable correction:** retain `Benchmark` as the lifecycle owner
  and `LocalServer` as the child-process owner, but put every command-owned
  setup, preparation, measurement, client cleanup, and replica cleanup phase
  under one enforceable absolute deadline. Migration/setup children must be
  terminable and reaped when that deadline expires; work before the binary
  starts must be bounded by the same command-level limit. Preserve the existing
  failure report, log retention, and no-new-knob constraint. Prove a controlled
  stalled migration/setup child is killed and reaped, later work does not
  start, diagnostics survive, the command returns nonzero within the shortened
  test deadline, and normal cleanup still runs for owners already created.

### INV-R3-002 — VIOLATION — the shared timeout boundary omits its cancellation contract

- **Prior finding:** `FIND-TASK-008-CLOSEOUT-6`, revised by r2 validation.
- **Violated obligation:** `AGENTS.md` section 16 and
  `architecture/agent-rules.md` require every materially changed async Rust
  boundary to document cancellation and partial progress when relevant.
- **Exact location:**
  `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:174-188`.
- **Evidence:** `Lifetime::bounded` owns the actual `tokio::time::timeout_at`
  call and drops arbitrary supplied work on expiry. Its rustdoc only says it
  runs work until a deadline and names failures. It does not state that
  cancellation occurs at the future's next cooperative yield or that effects
  already produced remain governed by the supplied workflow's owner. The more
  specific caller docs do not define this shared boundary for a future caller.
- **Observable consequence:** a maintainer can reuse the helper while assuming
  rollback or immediate preemption, exactly where the helper's behavior is
  cancellation and retained partial effects.
- **Required testable correction:** add the cancellation/partial-progress
  contract to the existing `Lifetime::bounded` rustdoc. Preserve its callers
  and runtime behavior; add no wrapper, trait, or second timeout abstraction.
  Close with a static audit of `Lifetime::{bounded,measure,shut_down}` and the
  existing focused capacity target.

## Prior-finding closure

| Prior finding | Invariant-review result |
|---|---|
| `FIND-TASK-008-CLOSEOUT-1` | CLOSED: the legacy query, verification, and ingest capacity entry points are gone. |
| `FIND-TASK-008-CLOSEOUT-2` | OPEN, REVISED as `INV-R3-001`: the deadline does not govern the complete command or blocking setup children. |
| `FIND-TASK-008-CLOSEOUT-3` | CLOSED: provider wait is produced and rendered separately from engine overhead. |
| `FIND-TASK-008-CLOSEOUT-4` | CLOSED: revision-57 task/spec metadata agrees. |
| `FIND-TASK-008-CLOSEOUT-5` | CLOSED: `Benchmark` owns the multi-step lifecycle and reconnection. |
| `FIND-TASK-008-CLOSEOUT-6` | OPEN, REVISED as `INV-R3-002`: `Lifetime::bounded` lacks its own cancellation/partial-progress contract. |
| `FIND-TASK-008-CLOSEOUT-7` | CLOSED: `StepKind` owns the closed identity and labels. |
| `FIND-TASK-008-CLOSEOUT-8` | CLOSED: staged live members participate in Scribe backlog. |
| `FIND-TASK-008-CLOSEOUT-9` | CLOSED: the exact drain boundary is preserved through report judgment. |
| `FIND-TASK-008-CLOSEOUT-10` | CLOSED: aggregate and operation rows use one table schema. |
| `FIND-TASK-008-CLOSEOUT-11` | CLOSED: CPU and peak memory use one measured interval. |
| `FIND-TASK-008-CLOSEOUT-12` | CLOSED: duplicate claims are test-owned while approved capacity errors remain. |
| `FIND-TASK-008-CLOSEOUT-13` | DEFERRED by explicit caller sequencing to integrated execution after other workstreams merge; not a finding or blocker for this candidate. |

## Verification notes and limits

- `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity`:
  14 passed, 0 failed in this review.
- `git diff --check f6159606c5c959e8fcc3423574ab0e7e6c86ee13..5c3bb79b3598abd88a3a234611fc400096adc975`:
  clean.
- The task records passing focused Postgres-backed cross-replica claims,
  tenant rotation, AC-041 durability/memory, direct/queued judgment, format,
  lint, and Clippy commands. Those were treated as available evidence rather
  than all being rerun in this pass.
- No blocking-subprocess deadline test exists. The cooperative paused-time
  unit test cannot prove that path.
- No real `--profile` run is recorded; source establishes the optional workflow
  and its failure checks, but actual host attachment remains a verification
  limit.
- The unmodified default benchmark is intentionally deferred under the
  caller's integration sequencing for `FIND-TASK-008-CLOSEOUT-13`.

## Overall result

**FAIL**

The revision-57 candidate closes the capacity evidence and report invariants
from r1 except for the benchmark lifetime itself: the configured deadline is
not enforceable over the complete default command or its blocking operator
children. The shared timeout helper also violates the repository's mandatory
async cancellation-documentation rule. The separately missing default
benchmark run is recorded as deferred to integration and does not contribute
to this result.
