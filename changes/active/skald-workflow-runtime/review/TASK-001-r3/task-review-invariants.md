# TASK-001 invariant review — round 3

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `afdd8cd716c4529bd8cbb7fbe175bef55ef6ee1f`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 11 (approval commit `9a621a28a`)
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Prior review and remediation authority: `changes/active/skald-workflow-runtime/review/TASK-001-r2/`, including `TASK-001-R1-close-validated-runtime-gaps.md` and `TASK-001-R1-addendum-revision-11.md`

The candidate remained at the stated commit throughout this review. I reviewed
the complete base-to-candidate range. Prior findings were treated as
hypotheses and retraced from their producers through the executor, result,
provider, Python, telemetry, and direct-consumer boundaries.

## Navigation and authority coverage

The invariant trace covered:

- pure Workflow contracts, validation, schemas, errors, retry counts, and
  header classification in `wyrd-spec`;
- resolved planning, binding, route selection, attempt classification,
  scheduling, cancellation, deadlines, snapshot accounting, and terminal
  projection in `skald-workflow`;
- provider-native request replay and Agent/model/tool telemetry in
  `skald-agent` and `skald-spec`;
- bound external-gateway transport and error containment in
  `skald-providers`;
- the local Python owner, exports, generated declarations, and tests in
  `wyrd-sdk-python`;
- direct Vala/Wyrd consumers; and
- deletion of the Skald Observer crate, hooks, exports, tests, examples,
  documentation, and check references without disturbing `wyrd.observe`.

Applicable authority was `AGENTS.md`, `architecture/agent-rules.md`,
`architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`,
`architecture/references/languages/spec-driven-development.md`,
`architecture/references/languages/maintainer-style.md`, the routed Rust,
Python, PyO3, testing, error, ownership, and agent references, approved spec
Revision 11, the original task, and both remediation files.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001–005, REQ-007–009, REQ-012–013A; AC-006: one declarative Agent-only graph, exact bindings, pure/resolved validation, no second renderer | `crates/wyrd-spec/src/card/workflow.rs`; `crates/skald/skald-workflow/src/{plan,workflow_surface}.rs` | Recorded `explicit_workflow_contract`, `resolved_bindings_reject_before_dispatch`, `explicit_builder_contract`; source trace of validation and lowering | PASS |
| REQ-006, REQ-010–012, REQ-018, REQ-020–023; INV-003/016/023; AC-005/007/008/019/020: deterministic namespaced state, bounded scheduling, complete terminal snapshots, and exact output projection | `skald-workflow/src/workflow.rs:228-408`; `run.rs:58-289` | Recorded `explicit_namespaced_results`; rerun `bounded_attempt_lifecycle` and `terminal_budget_reserve` | PASS |
| REQ-016–019, REQ-047–048; INV-003/004/014/016/021; AC-020: exact attempts, retry/backoff, checked deadlines, cancellation, peer drain, and owned lifetime | `wyrd-spec/src/card/workflow.rs:251-260`; `skald-workflow/src/plan.rs`; `workflow.rs:228-358,411-569`; `run.rs:198-289` | Rerun `bounded_attempt_lifecycle`; recorded focused deadline, retry-bound, cancellation, route, and executor-drop cases | PASS |
| REQ-017/022/045; INV-023; AC-019: exact input, step-result, complete-run, metadata, error, and terminal-reserve ceilings | `skald-workflow/src/attempt.rs:69-127`; `run.rs:127-155,198-305` | Rerun `terminal_budget_reserve`, including escaped text, rejected oversized payload, and canonical snapshot bound | PASS |
| REQ-035–040/043; INV-009/011/020; AC-011/011A/015: immutable isolated routes, safe remote projection, and provider-native Agent loops | `skald-workflow/src/{plan,route,workflow}.rs`; `skald-agent/src/request_builder.rs:162-166`; `skald-spec/src/wire/openai_responses.rs` | Rerun both exact `loop_responses` tests and the two exact `skald-spec` round-trip/dispatch tests; recorded `isolated_route_calls` | PASS |
| REQ-042/049; INV-010/010A/012/017; AC-016/023: exact-origin bound egress, shared reserved-header policy, bounded pinned transport, and secret containment | `wyrd-spec/src/card/workflow.rs:597-632`; `skald-workflow/src/route.rs:144-196`; `skald-providers/src/clients/external.rs:134-173` | Rerun `bound_external_gateway_security`, including reserved bindings and reflected-credential refusal | PASS |
| REQ-024/051 and Scenario 7: Rust/Python authoring and portable run projection use the same engine and exact nested result declarations | `sdks/wyrd-sdk-python/src/workflow.rs`; `python/wyrd/agent/{__init__.py,__init__.pyi}`; residual owner conversion only in `skald-workflow/src/python.rs` | Recorded `py:test:unit`, `py:typecheck`, `codegen:check`, and `check:pyo3-scope`; source/export/declaration trace | PASS |
| REQ-052; INV-008: resolved Agent tool declarations survive lowering and execute only through the supplied registry | `skald-workflow/src/plan.rs`; `workflow.rs:493-540`; `skald-agent/src/loop_runtime.rs:379-490` | Recorded local custom-tool and resolved-binding/runtime coverage; source trace shows no alternate executor or implicit tool selection | PASS |
| REQ-053: delete the Skald Observer system with no alias while preserving unrelated Vala observation surfaces | Deleted `crates/skald/skald-observer`; removed workspace/dependency edges, Agent/Workflow hooks, Python exports/stubs/tests/examples, Wyrd re-exports, docs, and check references; `wyrd.observe` remains registered in `sdks/wyrd-sdk-python/src/lib.rs` | Repository search found no `skald-observer`, `skald_observer`, `OtelObserver`, `with_observer(s)`, or `observers=` residue in the required surfaces; recorded boundary/codegen/Python/docs/example checks | PASS |
| REQ-053: synchronous, payload-free Workflow spans/events with one run span, sibling attempt spans, backoff event, terminal status/outcome, and stable error codes | `skald-workflow/src/workflow.rs:197-221,226-308,411-490,571-637` | Rerun `run_tracing_spans`; it proves hierarchy, retry siblings, backoff, cancellation, stable codes, and payload-marker absence | PASS |
| REQ-053: payload-free GenAI Agent/model/tool spans and retained timeout behavior, using the existing telemetry bridge only | `skald-agent/src/loop_runtime.rs:30-180,297-317,379-490,590-635`; `skald-agent/tests/agent_timeout.rs` | Reran all three exact `agent_timeout` tests; source and tests show identifier/status/code attributes only and the existing `wyrd_telemetry::init_test_capture` helper | PASS |
| Revision-10 public seams retained: secret bindings use `HashMap<HeaderName, SecretString>` and remote problems use the approved boxed five-field payload | `skald-workflow/src/route.rs`; `skald-providers/src/error.rs` and all direct matches | Static public-type and exhaustive-consumer trace; recorded all-feature lint | PASS |
| Repository rules and task proof: owner boundaries, generated surfaces, docs/examples, exact focused commands, clean cumulative diff | Cumulative changed source, manifests, generated files, task evidence, and deleted Observer references | Recorded format/lint/codegen/boundary/docs/examples/Python/Skald lanes; this review reran 11 exact focused tests; explicit `git diff --check base..candidate` exits zero | PASS |
| Non-goals: no remote Python/TS/MCP Workflow surface, compatibility engine/alias, new crate or third-party dependency, scheduler/queue, telemetry-init API, or credential administration | Complete cumulative diff and current dependency/public-surface graph | Static cumulative-diff and consumer inspection | PASS |

## Prior-finding closure

| Prior ID | Round-3 result | Producer-to-sink closure evidence |
|---|---|---|
| FIND-TASK-001-1 | CLOSED | `RunLedger::step_succeeded` charges exact JCS growth over the reserved `null` fields; escaped-text cases remain at or below the complete-run ceiling or fail with the exact 413 result. |
| FIND-TASK-001-2 | CLOSED | `assistant_message` retains every Responses output item; the wire type preserves reasoning `id`, summary parts, and encrypted continuation state; the next native request replays reasoning, function call, and tool output in order. |
| FIND-TASK-001-3 | CLOSED by approved Revision 10 | The public secret-header binding type remains the approved `HashMap<HeaderName, SecretString>` through construction and egress merge. |
| FIND-TASK-001-4 | CLOSED by approved Revision 10 | `ProviderError::RemoteProblem(Box<RemoteProblem>)` and its direct consumers retain the approved five-field safe payload. |
| FIND-TASK-001-5 | CLOSED by Revision 11 deletion | No payload-bearing Observer callback remains. `AttemptOutcome::from_agent` rejects and discards an oversized step payload at the engine admission boundary; tracing contains no result payload. |
| FIND-TASK-001-6 | CLOSED by Revision 11 deletion | No Workflow Observer callback can panic, block, or own the result. Span/event emission is synchronous field recording; cancellation/deadline still abort and drain owned step tasks. |
| FIND-TASK-001-7 | CLOSED | Changed test imports are module-scoped; recorded formatting and lint lanes pass. |
| FIND-TASK-001-8 | CLOSED | The required `docs:check` and `check:examples` lanes are recorded at exit zero. |
| FIND-TASK-001-9 | CLOSED | The four previously missing exact selectors, plus the remediation/REQ-053 selectors, are recorded and were independently rerun where material to this invariant pass. |
| FIND-TASK-001-10 | CLOSED | New Workflow PyO3 authoring/result behavior is owned and registered by `wyrd-sdk-python`; `skald-workflow` retains only the error conversion forced by the orphan boundary. |
| FIND-TASK-001-11 | CLOSED | Public stubs expose precise `WorkflowRunError`, `WorkflowStepResult`, and `WorkflowRunDict` structures while genuinely open JSON values remain `Any`. |
| FIND-TASK-001-12 | CLOSED by Revision 11 deletion | The Observer methods and hooks no longer exist. REQ-053 span behavior is documented at its owners and in the Workflow guide. |
| FIND-TASK-001-13 | CLOSED | The public guide describes deterministic sanitized/prefixed/suffixed IDs and directs callers to the resulting `Workflow.steps`; builder coverage remains green. |
| FIND-TASK-001-14 | CLOSED | Settlement marks an aborted task cancelled only after its shared attempt counter is nonzero; `RunLedger::finish` resets never-polled running entries to unstarted with zero attempts and no start timestamp. |
| FIND-TASK-001-15 | CLOSED | Resolved authored timeouts and local run deadlines use checked instant construction; representable execution paths retain the existing precedence and no unchecked public deadline construction remains. |
| FIND-TASK-001-16 | CLOSED | Pure Workflow validation rejects only `u32::MAX`; all accepted retry counts fit the public `max_retries + 1` attempt field. |
| FIND-TASK-001-17 | CLOSED | Binding insertion reuses the contract owner's transport/routing/internal classifier and still accepts binding-owned credential headers. |
| FIND-TASK-001-18 | CLOSED | `ExternalGatewayClient` replaces final refusal bodies before returning a publicly inspectable provider error while retaining status and retry metadata; Workflow projection remains redacted. |
| FIND-TASK-001-19 | CLOSED | The round-one trailing blank line is removed and `git diff --check a51af030b6039eea4b2914f3ebf2c31925d08721..afdd8cd716c4529bd8cbb7fbe175bef55ef6ee1f` exits zero. |

## Proposed findings

None. The complete cumulative candidate satisfies the original TASK-001
invariants and closes the prior validated ledger under approved Revision 11.

## Verification notes and limits

- Independently rerun in this review: four exact `skald-workflow` tests
  (`run_tracing_spans`, `terminal_budget_reserve`,
  `bounded_attempt_lifecycle`, `bound_external_gateway_security`), two exact
  Responses-loop tests, three exact Agent timeout/telemetry tests, and two
  exact `skald-spec` round-trip/dispatch tests. All 11 passed.
- Independently checked the explicit cumulative `git diff --check`; it passed.
- The remediation record reports exit zero for `fmt`, `lints`, `py:format`,
  `py:lints`, `codegen:check`, `check:client-tier`, `check:pyo3-scope`,
  `check:unwrap-audit`, `test:skald`, `py:test:unit` (490 passed),
  `py:typecheck`, `docs:check`, `check:examples`, the error-coverage script,
  `wyrd`, and `vala-eval` tests. I inspected the source and consumers those
  results cover rather than treating the recorded green lanes as acceptance by
  themselves.
- I did not rerun every recorded broad lane; the independent focused reruns,
  source trace, generated/public-surface inspection, and recorded broad evidence
  are sufficient for this invariant audit.

## Overall result

**PASS**
