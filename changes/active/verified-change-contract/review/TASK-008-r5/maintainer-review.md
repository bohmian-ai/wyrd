# Maintainer Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13`
- Candidate: `0973a03e5a389ea0fb3635c6d9175e25db0a6da0`
- Range: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13..0973a03e5a389ea0fb3635c6d9175e25db0a6da0`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 57
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior reviews treated as hypotheses: `review/TASK-008-r1/` through
  `review/TASK-008-r4/`
- Latest remediation reviewed:
  `review/TASK-008-r4/TASK-008-CLOSEOUT-R3-process-boundary-and-proof.md`
- Governing maintainer authority: `AGENTS.md` §§5, 6, 11, and 16;
  `architecture/agent-rules.md`;
  `architecture/references/languages/{maintainer-style,rust-core,testing-workflows,spec-driven-development}.md`;
  and the applicable Wyrd/Bifrost lifecycle authority.

The candidate commit matched the requested identity before and after source
inspection. CodeGraph is absent, so source, caller, and test tracing used Git
and repository text directly. This report changes no reviewed production or
test source.

Caller authority is applied as follows: `FIND-TASK-008-CLOSEOUT-13`, the full
default benchmark, is deferred to integration and is not a blocker here. The
report is intentionally written before outer teardown; the rejected request to
extend `total_seconds` over post-report teardown is not revived as a finding.

## Changed-surface coverage

| Surface | Material symbols, owners, callers, and tests followed | Maintainer assessment |
|---|---|---|
| Command and process-group ownership | `mise.toml` `bench:capacity` through its exported `phase` definition, RustFS setup, Postgres wrapper, release build, and capacity run; the two new task-entry process tests and their `run_task`/stand-in helpers | The single command remains discoverable and every phase reuses the one absolute deadline. The local `phase RESERVE KILL COMMAND...` vocabulary and adjacent comment make TERM/KILL/process-group behavior followable without introducing another command or dependency. The tests execute the task text itself and name the two failure paths clearly. |
| Benchmark lifetime and orchestration | `capacity/main.rs::{Lifetime,Benchmark,main}` through `Lifetime::{from_command,new,measure,shut_down,bounded}`, `Benchmark::{prepare,run,measure,provision,scale_out,reconnect,step,clean_up}`, `Report::write_to`, and `Deployment::run` | `Lifetime` remains the cohesive deadline owner and `Benchmark` the setup-to-report owner. Preparation, measurement, cleanup, reporting, and exit are ordered and documented. The latest extraction of replica-stop orchestration from `Benchmark::clean_up` creates the material owner-shape defect below. |
| Replica and operator process lifecycle | `release_server.rs::{LocalServer,OperatorRun,STOP_GRACE}` through migration, setup, serving, joined replicas, synchronous stop, graceful/forced termination, and both `Drop` paths; callers in `Benchmark::{provision,scale_out,clean_up}` | `LocalServer` remains the concrete process owner and `OperatorRun` the focused migration/setup-child owner. Their method names, errors, cancellation effects, and retained diagnostics are substantive. `LocalServer::stop` correctly remains synchronous; the async-runtime boundary belongs at its caller. |
| Latest normal-cleanup boundary | `capacity/main.rs::{Benchmark::clean_up,stop_replicas}` and `tests::a_slow_replica_stop_leaves_the_runtime_free` | `spawn_blocking` closes prior FIND-14 behaviorally, preserves newest-first stop and ordinal report order, and documents cancellation. However, the new module-level workflow threads the replica collection and output dependency instead of remaining on the owner that holds that lifecycle (MNT-R5-001). |
| Capacity domain modules | `step.rs::{Deployment,StepKind,Plan,ResourceWindow,Drain,Record}`, `load.rs::{Op,Work,Lane,TenantClients,Request}`, `fixture.rs::{Kind,Tenant}`, `evidence.rs::{Scrapes,Queue,Backlog}`, `judge.rs::{Judge,TlsListener}`, `profile.rs::{Profile,Capture}`, and `report.rs::{Cell,Row,Report,step_row,op_row}` through benchmark construction, execution, evidence, and rendering | The cumulative benchmark remains capability-shaped: closed sets use enums, stateful load/evidence/process behavior has concrete owners, and free report helpers are deterministic transformations. Names and field types expose the workload and evidence model. No generated declaration applies. |
| Process proofs and test clarity | `capacity/main.rs::tests::{an_unfinished_benchmark_fails_and_cleans_up_by_its_deadline,a_stalled_tenant_setup_stops_the_run_by_its_deadline,a_slow_replica_stop_leaves_the_runtime_free,a_hung_setup_step_is_killed_with_its_descendants_by_the_deadline,a_hung_benchmark_run_is_killed_with_its_descendants_by_the_deadline}` plus `stand_in`, `run_task`, `reaped`, `stopped`, and `calls` | Test names and assertions describe caller-relevant lifetime, descendant, runtime-progress, teardown, diagnostic, and report outcomes. Environment requirements and deliberate durations are visible on the ignored tests. The helpers stay beside their only callers. |
| Current task evidence | Revision-57 implementation matrix in `task-008-closeout.md`, including the six focused capacity tests | Prior FIND-15 is closed: every named test now carries one complete pinned `mise exec -- cargo nextest run --locked` command, explicit package/binary target, exact selector, and recorded result. |
| Cumulative correctness and regression proof | `pg_verification_runtime.rs` two-replica claim/fairness paths, `drift_verification.rs` direct judgments, `observe_run.rs` sustained 100-feature ingest, and their public SDK/server consumers | These retain appropriately placed integration/journey proof and descriptive outcome names. The latest remediation changes no public contract, workload, SLO, schema, Python/TypeScript declaration, or production server behavior. |

## Material findings

### MNT-R5-001 — replica shutdown orchestration was extracted from its lifecycle owner into a free async workflow

- **Changed location:**
  `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:589-604`
  (`Benchmark::clean_up`) and `:649-677` (`stop_replicas`), called directly
  again only by the focused test at `:948-992`.
- **Governing rule or guide principle:** `AGENTS.md` §5 and
  `architecture/agent-rules.md` require stateful, multi-step workflows and
  internal orchestration to be inherent methods on the concrete owner holding
  their dependencies. `rust-core.md` says passing dependencies as arguments
  does not make orchestration stateless; `maintainer-style.md` requires a
  workflow to remain discoverable with its owner.
- **Evidence:** `Benchmark::clean_up` owns the shutdown transition: it takes
  the `Deployment`, drains its clients, and owns the output directory. The
  remediation replaced its inline replica loop with `stop_replicas(replicas,
  output)`. That function is not a deterministic transformation: it consumes
  live `LocalServer` handles, derives log destinations, spawns and awaits
  blocking process stops in newest-first order, translates join/process
  failures, and reverses results into ordinal order. Its entire dependency
  bundle is threaded through parameters, and its only production caller is
  the owner from which it was extracted.
- **Concrete maintenance cost:** a maintainer changing deployment cleanup must
  now preserve one lifecycle across `Benchmark::clean_up` and a module-level
  function whose name does not expose which owner guarantees client-before-
  replica ordering or ordinal report alignment. The test-driven extraction
  also creates functional implementation drift in newly modified Rust where
  repository authority makes owner shape a hard completion criterion.
- **Smallest testable correction:** keep `LocalServer::stop` synchronous and
  keep the existing `spawn_blocking`, newest-first stop, log naming, join-error
  conversion, and ordinal result behavior, but return replica shutdown to an
  inherent owner. The existing `Deployment`, which owns `replicas`, is the
  nearby Wyrd shape: give it the focused asynchronous shutdown operation (with
  the output destination supplied by `Benchmark`) and have
  `Benchmark::clean_up` call that method after client shutdown. Exercise that
  owner method in `a_slow_replica_stop_leaves_the_runtime_free`; no new helper
  type, trait, process abstraction, or production behavior is needed.
- **Nearby Wyrd pattern:** the same `Deployment` already owns measured runtime
  operations such as `Deployment::run`, while `LocalServer` owns each concrete
  process stop. Keeping collection-level shutdown on `Deployment` preserves
  that existing division rather than introducing a third ownerless workflow.

## Prior-finding closure and calibration

- The process-group portion of prior `FIND-TASK-008-CLOSEOUT-2` is closed by
  the one `phase` boundary in `mise.toml` and its setup/run descendant tests.
  The integrator-rejected post-report elapsed sub-part is intentionally absent.
- Prior `FIND-TASK-008-CLOSEOUT-14` is behaviorally closed: ordinary replica
  stop runs through Tokio's blocking pool and the heartbeat proof checks that
  an async worker continues to progress. MNT-R5-001 concerns the newly changed
  structural shape, not a demand to undo `spawn_blocking`.
- Prior `FIND-TASK-008-CLOSEOUT-15` is closed by the six complete exact
  commands in the active task record.
- The exported shell-function text and `eval` are awkward in isolation, but
  they keep one literal process-group implementation across the outer shell
  and the wrapper's login shell. Replacing them with a general supervisor or
  another script solely for this benchmark would not provide a smaller
  maintenance boundary, so this is not a finding.
- `Benchmark` and the in-module test section are large, but their operations
  share one benchmark lifecycle and the tests exercise that binary's private
  command boundary. Line count alone does not justify a module split.

## Verification reviewed and limits

- Remediation evidence records 14 passing default `capacity` tests with four
  environment/process proofs skipped, passing exact process-group proofs,
  the passing slow-replica-stop proof, the stalled-setup proof, two passing
  `release_server` library tests, clean formatting, clean workspace lints, and
  a clean diff.
- The six current named capacity tests each record a complete exact command
  and one passing test.
- `git diff --check
  f6159606c5c959e8fcc3423574ab0e7e6c86ee13..0973a03e5a389ea0fb3635c6d9175e25db0a6da0`
  passed during this review. Cargo-backed commands were not rerun in the shared
  checkout.
- The unmodified default benchmark was not run. Per caller direction,
  `FIND-TASK-008-CLOSEOUT-13` is deferred to the post-integration sequence and
  is not a blocker for this candidate; no empirical AC-040/AC-041 claim is
  made here.

## Overall result

**FAIL**

Coverage is complete, and the latest remediation closes its behavioral process
and proof gaps. The newly extracted `stop_replicas` function nevertheless
violates Wyrd's hard struct-centered workflow rule. The correction is bounded
to the private benchmark harness and preserves all approved behavior.
