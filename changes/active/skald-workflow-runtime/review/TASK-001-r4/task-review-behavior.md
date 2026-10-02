# TASK-001 behavior review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `a704a8890ef20efe65fee1e116f7d02288f8ec5c`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`,
  Revision 11, including REQ-053
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Remediation authority:
  `changes/active/skald-workflow-runtime/review/TASK-001-r3/TASK-001-R2-close-round-three-runtime-gaps.md`
- Review scope: complete cumulative base-to-candidate diff, with
  `FIND-TASK-001-20` through `FIND-TASK-001-27` treated as closure hypotheses
  and commit `516d0fbcc` reviewed as part of the candidate

The candidate was exactly `a704a8890ef20efe65fee1e116f7d02288f8ec5c` at
review start and after focused verification. CodeGraph is not initialized in
this repository, so normal repository navigation was used.

## Review findings

### Critical

None.

### Important

- **BEH-R4-001 — `FIND-TASK-001-27` is not fully closed: permanent Skald
  architecture documentation contradicts the implemented dependency.**
  Classification: `INCORRECT`. The original task makes `wyrd-spec` the owner
  of Workflow contracts and the candidate correctly gives `skald-workflow` a
  direct `wyrd-spec` dependency
  (`crates/skald/skald-workflow/Cargo.toml:30`). The remediation-updated crate
  map also says that explicitly (`docs/architecture/skald.md:28-38`), but the
  same page still says Skald depends only on neutral infrastructure plus Skald
  crates, that Skald has no `wyrd-*` dependency, and that Skald engine crates
  do not depend on `wyrd-spec` (`docs/architecture/skald.md:3-5,42-46`). Its
  dependency diagram likewise omits the required `skald-workflow -> wyrd-spec`
  edge (`docs/architecture/skald.md:48-58`). The page therefore gives mutually
  exclusive ownership guidance for the exact boundary TASK-001 introduced,
  so maintainers cannot use it to decide whether the implemented dependency is
  required or forbidden. Update the opening boundary statement, dependency
  prose, and diagram to identify `skald-workflow`'s narrow `wyrd-spec`
  contract dependency while preserving that the other Skald runtime/provider
  crates do not depend on Wyrd application or Vala crates. This is the minimum
  correction; no compatibility note, new abstraction, or broader dependency
  allowance is needed.

### Suggestions

None. Optional improvements are outside this acceptance audit.

## Prior-finding closure

| Prior finding | Present-round behavior assessment | Evidence |
|---|---|---|
| `FIND-TASK-001-20` | **CLOSED** | `StepTask::run` fixes and races the Agent deadline after cancellation, total, and step deadlines, converts expiry through the existing typed Agent timeout, and drops pending settlement. `agent_deadline_bounds_settlement` proves retry/exhaustion, precedence, closed spans, and zero live work. |
| `FIND-TASK-001-21` | **CLOSED** | The complete attempt future is caught inside `StepTask::run` while `AttemptSpan` is alive; a panic becomes the existing non-retryable internal outcome and follows ordinary failed-span settlement. `attempt_panic_matches_span` proves matching run, step, and trace codes. |
| `FIND-TASK-001-22` | **CLOSED** | `ExternalGatewayClient::post` withholds decode/status details and refuses a typed successful response whose retained strings or member names contain a nonempty sensitive bound header value. The reflection test covers assistant text, escaped text, tool arguments, a retained member name, ignored unknown members, clean success, no retry, and no run projection leak. |
| `FIND-TASK-001-23` | **CLOSED** | Agent and chat spans use the Prompt model, `openai`, `anthropic`, `gcp.gemini`, or `gcp.vertex_ai`, retain the approved `chat` operation, and omit the scalar finish-reasons field. The OpenAI, Gemini, and Vertex capture tests pass. |
| `FIND-TASK-001-24` | **CLOSED** | The cited Agent entry points, request helpers, moved methods, Workflow fixtures, and timeout/telemetry fixtures now carry substantive rustdoc including errors and lifecycle behavior. |
| `FIND-TASK-001-25` | **CLOSED** | Stateful run, prompt-loop, journal, tool, and session orchestration is now on inherent `Agent` methods; the public `Agent::run` delegates to `run_with`, and only stateless transformations remain free. |
| `FIND-TASK-001-26` | **CLOSED** | `skald-workflow` uses `std::io::Error` through `SchemaResolverError` and no longer directly depends on `anyhow`. |
| `FIND-TASK-001-27` | **OPEN / REVISED by `BEH-R4-001`** | Deleted Workflow names and the absent example entry were removed, but the same materially updated architecture page still denies the direct `skald-workflow -> wyrd-spec` dependency that its crate map and manifest require. |

No earlier `FIND-TASK-001-1` through `FIND-TASK-001-19` behavior has become
reachable again. Revision 11's Observer deletion remains complete: the removed
crate, hooks, Python exports/stubs/tests/examples, and compatibility surfaces
are absent, while unrelated Vala/Bifrost observation code remains.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| One declarative Workflow Card model; Agent-only steps; pure Workflow contracts in `wyrd-spec`; existing Prompt binder remains the interpolation owner (`REQ-001`–`REQ-004`, `INV-002`–`INV-003`, `INV-014`) | `wyrd-spec/src/card/workflow.rs`; `skald-workflow/src/plan.rs`, `workflow_surface.rs`; resolved execution calls the existing Prompt render path | Contract/schema evidence and the task's focused pure/resolved validation tests | PASS |
| Explicit acyclic DAG, typed inputs, visible bindings, namespaced step results, explicit final projection, and pre-dispatch validation (`REQ-005`–`REQ-013A`, `AC-005`–`AC-008`) | `WorkflowSpec::validate`; `ExecutionPlan`; `WorkflowExecutor::bind`; `RunLedger` | `explicit_namespaced_results`, `resolved_bindings_reject_before_dispatch`, `explicit_builder_contract` | PASS |
| One async execution algorithm with bounded concurrency, owned child tasks, deterministic failure ordering, complete terminal snapshots, cancellation, and deadline drain (`REQ-015`–`REQ-023`, `REQ-043`, `REQ-047`–`REQ-048`, `INV-016`, `AC-019`–`AC-020`) | `WorkflowExecutor`, `StepTask`, `RunLedger`, one bounded `JoinSet`, checked deadline construction | Existing lifecycle suite plus independently rerun Agent-deadline and panic cases | PASS |
| Fixed Agent deadline covers provider/tool work, normalization, output admission, and terminal settlement with cancellation > total > step > Agent precedence (`REQ-016`, `REQ-043`, prior `FIND-TASK-001-20`) | `workflow.rs:430-518`, especially the biased Agent-deadline arm at 469-477 | `workflow::tests::agent_deadline_bounds_settlement` independently passed | PASS |
| Attempt panic and cancellation telemetry agrees with the returned run (`REQ-053`, prior `FIND-TASK-001-21`) | `AssertUnwindSafe(...).catch_unwind()` remains inside the attempt-span owner; ordinary failure records stable code; `AttemptSpan::drop` is reserved for interruption | `attempt_panic_matches_span` and the cancellation control in `run_tracing_spans` | PASS |
| Exact route semantics and immutable per-attempt gateway context; Native, WyrdGateway, and ExtGateway preserve the provider-native request (`REQ-035`–`REQ-040`, `INV-004`, `INV-009`, `INV-011`, `INV-020`) | `WorkflowExecutionDependencies`, `AttemptRouteContext`, route-specific provider adapters | `isolated_route_calls` and retained Responses continuation tests | PASS |
| ExtGateway policy, secret containment, direct execution-local egress, and shared endpoint owner (`REQ-042`, `REQ-049`, `INV-010`, `INV-010A`, `INV-012`, `INV-017`, `AC-016`, `AC-023`) | `skald-providers::EndpointPolicy`; `ExternalGatewayClient`; `ExternalGatewayBindings`; sensitive merged headers; typed-success reflection refusal | `bound_external_gateway_security` and `external_gateway_success_reflection` independently passed | PASS |
| Input, step-result, and complete-run bounds prevent oversized payload retention while preserving terminal metadata (`REQ-017`, `REQ-045`, `INV-023`) | `ExecutionPlan::resolve_input`; pre-ledger `AttemptOutcome::from_agent`; exact JCS delta and terminal reserve in `RunLedger` | Existing size-bound and terminal-reserve tests recorded green | PASS |
| Rust and Python local authoring/results project the same Rust engine and exact portable types; local tools remain caller supplied (`REQ-024`, `REQ-047`, `REQ-051`–`REQ-052`, `INV-007`–`INV-008`, `AC-024`, `AC-026`) | Rust `Workflow`/builder; SDK-owned `PyWorkflow`/`PyWorkflowRun`; Python runtime bridge delegates to native async execution | Recorded Python unit/type/codegen lanes and Rust builder/tool tests | PASS |
| Delete the Skald Observer system without aliases and replace it with payload-free synchronous tracing through the existing bridge (`REQ-053`) | Observer crate/dependencies/hooks/exports/stubs/tests/examples removed; `workflow.run`, per-attempt `workflow.step`, backoff event, `invoke_agent`, `chat`, and `execute_tool` spans remain | Independently rerun Agent telemetry cases; existing Workflow trace capture; source/manifests search | PASS |
| GenAI semantic fields use standard provider/model values and omit the incorrectly typed optional finish-reasons attribute (prior `FIND-TASK-001-23`) | `invoke_agent_span`, `chat_span`, `genai_provider_name` | `agent_run_emits_genai_spans_without_payloads` and `agent_run_genai_google_provider_and_model` independently passed | PASS |
| Rust structure, library error dependency, and documentation remediation (`FIND-TASK-001-24`–`FIND-TASK-001-27`) | Agent orchestration moved to `Agent`; direct Workflow `anyhow` removed; cited rustdoc added; permanent docs revised | Source inspection and recorded lints/docs/examples lanes | **FAIL** — `BEH-R4-001`; the Skald architecture page still contradicts the implemented contract dependency |
| Test-harness fix in `516d0fbcc` stops background roles before fixture database teardown without changing product behavior | `WyrdTestServer::shutdown` and `Drop` cancel the composed state token for bound and in-process modes; `fixture` is declared after state and runtime owners so database cleanup occurs last | Recorded post-fix `mise run test:wyrd`: 2282 passed, 161 skipped; source trace through `AppState`'s shared shutdown token and fixture `DROP DATABASE ... WITH (FORCE)` | PASS |
| Prohibited scope remains excluded: no compatibility alias, second engine/runtime, remote Python/TS/MCP Workflow surface, durable scheduler, gateway credential administration, new crate, or replacement Observer system | Complete cumulative diff and manifests | Source/diff inspection and recorded boundary lanes | PASS |

## Proposed findings

### BEH-R4-001

- Classification: `INCORRECT`
- Violated obligation: TASK-001 ownership and dependency boundary; remediation
  acceptance for `FIND-TASK-001-27` that changed permanent documentation
  accurately describe the implemented Revision 11 surface.
- Exact location: `docs/architecture/skald.md:3-5,42-58`, contradicted by
  `docs/architecture/skald.md:28-38` and
  `crates/skald/skald-workflow/Cargo.toml:30`.
- Evidence: the page both requires and denies the direct
  `skald-workflow -> wyrd-spec` dependency, and its diagram omits that edge.
- Observable consequence: maintainers following the architecture page can
  remove a required contract dependency or reject correct work as an ownership
  violation.
- Required testable correction: revise only the page's general Skald boundary,
  dependency-direction paragraph, and diagram to show the narrow
  `skald-workflow -> wyrd-spec` contract edge while keeping other Skald crates
  free of Wyrd application and Vala dependencies. Prove by source inspection,
  `mise run docs:check`, and `mise run check:client-tier`.

## Open questions

None.

## Verification notes

- Independently ran the exact `skald-workflow` selection for
  `agent_deadline_bounds_settlement`, `attempt_panic_matches_span`,
  `external_gateway_success_reflection`, and
  `bound_external_gateway_security`: 4 passed.
- Independently ran the exact `skald-agent --test agent_timeout` selection for
  `agent_run_timeout_terminates_cleanly`,
  `agent_run_no_timeout_runs_to_completion`,
  `agent_run_emits_genai_spans_without_payloads`, and
  `agent_run_genai_google_provider_and_model`: 4 passed.
- `git diff --check base..candidate` passed.
- The candidate records green `fmt`, `lints`, `test:skald`, `test:shared`,
  `docs:check`, `check:examples`, and the post-`516d0fbcc` full `test:wyrd`
  lane. I did not rerun every broad lane.
- `docs:check` is syntactic/build evidence and does not detect the semantic
  dependency contradiction in `BEH-R4-001`.

## Overall result

**FAIL**

The runtime, telemetry, credential-containment, owner-shape, error-dependency,
and test-harness remediations close `FIND-TASK-001-20` through
`FIND-TASK-001-26`. `FIND-TASK-001-27` remains materially open because the
permanent Skald architecture page still contradicts the required and
implemented `skald-workflow -> wyrd-spec` contract dependency.
