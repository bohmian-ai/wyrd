# Task 001 cumulative invariant review

Overall: **FAIL** — one proposed Important finding, **INV-R2-001**. All six prior findings are source-closed within their approved bounds. The newly located failure is a sibling selector consumer of the shared schema owner, not an additional Verdict inclusion requirement.

## Immutable subject and authority

Reviewed original base `08b38f74255d344e1c5ec9a6d278935a26277622` through candidate `542957a3622461e259aee5f8108d978e18adad1c`, including all 26 cumulative source files. HEAD remained the candidate at the final check. The four-file R2 diff was only a navigation aid. Uncommitted docsite/planning changes were excluded. Only this new report was written.

Inputs: original Task 001 and approved r6 spec snapshots; initial and R1 verdicts, validated ledgers, R1 and R2 remediation tasks; AGENTS, agent rules, spec-driven-development and maintainer references, design/doctrine, evaluation/drift references. The approved spec's explicit replacement and staging instructions override the older split described in architecture. No current discovery report or conversational implementation summary was consumed. CodeGraph exploration preceded code discovery.

Task 001 delivers staged pure contracts and narrow injected Vala signatures. Runtime admission, tool implementation, cancellation enforcement, persistent projection, SDK integration, journeys and final cutover belong to later tasks. Legacy LLM judge removal, including offline evaluation, belongs to Task 005; neither indefinite preservation nor immediate removal is required here.

## Review Findings

### Critical

None.

### Important

**INV-R2-001 — INCORRECT — A literal pattern-declared field is incorrectly classified as missing by the shared selector walker.**

- **Violated obligation:** original Task 001 Scenario 2 and REQ-003: references to known fields remain usable; reject nonexistent fields when known and admit unknown shapes. This is original authoring/reference behavior, not authority to expand R2's bounded Verdict comparison into a generic schema profile.
- **Exact defect:** `crates/wyrd-spec/src/card/blocks/schema_path.rs:93-94`, in the complete `Walk::admits` field arm. It checks `properties`, then immediately treats `additionalProperties: false` as proof that an undeclared field is absent, without checking `patternProperties`. The new existing `literal_name` at line 280 already recognizes the relevant exact anchored names for `fits_object` at line 207.
- **Concrete source-proven public reproduction:** deserialize this staged `VerifierSpec`, then call `validate()`:

```json
{
  "inputs": {
    "answer": {
      "type": "json",
      "schema": {
        "type": "object",
        "patternProperties": {"^score$": {"type": "number"}},
        "additionalProperties": false
      }
    }
  },
  "requirements": [{
    "id": "positive", "statement": "Score is positive",
    "check": {
      "kind": "assert", "path": "inputs.answer.score",
      "operator": "greater_than", "expected": 0
    }
  }]
}
```

`{"score": 1}` is an admitted object and `score` is explicitly declared by the exact anchored pattern. Nevertheless validation returns `rule: missing_field`. Source proof: `InputSlot::Json` retains the schema (`blocks/mod.rs:66`); `validate_slot` (`validate.rs:786`) permits the object schema; `Reference::parse` (`reference.rs:126`) accepts this root/field; `Validator::validate_requirement` selects the assertion path; `validate_reference` (`validate.rs:688-694`) calls `admits`. There is no `properties.score`, `type: object` does not exclude objects, and line 94 returns false. This path needs no Agent/provider or future runtime.

- **Sibling producer and consumer:** an action Agent can declare the same object response schema. Existing shared Prompt syntax validation expressly accepts and traverses `patternProperties` (`skald-spec/src/prompt.rs:283-291`), and an action Agent is not subject to the Verdict guard (`validate.rs:529-568`). `output_schema` (`validate.rs:762-764`) returns that schema unchanged for either an inline or caller-resolved Agent. Any step/whole requirement or downstream action/condition referencing `steps.<id>.output.score` then reaches the same `admits` defect through `validate.rs:736-740`. For a minimal adapted `review()` fixture, preserve the action's `outline` placeholder and workspace but change its response schema to the object above; the existing step assertion on `steps.findings.output.score` is wrongly rejected. Extract schemas and unknown code outputs share the selector owner but do not produce this literal-pattern state. Every caller of `admits` is one of the input/output branches above; actions, conditions, Agent/code bindings, and assertions funnel through `validate_reference`, so per-caller guards would duplicate the cause.
- **Observable consequence:** valid no-step Verifiers and valid preparation-output references cannot be authored/validated despite their field being allowed by the declared schema. This staged pure-contract rejection is material to Task 001; no mounted runtime exploit is claimed.
- **Minimal correction:** correct the field-presence decision once in existing `Walk::admits`. Reuse the existing bounded `literal_name` recognition so an exactly named pattern declaration is considered before concluding that `additionalProperties:false` proves absence, and inspect its remaining path under the same existing walk budget. Preserve named `properties`, local references, ordinary known-missing refusal, unknown-output fallback and Verdict comparison behavior. No new dependency, regex engine, general pattern inclusion, schema validator, public profile or runtime guard is necessary. Do not merely disable the missing-field check or prohibit patternProperties. Complex regex policy and numeric/string constraint strength remain outside this finding.
- **Focused proof:** extend the existing public validation fixtures with the exact schema above through (1) a JSON input slot and (2) a step Agent output, including inline and resolver output production. Existing `review()` and cyclic-schema fixture patterns already exercise those consumers. `score` and a nested literal pattern path must validate; an absent sibling under a schema whose only declaration is `^score$` must still return `missing_field`. Retain current cyclic/local-ref and Verdict compatibility negatives. Prove RED/GREEN through the exact existing `card::blocks::validate::tests::verifier_spec_refuses_invalid_graphs_and_references` and/or `verifier_spec_bounds_cyclic_schema_references` selections when extending those tests. No server/provider/journey harness is warranted. This reproduction was source-traced, not added or executed during this review.

### Suggestions

None. Optional hardening and unrelated style changes were excluded.

## Complete cumulative obligation matrix

| Requirement / criterion / constraint / non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001: subject-less inputs/steps/requirements/decide, no-step and empty forms, global IDs | `blocks/mod.rs`; `VerifierSpec::requirement_owners`, `validate.rs:134-172` | `verifier_spec_round_trips_and_validates`, graph/refusal tests | PASS |
| REQ-002: slot vocabulary, required defaults, valid names, schemas/formats/positive limits | `InputSlot`, `SlotName`, `validate_slot`; runtime input-value enforcement explicitly later | Slot/ID/refusal tests in 1139-library run | PASS |
| REQ-003: grammar, dependency inference, ID ownership, visibility, cycles, known/unknown selector semantics | `Reference`, dependencies/ancestors, existing `plan_stages`, `validate_reference`, `Walk::admits` | Reference/DAG tests and fresh recursion/selector tests pass; exact literal-pattern selector cases absent | FAIL — INV-R2-001 |
| REQ-004: three closed actions and all nine exact extraction formats, mandatory nullable fields, source ordering/locator semantics | `Action`, `ExtractOutput`, PDF/PPTX/DOCX/XLSX/table/cell types; reuse of generated extraction schemas | Locked examples, missing/extra-field refusal, schema selection tests | PASS |
| REQ-005/007: five checks, typed comparisons/trace/drift, Prompt-derived Agent inputs/schema, position constraints | `Check`, `ComparisonOperator::validate`, `ResolvedAgent`, `validate_agent` and `validate_placeholders`; generated `Verdict`/Citation/Locator passed into bounded fit owner | Fresh focused Verdict test passes canonical, local-ref, allOf/string finite-set, extra literal-pattern and prior incompatible cases; step/whole, inline/resolver paths retained | PASS within approved bounded comparison |
| REQ-006: missing/inconclusive/not-applicable distinctions and honest result/decision representation | `Decide`, `RequirementOutcome`, `StepStatus`, reasons, `BlockError`; runtime aggregation later | Pass-rate bounds and result round-trip; no fabricated runtime implementation | PASS |
| REQ-008: complete closed tool catalog, action workspace/tool restrictions, tool-less checks | `BuiltinTool::ALL/from_name`; one catalog reused by validation/scope/provenance; `validate_agent` | All-builtins and position/media tests in owner run | PASS |
| REQ-008/INV-003: exact/global scope validation/coverage/intersection and no implicit domain grant | `PermissionScope::Tool`, `PermissionWire` validation, `Resource::Tool`, `Permission::covers`, generic `PermissionSet::intersection`, existing authority encoding; gateway sibling ignores Tool scope | Tool permission and accepted-authority exact scope tests pass in owner run | PASS |
| REQ-008/015: tool arguments, bounded output digest and byte count preserved at seam/result | `ToolCallRecord`, `ActionOutput.tool_calls`, `StepResult.tool_calls`; output-less digest/count semantics documented | Wire round-trip and `action_tool_calls_carry_into_step_results` pass | PASS |
| REQ-009: original-slot citations travel in ordinary output with source-slot provenance | `Citation`/`Locator`, `CallInput.sources`, JSON action output and check output | Executor resolved-input and result locator round-trips; runtime citation existence/authorization later | PASS |
| REQ-010: one canonical format vocabulary and extraction/native/media compatibility | `MediaFormat::{from_str,as_str,native_kind,is_extractable,is_archive}`, Prompt.media_kind | Format aliases, canonical serialization, legacy Office distinction and position/media tests | PASS |
| REQ-011–013: Run/Window distinction, one request/result/status/identity contract, delayed conflict and result visibility representable | `CheckEvidence`, evidence-mode validation, execution wire declarations, `ExecutionId`, existing `VerificationError`/status reuse | Request/result/error/ID and mixed-evidence tests pass; no PG reservation or live route introduced | PASS |
| REQ-014: explicit schedule mapping, required slots, exact measurements, existing drift contract reuse | `ScheduledInput`, `validate_schedule`, reused `DriftSpec` and report/window | Schedule-input and drift-source/feature negative tests pass | PASS |
| REQ-015 attributable results and minimum injected boundary | `BlockCall` position/id/deadline/inputs/workspace; per-execution accepted-authority construction contract; `ActionExecutor` vs `CheckExecutor`; source/result/provenance types | Dyn action seam and tool handoff tests; future runtime enforcement not claimed | PASS |
| Existing Card/reference/Prompt/operator/DAG owner reuse; no independent loader/graph store | `Visit for blocks::VerifierSpec`; existing plan generalized with old validate_dag delegating; operator validation shared | Reference visitor and existing planner/operator suites pass | PASS |
| INV-001 foundation pure/async-free/PyO3-free; async seams owned by Vala | Cumulative diff adds serde/schemars/pure contract logic only in foundation; async only injected executor seam | Prior boundary/owner compile evidence; no manifests/dependencies changed | PASS |
| INV-003 accepted authority intact and domain permissions independent | Exact tool scope stays in permission and authority owners; no credentials/tenant auth fields authored in execution messages | Permission/accepted-authority tests; live protected tool dispatch deferred | PASS |
| INV-006 honest evidence/identity/failure representation | Distinct error/outcome/status enums, original evidence and source slots, exact IDs/provenance | Wire/seam/ID tests | PASS |
| INV-008 staged replacement: no dual live protocol, no aliases, final legacy removal handed off | New block/execution modules exported but not mounted in Spec/routes; original live consumers compile until Task 005 | Existing owner tests plus prior workspace all-features check evidence | PASS |
| Mandatory changed-item docs including previous FIND-6 omissions | Reference and MediaFormat fmt now include accurate Errors; R2 helper docs explain bounded role | Static inspection and prior lint/format evidence | PASS |
| AC-001/009/010 scoped isolated proof and staged handoff | Task switch-point inventory maps all exports to consumers; no premature generators/routes/SDK cutover | Fresh 1139 tests and three exact schema selections pass; new selector case is not covered | FAIL only for confirmed requirement gap above |
| Non-goals: runtime tool/sandbox/engine projector/storage/queue/SDK work and full journeys absent | Complete 26-file cumulative diff; changed gateway match only preserves exhaustiveness | Static diff; integrated verification explicitly deferred under task coordination | PASS |

## Six prior closures, independently source checked

| Stable ID | Current closure evidence | Disposition |
|---|---|---|
| FIND-TASK-001-1 tool permissions | Both wire forms represented by Tool resource and All/exact Tool scope; strict PermissionWire calls validation; exact scope equality and generic narrow meet retain identity; no built-in role grant added; gateway `invocable` preserves its own grant reach; authority round-trip retains subject/delegation | CLOSED |
| FIND-TASK-001-2 catalog | All ten approved builtins occur in one closed catalog; step membership uses that catalog; permission scope and tool records type the same enum; unknown/check/workspace refusals intact | CLOSED |
| FIND-TASK-001-3 bounded Verdict fit | `fits` now falls through to sibling keywords after allOf; finite values infer primitive types; exact anchored literal extra fields are checked in closed objects while same-name refinements remain allowed; generated typed source, existing negatives and both positions/resolution modes retained | CLOSED within approved comparison; selector defect above is distinct |
| FIND-TASK-001-4 tool provenance | Shared ToolCallRecord has typed `output_sha256` and required `returned_bytes`; ActionOutput hands the same records into StepResult; result/seam tests retain values and honest no-output case | CLOSED |
| FIND-TASK-001-5 schema recursion | One Walk budget spends on recursive admits/fits/values calls and every ref hop; exhaustion retains unknown-selector semantics and refuses unproved Verdict fit; self/mutual refs and finite known-missing test pass freshly | CLOSED |
| FIND-TASK-001-6 rustdoc | Reference Display docs at reference.rs:235 and MediaFormat Display docs at media.rs:304 now explicitly document formatter write errors. Previously repaired helper/codec/trait/test-module documentation is retained; no formatter body changes | CLOSED |

## Ownership and regression assessment

The complete existing DAG and comparison validation owners were extended rather than replaced. The visitor owns Card/Artifact/Prompt/baseline references; no new resolver or persistent graph is introduced. The staged validator has meaningful per-pass resolver/ID/ancestor state. The schema walker retains meaningful per-analysis work budget; reusing that owner and literal-name recognition is smaller than an added validation layer. Media vocabulary belongs beside existing media, not foundation-specific duplicate strings. New async executor traits represent the genuinely different action/check implementations that later tasks supply; no fake success, secondary orchestrator, new runtime or additional durable writer exists.

The schema failure was followed from both actual producers through the shared consumer. `fits` and `admits` deliberately have different unknown outcomes (unproved fit refuses; unproved selector absence admits); the correction must retain that distinction. Numeric bounds, arbitrary regex inclusion and string-format strength are excluded by the approved remediation and are not findings. The new proposal does not require interpreting arbitrary patterns; it needs the same exact-name declaration already recognized by the selected bounded structural mechanism.

## Open Questions

None affecting the bounded correction. Final finding identity and validation remain the independent validator's responsibility.

## Verification Notes

Freshly executed at the candidate:

```sh
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::blocks::validate::tests::verifier_spec_requires_check_agents_to_produce_a_verdict) | test(=card::blocks::validate::tests::verifier_spec_bounds_cyclic_schema_references) | test(=card::blocks::schema_path::tests::schema_path_refuses_only_known_missing_selectors)'
# 3 passed, 866 skipped
mise exec -- cargo nextest run --locked -p wyrd-spec -p wyrd-runtime -p wyrd-skald-spec -p vala-eval --lib
# 1139 passed, 0 skipped
```

Prior task evidence records focused R2 RED/GREEN, all-features owner Clippy, fmt, dependency/codegen and workspace compilation checks; those non-test lanes were inspected as recorded evidence, not rerun by this reviewer. No gate was weakened and no baseline checkout was used. Green existing tests do not cover INV-R2-001's exact input/output selector state.

Skipped: server/provider/SQL/sandbox fault runs, SDK journeys and final judge cutover, which are outside this staged task. Risk: valid literal-pattern field references remain rejected until the shared selector decision is corrected.
