# Behavior review: TASK-008 closeout, round 3

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13`
- Candidate: `5c3bb79b3598abd88a3a234611fc400096adc975`
- Cumulative diff: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13..5c3bb79b3598abd88a3a234611fc400096adc975`
- Approved authority: `changes/active/verified-change-contract/spec.md`, approved revision 57
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior review/remediation: `changes/active/verified-change-contract/review/TASK-008-r1/`
- Prior blocked review used as hypotheses: `changes/active/verified-change-contract/review/TASK-008-r2/`

The candidate commit remained unchanged throughout this review. The checkout has
no `.codegraph/` directory, so source, Git diff, callers, consumers, and tests
were inspected directly. Existing untracked review artifacts are not part of
the immutable candidate.

## Navigation and caller-to-result trace

The operator entry point is `mise run bench:capacity`. Its shell body first
starts RustFS, enters the repository Postgres wrapper, builds release
`wyrd-server`, and only then starts the `capacity` binary. Inside that binary,
`main` awaits `Benchmark::prepare`; `Benchmark::{run,measure,provision,
reconnect,scale_out,step,clean_up}` own the in-process lifecycle. `provision`
calls `LocalServer::start`, whose mandatory migration and per-tenant setup
commands execute through blocking `std::process::Command::output`. Each
`Deployment::run` drives the public-client workload, collects exporter/SQL/
resource evidence, drains server-owned work, and returns a `Record`.
`report::{step_row,Report::passed,Report::render}` produces the operator-facing
Markdown/JSON verdict.

The changed Rust SDK journeys remain the focused consumers for direct/queued
judgment and AC-041 durability/flat-client-byte proof. The changed server
integration tests retain tenant-fair claim and cross-replica exactly-once proof
outside the benchmark, as revision 57 requires.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| The candidate is reviewed against approved spec revision 57 and the current closeout task | `spec.md` frontmatter is approved revision 57; `task-008-closeout.md` is `status: review`, `spec_revision: 57`, and maps REQ-171/AC-040/AC-041 plus inherited obligations | Immutable source/Git inspection | PASS |
| CLOSE-01: inherited TASK-008 verification behavior remains closed without rebuilding it | Cumulative diff changes only benchmark/test support, the task/review packet, one queue default, and focused Rust/server tests; it does not remove the recorded Rust/Python/TypeScript, HTTP/MCP, SQL, storage, auth, audit, Operator, or generated-contract owners listed in the task's inherited matrix | Task records focused journey, SQL, SDK, codegen, docs, format, and lint evidence; this review reran only the capacity unit target | PASS with the verification limits below |
| REQ-171: exactly one server-capacity command and binary remain | `mise.toml:507-524`; `wyrd-testing/Cargo.toml`; legacy verification, ingest, and query capacity targets/sources are deleted while external comparison tools remain | Cumulative changed-path and repository search inspection | PASS |
| REQ-171 workload: open-loop public Rust-client mix, four identical tenants, equal direct/queued five-kind shares, 2.5L ingest of 100 features, and L/2 lookup/aggregate queries | `capacity/load.rs:480-515`; `fixture.rs:39-176,202-415`; `Request::send` uses public Verification, WyrdState, and Bifrost clients | `load::tests::{mix_offers_the_required_rates,queries_read_the_last_five_minutes}` | PASS |
| REQ-171 steps and verdict: warmup, ramp to knee, sustained K on one and two replicas, scale-out 2K on two; all three verdict steps required | `capacity/main.rs:346-394`; `step.rs:58-123`; `report.rs:394-423` | `report::tests::verdict_needs_every_verdict_step` | PASS |
| REQ-171 traffic/error/latency/backlog SLOs and one shared report table | `step.rs:303-485`; `evidence.rs:62-298`; `report.rs:131-369,443-563`; staged Scribe members participate and drain is exact at 60 seconds | Capacity unit tests for every SLO failure, approved error categories, staged members, drain boundary, percentiles, and one-table rendering | PASS |
| AC-040 reference workloads and paired non-judge overhead; judge overhead and provider wait separate | `fixture.rs` builds the five approved cases; `evidence.rs::Scrapes::overhead`; `judge.rs::Judge` owns provider waits; `report.rs::overhead` keeps judge wait reported-only | `report::tests::{every_slo_failure_fails_the_step,judge_provider_wait_is_reported_apart_from_overhead}` | PASS |
| AC-041 benchmark construction and real-server queue durability/flat-byte proof | At L=200, `mix` offers 500 observations/s and `Request::Ingest` emits 100 features through default `QueueConfig`; changed Rust journey checks exact rows and flat owned bytes | Recorded real-server `sustained_hundred_feature_drift_lands_exactly_once_with_flat_client_bytes`; mix unit test | PASS for construction and test-owned correctness; the default performance execution is deferred below |
| Prior FIND-1 and FIND-3 through FIND-5, FIND-7 through FIND-12 are closed | Single authority; separate judge wait; revision-57 task metadata; `Benchmark` owner; `StepKind`; staged-member backlog; exact drain edge; one table; common resource window; duplication remains test-owned | 14/14 capacity unit tests passed in this review; recorded focused integration/journey evidence supports the external seams | PASS |
| Prior FIND-2: one enforceable 30-minute boundary covers the complete default command, including setup, and stops owned work cleanly | `Lifetime` starts in `Benchmark::prepare` and bounds later cooperative measurement/client shutdown, but `mise.toml:510-523` performs RustFS startup and release build first; `main.rs:625-627` awaits preparation before `Benchmark::run`; `release_server.rs:147-189,618-635` runs blocking migration/setup children without a deadline | `tests::an_unfinished_benchmark_fails_and_cleans_up_by_its_deadline` covers only cooperative futures already handed to `Lifetime`; no shell/build or stalled-child proof exists | **FAIL (BHV-R3-001)** |
| Prior FIND-6: every changed async cancellation owner documents cancellation and retained effects | Caller-specific async workflows document cancellation, but `Lifetime::bounded` is the shared owner that invokes `timeout_at` and drops arbitrary work; its own rustdoc at `main.rs:174-188` omits that cancellation/partial-progress contract | Static async-item audit; runtime unit coverage does not replace required documentation | **FAIL (BHV-R3-002)** |
| Prior FIND-13: one unmodified default capacity execution | The task/remediation records only the reduced failing smoke and explicitly sequences the full default run to integration | Caller explicitly requires this review to record FIND-13 as deferred until the other workstreams merge | **DEFERRED — not a blocker for this candidate** |
| No production redesign, new limiter, public contract, storage format, capacity-only fairness/exactly-once judgment, Grafana asset, or benchmark fast path | Diff is limited to the benchmark/test owners, deletion of obsolete benchmark entry points, focused correctness tests, queue default selection, and task/review evidence | Full cumulative diff inspection | PASS |

## Proposed findings

### BHV-R3-001 — INCORRECT — the 30-minute boundary does not cover the complete default command

- **Violated obligation:** Revision-57 REQ-171 requires the default
  `mise run bench:capacity` to finish within 30 minutes including setup. Prior
  `FIND-TASK-008-CLOSEOUT-2` further requires one absolute setup-through-cleanup
  boundary whose expiry stops later work, retains diagnostics, and fails
  cleanly.
- **Exact location:** `mise.toml:507-524`;
  `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:124-188,227-343,397-418,500-531,621-635`;
  `crates/wyrd/wyrd-testing/src/release_server.rs:147-189,347-388,469-483,618-635`.
- **Evidence:** RustFS startup, the Postgres wrapper, and the release build run
  before `Benchmark::prepare` creates `Lifetime`. Preparation itself is awaited
  outside `Benchmark::run`'s bounded measurement/report path. During required
  server provisioning, migration and four tenant setup commands use blocking
  `Command::output`; while one blocks the Tokio worker polling the bounded
  future, `timeout_at` cannot regain control to cancel it. The paused-clock test
  passes a cooperative pending future directly to `Lifetime` and exercises none
  of these reachable producers.
- **Observable consequence:** A stalled build, emulator/bootstrap command,
  migration, or setup child can keep the operator command alive past 30 minutes
  and can prevent the failed report and owned cleanup path from running. The
  benchmark therefore does not provide the bounded operational behavior it
  advertises even before the deferred default performance result is considered.
- **Required testable correction:** Keep lifecycle ownership at the existing
  command/`Benchmark` boundary and child ownership at `LocalServer`, but enforce
  one wall-clock deadline from the beginning of the `mise` command through
  report/cleanup. Run migration/setup as owned child processes that can be
  terminated and reaped when the remaining deadline expires; route in-binary
  preparation errors/expiry through the failed-report path once reporting is
  initialized. Add no new user-facing timeout knob or production deadline.
  Prove the real blocking-child path with a controlled child that never exits,
  asserting bounded nonzero completion, termination/reaping, retained
  diagnostics, and no later setup/measurement.

### BHV-R3-002 — VIOLATION — the shared timeout owner lacks its cancellation contract

- **Violated obligation:** `AGENTS.md` section 16 requires every new or
  materially modified async Rust item to document relevant cancellation and
  partial progress. This is also the unresolved documentation portion of prior
  `FIND-TASK-008-CLOSEOUT-6`.
- **Exact location:**
  `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:174-188`.
- **Evidence:** `Lifetime::bounded` directly owns `tokio::time::timeout_at` and
  drops the supplied workflow on expiry. Its rustdoc states only that it runs
  work until a deadline. `measure` and `shut_down` document their particular
  effects, but those caller comments do not define the reusable boundary's
  behavior for present or future callers.
- **Observable consequence:** A maintainer adding or changing a caller at the
  exact cancellation owner must infer whether timeout rolls work back and which
  effects survive, creating a realistic risk of treating partial durable setup
  or measurement work as atomic.
- **Required testable correction:** Add the missing cancellation/partial-effect
  contract to the existing `Lifetime::bounded` rustdoc: expiry drops the future
  at its next cooperative yield, and already-produced effects remain according
  to that future's owner. Preserve the more specific caller contracts; add no
  wrapper, trait, or second timeout abstraction. Close with a complete static
  async-item documentation audit and the existing capacity unit target.

## Prior-finding closure

- Closed: `FIND-TASK-008-CLOSEOUT-1`, `-3`, `-4`, `-5`, and `-7` through `-12`.
- Open, revised by source evidence: `FIND-TASK-008-CLOSEOUT-2` via
  `BHV-R3-001`.
- Open documentation violation: `FIND-TASK-008-CLOSEOUT-6` via
  `BHV-R3-002`.
- Deferred by explicit caller sequencing, not blocking this candidate:
  `FIND-TASK-008-CLOSEOUT-13`.

## Verification assessment and limits

- `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity`:
  14 passed, 0 failed in this review.
- `git diff --check f6159606c5c959e8fcc3423574ab0e7e6c86ee13..5c3bb79b3598abd88a3a234611fc400096adc975`:
  clean.
- The task/remediation records passing focused Postgres-backed fairness and
  exactly-once tests, the real-server AC-041 durability/flat-byte journey,
  direct/queued judgment journeys, Clippy, formatting, and lints. They were
  treated as available evidence and were not all rerun in this behavior pass.
- The full default `mise run bench:capacity` was not run, per the caller's
  explicit integration sequencing for `FIND-TASK-008-CLOSEOUT-13`.
- No test exercises a stalled migration/setup child or proves that the
  pre-binary shell/build portion shares the 30-minute deadline.

## Overall result

**FAIL**

The candidate closes the benchmark's workload, SLI/report, ownership, and
focused correctness gaps, and the full default benchmark is correctly treated
as deferred rather than blocking. It still does not enforce the approved
30-minute bound over the complete operator command and leaves the shared
cancellation boundary undocumented.
