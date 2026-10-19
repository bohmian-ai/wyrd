# Independent findings validation

Result: **FIX_REQUIRED**. Two deduplicated, bounded findings remain. No material specification revision is needed to correct them.

## Subject, inputs and authority

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-002`.
Reviewed cumulative range: `ef3a57b0776dd7bc451273b9176498fcca700550...5c421663b43d7deeca135134d6b07cf49a11b83f`.
HEAD was independently checked and remains the candidate. Only this report was written by this validator; no production/test edits or commits.

Inputs were subject.md, the original TASK-002 packet, specification REQ-012/AC-007/INV-001–005, both task reviews, standards-review.md, maintainer-review.md, system-review.md, both domain reports, claim-comparison.md and verification.md. The task-review skill, AGENTS.md, agent rules, spec-driven-development, maintainer and error guidance govern this validation. CodeGraph exploration preceded code searches, followed by the actual cumulative diff, owning source, callers, generated consumers and tests. Findings below are based on source, not the number of reports supporting them.

The supplied human authority explicitly identifies approved revision 13. The checked-in specification says revision 14 draft. This validation applies the scoped revision-13 obligations supplied in subject.md; it neither approves revision 14 nor relies on its managed-table upgrade addition. Neither correction below changes an architecture, compatibility, persistence, resource, security or public behavior decision. The existing binding-column contraction is deliberately selector-removing; mixed-version rollout qualification is not inferred from draft text.

## Every discovery proposal reconciled

| Discovery IDs | Independent classification | Disposition and root cause |
|---|---|---|
| BEH-001, INV-DOC-1, RSTD-001, MAINT-001 | CONFIRMED — VIOLATION | FIND-TASK-002-1: two changed functions in the same shared decoding owner lack the expressly required rustdoc/error contract. One correction boundary. |
| INV-ERR-1, MAINT-002 | CONFIRMED — REGRESSION | FIND-TASK-002-2: authoritative error metadata still prescribes a field removed by this candidate. HTTP and generated reference text are consumers of one stale producer, not separate defects. |

No discovery finding is rejected. System, persistent-data and evaluation reports propose no additional findings; this is not treated as proof by agreement. No follow-up report exists because comparison found no conflicting claim or unresolved reachable path. My source tracing resolves both proposed causes without follow-up.

## Final deduplicated ledger

### FIND-TASK-002-1 — CONFIRMED — VIOLATION

**Obligation.** AGENTS.md §16, architecture/agent-rules.md's Rust documentation rule and the task-review skill explicitly require substantive rustdoc for new/materially changed private Rust items and `# Errors` on fallible functions. Missing rustdoc remains blocking even without a runtime consequence.

**Exact locations.** `crates/wyrd-spec/src/envelope.rs:752` (`decode`, signature at 755) and `:759` (`spec_from_kind_value`).

**Source evidence and reachability.** The new `decode<T>` returns `Result<T, String>` and calls `serde_path_to_error::deserialize(value)`, converting errors to strings. Its purpose paragraph has no Errors section. `spec_from_kind_value` has no rustdoc; its match is materially changed for all fourteen native kinds to use this decoder. Its complete body also removes top-level `type` before dispatch and explicitly refuses `CardKind::External`. The normalization already existed at base and must be documented accurately, not removed as incidental remediation.

The only direct production callers of the private dispatcher are `Spec::from_kind_and_value` (`envelope.rs:407`) and `Card::deserialize` (`:715`). The former attaches the kind to `SpecDecodeError`; the latter maps the shared string into its Serde error. The generic `decode` is called only by that dispatcher. Sibling consumers of the public conversion include shared loader `parse_single_envelope` (`wyrd-loader/src/parse.rs:216`), spec graph composition/canonicalization, inline reference decoding (`wyrd-spec/src/reference.rs:450`), SQL Card-row decoding, and server effective-spec validation, mapping and registration. They share these same bodies; no separate decoder correction is needed. Tests and fixtures call the same conversion, including the new removed-field refusal test.

**Consequence.** The changed shared parsing boundary fails an explicit acceptance gate. Its malformed-body/path behavior, unsupported-kind refusal and retained normalization are not declared in the required function contract. This finding makes no claim that invalid fields are accepted.

**Decision-complete minimal correction.** Add substantive rustdoc to the two existing functions. Describe typed decoding and path-bearing failure for `decode`; describe kind dispatch, retained preprocessing and unsupported External/malformed kind-body errors for `spec_from_kind_value`. Include actual `# Errors` conditions on both. Preserve bodies, accepted inputs, error families and formatting semantics. Deletion of the new decoder would lose the required authored-path diagnostics; introducing a new parser, wrapper or owner is unnecessary. Existing in-place documentation is the smallest safe mechanism.

**Focused closure proof.** Inspect both doc blocks against their complete bodies and the two direct callers; `mise run fmt:check` verifies formatting. Documentation-only correction needs no new runtime test. If decoding code is changed despite this recommendation, retain and run the exact existing proof:

```bash
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::verifier_card_tests::removed_scheduled_samples_fields_fail_at_their_path)'
```

### FIND-TASK-002-2 — CONFIRMED — REGRESSION

**Obligation.** TASK-002 caller/documentation closure and REQ-012/AC-007 require selector-free authoring and agreeing public projections. AGENTS.md §4 requires authoritative derive-backed public error metadata; the error guidance identifies remediation as a structured public contract.

**Exact producer.** `crates/wyrd-spec/src/error.rs:2076`, `SpecTriggerActivationMismatch` remediation. Changed causal boundary: `crates/wyrd-spec/src/card/verifier/mod.rs:636` removes binding inputs while retaining `deny_unknown_fields`; `crates/wyrd/wyrd-server/src/components/cards/resolve.rs:37` preserves the activation refusal. Generated sibling: `docs/src/content/docs/api/errors.md:351`.

**Producer-to-consumer proof.** The remediation says to “map its samples slots in the binding inputs”. This is live metadata, not unused prose. `EffectiveSpecs::validate_bindings` enumerates all legal binding sites and invokes `validate_binding` (`resolve.rs:258`); that function loads the effective referenced/inline Trigger and Verifier, and calls `check_activation` for schedule activation. A samples-free Verifier produces `SpecTriggerActivationMismatch`. The existing authenticated route test `pg_card_registration_route::referenced_binding_refusals_leave_no_writes` (`:965`, mismatch branch `:1062`) reaches precisely this refusal and checks its public code and absence of writes.

The WyrdError derive emits `remediation()` from the attribute (`wyrd-error-derive/src/lib.rs:206`). `WyrdError::as_problem_json` (`error.rs:3580`) includes it; server `http/error.rs::wyrd_error_response_from_parts` serializes that problem for HTTP. The same problem projection is used by MCP. Native error/SDK projections consume central metadata rather than maintaining another activation-specific hint. Repository search found no second production constructor of this variant beyond `check_activation`; `message_details` merely exposes its stored message/details.

The docs sibling has a separate existing projection owner: `docs/scripts/generate_api_docs.py::error_catalog` reads derive attributes and `render_errors` renders the reference. `mise run docs:generate` runs this generator and the llms index generator. `codegen:regen` alone does **not** regenerate API error docs. The two discovery reports' source-level recommendation is valid; closure must use this actual docs owner.

After a caller selects a samples Verifier as advised, following the hint's additional mapping instruction inserts `VerificationBinding.inputs`. That field is absent from the candidate's deny-unknown-fields contract, so the shared decoder rejects it before persistence. The new contract test and raw-HTTP journey explicitly establish refusal of both empty and exact maps. Thus the stale hint was made wrong by this candidate; it is neither dormant nor unrelated wording debt.

**Observable consequence.** A human or agent following the server's recovery instruction creates another invalid registration. The typed contract and updated workflow guides contradict a reachable public error and its generated reference.

**Decision-complete minimal correction.** Edit only the variant's existing authoritative remediation text to direct callers to a recorded-samples Verifier whose own declarations supply the measurements; remove the instruction to add binding inputs. Preserve the error code, title, status, refusal condition, field details and guidance to use verify/submit for other evidence. Regenerate reference/index projections with `mise run docs:generate`; commit only outputs attributable to this metadata change. Do not hand-edit generated text, add aliases, add a consumer guard, change activation validation or alter ordinary submission/Operator behavior. Deleting the obsolete clause closes the cause; no new runtime mechanism is justified.

**Focused closure proof.** Extend the existing activation-mismatch branch in `referenced_binding_refusals_leave_no_writes` to inspect the returned remediation and establish that it directs accepted samples authoring rather than a binding map, keeping its code/no-write assertions. This existing HTTP assertion is enough; no new harness or journey suite. Run its exact command after confirming the unchanged name:

```bash
WYRD_LOG=info mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration -E "test(=pg_card_registration_route::referenced_binding_refusals_leave_no_writes)"'
```

Run `mise run codegen:check` for source-derived contract outputs and `mise run docs:check` for edited generated documentation, along with the required Rust format/lint lanes (`mise run fmt:check`, `mise run lints`). Inspect the rendered catalog row for this code. No broad gate or full user-journey suite belongs to this bounded remediation.

## Scope and proof reconciliation

The cumulative production change removes the selector representation through contract, registration projection, SQL storage/claim and scheduled runner input; `DriftEngine::checks` iterates every `spec.drift_checks()` and preserves existing `score`/unscored handling. JSON/media/trace optionality remains in `InputSlot::required`; mixed samples/other evidence remains rejected by the existing validator. Claim input now uses execution-owned window columns, and exact Verifier UID/subject/window ownership and retry/reclaim lifecycle remain in their existing owners. The new Rust/Python/TS scenarios cover both two-requirement outcome combinations, and Rust additionally bypasses local SDK refusal over HTTP. No extra scoring, retry, tenancy, durability, compatibility or availability correction is justified by these findings.

The samples error reports the parent slot path together with the explicit unknown `required` field. That identifies the authored field; a bespoke error parser to concatenate `.required` would add machinery without a demonstrated task failure. Deletion of the optional-slot omission test removes expressly retired behavior; replacement complete-requirement scenarios retain the relevant outcome proof. Retry/reclaim proof is composed from existing lifecycle tests plus immutable UID resolution and the unconditional iterator; a new two-slot failure-injection journey is not a justified remediation demand.

Standards notes about the stale derived drift-monitoring reference do not establish a separate blocking defect: active design wins and the skill excludes wording-only findings absent a public/behavioral consequence. This differs from FIND-TASK-002-2, which is emitted on the active HTTP path. No optional wording cleanup is added to the ledger.

The task explicitly records no separate RED execution. Source comparison cannot retroactively establish TDD, so this remains a historical process/evidence limitation. Do not manufacture a RED result or require production churn to recreate history. It is not a third source defect with a bounded correction.

## Verification limits

This validator ran read-only Git, CodeGraph and source inspection only. The primary's verification.md records independently executed: diff check; the exact removed-field contract test (1 pass); Python format and lint checks; Wyrd and Vala migration/idempotence tests (1 + 1 pass); and binding SQL tests (10 pass). The managed Postgres endpoint recovered through its sanctioned wrapper rebuild; no unavailable-Postgres assertion is warranted. These are attributed primary results, not this validator's executions.

Before completion, verification.md recorded two focused served-OpenAPI checks passing (2 selected, 268 skipped), plus setup migrations (1 + 1 pass). These inspect served Card/verification contracts but do not newly assert every removed scheduled field or constitute the full principals suite. Historical implementation counts remain separately attributed claims. No full journeys, broad gate, crash/recovery execution, mixed-version rollout qualification or performance checks were run by this validator. Neither retained finding depends on an unexecuted runtime result.

Skipped new runtime reruns and broad suites; the remaining risk is incomplete acceptance until the two bounded source/documentation corrections and their focused closure proof are complete.
