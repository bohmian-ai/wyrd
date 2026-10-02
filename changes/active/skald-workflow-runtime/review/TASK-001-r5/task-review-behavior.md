# TASK-001 behavior review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `09e4b82c2a4cab3ea27e0889d3acf3ea22c2b596`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`,
  Revision 11, including `REQ-053`
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Prior verdict and validated ledger:
  `changes/active/skald-workflow-runtime/review/TASK-001-r4/{verdict.md,findings-validation.md}`
- Reviewed remediation:
  `changes/active/skald-workflow-runtime/review/TASK-001-r4/TASK-001-R3-close-round-four-review-gaps.md`
- Review scope: complete cumulative base-to-candidate diff, with
  `FIND-TASK-001-27`, `FIND-TASK-001-28`, and `FIND-TASK-001-29` treated as
  closure hypotheses rather than conclusions

The candidate was exactly `09e4b82c2a4cab3ea27e0889d3acf3ea22c2b596`
at review start and after focused verification. CodeGraph is not initialized
in this repository, so source and caller tracing used Git, `rg`, and direct
inspection.

## Review findings

### Critical

None.

### Important

- **BEH-R5-001 — Implicit in-process teardown on an active Tokio runtime can
  release the fixture before Bifrost abort settlement completes.**
  Classification: `INCORRECT`; prior stable finding hypothesis:
  `FIND-TASK-001-28`. `Drop` selects a zero teardown budget whenever it runs
  inside a Tokio runtime and drives `settle_lifecycle(Duration::ZERO)` on a
  scoped thread (`crates/wyrd/wyrd-testing/src/server.rs:3685-3703`). That
  method marks `bifrost_settled = true`, skips graceful shutdown, and wraps the
  existing deliberately unbounded `Bifrost::abort()` in
  `timeout_at(Instant::now(), ...)` (`server.rs:831-846`). Tokio polls the abort
  future once before the already-expired timer wins. That first poll signals
  role cancellation and starts storage abort, but `BifrostStorage::abort`
  explicitly awaits every loader and governed request before it establishes
  quiescence (`crates/vala/vala-bifrost-redux/src/storage/mod.rs:1040-1059`).
  If any such work is still pending, the timeout drops the settlement future,
  logs a warning, and field destruction proceeds through runtimes to the
  last-declared `PgFixture`. The retained task may therefore still observe the
  forced database drop—the exact race the remediation must close. This is the
  ordinary implicit-drop path of the many async tests that construct
  `WyrdTestServer::start_in_process()` and let the value fall out of scope, not
  a speculative topology. The new in-process test does not cover it: it is a
  synchronous `#[test]`, deliberately drops off-runtime, gets the two-second
  budget, and asserts only storage settlement (`server.rs:5479-5504`). Keep
  the existing owner and off-runtime cleanup thread, but let the abort path run
  to completion and join before field/fixture destruction; only the graceful
  attempt needs a deadline. Add a deterministic active-runtime implicit-drop
  case with pending Bifrost-owned work and assert settlement precedes fixture
  release. Do not add a sleep or weaken Oracle self-fencing.

### Suggestions

None. Optional improvements are outside this acceptance audit.

## Prior-finding closure

| Prior finding | Present-round behavior assessment | Evidence |
|---|---|---|
| `FIND-TASK-001-27` | **CLOSED** | `docs/architecture/skald.md:3-6,43-66` now describes and diagrams the narrow `skald-workflow -> wyrd-spec` contract edge, names the other live foundational consumers, and preserves the prohibition on `wyrd-server`/application and Vala dependencies. This agrees with the Skald manifests and doctrine. |
| `FIND-TASK-001-28` | **OPEN / REVISED by `BEH-R5-001`** | Explicit `shutdown()` retains, aborts, and joins an overdue bound serve task; off-runtime in-process drop reaches the existing Bifrost abort fallback. Active-runtime implicit drop, however, applies an already-expired timeout to the unbounded settlement owner and proceeds after only signalling abort. |
| `FIND-TASK-001-29` | **CLOSED** | `chat_span` derives the model from the effective post-callback request with the existing `request_model` helper and falls back to the resolved Prompt model only for model-less request shapes. The focused callback-replacement test proves dispatch and span both record `gpt-4o-mini` while retaining provider, operation, and payload-exclusion controls. |

Earlier `FIND-TASK-001-1` through `FIND-TASK-001-26` were rechecked as
closure hypotheses against the cumulative candidate and prior source-grounded
evidence. No removed Observer surface, implicit Workflow behavior, credential
leak, deadline/panic regression, second runtime, compatibility alias, or other
previously diagnosed behavior became reachable again.

The user-highlighted Forge residual is not itself a separate finding.
In-process mode does not run `BoundServer::run`, so it does not spawn or join
the production Forge supervisors and cannot truthfully set
`Forge::supervision_drained`. `Bifrost::shutdown` therefore rejects graceful
drain and enters the existing abort path. That is the remediation's authorized
fallback and is safe when awaited to completion; `BEH-R5-001` is that the
active-runtime drop path does not await that fallback to completion.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| One declarative Workflow Card model; Agent-only steps; pure Workflow contracts in `wyrd-spec`; existing Prompt binder remains the interpolation owner (`REQ-001`–`REQ-004`, `INV-002`–`INV-003`, `INV-014`) | `wyrd-spec/src/card/workflow.rs`; `skald-workflow/src/{plan,workflow_surface}.rs`; resolved execution calls the existing Prompt binder | Contract/schema evidence and focused pure/resolved validation tests retained in the cumulative candidate | PASS |
| Explicit acyclic DAG, typed invocation inputs, visibility-checked bindings, namespaced step results, explicit output projection, and pre-dispatch refusal (`REQ-005`–`REQ-013A`, `AC-005`–`AC-008`) | `WorkflowSpec::validate`; `ExecutionPlan`; `WorkflowExecutor::bind`; `RunLedger` | `explicit_namespaced_results`, `resolved_bindings_reject_before_dispatch`, `explicit_builder_contract` | PASS |
| One async executor with bounded concurrency, owned child tasks, deterministic failure ordering, complete portable terminal snapshots, cancellation, timeout, and retry semantics (`REQ-015`–`REQ-023`, `REQ-043`, `REQ-047`–`REQ-048`, `INV-016`, `AC-019`–`AC-020`) | `WorkflowExecutor`, `StepTask`, `RunLedger`, bounded `JoinSet`, checked deadline construction | Retained lifecycle, attempt-deadline, panic-settlement, cancellation, and result-budget evidence | PASS |
| Exact Native, WyrdGateway, and ExtGateway routing with immutable per-attempt context and provider-native request shapes (`REQ-035`–`REQ-040`, `INV-004`, `INV-009`, `INV-011`, `INV-020`, `AC-011A`) | `WorkflowExecutionDependencies`, `AttemptRouteContext`, route-specific adapters | `isolated_route_calls` and Responses continuation coverage | PASS |
| One shared ExtGateway policy with DNS/address pinning, TLS/no-proxy/no-redirect/body bounds, exact binding checks, and secret containment (`REQ-042`, `REQ-049`, `INV-010`, `INV-010A`, `INV-012`, `INV-017`, `AC-016`, `AC-023`) | `skald-providers::EndpointPolicy`; `ExternalGatewayClient`; `ExternalGatewayBindings` | `bound_external_gateway_security`, successful-response reflection refusal, and boundary evidence | PASS |
| Input, step-result, full-run, metadata, and terminal-reserve accounting prevents oversized retained data (`REQ-017`, `REQ-045`, `INV-023`) | `ExecutionPlan::resolve_input`; `AttemptOutcome::from_agent`; exact JCS accounting in `RunLedger` | Size-bound and terminal-reserve tests retained green | PASS |
| Rust and Python local authoring/results share the Rust executor and exact portable contracts; caller-supplied tools remain explicit (`REQ-024`, `REQ-047`, `REQ-051`–`REQ-052`, `INV-007`–`INV-008`, `AC-024`, `AC-026`) | Rust `Workflow` builder/runtime; SDK-owned `PyWorkflow`/`PyWorkflowRun`; shared runtime bridge | Recorded Rust builder/tool, Python unit/type, and codegen lanes | PASS |
| Revision 11 deletes the Observer system and uses payload-free tracing with correct semantic provider/model attributes (`REQ-053`) | Observer crate/hooks/exports/stubs/tests/examples removed; existing spans retained; `chat_span` uses the effective request model | `agent_run_chat_span_records_callback_replaced_model` independently passed; retained ordinary OpenAI/Gemini/Vertex capture coverage | PASS |
| Permanent Skald architecture guidance matches the implemented foundational contract dependency (`FIND-TASK-001-27`) | Updated overview, dependency prose, and diagram in `docs/architecture/skald.md` | Direct comparison with Skald manifests and `architecture/wyrd-doctrine.mdx`; recorded `docs:check` and client-tier checks | PASS |
| Bound and in-process `WyrdTestServer` teardown finishes or explicitly aborts owned database work before fixture removal, including implicit drop (`FIND-TASK-001-28` remediation acceptance) | Bound timeout aborts and joins the serve task; explicit shutdown/off-runtime drop invokes Bifrost shutdown and abort fallback; active-runtime drop uses zero-budget `timeout_at` around abort | The two new focused tests independently passed, but neither exercises active-runtime implicit drop with pending work | **FAIL — `BEH-R5-001`** |
| Prohibited scope remains excluded: no compatibility alias, second Workflow runtime, remote Python/TS/MCP Workflow surface, durable scheduler, credential administration, production lifecycle API, new dependency, or replacement Observer system | Complete cumulative diff and remediation write set | Source/diff inspection and recorded boundary lanes | PASS |

## Proposed findings

### BEH-R5-001

- Classification: `INCORRECT`
- Violated obligation: `FIND-TASK-001-28` remediation acceptance requires
  implicit `WyrdTestServer` drop to establish completion or abort of existing
  Bifrost-owned work before runtime owners and `PgFixture` are released.
- Exact location: `crates/wyrd/wyrd-testing/src/server.rs:831-846,3685-3703`
  and the incomplete proof at `server.rs:5479-5504`; settlement contract at
  `crates/vala/vala-bifrost-redux/src/storage/mod.rs:1040-1059`.
- Evidence: active-runtime `Drop` passes `Duration::ZERO`; the already-expired
  `timeout_at` can drop `Bifrost::abort()` after its first pending poll, while
  that owner explicitly must await loaders and governed requests to guarantee
  no backend operation remains. `bifrost_settled` is already true, so no later
  owner retries settlement before fixture drop.
- Observable consequence: a common async-test implicit drop can still race a
  database-using task with `DROP DATABASE ... WITH (FORCE)`, preserving the
  scheduling-dependent Oracle self-fence/process-abort or after-drop failure
  the remediation was intended to eliminate.
- Required testable correction: preserve the current graceful budget for the
  serve/shutdown attempt, but run and join the existing abort owner without a
  second timeout before field destruction. Prove an in-process server dropped
  inside an active Tokio runtime with deterministically pending Bifrost-owned
  work is settled before fixture-drop observation. Reuse the existing owner and
  off-runtime scoped thread; add no sleep, retry, production API, or second
  shutdown abstraction.

## Open questions

None.

## Verification notes

- Independently ran
  `mise exec -- cargo nextest run --locked -p skald-agent --test agent_timeout
  -E 'test(=agent_run_chat_span_records_callback_replaced_model)'`: 1 selected,
  1 passed.
- Independently ran the repository-managed Postgres wrapper and exact
  `wyrd-testing` selectors
  `shutdown_aborts_and_joins_a_serve_task_that_outlives_its_drain` and
  `dropping_an_in_process_server_settles_bifrost_before_fixture_release`: 2
  selected, 2 passed.
- `git diff --check base..candidate` passed.
- The candidate records green `fmt`, `lints`, `test:skald`, `test:wyrd`,
  `docs:check`, `check:client-tier`, and the focused remediation tests. I did
  not rerun every broad lane.
- The in-process focused test is off-runtime and proves only the nonzero-budget
  branch; no available evidence directly exercises the active-runtime
  implicit-drop path diagnosed by `BEH-R5-001`.

## Overall result

**FAIL**

`FIND-TASK-001-27` and `FIND-TASK-001-29` are closed. The highlighted
in-process Forge abort fallback is appropriate because no production Forge
supervision runs in that mode, but `FIND-TASK-001-28` remains open: the common
active-runtime implicit-drop path can time out the abort settlement itself and
release the fixture while owned work remains live.
