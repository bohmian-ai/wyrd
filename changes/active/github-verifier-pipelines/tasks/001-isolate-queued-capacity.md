---
id: TASK-001
status: proposed
approved_spec_revision: 5
requirements: [REQ-015, REQ-016]
acceptance: [AC-010]
invariants: [INV-001, INV-005, INV-007]
depends_on: []
---

# Isolate queued capacity

Carry **approved-runtime-and-github.md → Direct and queued capacity / Direct
execution eligibility** into the existing engine composition. Direct-eligible
verification must still execute while the queue fills its own execution,
workspace, Python and model-call allowances. This is reserved admission, with no
borrowing; it does not promise isolation of shared CPU, database or providers.

## Existing flow and selected change

Inputs are existing direct `verify` and queued `submit`/runner requests, their
accepted Caller/Verifier/evidence, and operator configuration. Today
`server/verification/mod.rs` composes one `HostedBlocks`; `hosted/blocks.rs`
constructs its Sandbox and Workspaces and builds the common engine, while
`runner.rs` owns queue execution slots. Splitting that semaphore alone leaves
workspace, Python and gateway model admission shared.

Extend that composition to select independent direct and queued resource
ownership at all four admissions, keeping `HostedBlocks::engine`, the hosted
block executors and the normal gateway/tenant accounting path. Route each call
through its accepted execution mode, including queued continuations. Expose the
isolated limits and original execution ceiling through server configuration's
existing typed/environment validation, not a new scheduler or process service.
Keep limits at their existing owners; model requests must use the selected lane
before entering the existing provider path.

Establish direct eligibility validation before any engine step. Tasks 005, 007
and 008 attach their new extraction/deferred/approval forms to this validation
when those contracts arrive; do not create those actions here or silently enqueue
a direct request. Ordinary direct graphs retain their current behavior.

Amend **wyrd-design.md → Verifier / Agents, tools, and workspaces** for isolated
admission and queued-only features, and relevant operator operations guidance
for limits/deadline ceilings. Preserve the one engine and existing authority,
evidence, billing, cancellation and platform behavior.

## Output and consumer

Direct and queued engine composition now select their own resource pools under
operator limits. The existing runner and tasks 005–008 consume queued resources;
the synchronous service consumes direct resources. No new user operation or
queue is introduced. Inspection/configuration documentation must explain what
is reserved and how intended human approval periods fit the original ceiling.

## Proof and delivery

**AC-010:** Against WyrdTestServer and repository Postgres, hold queued
execution, workspace, Python and model admission at configured capacity, then
run a direct-eligible SDK Verifier that uses each corresponding resource and
observe completion through the SDK. Exercise each admission distinctly so an
unused shared pool cannot hide a gap. Queued work must wait for its own capacity;
releasing it allows progress. Confirm existing tenant/subject accounting remains
on the ordinary gateway path. The later introducing tasks prove each queued-only
action refuses direct execution before its first step.

Owner/tier: resource and configuration branch tests in their owning Rust modules;
real resource composition/journey proof in `wyrd-testing`'s existing `server`
target (`verification_runtime` owner). Use `mise run test:bifrost:journey:server`
for the managed Postgres/server runner, and the affected `test:wyrd`/`test:skald`
owner slices for hosted and model admission. New test names/focused commands are
recorded after implementation, before RED; none are invented here.

Run `mise run fmt`, `mise run lints`, `mise run check:deps` and
`git diff --check`. No synthetic host load, separate engine, platform-specific
resource enforcement or completion-time SLA is part of this task.

## Obligations

| ID | Source (task line) | Observation | Planned assertion |
|---|---|---|---|
| O1 | AC-010 "hold queued execution … run a direct-eligible SDK Verifier" | With every queued execution slot held, a direct SDK `verify` completes | SDK journey: queued slot held by a held queued run; direct verify result `passed` |
| O2 | AC-010 "workspace" | With every queued workspace held, a direct SDK Verifier whose Agent uses a workspace completes | SDK journey: queued workspaces held; direct workspace Agent passes |
| O3 | AC-010 "Python" | With every queued Python admission held, a direct SDK `code` Verifier completes | SDK journey: queued sandbox held; direct code Verifier passes |
| O4 | AC-010 "model admission" | With every queued model-call admission held, a direct SDK Agent Verifier completes | SDK journey: queued model calls held; direct Agent passes |
| O5 | AC-010 "Queued work must wait for its own capacity; releasing it allows progress" (execution) | A queued run does not start while the queued slot is held; completes after release | SDK journey: second queued run not settled while held, completed after release |
| O6 | same (workspace) | A queued workspace run waits while queued workspaces are held; completes after release | SDK journey: deferral observed, then completed |
| O7 | same (Python) | A queued code run waits while queued Python is held; completes after release | SDK journey: deferral observed, then completed |
| O8 | same (model) | A queued Agent run waits for queued model admission; completes after release | SDK journey: wait observed, then completed |
| O9 | "no borrowing" (design/spec REQ-015) | Direct work never takes queued capacity: a full direct lane refuses direct work although queued capacity is free | SDK journey: direct workspace held → direct verify `WYRD_SERVER_503_SERVICE_UNAVAILABLE` |
| O10 | "Route each call through its accepted execution mode, including queued continuations" | Direct calls use direct resources; queued attempts, including re-claimed deferred continuations, use queued resources | O2–O8 assertions (deferred queued run resumes on queued lane after release) |
| O11 | "Expose the isolated limits and original execution ceiling through server configuration's existing typed/environment validation" | Every lane limit, direct/queued execution ceiling and the queued execution deadline parse from env; bad/inconsistent values refuse startup | `config::tests::verification_limits_env_applies_and_refuses` |
| O12 | "Keep limits at their existing owners" / defaults | Defaults keep the original combined Python/workspace capacity and the 64-slot five-minute queued ceiling | unit tests in `verification::hosted` and `config` |
| O13 | "Establish direct eligibility validation before any engine step" | A queued-only (Window) Verifier is refused by direct `verify` before any step runs, through one named check | existing SDK journey `verify_in_real_time` window refusal |
| O14 | "Confirm existing tenant/subject accounting remains on the ordinary gateway path" | A direct Agent's model call admitted by its lane still passes through the gateway and is attributed to the direct subject | SDK journey: gateway capture/accounting row carries subject |
| O15 | "Amend wyrd-design.md … and relevant operator operations guidance" | Design and operator docs describe isolated admission, queued-only features, limits and the deadline ceiling for approvals | `mise run docs:check` |
| O16 | "Run fmt, lints, check:deps, git diff --check" | All pass | command output |
| O17 | "Preserve the one engine" | Both modes build their engine through `HostedBlocks::engine` | code citation + O1–O8 |
| O18 | "Preserve the one engine and existing … cancellation" (checker finding) | A model call waiting for its lane is cancelled with its execution and leaks no permit | SDK journey: direct model calls held → direct verify `WYRD_VERIFICATION_504_EXECUTION_TIMED_OUT`; after release a direct Agent passes |
| O19 | "Ordinary direct graphs retain their current behavior" (checker finding) | Existing direct and queued Agent/workspace journeys pass unchanged | existing `hosted_agent_tools` and `verify_in_real_time` journeys |

## Evidence

Journey selector: `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai -P journey --run-ignored=all -E 'test(=isolated_capacity::direct_verification_runs_while_queued_capacity_is_held)'"` (below: **J**). Unit selector: `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=config::tests::verification_limits_env_applies_and_refuses) | test(=verification::hosted::tests::lanes_split_the_memory_ceilings) | test(=verification::tests::production_defaults_are_the_specified_ceilings)'` (below: **U**). `J` file: `sdks/wyrd-sdk-rust/tests/integration/isolated_capacity.rs`.

| Obligation | Implementation evidence | Assertion | Verification evidence | Result |
|---|---|---|---|---|
| O1: direct completes with every queued execution slot held | `RuntimeLimits.execution_slots` configurable (`config.rs` `apply_limits_env`); direct ceiling `HostedBlocks::admit_direct` (`blocks.rs`), `service.rs:508` | `J:290` (the queued model run, holding the only slot, is waiting) then `J:301` (direct `answer-check` passed) | J PASS | PASS |
| O2: direct workspace Agent completes with queued workspaces held | per-lane `Workspaces` (`blocks.rs` `Lane`) | `J:328` hold, `J:333` | J PASS | PASS |
| O3: direct code Verifier completes with queued Python held | per-lane `Sandbox` | `J:315` hold, `J:320` | J PASS | PASS |
| O4: direct Agent completes with queued model calls held | `ServerWyrdGatewayCaller::admitted` (`caller.rs`), `blocks.rs:286` | `J:288` hold, `J:296` | J PASS | PASS |
| O5: queued run waits for the queued execution slot, completes after release | runner execution slots | `J:302` (status `Pending` after two direct runs), `J:313` after release | J PASS | PASS |
| O6: queued workspace run deferred while held, completes after release | `Workspaces::open` Busy → runner `Transition::Defer` | `J:335` (deferral of that execution id), `J:338` | J PASS | PASS |
| O7: queued code run deferred while held, completes after release | `Sandbox` Busy → `Transition::Defer` | `J:322`, `J:325` | J PASS | PASS |
| O8: queued Agent waits for queued model admission, completes after release | lane wait in `caller.rs` | `J:290` (`MODEL_WAIT` with `queued`), `J:312` | J PASS | PASS |
| O9: no borrowing, so a full direct lane refuses or waits even though queued capacity is free | `blocks.rs` `lane()`; `service.rs:508`; `caller.rs` lane wait | `J:374`–`J:381` (direct workspace and Python held → `WYRD_SERVER_503_SERVICE_UNAVAILABLE`; the same queued work completes); `J:384`–`J:391` (direct model calls held: the direct call logs `MODEL_WAIT` `direct` with the queued lane free, and passes only after release); `J:415`–`J:420` (direct execution ceiling → 503) | J PASS | PASS |
| O10: each call routes by accepted mode, including queued continuations | `Execution.mode` set `Direct` in `service.rs` and `Queued` for every runner claim and re-claim (`runner.rs:821`); `HostedBlocks::engine` selects `lane(execution.mode)` | the deferred queued Python and workspace runs resume and pass while the same direct admission is held (`J:323`–`J:325`, `J:336`–`J:338`); queued work completes with the direct lane held (`J:381`); direct runs complete with the queued lane held (O1–O4) | J PASS | PASS |
| O11: lane limits, direct/queued ceilings and the deadline parse from env; bad or inconsistent values refuse | `config.rs` `apply_limits_env` | `config.rs:4339` test (`4387`–`4388` runtime; bad and inconsistent cases follow) | U PASS | PASS |
| O12: defaults keep the combined capacity and the 64-slot, five-minute queued ceiling | `HostedLimits::default`, `RuntimeLimits::default` | `hosted/mod.rs:221` test (totals 5/16/16/2); `verification/mod.rs:631` test (64, 5 min) | U PASS | PASS |
| O13: a queued-only (Window) Verifier is refused by direct `verify` through one named check before any step | `service.rs:159` `direct_eligibility`, called in `resolve` (`service.rs:962`) before the engine is built | `service.rs` `tests::direct_eligibility_refuses_only_queued_only_verifiers` (window → `VerificationInputUnsupported`, ordinary → `Ok`); end to end `verify_in_real_time.rs:987`, `:993` | `components::verification::service::tests::direct_eligibility_refuses_only_queued_only_verifiers` PASS; `verify_in_real_time::window_verifier_records_samples_and_refuses_verify` PASS | PASS |
| O14: lane-admitted model calls stay on the ordinary gateway path, attributed to the subject | the caller holds the lane permit, then calls `GatewayInvocation::run` unchanged | `J:348`–`J:353` (six `bifrost.gateway.calls` rows read under the tenant, each `subject_card_uid` = `capacity-desk`) | J PASS | PASS |
| O15: design and operator docs updated | `architecture/wyrd-design.md` Agents, tools, and workspaces; `operate/configure-verification-runtime.md` "Split direct and queued capacity"; `reference/verification-runtime-limits.md` "Direct and queued capacity" (no borrowing, shared resources, the deadline counted from first claim, approval periods inside it); regenerated `docs/public/llms*.txt` | `mise run docs:check` (generated-drift check, docs tests, commands, links, build, a11y) | `mise run docs:check` exit 0 | PASS |
| O16: fmt, lints, check:deps, `git diff --check` | — | command exit codes | `mise run fmt` clean, `mise run lints` 0, `mise run check:deps` 0, `git diff --check` 0 | PASS |
| O17: one engine | both modes build through `HostedBlocks::engine` (`blocks.rs:241`); no second engine type | J direct and queued runs (O1–O8) | J PASS | PASS |
| O18: a lane wait is cancelled with its execution | the `cancellation.cancelled()` arm of the lane wait in `caller.rs` | `J:393`–`J:398` (direct model calls held → `WYRD_VERIFICATION_504_EXECUTION_TIMED_OUT` at the 10 s direct deadline), then a direct Agent passes after release (no leaked permit) | J PASS | PASS |
| O19: ordinary direct graphs keep their behavior | unchanged engine and executors | existing journeys: `hosted_agent_tools` (7) and `verify_in_real_time::window_verifier_records_samples_and_refuses_verify` | all PASS (see Other lanes) | PASS |

**RED.** Production code was written before the journey, which is a process exception. RED was taken afterwards by mutation: `blocks.rs` `lane()` was made to return the queued lane for `Direct`. J then failed at `J:288`: `VerificationExecutionTimedOut … "deadline_seconds": 60`, because the direct Agent waited on the held queued model-call permit. Restoring the mapping turns J green.

**Other lanes.** `wyrd-server` sandbox_worker, pg_hosted_artifacts, pg_durable_verifier_work, hosted workspace/tools pg_tests, `components::gateway`, `verification::hosted`, `config` ran 207 tests: 207 passed. Rust SDK `hosted_agent_tools`, `verifier_orchestration` and the window test ran 17: 16 passed. The failure is `verifier_orchestration::speculative_recovery_cannot_resurrect_content`. It failed twice, at different assertions (`verifier_orchestration.rs:2114`, `:2159`).

Diagnosis, confirmed by a read-only diagnostician:
- **Symptom:** the speculation disposition is either absent, or recorded with no `extra` invocation, before the takeover.
- **Evidence:** the trace shows no lane wait, deferral or Busy. The test's server keeps default runtime limits and uses at most about 2 of its 48 queued model calls and 12 workspaces.
- **Cause:** commit `f04384dbf` (an ancestor of this task's base) removed the `COMPARE:` reply delay for `LATE-DECISION` prompts. This test's ordering depended on that delay. `compare` now races `bootstrap`/`extra`. The engine correctly records a "known" decision, or cancels a not-yet-begun `extra`.
- **Fix site:** that test. Order the race with `HeldUpstream` holds and invocation milestones, as its sibling `speculative_disposition_controls_consumer_admission` already does.

This is independent of this diff and outside its write set, so it is left unchanged and reported.

**Mechanical deviations.**
- The AC-010 journey lives in the Rust SDK integration target (`isolated_capacity`), not in `wyrd-testing`'s `server` target. That target composes clusters through `wyrd_client` only and has no SDK Card bundle or `verify` harness. The task requires an SDK Verifier observed through the SDK.
- No Python or TypeScript mirror was added: no new SDK surface ships.
- The journey observes server events through the new `LogCapture::install_global`, because the in-process server runs on the shared runtime's threads.
- The env names `WYRD_VERIFICATION_MAX_WORKSPACES`, `…_MONTY_MAX_WORKERS` and `…_MONTY_MAX_WAITERS` are replaced by per-lane `WYRD_VERIFICATION_{DIRECT,QUEUED}_{WORKSPACES,PYTHON_WORKERS,PYTHON_WAITERS,MODEL_CALLS}`. `MONTY_MIN_WORKERS` now prewarms each lane, with default 1 per lane (2 in total, as before).

**Non-goals.** No separate engine, scheduler, process service, synthetic load, platform-specific enforcement, or queued-only action was added.
