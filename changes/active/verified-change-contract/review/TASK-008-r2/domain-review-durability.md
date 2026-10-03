# Durability, concurrency, and persistent-state review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `5c3bb79b3598abd88a3a234611fc400096adc975`
- Cumulative range: `7d96c30066425e0cde2290842d5801307843283d..5c3bb79b3598abd88a3a234611fc400096adc975`
- Prior candidate/remediation range: `852894689388124960993014a46934e73c0ed2a8..5c3bb79b3598abd88a3a234611fc400096adc975`
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior review and remediation: `changes/active/verified-change-contract/review/TASK-008-r1/`

`HEAD` resolved to the candidate before and after this review.

## Reviewed boundary

This review traced the durability-sensitive part of the cumulative task and
remediation:

- the capacity benchmark's durable run, Scribe staging, audit staging, and
  Forge-demand backlog observations;
- the exact 60-second drain decision;
- benchmark cancellation, client flush/shutdown, server stop, and diagnostic
  retention after deadline or failure;
- the cross-replica durable-run claim and tenant-fairness proofs; and
- the approved-spec identity supplied for this repeat review.

The deployed topology in scope is two release `wyrd-server` replicas sharing
PostgreSQL and storage, with traffic driven through public Rust clients.
Production durable queue, claim, settlement, Scribe, and client behavior was
read only; this review does not authorize a redesign of it.

## Authority and source coverage

| Boundary | Authority and source evidence | Assessment |
|---|---|---|
| Approved specification identity | The caller names approved `spec.md` revision 58. The immutable candidate's `changes/active/verified-change-contract/spec.md:3` is revision 57, and `task-008-closeout.md:7` also says `spec_revision: 57`. A revision-58 document exists only in sibling commit `e202e1f272c101f93d13dcc9d0b2a7edc1320f74`, which is not contained in the candidate. It adds a runner PostgreSQL resource/durability decision while reusing the already-present `REQ-178` identifier. | **BLOCKED.** The caller-named authority is not part of the immutable subject, and the sibling document cannot safely be substituted for the named candidate path. |
| Scribe durable backlog | `capacity/evidence.rs:91-103` now includes `bifrost_scribe_staging_live_members` with persistence-queue and immutable-generation gauges. `bifrost-design.md:910-918` defines that gauge as the staging assembler's ready-or-claimed unpublished members, including restored members after restart. `capacity/step.rs:406-419` keeps the step pending until the aggregate is zero or the limit expires. | Conditional **PASS** against revision 57 and prior FIND-8. The benchmark no longer reports an empty Scribe backlog while a replica advertises durable staged members. |
| Persistent queue observations | `capacity/evidence.rs:238-267` reads unfinished/expected Verifier runs, audit rows above each tenant watermark, and unsettled Forge demands from PostgreSQL. `capacity/step.rs:353-364` derives accepted queued activations, drains those authorities, then reads durable run settlements for traffic evidence. | Conditional **PASS** against revision 57. The release benchmark observes production-owned state rather than a substitute in-memory queue. Metrics remain an operational observation, not durable proof; the dedicated integration/journey tests continue to own exactly-once guarantees. |
| Exact drain boundary | `capacity/step.rs:239-261,383-420` accepts an empty observation only at or before `DRAIN_LIMIT` and expires a non-empty read at the boundary. | Conditional **PASS** against prior FIND-9. The below/at/above focused test passed. |
| Client and process cleanup | `capacity/main.rs:124-188,311-343,500-531` reserves client and per-replica stop time, records deadline/cleanup failure in the report, shuts clients down, and stops replicas newest-first. `capacity/load.rs:196-228` documents flush and shutdown residue. `release_server.rs:347-388,469-483` bounds graceful process stop and preserves logs on abnormal drop. | **FAIL** for the blocking setup path; see DUR-001. Async-only focused proof does not establish the command-wide bound. |
| Cross-replica claims and tenant fairness | `pg_verification_runtime.rs:1267-1401` uses the shared PostgreSQL queue with two runtimes for exactly-once claims and a held flood for claim-order fairness. Candidate production sources keep claims and fenced settlement PostgreSQL-owned. | Conditional **PASS** against revision 57. These are appropriate durable integration proofs, but the review did not rerun their PostgreSQL lane. |

## Material proposed finding

### DUR-001 — INCORRECT: blocking setup subprocesses bypass the benchmark lifetime

- **Violated obligation:** Revision-57 `REQ-171` and prior
  `FIND-TASK-008-CLOSEOUT-2` require one absolute 30-minute boundary covering
  setup through bounded cleanup. Expiry must stop later work, retain
  diagnostics, fail the report, and use the owned cleanup path.
- **Location:**
  `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:159-187,311-314,411-412`;
  `crates/wyrd/wyrd-testing/src/release_server.rs:147-189,618-635`.
- **Evidence:** `Lifetime::measure` uses `tokio::time::timeout_at`, which can
  cancel only when the measured future yields. `Benchmark::provision` awaits
  `LocalServer::start`, but that async function runs migration and each tenant
  setup through `run`, whose `std::process::Command::output()` blocks the
  executor thread until the child exits. A stuck `wyrd-server migrate` or
  `wyrd-server setup` therefore prevents the measured future from reaching an
  await where the timeout can fire. The paused-clock test uses a pending async
  future and cannot exercise this path.
- **Observable consequence:** The operator command can outlive the approved
  30-minute ceiling during setup without reaching benchmark cleanup or writing
  the deadline failure report. If the blocked setup occurs after the release
  server starts, its process and existing durable tenant state remain outside
  the intended deadline-controlled cleanup until the subprocess eventually
  returns or the operator kills the benchmark.
- **Required testable correction:** Keep the correction inside the benchmark
  release-server harness. Make every migration/setup subprocess owned and
  terminable under the benchmark's remaining absolute lifetime, so expiry
  kills and reaps that child, returns control to `Benchmark::clean_up`, retains
  the server log and report failure, and does not change production server,
  queue, claim, or durability behavior. Add a focused proof using a controlled
  non-terminating setup command that demonstrates deadline termination,
  reaping, cleanup, and reportable failure; the existing async-pending test is
  insufficient by itself.

## Verification evidence and limits

Executed on candidate `5c3bb79b3598abd88a3a234611fc400096adc975`:

```text
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity \
  -E 'test(=tests::an_unfinished_benchmark_fails_and_cleans_up_by_its_deadline) | test(=evidence::tests::staged_members_hold_the_scribe_backlog) | test(=step::tests::a_backlog_drains_only_within_the_limit)'
```

Result: 3 passed, 11 skipped.

Limits:

- The caller-named revision-58 `spec.md` is absent from the candidate, so a
  complete authority-to-source acceptance judgment is impossible.
- The focused lifetime test proves cancellation of a yielding async future;
  it does not cover the blocking operator subprocess path in DUR-001.
- The two PostgreSQL integration tests, AC-041 journey, and unmodified default
  `mise run bench:capacity` were not rerun in this domain pass. Their recorded
  prior results are supporting evidence, not fresh execution evidence.
- The staged-member unit test proves scrape aggregation and the zero/nonzero
  decision. Production restoration/publication semantics are covered by the
  owning Scribe sources and existing Scribe tests, not re-executed here.

## Overall result

**BLOCKED**

The immutable candidate does not contain the caller-named approved revision-58
specification, so this reviewer cannot establish the complete durability and
concurrency contract without substituting authority from another branch. The
revision-57 remediation also has one independently source-backed defect:
`DUR-001` leaves blocking migration/setup subprocesses outside the effective
30-minute cancellation boundary. The Scribe staged-member observation and
exact drain-boundary remediations are otherwise credible within revision 57.
