# Independent findings validation — TASK-001

**Recommendation: FIX_REQUIRED.** Six bounded findings remain. No validated correction requires a specification revision. There are no Critical findings: this candidate stages the new contracts, without activating a new server execution or registration path. The schema recursion defect nevertheless exists in the public pure validation API required by this task.

## Immutable subject and authority

- Base: `08b38f74255d344e1c5ec9a6d278935a26277622`.
- Candidate: `896e563bee69ab6b5f47b31977d7de1a0ca88d57`.
- Approved authority: `spec-snapshot.md`, revision 6; original `task-snapshot.md`, including Scenarios 3–5 and the explicit staged-contract handoff. The approved BlockError/JudgeError decision is accepted human-directed scope, not a finding.
- Reviewed `subject.md`, complete change inventory and cumulative change/source, both implementation reviews, standards, maintainer, system and security reviews, and `claims-comparison.md`. CodeGraph preceded source navigation; current production source matched the candidate (`HEAD` and candidate-relative production diff checked). Source, task and specification were not edited.
- Governing rules include `AGENTS.md` (especially §§2–6, 11, 16), agent rules, Wyrd design/doctrine, spec-driven development, maintainer guidance, and the invoked task-review skill. The approved replacement/staging instruction controls Task 001; older live Verifier contracts are not reimposed on its replacement. Ordinary implementation details remain implementer-owned.

Agreement among discovery reports was not used as proof. Each proposal was checked at its producer, full correction owner, immediate consumers and sibling paths. No prior finding IDs exist; the IDs below are the initial stable ledger.

## Per-proposal dispositions

| Discovery proposals | Disposition | Source validation and correction boundary | Ledger |
| --- | --- | --- | --- |
| BEH-001, INV-001, SEC-001: missing tool permission representation/coverage | CONFIRMED | `PermissionScope` has no Tool variant and `Resource` has no Tool variant. Strict `PermissionWire` decoding cannot admit either approved example. `Permission::validate`, scope coverage, and `PermissionSet::intersection` are the existing owners; extend them, not runtime dispatch or another permission service. | FIND-TASK-001-1 |
| BEH-002, INV-002: four approved built-ins rejected | CONFIRMED | `STEP_TOOLS` contains six names. All four reported omitted names fail the same `validate_agent` membership branch. The typed catalog required by finding 1 can supply this membership instead of a second list. | FIND-TASK-001-2 |
| BEH-003, INV-003: incompatible check-Agent response schemas accepted | REVISED | The concrete wrong-type/invalid-outcome/citation examples pass the required-name-only branch and shared Prompt syntax validation. Retain the defect; narrow correction to Verdict-specific structural compatibility, not a general schema-inclusion theorem, exact schema JSON comparison, hidden Prompt replacement, or new schema dialect restriction. | FIND-TASK-001-3 |
| BEH-004, INV-004: missing tool output digest/byte count | REVISED | The only staged tool record lacks both fields, and both handoff/result consumers use that type. Retain the missing contract. Closure proves pure value passage and serialization; there is no production ActionOutput-to-result projection yet to implement in this task. | FIND-TASK-001-4 |
| INV-005, SEC-002: cyclic combinator/reference traversal | CONFIRMED | `admits` re-enters on the same schema/path through `oneOf`/`anyOf` and `$ref:#`; `resolve` resets its hop count each invocation. One shared bounded walk fixes both slot and step-output consumers. No crash reproduction or live endpoint exposure is asserted. | FIND-TASK-001-5 |
| RS-001, MNT-001: required rustdoc omissions | CONFIRMED | Concrete undocumented fields/trait methods/test modules and missing `# Errors` remain in changed items. This blocks under the explicit §16 exception despite no asserted runtime defect. One documentation correction, not a style-only redesign or new checker. | FIND-TASK-001-6 |
| Behavior report's missing-codegen verification objection and stale “not run” prose in other reports | REJECTED as a remaining gap | Fresh `codegen-check.log` and subject record show successful `mise run codegen:check`, with tracked generated outputs unchanged. The original omission is historical evidence, not an open finding. | None |
| Standards' import/fully-qualified-name notes and general maintainer preferences | REJECTED as blockers | No independent behavioral/public-contract consequence or required deletion is established. No refactor is required. | None |

Security's catalog observation duplicates finding 2; its missing permission contract duplicates finding 1. System review supplies no proposed finding. Its unchanged-deployment conclusion is consistent with the pure-contract defects above. The comparison's no-follow-up decision is sound: there is no unresolved contradictory source claim requiring another reviewer.

## Source and consumer trace coverage

| Authority / producer | Full correction owner and sibling consumers checked | Existing alternative and result |
| --- | --- | --- |
| Role/request/forwarded permission JSON | `auth/permission_scope.rs` complete scope enum, validation and coverage; `wyrd-runtime/permission.rs` Resource, its labels/coverage, Action, PermissionWire conversion, Permission validation/coverage/subsumption, PermissionSet insertion and intersection | Existing three-axis RBAC already performs the workflow. `wyrd-auth/src/issuance.rs:797` decodes role rows as `Vec<Permission>`; `Principal.effective_permissions`, `request_context.rs:78` AcceptedAuthority and `permission_check.rs:64` RbacCheck consume the same values. No separate producer supports the missing form. Preserve general wildcard and gateway-specific intersection behavior. |
| `AgentSpec.tool_names` and derived Prompt | `VerifierSpec::{validate,validate_with}`, Validator run, step and requirement dispatch, complete `validate_agent`/`validate_placeholders`; inline and resolver-provided Agent paths, both check positions | Shared Prompt compiler owns placeholder/media/schema derivation. `Prompt::new` and `check_output_schema` validate schema syntax, not Verdict compatibility. The position-specific block validator owns the additional restriction; tools/workspace/action rules are adjacent behavior to preserve. |
| Authored JSON schema / resolved Agent action output schema / generated extraction schema | Complete `schema_path::{admits,resolve,excludes_type}`, `validate_slot`, `validate_reference`, `whole_slot`, `output_schema`; both calls at validate.rs:663 and :709 | Existing Prompt schema syntax traversal never follows `$ref` and cannot solve the cycle. No runtime JSONPath evaluator or Card loader owns this pure known-missing analysis. Extend the existing narrow walker. |
| Future injected ActionExecutor result | Complete `ActionOutput`, `ToolCallRecord`, `CallProvenance`, `StepResult`, VerificationOutcome/Result, executor fixture and result round-trip fixture | Only two staged tool-record consumers exist: `ActionOutput.tool_calls` and `StepResult.tool_calls`. No alternative digest/byte fields or current runtime writer exist. Reuse `Sha256Hex`; no evidence subsystem or log sink is needed. |
| Changed custom codecs, validators and tests | Complete custom DriftCheck codecs, Reference cursor/Display/FromStr/Serde/schema bodies, MediaFormat codecs/schema, changed operator delegate and reported validation helpers | Required rustdoc can be added in place. Existing documented parse/public validation methods are appropriate nearby examples; no implementation rewrite or permanent lint is needed. |

The new `blocks::VerifierSpec` is not the envelope's live `Spec::Verifier`; its Visit implementation is staged. The old server verification service still consumes the old execution contract. New executor traits have only the Echo test implementation. Existing DAG/operator callers retain their offline owners. Therefore missing future sandbox isolation, provider execution, server permission/audit enforcement, durable result publication and SDK journeys are not added to this task's finding ledger.

## Final deduplicated ledger

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

## Recommendation boundaries and verification

The six IDs can be remediated in one bounded task. Findings 1 and 2 share a catalog correction; all rustdoc corrections belong in that same pass. Every retained behavior is already approved by r6 or explicit repository authority. None authorizes a new public API/profile, persistent store, runtime, security service, compatibility route or resource-ownership change. No SPEC_REVISION_REQUIRED conflict is established by the validated minimal corrections.

Fresh available proof was reconciled from the subject and logs:

- `library-check.log`: 1069 library tests passed, zero skipped, across wyrd-spec, wyrd-skald-spec and vala-eval.
- `focused-check.log`: three exact selections passed (action executor handoff, Agent positions/media, block reference inventory), 1066 skipped in that selection.
- `codegen-check.log`: `mise run codegen:check` passed; tracked generated outputs remained unchanged. Older “codegen not run” statements in task/behavior/maintainer/system prose are superseded, without modifying those immutable reports.
- Cumulative `git diff --check` passed per fresh subject proof. Implementer-reported format, Clippy, workspace compilation and dependency checks remain reported evidence, not additional reruns by this validator.

These checks establish the existing test behavior, not the omitted negative cases or missing fields. No test was weakened and no runtime exploit/crash was induced. Remediation should use exact focused tests for the new cases, the narrow affected owner library/compile/lint lanes, formatting, and source-driven codegen proof where the permission/provenance schemas change. Select actual test names when authored; do not claim uncreated selectors or require full server/journey aggregates here.

Skipped fresh suite execution, live runtime security/fault tests and integrated SDK journeys; those assembled proofs remain later-task/change-review obligations. Risk: required contracts and pure validation remain incomplete until findings 1–6 close, despite the current green library/codegen evidence.
