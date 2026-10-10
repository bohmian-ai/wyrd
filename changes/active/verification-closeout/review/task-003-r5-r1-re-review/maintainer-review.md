# TASK-003 R1 maintainer review

## Subject and coverage

- Immutable cumulative range: `7f79fb3417db651adedac194ada8908f0a0372d7..9a8f9f7eef95f70d356c037a192b7d7b90a37f31` (base abbreviated below as `7f79fb341`; candidate `9a8f9f7ee`). Latest repair range: `f6c841d57..9a8f9f7ee`. `HEAD` matched candidate at review; the worktree had no source changes.
- Read `AGENTS.md` §§5, 11, 16; `architecture/agent-rules.md`; `architecture/references/languages/maintainer-style.md`; the approved specification, original TASK-003 r4, prior verdict and R1 remediation. `.codegraph/` is absent. Inspected the cumulative file/diff map and used the repair diff to locate changed owners. Verification evidence is the task's recorded focused lanes; I did not rerun tests.
- The previous maintainer review covered the cumulative contracts, server, Bifrost, SDK, generated declaration, journey, documentation, and tooling changes. The present pass rechecked those paths against the repair locations and inspected the materially changed functions, surrounding owner code, their production callers, and tests. No new public SDK type or generated declaration was added by R1.

| Changed surface and caller/test path | Maintainer assessment |
|---|---|
| Support-desk `deploy` in Rust, Python, and TypeScript; Prompt YAML; each SDK's support-desk journey | Each example checks the same exact gateway provider/model pair that its fixed Prompt routes to, then names the pair and configuration endpoint on refusal. The three journeys exercise a same-name, other-provider deployment before the correct deployment. The new checks remain next to the existing registration/capture work, with no extra abstraction. |
| `OracleAdmission::shutdown` and new `oracle_memory_reserved`; `BifrostResourceGovernor::snapshot`; admission unit test; server drain caller | The admission owner still coordinates both wakeups and its own deadline. The helper names the governor attribution used for the shutdown report. The test holds Forge bytes while an Oracle child reservation drains, so the owner and proof are easy to follow. |
| `ForgeWorker::run`, `run_event_loop`, `drain_recoverable_work`, and `ForgeLoopQuiescence`; `spawn_forge_worker` supervisor caller; `ForgeTasks::reclaim_attempts`; new unit and existing Postgres/journey tests | The quiescence flag is retained on the cloneable worker that the production supervisor clones for each restart. `run_event_loop` joins spawned work on a normal return; `run` marks this state and cancels the stop token on unwind. The reclaim predicate stays at worker startup, and SQL remains its durable owner. The new type has a single cohesive invariant and explanatory rustdoc. The focused unit test proves only the flag's panic transition, so the recorded lack of a full running-plan panic journey remains a verification limit, not a separate maintenance defect. |
| Late-Scribe query journey and `WyrdTestCluster` delayed-start caller | The existing cluster fixture now holds Scribe unbooted, stages a real audit, starts Scribe, drains Oracle, and reads the row; no new fixture or lifecycle owner was introduced. |
| Role rerun shell script | The container `psql` rerun now uses `ON_ERROR_STOP`, matching adjacent invocations and closing prior `MAINT-001`. |
| Card UID design passages and rustdoc on the Bifrost fixture and Rust example `main` | The corrected wording distinguishes observation correlation from legitimate CardRef registry use; both changed Rust functions now have substantive item documentation and `main` names its error conditions. |

## Material findings

None.

## Non-blocking calibration

- `oracle/admission.rs` puts `use datafusion::execution::memory_pool::MemoryConsumer` inside the new test function. `architecture/agent-rules.md` requires imports at the top of their module; moving it to the tests module import block would keep the dependency manifest visible. This is an explicit placement rule but has no behavioral or public-contract consequence, so it does not change this review's result.
- The new Python `PROVIDER` constant sits between `REQUESTS = 100` and the old standalone string explaining `REQUESTS`; that string should be adjacent to its subject if edited again. It does not change the example's typed contract or behavior.

**Overall: PASS** — prior maintainer finding closed; no material maintainability finding remains. The running-plan panic path has no full end-to-end proof in the supplied evidence.
