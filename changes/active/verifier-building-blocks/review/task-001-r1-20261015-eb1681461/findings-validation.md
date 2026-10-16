# Independent final findings validation — TASK-001 R1

**Recommendation: FIX_REQUIRED.** Two existing findings remain partially open: **FIND-TASK-001-3** and **FIND-TASK-001-6**. Findings 1, 2, 4 and 5 are closed. No new finding ID, Critical issue, live authorization bypass or specification revision is established.

## Immutable subject and authority

- Original base: `08b38f74255d344e1c5ec9a6d278935a26277622`.
- Prior candidate: `896e563bee69ab6b5f47b31977d7de1a0ca88d57`.
- Cumulative candidate: `eb1681461897ee1eb9fe1fa4e00cb0b435658559`. HEAD matched; the candidate-relative `crates/` diff was empty at final inspection.
- Reviewed the original task/spec r6 snapshots, prior verdict/validated ledger/R1 task, current remediation snapshot, complete change inventory and cumulative/latest diffs, both implementation reports, standards, maintainer, system, security, claim comparison and fresh focused follow-up. Findings are validated against actual source, not reviewer agreement or implementation summaries.
- Governing authority: AGENTS.md, agent rules, active design/doctrine, spec-driven-development authority order and maintainer guidance, plus the invoked task-review skill. CodeGraph preceded source discovery. Source and earlier artifacts remain unchanged; this report is the only assigned write.
- The approved subject-less building-block replacement is staged. Task 001 explicitly requires public pure authoring/validation and compile-consumable seams even before server activation. Its lack of a live endpoint does not make these contract defects unreachable. Server mounting, hosted tools, sandbox, queue, providers and SDK journeys remain later tasks.

The current owner direction and TASK-005 handoff explicitly assign **full legacy LLM-judge removal, including offline evaluation**, to TASK-005. Temporary retention keeps the base compiling. Neither immediate removal nor indefinite preservation is a condition of this review. The approved new seam uses BlockError. Scenario simulation is not blanket-removed. This is human-directed scope, not a retroactive FIND item.

## Per-proposal dispositions

| Proposal or closure claim | Disposition | Independent source result |
|---|---|---|
| BEH-R1-001 / INV-R1-001 / SEC-R1-001: authored allOf discards compatible sibling structure | CONFIRMED | `schema_path.rs:128-131` returns solely from authored allOf parts. Reason `{"type":"string","allOf":[{"maxLength":500}]}` therefore loses its string type and reaches false at :152. The equivalent direct maxLength form passes. The outcome enum plus redundant allOf minLength case fails for the same reason. Consolidated under FIND-3. |
| BEH-R1-001: finite string const/enum reason falsely refused | CONFIRMED | `fits` uses authored `values` only when typed `values` exists (:133-136). Typed reason has type string and no finite set. Authored `{"const":"fixed explanation"}` / `{"enum":["ok","rejected"]}` reaches `types(authored)==[]` and false at :152 despite admitting only strings. Same structural-fit owner and FIND-3. |
| INV-R1-001: patternProperties `^score$` defeats closed Verdict | REVISED | The concrete acceptance bug is confirmed: `fits_object` :187-211 considers only additionalProperties/properties/required. Adding `"patternProperties":{"^score$":{"type":"number"}}` to the accepted fixture leaves every inspected fact unchanged, so it passes. A response with score:1 conforms to that declaration but violates Verdict's deny_unknown_fields. Retain the exact structural case; narrow correction to local literal property evidence, not arbitrary regex inclusion. |
| RS-R1-001 / MNT-R1-001 / INV-R1-002 and follow-up: two fmt error sections missing | CONFIRMED | Full methods at `reference.rs:235` and Skald `media.rs:304` return actual formatter-write errors; summaries lack Errors sections. AGENTS.md:723-737 explicitly requires those sections for every fallible changed method, without a trait exemption. FIND-6 remains narrowly open. |
| Behavior report's complete FIND-6 closure | REJECTED | Summary rustdoc was added, but the two required error sections remain absent. Deferring full compliance to another role does not establish closure. The focused follow-up resolves this correctly from authority and source. |
| Closure claims for FIND-1, FIND-2, FIND-4, FIND-5 | CONFIRMED | Independently checked at their shared owners, producers, consumers and tests; details below. |
| System review: no additional deployed-path regression | CONFIRMED within this task | New block executors have only test implementations; staging adds no worker, durable writer, provider dispatch, shutdown or recovery owner. Existing gateway change is an exhaustive disjoint-scope arm. Pure contract defects above remain independently required. |
| Generic schema/regex inclusion engine, arbitrary witnesses, dependency addition, exact generated-schema equality or new public authoring profile as remediation | REJECTED | None is necessary to correct the concrete local cases. Blanket refusal of all allOf/const/enum/patternProperties would itself reject compatible restrictions. Do not prescribe it. |
| Looser numeric bounds, string format or pattern-strength checks as additional findings | REJECTED as new findings here | These are explicitly outside the selected prior bounded structural correction. No independent newly proposed reachable obligation justifies expanding this review into complete schema inclusion. Runtime typed decoding remains required later. |
| Older missing-codegen objection / broad-rerun requirement | REJECTED as remaining gaps | Fresh logs establish successful codegen and owner tests. Missing concrete cases are coverage holes, not evidence that the existing tests failed or that full server journeys are due in this remediation. |

## Producer, owner, sibling consumer and existing-alternative validation

| Boundary | Source trace and full relevant owner bodies inspected | Reuse decision |
|---|---|---|
| Derived response → check validation | Skald `Prompt::new`, response derivation, `check_output_schema`/`check_schema` (`prompt.rs:235-314`) → AgentSpec/compiled Prompt → `VerifierSpec::validate/validate_with` → Validator run → step/requirement dispatch → full `validate_agent` (`validate.rs:492-581`) → complete `Walk::fits`, `fits_object`, `values`, `types`, `required` → typed Verdict/Citation/Locator (`execution.rs:339-391`). Both StepCheck/VerifierCheck and inline/resolver modes share this guard; action mode skips it. | Existing Prompt owner checks syntax and provider constraints; it accepts all concrete proposals. It does not prove Verdict fit. Existing generated typed Verdict is the shape source. Retain the position-specific guard where it owns the invariant; do not duplicate it in each executor, alter Prompt authoring, or weaken typed decoding. |
| Scope references → known-shape consumers | Complete bounded Walk/resolve/admits; `validate_reference`, `output_schema`, `validate_slot`; slot schema and resolved Agent output producers; extraction output schema consumer; cyclic/finite recursive tests. | Existing narrow analysis is justified by known-missing selector requirements. One budget spans every recursive call/ref hop. Unknown selector shape admits; unproven Verdict fit refuses. These are distinct intended results. |
| Authored/stored grants → effective authority | BuiltinTool complete catalog; PermissionScope validate/covers; PermissionWire conversion, full Permission::validate/covers/subsumes; PermissionSet insert/contains/intersection; role `permission_set_from_rows` at `wyrd/wyrd-auth/src/issuance.rs:797`; Principal/AcceptedAuthority new/serde; RbacCheck. | Existing three-axis RBAC performs decoding, attenuation and checking. Tool is a normal disjoint scope, not a second service or token profile. Exact Tool requires Tool/Invoke; All retains existing general behavior; wildcard meets narrow correctly. Gateway's special intersection semantics remain intact. |
| Shared vocabulary → authoring and live sibling consumer | BuiltinTool ALL/from_name/as_str → action validate_agent membership and filesystem/workspace rules; same enum → permission scope and ToolCallRecord. Full gateway `invocable` (`batches.rs:169-195`) filters Gateway/Invoke before scope projection; real listing/invocation retains authoritative decision. | No incomplete second tool catalog remains. Tool scope grants no gateway reach. Check tools/workspace refusal and independent Cards/Bifrost grants stay intact. No default Tool grant was introduced. |
| Future output producer → result wire | Full ActionOutput/CheckOutput/BlockCall/BlockError/executor seam; ToolCallRecord (`execution.rs:315-336`); StepResult; handoff and result round-trip tests. | One shared typed record carries optional Sha256Hex and returned_bytes. No-output calls honestly omit digest with zero bytes. No production projector exists to repair or demand here; pure handoff proof is appropriate. |
| Existing alternatives and cumulative regression boundary | Existing `validate_dag`/complete `plan_stages`, operator validator/delegate, Visit Agent/code/drift inventory, staged module exports, media/ref codecs and their consumers. | Existing planner/operator/visitor/Prompt owners are extended. Scope references are not Card loader refs or existing `$` JSONPath. No duplicate loader, runtime, permission store or schema fetch is warranted. |
| Documentation | Complete Reference fmt/serde/FromStr bodies; MediaFormat fmt/serde/FromStr; remediated validation helpers, DriftCheck codecs and named test module docs. | Add two accurate error sections in place. No wrapper, permanent check, behavior test or unrelated documentation sweep. |

The full correction owner for FIND-3 has no external IO. Its inputs are finite authored JSON and its descendants/ref hops use one private budget. Correction must preserve that bound and the adjacent selector behavior. All proposed concrete schemas pass upstream syntax: `patternProperties` and `allOf` are explicitly traversed; const is retained and enum arrays are accepted. The root remains a typed object, so strict OpenAI object-root validation does not screen out these examples.

## Prior-finding closure

| Stable finding | Closure | Exact current source and proof assessment |
|---|---|---|
| FIND-TASK-001-1 | CLOSED | `auth/permission_scope.rs:54,153-163`; runtime `permission.rs:149,342-372,789-809,980-1033`. Both wire forms decode through validated PermissionWire; exact names cannot widen or cross domains; existing intersection and role/AcceptedAuthority preserve them. Permission/accepted-authority focused tests pass. |
| FIND-TASK-001-2 | CLOSED | `card/agent.rs:61-139` contains all ten names; `validate.rs:535-542` uses from_name. The four formerly omitted names validate without workspace; filesystem/workspace/check-position refusals remain. All-builtins public validation test passes. |
| FIND-TASK-001-3 | PARTIALLY CLOSED / OPEN | Generated typed schema replaces three-required-name heuristic; canonical/ref/annotation/subset positives and original incompatible type/enum/citation/extra-property negatives exist. Remaining shared structural cases are below. |
| FIND-TASK-001-4 | CLOSED | `execution.rs:315-336`, `StepResult.tool_calls` and `vala-eval/blocks.rs:83-94` share digest/count/error metadata. Result round-trip and action-to-result value handoff tests pass. No claim that a deferred runtime producer is implemented. |
| FIND-TASK-001-5 | CLOSED | `schema_path.rs:14,41-69,73-117,122-245`: each recursion and ref hop spends the same finite budget, including values enumeration. Public cyclic input/output and finite-recursive known-missing tests exist and pass. No stack-overflow reproduction was attempted or needed. |
| FIND-TASK-001-6 | PARTIALLY CLOSED / OPEN | Previously missing summaries/helper Errors/test module docs are repaired. Two fallible Display methods still lack mandatory Errors sections. Follow-up confirms this residual; source and rule independently agree. |

## Final deduplicated ledger

### FIND-TASK-001-3 — Important — Complete the bounded Verdict structural comparison

- **Disposition / classification:** REVISED / INCORRECT; residual prior finding.
- **Discovery aliases:** BEH-R1-001, INV-R1-001, SEC-R1-001.
- **Violated obligation:** REQ-005/007 and original Task 001 Scenario 3; R1 explicitly requires incompatible field structure refusal while preserving compatible additional restrictions without a new authoring profile.
- **Exact locations:** `crates/wyrd-spec/src/card/blocks/schema_path.rs:128`, `:149`, `:187`; consumer `validate.rs:568`; typed boundary `verification/execution.rs:339`.
- **Reachable consequences:** valid check Agents using reason type+allOf refinement or finite string const/enum are refused as check_agent_schema; a declared pattern-added score field is accepted although conforming model output containing score cannot decode as Verdict. Both check positions and resolution modes share the cause. No mounted-route exploit is asserted. Runtime malformed-output handling cannot repair a declaration refused before execution and does not replace required early structural refusal.
- **Minimum selected correction:** extend the existing bounded Walk at the current check-position guard, retaining generated Verdict/Citation/Locator as the shape source. Preserve same-node structural evidence when allOf merely conjoins refinements; a sufficient sibling type/enum must not be discarded. Reuse the current finite-values analysis to establish primitive string shape from a string const/enum, without requiring redundant type. In closed-object comparison, account for ordinary exact anchored literal-name pattern properties such as `^score$` as named property declarations: an admissible score field must meet the same closed-property refusal as properties.score. An exact-name refinement of an existing field such as `^reason$` is not an extra field and must not be blanket-refused. This bounded literal-name case selects the necessary correction; it does **not** ask the implementer to decide a general regex acceptance policy or solve arbitrary regular-language/schema inclusion. Keep existing conservative behavior for analysis outside established structural evidence, and do not ban keywords or announce a new authoring profile.
- **Preserve:** action output freedom, Prompt-derived schemas/media/placeholders, tool/workspace refusals, prior incompatible outcome/reason/citation/locator/extra-field negatives, local refs/annotations, shared traversal bound, selector unknown/known-missing semantics and later malformed-output refusal. No extra durable fields, runtime executor changes, provider/schema fetch, dependency, witness generator or alternative validation platform.
- **Focused closure proof:** extend the existing public validation test with reason `{"type":"string","allOf":[{"maxLength":500}]}`, finite string const and enum positives; include numeric/mixed finite-value negatives. Add equivalent outcome enum plus redundant allOf refinement. Add root patternProperties `^score$` number negative and same-name `^reason$` maxLength refinement positive. Keep the existing canonical/local-ref and incompatible suite. Exercise the shared guard through both check positions and inline/resolver Agents using existing fixtures, retaining arbitrary structured action output. The cases need no server/provider/journey harness. Run the exact existing owner test:

```sh
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::blocks::validate::tests::verifier_spec_requires_check_agents_to_produce_a_verdict)'
```

These new cases were proven by source trace during this read-only review, not added or executed. No broad theorem suite is required for closure.

### FIND-TASK-001-6 — Important — Document the two formatter failure contracts

- **Disposition / classification:** CONFIRMED / VIOLATION; residual prior finding.
- **Discovery aliases:** RS-R1-001, MNT-R1-001, INV-R1-002; fresh follow-up resolved the closure discrepancy.
- **Exact locations:** `crates/wyrd-spec/src/card/blocks/reference.rs:235`; `crates/skald/skald-spec/src/media.rs:304`.
- **Authority/evidence:** AGENTS.md:723-737 requires rustdoc for changed trait items and Errors on every fallible method, explicitly blocking completion. Reference fmt propagates write errors with `?`; MediaFormat fmt directly returns `write_str` failure. Both have only summaries. There is no trait-method exception and no inference that canonical formatting itself is wrong.
- **Consequence:** required changed-item documentation gate is incomplete. This is the skill's explicit rustdoc exception to non-blocking wording/style findings.
- **Minimum correction:** add an accurate Errors section to each current method documenting formatter/write failure. Keep bodies and canonical output unchanged. Batch with FIND-3; no new checker, abstraction, behavior test or separate style-only round.
- **Focused proof:** static inspection of both complete docs and normal formatting. Existing narrow lint/owner proof applies to the combined remediation; tests cannot establish missing prose.

## Open questions

None affecting this bounded correction. A generic schema profile is neither required nor authorized; full judge removal already has an explicit later owner.

## Verification notes and limits

Inspected fresh pinned-candidate logs: `library-check.log` concludes **1139 passed, zero skipped** across the four owning libraries; `focused-check.log` concludes **9 passed**; `codegen-check.log` records successful source-generation checks. Subject records successful cumulative whitespace check and unchanged generated source. Existing focused schema tests omit the newly confirmed cases. Their green status does not establish those cases.

R1 evidence additionally records all-features owner Clippy, workspace all-features test compilation, fmt and dependency checks; raw fresh logs for those were not independently rerun by this validator. No failures were hidden or attributed to baseline. No new test harness, production edit, or broad rerun was performed. Candidate/production source stayed pinned.

Skipped: provider/server/SQL/sandbox fault runs and SDK journeys, which remain outside this staged task. Risk: current public validation still misclassifies the concrete Verdict declarations above; later runtime safety and final integrated journeys require their own assigned proof.
