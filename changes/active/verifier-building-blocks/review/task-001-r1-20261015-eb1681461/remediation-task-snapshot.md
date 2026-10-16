---
id: TASK-001-R1
kind: remediation
status: ready
spec: SPEC-verifier-building-blocks
spec_revision: 6
requirements: [REQ-003, REQ-005, REQ-008, REQ-015, INV-001, INV-003, AC-001, AC-010]
depends_on: []
parent_task: TASK-001
remediates: [FIND-TASK-001-1, FIND-TASK-001-2, FIND-TASK-001-3, FIND-TASK-001-4, FIND-TASK-001-5, FIND-TASK-001-6]
---

# Complete the Task 001 contract handoff

## Authority, subject and routing

Execute through `$wyrd-implement`. This packet authorizes bounded remediation, not a new design or the later runtime implementation.

- Approved spec: `changes/active/verifier-building-blocks/spec.md`, r6; immutable review copy `spec-snapshot.md` beside this task.
- Original task: `changes/active/verifier-building-blocks/tasks/TASK-001-verifier-contract.md`; immutable review copy `task-snapshot.md`.
- Repository: `/home/thorrester/Documents/GitHub/wyrd-verifier-building-blocks`.
- Original review base: `08b38f74255d344e1c5ec9a6d278935a26277622`.
- Reviewed cumulative candidate / remediation starting point: `896e563bee69ab6b5f47b31977d7de1a0ca88d57`.
- Review: `verdict.md`; independently validated evidence and recommendations: `findings-validation.md`, both alongside this task.
- First remediation round; no earlier Task 001 findings or closure claims.

## Outcome and ownership

Complete the pure authoring, permission and result contracts so Tasks 002–004 can consume the approved handoff without inventing security fields or accepting invalid check definitions. Keep one shared built-in vocabulary and existing permission/schema/validation owners. The six diagnosed issues and selected corrections below are implementation instructions within approved behavior; no further product decision is needed.

Own `wyrd-spec` block/schema/result/auth contract owners, existing `wyrd-runtime` permission owners and their pure tests, minimum Vala contract fixtures, and required rustdoc on the cumulative Task 001 write set including Skald media. Source-driven generated outputs change only if the existing generator requires it. Coordinate with concurrent work; do not revert others' edits.

## Diagnosis and selected correction boundaries

The following is the complete independently validated ledger. Each section specifies the invalid-state producer, affected consumers, observable consequence, smallest selected correction and focused closure proof; implementation must not rediscover or relocate those decisions.


### FIND-TASK-001-5 — Important — Bound schema-path recursion across branch/reference hops

- **Status / classification:** CONFIRMED / INCORRECT.
- **Discovery:** INV-005, SEC-002.
- **Obligation:** REQ-003, Task 001 Scenario 2's pure known/unknown shape validation; bounded foundational processing. Unknown shapes must remain usable.
- **Exact source:** `crates/wyrd-spec/src/card/blocks/schema_path.rs:19`, especially the recursive call at :28 and reset hop bound at :54; consumers `validate.rs:663` and :709; producer admitted at :743.
- **Evidence:** for a JSON slot schema `{"oneOf":[{"$ref":"#"}]}`, an assertion over `inputs.data.x` starts `admits(root, root, [x])`. The branch enters `admits(root, ref, [x])`; `resolve` returns root; the same branch repeats with unchanged selectors. `MAX_REF_HOPS` only counts within each new resolve invocation. The `anyOf` example in INV-005 follows the same loop. This is a small finite authored JSON value, not a cyclic in-memory Rust Value or a huge-input assumption.
- **Consequence:** the required public `VerifierSpec::validate()` can exhaust its calling thread's stack and abort the process. No currently mounted remote endpoint is claimed. Future activation would carry this defect into registration.
- **Source-to-consumer trace:** authored JSON slot → `validate_slot` accepts object → requirement dispatch → `validate_reference` → `admits`. Resolved Agent action schema and extraction schema feed the same walker through `output_schema`; fix the walker rather than adding per-producer guards.
- **Minimum correction:** retain this narrow conservative analysis and give the complete recursive traversal a finite work/depth budget covering branch recursion and local-ref resolution. Budget exhaustion or unresolved cyclic analysis yields the existing unknown/admitted result, not a fabricated known-missing refusal. Preserve finite property/index/wildcard traversal and known-missing refusals. A bounded local walk needs no new graph store, framework, dependency, configuration setting, schema fetch or process isolation. Deleting the walker would lose required known-missing detection; its existing per-resolve counter alone is insufficient.
- **Focused closure proof:** pure public-validation tests for self-reference through `oneOf` and mutually referenced `anyOf`, completing normally; retain ordinary local-ref, finite recursive-property and known-missing selector cases. Include the resolved-Agent action output consumer in focused coverage of the shared walk. Do not deliberately crash a process as proof.

### FIND-TASK-001-1 — Important — Complete the approved tool permission contract

- **Status / classification:** CONFIRMED / MISSING.
- **Discovery:** BEH-001, INV-001, SEC-001.
- **Obligation:** REQ-008 revision 6, INV-003, Task 001 explicit auth scope and Scenario 5.
- **Exact source:** `crates/wyrd-spec/src/auth/permission_scope.rs:44` and :171; `crates/shared/wyrd-runtime/src/permission.rs:73`, :333 and :954.
- **Evidence:** no Tool resource or Tool scope exists. Both approved wire permissions fail to decode; the exact form additionally cannot decode `{"tool":"bifrost.query"}`. These owners are unchanged in the cumulative diff. This omission is explicitly Task 001 work rather than deferred hosted dispatch.
- **Consequence:** consumers cannot express, attenuate or preserve the required independent tool invocation grant through Wyrd permissions without inventing an alternate contract. This does not assert an existing authorization bypass.
- **Source-to-consumer trace:** role permissions / request / forwarded JSON → strict PermissionWire and `Permission::validate` → PermissionSet insertion, covers and intersection → effective principal → AcceptedAuthority serialization. Issuance `permission_set_from_rows` and RbacCheck already consume these owners generically. New labels/schema variants must also project through the existing source-driven contract mechanisms; no new store or checker belongs here.
- **Minimum correction:** add the approved Tool resource and exact built-in-name scope to these existing owners. Use one closed typed vocabulary for all ten approved names and share it with finding 2's declaration validation. Extend normal scope coverage with All covering exact and exact matching only itself; the existing narrower/intersection algorithm then supplies global/exact attenuation and disjoint names. Extend existing resource labels and scope validation: an exact Tool scope belongs to Tool/Invoke, never another resource/action. Preserve existing covering wildcard grants, existing All behavior, independent domain permissions, strict decoding and gateway-specific intersection rules. Reuse `PermissionSet`, Principal and AcceptedAuthority unchanged wherever their generic behavior already suffices; do not add special role/delegation state, default grants, audits, or runtime tool execution in this task.
- **Focused closure proof:** both approved wire forms round-trip; all ten names decode and unknown/prefix/path names fail; wrong exact-scope resource/action combinations fail; exact match/mismatch, distinct-name disjoint intersection, and global/exact intersection in both orders behave correctly. Prove existing covering wildcard behavior and no implied Tool permission from Cards/Bifrost/Verifier grants. Exercise role permission JSON and AcceptedAuthority round-trips, retaining exact scope, subject and delegation without widening. Use the existing pure tests; no Postgres fixture is necessary to prove generic role JSON decoding.

### FIND-TASK-001-3 — Important — Validate the check Agent's declared Verdict shape

- **Status / classification:** REVISED / INCORRECT.
- **Discovery:** BEH-003, INV-003.
- **Obligation:** REQ-005 registration refusal of incompatible derived response schema; Task 001 Scenario 3.
- **Exact source:** `crates/wyrd-spec/src/card/blocks/validate.rs:543`; typed contract `verification/execution.rs:323`, :336 and :347; weak positive fixture `validate.rs:937`.
- **Evidence:** the branch tests only top-level required-name membership. An object requiring outcome/reason/citations with integer/boolean/string types respectively passes shared Prompt syntax checks and this branch. Every conforming response violates the Rust Verdict shape. An outcome const of `yes` and citations items of the wrong shape likewise escape. `Verdict` requires a three-value VerificationVerdict, String, and Vec<Citation>; its objects and citation/locator shapes are closed.
- **Consequence:** the public validation API accepts a check whose declared output cannot produce a valid judgment. Later malformed-provider-output → inconclusive handling cannot replace the explicitly required early refusal. A current live registration change is not asserted.
- **Source-to-consumer trace:** portable/native Prompt → existing compiler derives ResponseType/schema and validates schema syntax → resolved inline or referenced Agent → `validate_agent` in step-check or whole-Verifier-check position → future typed CheckExecutor Verdict. The position boundary is the proper guard because ordinary action Agents may return arbitrary data.
- **Minimum correction:** replace the three-name heuristic with bounded, Verdict-specific structural compatibility validation in this existing position guard. Reuse the typed Verdict/Citation/Locator schema source for field shapes and allowed outcomes; retain shared Prompt compilation. Check the object/required/closed-field shape, outcome's valid string values, reason's string shape, and citations array/item/locator structure; refuse incompatible extra output/required fields. Compatible restrictions such as a subset of allowed outcomes or narrower reason constraints need not be discarded. Follow ordinary local schema references with bounded traversal rather than comparing raw JSON. Keep action-position output unrestricted and keep runtime malformed-output handling.
- **Recommendation boundary:** this finding does **not** require proving arbitrary JSON Schema set inclusion, generating witness values, accepting only byte-for-byte generated schema JSON, adding an opaque schema-normalization/proof platform, fetching schemas, or altering authored Prompt schemas. Annotations, property order, definition naming/local refs, and compatible additional constraints are not by themselves incompatibility. Correct the concrete structural contract at the existing guard; do not introduce a new public schema profile or change accepted authoring semantics. If an implementation proposal needs such a new profile, that proposal requires spec revision rather than being authorized by this finding.
- **Focused closure proof:** a genuinely compatible typed-contract schema passes, including an equivalent local-ref/annotation variation; wrong outcome/reason/citations types, invalid outcome enum/const, malformed citation item/locator, and incompatible extra fields are refused. Cover both check positions and inline/resolver-supplied Agents through the shared guard; retain a positive arbitrary structured-output action. Replace the loose fixture with a true Verdict fixture. Pure registration tests suffice; no provider call, generic theorem suite or journey is required.

### FIND-TASK-001-2 — Important — Admit every approved built-in name

- **Status / classification:** CONFIRMED / INCORRECT.
- **Discovery:** BEH-002, INV-002; SEC-001 corroborating catalog observation.
- **Obligation:** REQ-008's ten-tool catalog; Task 001 Scenario 3.
- **Exact source:** `crates/wyrd-spec/src/card/blocks/validate.rs:30` and :509.
- **Evidence:** the list permits only fs.list/fs.read/fs.search/code.run/cards.get/bifrost.query. A resolved step Agent with a matching no-placeholder Prompt, no workspace, and any of cards.list/cards.get_by_ref/bifrost.list_tables/bifrost.describe_table is rejected as `agent_tools` before any runtime dispatch.
- **Consequence:** approved discovery/read preparation cannot be authored through the task's validation contract.
- **Source-to-consumer trace:** AgentSpec.tool_names → inline/resolver Agent → step `validate_agent` membership. Both check positions share the sibling tool-less rule, which is correct. Filesystem workspace and native-media checks remain independent correct rules.
- **Minimum correction:** replace the private incomplete list with membership in finding 1's complete shared typed built-in vocabulary. Do not introduce a second catalog, runtime registrations, SDK callables, or permissive unknown-name fallback. The auth omission and declaration subset are distinct obligations but should be corrected together to establish one vocabulary.
- **Focused closure proof:** all ten tools validate in step position with required valid workspace for filesystem tools; explicitly include the four omitted names with no unrelated workspace. Unknown names and all check-position tool declarations remain refused. Retain filesystem-required/workspace-requires-filesystem-or-code restrictions.

### FIND-TASK-001-4 — Important — Carry output digest and returned bytes in tool provenance

- **Status / classification:** REVISED / MISSING.
- **Discovery:** BEH-004, INV-004.
- **Obligation:** REQ-008's recorded names/arguments/output digests/returned byte counts, REQ-015 attribution, Task 001 complete handoff acceptance.
- **Exact source:** `crates/wyrd-spec/src/verification/execution.rs:310`; consumers :292 and `crates/vala/vala-eval/src/blocks.rs:85`.
- **Evidence:** ToolCallRecord has only tool, arguments and optional error. Neither enclosing ActionOutput nor StepResult/CallProvenance has output digest/returned-byte metadata. The existing result fixture therefore cannot retain it.
- **Consequence:** hosted/runtime/client tasks would need to invent missing durable provenance fields after this purported contract handoff.
- **Source-to-consumer trace:** future ActionExecutor → `ActionOutput.tool_calls: Vec<ToolCallRecord>` → future engine projection → `StepResult.tool_calls` → result serialization. The shared type is already the correct boundary; there is no runtime writer to add here.
- **Minimum correction:** complete ToolCallRecord with the approved SHA-256 output digest using existing `Sha256Hex` and a returned-byte count. These describe the bounded output actually returned, not unrestricted raw output or a newly invented canonical evidence format. Preserve error-only calls: absence of returned output must be representable honestly, without fabricating a digest; document the optional/no-output semantics alongside the fields. Use the same record in both existing consumers. Do not add a second record, evidence wrapper, log sink, full tool output persistence, or an engine implementation.
- **Focused closure proof:** pure serde/schema coverage for successful-output provenance and error/no-output provenance; a narrow handoff test carries a tool record on ActionOutput into the existing StepResult type and confirms result round-trip retains digest/count. This is a value-copy contract proof, not a request to invent the deferred production projector. Update existing fixtures and affected constructors.

### FIND-TASK-001-6 — Important — Complete mandatory rustdoc in the cumulative write set

- **Status / classification:** CONFIRMED / VIOLATION.
- **Discovery:** RS-001, MNT-001.
- **Obligation:** `AGENTS.md` §16 and agent rules explicitly mark missing substantive rustdoc on changed items and missing `# Errors` on fallible items as blocking. This is the stated exception to non-blocking style preferences.
- **Exact source and evidence:**
  - `card/blocks/validate.rs:145,208,244,292,355,401,430,468,566,639,743,783`: named fallible helpers have summaries without required error sections.
  - `card/blocks/mod.rs:335,345`: custom DriftCheck codecs have no item docs/error sections, despite flattening, required `on` extraction and delegated parse failures.
  - `card/blocks/reference.rs:179,231,267,269,275,281,288,292`: cursor field, trait methods and associated Err lack docs.
  - `crates/skald/skald-spec/src/media.rs:183,300,306,334,340,348,352`: tuple field, trait methods and associated Err lack docs. `from_str` at :313 already documents its error and is not part of this omission.
  - `crates/wyrd-spec/src/vala/eval/spec.rs:182`: materially changed operator delegate lacks rustdoc/error section.
  - New test modules at `card/blocks/extract.rs:187`, `reference.rs:314`, `schema_path.rs:74`, `validate.rs:867`, `ids.rs:532`, `refs/mod.rs:541`, `verification/execution.rs:387`, Skald `media.rs:370`, and Vala `blocks.rs:159` lack module rustdoc. Unless otherwise prefixed, these paths are under `crates/wyrd-spec/src`.
- **Consequence:** explicit repository completion/merge documentation requirements remain unsatisfied; codec/refusal invariants needed for safe maintenance are undocumented. Green Clippy does not establish compliance.
- **Source-to-consumer trace:** these changed helpers and codecs are directly used by validation, reference/media wire conversion and schema production; test modules are directly included by the library targets. Their docs belong at their current owners, not in a new report or wrapper. Complete bodies were inspected to verify omissions and actual propagated errors.
- **Minimum correction:** add substantive intent/workflow/invariant docs and accurate error sections to affected new/materially changed items, including private fields/trait items/test modules. Cover the cumulative changed set in this same correction; do not document untouched legacy neighbors, rewrite APIs, create a new permanent checker, or run a separate style-only remediation round.
- **Focused closure proof:** static review of changed-item rustdoc against §16, accurate error/cancellation sections where applicable, and formatting. Existing narrow owner lint checks apply to the combined remediation write set; no new behavioral test is needed for documentation alone.


## Constraints and preserved behavior

- Use Wyrd's general global or exact scope permission rule, preserving existing covering wildcard and scoped intersection semantics. Invocation grants never grant underlying Cards/Bifrost data access; those checks remain independent later runtime work.
- Preserve typed Card references, existing loader/visitor and DAG owners, Prompt-derived variables/schema/media, action-versus-check positions, original-source provenance, and offline scenario evaluation.
- Keep the approved BlockError seam; do not remove JudgeError from unrelated offline evaluation.
- Preserve conservative unknown-schema behavior and known-missing refusals. Do not impose a new public schema profile or exact generated-schema-text requirement.
- Do not implement runtime tools, a registry service, execution engine, provider calls, sandbox, server routes, queue/persistence, client transports, final protocol cutover, or full journey suites here.
- Keep staged contracts inactive; Task 005 still owns old-type removal and activation. Do not add compatibility routes, aliases, feature switches, grant defaults, new dependencies or permanent check scripts.
- Keep error-only tool records honest; do not fabricate returned output/digests or persist complete raw tool output.
- Complete required documentation in this same pass. Do not broaden into unrelated style or legacy documentation rewrites.

## Ordered proof scenarios

For each changed behavior: add a focused failing test, confirm the expected missing behavior, make the smallest correction in the named owner, rerun the relevant focused tests, then refactor only if it removes real duplication. Never cause a process stack overflow to obtain RED: the source recursion trace already establishes the defect; prove bounded normal completion after correction.

1. **Bound reference traversal** (FIND-TASK-001-5): prove combinator/local-ref cycles complete through public validation; preserve finite recursive selectors, both schema consumers and known-missing refusals.
2. **Complete one tool vocabulary and permission contract** (FIND-TASK-001-1/2): prove all ten declared tools, both grant forms, strict rejection, exact/global coverage/intersection, no implicit grants, and role/accepted-authority preservation. Preserve check-position and filesystem workspace refusals.
3. **Enforce derived Verdict compatibility** (FIND-TASK-001-3): prove canonical and equivalent local-ref schemas accepted, incompatible types/outcomes/citations/locators/extra fields refused, and arbitrary action outputs retained. Cover both check positions and resolved/inline Agents at the shared owner.
4. **Complete tool provenance** (FIND-TASK-001-4): prove digest/count/error semantics and pure handoff/result serialization using the existing record; no runtime projector.
5. **Complete rustdoc and handoff evidence** (FIND-TASK-001-6): statically inspect every affected new/materially changed item, including private/test/trait items and actual Errors/cancellation documentation. Documentation alone needs no behavior test.

## Acceptance and focused verification

| Finding | Acceptance |
| --- | --- |
| FIND-TASK-001-1 | Both approved grant forms and all named scopes decode, validate, cover, intersect and survive role/accepted-authority encoding without widening; existing wildcard/domain behavior retained. |
| FIND-TASK-001-2 | All ten approved tool names validate in action position through the shared catalog; unknown/check-position tools and invalid workspace bindings remain refused. |
| FIND-TASK-001-3 | Both check positions reject demonstrably incompatible declared Verdict structures while compatible equivalent schemas and arbitrary action schemas remain usable. |
| FIND-TASK-001-4 | Existing shared record carries returned-output digest and byte count honestly, including failed/no-output calls, through handoff and result wire round-trip. |
| FIND-TASK-001-5 | Cyclic local-ref/combinator schemas terminate normally through both input and output validation; known-missing and finite-recursive paths retain their semantics. |
| FIND-TASK-001-6 | All affected cumulative changed items satisfy mandatory rustdoc and error documentation; no unrelated code/style rewrite. |

Choose actual focused test names in the owning libraries and record/run every named test with its exact `mise exec -- cargo nextest run --locked -p <owner> --lib -E 'test(=<actual qualified test>)'` command. Do not claim placeholder selectors were run. Tests are pure and require no Postgres or credentials. Preserve RED/GREEN evidence or the documented source-diagnosed recursion exception.

After focused proof, run only these bounded owner/contract checks, adjusting the owner list only when the actual remediation write set requires it:

```bash
mise exec -- cargo nextest run --locked -p wyrd-spec -p wyrd-runtime -p wyrd-skald-spec -p vala-eval --lib
mise exec -- cargo clippy --locked -p wyrd-spec -p wyrd-runtime -p wyrd-skald-spec -p vala-eval --all-features --lib --tests -- -D warnings
mise run fmt
mise run check:deps
mise run codegen:check
git diff --check
```

Record the stable exports and any affected contract fixtures/constructors. Re-review the complete original base-to-remediated candidate against the original task, this remediation and the prior verdict; preserve finding IDs. Do not run full user journeys, server integration, or the repository aggregate as remediation proof. They remain the final merged change's obligation.

## Implementation Evidence

Status: `IMPLEMENTED`. Candidate `896e563be..eb1681461` (commits 67a95f564, 86e5e47a7, d7be7fa02, 0375aed2d, eb1681461).

### Reuse map

| Need | Existing owner reused | Gap closed |
|---|---|---|
| Tool vocabulary | `card::agent` (beside `AgentSpec.tool_names`) | New closed `BuiltinTool` (10 names, `ALL`, `as_str`, `from_name`, `is_filesystem`); shared by permission scope, step-Agent validation, and `ToolCallRecord.tool`. |
| Tool permission | `PermissionScope`, `Resource`, `Permission::validate`, `PermissionSet` covers/intersection | Added `PermissionScope::Tool(BuiltinTool)`, `Resource::Tool` (`"tool"`), `tool:invoke` exact/global constructors. No change to `PermissionSet`, `Principal`, `AcceptedAuthority`, built-in roles, or issuance. |
| Bounded schema walk | `schema_path::admits` | One `Walk` budget (1024 node visits + `$ref` hops) shared across branches and refs; exhaustion admits. |
| Verdict compatibility | Same walker + generated `schemars` schema of `Verdict` | `fits(authored, typed)`: types (integer ⊆ number), closed value sets (`enum`/`const`/combinators), `allOf`, closed objects, required fields, items, local refs both sides. Formats/patterns/numeric bounds left to output decoding. |
| Tool provenance | Existing `ToolCallRecord` in `ActionOutput` and `StepResult` | Added `output_sha256: Option<Sha256Hex>` and `returned_bytes: u64` (no output ⇒ no digest, 0 bytes). |

### Acceptance

| Finding | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-TASK-001-1 | `wyrd-spec/src/auth/permission_scope.rs`, `wyrd-runtime/src/permission.rs`, server `gateway/batches.rs` exhaustive match | `permission::tests::tool_invoke_is_exact_name_scoped_and_independent`, `request_context::tests::accepted_authority_keeps_exact_tool_scope` | PASS |
| FIND-TASK-001-2 | `card/agent.rs::BuiltinTool`; `validate.rs::validate_agent` | `verifier_spec_admits_every_builtin_step_tool`, `verifier_spec_enforces_agent_positions_and_media` | PASS |
| FIND-TASK-001-3 | `schema_path.rs::fits`; `validate.rs::verdict_schema` + check-position guard; true Verdict fixture | `verifier_spec_requires_check_agents_to_produce_a_verdict` (whole-Verifier + step check, inline + resolver, generated schema, ref/annotation/subset variant, 15 incompatible shapes) | PASS |
| FIND-TASK-001-4 | `verification/execution.rs::ToolCallRecord` | `verification_result_round_trips_documented_outcome`, `blocks::tests::action_tool_calls_carry_into_step_results` | PASS |
| FIND-TASK-001-5 | `schema_path.rs::Walk` | `verifier_spec_bounds_cyclic_schema_references` (self `oneOf`, mutual `anyOf`, via input slot and resolved Agent output; finite recursive tree keeps `missing_field`), `schema_path_refuses_only_known_missing_selectors` | PASS |
| FIND-TASK-001-6 | `# Errors` on 12 validate helpers; DriftCheck codecs; Reference cursor/trait items; Skald `MediaFormat` field/trait items; `validate_task_operator`; module docs on 9 test modules | Static review; clippy `-D warnings` | PASS |

RED: the permission wire forms and the 4 omitted tools failed at the reviewed candidate (review evidence; the Tool variant did not exist). For the cyclic recursion, the source trace from the review stands in for RED, so no process was crashed. The integer-outcome Verdict case first passed `fits` because of schemars' `allOf` ref wrapper; that was fixed in the same commit, before it was committed.

### Commands

```bash
mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(=permission::tests::tool_invoke_is_exact_name_scoped_and_independent)'
mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(=request_context::tests::accepted_authority_keeps_exact_tool_scope)'
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::blocks::validate::tests::verifier_spec_admits_every_builtin_step_tool)'
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::blocks::validate::tests::verifier_spec_requires_check_agents_to_produce_a_verdict)'
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::blocks::validate::tests::verifier_spec_bounds_cyclic_schema_references)'
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::blocks::validate::tests::verifier_spec_enforces_agent_positions_and_media)'
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::blocks::schema_path::tests::schema_path_refuses_only_known_missing_selectors)'
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=verification::execution::tests::verification_result_round_trips_documented_outcome)'
mise exec -- cargo nextest run --locked -p vala-eval --lib -E 'test(=blocks::tests::action_tool_calls_carry_into_step_results)'
mise exec -- cargo nextest run --locked -p wyrd-spec -p wyrd-runtime -p wyrd-skald-spec -p vala-eval --lib   # 1139 passed
mise exec -- cargo clippy --locked -p wyrd-spec -p wyrd-runtime -p wyrd-skald-spec -p vala-eval --all-features --lib --tests -- -D warnings   # clean
mise exec -- cargo check --locked --workspace --all-features --tests   # clean (downstream of Resource/PermissionScope/ToolCallRecord)
mise run fmt; mise run check:deps; mise run codegen:check; git diff --check   # all clean; codegen produced no diff
```

### Stable exports and switch points

- New: `wyrd_spec::card::agent::BuiltinTool`, `PermissionScope::Tool`, `wyrd_runtime::permission::Resource::Tool`, `Permission::tool_invoke(tool)` / `tool_invoke_any()`.
- Changed: `ToolCallRecord.tool` is now `BuiltinTool` (was `String`) and carries `output_sha256`/`returned_bytes`. Runtime tasks fill these from the bounded returned output.
- No default grant: built-in roles do not carry `tool:invoke`. Only explicit grants or the existing covering wildcard reach it.

### Non-goals and limits

- No runtime tools, registry, engine projector, routes, persistence, SDK, or journeys. Staged contracts stay inactive. JudgeError was not touched.
- Verdict fit does not check string patterns, formats, or numeric bounds. A schema with a looser `slot` or a negative `page` still decodes-or-fails at runtime as inconclusive.
- Only the remediation write set changed. The user's pre-existing docs edits stay unstaged.
