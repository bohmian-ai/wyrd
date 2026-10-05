# Maintainer Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `09e4b82c2a4cab3ea27e0889d3acf3ea22c2b596`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`,
  Revision 11, including `REQ-053`
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Round-four remediation:
  `changes/active/skald-workflow-runtime/review/TASK-001-r4/TASK-001-R3-close-round-four-review-gaps.md`

The candidate was exactly `09e4b82c2a4cab3ea27e0889d3acf3ea22c2b596`
before and after source inspection. No `.codegraph/` directory exists, so
navigation used the cumulative Git diff, repository search, owning modules,
callers, tests, manifests, and generated declarations.

## Authority Coverage

This pass applied `AGENTS.md`, `architecture/agent-rules.md`,
`architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`,
`architecture/references/languages/spec-driven-development.md`,
`architecture/references/languages/maintainer-style.md`, the approved Revision
11 specification, the original task, the round-four verdict and validated
ledger, and the human-authorized round-four remediation task. Prior findings
`FIND-TASK-001-27`, `FIND-TASK-001-28`, and `FIND-TASK-001-29` were treated as
closure hypotheses, not conclusions.

## Changed-Surface Coverage

| Surface | Symbols, callers, tests, and declarations inspected | Assessment |
|---|---|---|
| Workflow contracts and schemas | `WorkflowSpec`, bindings, routes, run/status/result/error DTOs, validation, generated JSON schemas, and Rust/Python consumers across the cumulative diff | Public names, wire shapes, generated declarations, and ownership remain aligned: pure contracts stay in `wyrd-spec`, execution stays in Skald, and Python remains a thin SDK projection. |
| Workflow planning and execution | `ExecutionPlan`, `WorkflowExecutor`, `StepTask`, `AttemptOutcome`, `RunLedger`, retry/deadline/cancellation ordering, output projection and budgeting, and focused Workflow tests | Stateful workflows remain on cohesive owners with narrow deterministic helpers. No new utility owner or forwarding layer was introduced by this remediation. |
| Agent orchestration and telemetry | `Agent::{run,run_with,run_prompt,run_loop}`, `RunLoopInputs`, `chat_span`, `request_model`, callback replacement, provider dispatch, journal consumers, and `agent_timeout` telemetry capture | `chat_span` now derives the effective model from the post-callback request and keeps the resolved Prompt model only as the documented fallback for model-less Gemini/Vertex wire shapes. The helper and test names expose the rule directly; payload exclusion and generated/public declarations are unchanged. |
| Provider and external-gateway path | `ExternalGatewayClient`, endpoint policy, provider errors, route construction, and their Workflow callers/tests | Existing owner and dependency direction remain intact. The final remediation adds no transport abstraction, dependency, or configuration surface. |
| Rust/Python public authoring | Rust `Workflow`/builder surfaces, SDK-owned PyO3 wrappers, public exports, stubs, examples, and unit coverage | Names, argument/result types, docs, and declarations remain discoverable and aligned. No duplicate durable behavior was found. |
| Observer deletion | Removed crate, hooks, Python exports/stubs, tests, examples, checks, and the replacement Agent/Workflow tracing owners | `REQ-053` remains closed without aliases or a second telemetry API. |
| Skald architecture page | Overview, crate map, dependency prose and diagram compared with every live `crates/skald/*/Cargo.toml`, crate-root ownership docs, and doctrine | The page now consistently records the narrow `skald-workflow -> wyrd-spec` contract edge, the other shared-foundation consumers, and the prohibition on Wyrd server/Vala dependencies. |
| Test-server lifecycle | `WyrdTestServer::{shutdown,settle_lifecycle,cancel_and_join_for_test,terminate_abruptly_for_test,await_terminal_failure_for_test,shutdown_and_inspect,rollback_bound_startup,bind}`, `Drop`, `bifrost_settled`, all call sites, `Bifrost::{shutdown,drain_selected_owners,abort}`, Forge supervision marking in `BoundServer::run`, storage close/abort, and teardown tests | Bound serve tasks are retained, and timeout abort is joined. The in-process path correctly cannot claim a graceful Forge drain because only `BoundServer::run` joins and marks Forge supervision. However, the common lifecycle helper can claim settlement after cancelling the abort future; see `MNT-R5-001`. |
| Verification artifacts | Exact focused selectors, broader recorded lanes, remediation evidence, and cumulative `git diff --check` | The telemetry and bound-task tests directly cover their changes. The in-process teardown test does not exercise its stated live-work boundary and therefore misses the remaining lifecycle defect. |

## Prior-Finding Closure

| Prior finding | Closure evidence | Result |
|---|---|---|
| `FIND-TASK-001-27` | `docs/architecture/skald.md:3-6,42-62` now agrees with the manifests and doctrine: Skald may consume shared foundation and `wyrd-spec`, while it does not depend on `wyrd-server` or Vala. The diagram contains the Workflow contract edge. | CLOSED |
| `FIND-TASK-001-28` | A bound serve task that exceeds its drain budget is aborted and joined. The in-process path reaches the existing Bifrost abort owner, but the helper can cancel that abort and mark it settled before completion. The claimed implicit-drop ordering is therefore not closed. | OPEN (`MNT-R5-001`) |
| `FIND-TASK-001-29` | `chat_span` calls the existing `request_model` on the effective post-callback request and uses the Prompt model only when the request has no model. `agent_run_chat_span_records_callback_replaced_model` proves dispatch and span agreement while retaining provider, operation, and privacy assertions. | CLOSED |

## Material Findings

### MNT-R5-001 — Teardown records Bifrost as settled before the abort owner finishes

- Changed location: `crates/wyrd/wyrd-testing/src/server.rs:780-849,3675-3708,5479-5505`.
- Governing principle: the maintainer guide requires owner state and names to
  expose the real invariant, and tests to prove the caller-visible outcome.
  The round-four remediation requires explicit and implicit teardown to finish
  or abort Bifrost-owned work before fixture release.
- Evidence: `settle_lifecycle` sets `bifrost_settled = true` before attempting
  either operation. It wraps `Bifrost::shutdown` in a deadline even though that
  method already owns graceful deadline handling and deliberately falls back to
  the unbounded `Bifrost::abort`. If that outer timeout cancels the shutdown
  during its abort, the helper retries `Bifrost::abort` under the same already
  expired deadline, warns, and returns with `bifrost_settled == true`.
  Active-runtime `Drop` selects a zero budget, so this is its normal shape.
  `BifrostStorage::abort` explicitly awaits every aborted loader and governed
  request without a second bound (`crates/vala/vala-bifrost-redux/src/storage/mod.rs:1041-1058`);
  cancelling that future defeats the completion guarantee. Subsequent Drop
  sees the false settled flag and releases the fixture without retrying.
- Test gap: `dropping_an_in_process_server_settles_bifrost_before_fixture_release`
  starts an idle server and checks only that storage changes from open to
  settled. It installs no existing `StorageOperationBarrier`, admits no live
  operation, and therefore does not prove its rustdoc claim that role work is
  complete or aborted before fixture release. It also runs only off a Tokio
  runtime, leaving the zero-budget Drop path untested.
- Concrete maintenance cost: the field, rustdoc, warning, and test all tell a
  maintainer that lifecycle ownership has settled when a retained task may
  still be using the fixture. Repeated teardown becomes a no-op based on that
  false state, making the one owner path unsafe to reuse or reason about.
- Smallest testable correction: keep the graceful deadline on
  `Bifrost::shutdown`, but once it fails or expires, await the existing
  `Bifrost::abort` to completion without another timeout, exactly as that
  owner's contract requires. Set `bifrost_settled` only after either a bound
  production drain report or the completed abort. Preserve the fact that an
  in-process server with Forge uses abort rather than inventing a second Forge
  supervisor or marking supervision drained. Extend the inline teardown test
  with the existing storage operation barrier so live work is observably
  cancelled and settled before fixture release, including the active-runtime
  implicit-drop path.
- Focused proof: before the correction, retain a governed storage operation at
  the existing barrier and show Drop returns while inspection is still
  `Closing` or the fixture-release sentinel wins; after the correction, the
  operation is cancelled, storage is `Closed`, and only then may the fixture
  release. Retain the bound stalled-serve abort-and-join test.

## Residual-Risk Assessment

The reported fact is correct: an in-process `WyrdTestServer` containing Forge
cannot produce a graceful Forge drain report because only `BoundServer::run`
owns and joins Forge supervision before calling `mark_supervision_drained`.
That is not independently a finding: the approved remediation explicitly
allows the in-process owner to complete **or abort** existing work, and marking
Forge drained from the harness would counterfeit a production lifecycle fact.
The defect is instead that the abort fallback is itself time-limited and its
completion is recorded before it completes.

## Verification Evidence and Limits

- The remediation record reports both new teardown tests failing before the
  implementation and passing afterward, plus the callback-model test and
  Gemini/Vertex fallback test passing.
- It reports green `mise run fmt`, `mise run lints`, `mise run test:skald`,
  `mise run test:wyrd`, `mise run docs:check`, and
  `mise run check:client-tier` lanes.
- This review independently confirmed that
  `git diff --check a51af030b6039eea4b2914f3ebf2c31925d08721..09e4b82c2a4cab3ea27e0889d3acf3ea22c2b596`
  exits zero. It did not rerun the recorded compilation and test suites.
- The green idle-storage teardown test is not credible proof of live-work
  completion, and no recorded test exercises implicit Drop from an active Tokio
  runtime. This limit is part of `MNT-R5-001`, not a reason to accept it.

## Overall Result

**FAIL**

`FIND-TASK-001-27` and `FIND-TASK-001-29` are closed. The lifecycle owner and
test remain readable in shape, but `FIND-TASK-001-28` is not closed because the
common teardown path can cancel its abort, record false settlement, and release
the fixture while owned work remains live.
