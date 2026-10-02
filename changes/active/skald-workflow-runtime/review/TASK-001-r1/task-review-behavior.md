# Task behavior review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `eb22b03f2bb766886d839bda23aafbd4ba130ab3`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, revision 9
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Review scope: complete cumulative base-to-candidate diff

## Proposed findings

### Important

#### BEH-001 — INCORRECT: complete-run size accounting undercharges escaped text

- Violated obligation: REQ-017, REQ-045, INV-023, AC-020, and Scenario 6 require `max_run_bytes` to bound the JCS UTF-8 serialization of the complete `WorkflowRun`, with no oversized provider payload retained.
- Location: `crates/skald/skald-workflow/src/attempt.rs:34-41`; consumed by `crates/skald/skald-workflow/src/run.rs:128-150`.
- Evidence: `StepPayload::charged_bytes` charges a text result with `String::len()`, while `RunLedger` treats that charge as the complete incremental serialized cost before retaining the text. JCS/JSON escaping can expand a Rust string: for example, each U+0000 byte is serialized as `\u0000`. The terminal reserve contains `text: null`; replacing that four-byte literal with a long escaped string can therefore grow the complete snapshot by substantially more than the raw UTF-8 length charged here. The focused `terminal_budget_reserve` test passes, but its payloads do not exercise escape expansion and so do not falsify this path.
- Observable consequence: a run can return `Succeeded` with `jcs_len(run) > max_run_bytes`, violating the configured retention/admission ceiling and the public snapshot invariant.
- Required testable correction: on the existing `RunLedger` accounting boundary, charge the exact JCS delta of each candidate step result against the payload-free representation (or directly validate the candidate complete snapshot plus remaining reserve) before retaining it. Add one focused case using control characters whose serialized representation expands and assert either the exact bound is respected or the payload is discarded with `WYRD_WORKFLOW_413_RUN_TOO_LARGE`.

#### BEH-002 — INCORRECT: OpenAI Responses tool-loop history deliberately drops native reasoning items

- Violated obligation: REQ-039 and Scenario 4 require OpenAI Responses to retain its native request dialect and work through the supported route combinations; the task explicitly requires all five request dialects to retain their native shape through Agent loops.
- Location: `crates/skald/skald-agent/src/request_builder.rs:162-171`; the behavior is asserted in `crates/skald/skald-agent/tests/loop_responses.rs:107-137`.
- Evidence: `assistant_message` filters every `OpenAiResponseItem::Reasoning` out of the response before rebuilding the next Responses request. The adjacent comment states that the wire type drops the item ID, and the new test expects the second request to omit the reasoning item. Thus the test codifies a reduced subset rather than proving native-history preservation. This is reachable whenever an OpenAI Responses reasoning model emits a reasoning item before a tool call.
- Observable consequence: the next tool-loop request is not the provider-native continuation returned by the prior response; reasoning-model continuations that require their reasoning item/identity can be rejected or lose required context even though this candidate advertises Responses loop support.
- Required testable correction: retain the native reasoning item fields needed for replay in the existing typed Responses wire owner and include those items, in order, when rebuilding stateless loop history. Update the focused loop test so the second request contains the returned reasoning item together with the function call and function-call output. Do not switch to or overwrite an authored `previous_response_id` implicitly.

#### BEH-003 — VIOLATION: the fixed external-binding Rust seam was changed without approved authority

- Violated obligation: the task's packet-local seam and the approved specification fix `ExternalGatewayBinding.secret_headers` as `BTreeMap<HeaderName, SecretString>`; REQ-051 requires the contract and direct consumers to move together without an unapproved API substitution.
- Location: `crates/skald/skald-workflow/src/route.rs:95-112`.
- Evidence: the candidate exposes `HashMap<HeaderName, SecretString>` instead. The task's appended implementation evidence acknowledges this as a deviation because `HeaderName` is not ordered, but implementation evidence is below the approved specification and ready task in the authority order and cannot revise the fixed public seam.
- Observable consequence: Rust callers written to the approved constructor shape do not compile, and the delivered public runtime contract differs from the task being accepted.
- Required testable correction: resolve the impossible ordered-key contract through approved authority before acceptance, then implement and compile-test exactly the resulting public field type at every constructor. Silently retaining the `HashMap` deviation is not task completion.

#### BEH-004 — VIOLATION: `ProviderError::RemoteProblem` does not have the fixed variant shape

- Violated obligation: the approved specification and packet-local seam fix `ProviderError::RemoteProblem` as a struct variant containing `code`, `status`, `message`, `field`, and `remediation` directly; REQ-051 requires direct error consumers to move with that contract.
- Location: `crates/skald/skald-providers/src/error.rs:59-87`.
- Evidence: the candidate instead adds a separate public `RemoteProblem` struct and exposes the tuple variant `RemoteProblem(Box<RemoteProblem>)`. The task's implementation evidence calls the boxing a deviation made for `clippy::result_large_err`; no approved spec/task revision authorizes that different construction or match shape.
- Observable consequence: downstream callers cannot construct or exhaustively match the approved variant, so the candidate does not deliver the fixed Rust seam even though its runtime projection fields are equivalent.
- Required testable correction: resolve the exact-shape versus lint constraint through approved authority, then use the approved variant shape throughout all direct constructors and match consumers and prove it with the provider-error compile/tests. A locally documented implementation deviation cannot replace that decision.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Scenario 1: exact Workflow contracts, Agent-only graph, binding grammar, graph/input/resolved-Prompt validation (REQ-001–005, REQ-007–009, REQ-012–013A, REQ-036–037, REQ-040, REQ-045; AC-006) | `wyrd-spec/src/card/workflow.rs`; `skald-workflow/src/plan.rs` and `workflow_surface.rs` | Recorded `explicit_workflow_contract` and `resolved_bindings_reject_before_dispatch`; source inspection confirms pre-dispatch validation and Agent-only actions | PASS |
| Scenario 2: namespaced dependency data, exact binding conversion/output projection, deterministic complete results (REQ-006, REQ-010–012, REQ-020–023; AC-005, AC-007, AC-008) | `skald-workflow/src/workflow.rs`, `run.rs`, `plan.rs` | Recorded `explicit_namespaced_results`; source inspection confirms BTreeMap snapshots, declared binding selection, and plan-order primary errors | PASS |
| Scenario 3: bounded concurrency, retry/deadline classification, ordinary drain, cancellation and owned task lifetime (REQ-016, REQ-018–019, REQ-043, REQ-047–048; AC-020) | `workflow.rs` owned `JoinSet`, `attempt.rs` classification, `run.rs` settlement | Independently reran `workflow::tests::bounded_attempt_lifecycle`: 1/1 passed | PASS |
| Scenario 4: exact route dependency/call interfaces, isolated immutable call context, all five native dialects, safe remote projection (REQ-035–040, REQ-043, INV-020; AC-011A) | `skald-workflow/src/route.rs`; `skald-providers/src/error.rs`; `skald-agent/src/request_builder.rs` | Recorded `isolated_route_calls`; independently reran Responses loop test: 1/1 passed, but that test explicitly expects reasoning history to be dropped; BEH-002 and BEH-004 remain | FAIL |
| Scenario 5: one scoped ExtGateway transport, exact binding contract, header/origin/protocol checks, DNS screening/pinning, no redirects/proxy (REQ-042, REQ-049, INV-010/010A/017; AC-016, AC-023) | `skald-providers/src/endpoint.rs` and `clients/external.rs`; `skald-workflow/src/route.rs`; gateway consumers moved to provider owner | Recorded `bound_external_gateway_security` and gateway endpoint tests; source inspection confirms single endpoint-policy implementation, but BEH-003 violates the fixed Rust binding seam | FAIL |
| Scenario 6: exact input/step/full-snapshot JCS budgets and guaranteed terminal reserve (REQ-017, REQ-045, INV-023; AC-020) | `plan.rs::resolve_input`, `attempt.rs::StepPayload`, `run.rs::RunLedger` | Independently reran `workflow::tests::terminal_budget_reserve`: 1/1 passed; BEH-001 identifies an uncovered escaping path that defeats the complete-run ceiling | FAIL |
| Scenario 7: Rust/Python explicit `with_*` authoring, shared engine, portable `outputs`/`steps`, declared local tools (REQ-024, REQ-047, REQ-051; AC-024, AC-026) | `workflow_surface.rs`, `python.rs`, generated Python exports/stubs and unit tests | Recorded `explicit_builder_contract`, `test_explicit_workflow_bindings`, Python lanes and Python-feature compile | PASS |
| Public run/error schema, UUIDv7 IDs, snake-case statuses, default-empty maps, deny-unknown request/config DTOs, derive-backed codes | `wyrd-spec/src/card/workflow.rs`, `ids.rs`, `error.rs`, checked-in schemas, TypeScript error codes | Recorded `codegen:check`, `test:shared`, `test:wyrd` | PASS |
| Existing consumers compile in the same cohesive change; no contract-only handoff (REQ-051, AC-024) | Agent/provider/gateway/client/Vala-facing consumers are included in the cumulative diff | Recorded `test:skald`, `test:shared`, `test:wyrd`, all-feature lints, and retained Python feature check; fixed-seam deviations BEH-003/004 mean semantic closure is incomplete despite compilation | FAIL |
| Non-goals: no remote Python/TS/MCP Workflow runtime, compatibility alias, migration layer, new crate/dependency, Vault/Operator policy migration, or second engine | Diff introduces no such surface; endpoint policy move stays in existing crates and local Python remains a thin wrapper | Diff/manifests and caller inspection | PASS |

## Verification notes

- Candidate remained `eb22b03f2bb766886d839bda23aafbd4ba130ab3` at report completion.
- Independently executed:
  - `mise exec -- cargo nextest run --locked -p skald-workflow --lib -E 'test(=workflow::tests::terminal_budget_reserve) | test(=workflow::tests::bounded_attempt_lifecycle)'` — 2 passed.
  - `mise exec -- cargo nextest run --locked -p skald-agent --test loop_responses -E 'test(=agent_run_executes_openai_responses_tool_loop)'` — 1 passed.
- The task records all remaining focused and scoped lanes as passing. This review did not rerun the full broad set; green lanes do not close the source-confirmed gaps above.

## Overall result

**FAIL**

The candidate implements most of the explicit local runtime, but it does not satisfy the original task exactly: complete-run accounting can retain an oversized snapshot, Responses loop history intentionally drops a native item class, and two fixed public Rust seams were changed only in implementation evidence rather than approved authority.
