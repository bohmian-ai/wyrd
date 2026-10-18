# Independent maintainer review

Result: **FAIL**

Subject: `ef3a57b0776dd7bc451273b9176498fcca700550...5c421663b43d7deeca135134d6b07cf49a11b83f` in `/home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-002`. HEAD remained the candidate during inspection. This is an independent source review; no other review reports or intended verdict were consumed.

Authority: AGENTS.md, architecture/agent-rules.md, the complete maintainer-style and spec-driven-development references, applicable verification/binding/Trigger sections of wyrd-design.md, wyrd-doctrine.mdx, TASK-002, and REQ-012/AC-007/INV-001–005 under the explicitly approved revision-13 scope. Revision 14's draft upgrade note does not authorize additional requirements in this review.

## Changed-surface coverage

The complete cumulative diff was inspected, with larger portions inspected separately to avoid truncated tool output. CodeGraph exploration preceded code location searches. The following groups cover all 54 changed files.

| Surface | Symbols, callers, and proof inspected | Assessment |
|---|---|---|
| Dependency manifests and lock | Workspace/contract serde_path_to_error declaration and locked dependency | Reuses an installed standard Serde adapter; no new parser framework or feature introduced. |
| Contract and parsing | VerificationBinding, InputSlot and required(), removed ScheduledInput and validate_schedule, validate_slot and validate_evidence_mode, envelope decode/spec_from_kind_value/Card::deserialize/Spec::from_kind_and_value | Contract removes the duplicate selectors and preserves JSON/media/trace optionality. Shared decoding remains the single kind-dispatch path. Documentation gap MAINT-001. |
| Contract consumers and fixture tests | card/mod.rs field-refusal test; verifier_fixtures Fixtures owner and traversal removal; reference.rs/refs visitor fixtures; shared loader parse_single_envelope and validation fixture; client state fixture; Skald author/save/load fixture | No duplicate reference walker, fixture decoder, or removed selector representation added. The fixture traversal deletion is appropriate for the removed map. |
| Registration and projection | EffectiveSpecs::resolve/new/validate_bindings/validate_binding and check_activation, BindingProjector::project/freeze, NewBinding and project_bindings; registration route assertions and lifecycle fixture consumers; auth issuance fixture | Existing retained-state owners remain discoverable; no second registry workflow. Error-catalog consumer closure is incomplete, MAINT-002. |
| SQL and migration | Binding INSERT and arguments, migration dropping inputs, CLAIM_SQL, ExecutionInput, ClaimedRow::into_claimed; binding/execution integration tests, scheduler occurrence tests and submitted-execution assertions | Selector removal is consistent through stored bindings and claim shape; subject, exact Verifier, window and submitted inputs retain their existing distinct types. No new lifecycle owner or queue. |
| Runtime and test harness | VerifierRunner::dispatch scheduled/submitted branches, DriftEngine::checks/score, ObservationWindow, VerificationFixture::bind_schedule; route/runtime/operator tests; role-separated testing runtime changes and deleted optional-slot omission test | Existing DriftEngine owns retained runtime dependencies. Every drift requirement now reaches existing scoring; only the obsolete selector-presence filter disappeared. Deleted test demanded behavior the approved change removes. |
| Rust journey | Watch::start_with/deploy/submit/record/judge, both new scenarios, existing failed/passing/empty/SPC/operator scenarios | Extends the existing real server/SDK story and helper owner; no new transport or harness. Per-requirement assertions are explicit and named by outcome. |
| Python journey | watching, latency_budget fixture, both parameterized scenarios and existing submit/result/operator helpers | Uses existing fixture lifecycle and public SDK. Typed parameterization covers both deterministic windows. No public Python API or native stub shape was authored here. |
| TypeScript journey and generated contract | startWatch/submit/record/judge, both parameterized scenarios, generated InputSlotSamples/VerificationBinding declarations and deleted ScheduledInput types | Generated declarations agree with Rust removal. Runtime tests use existing server fixtures and SDK methods. |
| CLI/MCP and authored assets | CLI end-to-end assertions and YAML, MCP scheduled Service fixture, all changed/new Cards and invalid fixtures, fixtures README | Existing consumers remove selectors without adding compatibility forms. |
| Durable documentation and task evidence | Changed design examples; binding, drift and verification guides; task's verification claims | User guides describe declaration-owned samples; error remediation still contradicts them (MAINT-002). Verification claims were read as claims, not independently rerun results. |

## Existing-owner comparison

The new private `decode` is a thin Serde error-path adapter inside the existing envelope parser. Loader parsing already calls `Spec::from_kind_and_value`; registration and composition use that same entry point; Card deserialization uses the same private dispatch. Extending this owner is sufficient and avoids a separate removed-field scanner. `validate_evidence_mode` already rejects samples mixed with other slot types, so deleting the old mapping validator does not require another schedule validator. Registration keeps EffectiveSpecs and BindingProjector; runtime keeps DriftEngine and VerifierRunner; persistent state keeps the existing binding/execution owners. No competing graph, cache, parser workflow, transport, or lifecycle owner was found in the change.

## Material findings

### MAINT-001 — Shared decoding changes omit mandatory rustdoc

- Classification: **VIOLATION**, blocking under the explicit rustdoc exception in the review skill.
- Changed location: `crates/wyrd-spec/src/envelope.rs:752–759`, `decode` and materially rewritten `spec_from_kind_value`.
- Rule: AGENTS.md §16 and maintainer-style “Documentation”: every new or materially modified Rust item needs substantive rustdoc; every fallible function needs an Errors section. Missing rustdoc remains a hard blocker.
- Evidence: `decode` has a purpose paragraph but no `# Errors`. `spec_from_kind_value` has no rustdoc at all despite replacing every native kind's decoder call. These are reached by `Spec::from_kind_and_value` and `Card::deserialize`; loader `parse_single_envelope`, registration EffectiveSpecs, graph composition, and reference inline decoding depend on them.
- Concrete maintenance cost: these are the shared error-boundary functions for every Card kind. Their undocumented contract hides which errors retain authored paths, that kind dispatch can refuse External, and the existing top-level `type` removal before decoding. A maintainer changing path handling must recover this behavior from implementation and callers rather than its declared contract.
- Smallest correction: document the two existing functions in place, including their failure conditions and kind-dispatch normalization. Do not add another parser or change behavior. Nearby `EffectiveSpecs::new` and `validate_bindings` show appropriate concrete decode-error documentation.
- Testable closure: inspect both rustdoc blocks and run the narrow contract format/check lane; existing removed-field refusal test remains the behavioral proof. No new runtime test is required for documentation alone.

### MAINT-002 — Activation-error remediation still tells callers to author the rejected field

- Classification: **REGRESSION / incomplete consumer closure**.
- Changed location: `crates/wyrd/wyrd-server/src/components/cards/resolve.rs:25–48` and removed binding field in `crates/wyrd-spec/src/card/verifier/mod.rs:633–652`; missed authoritative consumer `crates/wyrd-spec/src/error.rs:2076` and its generated `docs/src/content/docs/api/errors.md:351`.
- Obligation: REQ-012/AC-007 consumer closure, plus maintainer-style public-contract/documentation agreement and AGENTS.md §4's source-derived error metadata.
- Evidence: `check_activation` still returns `SpecTriggerActivationMismatch` for a scheduled run-evidence Verifier. The existing `pg_card_registration_route` activation-mismatch scenario at lines 1062–1081 proves that reachable route. Its catalog remediation remains: “Attach a schedule Trigger only to a Verifier that judges recorded samples, and map its samples slots in the binding inputs; supply other evidence through verify or submit.” The candidate removes `VerificationBinding.inputs` under deny_unknown_fields and explicitly tests rejecting even `{}` or exact selectors. The generated error guide retains the same instruction.
- Observable consequence and maintenance cost: an API/agent caller following the server's prescribed recovery gets another 400 from the newly forbidden field. Maintainers and callers see contradictory guidance from the typed contract, updated workflow guides, and public error catalog. This becomes incorrect because of this change; it is not unrelated old wording.
- Smallest correction: update only the error catalog's authoritative remediation to describe a samples Verifier whose own declarations supply measurements, then regenerate its existing documentation projections. Preserve the error code, activation refusal, and all ordinary registration behavior; do not hand-edit generated docs.
- Testable closure: assert the activation-mismatch remediation no longer instructs adding binding inputs, and run source-driven codegen checks. The existing activation-mismatch registration scenario is the relevant HTTP proof; no new harness is needed.

## Verification and limits

This role ran read-only Git, CodeGraph, and source searches, and wrote only this report. No test/build suite, Postgres instance, commit, or source/test mutation was performed; the primary agent owns coordinated verification. The task records green contract/SQL/server/Rust/Python/TypeScript/CLI/MCP and static checks, but those claims were not converted into independently executed results here. No full suite or gate was run.

No additional blocking layout, naming, struct ownership, typing, generated declaration parity, or duplicate workflow finding was established. The two bounded findings above require correction before this maintainer review passes.
