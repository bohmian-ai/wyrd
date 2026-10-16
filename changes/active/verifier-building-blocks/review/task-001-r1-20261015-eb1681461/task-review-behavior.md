# TASK-001 cumulative behavior review

**FAIL.** FIND-TASK-001-1, -2, -4, -5 and -6 are source-closed for this role. FIND-TASK-001-3 remains partially open because the new shared guard refuses compatible additional schema constraints that R1 expressly preserves. No new finding ID is proposed.

## Subject and authority

Original base `08b38f74255d344e1c5ec9a6d278935a26277622`; immutable candidate `eb1681461897ee1eb9fe1fa4e00cb0b435658559`. HEAD matched during inspection. Reviewed the complete 26-file cumulative production diff, not just remediation.diff. Known uncommitted docsite/planning changes are excluded.

Read the task-review skill; approved spec r6 and original TASK-001 snapshots; prior verdict, validated ledger and R1 task; supplied AGENTS instructions, agent rules, active design/doctrine and applicable spec-driven-development/maintainer-style references. Current subject/owner direction explicitly places legacy LLM judge removal, including offline eval, in TASK-005. Keeping the base consumers compiling now is correct and does not promise indefinite preservation. Scenario simulation is not blanket-removed. BlockError is the approved new seam.

Used CodeGraph before source discovery, then expanded the map through actual code. Did not consume conversational implementation summaries or other current discovery reports. This task stages pure contracts; runtime tools, providers, sandbox, aggregation, queue/server activation and SDK journeys belong to later tasks.

## Expanded caller and reuse map

| Caller-to-result path | Existing owner reused and result |
|---|---|
| Authored Prompt → compiled variables/response → Agent → Verifier validation | `skald_spec::Prompt::new`, `check_output_schema`, `check_schema` remain syntax/compiler owners; `Validator::validate_placeholders` derives bindings and kinds. `validate_agent` supplies position checks. Action output remains arbitrary. One false-refusal family remains in the new shape guard below. |
| Authored scope reference → parse → dependencies/visibility/known fields | `Reference` implements the approved execution-scope dialect; `step_references` and `dependencies` feed existing `plan_stages`, shared with `validate_dag`; `validate_reference` checks visibility. `admits` is the shared bounded known-shape consumer. No duplicate graph store/orchestrator. |
| Agent/Artifact/baseline identity → loader reference traversal | Existing `refs::Visit` has a staged-Verifier implementation; inline Agent recursion calls existing `visit_agent`; code Artifacts use Durable slots; drift uses existing DriftSpec traversal. No separate loader. |
| Role/forwarded JSON → PermissionWire → effective set/intersection → accepted authority | Tool scope/resource extend current owners; unchanged PermissionSet/Principal/AcceptedAuthority machinery preserves exact authority. One BuiltinTool enum feeds declaration, permission and result provenance. No new authorization store/default grant. |
| Gateway batch-list caller → invocable → listing reach | Resource/action filtering precedes exhaustive scope match (`gateway/batches.rs:166–194`). Tool arm contributes no gateway reach. Existing All and deployment/model scoped behavior survives. |
| Injected action → ActionOutput → StepResult → result wire | `vala-eval::blocks` follows injected executor pattern. BlockCall carries resolved values/source slots, position, workspace and deadline; BlockError retains failure/retry distinction. ToolCallRecord is shared by action output and result. No deferred projector was invented. |
| Existing eval → DAG/operator → offline consumers | `validate_dag` delegates to shared plan_stages; old/new comparison validation shares ComparisonOperator::validate; original judge/scenario consumers remain. Library proof covers old planner/operator/eval paths. Final judge removal is TASK-005. |

The local bounded schema walk is needed because Prompt syntax validation does not establish selector existence or Verdict shape. It does not justify a generic inclusion solver, external schema resolution, new dependency, public schema profile or exact generated-schema equality.

## Full task obligation matrix

PASS means the bounded staged task obligation passes, not that the later product capability is live. Test names below refer to source and the fresh logs inspected under Verification.

| Requirement, acceptance, constraint or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 subject-less declaration, optional steps, requirements, decision and capture | `blocks/mod.rs:41–65` | `verifier_spec_round_trips_and_validates`, serde negatives | PASS |
| IDs globally unique across steps and both requirement positions | `VerifierSpec::requirement_owners`, validate.rs:149–183 | `verifier_spec_refuses_invalid_graphs_and_references` | PASS |
| REQ-002 JSON/media/trace/samples declarations, required default, optional representation | InputSlot and required helpers in blocks/mod.rs; SlotName | Round trips; `slot_name_uses_identifier_grammar` | PASS |
| Slot declaration bounds and structured refusals | validate.rs::validate_slot refuses empty media/samples, zero size, non-schema JSON | Negative authoring cases, owner suite | PASS |
| Missing/null/incorrect runtime values remain representable for later implementation | InputDeclaration, CallInput, RequirementOutcome, status/result separation | Wire/handoff proof; runtime admission deferred | PASS |
| REQ-003 reference roots/fields/indexes/wildcards and position safety | blocks/reference.rs; validate_reference at validate.rs:682–777 | Reference root/malformed/serde tests; position negatives | PASS |
| Action/when imply deps; explicit deps/cycles refused | step_references, dependencies, plan; shared plan_stages | Authoring graph and existing DAG tests | PASS |
| Step checks see own/upstream; whole checks all; checks cannot read verdicts | validate_reference and ancestors | Step-position and verdict-position cases | PASS |
| Known-missing fields refused, unknown usable, complete traversal bounded | schema_path::Walk budget across branches/ref hops; input/output consumers share admits | `verifier_spec_bounds_cyclic_schema_references`, schema-path and known-output tests | PASS |
| REQ-004 closed extract/Agent/code actions, immutable source, Python-only runtime | Action, CodeCall/Runtime/Source; validate_step_shape/validate_code | Round trips and source/kind/entrypoint/budget refusals | PASS |
| All locked extraction formats, required/null fields, DOCX union, XLSX value/formula/error | blocks/extract.rs; required Option codecs/schema fields | Three extraction tests, nine exact format examples | PASS |
| Extraction ordering/source locator meanings; no counts/image helpers | Ordered vectors and docs in extract.rs; Locator/SlotName | Locked examples, missing/extra/count refusal | PASS |
| REQ-005 five check kinds and typed check detail | Check, TraceCheck, DriftCheck, RequirementDetail, Verdict | Authoring/drift/wire cases | PASS |
| Check Agents tool-less and no workspace | validate_agent:497–509,560–567 | Agent positions/media test; shared two-position guard | PASS |
| Derived Verdict compatibility preserves compatible additional restrictions | validate_agent:568–577; schema_path::fits | Existing cases cover wrong shape and canonical/ref variants; misses BEH-R1-001 | **FAIL: FIND-TASK-001-3** |
| REQ-006 all/pass_rate wire/default, finite positive threshold, honest outcomes | Decide; Validator::run; RequirementOutcome/StepStatus/execution statuses | Pass-rate bounds and wire tests; runtime aggregation deferred | PASS |
| REQ-007 Prompt-derived inputs/schema/kinds and exact placeholder agreement | Existing compiler; validate_placeholders:590–672; AgentCall contains no authored variables/schema | Placeholder/media negative cases, Prompt suite | PASS |
| Route ownership, override identity and Agent/Prompt provenance represented | Agent owns route; request TaskId-keyed overrides; CallProvenance::Agent | Request/result round trips; runtime overrides deferred | PASS |
| REQ-008 ten tools share one closed vocabulary | agent.rs::BuiltinTool, validate_agent membership, typed Tool scope/record | All-builtins test and every-name permission decoding | PASS |
| Filesystem tools need workspace; workspace needs filesystem/code tool; safe virtual paths | validate_agent:511–559; valid_workspace_path | Workspace missing/wrong-tool/path traversal cases | PASS |
| Global/exact tool grants decode strictly; exact belongs to tool/invoke | PermissionScope::Tool; Resource::Tool; Permission::validate and constructors | tool_invoke_is_exact_name_scoped_and_independent | PASS |
| Coverage/intersection/wildcard, disjoint scopes and no implicit domain grants | Scope::covers; unchanged PermissionSet algorithm | New permission test plus original library suite | PASS |
| Role/effective/accepted authority retains exact scope/subject/delegation | Generic PermissionWire, PermissionSet, Principal, AcceptedAuthority | Role JSON and accepted_authority_keeps_exact_tool_scope | PASS |
| Names/arguments/digest/returned-byte provenance through one record | execution.rs::ToolCallRecord used in ActionOutput and StepResult | Result round trip; action_tool_calls_carry_into_step_results | PASS |
| REQ-009 original-slot citations and derived data without new wrapper | Citation/Locator, CallInput.sources, arbitrary ActionOutput JSON | All locator variants and seam source-slot test; runtime validation deferred | PASS |
| REQ-010 canonical media aliases, OOXML distinct from legacy, incompatible extraction/native/archive refusal | MediaFormat and shared format metadata; validation consumers | Media parse/serialize/distinction/native tests, binding refusals | PASS |
| REQ-011–013 generic request/result/status, UUIDv7 ExecutionId, async conflict and delayed result states | execution.rs; reused status/error; ids.rs | Three wire tests and ExecutionId test; no queue behavior claimed | PASS |
| REQ-014 explicit schedule inputs, required slots/exact measurements and run/window refusal | validate_schedule/evidence_mode/drift; ScheduledInput | Schedule, mixed evidence, window step and drift-source negatives | PASS |
| REQ-015 attributable requirement/step breakdown and config/code/extract/Agent/drift provenance | Full execution wire; ActionOutput/CheckOutput | Documented result/handoff round trips | PASS |
| Existing loader/visitor/DAG/operator owners, no duplicate validation graph | refs::Visit; shared plan_stages and ComparisonOperator::validate | blocks_verifier_yields_every_reference_slot; old planner/operator tests | PASS |
| Usable injected seam: resolved values/sources, data vs verdict, position, authority/lifetime, errors | vala-eval/blocks.rs; accepted authority bound to per-execution executor; deadline/drop cancellation documented | Dynamic action handoff test; supplied workspace compile evidence | PASS |
| Foundation IO-/async-/PyO3-free; normal auth owners and no trusted request tenant/principal | Synchronous pure contracts/validation; request only identity/input/override fields; no dependency addition | Complete diff; supplied dependency check claim | PASS |
| Staging keeps base buildable; no parallel live authored protocol | Modules exported but not mounted in Spec/routes; live cutover and judge removal assigned TASK-005 | Diff/source; supplied workspace check claim; handoff map | PASS |
| Non-goals: no endpoints/storage/scheduler/sandbox/client transports/aliases/features | Complete cumulative diff; only server edit exhaustive gateway scope arm | Static diff review | PASS |
| Task-scoped AC-001/009/010, cross-task handoff and generated proof | Above source/contracts/test matrix | Fresh focused/library/codegen logs; shape gap remains | **FAIL: FIND-TASK-001-3** |
| Mandatory cumulative rustdoc including prior omissions | Helpers/error docs, DriftCheck codecs, Reference/MediaFormat trait items, operator delegate and test modules inspected | Static doc review; no behavior test needed | PASS |

## Prior finding closures

- **FIND-TASK-001-1 CLOSED:** Tool resource/scope and typed exact names exist. PermissionWire still validates every decode. All/exact coverage and generic intersection preserve attenuation; role and AcceptedAuthority codecs unchanged. Wrong exact-scope combinations, domain independence and wildcard behavior are tested. Gateway adaptation preserves its prior filtered reach.
- **FIND-TASK-001-2 CLOSED:** all ten names (including four prior omissions) are in BuiltinTool and shared with Agent membership. Check tool-less and filesystem/workspace rules remain separate and exercised.
- **FIND-TASK-001-3 PARTIALLY CLOSED / OPEN:** canonical and equivalent local-ref/annotation fixtures pass; incompatible field/outcome/citation/locator/extra-field cases now fail. Compatible constraints can nevertheless be falsely refused by the new checker; see BEH-R1-001.
- **FIND-TASK-001-4 CLOSED:** shared ToolCallRecord carries optional Sha256Hex and required returned_bytes. No-output fixture uses None/0 honestly. Action handoff and StepResult use the same record; pure value-round-trip proof suffices without deferred production projector.
- **FIND-TASK-001-5 CLOSED:** one 1024-work Walk covers branch recursion and local-ref hops with no recursive budget reset. Exhaustion admits unknown selectors. Public cyclic input and resolved-Agent-output tests, finite recursive path and known-missing cases passed. No intentional process crash needed.
- **FIND-TASK-001-6 CLOSED for this role:** prior listed fallible helpers, codecs, trait fields/methods/associated errors, operator delegate and test modules now carry substantive docs/error sections. New catalog/walk/provenance/seam docs were also inspected. Dedicated standards review remains the complete rule gate; this role found no remaining prior omission.

## Proposed finding

### BEH-R1-001 — Important — FIND-TASK-001-3: compatible Verdict constraints are rejected

**Classification:** INCORRECT, at the same prior finding boundary.

**Obligation:** REQ-005/007, original Scenario 3 and R1 FIND-TASK-001-3. R1 explicitly preserves compatible additional constraints without inventing a new authoring profile. The following cases require only local structural/literal facts, not arbitrary schema equivalence.

**Exact source:** `crates/wyrd-spec/src/card/blocks/schema_path.rs:128–131,149–153`; consumer `crates/wyrd-spec/src/card/blocks/validate.rs:568–577`; existing producer `crates/skald/skald-spec/src/prompt.rs:235–249,255–314`.

**Reachable case:** use the valid Verdict fixture already in `verifier_spec_requires_check_agents_to_produce_a_verdict`; change only reason from `{"type":"string","maxLength":500}` to `{"type":"string","allOf":[{"maxLength":500}]}`. It still declares exactly a string reason of at most 500 characters. Existing Prompt compilation accepts this schema: root object type and nested schema syntax remain valid.

**Trace:** authored Prompt → normal compiler/syntax validator → inline or resolver-supplied check Agent → validate_agent → fits against generated Verdict reason:string → authored-allOf branch at schema_path.rs:128 returns only `fits({maxLength:500}, typed-string)` → branch has no type → :152 returns false → entire check refused with check_agent_schema. The parent node's already sufficient type:string is discarded. Both step-check and whole-Verifier-check positions share this guard. Ordinary action Agent output remains unaffected.

**Corroborating bounded case:** reason `{"const":"fixed explanation"}` or `{"enum":["ok","rejected"]}` only admits strings and passes Prompt syntax checks. fits inspects authored finite values only when the typed side is also finite (:133–136). Typed reason:string is open, so these cases reach the explicit-type requirement at :149–153 and fail. Finite values are already present in this walk's values mechanism.

**Consequence:** staged public Verifier validation rejects usable check Agents after normal Prompt compilation, solely due to conventional compatible constraints. Later consumers inherit an unintended schema profile. Runtime malformed-output handling cannot repair a rejected declaration.

**Smallest correction:** keep the bounded shared guard and generated Verdict/Citation/Locator source. Preserve same-node structural facts when allOf only adds restrictions, and recognize finite literal string constraints through existing local values/types analysis instead of requiring redundant type. No generic schema theorem engine, normalization platform, witness generation, new dependency/profile/API, schema fetch or Prompt rewriting is required. Retain current incompatible shape/value/extra-field refusals, action freedom and runtime malformed-output refusal.

**Focused proof:** add positive reason type:string+allOf(maxLength), string const and string enum cases to the existing public validation fixture; retain negative numeric finite reason and current incompatible cases. Reuse existing two-position and inline/resolved guard coverage. Exact focused owner command: `mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::blocks::validate::tests::verifier_spec_requires_check_agents_to_produce_a_verdict)'`. New cases were source-traced, not added or run during this review-only pass. No provider/server/journey harness is needed.

## Verification

Reviewed fresh pinned-candidate logs: focused-check.log has 9/9 remediation tests passing; library-check.log has **1139 passed, zero skipped** across wyrd-spec, wyrd-runtime, wyrd-skald-spec and vala-eval. Existing DAG/operator/Prompt/offline eval tests are included. codegen-check.log shows source generation completed and checked targets stayed in sync. Subject records fresh cumulative whitespace proof. R1 task evidence additionally claims all-feature Clippy and workspace all-feature test compilation; this role did not independently establish raw logs for those claims.

No production files edited; this role wrote only this report. HEAD matched the pinned candidate. Existing passing tests omit the compatible forms above and therefore do not close FIND-TASK-001-3.

Skipped: no provider/server/SDK/journey or broad aggregate rerun; runtime paths are deferred by TASK-001/owner direction. Risk: valid check-Agent schemas remain rejected until the bounded shape guard is corrected.
