# Task 001 behavior review

Overall result: **FAIL**.

## Immutable subject and scope

Reviewed the complete `08b38f74255d344e1c5ec9a6d278935a26277622` → `896e563bee69ab6b5f47b31977d7de1a0ca88d57` change, against specification revision 6 and the original Task 001 snapshots beside `subject.md`. HEAD matched the candidate; the working-tree differences were documentation/planning outside the production subject. No other reviewer reports were read. No production files were changed.

Authority consulted: supplied AGENTS.md and its repository rules, `architecture/agent-rules.md`, Wyrd design/doctrine, maintainer style, spec-driven-development, and relevant evaluation/testing references. The approved replacement and staging instructions govern this task's new vocabulary; final live activation and obsolete-type removal remain Task 005 work. The expressly approved removal of JudgeError from these new seams is accepted, without proposing removal of offline scenario evaluation.

## Independently expanded navigation map

CodeGraph was used first for Verifier validation/media discovery and then Agent/registrability discovery. Its symbol snippets were supplemented with complete relevant source and the cumulative git diff.

| Producer / owner | Consumer path checked | Existing alternative / reuse assessment |
|---|---|---|
| `card/blocks/mod.rs`, `reference.rs` | authored JSON → Reference parser → `VerifierSpec::validate[_with]` → `plan` | New approved authoring vocabulary; Card envelope remains unchanged and inactive. Existing TaskId and typed Prompt parameters reused. |
| `card/blocks/validate.rs`, `schema_path.rs`, `extract.rs` | slot/action/check validation → dependency discovery → stage planning → position checks and known output selection | Existing `vala::eval::plan_stages` is used; no second execution orchestrator. Schema walker is pure registration analysis, not an execution engine. |
| `refs/mod.rs` | block visitor → Agent refs → inline Agent Prompt refs; Artifact source and drift baseline visits | Existing Visit/SlotEntry mechanisms extended, including nested Prompt traversal. No parallel Card loader. |
| `vala/eval/{plan,operator,spec}.rs` | old `validate_dag` / EvalSpec → generic planner and shared regex validation | Existing offline tasks retain their old entry point and operator semantics. Generic extraction preserves existing graph behavior. |
| `skald-spec/{media,prompt}.rs` | format aliases → canonical token/native kind/extractability; Prompt compiler → derived placeholders/media kind → block validation | Media formats now have a single pure owner; compiler still derives placeholder/output information. |
| `verification/execution.rs`, `ids.rs` | request/input declaration → result/status/error serialization; identity and citations | Reuses stable verification status, errors, UploadPlan, digest and trace IDs. Result is staged, not mounted as a live alternate protocol. |
| `vala-eval/blocks.rs` | resolved values/source slots/deadline/position → injected action or check → output/provenance | Mirrors existing injected executor pattern; no execution implementation, queue or storage owner introduced. |
| `auth/permission_scope.rs`, `wyrd-runtime/permission.rs` | permission decode → validate → scope coverage → PermissionSet intersection; role/forwarding use the same Permission serde contract | These are the existing owners required by Scenario 5, but neither was extended. |
| Existing Agent / Prompt owners | `Prompt::new`/`check_output_schema`; `Agent::validate_registrable`; inline and registered references | Prompt validates general schema structure, not Verdict compatibility. Current runtime-local Agent registrability restriction remains a later integration consideration; this report does not require activating a second Agent protocol. |

## Acceptance matrix

PASS below is scoped to Task 001's pure contracts and handoff, not a claim that the complete five-task capability ships now. Verification references are inspected source tests and reported evidence, not fresh execution.

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001: declarative subject-less authoring, optional steps and requirements, global block namespace | `blocks/mod.rs:42`; `requirement_owners` includes both requirement categories and steps | `verifier_spec_round_trips_and_validates`, `verifier_spec_refuses_invalid_graphs_and_references` | PASS |
| REQ-002: named slot kinds, required default, optional/schema/media limits and scheduled capability contracts | `InputSlot`, `SlotName`, `validate_slot`, `validate_schedule`; wire declarations cover Run evidence | slot/name and schedule tests; actual input-value enforcement belongs to engine/server tasks | PASS |
| REQ-003: approved reference grammar, dependencies, cycles, wildcard and requirement-verdict positions | `Reference::parse`; `step_references`; `validate_reference`; shared `plan_stages` | reference tests and invalid graph/reference cases | PASS |
| REQ-003: known-missing output paths refused while unknown shapes remain legal | `output_schema` + `schema_path::admits`; extract schema narrowed by allowed formats | schema-path and graph/reference tests | PASS |
| REQ-004: exactly extract/Agent/code, immutable source and bounded step budgets | `Action`, `CodeCall`, `CodeSource`; `validate_step_shape`/`validate_code` | round-trip and invalid source/budget cases | PASS |
| REQ-004: all nine extraction formats and locked required/null/order/locator contract | `extract.rs`; schemas for DOCX union and XLSX cells; locator declarations | three extraction tests include all nine JSON examples and missing nullable-field refusal | PASS |
| REQ-005: all five check kinds and four requirement outcomes | `Check`, `RequirementOutcome`, `RequirementResult` | authoring and result round-trip tests | PASS |
| REQ-005/007: check Agent must be tool-less and return the closed Verdict schema | tool refusal exists; schema validation at `validate.rs:543` checks only required names | current positive fixture has only loose string/array fields; wrong-type schema cases absent | FAIL — BEH-003 |
| REQ-006: missing, skipped, failed and no-applicable cases can be represented; all/pass-rate thresholds | `RequirementOutcome`, `StepStatus`, reasons, `Decide`; finite `(0,1]` validation | pass-rate bounds and result examples; runtime skip propagation/aggregation belongs to Task 002 | PASS |
| REQ-007: derive Prompt placeholders/schema, exact two-way binding and media kind | resolved Prompt compiler values consumed by `validate_placeholders`/`output_schema`; no reauthored variables/response type | Agent position/media tests | PASS |
| REQ-008 / Scenario 3: approved built-in tools can be declared | six-entry `STEP_TOOLS` is incomplete and rejects four approved built-ins | test only covers fs.read/cards.get and unknown http.fetch | FAIL — BEH-002 |
| REQ-008 / Scenario 5: global/exact tool resource permission, strict decoding, coverage/intersection, authority encoding | neither `PermissionScope` nor runtime `Resource` has a Tool variant; no relevant diff | no tool-scope permission tests in candidate | FAIL — BEH-001 |
| REQ-008/015: tool name/arguments/output digest/returned byte count provenance | `ToolCallRecord` has name, arguments and optional error only | result example omits digest/count; no proof of these fields | FAIL — BEH-004 |
| REQ-009: original-slot citations and source propagation through ordinary outputs | `Citation`, `Locator`, `CallInput.sources`, `ActionOutput.output` | locator result examples and executor handoff test; runtime citation validation remains Task 003 | PASS |
| REQ-010: one canonical format vocabulary, aliases, distinct legacy Office, native/extract refusal | `MediaFormat`; native binding validation; extraction validation | four media tests plus Agent/extract negative cases | PASS |
| REQ-011: typed common Run request/output and closed evidence mode | `VerificationRequest`; validation refuses mixed samples/Run checks and Window preparation steps | request round-trip and mixed evidence tests; SDK entry points remain Tasks 004/005 | PASS |
| REQ-012: separate application Run and execution identities; status/result/error contracts | `ExecutionId`, optional RunId, `VerificationExecutionState`, staged routes | execution ID and status/conflict/result tests | PASS |
| REQ-013: asynchronous conflict representable without new synchronous reservation | existing status/errors reused; conflict conversion maps stable error; no runtime persistence code added | `verification_error_carries_idempotency_conflict`; acknowledgment execution deferred | PASS |
| REQ-014: explicit scheduled slot selectors and old drift configuration/statistics reuse | `ScheduledInput`; `validate_schedule`; `DriftCheck`/`RequirementDetail::Drift` | schedule and drift negative tests; scheduling runtime unchanged | PASS |
| REQ-015: attributable common result, subject/Verifier identities, checks and provenance | `VerificationResult`, `VerificationOutcome`, requirement/step records and `CallProvenance` | result round-trip; exception is missing tool output provenance noted separately | FAIL — BEH-004 |
| INV-001: pure IO/async/PyO3-free foundation and narrow engine boundary | serde/schema/validation in wyrd-spec; async traits solely in vala-eval | inspected imports and reported dependency check | PASS |
| INV-003: typed existing authorization remains the tool-contract owner | correct existing owner was identified but required extension is absent | no tool grant encode/attenuation cases | FAIL — BEH-001 |
| INV-006: honest evidence and missing/inconclusive distinctions in handoff | four requirement outcomes and block errors, original sources and locators | static enum/field proof; runtime downgrades deferred | PASS |
| INV-008: staged old types only for old consumers, no live dual protocol; offline scenarios preserved | `card/mod.rs` adds blocks module without switching Spec; old executor/judge remains; handoff lists final switches | full diff and consumers inspected; reported workspace compile check | PASS |
| AC-001: pure authoring/schema/registration negatives | new tests cover duplicate IDs, cycles, known/unknown paths, wildcard/position and evidence mode | relevant unit test bodies inspected; Verdict-schema and catalog gaps remain | FAIL — BEH-002/003 |
| AC-009: staged export and final cutover closure, not premature SDK/server/doc cutover | handoff lists staged contract exports and owning final switch locations | source does not mount new routes or register new schemas | PASS (task scope) |
| AC-010: required scoped verification and generated-contract check evidence | unit/lint/compile/fmt/dependency evidence supplied; codegen check explicitly not run | source does not yet register new schema roots; missing permission implementation also lacks required permission proof | FAIL — missing Scenario 5 proof; codegen lane is a verification gap |
| Task boundary: no new endpoints, storage, scheduler, sandbox or transport; no new Card/runtime/plugin/reference dialect | entire 21-file diff stays within pure contracts/minimum seams | static diff inspection | PASS |
| Task boundary: no aliases/compatibility features, empty runtime implementations or fake successes | new exports staged; no feature aliases or runtime implementations | full cumulative diff | PASS |
| Task acceptance: all parallel modules consume one complete typed boundary without inventing durable fields or semantics | action/check split, source slots, position, deadline and errors exist; tool grants and required tool-output provenance do not | injected action handoff test proves basic object-safe dispatch; no tool-permission/provenance completeness cases | FAIL — BEH-001/004 |
| Non-goals: no nested Verifiers/loops/human verdict/scenario action/rendering/new algorithms/external writes/new durable recovery | no such action/check/contract/runtime added | static closed-set and diff inspection | PASS |

## Proposed material findings

### BEH-001 — MISSING — Implement the approved tool permission contract

- **Obligation:** specification REQ-008 revision 6, Task 001 Scenario 5 and explicit auth ownership/write set; INV-003.
- **Location:** `crates/wyrd-spec/src/auth/permission_scope.rs:44`; `crates/shared/wyrd-runtime/src/permission.rs:76` and `:333` (omitted required changes in existing owners).
- **Evidence/path:** approved `{"resource":"tool","action":"invoke","scope":"all"}` cannot deserialize as Permission because `Resource` has no Tool variant. Exact `{"tool":"bifrost.query"}` cannot deserialize as PermissionScope either. The union ends at All/Bifrost/Gateway/Verifier and runtime validation has no tool branch. Neither file occurs in the complete cumulative diff. `PermissionSet::intersection` already delegates scope narrowing to `PermissionScope::covers`; the new scope belongs there rather than in an Invoker-local authorization subsystem. Role and forwarded authority decode the same validated Permission contract.
- **Consequence:** clients/roles/delegation cannot represent either approved grant form; downstream hosted tooling cannot implement the mandatory independent invocation gate against the frozen handoff. This is missing functionality, not evidence of a currently reachable permission bypass.
- **Smallest correction:** extend the existing scope/resource/action validation and coverage/intersection owners with the closed exact built-in name vocabulary and global tool/invoke grants. Preserve wildcard meanings, strict decode, attenuation and existing domain grants. Share the approved name authority with Verifier validation rather than adding a separate permission store/checker.
- **Focused proof:** pure permission tests decode/round-trip both approved wire forms, refuse unknown names/wrong resource-action-scope combinations, prove global/exact coverage, disjoint names and narrowed intersection, and prove role/accepted-authority encoding preserves exact scope. Include relevant runtime permission lint/check and generated schema proof.

### BEH-002 — INCORRECT — Approved built-ins are rejected at authoring validation

- **Obligation:** REQ-008's closed ten-tool initial catalog; Task 001 Scenario 3's declaration handoff.
- **Location:** `crates/wyrd-spec/src/card/blocks/validate.rs:30` and `:509`.
- **Evidence/path:** a resolved step Agent with a matching zero-placeholder Prompt, no workspace and `tool_names: ["cards.list"]` reaches the allowlist check and returns `agent_tools`. The same happens for `cards.get_by_ref`, `bifrost.list_tables`, and `bifrost.describe_table`. All four are explicitly approved. Inline and resolved registered/file Agents share `validate_agent`, so one correction closes all callers.
- **Consequence:** valid spec-r6 Verifiers cannot validate/register, preventing inventory/discovery before exact reads/query.
- **Smallest correction:** validate against the complete shared approved built-in catalog used by the typed tool permission extension. Keep unknown-tool refusal and check-position/workspace restrictions intact; do not add runtime callables or a second catalog.
- **Focused proof:** parameterize the approved built-ins through action-position Agent validation with valid workspace where required. Refuse unknown names and retain check Agent tool refusal. Specifically exercise all four previously absent names.

### BEH-003 — INCORRECT — Check schemas with incompatible Verdict fields pass validation

- **Obligation:** REQ-005: registration refuses check Agents whose derived response schema violates the closed Verdict contract; Task 001 Scenario 3.
- **Location:** `crates/wyrd-spec/src/card/blocks/validate.rs:543–552`.
- **Evidence/path:** the entire check is membership of outcome/reason/citations in top-level `required`. A valid Prompt JSON schema such as `{"type":"object","properties":{"outcome":{"type":"integer"},"reason":{"type":"boolean"},"citations":{"type":"string"}},"required":["outcome","reason","citations"],"additionalProperties":false}` passes both Prompt's general structural `check_output_schema` and block validation, although no conforming response deserializes as `verification::execution::Verdict` (`VerificationVerdict`, String, Vec<Citation>). Open extra fields and incompatible outcome enums similarly escape the check. The existing loose positive test schema also omits a closed outcome vocabulary and citation item shape. Both step and whole-Verifier checks share this branch.
- **Consequence:** registration accepts an Agent contract that cannot produce the required judgment; a successful model call still yields malformed output/inconclusive instead of the authored check behavior. Deferring malformed runtime output handling does not satisfy mandatory registration refusal of known incompatible schemas.
- **Smallest correction:** enforce compatibility of the derived Prompt response with the existing typed closed Verdict contract, reusing its schema source and existing schema facilities. Do not inject a hidden response schema or reintroduce authored response_type fields. Reject known wrong field types, incompatible outcome/citation shapes and extra-output contracts at this shared validation boundary.
- **Focused proof:** pure registration tests cover a genuinely compatible Verdict schema, wrong outcome/reason/citations types, incompatible outcome enum, malformed citation items and extra output fields, in both check positions. Valid action-position structured outputs remain unrestricted by Verdict rules.

### BEH-004 — MISSING — Shared tool provenance cannot carry output digest or byte count

- **Obligation:** REQ-008 requires recording tool names, arguments, output digests and returned byte counts; REQ-015 requires step tool provenance. Task 001 must hand consumers a complete shared result contract without new durable-field decisions.
- **Location:** `crates/wyrd-spec/src/verification/execution.rs:310–317`; producer `crates/vala/vala-eval/src/blocks.rs` ActionOutput.tool_calls; sink StepResult.tool_calls.
- **Evidence/path:** the only shared tool record fields are tool, arguments and optional error. ActionExecutor returns this exact type, and StepResult serializes the same type. There is no result digest or returned-byte field in either producer or sink; no sibling record is reused to carry them. The serialization test records only fs.read's input path.
- **Consequence:** the parallel runtime cannot return/persist the required explanation of what a tool supplied through the agreed typed handoff. Adding the fields later would require revising the contract after consumers begin implementing it, contrary to this task's handoff acceptance.
- **Smallest correction:** complete the existing ToolCallRecord with typed output digest and returned-byte count, projecting the same values from the executor output into the step result. Do not introduce a second evidence envelope or store the complete tool response merely to obtain this provenance.
- **Focused proof:** a pure result round-trip contains these values and an executor contract test demonstrates their passage from ActionOutput to the shared result record; preserve error-call representation.

## Verification and limits

No fresh build/test commands were run by this reviewer; no failure diagnosis or test suppression was introduced. Inspected the actual test bodies for authoring, reference parsing, schema-path walking, extraction outputs, format parsing, wire round-trips and executor input handoff. Available implementer evidence reports 1,069 library tests plus lint, workspace compile, formatting and dependency checks. The orchestrator subsequently reported an independent fresh library lane passing all 1,069 tests at the pinned candidate; that supports existing test behavior but cannot cover absent cases. No existing test covers tool permissions, the four omitted built-ins, incompatible Verdict field schemas, or output digest/byte-count provenance.

`codegen:check` was not run. New block/result schemas are staged outside live schema registrations, so this review does not claim demonstrated generated drift; repository-required contract generation remains unverified. Full integration and journey execution is expressly deferred to the combined five-task candidate and is not required as this task's focused remediation proof. Runtime sandboxing, cancellation/resource reclamation, citation existence validation, queue acknowledgment/replay behavior and real SDK integration were not exercised here and remain the owning later tasks' delivery risks.
