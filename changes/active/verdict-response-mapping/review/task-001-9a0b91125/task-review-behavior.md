# TASK-001 independent behavior review

## Immutable subject and scope

- Reviewed tree: `/tmp/wyrd-task-review-9a0b91125`, clean detached checkout at `9a0b9112523252328813041c0b11be375f0eed29` before and after source inspection.
- Base/spec approval and original task: `08998b9e9b6f04825f7743b877391247e0ad9621`.
- Actual cumulative diff: `git diff 08998b9e9...9a0b91125`; nine implementation commits beginning `5a3caa8b3`.
- Contract: SPEC-verdict-response-mapping revision 1. TASK-001 owns Jev leaf policies; aggregation and Google Decisions remain TASK-002/TASK-003.
- Primary scope: requirements, scenarios, actual assertions, authoring/declaration/execution behavior, direct-Verdict preservation, documentation and TDD. SDK projection/allocations are assigned to repo-rev; waits, capture/tenancy/lifecycle and benchmarks to system-rev. This report does not substitute for those specialist reports.

Read AGENTS.md, spec-driven-development.md, implementation-execution.md, maintainer-style.md, TESTING.md, agent_docs index/rust/agent-surfaces, SDK/docs directory rules, approved spec/task and candidate evidence. Applicable architecture: Prompt, Verifier, In-memory authoring, Recording and eventual consistency; Bifrost Storage format and Variant; security Trust boundaries, Tenant and data isolation and Audit integrity/privacy. The detached tree has no `.codegraph/`; no index was created and the dirty main checkout's indexed source was not used.

## Connected source inspection

The existing Prompt builder constructs Jev requests (`skald-prompt/src/builder.rs:621–647`) and validates native limits through JevRequestView. AgentCheck flattens AgentCall and carries the optional policy (`wyrd-spec/src/card/verifier/transform.rs:31–94`); reference traversal visits `check.call` (`refs/mod.rs:371`). Local graph validation resolves siblings (`wyrd-loader/src/validate.rs:29–86`) and shares policy validation with authoritative registration (`wyrd-server/src/components/cards/service.rs:1525,1999–2065`). Registered Agents/Prompts are read in the existing tenant declaration transaction before invocation.

Vala moves the run's existing final_response Arc into Invoked (`blocks/agent.rs:519`), invokes once, and applies the explicit policy over typed native request/answer (`agent.rs:582–603`, `transform.rs:61–150`). Choice checks declared labels and numeric metadata before mapping. Probability applies authored polarity/inclusive thresholds. Score reuses evaluate_operator. The selected answer supplies existing CheckOutput.actual; reasons are generated, citations empty, provenance preserved. With no transform, execution retains the original structured Verdict/verdict_from branch. No second provider call or result envelope was introduced.

## Requirement/scenario matrix

PASS below means credible source and recorded focused execution evidence within primary scope; specialist findings can still prevent final acceptance. Recorded execution results are bound to the candidate task and corroborated by inspecting actual assertions. No test was rerun by this reviewer.

| Requirement / scenario | Implementation and inspected assertions | Assessment |
| --- | --- | --- |
| REQ-001/002 Jev; 1–2, J1/J8 | Ordinary Prompt::jev/native question lowering. Rust gateway_jev.rs:473–540; Python test_gateway_jev.py:510–586; TS gateway-jev.test.ts:409–467 compare typed/native/mixed values and hydrated stored Prompt/check fields. Optional-Pydantic dictionary proof is in test_public_exports.py. | PASS within primary scope; projection/allocation conclusions require repo-rev. |
| REQ-003 Jev; 1, J1 | All SDK Choice journeys inspect actual /v1/systemone request with literal quoted/newline state, questions/model and one upstream invocation per check. Existing rendering owner remains in use. | PASS. |
| REQ-005; 1–4/6–7 | AgentCheck, authored/compiled traversal and absent-policy branch; stored policy checked in Python/TS journeys; action refusal in Python authoring test. | PASS SDK/HTTP behavior; MCP proof fails B-001. |
| REQ-006; 1/3 | Choice branch/choose and choice_maps_labels_and_ignores_confidence; hosted custom compliant/violation/unknown mapping, low-confidence pass and high-confidence failure, exact reason/actual. | PASS. |
| REQ-007; 3 | threshold at transform.rs:195–233; probability_thresholds_are_inclusive_for_both_polarities; SDK hosted policies cover true inclusive bounds, false polarity and middle interval. | PASS. |
| REQ-008; 3 | grade at transform.rs:241–278 reuses evaluate_operator; Score 2 versus 1.8 hosted; pure tests add alternate/approximate operators. | PASS arithmetic; system-rev assesses native metadata validation. |
| REQ-010; 3/5 | Native tag/key dispatch; unusable_answers_are_check_errors includes native nonfinite Noul; hosted missing answer, wrong tag, unknown label and invalid Noul/Score affect only the reading requirement. Existing gateway refusal/retry tests retained. | PASS examined cases; remaining native metadata validation assigned to system-rev. |
| REQ-011; 4, J6 | Shared local/sibling/registered validation. Rust registration journey:824–950 checks raw HTTP bypass, field diagnostics, no upstream call, and corrected execution. Python:760–802 and TS:645 onward exercise exact registered Agent CardRef, local success, authoritative refusal and correction. | PASS within primary scope. |
| REQ-012; 1/3/6 | Exact generated reasons/selected actual asserted in policy journeys; full/hash retained reads compare returned results and foreign tenant 404. | PASS SDK/HTTP assertions; B-001 MCP gap and system-rev provenance/capture remain. |
| REQ-013; 2/4/7 | Public questions/leaf policies, runtime schemars unit test, generated TS policy, docs and journey-tested refund example. The shipped MCP details contract was incorrectly excluded. | FAIL — B-001; null-contract parity independently corroborated below. |
| REQ-014; 5–7, J7/J8 | Original no-transform Verdict path preserved; task records 48 combined gateway_jev/verifier_building_blocks/authoring journeys; existing native Jev 400/401/429 and ordinary action construction preserved. Each SDK denies foreign retained read with exact code. | PASS within primary scope, subject to system-rev. |
| Spec allocation/hot-path constraints | Sharing typed response and selected-answer serialization inspected. Mandatory wait inventory/benchmarks and authoring allocation proof need specialist assessments. | UNRESOLVED here; assigned, not waived. |

Scenario 2 names reconstruction and acknowledges saved-bundle fromPath was not tested. I do not retain a separate defect solely for that mechanism: declare already pulls/hydrates the registered graph, tests inspect stored check/Prompt and compare equivalent redeclarations, satisfying REQ-001 native/read-back/redeclaration meaning. No saved-bundle execution proof is claimed.

## Proposed blocking finding

### B-001 — EVIDENCE_GAP: the shipped MCP execution/details boundary is omitted on a false inapplicability claim

**Authority:** REQ-005 existing execution/inspection surfaces; REQ-012 retained read/provenance; REQ-013 public MCP projection; spec Acceptance observations requires shipped HTTP/MCP/CLI boundaries. Original TASK-001 scenario 6 explicitly requires applicable MCP execute/read proof and scenario 7 runtime MCP contract checks.

**Locations:** candidate task `TASK-001-jev-verdict-checks.md:370–375`; server `src/mcp/verification.rs:57,210–222,429–439`; server `src/mcp/mod.rs:442–445`; existing MCP test owner `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs:724–741,933–1074`.

**Starting conditions/trigger:** an agent uses verification.execute and verification.get_execution_details for a registered Jev-policy Verifier after asynchronous recording becomes visible.

**Expected:** representative executable proof through this shipped boundary that canonical verdict/reason, authored check policy, selected native capture and provenance project through the real MCP catalog/result path, preserving existing negative/read-denial behavior.

**Actual/missing proof:** scenario 6 says “no MCP tool or CLI command reads verification execution details,” treating HTTP details as the sole applicable owner. The candidate already registers verification.get_execution_details with VerificationDetails, dispatches to VerificationControl, and has a details journey/helper. No candidate diff or task execution evidence exercises transformed Jev policies through this owner or its runtime contract. The unchanged details journey explicitly tests `kind: assert`; it does not prove the changed AgentCheck/policy projection. This is missing required proof, not a demonstrated production failure.

**Consequence:** an agent-facing surface would be accepted without verifying its changed check contract; the stated exclusion is factually incorrect.

**Smallest testable correction:** extend the existing MCP verification journey owner, reusing its server/provider setup and details helper, with representative Jev leaf execution/retained read and relevant runtime descriptor assertion. Assert policy, canonical reason/result, selected capture and provenance; retain representative existing negative/capture semantics. Run the exact focused journey selector and correct the limitation in task evidence. Add no tool, route or envelope, and require no production change without a demonstrated defect. Full MCP suites belong to integrated review.

## Independent corroboration of repo-rev null-contract finding

Generated public CheckAgent (`sdks/wyrd-sdk-ts/wyrd/src/card-types.ts:851`) advertises `verdict_transform?: VerdictTransform | null`. AgentCheck derives JsonSchema over Option<VerdictTransform> (`transform.rs:31–45`), but its custom Deserialize removes a present member and calls from_value::<VerdictTransform> (`transform.rs:67–70`): explicit null fails while omission succeeds. TS native transform also deserializes a non-optional VerdictTransform (`native/src/authoring.rs:427–433`). This confirms disagreement for a public advertised value. Retain the final finding once through repo-rev/validator, not as a duplicate here.

## Behavioral acceptance and TDD/process history

- Scenario 1 records a runtime missing-behavior “no structured Verdict” refusal in addition to compile errors. Compile/setup errors are not RED.
- Scenario 2 missing-module and missing-constructor failures show absent public runtime authoring surfaces.
- Scenarios 3, 5 and 6 explicitly had no RED: their behavior shipped with scenario 1 before these connected journeys. Preserve test-first process deviations separately. Already-correct underlying behavior may pass initially, but later GREEN does not establish test-first history for behavior introduced in this change. Do not manufacture RED or revert correct production code.
- Scenario 4's run with validate_registered_transforms disabled is mutation evidence of assertion strength, not historical RED. Python/TS DID NOT RAISE observations show missing validation; committed history does not recover all intervening local chronology.
- Loader-path/mock-marker errors, omitted TS criteria undefined, and flat-versus-result retained-read mismatch are test/setup defects, not product RED. Task records concrete causes/fixes. Fresh diagnostician reports/traces for these unexpected fixes are not present, so full diagnostic-process compliance is not independently established. This records history rather than requesting production churn.

## Verification limits and conclusion

No tests or benchmarks were rerun. Focused commands/results and actual assertions were inspected; the maintainer's final Python/TS whole-file report is 18 passing each. Propose FIX_REQUIRED for B-001 pending independent validation and specialists. No implementation changed. Aggregation, Google onboarding, broad integrated suites and optional refactors were excluded.
