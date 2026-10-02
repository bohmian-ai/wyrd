# Maintainer Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `a704a8890ef20efe65fee1e116f7d02288f8ec5c`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`,
  Revision 11, including REQ-053
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Round-three remediation:
  `changes/active/skald-workflow-runtime/review/TASK-001-r3/TASK-001-R2-close-round-three-runtime-gaps.md`

The candidate was exactly `a704a8890ef20efe65fee1e116f7d02288f8ec5c`
before and after source inspection. No `.codegraph/` directory exists, so
navigation used the cumulative Git diff, repository search, owning modules,
callers, tests, and generated declarations.

## Authority Coverage

This pass applied `AGENTS.md`, `architecture/agent-rules.md`,
`architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`,
`architecture/references/languages/spec-driven-development.md`,
`architecture/references/languages/maintainer-style.md`, the approved Revision
11 specification, the original task, the round-three verdict and validated
ledger, and the round-three remediation task. It reviewed the complete
base-to-candidate subject and treated every prior finding as a hypothesis to
retrace rather than as proof.

## Changed-Surface Coverage

| Surface | Symbols, callers, tests, and declarations inspected | Assessment |
|---|---|---|
| Workflow contracts and schemas | `WorkflowSpec`, `WorkflowStep`, bindings, routes, run/status/result/error DTOs, validation, generated JSON schemas, error declarations, and direct Rust/Python consumers | Public names, wire shapes, and generated declarations remain aligned. The contract stays in `wyrd-spec` and the execution owner stays in Skald. |
| Workflow planning and execution | `ExecutionPlan`, `WorkflowExecutor`, `StepTask`, `AttemptOutcome`, `RunLedger`, retry/deadline/cancellation order, output validation/budgeting, and focused Workflow tests | Owners and method shape are cohesive. The fixed Agent deadline and panic settlement remain on `StepTask`; helpers are narrow and deterministic. |
| Agent orchestration and Responses replay | `Agent::run`, `run_with`, `run_prompt`, `run_loop`, session/journal helpers, request reconstruction, GenAI span helpers, direct callers, `loop_responses`, structured-output, and timeout/telemetry tests | The full stateful lifecycle now lives on `Agent`; public behavior is unchanged. Entry points and changed helpers document errors, cancellation, terminal journaling, and model/provider metadata. |
| External gateway egress | `ExternalGatewayClient::{new,send,post,reflects_credential}`, `contains_any`, endpoint policy, route construction, provider errors, and reflection/security tests | Credential-containment logic remains localized to the existing client, uses retained typed response content, and adds no abstraction or dependency. Names and diagnostics are clear and redacted. |
| Rust/Python authoring surfaces | `Workflow`, `WorkflowBuilder`, SDK-owned PyO3 wrappers, public Python exports, exact `TypedDict` declarations, examples, and tests | The Rust owner and thin Python projection remain discoverable and type-aligned. No duplicate durable behavior or generated-declaration drift was found. |
| Observer deletion and tracing | Removed crate/hooks/exports/examples, Agent and Workflow span owners, telemetry tests, manifests, checks, and docs | REQ-053 remains implemented without a compatibility Observer surface. Span fields are payload-free and the changed Agent/Workflow tracing helpers have clear owners. |
| Test-harness lifecycle fix (`516d0fbcc`) | `WyrdTestServerInner` field order and construction, `WyrdTestServer::shutdown`, `Drop`, state and serve cancellation tokens, in-process/bound callers, and recorded `test:wyrd` failure diagnosis/rerun | The fixture is now dropped after runtime owners, and both explicit and implicit teardown cancel the state token used by in-process roles. The change is small, owner-local, documented, and matches the diagnosed lifetime bug. |
| Permanent documentation and examples | `CHANGELOG.md`, `docs/architecture/skald.md`, crate-root docs/manifests, Rust/Python example indexes and deleted Observer examples | Removed names and the deleted tracing example are gone, but the architecture page now contradicts its own corrected Workflow entry and the actual dependency graph (MNT-R4-001). |
| Verification artifacts | Exact focused selectors, scoped Skald/shared/Wyrd lanes, docs/examples, formatting/lints, and cumulative `git diff --check` | Recorded proof is broad and directly covers the remediation and harness failure. This review independently ran the cumulative diff check but did not rerun the full suites. |

## Prior-Finding Closure

| Prior finding | Closure evidence | Result |
|---|---|---|
| `FIND-TASK-001-20` | `StepTask::run` races the fixed Agent deadline after cancellation, total, and step deadlines; `agent_deadline_bounds_settlement` covers held terminal journaling, retry exhaustion, and precedence. | CLOSED |
| `FIND-TASK-001-21` | The complete attempt future is caught while `AttemptSpan` is alive; `attempt_panic_matches_span` proves the run, step, and span use the same internal error. | CLOSED |
| `FIND-TASK-001-22` | `ExternalGatewayClient` checks decoded retained content against sensitive header values and withholds decode/refusal details; focused tests cover reflected strings, escaped content, tool/structured members, clean responses, and no Workflow retry. | CLOSED |
| `FIND-TASK-001-23` | Agent spans derive canonical provider/model values from typed Prompt/request data and omit the invalid scalar finish-reason attribute; OpenAI, Gemini, and Vertex capture tests cover the result. | CLOSED |
| `FIND-TASK-001-24` | Cited Agent entry points, helpers, fixtures, fields, trait methods, and new Workflow fixtures now have substantive rustdoc with applicable errors and lifecycle behavior. | CLOSED |
| `FIND-TASK-001-25` | The two orchestration bodies and their state-using helpers are inherent `Agent` methods; the public `Agent::run` remains a thin default-registry convenience method. | CLOSED |
| `FIND-TASK-001-26` | The resolver refusal uses `std::io::Error` through `SchemaResolverError`; the direct `skald-workflow` `anyhow` dependency is gone. | CLOSED |
| `FIND-TASK-001-27` | Deleted Workflow APIs and the absent `tracing_stdout.rs` entry were removed from the three cited surfaces. A distinct contradiction introduced by the replacement architecture text is reported below. | CLOSED; NEW FINDING |

## Material Findings

### MNT-R4-001 — The corrected Skald architecture page contradicts the implemented dependency boundary

- Changed location: `docs/architecture/skald.md:3-5,28-38,44-58`.
- Governing principle: the maintainer guide requires permanent documentation to
  match the typed surface and dependency ownership; changed architecture text
  must let a maintainer identify the real crate boundary without reconstructing
  it from manifests.
- Evidence: the page says Skald depends on neutral infrastructure and Skald
  crates only, says Skald does not depend on any `wyrd-*` crate, later says
  Skald engine crates do not depend on `wyrd-spec`, and draws a graph with no
  Wyrd edge. The replacement Workflow entry on the same page says
  `skald-workflow` depends on `wyrd-spec`. That latter statement matches
  `architecture/wyrd-doctrine.mdx`'s service-boundary rule that `wyrd-spec` is
  the shared wire-contract crate on which each product depends,
  `crates/skald/skald-workflow/Cargo.toml:23-31` and
  `crates/skald/skald-workflow/src/lib.rs:26-29`, which identify the intentional
  foundational contract dependency while excluding Wyrd server and Vala.
- Concrete maintenance cost: the architecture authority gives mutually
  exclusive answers about whether the implemented Workflow crate may import
  its contract owner. A maintainer cannot use the page to evaluate a future
  Skald dependency or update its graph safely.
- Smallest testable correction: update the overview and dependency-direction
  paragraph to state the existing narrow rule—`skald-workflow` may depend on
  foundational `wyrd-spec`, while Skald does not depend on Wyrd server or Vala
  crates—and add the existing `skald-workflow -> wyrd-spec` edge to the graph.
  Reuse the wording already present in the crate-root independence docs; do not
  introduce another layer or change code.
- Proof: source inspection against the manifest and crate-root docs, followed
  by `mise run docs:check`.

## Uncertain Preferences

- `WyrdTestServerInner` uses declaration order to enforce fixture teardown.
  A custom `Drop` implementation on the inner struct could make that ordering
  more explicit, but the current field-order guard is documented, minimal, and
  already paired with state-token cancellation; this is not a finding.
- The long inline Workflow scenario module is sizable, but its shared setup and
  same-crate access serve the repository's test-layout rule. Splitting it would
  add test binaries without a demonstrated maintenance benefit.

## Verification Evidence and Limits

- The remediation record reports green focused tests for Agent deadline
  settlement, panic/span consistency, external credential reflection, and
  GenAI provider/model fields.
- It reports `mise run fmt`, `mise run lints`, `mise run test:skald`,
  `mise run docs:check`, `mise run check:examples`, `mise run test:shared`, and
  the post-fix `mise run test:wyrd` run (2282 passed, 161 skipped).
- The harness diagnosis identifies the observed SIGABRT, the uncancelled
  in-process state token, premature forced fixture-database drop, the exact
  owner-local correction, and a green full Wyrd rerun. No timeout, assertion,
  ignored test, or gate was weakened.
- This review independently confirmed that
  `git diff --check a51af030b6039eea4b2914f3ebf2c31925d08721..a704a8890ef20efe65fee1e116f7d02288f8ec5c`
  exits zero. It did not rerun the recorded compilation or test suites.
- Green docs checks do not prove semantic consistency between prose, manifests,
  and crate-root ownership docs; that is the remaining gap in MNT-R4-001.

## Overall Result

**FAIL**

All eight round-three findings are closed, the runtime owners and public
declarations are maintainable, and the test-harness lifecycle fix is a focused
root-cause correction. One bounded documentation contradiction remains on the
task-changed Skald architecture boundary (`MNT-R4-001`).
