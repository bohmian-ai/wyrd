# TASK-001 cumulative invariant review

## Review Findings

### Critical

None.

### Important

**INV-R1-001 — FIND-TASK-001-3 remains partially open — INCORRECT.**

`crates/wyrd-spec/src/card/blocks/schema_path.rs:128` and `:177` implement an incomplete structural interpretation that both rejects compatible Verdict schemas and accepts schemas allowing incompatible output. Obligations: REQ-005/007, Scenario 3, and the prior finding's explicit preservation of compatible restrictions and refusal of incompatible extra fields.

Producer-to-consumer trace: portable `response_format` or native provider response schema → existing Prompt `check_output_schema`/`check_schema` → derived `ResponseType::JsonSchema` → inline/resolver Agent → shared `Validator::validate_agent` check-position guard (`validate.rs:568`) → `fits`/`fits_object` → future typed `Verdict` (`verification/execution.rs:339`). Prompt syntax checking explicitly accepts both `patternProperties` and `allOf` (`skald-spec/src/prompt.rs:285,293`); this is not malformed input rejected upstream.

Two concrete counterexamples require no arbitrary JSON Schema inclusion proof:

1. Start with the accepted Verdict fixture in `validate.rs::tests::verdict_schema` and add top-level `"patternProperties":{"^score$":{"type":"number"}}`. Keep `additionalProperties:false` and all existing properties/required fields. `fits_object` treats the authored object as closed solely because of `additionalProperties:false`, then inspects only `properties` and `required`; it therefore accepts. JSON Schema permits `score:1` through `patternProperties`, so `{"outcome":"passed","reason":"ok","citations":[],"score":1}` conforms to that declared schema but fails `Verdict`'s `deny_unknown_fields`. Existing fixture shape without score otherwise remains unchanged. Early refusal of incompatible extra fields is still missing.
2. In the accepted fixture, change only the outcome schema to `{"type":"string","enum":["passed","failed","inconclusive"],"allOf":[{"minLength":1}]}`. This is equivalent to its original outcome schema: every listed outcome is already nonempty. `fits` sees authored `allOf` and returns comparison of its parts alone (`:128–131`), discarding sibling `type`/`enum`. The sole `minLength` part has no closed value set; comparison against the typed outcome's enum fails (`:133–136`). The public validator refuses this compatible check as `check_agent_schema`. The same occurs when a structurally complete reason schema adds an annotation/refinement-only `allOf` part: sibling type evidence is lost.

Both check positions and both Agent resolution modes use this owner, so fixing downstream invokers or individual call sites would repeat the error. Action-position output remains correctly unrestricted. The previous wrong-type/enum/citation cases are fixed; these counterexamples show why the complete prior obligation is not closed.

Smallest correction: retain the current typed Verdict source and bounded existing guard, preserve a node's own structural evidence when traversing conjunctive refinements, and account for pattern-declared properties before treating an object as closed. Refuse an authored shape when its extra-field reach cannot satisfy the closed typed object. Do not rewrite Prompt schemas, weaken typed decoding, add a profile/dependency/platform, or attempt general regex/schema inclusion. Keep runtime malformed-output refusal independent.

Focused proof: extend the existing pure public validation test with the two fixture variations above; require pattern-added score refusal and equivalent allOf refinement acceptance. Exercise the shared guard through both check positions and inline/resolver Agents using existing fixtures. Preserve current incompatible-field tests and an arbitrary structured action positive. These counterexamples are source-traced; I did not add tests or modify source to reproduce them.

**INV-R1-002 — FIND-TASK-001-6 remains narrowly open — VIOLATION.**

`crates/wyrd-spec/src/card/blocks/reference.rs:235` and `crates/skald/skald-spec/src/media.rs:304` now have summary rustdoc, but their fallible `Display::fmt` methods still have no `# Errors` section. `Reference::fmt` propagates formatter errors through `write!`/`?`; `MediaFormat::fmt` returns `f.write_str`'s error. These are changed trait items explicitly included in the prior finding. AGENTS.md §16 requires error conditions on every fallible Rust function/method and explicitly makes missing documentation blocking. No runtime defect is asserted.

Smallest correction: document formatter-write failure at these existing methods in the same bounded remediation; no implementation change, new test, unrelated documentation sweep, or standalone style redesign. Static changed-item rustdoc inspection and formatting are sufficient proof. The earlier undocumented validation helpers, codecs, fields, associated types, and test modules have otherwise received substantive documentation.

### Suggestions

None; optional hardening and style preferences excluded.

## Immutable subject and authority

- Original base: `08b38f74255d344e1c5ec9a6d278935a26277622`.
- Cumulative candidate: `eb1681461897ee1eb9fe1fa4e00cb0b435658559` (HEAD confirmed unchanged during review).
- Reviewed full 26-file base-to-candidate implementation, not just remediation. Excluded uncommitted docsite/planning files.
- Read task-review skill, subject, approved r6 spec/task snapshots, prior verdict/validated ledger/R1 task, AGENTS.md, agent rules, design/doctrine and applicable spec-driven/maintainer references. Inspected manifests and canonical verification configuration.
- CodeGraph was attempted first for discovery, then source inspection expanded the map independently. No other current discovery report was read.
- Owner direction assigns legacy LLM-judge removal, including offline eval, to Task 005. Its temporary compilation-preserving retention is accepted. The new seam uses BlockError; scenario simulation is preserved.

## Expanded ownership and invariant map

| Producer / authority | Shared owner and sibling consumers | Invariant / assessment |
|---|---|---|
| Authored blocks, IDs and references | `blocks/mod.rs`, `reference.rs`, `validate.rs`; `plan_stages`, `Visit` | One global namespace, inferred action/when dependencies, no verdict feedback between checks; current parsing/planning and ancestor visibility preserve this. |
| Input schema / resolved Agent output / selected extraction schema | `schema_path::admits`; two `validate_reference` callers and `output_schema` | Same bounded walk handles local refs/combinators and unknowns. Both input/output consumers share closure of ID5. |
| Compiled Prompt response schema | `schema_path::fits`; shared check-position Agent guard | Typed Verdict structure derives from actual wire type. Two remaining structural interpretation defects above affect both check positions/resolution modes. |
| Agent declared tool names | `agent::BuiltinTool`; action membership, `PermissionScope::Tool`, `ToolCallRecord.tool` | All ten approved names share one closed typed catalog; filesystem/workspace rules remain separate. |
| Role/grant JSON | `PermissionWire` + `Permission::validate`; generic PermissionSet/Principal/AcceptedAuthority | Exact scope does not become All; global/exact coverage and disjoint intersection use existing owners. No default tool grant added. |
| Gateway caller effective grants | `gateway/batches.rs::invocable`, existing scope filtering | New Tool arm contributes no gateway reach; existing All/deployment/model behavior unchanged. |
| Future action output | Vala `ActionOutput.tool_calls` → shared `ToolCallRecord` → `StepResult.tool_calls` | Digest/count and error/no-output states are representable and round-trip; no deferred projector invented. |
| Existing eval task graph/operator | `validate_dag` delegates to `plan_stages`; `validate_task_operator` delegates to `ComparisonOperator::validate` | Existing offline callers retain graph/error/operator semantics; no replacement engine/store. |
| Card/Prompt/Artifact/drift references | Existing `Visit` implementations plus block traversal | Inline Agents recurse into existing Prompt/verification bindings; Artifact and baseline references remain visible to the loader. No second resolver. |
| Media strings | `skald_spec::MediaFormat`, Prompt media-kind accessor | One canonical vocabulary preserves legacy/OOXML distinction, archives not native, extraction union matches supported formats. |

The changed pure helpers retain caller-owned state; Validator and Walk retain the meaningful state for their bounded pass. New async seams live in the existing Vala execution owner and add no runtime implementation. Existing graph/operator/Prompt/reference ownership is extended rather than replaced. No additional abstraction or dependency is required for these corrections.

## Obligation matrix

PASS here means this task's staged contract obligation, not final runtime activation.

| Requirement, acceptance criterion, constraint or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 subject-less no-step/preparation contract and global block IDs | `VerifierSpec`, `requirement_owners` | authoring round-trip, invalid graph/reference tests | PASS |
| REQ-002 typed slots, required default, optional representation, positive bounds | InputSlot, SlotName, validate_slot; InputDeclaration | wire/slot tests; request-time values deferred | PASS |
| REQ-003 grammar, inferred dependencies, cycles and visibility | Reference parser, step_references, plan, ancestors, validate_reference | reference and graph negative fixtures | PASS |
| REQ-003 bounded known/unknown schema traversal | shared Walk budget across refs and recursive branches | fresh cyclic public-validation test passed; finite recursive missing-field fixture | PASS |
| REQ-004 three actions and exact extraction shapes | Action/CodeSource, ExtractOutput/Pdf/Pptx/Docx/Xlsx types and schema_for | nine output examples, required/null and extra-field refusals | PASS |
| REQ-005 five check kinds and position rules | Check, CheckEvidence, validate_requirement/agent | existing negative cases pass; structural compatibility counterexamples above | FAIL — ID3 |
| REQ-006 honest outcomes, skips, threshold and empty requirement semantics representable | Decide, RequirementOutcome, StepStatus, reasons and errors | threshold/result fixtures; aggregation belongs Task 002 | PASS |
| REQ-007 Prompt-derived placeholders/output, route/identity handoff | validate_placeholders; Prompt.media_kind, execution-route overrides and CallProvenance | position/media/derived-schema tests; remaining compatibility defect | FAIL — ID3 |
| REQ-008 complete built-in catalog, workspace/tool-less checks | BuiltinTool, validate_agent, valid_workspace_path | all-ten test and existing workspace/check refusals | PASS |
| REQ-008 independent global/exact tool grants | Tool Resource/Scope, strict validate, generic coverage/intersection | fresh exact permission and accepted-authority tests passed | PASS |
| REQ-008/015 output digest and bytes | shared ToolCallRecord using Sha256Hex/returned_bytes | fresh action→step round-trip passed; error/no-output result fixture | PASS |
| REQ-009 original-slot citations and ordinary derived data | Citation/Locator, CallInput.sources, ActionOutput, CheckOutput | result/handoff fixtures; runtime existence checks deferred | PASS |
| REQ-010 one media vocabulary and compatibility | MediaFormat aliases/native_kind/is_extractable; media placeholder validation | format and incompatible-media fixtures; sniffing deferred | PASS |
| REQ-011/012 Run/Window/request/status/result/identity distinctions | mixed-evidence validation, generic messages, ExecutionId | request/result/UUIDv7 tests; routes/SDKs deferred | PASS |
| REQ-013 async conflict remains error/status, no new reservation | existing VerificationExecutionStatus/Error plus WyrdError conversion | delayed conflict fixture; queue implementation deferred | PASS |
| REQ-014 explicit schedule maps/drift reuse | ScheduledInput, validate_schedule, DriftCheck and existing DriftSpec | exact mapping and drift feature tests | PASS |
| REQ-015 complete result/provenance and engine integration seam | execution.rs, Vala BlockCall/outputs/BlockError | result and dynamic action/handoff fixtures | PASS |
| INV-001/003 foundational purity and authority contract | no IO/async/PyO3 in foundation; accepted authority remains runtime-owned | source/manifests; fresh permission tests | PASS |
| INV-006 honest evidence contract | inconclusive/not_applicable, status/error separation, original source slots | serialization and handoff evidence; evaluation runtime deferred | PASS |
| INV-008 staged replacement/base compilation and Task 005 removal | blocks/execution modules not live Spec/route replacements; old consumers preserved | source diff; reported workspace compilation | PASS |
| Reuse existing loaders, reference visitors, DAG, operator and runtime seams | existing owner extensions described above | existing visitor/planner/operator tests and source inspection | PASS |
| No runtime tools/providers/sandbox/store/endpoints/SDK transports here | full cumulative diff contains contracts, validation and minimal compile adaptation | static write-set/caller inspection | PASS |
| AC-001 scoped registration/schema proof | pure owner tests plus source audit | fresh five selected tests pass, uncovered compatibility cases remain | FAIL — ID3 |
| AC-009 cutover/consumer coordination bounded to task | staged exports and switch points identify Tasks 2–5; no alternate live authored format | source and handoff map; final schemas/journeys deferred | PASS |
| AC-010 scoped checks and changed-item documentation | declared bounded checks; no tests weakened/ignored | fresh selection/diff check; mandatory fmt errors docs missing | FAIL — ID6 |

## Prior-finding closure

| Prior ID | Source closure evidence | Disposition |
|---|---|---|
| FIND-TASK-001-1 | PermissionScope::Tool and Tool Resource; validate exact resource/action; covers delegates exact identity; generic narrower/intersection and authority serde unchanged | CLOSED; fresh exact grant and authority tests passed |
| FIND-TASK-001-2 | BuiltinTool::ALL has ten approved names; validate_agent uses from_name, maintains independent fs/workspace/check rules | CLOSED; all-ten fixture inspected |
| FIND-TASK-001-3 | New fits compares generated typed schema and closes original simple wrong-type/enum/citation holes; loses conjunction siblings and ignores pattern-declared extra fields | OPEN; INV-R1-001 preserves ID3 |
| FIND-TASK-001-4 | ToolCallRecord now retains optional output_sha256 and returned_bytes in both existing consumers; no-output docs and error fixture | CLOSED; fresh handoff round-trip passed |
| FIND-TASK-001-5 | One Walk budget is passed recursively and spent by resolve; exhaustion admits unknown shape; no per-recursion reset | CLOSED; fresh self/mutual-ref test passed |
| FIND-TASK-001-6 | Original helper/codec/field/test documentation supplied; Display methods now documented but missing mandatory error sections | OPEN, narrowed; INV-R1-002 preserves ID6 |

No new stable FIND ID is proposed: these are residual obligations of IDs3/6, not per-caller rediscovery. ID3's two failures share the existing structural compatibility boundary; ID6 remains a compact documentation correction in the same remediation.

## Open Questions

None requiring a new product, API, architecture or persistence decision.

## Verification Notes

Fresh command executed successfully:

```bash
mise exec -- cargo nextest run --locked -p wyrd-spec -p wyrd-runtime -p vala-eval --lib -E 'test(=card::blocks::validate::tests::verifier_spec_requires_check_agents_to_produce_a_verdict) | test(=card::blocks::validate::tests::verifier_spec_bounds_cyclic_schema_references) | test(=permission::tests::tool_invoke_is_exact_name_scoped_and_independent) | test(=request_context::tests::accepted_authority_keeps_exact_tool_scope) | test(=blocks::tests::action_tool_calls_carry_into_step_results)'
# 5 passed; 1035 skipped by exact selection.

git diff --check 08b38f74255d344e1c5ec9a6d278935a26277622 eb1681461897ee1eb9fe1fa4e00cb0b435658559
# Passed.
```

Reviewed original task/remediation recorded commands and the tests implementing the stated evidence. Reported 1139-library-test/all-features lint/workspace check/fmt/dependency/codegen results were not rerun by this reviewer; the five results above are independent fresh proof. Passing existing schema tests does not cover the source-traced counterexamples. No new fixtures or scripts were created.

## Overall result

**FAIL** — retain FIND-TASK-001-3 and the narrowed FIND-TASK-001-6 for independent validation. No Critical finding or current mounted-endpoint exploit is asserted. The pure validation API itself is an explicitly required Task 001 surface.

Skipped full server/SDK journeys and deferred runtime work, as this task requires. Remaining risk: compatible checks can be refused, and a schema permitting an extra output field can still pass early validation.
