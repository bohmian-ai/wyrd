# Task 001 independent invariant review

Result: **FAIL**.

Subject: base `08b38f74255d344e1c5ec9a6d278935a26277622`, candidate `896e563bee69ab6b5f47b31977d7de1a0ca88d57`; approved specification revision 6 and original task snapshots beside this report. HEAD matched the candidate during inspection. Uncommitted documentation/planning files were excluded. No other discovery report or implementation conversation was read. Source was not edited.

## Proposed findings

### INV-001 — MISSING: approved tool permission contracts are absent

- Obligation: TASK-001 scope, reuse map and Scenario 5; specification REQ-008 and INV-003. Both global `tool:invoke/all` and exact built-in-name tool permissions must round-trip, validate, cover and intersect through the existing permission owners.
- Location: `crates/wyrd-spec/src/auth/permission_scope.rs:44` (`PermissionScope`); `crates/shared/wyrd-runtime/src/permission.rs:73` (`Resource`) and `:333` (`Permission::validate`). These required owners are unchanged in the complete candidate diff.
- Evidence: `PermissionScope` contains only All/Bifrost/Gateway/Verifier; `Resource` contains no Tool variant. Consequently both approved permission examples fail decoding (resource `tool` is unknown; the scoped form additionally has an unknown `tool` scope variant). This is not a runtime implementation deferred to another task: Scenario 5 explicitly assigns these pure contracts and runtime coverage/intersection semantics to this task.
- Producer-to-consumer trace: role JSON is decoded as `Vec<Permission>` by `wyrd-auth/src/issuance.rs:797`; `PermissionWire` delegates to `Permission::validate`; `PermissionSet::covers` and `intersection` consume typed scope coverage; `Principal.effective_permissions` and `AcceptedAuthority` serialize the same set across deferred work. All these sibling consumers share the missing resource/scope producer. There is no alternate typed representation satisfying the requirement.
- Consequence: parallel hosted runtime work cannot express the required invocation authorization or preserve a named tool grant through accepted authority without inventing another contract. Existing domain grants cannot substitute.
- Smallest testable correction: extend the existing resource and typed scope owners with the approved closed built-in-name tool scope and invoke-only validation, using their existing coverage/intersection and serialization machinery. Preserve existing wildcard, Bifrost and gateway behavior. Add pure cases for both wire forms, unknown names, wrong resource/action/scope, exact-name mismatch, disjoint names, global/exact intersection, and role/accepted-authority round-trips. No new permission checker, grant store, or policy mechanism.

### INV-002 — INCORRECT: registration rejects four approved built-ins

- Obligation: REQ-008's closed initial catalog and TASK-001 Scenario 3's declaration/position validation.
- Location: `crates/wyrd-spec/src/card/blocks/validate.rs:30` and `:509`.
- Evidence: `STEP_TOOLS` permits only `fs.list`, `fs.read`, `fs.search`, `code.run`, `cards.get`, `bifrost.query`. The approved catalog also includes `cards.list`, `cards.get_by_ref`, `bifrost.list_tables`, `bifrost.describe_table`. A step Agent selecting any of these four, with an empty workspace and otherwise matching Prompt inputs, deterministically gets `agent_tools`.
- Producer-to-consumer trace: `AgentSpec.tool_names` is the existing authored source; `VerifierSpec::validate`/`validate_with` resolve inline or registered Agents into `validate_agent`; the local subset rejects their names before runtime registry resolution. Check-position tool refusal is a separate correct sibling rule and must remain intact.
- Consequence: approved read-only preparation calls cannot be authored/registered, even when runtime capabilities and caller grants exist.
- Smallest testable correction: use the complete approved built-in vocabulary, preferably the same closed vocabulary needed by INV-001 rather than another independent name list. Prove each approved name is allowed in a step without unrelated workspace requirements, and unknown names and check-position tools remain refused.

### INV-003 — INCORRECT: check-Agent schema validation accepts incompatible outputs

- Obligation: REQ-005 requires registration to refuse check Agents whose derived response schema violates the closed Verdict contract; TASK-001 Scenario 3 requires position/schema compatibility.
- Location: `crates/wyrd-spec/src/card/blocks/validate.rs:543`–`:557`.
- Evidence: validation checks only whether `required` contains three strings. It does not inspect root/object shape, property types, closed outcome values, citation structure, additional properties, or extra mandatory fields. For example a structurally valid object schema requiring `outcome`, `reason`, and `citations`, with `outcome: {"const":"yes"}`, `reason: {"type":"integer"}`, and `citations: {"type":"string"}`, passes this branch. No such response can deserialize to `verification::execution::Verdict` (`execution.rs:323`). Even the passing test fixture `verdict_schema` at `validate.rs:937` leaves outcomes unrestricted and citations untyped.
- Producer-to-consumer trace: the existing shared Prompt compiler derives `ResponseType::JsonSchema` and checks JSON Schema syntax (`skald-spec/src/prompt.rs:235,399`), not the Verifier-specific Verdict contract. `validate_agent` is the owning registration boundary for that extra invariant. `CheckExecutor` returns a typed Verdict and malformed provider output must become inconclusive later; that runtime refusal does not satisfy the explicit registration refusal. Both inline and resolver-supplied registered Agent consumers use this same branch.
- Consequence: authors can successfully register a check whose declared structured output is incompatible with every valid judgment, then incur provider execution and receive an inconclusive malformed-output result instead of the required early validation error.
- Smallest testable correction: validate the derived check schema against the canonical `Verdict`/`Citation` contract already defined in `verification::execution`, including closed outcome and nested locator shapes, rather than maintaining only a field-name list. Preserve shared Prompt compilation. Add focused negative cases for wrong property types, invalid outcome constraints, malformed citation items and incompatible extra required fields, plus a canonical-schema positive case.

### INV-004 — MISSING: tool-result provenance cannot carry required digest and byte count

- Obligation: REQ-008: tool names, arguments, output digests, and returned byte counts are recorded; TASK-001 must publish the result/provenance boundary so parallel consumers do not invent durable fields.
- Location: `crates/wyrd-spec/src/verification/execution.rs:310` (`ToolCallRecord`).
- Evidence: the record has only `tool`, `arguments`, and optional `error`; no field represents the output digest or returned byte count. It is the only tool-call record contract in the candidate. `vala-eval/src/blocks.rs:85` returns these records on `ActionOutput`, and `StepResult.tool_calls` (`execution.rs:292`) persists/projects the same type. `CallProvenance` has no equivalent tool output information.
- Consequence: the published handoff loses the required tool result provenance; successful tool calls cannot be attributed to the exact bounded output returned using this contract. Hosted runtime and SDK tasks would have to extend the public result type independently.
- Smallest testable correction: add the approved output-digest and returned-byte-count data to this existing record, using existing digest types and a byte count. Make success/error meanings explicit without storing unrestricted tool output. Add a serde/schema round-trip proving these fields survive `ActionOutput` → `StepResult` → result projection. No new evidence wrapper or log sink.

### INV-005 — INCORRECT: recursive schemas can exhaust the stack during pure validation

- Obligation: REQ-003's safe known-schema reference validation, INV-001's pure foundational validation and INV-007's bounded work; unknown shapes must remain usable.
- Location: `crates/wyrd-spec/src/card/blocks/schema_path.rs:26`–`:28`, with the ineffective per-call bound at `:54`–`:61`.
- Evidence: a JSON slot schema `{"anyOf":[{"$ref":"#"},{"type":"object"}]}` is accepted by `validate_slot`. A requirement assertion selecting `inputs.x.field` calls `admits(root,root,[field])`. The first branch calls `admits` on `$ref:#` with the unchanged path; `resolve` returns the original root, and `anyOf` immediately repeats the same branch. The local 32-hop limit resets on every call and does not count these branch-mediated reference hops. This is an infinite recursion without consuming a selector; the second branch is never visited.
- Producer-to-consumer trace: JSON input schema is caller-authored pure `Value`; `validate_slot` only requires object/bool; `validate_reference` sends it into `admits`. The same walker also consumes resolved Agent output schemas, so the correction belongs in this shared schema-path walker, not only at the input caller. Ordinary recursive property schemas consume selectors and must remain supported.
- Consequence: validating one authored Verifier can overflow the calling thread's stack and abort its process, instead of admitting an undecidable path or returning a structured refusal. The candidate's types are staged, so this is immediately reachable through their required public `validate` API and becomes a registration-process hazard at activation.
- Smallest testable correction: make the existing walk bounded across recursive branch/ref traversal, conservatively treating an unresolved/revisited state as unknown according to its documented behavior. Retain known-missing-field refusals. Add an IO-free test for this branch/reference cycle and ordinary recursive schemas with finite selectors; no replacement schema engine or second graph store.

## Obligation matrix

PASS below means the task-scoped declaration/validation/handoff obligation is implemented; it does not claim deferred runtime execution has shipped. Each row assesses the complete candidate, not only the last commit.

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 subject-less four-part authoring; defaults and empty requirements representable | `card/blocks/mod.rs:42` VerifierSpec, default Decide | `verifier_spec_round_trips_and_validates` | PASS |
| One global namespace across step and requirement IDs | `validate.rs:150` requirement_owners called before planning | duplicate-ID negative cases in `verifier_spec_refuses_invalid_graphs_and_references` | PASS |
| REQ-002 JSON/media/trace/samples; required default, optional and named slots | `mod.rs:71` InputSlot, SlotName in `ids.rs` | slot grammar/round-trip/default tests; runtime missing-evidence handling deferred | PASS |
| REQ-002 slot declaration shape and lower positive media limits | `validate.rs:740` validate_slot | empty formats and zero-size rejection cases | PASS |
| REQ-003 approved reference roots/array indexes; no bare output/`$.`; wildcard position | `reference.rs:132` parse, `validate.rs:643` validate_reference | parser malformed/round-trip and wildcard negative cases | PASS |
| REQ-003 dependency inference, ordering-only edges, cycles and position rules | `validate.rs:118` plan/dependencies, ancestors; shared `plan_stages` | graph, inaccessible requirement, cycle and stage-order cases | PASS |
| REQ-003 known-schema field checks with safe recursive/unknown behavior | `schema_path.rs:19`, `validate_reference`/output_schema | existing finite selector test misses branch/ref cycles; INV-005 | FAIL |
| REQ-004 closed extract/agent/code action kinds and Python-only runtime | `mod.rs:181,225` | serde closed-kind tests; no runtime added | PASS |
| REQ-004 all locked extraction types, explicit nullable fields and no duplicate counts | `extract.rs:16` ExtractOutput and nested typed values | every format round-trips; missing/extra fields and selected format schema tests | PASS |
| REQ-004/009 extraction provenance and original-input citations expressible | `execution.rs` CallProvenance::Extract/Citation/Locator; `blocks.rs:58` CallInput.sources | result round-trips all locator forms; executor handoff test | PASS |
| REQ-005 assert/trace/agent/code/drift check kinds and four requirement outcomes | `mod.rs:273` Check; execution.rs RequirementOutcome/Verdict | Run and Window examples; result contract test | PASS |
| REQ-005 check Agent has neither tools nor workspace | `validate.rs:478,536` | fresh exact `verifier_spec_enforces_agent_positions_and_media` passed | PASS |
| REQ-005 check Agent derived schema is the closed Verdict shape | `validate.rs:543` only tests required names | wrong-type/enum/citation cases absent; INV-003 | FAIL |
| REQ-006 decision default and finite positive bounded threshold; missing/not-applicable statuses expressible | Decide, RequirementOutcome, StepResult/StepStatus | pass-rate bounds and result tests; aggregation belongs to task 2 | PASS |
| REQ-007 derive placeholders from shared Prompt, exact bidirectional input mapping | `validate.rs:566` validate_placeholders using resolved Prompt | missing/extra text/media input negative tests | PASS |
| REQ-007 native media format and portable block-kind compatibility | MediaFormat.native_kind; Prompt.media_kind; validate_placeholders | positive and mismatched image/document rejection in fresh Agent test | PASS |
| REQ-007 exact identity/effective route and override contracts | VerificationRequest.execution_route_overrides, CallProvenance::Agent | request/result round-trips; server gateway/offline execution deferred | PASS |
| REQ-008 selected built-in names cover the approved catalog | `validate.rs:30` six-name subset | INV-002; missing four positive catalog cases | FAIL |
| REQ-008 existing tool resource/scope grants, validation and intersection | missing Tool resource/scope; unchanged existing owners | no Scenario 5 tests or implementation in candidate; INV-001 | FAIL |
| REQ-008 workspace declaration/position rules and engine call lifetime | AgentCall.workspace; valid_workspace_path; BlockCall.position/deadline; executor per-execution authority contract | path traversal and workspace/tool position tests; executor signature test | PASS |
| REQ-008 tool result digest and returned-byte record | ToolCallRecord only name/arguments/error | no round-trip can carry required fields; INV-004 | FAIL |
| REQ-010 shared canonical media tokens/MIME aliases and legacy/archive distinctions | `skald-spec/media.rs` MediaFormat | token/MIME/canonical/legacy/native tests; sniffing is explicitly task 3 | PASS |
| REQ-011 Run/Window refusal and no mixed evidence/preparation in Window mode | `validate.rs:255` validate_evidence_mode | mixed slots/checks and Window step negative cases; submission facade deferred | PASS |
| REQ-012 generic execution identity/request/evidence plan/status/result declarations | `ids.rs` ExecutionId; `verification/execution.rs` | UUIDv7, all Run input declarations and documented result tests | PASS |
| REQ-013 asynchronous conflict error representable using existing public error owner | VerificationExecutionState + From<&WyrdError> for VerificationError | idempotency conflict code/message test | PASS |
| REQ-014 explicit scheduled selectors, required-slot coverage and measurement consistency | ScheduledInput; validate_schedule; validate_drift | drift feature and exact schedule mapping cases | PASS |
| REQ-015 budgets and context-capture declaration | Step timeout/retries, MAX_STEP_RETRIES, EvalContextCapture | zero timeout/excess retry negative cases; server deployment settings deferred | PASS |
| Existing Card/Prompt loaders/reference visitors are retained | `refs/mod.rs:356` Visit reuses SlotEntry/SlotValue/visit_agent/DriftSpec visitor | fresh exact reference inventory test passed | PASS |
| Existing shared planner/operator owners reused; offline scenario evaluation preserved | validate_dag delegates plan_stages; old validate_task_operator delegates ComparisonOperator::validate | original tests included in reported library run; diff preserves old task routing/scenario structures | PASS |
| Parallel action/check data, provenance, position, cancellation and error boundary | `vala-eval/src/blocks.rs` narrow injected interfaces; owner-approved removal of JudgeError dependency | action executor contract test reported green; missing tool result fields separately fail above | PASS |
| IO-/async-/PyO3-free foundation, no endpoints/storage/scheduler/sandbox/transport/new Card kind | entire 21-file diff; async signatures only in vala-eval | check:deps, Clippy/check reported green; static imports/manifests show no dependency addition | PASS |
| New staged contracts do not activate two authored Verifier protocols; final switch handed off | `card/mod.rs` staged blocks; existing Spec/Verifier routes remain original; handoff table in task snapshot | compile checks reported green; no alternate live route | PASS |
| Complete typed boundary needs no further durable field decisions by parallel tasks | absent tool scope plus incomplete ToolCallRecord | INV-001/004 | FAIL |
| Required source-driven generated output proof | schema derives added; new blocks not registered in generator; existing ParameterName wire shape unchanged | orchestrator subsequently reports fresh codegen:check passed with generated/production outputs unchanged | PASS |
| No broadened application runtime, arbitrary plugins, external writes, new durability or historical reinterpretation | declaration-only task write set; existing owners unchanged | static diff review; integration intentionally deferred by task | PASS |

## Coverage and reuse assessment

Inspected all 21 changed files and their relevant surrounding owners. CodeGraph was used before code discovery, followed by source/diff verification. Expanded beyond the shared map into Prompt's schema compiler, existing `TraceAssertionTask`/`JsonPath`/TaskId, old Eval planner/operator consumers, existing Permission/PermissionSet/Principal/AcceptedAuthority and issuance role decoding. Existing reference visitor traversal covers inline Agent Prompt and verified_by references as well as code Artifact and drift baseline references. No second Card loader, permission checker, durable graph store, transport or runtime was added. The new scope-reference parser serves the explicitly approved different reference dialect; the generic planning extension preserves the old Eval call path.

Sibling-consumer audit: schema-path checks are shared between input schemas and Agent/extract output schemas (INV-005); check schema validation is shared between inline and resolver-provided Agents (INV-003); tool records flow through ActionOutput and StepResult (INV-004); role issuance, principal permissions, intersections and accepted authority all rely on the same missing permission producer (INV-001). Corrections should occur at these owners rather than repeating consumer guards.

No optional style findings are proposed. Scope/export staging is authorized by the task. No active protocol replacement, runtime sandbox, runner, queue, client facade or full journey is claimed complete at Task 001; those deferred obligations are not reported as missing here.

## Verification notes

Fresh execution:

```text
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::blocks::validate::tests::verifier_spec_enforces_agent_positions_and_media) | test(=card::blocks::schema_path::tests::schema_path_refuses_only_known_missing_selectors) | test(=refs::blocks_tests::blocks_verifier_yields_every_reference_slot)'
3 tests passed, 863 skipped.
```

These passing cases do not cover the five findings. INV-005 is established by the unchanged-path recursive source trace; no process-aborting reproduction was run. The task reports 1069 library tests, all-features Clippy, workspace tests compile, fmt and dependency checks green; those are implementer evidence, not fresh execution by this reviewer. The orchestrator additionally reports a fresh 1069/1069 library run and a passed codegen:check with generated/production outputs unchanged; those checks were not independently rerun here. Full integration/journeys are explicitly deferred by the task. No tests were weakened or production fixes made during this review.

Skipped broad builds, full runtime journeys and crash reproduction. Remaining risk: the required contract and registration validation gaps above prevent task acceptance despite the focused tests passing.
