# Task behavior review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `afdd8cd716c4529bd8cbb7fbe175bef55ef6ee1f`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, revision 11, approved by `9a621a28a`
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Remediation inputs: `review/TASK-001-r2/TASK-001-R1-close-validated-runtime-gaps.md` and `review/TASK-001-r2/TASK-001-R1-addendum-revision-11.md`
- Review scope: complete cumulative base-to-candidate diff

The candidate was exactly `afdd8cd716c4529bd8cbb7fbe175bef55ef6ee1f` at review start. CodeGraph is absent. I read the repository authorities, Revision 11, the original and remediation tasks, the complete cumulative diff, the prior ledger as hypotheses, and the changed producer-to-result paths. The prior Observer remediation is superseded where the Revision 11 addendum says deletion closes it.

## Review findings

### Critical

None.

### Important

None.

### Suggestions

None. Optional improvements are outside this acceptance audit.

## Prior-finding closure

| Prior finding | Present-round behavior assessment | Evidence |
|---|---|---|
| `FIND-TASK-001-1` | **CLOSED** | `RunLedger::step_succeeded` charges the exact JCS growth of retained `text` and `structured_output` over their reserved `null` fields (`skald-workflow/src/run.rs`). `terminal_budget_reserve` covers escaped text at the ceiling. |
| `FIND-TASK-001-2` | **CLOSED** | `assistant_message` retains every Responses output item in provider order; `OpenAiResponseItem::Reasoning` now carries replay identity, typed summaries, and encrypted continuation state. The two Responses loop tests pass. |
| `FIND-TASK-001-3` | **CLOSED by approved Revision 10** | `ExternalGatewayBinding.secret_headers` remains the approved `HashMap<HeaderName, SecretString>` seam. |
| `FIND-TASK-001-4` | **CLOSED by approved Revision 10** | `ProviderError::RemoteProblem(Box<RemoteProblem>)` remains the approved boxed five-field projection. |
| `FIND-TASK-001-5` | **CLOSED by Revision 11 deletion plus direct admission** | No payload-bearing Observer path remains. `AttemptOutcome::from_agent` enforces `max_step_result_bytes` before the result enters the run ledger; tracing carries no input or result payload. |
| `FIND-TASK-001-6` | **CLOSED by Revision 11 deletion** | The user callback-based Observer boundary and its pending/panic liveness paths no longer exist. Plain tracing does not participate in run control flow; cancellation/drop records an attempt outcome without changing the authoritative run. |
| `FIND-TASK-001-7` | **CLOSED** | The changed tests use their module import blocks; no reviewed test function retains the cited local imports. Formatting and lint lanes are recorded green. |
| `FIND-TASK-001-8` | **CLOSED** | `mise run docs:check` and `mise run check:examples` are recorded exit 0 on the candidate. |
| `FIND-TASK-001-9` | **CLOSED** | The remediation evidence records exact `mise exec -- cargo nextest run --locked` selectors for all named tests. This review independently reran the 15 relevant tests (in three exact-filter invocations); all passed. |
| `FIND-TASK-001-10` | **CLOSED** | New Workflow PyO3 ownership is in `sdks/wyrd-sdk-python/src/workflow.rs`; the retained `skald-workflow` Python module no longer owns the task-added surface, and Observer registration is deleted. |
| `FIND-TASK-001-11` | **CLOSED** | Generated public declarations contain precise `WorkflowRunError`, `WorkflowStepResult`, and `WorkflowRunDict` `TypedDict`s; only JSON-valued fields remain broad. Codegen/typecheck are recorded green. |
| `FIND-TASK-001-12` | **CLOSED by Revision 11 deletion** | The concrete Observer hooks no longer exist. The surviving tracing contract is documented and source-documented. |
| `FIND-TASK-001-13` | **CLOSED** | The workflow guide describes sanitization, leading-digit prefixing, unnamed IDs, collision suffixes, and directs callers to `Workflow.steps`; `explicit_builder_contract` passes. |
| `FIND-TASK-001-14` | **CLOSED** | Executor settlement leaves an aborted never-polled task for `RunLedger::finish`, which produces `unstarted`, zero attempts, and no timestamps; begun interrupted work is cancelled. `bounded_attempt_lifecycle` passes. |
| `FIND-TASK-001-15` | **CLOSED** | Plan preparation and local run construction use checked deadline creation and return field-specific/pre-dispatch errors rather than panicking or clamping. The focused validation/lifecycle tests pass. |
| `FIND-TASK-001-16` | **CLOSED** | Pure `WorkflowSpec` validation rejects only `u32::MAX` retries; `u32::MAX - 1` remains valid. Attempt accounting therefore remains representable. |
| `FIND-TASK-001-17` | **CLOSED** | `ExternalGatewayBindings::insert` reuses `is_reserved_transport_header` and rejects transport, routing, forwarding, proxy, and Wyrd-internal names while permitting binding-owned credential headers. |
| `FIND-TASK-001-18` | **CLOSED** | `ExternalGatewayClient::post` replaces external non-success bodies with a fixed diagnostic while retaining status and retry metadata; native-provider behavior is unchanged. |
| `FIND-TASK-001-19` | **CLOSED** | The extra terminal blank line was removed; `git diff --check a51af030b6039eea4b2914f3ebf2c31925d08721..afdd8cd716c4529bd8cbb7fbe175bef55ef6ee1f` exits 0. |

No prior finding remains reachable in the cumulative candidate.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| One Workflow Card model; Agent-only steps; existing Prompt/request and binding owner; unsupported actions/condition removed (`REQ-001`–`REQ-004`, `INV-002`–`INV-003`, `INV-014`) | `wyrd-spec/src/card/workflow.rs`; `skald-workflow/src/plan.rs`, `workflow_surface.rs`; existing Prompt binder called from the resolved plan | Contract/schema evidence plus `resolved_bindings_reject_before_dispatch` and `explicit_builder_contract` | PASS |
| Explicit acyclic DAG, input typing, exact visible bindings, namespaced outputs, explicit final projection, and complete pure/resolved validation (`REQ-005`–`REQ-013A`, `INV-016`, `AC-005`–`AC-008`) | `WorkflowSpec::validate`; `ExecutionPlan`; `WorkflowExecutor::bind`; `RunLedger::select/finish` | `explicit_namespaced_results`, `resolved_bindings_reject_before_dispatch`, and the recorded contract tests | PASS |
| Same prepared plan and provider abstraction; local suitability and route semantics (`REQ-015`, `REQ-035`–`REQ-040`, `INV-001`, `INV-004`, `INV-009`, `INV-011`) | `ExecutionPlan`; `WorkflowExecutionDependencies`; immutable per-attempt adapters; native request retained in Agent loop | `isolated_route_calls`; Responses loop and request round-trip tests | PASS |
| Retry, timeout, bounded concurrency, failure drain, cancellation, partial terminal snapshots, exact statuses, deterministic ordering, and owned task lifetime (`REQ-016`, `REQ-018`–`REQ-023`, `REQ-043`, `REQ-047`–`REQ-048`, `INV-020`–`INV-021`, `AC-019`–`AC-020`) | One `WorkflowExecutor` with bounded `JoinSet`; `StepTask`; `RunLedger`; checked deadlines; representable retry bound | `bounded_attempt_lifecycle`, `explicit_namespaced_results`, `isolated_route_calls` | PASS |
| Input, step-result, graph, and complete-run size bounds, including terminal reserve and errors (`REQ-017`, `REQ-045`, `INV-023`, `AC-019`–`AC-020`) | `ExecutionPlan::resolve_input`; `AttemptOutcome::from_agent`; exact JCS delta and reserve in `RunLedger` | `terminal_budget_reserve` independently passed, including escaped text and exact 413 behavior | PASS |
| Rust/Python local authoring and result surfaces share the Rust engine and explicit contract; local tools remain caller supplied (`REQ-024`, `REQ-047`, `REQ-051`–`REQ-052`, `INV-007`–`INV-008`, `AC-024`, `AC-026`) | Rust `Workflow`/builder and SDK-owned `PyWorkflow`/`PyWorkflowRun`; Python runtime bridge delegates to the native async engine; resolved Agents retain tools | Recorded Python unit/type/codegen lanes; `explicit_builder_contract`; `isolated_route_calls` tool path | PASS |
| External gateway binding, origin/protocol/header policy, direct execution-local egress, safe errors, and shared lower-level endpoint policy (`REQ-042`, `REQ-049`, `INV-010`, `INV-010A`, `INV-012`, `INV-017`, `AC-016`, `AC-023`) | `skald-providers::EndpointPolicy` and `ExternalGatewayClient`; `ExternalGatewayBindings::insert`; fixed refusal-body projection | `bound_external_gateway_security`; recorded client-tier and lint checks | PASS |
| Cohesive public contract/error/schema change and all direct existing consumers (`REQ-045`, `REQ-051`, `AC-024`) | Cumulative diff updates `wyrd-spec`, Skald Agent/provider/workflow, Wyrd/Vala consumers, Python owner, schemas, docs, and examples together | Recorded `test:skald`, `lints`, `codegen:check`, Python lanes, Wyrd and Vala tests | PASS |
| Delete the Skald Observer system without aliases and preserve unrelated Vala/Bifrost observation surfaces (`REQ-053`, addendum item 1) | `skald-observer` crate/member and dependency edges deleted; Agent/Workflow/Python/Wyrd hooks, exports, stubs, tests, examples, docs, and check references deleted; no `wyrd.observe` deletion | Source/manifests/lockfile search; recorded client-tier, PyO3-scope, error-coverage, docs, examples, and Python lanes | PASS |
| Payload-free Workflow/Agent tracing with required hierarchy, attributes, outcomes, backoff, and stable error codes; no local init API (`REQ-053`, addendum items 2–4) | `workflow.run`, sibling per-attempt `workflow.step`, `workflow.step.backoff`, and Agent `invoke_agent`/`chat`/`execute_tool` spans in `workflow.rs` and `loop_runtime.rs`; existing `wyrd_telemetry` capture is test-only | `run_tracing_spans` and all three `agent_timeout` cases independently passed; assertions cover hierarchy and payload absence | PASS |
| Exact native Responses continuation (`REQ-039`, prior `FIND-TASK-001-2`) | Reasoning identity/summary/encrypted state in the wire owner; all response items replayed in order | Both `loop_responses` tests and both exact `skald-spec` request tests independently passed | PASS |
| Revision 11 non-goals: no remote Python/TS/MCP Workflow surface, compatibility alias, second engine/runtime, new crate/dependency/check, durable scheduler/queue, or credential administration | Complete diff and manifests contain none of these additions; deletion reduces the surface | Diff/manifests and boundary evidence | PASS |

## Proposed findings

The proposed finding ledger is empty. I found no `MISSING`, `INCORRECT`, `DRIFT`, `VIOLATION`, or `REGRESSION` that remains reachable and required by TASK-001 under approved Revision 11.

## Open questions

None.

## Verification notes

- Independently reran 15 focused tests covering exact run accounting, cancellation/deadline/retry bounds, route isolation, external-gateway security, builder validation, Responses continuation, Agent timeout behavior, and payload-free tracing. All passed.
- The candidate records exit-zero results for `mise run fmt`, `lints`, `py:format`, `py:lints`, `codegen:check`, `check:client-tier`, `check:pyo3-scope`, `check:unwrap-audit`, `test:skald`, `py:test:unit` (490 passed), `py:typecheck`, `docs:check`, and `check:examples`, plus Wyrd/Vala consumer tests and `scripts/checks/error-coverage.sh`.
- I did not rerun every broad lane already recorded in the immutable candidate; source inspection and the focused reruns did not reveal a contradiction in that evidence.
- Candidate remained `afdd8cd716c4529bd8cbb7fbe175bef55ef6ee1f` at report completion.

## Overall result

**PASS**

The complete cumulative candidate satisfies TASK-001 under approved specification Revision 11. All prior round findings are closed, including Observer removal and its replacement with payload-free tracing, and the independently proposed finding ledger is empty.
