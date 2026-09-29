# TASK-001 r2 domain review — contracts and projections

## Subject and boundary

- Repository: `/Users/stevenforrester/Documents/GitHub/verified-change-contract`
- Immutable subject: base `5293546f33b3a5fd9de529098e23ea70d472c412` → cumulative candidate `dd0503e7149017d760a987346e2838001e5c3429`
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 32
- Original task: `changes/active/verified-change-contract/tasks/TASK-001-verifier-contract-and-registration.md`
- Prior review: `review/TASK-001-r1/verdict.md`, `findings-validation.md`, and `TASK-001-R1-verifier-binding-closure.md`
- Reviewed boundary: the `Verifier`/binding/Trigger/Operator wire contracts; canonical reference discovery; loader parse, resolution, validation, and ordering; server registration resolution and UID pinning; derived relationships; generated JSON Schema and OpenAPI; Rust, Python, TypeScript, CLI, and documentation projections.
- Result: **FAIL**. Two material contract findings remain.

HEAD is `5f14f3c325cc8c45081fe9ad0543e7794b4a4e0f`, whose only difference from the candidate is `AGENTS.md`. `git diff --quiet dd0503e71 5f14f3c32 -- . ':(exclude)AGENTS.md'` returned success, so the reviewed source matches the immutable candidate.

## Authority and source coverage

| Boundary | Authority | Source and proof inspected | Result |
|---|---|---|---|
| Native Card catalog and closed Verifier union | REQ-045/046/047/109, INV-012/014; `AGENTS.md` §2; `architecture/wyrd-design.md` Verifier contract; doctrine | `envelope.rs::{CardKind,Spec}`, `card/verifier.rs`, `card/{drift,eval}.rs`, Verifier fixtures and unit tests | PASS, except stale public Eval rustdoc in DC-2 |
| Binding ownership and strict authored shape | REQ-090/091/093/094/102/103, INV-001/006/013, AC-004 | `AgentSpec`, `ServiceSpec`, `ServiceComponent`, `VerificationBinding`, `TriggerSpec`, `OperatorSpec`, strict serde tests and generated JSON Schema | PASS |
| Canonical traversal, loader resolution, and graph ordering | REQ-045/092, AC-018; task prohibition on a second walker | `refs::ReferenceSlotVisitor`, `Spec::binding_sites`, `spec_binding_errors`, loader `parse.rs`, `resolve.rs`, `validate.rs`, `order.rs`, visitor completeness tests | PASS |
| Server registration, effective referenced specs, and persistence projection | REQ-092/094, AC-018 | `cards/resolve.rs::{resolve_card_references,EffectiveSpecs,bind_card_references}`, `cards/service.rs::{validate_request,submission_card}`, server refusal tests | PASS |
| Relationship derivation and UID-bearing projections | REQ-092, AC-018 | `graph::relationships_from_spec`, `scope_child_card_refs`, CLI lifecycle assertions for Verifier/Trigger/Operator relationships and UID pins | PASS |
| JSON Schema/codegen | REQ-045/046/091/093/109/114, AC-004/021; generated files must derive from owners | `gen_schemas.rs`, `schemas/{card,verifier_spec,verifier_implementation,verification_binding,trigger_spec,trigger_activation}.json`, matching golden files; recorded `codegen:check` | PASS |
| HTTP/OpenAPI contract | REQ-045/109/114, AC-021; language-agnostic wire contract | `http/openapi.rs`, `Spec::PartialSchema/ToSchema`, `VerifierImplementation::PartialSchema/ToSchema`, generated `openapi.yaml` | **FAIL — DC-1** |
| Rust/Python/TypeScript client projections | REQ-109/114, AC-021; task prohibition on a symmetry-only TS `CardKind` | `wyrd-client::WyrdState::verifier`, Python native wrapper and generated stubs, `wyrd-cards::Kind`, TS generated error-code union, UI kind projection, recorded Python/TS lanes | PASS |
| Retired Eval pull/public contract | REQ-120 and Scenario 3 | deleted route/client/CLI protocol, `vala/eval/protocol.rs`, ID exports, generators, fixtures, stable error projections | PASS for code and generated artifacts; **FAIL for public rustdoc — DC-2** |

## Prior-finding closure

| Prior finding | Contract-domain closure | Result |
|---|---|---|
| FIND-TASK-001-1 | Retired Bifrost server target removed from `mise.toml` and change detection. | PASS (verification owned by broader review) |
| FIND-TASK-001-2 | `Spec::binding_sites` is now the single binding-location owner. Nested inline-Agent bindings are refused by `spec_binding_errors`, consumed by loader, composition, and server request validation. | PASS |
| FIND-TASK-001-3 | `VerifierImplementation` has `deny_unknown_fields`; `ObservationsReady {}` preserves the wire shape while rejecting sibling keys; JSON Schema carries `additionalProperties: false`. | PASS |
| FIND-TASK-001-4 | Stable-code unit branches, loader old-kind refusal, real-server referenced binding refusals, and the CLI registration/hydration journey were added. | PASS within the r1 remediation boundary |
| FIND-TASK-001-5 | Error payloads, generated JSON Schemas, CLI remediation, and named docs use Verifier vocabulary. Two public Rust module docs outside the r1 location list still describe the retired Card API. | **INCOMPLETE — DC-2** |
| FIND-TASK-001-6 | Pull request/response types, lease token, fixture schemas, exports, and unreachable CLI variants are absent. | PASS |
| FIND-TASK-001-7 | Task-verification prose removed from the changed composition source. | PASS |
| FIND-TASK-001-8 | The five named fallible items carry intent and `# Errors` documentation. | PASS |
| FIND-TASK-001-9 | `EffectiveSpecs` owns resolved identities and the effective-spec cache; validation is an inherent IO workflow. | PASS |
| FIND-TASK-001-10 | The named imports are module-scoped and signatures use imported names. | PASS |

## Material findings

### DC-1 — INCORRECT — the generated OpenAPI cannot resolve the new Verifier contract

- **Violated obligation:** REQ-045 requires normal schema generation for Verifier Cards; REQ-109/114 and AC-021 require HTTP/OpenAPI and generated public contracts to expose the same Verifier + `verified_by` model. `AGENTS.md` §2 and the Wyrd doctrine make the language-agnostic wire contract authoritative for all clients.
- **Exact location:** `crates/wyrd-spec/src/envelope.rs:289-324`; `crates/wyrd-spec/src/card/verifier.rs:111-136`; generated `openapi.yaml:1922,3967,4200,4262,4332,4478,4631,4821`.
- **Evidence:** the OpenAPI document emits `$ref` targets for `VerificationBinding`, `VerifierImplementation`, and `TriggerActivation`, but its `components.schemas` defines none of those names. `rg '^    (VerifierImplementation|VerificationBinding|TriggerActivation):' openapi.yaml` is empty while the references above are live from `AgentSpec`, `ServiceSpec`, `Spec`, `TriggerSpec`, and `VerifierSpec`. `Spec::schemas` registers only the 15 direct spec bodies. The custom `VerifierImplementation::schemas` pushes only `DriftSpec`, not the `VerifierImplementation` component itself, and no owner registers `VerificationBinding` or `TriggerActivation`. The recorded clean `codegen:check` proves artifact reproducibility, not reference validity.
- **Observable consequence:** an HTTP/OpenAPI consumer reaches unresolved component references when it tries to inspect or generate the new Verifier implementation, binding, or Trigger activation types. The public HTTP schema therefore does not actually describe the TASK-001 Card contract even though the runtime and standalone JSON Schemas do.
- **Required testable correction:** extend the existing `utoipa` schema-registration owner so every component reachable from the new Verifier, binding, and Trigger branches is present in `components.schemas`; do not create a second schema model. Add one focused OpenAPI test that starts from the Verifier/binding/Trigger roots and proves every reachable component reference resolves, then regenerate `openapi.yaml` and run `mise run codegen:check`.

### DC-2 — DRIFT — public Rust docs still promise the retired `Eval` Card and `Spec::Eval`

- **Violated obligation:** REQ-109 requires `Eval` to cease being a Card kind with no compatibility registration or alternate public story; Scenario 3 requires old client/public contract paths to be unreachable; REQ-114 requires published authorities and documentation to describe one model.
- **Exact location:** `crates/wyrd-spec/src/card/eval.rs:1-8`; `crates/wyrd-spec/src/vala/eval/mod.rs:1-11`.
- **Evidence:** `card/eval.rs` says it is an “`Eval` card spec” and that its re-export preserves `Spec::Eval`, a variant deleted by this candidate. `vala/eval/mod.rs` still calls `EvalSpec` the body for the “`Eval` card kind.” The latter module was materially modified during the pull-protocol retirement, so its public module contract remained in the changed boundary. `git grep 'Spec::Eval' -- crates` now finds only this false rustdoc; production source contains only `Spec::Verifier(VerifierImplementation::Eval(_))`.
- **Observable consequence:** Rust API documentation directs callers to a nonexistent enum variant and advertises the explicitly retired registrable kind. Copying the documented shape fails to compile or leads an author back to rejected `kind: Eval` YAML.
- **Required testable correction:** rewrite only these module comments to describe `EvalSpec` as the typed `eval` implementation payload re-exported at `card::eval::EvalSpec`, reached through `Spec::Verifier`; retain the re-export and all Eval engine semantics. Prove closure with `git grep -n 'Spec::Eval\|Eval.*card kind' -- crates/wyrd-spec/src/card/eval.rs crates/wyrd-spec/src/vala/eval/mod.rs` returning empty, plus `mise run docs:check`.

## Verification limits

- I performed static source, cumulative-diff, generated-artifact, and caller/consumer inspection. I did not run Cargo-backed commands because reviewers share one checkout/target directory and repository rules require those commands to be sequential.
- The remediation implementation records these green lanes: `fmt`, `lints`, `codegen:check`, `docs:check`, `test:shared`, `test:cards:unit`, `test:cards:integration`, `test:cli:journey`, `test:wyrdstate:journey`, `test:sql`, `test:bifrost`, `test:wyrd`, Python unit/typecheck, TypeScript typecheck/unit, boundary checks, and focused contract/server tests. Those results do not exercise OpenAPI component-reference closure.
- TASK-008 owns the integrated HTTP/CLI/SDK/MCP cross-surface half of AC-021. This review does not demand a new MCP Card-registration tool, a TypeScript `CardKind` abstraction, runtime/executor behavior, or later-task SDK journeys.
- The pre-existing OpenAPI document has other unresolved component references. DC-1 is limited to the three new unresolved roots introduced by TASK-001 and the dependency closure needed to make those roots usable; it does not require unrelated OpenAPI cleanup.

## Overall result

**FAIL.** Loader and server composition behavior closes the prior binding defects, but TASK-001 cannot pass while the generated HTTP contract has unresolved Verifier components and public Rust documentation still advertises the retired Eval Card API.
