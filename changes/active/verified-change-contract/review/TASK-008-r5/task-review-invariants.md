# TASK-008 invariant review, round 5

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13`
- Candidate: `0973a03e5a389ea0fb3635c6d9175e25db0a6da0`
- Cumulative range: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13..0973a03e5a389ea0fb3635c6d9175e25db0a6da0`
- Approved authority: `changes/active/verified-change-contract/spec.md`, approved revision 57
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior reviews used as hypotheses: `review/TASK-008-r1/` through `review/TASK-008-r4/`
- Reviewed remediation: `review/TASK-008-r4/TASK-008-CLOSEOUT-R3-process-boundary-and-proof.md`

The candidate remained at `0973a03e5a389ea0fb3635c6d9175e25db0a6da0`
throughout this review. `.codegraph/` is absent, so navigation used Git,
repository search, and direct source inspection.

The caller explicitly sequenced `FIND-TASK-008-CLOSEOUT-13`, the unmodified
default benchmark execution, to integration after the other workstreams merge.
This report records that proof as **DEFERRED**; it is neither a candidate
finding nor a blocker. The caller also confirmed the integrator's rejection of
the former FIND-2 sub-part that required a pre-teardown report to include
post-report teardown time. Revision-57 REQ-171 requires the command to finish
within 30 minutes; it does not require rewriting an already-written report
after wrapper teardown. This review does not reintroduce that rejected claim.

## Navigation and invariant trace

`mise.toml:507-557` is the complete command-lifetime producer. Before any
setup action it fixes one Unix start and one 30-minute deadline, exports both,
and defines `phase RESERVE KILL COMMAND...`. Each phase starts non-foreground
GNU `timeout` as a background child. In that mode `timeout` becomes the process
group owner for its command and descendants. The phase computes its allowance
from the same absolute deadline, forwards `INT`/`TERM` to the timeout owner,
waits for it, and finally sweeps its process group. RustFS startup and setup,
the Postgres wrapper, the release build, and the capacity run all use that
owner. The wrapped login shell imports the same function rather than deriving
a second deadline.

The nested reservations are coherent. The capacity run receives TERM at
deadline minus 30 seconds and forced KILL at deadline minus 10 seconds. The
outer Postgres wrapper then has its existing TERM trap and up to five seconds
to run its Compose teardown before forced KILL at deadline minus five seconds.
A failed phase returns nonzero under the surrounding `errexit`, so no later
phase starts. The two task-entry proofs read the actual `mise.toml` task text,
replace only the overall limit, and exercise both a pre-wrapper command and a
nested run whose child ignores TERM. Their recorded results establish bounded
failure, descendant termination, later-step exclusion, and wrapper teardown.

Inside the binary, the same exported times flow through
`Lifetime::from_command` into monotonic `started`, `measure_until`,
`clients_until`, and `deadline` values. `Benchmark` remains the cohesive owner
of preparation, provisioning, measurement, client shutdown, replica stop, and
report creation. Direct migration and setup children remain owned by
`OperatorRun`; cancellation kills and reaps them while preserving stderr.
`Lifetime::bounded` retains its cooperative cancellation and partial-progress
contract. These downstream owners provide orderly cleanup and diagnostics;
the outer process groups are the final enforcement boundary when orderly work
does not finish.

`Benchmark::clean_up` now moves every synchronous `LocalServer::stop` onto
Tokio's blocking pool through `stop_replicas`. Replicas are still stopped
newest first, while results are reversed back into ordinal order. Each closure
continues to own `STOP_GRACE`, TERM, forced kill/reap, and log copying. If the
future is cancelled, the in-progress closure continues with its owned replica
and later replicas fall back to their existing `Drop` cleanup. The recorded
single-worker heartbeat proof shows timers continue during a deliberately slow
clean stop while the process is reaped and its log retained.

Sibling consumers remain coherent. `QueueConfig::default` still feeds both the
benchmark and the AC-041 real-server journey. The five-kind direct/queued mix,
default-queue ingest, Oracle queries, typed step sequence, report verdict,
provider-wait separation, full Scribe backlog, exact drain edge, common
resource interval, and test-owned correctness/fairness invariants are unchanged
from the source-audited r3/r4 candidate. The remediation changes no production
timeout, public contract, SLO, workload, storage format, or capacity claim.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Approved authority and task identity agree | `spec.md:1-5`; task frontmatter names revision 57 and review status | Immutable candidate inspection | PASS |
| REQ-171 exposes one server-capacity entry point and removes its predecessors | `mise.toml` has `bench:capacity`; `wyrd-testing/Cargo.toml` has one `capacity` binary; predecessor source trees are deleted | Candidate-wide search outside historical change records | PASS |
| REQ-171 four-tenant production mix, typed step sequence, knee selection, three verdict steps, and report shape remain intact | `capacity/{load,step,evidence,report,fixture}.rs`; no remediation delta to these owners | Recorded complete capacity target and focused report/load tests | PASS with recorded-evidence limit |
| Prior FIND-2: one absolute deadline is produced before setup and consumed by every command phase | `mise.toml:511-556`; `main.rs::Lifetime::from_command` | Static producer-to-sink trace; task-entry process proofs | PASS |
| Every mandatory phase owns its command descendants through TERM-to-KILL escalation | `mise.toml::phase`; non-foreground `timeout --kill-after`; final process-group sweep | `tests::a_hung_setup_step_is_killed_with_its_descendants_by_the_deadline` and `tests::a_hung_benchmark_run_is_killed_with_its_descendants_by_the_deadline`, each recorded passing with its exact command | PASS |
| No later phase starts after an expired phase, and Postgres teardown retains its opportunity | `errexit` task shell; outer wrapper reservation and existing TERM/EXIT traps in `with-test-postgres.sh` | Setup proof records only the first Docker call; nested-run proof records teardown and no later call | PASS |
| Report duration semantics preserve the approved design | `Report::total_seconds` remains the command-start-to-report-construction interval; wrapper teardown remains after the report by design | Caller/integrator authority rejects requiring post-report teardown in this field | PASS; rejected r4 sub-part excluded |
| Prior FIND-14: normal synchronous replica stop does not block a Tokio worker | `main.rs:649-677` runs `LocalServer::stop` through `spawn_blocking`, newest first, restoring ordinal result order | `tests::a_slow_replica_stop_leaves_the_runtime_free` recorded passing; heartbeat, reap, log, and duration assertions | PASS |
| Prior FIND-15: current named capacity-test evidence is complete and reproducible | Task revision-57 matrix records six full `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=...)'` commands | Each records one passing test | PASS |
| AC-040 reference work, non-judge overhead floor/SLO, and separate judge provider-wait evidence remain correctly produced and consumed | `fixture.rs`, `evidence.rs`, `judge.rs`, `step.rs::Record`, `report.rs` | Focused capacity tests and prior source audit | PASS structurally; default empirical run deferred |
| AC-041 default-queue refusal/drain evidence and real-server durability/flat-byte proof remain separate and intact | Capacity ingest lane; `observe_run.rs::sustained_hundred_feature_drift_lands_exactly_once_with_flat_client_bytes` | Recorded focused real-server journey | PASS structurally; default empirical run deferred |
| Correctness and isolation remain test-owned rather than benchmark verdict extensions | `pg_verification_runtime.rs`, `drift_verification.rs`; no fairness, duplicate-claim, or judgment-correctness verdict added to capacity report | Recorded exact focused commands | PASS |
| Prior FIND-1 and FIND-3 through FIND-12 remain closed | Single entry point; separate judge wait; current task metadata; `Benchmark` owner; cancellation docs; `StepKind`; complete Scribe backlog; exact drain boundary; one report table; common resource window; duplicate claims test-owned | Complete cumulative source trace and prior independently validated closure | PASS |
| Prior FIND-13 full unmodified default benchmark | No new empirical qualification is claimed by this remediation | Explicit caller sequencing | DEFERRED — non-blocking |
| Non-goals: no public/API/schema change, production limiter, timeout policy, new workload/SLO, storage change, supervisor framework, or capacity claim | Latest implementation delta is confined to the benchmark command, private harness/test support, and task evidence | Cumulative and remediation diff inspection | PASS |

## Proposed findings

None.

The latest remediation closes the remaining source-backed process ownership,
async blocking, and evidence-command hypotheses. I found no reachable invalid
state flowing from those producers into the benchmark lifecycle, report
verdict, cleanup path, or sibling production consumers. The integrator-rejected
post-report elapsed request is not an approved obligation and therefore is not
a finding.

## Prior-finding closure

| Prior finding | Invariant-review result |
|---|---|
| `FIND-TASK-008-CLOSEOUT-1` | CLOSED: only `bench:capacity` remains as the Wyrd server-capacity entry point. |
| `FIND-TASK-008-CLOSEOUT-2` | CLOSED for its approved process-boundary scope: every mandatory command phase owns and terminates its group under the shared absolute deadline. The integrator-rejected post-report teardown sub-part is excluded by caller authority. |
| `FIND-TASK-008-CLOSEOUT-3` through `FIND-TASK-008-CLOSEOUT-12` | CLOSED: their source and consumer corrections remain intact. |
| `FIND-TASK-008-CLOSEOUT-13` | DEFERRED by explicit caller sequencing to post-merge integration; not a candidate finding or blocker. |
| `FIND-TASK-008-CLOSEOUT-14` | CLOSED: ordinary `LocalServer::stop` work runs through Tokio's blocking pool. |
| `FIND-TASK-008-CLOSEOUT-15` | CLOSED: every current named capacity test has a complete exact command and recorded result. |

## Verification notes and limits

- This reviewer did not start another Cargo-backed command in the shared
  checkout. The candidate records: complete capacity target 14 passed with 4
  environment-gated tests skipped; each of the three new ignored lifecycle
  proofs passed when selected exactly; release-server tests passed 2/2;
  formatting, lints, and `git diff --check` passed.
- The two task-entry tests exercise the actual `mise.toml` script with a
  shortened limit and PATH-controlled stand-ins. They do not execute Docker,
  Postgres, or the release server; those real services are covered by the
  existing benchmark/journey evidence and the caller-deferred default run.
- No real `--profile` host capture was rerun in this remediation. The optional
  path and its failure checks remain source-audited and unchanged.
- The unmodified `mise run bench:capacity` execution remains intentionally
  deferred under `FIND-TASK-008-CLOSEOUT-13`; this PASS makes no empirical
  AC-040/AC-041 capacity qualification claim.

## Overall result

**PASS**

The cumulative candidate satisfies the original task's source and focused
proof obligations under approved revision 57. The process boundary now owns
every command phase and descendant group under one absolute deadline, normal
replica shutdown no longer blocks Tokio, exact focused evidence is
reproducible, and all adjacent invariants remain intact. The only remaining
qualification is the caller-deferred unmodified default benchmark run, which
is explicitly non-blocking for this candidate.
