# TASK-001 invariant review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `a704a8890ef20efe65fee1e116f7d02288f8ec5c`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 11
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Remediation task: `changes/active/skald-workflow-runtime/review/TASK-001-r3/TASK-001-R2-close-round-three-runtime-gaps.md`

The candidate remained at the stated commit throughout this review. The complete
base-to-candidate range was reviewed; the round-three findings were treated as
hypotheses and retraced through their producers, consumers, and failure paths.

## State and lifecycle traces

- `WorkflowSpec` and its resolved Agents produce an immutable `ExecutionPlan`;
  `WorkflowExecutor` alone schedules the plan, and `RunLedger` alone transitions
  and terminalizes the portable `WorkflowRun`.
- A step attempt fixes total, step, and Agent deadlines before dispatch. The
  biased race orders cancellation, total deadline, step deadline, Agent
  deadline, then the complete attempt. Every terminal arm drops in-flight work
  before retry or settlement.
- Provider, tool, normalization, schema validation, size admission, and terminal
  Agent journal settlement remain inside the attempt future. A panic is caught
  while the attempt span guard is alive and becomes the same non-retryable
  `WYRD_WORKFLOW_500_INTERNAL` outcome consumed by the ledger.
- ExtGateway bound headers are marked sensitive at binding merge. The
  `ExternalGatewayClient` retains that designation, withholds unsuccessful and
  decode diagnostics, and checks the decoded typed success before it becomes a
  general `ProviderResponse` consumed by Agent or Workflow results.
- Agent telemetry is produced synchronously by the Agent owner. The resolved
  Prompt supplies model identity, the typed request supplies semantic provider
  identity, and only identifiers, counts, status, and stable error codes enter
  spans/events.
- `WyrdTestServer` now cancels the composition's shared state token in explicit
  shutdown and drop. `WyrdTestServerInner` declares `PgFixture` after the
  `AppState` and dedicated runtime owners, so the harness releases its database
  ownership after those owners; Oracle's reader-epoch supervisor also races the
  same process token inside an in-flight renewal.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001–015: one declarative Agent-only DAG, explicit bindings, pure/resolved validation, deterministic namespaced data flow, and one execution plan | `wyrd-spec/src/card/workflow.rs`; `skald-workflow/src/plan.rs`, `workflow_surface.rs`, `workflow.rs`, `run.rs` | Recorded task scenario tests `explicit_workflow_contract`, `resolved_bindings_reject_before_dispatch`, and `explicit_namespaced_results`; recorded `test:skald`, `test:shared`, codegen and boundary lanes | PASS |
| REQ-016/018/019/043: fixed deadlines, exact retry classification, precedence, abort-and-drain, peer drain, and no surviving attempt work | `workflow.rs:227-311,414-569`; `attempt.rs:67-193`; `run.rs:120-280` | `workflow::tests::agent_deadline_bounds_settlement` and `attempt_panic_matches_span` rerun: 2/2 passed; existing bounded lifecycle and cancellation controls recorded green | PASS |
| REQ-017/020–023/045: bounded input, step payload, full snapshot, complete terminal states, primary error, and typed deterministic outputs | `plan.rs:234-304`; `attempt.rs:34-125`; `run.rs:56-356` | Recorded `bounded_attempt_lifecycle`, `terminal_budget_reserve`, and full `test:skald` | PASS |
| REQ-024/047/051 and AC-019/024/026: aligned Rust/Python local authoring and portable result surfaces with retained feature compilation | `skald-workflow/src/python.rs`, `workflow_surface.rs`; Python SDK exports/stubs/tests in the cumulative diff | Recorded focused Python test, `py:test:unit`, `py:typecheck`, retained Python-feature check, codegen and lint lanes | PASS |
| REQ-035–040/052: route isolation, native request preservation, Responses continuation, tools, fallback/deadline/correlation, and safe error projection | `skald-workflow/src/route.rs`, `attempt.rs`; `skald-agent/src/request_builder.rs`, `session.rs`, `loop_runtime.rs`; `skald-spec` Responses wire types | Recorded `isolated_route_calls`, `loop_responses`, structured-output, and full Skald lane | PASS |
| REQ-042/049, INV-012/017, AC-016/023: one screened ExtGateway transport and no bound credential in a result or error | `skald-workflow/src/route.rs:463-502`; `skald-providers/src/clients/external.rs:68-235` | `external_gateway_success_reflection` and `bound_external_gateway_security` rerun: 2/2 passed, including escaped/structured reflections, clean success, refusal, and no-retry controls | PASS |
| REQ-053: Observer deletion, payload-free hierarchy, retries as sibling attempt spans, stable failure codes, semantic provider/model fields, and retained timeout behavior | Observer crate/hooks/exports deleted; `skald-agent/src/loop_runtime.rs:33-249,653-670,817-845,919-940`; `skald-workflow/src/workflow.rs:200-224,414-519,611-661` | Agent GenAI tests rerun: 2/2 passed; Workflow deadline/panic tests rerun: 2/2 passed; repository search found no Skald Observer surface outside historical changelog text | PASS |
| FIND-TASK-001-20 through FIND-TASK-001-27 close at their shared source without compatibility or a second owner | Fixed Agent-deadline arm; in-span panic projection; typed success reflection check; semantic GenAI metadata; in-place rustdoc; inherent Agent methods; standard resolver error; corrected permanent docs | Four Workflow and two Agent focused tests rerun green; recorded lint, docs, examples, Skald, shared, and Wyrd lanes green | PASS — all eight closed |
| No second engine, public compatibility alias, remote Python/TS/MCP surface, Observer replacement, credential administration, or new third-party dependency | Complete cumulative diff and manifests | Source/diff inspection; `git diff --check` exits zero | PASS |
| Test-harness correction for the red `test:wyrd` lane is root-cause scoped | `wyrd-testing/src/server.rs:185-241,745-774,3609-3638`; state token is the same token passed to Scribe, Oracle, Forge, and the Oracle reader epoch | Recorded diagnosis ties the SIGABRT to database-first drop; post-fix `mise run test:wyrd` reports 2282 passed, 161 skipped | PASS |

## Prior-finding closure

| Prior finding | Result | Closure evidence |
|---|---|---|
| `FIND-TASK-001-20` | CLOSED | `StepTask::run` races the fixed Agent deadline after higher-precedence arms and projects the existing retryable Agent timeout; held terminal journal futures are dropped. |
| `FIND-TASK-001-21` | CLOSED | The complete attempt future is caught inside the live attempt span; panic, step, run, and span share `WYRD_WORKFLOW_500_INTERNAL`. |
| `FIND-TASK-001-22` | CLOSED | The sole client owning sensitive bound headers refuses decoded retained success content containing a nonempty bound value before returning it. |
| `FIND-TASK-001-23` | CLOSED | Gemini and Vertex use `gcp.gemini` and `gcp.vertex_ai`; resolved Prompt models reach Agent and model-call spans; the unsupported scalar finish-reason field is absent. |
| `FIND-TASK-001-24` | CLOSED | The cited Agent entry points, helpers, fixtures, fields, and trait methods now document errors and lifecycle effects in place. |
| `FIND-TASK-001-25` | CLOSED | Stateful run orchestration and its state-dependent helpers are inherent `Agent` methods; only stateless conversions remain free. |
| `FIND-TASK-001-26` | CLOSED | `NoRemoteResolver` converts `std::io::Error` through `SchemaResolverError`; `skald-workflow` has no direct `anyhow` dependency. |
| `FIND-TASK-001-27` | CLOSED | The changed architecture, changelog, and Rust-example surfaces describe the implemented runtime and no longer list the deleted example or obsolete Workflow owner APIs. |

The earlier `FIND-TASK-001-1` through `FIND-TASK-001-19` closure evidence was
also checked against the cumulative candidate; no producer or sibling consumer
in the remediation range reopens one.

## Proposed findings

None.

## Verification notes

- Reran exactly six remediation-focused tests: four `skald-workflow` tests and
  two `skald-agent::agent_timeout` tests; all passed.
- Confirmed `git diff --check` for the immutable base-to-candidate range exits
  zero.
- Relied on the candidate's recorded broad evidence for `mise run test:skald`,
  `test:shared`, `test:wyrd`, lints, docs, examples, Python, codegen, and
  boundary lanes; this reviewer did not repeat those broad lanes.
- The harness fix has no new single-purpose test. Its proof is the diagnosed
  failure trace plus the complete post-fix `test:wyrd` rerun; the shared-token
  and field-drop ownership were additionally inspected in source.

## Overall result

**PASS** — no missing, incorrect, drifting, violating, or regressing invariant
was found in the cumulative candidate, and all prior round-three findings are
closed at their owning boundaries.
