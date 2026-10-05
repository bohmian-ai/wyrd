# TASK-001 invariant review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `09e4b82c2a4cab3ea27e0889d3acf3ea22c2b596`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 11
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Remediation task: `changes/active/skald-workflow-runtime/review/TASK-001-r4/TASK-001-R3-close-round-four-review-gaps.md`

The candidate remained at the stated commit throughout this review. The complete
base-to-candidate range was reviewed, with the round-five remediation diff
traced back through the cumulative Workflow, Agent, provider, and test-server
owners. Prior findings `FIND-TASK-001-27`, `FIND-TASK-001-28`, and
`FIND-TASK-001-29` were treated as hypotheses rather than presumed closed.

## Producer-to-sink and lifecycle traces

- `WorkflowSpec` and resolved Agent/Prompt Cards still produce one immutable
  `ExecutionPlan`; `WorkflowExecutor` schedules it, and `RunLedger` owns the
  portable `WorkflowRun` transitions and terminal snapshot. The remediation
  does not alter those producers, consumers, or retry/deadline ordering.
- A `before_model` callback produces the effective `ProviderRequest` consumed
  by dispatch. `chat_span` now reads the model from that same effective request
  through the existing `request_model` helper, using the resolved Prompt model
  only for wire shapes that do not carry one. The provider and telemetry sinks
  therefore agree after request replacement.
- `WyrdTestServer::settle_lifecycle` is the shared producer of teardown state
  for explicit shutdown, startup rollback, abrupt termination, terminal
  failure, and implicit `Drop`. A completed bound `BoundServer::run` is the
  only path that can truthfully set Forge supervision drained, because it owns
  and joins the Forge supervisor set before calling `Bifrost::shutdown`.
- An in-process server does not run `BoundServer::run`, so its Forge
  `supervision_drained` flag remains false. `Bifrost::shutdown` consequently
  rejects a graceful drain and enters its awaited abort path. That is an
  acceptable and expected fallback under the remediation task; marking
  unobserved Forge supervision as drained would weaken the production
  invariant.
- The harness nevertheless does not preserve that abort-to-completion
  invariant on every reachable path. It sets `bifrost_settled = true` before
  either lifecycle call completes and wraps the fallback `Bifrost::abort` in
  the same deadline used by graceful shutdown. With `budget == ZERO`, or after
  graceful shutdown consumes the budget, Tokio polls `abort` once and then
  returns `Elapsed` if it is pending. `Bifrost::abort` cancels the role owners
  but must await `BifrostStorage::abort`; dropping that future can therefore
  leave governed storage work live while `PgFixture` force-drops the database.
  The active-runtime `Drop` path always selects this zero-budget branch.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001–015: one declarative Agent-only DAG, explicit bindings, pure/resolved validation, deterministic namespaced data flow, and one execution plan | `wyrd-spec/src/card/workflow.rs`; `skald-workflow/src/plan.rs`, `workflow_surface.rs`, `workflow.rs`, `run.rs` | Cumulative scenario evidence and prior independently reviewed `test:skald`, `test:shared`, codegen, and boundary lanes | PASS |
| REQ-016–023/043/045: fixed deadlines, retry classification and precedence, bounded owned tasks, complete terminal states, and deterministic outputs | `skald-workflow/src/workflow.rs`, `attempt.rs`, `run.rs` | Cumulative focused deadline, panic, lifecycle, and result tests; no remediation change to these owners | PASS |
| REQ-024/047/051 and AC-019/024/026: aligned Rust/Python local authoring and portable result projection | `skald-workflow/src/python.rs`, `workflow_surface.rs`; Python SDK projections in the cumulative diff | Recorded Python unit/typecheck, retained-feature, codegen, and lint evidence | PASS |
| REQ-035–040/042/049/052, INV-012/017, AC-011A/016/023: route isolation, native request preservation, screened ExtGateway transport, and no credential reflection | `skald-workflow/src/route.rs`, `attempt.rs`; `skald-providers/src/clients/external.rs`; Agent request/session owners | Cumulative focused route and external-gateway evidence; no remediation change to these owners | PASS |
| REQ-053: Observer deletion and payload-free Agent/Workflow tracing with the effective semantic provider/model | `skald-agent/src/loop_runtime.rs:818-835,906-919`; Observer crate and public projections remain deleted | `agent_run_chat_span_records_callback_replaced_model` rerun: 1 selected, 1 passed; Gemini/Vertex fallback remains covered by the adjacent existing test | PASS |
| `FIND-TASK-001-27`: permanent Skald architecture describes the live narrow contract edges without admitting server/Vala dependencies | `docs/architecture/skald.md:3-6,43-67`; manifests for `skald-workflow`, `skald-agent`, `skald-tool`, `skald-prompt`, and `skald-providers` | Candidate records `docs:check`; source/manifests re-read | PASS — CLOSED |
| `FIND-TASK-001-28`: every serve task and Bifrost/storage owner completes or is explicitly aborted before fixture release in explicit and implicit teardown | `wyrd-testing/src/server.rs:780-848,937-948,961-992,3675-3708`; `wyrd-server/src/state.rs:1990-2089`; `wyrd-server/src/app/server.rs:780-782,862-884` | Two remediation tests rerun: 2 selected, 2 passed, but the in-process test is synchronous/off-runtime and does not execute the zero-budget async `Drop` branch; no test holds abort pending through deadline expiry | **FAIL — `INV-R5-001`** |
| `FIND-TASK-001-29`: callback-replaced OpenAI request dispatch and span both use the replacement model; model-less Google requests keep the resolved fallback | `skald-agent/src/loop_runtime.rs:818-835,906-919`; `skald-agent/tests/agent_timeout.rs:266-320` | Replacement-model test rerun green and asserts both dispatch and span identity plus payload exclusion | PASS — CLOSED |
| No second engine, compatibility alias, remote Python/TS/MCP surface, Observer replacement, credential administration, production lifecycle API, or new dependency | Complete cumulative diff and remediation range | Source and manifest inspection; `git diff --check` exits zero | PASS |

## Prior-finding closure

| Prior finding | Result | Closure evidence |
|---|---|---|
| `FIND-TASK-001-27` | CLOSED | The overview, dependency prose, and diagram now agree with the live manifests: Skald may consume shared foundational Wyrd contracts/infrastructure while remaining independent of `wyrd-server` and Vala. |
| `FIND-TASK-001-28` | OPEN | Bound serve-task timeout now aborts and joins the handle, and the off-runtime in-process path settles storage. The shared fallback still abandons a pending `Bifrost::abort` at a zero or expired deadline and records settlement before completion, so active-runtime implicit Drop and shutdown-timeout paths can release the fixture too early. |
| `FIND-TASK-001-29` | CLOSED | `chat_span` and the adjacent journal path now derive model identity from the dispatched post-callback request, with the approved Prompt-model fallback only for model-less wire shapes. |

Earlier `FIND-TASK-001-1` through `FIND-TASK-001-26` were checked against the
cumulative candidate and the narrow remediation range; no changed producer or
sibling consumer reopens them.

## Proposed findings

### INV-R5-001 — The teardown fallback can declare Bifrost settled before abort completes

- Classification: **INCORRECT**.
- Violated obligation: prior `FIND-TASK-001-28` and the remediation acceptance
  criterion require every Bifrost-owned database task and storage operation to
  finish or be explicitly aborted before runtime owners and `PgFixture` are
  released.
- Exact location: `crates/wyrd/wyrd-testing/src/server.rs:831-846,937-948,`
  `986-992,3675-3708`; lifecycle contract at
  `crates/wyrd/wyrd-server/src/state.rs:1990-2001,2062-2089`.
- Evidence: `settle_lifecycle` sets `bifrost_settled` at line 832 before either
  operation completes. Its fallback at lines 840-843 uses the graceful
  shutdown deadline. `Drop` supplies `Duration::ZERO` whenever called from an
  active Tokio runtime, and two explicit abrupt paths also supply zero. Tokio
  polls the abort once, but an expired timeout cancels it as soon as it returns
  `Pending`. The abort contract intentionally awaits storage settlement; it is
  not safe to cancel. A graceful shutdown that consumes its deadline reaches
  the same expired fallback. The warning at line 845 does not prevent field
  destruction.
- Reachability: ordinary async Wyrd tests create in-process servers and rely on
  implicit drop. Those servers never run `BoundServer::run`, so the Forge
  supervision flag remains false and graceful `Bifrost::shutdown` correctly
  selects abort. The new synchronous teardown test runs outside Tokio and
  therefore uses the nonzero budget; it cannot exercise the production-shaped
  active-runtime branch. The same cancellation is directly reachable through
  `terminate_abruptly_for_test` and `await_terminal_failure_for_test`.
- Observable consequence: fixture destruction can race a still-settling
  storage request or role task, preserving the forced-database-drop failure
  and Oracle self-fence/process-abort class that the remediation was required
  to remove. Later lifecycle calls are skipped because the boolean already
  claims settlement.
- Required testable correction: keep the correction in
  `WyrdTestServer::settle_lifecycle` and reuse `Bifrost::shutdown`/`abort`.
  Record `bifrost_settled` only after graceful shutdown succeeds or the
  fallback abort returns. A zero budget must skip graceful drain and fully
  await `Bifrost::abort`; a timed-out graceful drain must likewise await that
  abort without reusing the expired deadline. Do not mark in-process Forge
  supervision drained, add another lifecycle owner, weaken storage settlement,
  or add a sleep.
- Focused closure proof: add a deterministic active-Tokio implicit-drop case
  that keeps a real governed Bifrost storage operation pending long enough for
  abort to yield, then proves storage is settled before `drop(server)` returns
  and before fixture-drop observation. Add or extend a timeout-path case to
  prove an exhausted graceful deadline still completes the abort fallback.
  Retain the bound serve-task abort-and-join test and rerun the exact focused
  tests plus `mise run test:wyrd`.

## Verification notes

- Reran
  `mise exec -- cargo nextest run --locked -p skald-agent --test agent_timeout -E 'test(=agent_run_chat_span_records_callback_replaced_model)'`:
  1 selected, 1 passed.
- Reran the two remediation teardown tests through the repository-managed
  Postgres wrapper: 2 selected, 2 passed. Their assertions do not cover the
  zero/expired-budget abort path identified above.
- `git diff --check a51af030b6039eea4b2914f3ebf2c31925d08721..09e4b82c2a4cab3ea27e0889d3acf3ea22c2b596`
  exited zero.
- Relied on the candidate's recorded broad evidence for format, lints,
  `test:skald`, `test:wyrd`, docs, and client-tier checks. A green broad lane
  cannot prove teardown ordering that its focused test does not exercise.

## Overall result

**FAIL** — `FIND-TASK-001-27` and `FIND-TASK-001-29` are closed, but prior
`FIND-TASK-001-28` remains open through `INV-R5-001`. The residual fact that an
in-process server lacks a `BoundServer::run` Forge-drained signal is not itself
a defect; the defect is cancelling its required abort fallback before storage
settlement and then recording the owner as settled.
