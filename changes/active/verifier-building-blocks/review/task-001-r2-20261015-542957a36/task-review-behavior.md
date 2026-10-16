# Independent behavior review — Task 001 cumulative R2

**Result: PASS. No material findings proposed.** All six prior findings are source-closed within the bounded Task 001 contract. This is acceptance of the staged contracts and integration seams, not certification of the later hosted runtime or final protocol cutover.

## Immutable subject and authority

- Original base: `08b38f74255d344e1c5ec9a6d278935a26277622`.
- Reviewed candidate: `542957a3622461e259aee5f8108d978e18adad1c`; HEAD matched and `git diff --name-only 542957a36 -- crates` was empty at inspection.
- Reviewed the complete cumulative 26-file source change, using the latest four-file remediation only as a locator. Reviewed the original Task 001 and approved spec r6, both earlier verdicts and validated ledgers, R1 contract-gaps task and R2 verdict-schema-closure task. Earlier closure claims were hypotheses checked against source, not inherited conclusions.
- Applied AGENTS.md, agent rules, active design/doctrine, reference router, spec-driven development, maintainer style, ownership patterns, Rust/error and testing guidance, evaluation and drift guidance. Approved replacement authority supersedes the older kind-specific Verifier design only where explicitly approved. Task 001's staging and focused-proof boundary controls this review.
- Current owner direction assigns legacy LLM judge removal, including offline evaluation, to Task 005. Temporary retention here preserves compilation. The new seam uses BlockError; scenario evaluation is not blanket-removed.
- No other current discovery report or conversational implementation summary informed this report. Production source, specifications, tasks and prior reports were not changed.

## Navigation map and actual caller paths

CodeGraph preceded source discovery. I expanded the supplied map into these producer/consumer paths and inspected the relevant full bodies and cumulative changes:

| Producer / entry point | Shared owner and sibling consumers | Existing alternative / reuse assessment |
|---|---|---|
| Authored Verifier JSON | `blocks/mod.rs`; `VerifierSpec::validate/validate_with` → `requirement_owners` → `Validator::run` → slots/evidence modes/actions/requirements | Extends the foundational declarative contract; no registry, IO owner or second live format added. |
| Action/when references | `Reference::parse` and selectors → `step_references` → `VerifierSpec::dependencies/plan` → `plan_stages`; step checks use `ancestors` | Existing DAG owner was generalized and `validate_dag` delegates. Card references still use the existing `Ref`/`InlineableRef` visitor, not this execution-scope grammar. |
| Known input/output schema | `validate_reference` → `admits`; `output_schema` supplies extraction schema, resolved Agent schema, text shape or unknown code shape | Same bounded walker handles input slots and step outputs. Existing Prompt schema syntax checking does not follow local references or own selector existence. |
| Inline / resolved Agent + compiled Prompt | Action or either check position → `validate_agent`; placeholder/native-media/workspace/tool rules; check position alone → `fits(schema, verdict_schema())` | Prompt remains owner of derived placeholders/response schema. Generated Verdict/Citation/Locator remains the shape source. No second Prompt compiler or schema service. |
| Grant / role / accepted-authority JSON | PermissionWire → `Permission::validate`; PermissionScope covers → Permission/PermissionSet covers, insert and intersection → effective principal / AcceptedAuthority | Existing three-axis permission model performs decoding and attenuation. Global/exact tool scope is ordinary authority; no new checker, role default or store. |
| Tool catalog | `BuiltinTool::{ALL,as_str,from_name,is_filesystem}` → Agent declaration validation, Tool scope, ToolCallRecord | One complete ten-name vocabulary; no separate incomplete declaration list. Gateway batch pruning remains an independent existing sibling consumer of PermissionScope. |
| Future executor output | `BlockCall` source-slot sets / position / deadline → ActionOutput or CheckOutput → shared ToolCallRecord / StepResult and result wire | Narrow injected Vala seam reuses existing executor shape. No deferred runtime projector, provider, worker or hosted tool was fabricated. |
| Media declarations | shared `MediaFormat` parse/serde/schema/native-kind/extractable → slot/extract/native-media checks; Prompt::media_kind derives portable block kind | Vocabulary remains in Skald's existing media owner; no foundation runtime dependency. |
| Card-reference traversal | blocks Visit → Agent slot and nested Prompt/bindings, Artifact code source, drift baseline | Extends current visitor; no parallel loader or graph store. |

## Obligation matrix

PASS refers to the Task 001 contract/staging slice. Runtime behavior deliberately assigned to Tasks 002–005 is identified explicitly, rather than credited to declarations alone.

| Requirement, criterion, constraint or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 subject-less authoring, optional steps, no implementation discriminator | `blocks/mod.rs:39` VerifierSpec and closed serde contract; `validate.rs:137` global duplicate detection | `verifier_spec_round_trips_and_validates`, `verifier_spec_refuses_unknown_fields`, graph/refusal test | PASS |
| REQ-001 one global step/requirement namespace | `requirement_owners` visits steps, their requirements and whole requirements before validation; duplicate cannot be hidden by planner map collection | Duplicate step and cross-category requirement cases; source trace covers other collisions | PASS |
| REQ-002 JSON/media/trace/samples, required default and typed named slots | `InputSlot` and `required`; `SlotName`; `validate_slot:786`; typed InputDeclaration | Round-trip/default/boundary tests, identifier test, request declaration test. Request-time value loading belongs later | PASS |
| REQ-003 approved reference grammar and wildcard positions | `reference.rs:117` parse + Tokens; `validate_reference:668`; Check::Assert exactly-one selector | Reference root/malformed/serde tests; invalid graph/reference cases | PASS |
| REQ-003 dependencies, cycles, check scope and inaccessible verdicts | `dependencies`, `plan`, shared `plan_stages`; `ancestors`; step check own/upstream restriction and verdict reads restricted to action/when | Planner existing tests plus graph/reference fixture; own-step check demonstrated in review fixture | PASS |
| REQ-003 known missing fields refused, unknown shapes usable | `validate_reference:688,736`; `output_schema:756`; bounded `admits` preserves unknown on budget/pointer uncertainty | `schema_path_refuses_only_known_missing_selectors`; cyclic public-validation and finite-recursive cases | PASS |
| REQ-004 three closed action kinds and Python source/entrypoint contract | `Action`, CodeCall/CodeRuntime/CodeSource; `validate_step_shape` and `validate_code` | Authoring round-trip, invalid kind/source/entrypoint/budget cases | PASS |
| REQ-004 locked nine-format extraction contract | `extract.rs` closed union; required nullable fields, ordered arrays, sparse cells, formula/error docs; `schema_for` selects permitted formats | All nine concrete round-trips, missing/extra-field refusals and format-schema selection | PASS |
| REQ-005 five checks and separate check verdicts/data | `Check`, TraceCheck, DriftCheck; typed Verdict; Vala ActionExecutor versus CheckExecutor | Authoring fixture and execution-wire tests; seam fixture. Execution/aggregation implementation remains Task 002 | PASS |
| REQ-005/007 check Agents tool-less and Verdict-compatible, both positions/resolution forms | `validate_requirement` sends both check positions through `validate_agent:492`; guard at :568 uses generated schema | Existing exact `verifier_spec_requires_check_agents_to_produce_a_verdict`; original incompatible cases retained, bounded R2 cases added | PASS |
| REQ-006 default all, finite positive pass-rate and honest result states | Decide and validation :210; RequirementOutcome separates not_applicable/inconclusive; step status/reason and execution errors remain distinct | Threshold/default/wire cases. Missing-evidence propagation and denominator execution remain Task 002 | PASS |
| REQ-007 Prompt-derived inputs, media and response; explicit bindings | `validate_placeholders:590`; `Prompt::media_kind`; no authored variables/media_variables/response_type on block call | Existing action/check/native-media/placeholder tests and arbitrary structured action fixture | PASS |
| REQ-008 all ten built-ins, workspace restrictions | `BuiltinTool:61`; `validate_agent:533`; fs tools require workspace; workspace requires fs or code.run; unknown names refused | `verifier_spec_admits_every_builtin_step_tool`, position/media tests | PASS |
| REQ-008 r6 / Scenario 5 / INV-003 independent global/exact tool permission | PermissionScope::Tool; Resource::Tool; `Permission::validate`, constructors, covers/intersection; strict PermissionWire | Runtime `tool_invoke_is_exact_name_scoped_and_independent`, accepted-authority test; prior four-owner 1139-test proof | PASS |
| REQ-008/015 output digest/byte provenance and honest no-output shape | `execution.rs:315` ToolCallRecord uses BuiltinTool, optional Sha256Hex and returned_bytes; same type in ActionOutput and StepResult | Result serde negatives/positives; `action_tool_calls_carry_into_step_results` pure seam proof | PASS |
| REQ-009 original-slot citations remain ordinary data and survive seam | Citation/Locator at `execution.rs:359`; ActionOutput output Value; CallInput.sources in Vala | Locator result examples, resolved-input and provenance handoff tests. Actual pinned-location validation remains hosted work | PASS |
| REQ-010 shared canonical media/aliases and legacy distinction | Skald MediaFormat; native_kind and is_extractable; format checks at action and Prompt binding | Media parse/serde/legacy/native tests; extract/native mismatch cases. File sniff/hash/upload remain later owners | PASS |
| REQ-011–013 request/result/status identities and delayed conflicts | VerificationRequest, InputDeclaration, EnqueueVerificationResponse, VerificationExecutionState, ExecutionId; reuses VerificationExecutionStatus/Error | Every input-kind round-trip, UUIDv7 refusal, delayed idempotency error conversion. No synchronous DB reservation added | PASS |
| REQ-014 explicit schedule mapping and existing drift | ScheduledInput; `validate_schedule:878` requires all required slots, samples-only maps and exact measurement set; DriftCheck reuses DriftSpec | Schedule mapping test, drift round-trip and feature mismatch tests. Fitting/scheduling remain later work | PASS |
| REQ-015 attributable complete result/handoff vocabulary | VerificationResult/Outcome/RequirementResult/StepResult and CallProvenance retain exact identities/configuration/routes/digests, attempts/duration/errors/details | Documented result round-trip and seam proof; no fake production writer | PASS |
| INV-001 foundation purity / ownership | New spec code is sync serde/schema/validation; shared media in Skald; async executor signature in Vala | No manifest/dependency changes; recorded check:deps and all-features compile proof | PASS |
| INV-006 honest evidence / errors | RequirementOutcome, Verdict, BlockError and VerificationError distinguish judgments from inability to judge; source slots explicit | Wire/error/seam tests; runtime invalid-citation and missing evidence enforcement belongs later | PASS |
| INV-008 staged single protocol and compile-preserving cutover | New `card::blocks` and `verification::execution` exports are staged; live Spec/routes remain original; exact handoff switch points recorded in original task | Source/export inventory, prior workspace all-features tests check; no new route/alias/compatibility feature | PASS |
| Existing-owner reuse and minimum machinery | DAG/operator/visitor/Prompt/media/permission owners extended; Walk owns finite traversal state; Validator owns one pass; traits are action/check implementation seam | Source/caller comparison above; no duplicate engine/loader/permission store/schema service | PASS |
| AC-001/009/010 Task 001 proof and generated-contract coordination | Required source schema/serde inputs and pure cases supplied; no live OpenAPI/MCP change except disjoint scope match adaptation | Fresh exact schema 1/1, wyrd-spec 869/869, owner Clippy, codegen logs; prior broader owner/compile/dependency evidence | PASS |
| Mandatory changed-item rustdoc / FIND-6 | Repaired helper/codec/reference/media/test docs; both fmt methods now document formatter-write errors; new R2 helpers documented | Static body-to-doc check; owner Clippy and recorded fmt. No claim lint alone proves prose | PASS |
| Non-goals: no new Card kind, plugin/sandbox/platform, nested graph, runtime/store/queue/transport or premature judge removal | Complete cumulative source/write inventory; new contract modules stay unmounted; old judge remains temporary Task 005 cutover state | Static diff/consumer closure; no broad journey claim | PASS |

## Prior finding closure and R2 regression checks

| Stable ID | Independent source closure |
|---|---|
| FIND-TASK-001-1 | **CLOSED.** Tool scope is a typed exact BuiltinTool and validates only with Tool/Invoke. All covers exact; exact only itself. Existing PermissionSet intersection picks the narrower scope before resource/action meets and validates the result. Distinct tool scopes contribute nothing. Role/forwarding/AcceptedAuthority use the same strict decoder, preserving subject and delegation without All promotion. Cards/Bifrost/Verifier grants have disjoint resources; no default tool grant added. Gateway batch `invocable` first filters Gateway/Invoke and ignores Tool in the exhaustive scope match, preserving its earlier reach. |
| FIND-TASK-001-2 | **CLOSED.** All ten approved names exist in BuiltinTool; action validation calls from_name, rather than a six-name list. The four discovery/read tools require no unrelated workspace. Check-position tools and workspaces are still refused; fs/workspace/code restrictions are retained. The same catalog drives permission names and tool provenance. |
| FIND-TASK-001-3 | **CLOSED within the selected bounded correction.** Shared guard still derives typed Verdict/Citation/Locator. `Walk::fits:130` tries sufficient allOf parts, then falls through to the current node's structural facts instead of discarding type/enum siblings. `:153` infers primitive types from existing finite-value analysis only when no type is declared; string const/enum pass, numeric/mixed reason cases fail. `fits_object:202` handles exact anchored literal-name patterns as named declarations: new score is refused by closed typed Verdict, whereas a reason refinement adds no field. Existing original negatives for outcomes/reason/citations/locators/extra properties remain, as do canonical/ref/annotation/subset positives and arbitrary action outputs. Both check positions and inline/resolver modes converge at this one guard; no separate missed consumer correction is needed. |
| FIND-TASK-001-4 | **CLOSED.** ToolCallRecord's optional canonical Sha256Hex and mandatory returned_bytes describe bounded returned output; no-output/error calls can omit digest with zero bytes. One record type is used in ActionOutput and StepResult. Pure value passage and wire round-trip prove the Task 001 handoff; there is no production projector to demand in this task. |
| FIND-TASK-001-5 | **CLOSED.** One Walk budget spans every admits/fits/values recursive call and local-reference hop; there is no fresh nested budget. Exhausted/unresolved selector analysis admits unknown; exhausted/unresolved compatibility analysis refuses unproven fit. R2 helpers do not introduce unbounded recursion or change admits. Cyclic input and resolved-Agent output cases terminate; finite recursive known-missing fields remain refused. |
| FIND-TASK-001-6 | **CLOSED.** Reference fmt at `reference.rs:235` now names fmt::Error from writer failure; MediaFormat fmt at `media.rs:304` names the same formatter failure. Both bodies retain canonical write behavior. Previously named custom codecs, validation helpers and test modules have substantive docs/error sections; new json_type/literal_name/types helpers are documented. |

Additional falsification focused on shared-cause regressions: action mode still bypasses the check-only fit guard; nested citation/local-ref comparison still spends one budget; typed allOf wrappers remain checked; const/enum mixed primitives cannot become string by the new fallback; exact-name pattern refinement only narrows an already declared field. Known-missing extraction/Agent-output selection still uses admits, not fits, so uncertainty retains its prior opposite policy. There is no provider/schema fetch or new dependency.

Arbitrary regex inclusion, numeric bounds, string formats/pattern strength and general schema set inclusion remain outside the approved bounded comparison. Their absence is not a new finding. Output decoding remains a later runtime obligation. I did not require a new schema profile, keyword ban, exact-schema equality, platform abstraction, witness generator or broad test suite.

## Review Findings

### Critical

None.

### Important

None.

### Suggestions

None; no optional refactors are proposed.

## Open Questions

None affecting this bounded task's acceptance.

## Verification Notes

Inspected current review logs: focused-check.log records the exact existing schema test with **1 passed**; library-check.log records **869 passed, zero skipped** in wyrd-spec; clippy-check.log finishes successfully; codegen-check.log completes its checks. The R2 evidence records RED on the first new compatible allOf reason case followed by GREEN, owner Clippy, fmt, codegen with unchanged outputs and whitespace proof. Earlier R1 evidence records the four-library **1139-test** run, nine exact selections, all-features owner lint and workspace test compilation, dependency checks and contract checks. Permission and Vala source are unchanged by R2. I did not start additional Cargo work in the shared target directory.

The focused schema fixture exercises all selected new positives/negatives through whole-Verifier inline validation, with canonical/incompatible cases additionally passing through step-check and resolver modes. Source confirms the same guard owns all combinations. This is credible focused closure without requiring a Cartesian test expansion.

Skipped: live providers/server/SQL/sandbox and SDK journeys, as assigned to later integrated work; no additional broad tests or production edits.
Risk: this PASS covers staged pure contracts only; hosted authorization, citation enforcement, resource isolation, queue/recovery and final judge removal still need their owning tasks' proof.
