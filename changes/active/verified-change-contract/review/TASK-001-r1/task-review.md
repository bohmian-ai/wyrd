# TASK-001 Task Review — r1

- Subject: base `5293546f3` → candidate `2e09ae81213cb608253b75de276e3c946353ac35` (HEAD unchanged during review)
- Spec: `changes/active/verified-change-contract/spec.md` revision 32 (matches task `spec_revision: 32`)
- Reviewer: independent `task-rev`. The implementer's trailing "Implementation Evidence" section was not used as evidence.
- Verification source: orchestrator run log `scratchpad/verify/summary.txt`. The reviewer ran no builds or tests.

## Verification results observed

| Command | Result |
|---|---|
| `git diff --check` | rc=0 |
| `mise run fmt` (+ clean status) | rc=0 |
| `refs::completeness_tests::visitor_covers_every_slot_in_the_migration_table` | 1 passed |
| `graph::composition::tests::publication_validation_rejects_duplicate_targets` | 1 passed |
| `wyrd-loader validate::tests::validate_rejects_duplicate_component_publication_targets` | 1 passed |
| `wyrd-cli loader::end_to_end::load_end_to_end_reference_tree` | 1 passed |
| `wyrd-spec --all-features --lib` | 854 passed |
| `mise run codegen:check` | rc=0 |
| `mise run check:client-tier`, `check:pyo3-scope` | rc=0 |
| `mise run test:shared`, `test:cards:unit`, `test:cards:integration` | rc=0 |
| `mise run test:cli:journey` | 23 passed, 5 ignored (the 3 ignored card_lifecycle tests belong to other lanes) |
| `mise run test:wyrdstate:journey` | rc=0 |
| `mise run test:sql` | 106 + 2 passed |
| `mise run py:test:unit` / `py:typecheck` | 463 passed / rc=0 |
| `ts:typecheck`, `ts:test:unit`, `lints`, `test:wyrd` | **pending** (not finished at review time) |
| `mise run test:bifrost` / `test:bifrost:integration:server` | **not run.** See TR-1: it is now broken. |

## Acceptance matrix

| Requirement/AC/constraint/non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-045 Verifier registrable via normal envelope/CardRef/composite/schema/loader | `envelope.rs` `CardKind::Verifier`, `Spec::Verifier`; `card/verifier.rs` | `verifier_card_tests::*`; typed_state/end_to_end fixtures register through real server (`multi_card_service_get_hydrates_complete_and_metadata_bundles`, wyrdstate journey) | PASS |
| REQ-046 one adjacently tagged closed `implementation` | `verifier.rs:40` `tag="kind", content="spec"` | `unknown_implementation_kind_fails_to_deserialize` | FAIL (TR-3: unknown sibling keys inside `implementation` are silently accepted) |
| REQ-047 variants exactly Drift(DriftSpec)/Eval(EvalSpec) | `verifier.rs:44-49` | spec lib tests | PASS |
| REQ-056 no Verifier DAG | one implementation per Verifier; no composition added | source | PASS |
| REQ-090 `verified_by: Vec<VerificationBinding>` on Service, component, standalone Agent | `service.rs`, `agent.rs`, `verifier.rs::VerificationBinding` | visitor test (all three locations); journeys cover component location only | FAIL (TR-2: Service-level and standalone-Agent bindings are never registered end-to-end) |
| REQ-091 Ref / InlineableRef<Trigger> / InlineableRef<Operator> semantics | `VerificationBinding` fields; `SlotValue::InlineableTrigger/Operator` | loader e2e: path→sibling and inline Trigger | FAIL (TR-2: CardRef form of registered Trigger/Operator and inline Operator have no registration evidence) |
| REQ-092 resolve, UID-pin, kind/tenant/authz, nested traversal, relationships, fail closed | `resolve.rs` visitor-driven collect/bind + `validate_effective_bindings`; relationships via `scope_child_card_refs` | hydrate journey (12 cards reachable); Python `assert_all_refs_are_exact_and_uid_bearing` | FAIL (TR-2: no unresolved/wrong-kind/unauthorized/cross-tenant binding cases) |
| REQ-093 TriggerSpec flattened closed activation; no Verifier/Operator/threshold content | `trigger.rs` | loader e2e | FAIL (TR-3: `observations_ready` accepts arbitrary extra keys, including `verifier`/`pass_rate`) |
| REQ-094 / REQ-143 OperatorSpec flattened; Workflow parseable but rejected in `on_failure` | `operator.rs` flatten; `composition.rs::check_on_failure` (inline); `resolve.rs:98-116` (referenced) | none | FAIL (TR-2: no test for either path) |
| REQ-102 duplicate Verifier per occurrence rejected | `composition.rs::binding_validation_errors` | `publication_validation_rejects_duplicate_targets`, loader dup test | PASS (pure/loader); server path covered through `validate_request` |
| REQ-103 `verified_by` replaces `publishes_to`, no alias | field removed from all specs; grep clean in code | grep | FAIL (TR-5: stale `publishes_to`/publisher vocabulary remains in the public error payload and in `EvalRecordObservation` docs, which are generated into schemas) |
| REQ-109 / INV-014 Drift/Eval not registrable; 15 kinds; authorities agree | `CardKind`; migration `20260601000025`; AGENTS.md (already 15 at base), design/doctrine updated | `native_card_kind_count_is_locked`, `card_kind_schema_is_string_enum_with_registrable_kinds`, `removed_drift_and_eval_card_kinds_fail_to_deserialize` | FAIL (TR-4, TR-5) |
| REQ-110 Drift payload removals, 3 pairs only, SPC+Metric rejected, Statistical only | `drift.rs`; `vala-drift` fitter branch removed | `rejects_spc_with_metric_signal`, `rejects_non_statistical_conditions` | PASS |
| REQ-111 Eval payload retained | `EvalSpec` unchanged, wrapped | `eval_verifier_yaml_deserializes_as_verifier_card` | PASS |
| REQ-113 reuse existing mechanisms | visitor, composite pipeline, `vala-eval` reused | source | PASS (TASK-001 portion) |
| REQ-114 authorities describe Verifier model | wyrd-design, doctrine, bifrost-design, references updated | `codegen:check` | FAIL (TR-5: generated schemas still say `kind: Eval` / "DriftCard"). Operator-connection prose deferral: accepted, because TASK-008 owns REQ-114 closure and the connections do not exist yet |
| REQ-116 remove eval.runs/assertions, drift_alerts + SQL API | `tables/mod.rs` (6 builtins), migration `20260910000028`, vala-sql queries/tests deleted | `test:sql` | PASS (code). Regression assertion missing (TR-2) |
| REQ-120 retire `/v1/eval/runs` | server eval component, client eval handle, CLI `--server` deleted; error codes removed at HEAD | grep | FAIL (TR-6: pull-protocol wire types, lease id, fixtures, and dead CLI error variants remain) |
| REQ-144 retire vala-core alert router, crate, schemas, generator, check | crate dir, workspace, mise codegen, `check:alert-router-schema-drift`, test-families removed | grep clean | PASS |
| INV-001 inline bindings, not Cards | `VerificationBinding` is not a kind | source | PASS |
| INV-006 no secrets/runtime health in Verifier | typed fields, `details` map removed | none | FAIL (TR-2/AC-004: no secret-rejection evidence) |
| INV-012 reuse Drift/Eval contracts | payloads wrapped unchanged except approved removals | spec lib tests | PASS |
| INV-013 inline == referenced semantics | flattened Trigger/Operator shapes; server loads same `Spec` | source only | PASS (shape). The strictness gap is in TR-3 |
| AC-004 | see REQ-046/093/094, secret | partial | FAIL (TR-2, TR-3) |
| AC-018 (TASK-001 portion) | component bindings journey | hydrate journey | FAIL (TR-2) |
| AC-021 | spec-level rejection only | unit only | FAIL (TR-4, TR-5) |
| AC-022 retirement portion | tables/crate/check removed | `test:sql`, grep | FAIL (TR-1: build references to a deleted test target remain) |
| Canonical visitor covers new slots; no old alias | `refs/mod.rs` | visitor test | PASS |
| Only Drift/Eval as implementations; neither a kind | `VerifierImplementation` | tests | PASS |
| Engines reusable, no future executor | `vala-drift`/`vala-eval` retained; no new executor | `test:shared` | PASS |
| Constraint: no compat alias / binding Card / DAG / speculative variants | none found | grep | PASS |
| Constraint: no second reference walker | loader/server/refs use `ReferenceSlotVisitor` | source | PASS (a binding-location enumerator is duplicated, see TR-8) |
| Constraint: no TS CardKind abstraction | TS diff = generated error codes only | source | PASS |
| Constraint: wyrd-spec IO/async/PyO3-free | no such imports added | `check:pyo3-scope` | PASS |
| Constraint: generated files regenerated, not hand-edited | `codegen:check` clean | rc=0 | PASS |
| AGENTS §12 no new `#[allow]` | `verifier.rs:43` | source | FAIL (TR-7) |
| Scenario 3 "registration alone activates no work" | no projection/runtime exists yet | by construction | PASS |

## Findings

### TR-1 — REGRESSION — retired test target still referenced by the Bifrost gate
- Violated obligation: AC-022 / Scenario 3 (no build references to retired paths). AGENTS §11: `test:bifrost` is a capability gate.
- Location: `mise.toml:256` (`test:bifrost:integration:server:inner` passes `--test pg_eval_v1_protocol`); `.github/scripts/detect-changes.sh:64` (path pattern lists `pg_eval_v1_protocol`).
- Evidence: `crates/wyrd/wyrd-server/tests/pg_eval_v1_protocol.rs` and its `[[test]]` entry in `crates/wyrd/wyrd-server/Cargo.toml` were deleted. `scripts/run-bifrost-tests.sh:11` runs `integration:server`.
- Consequence: `mise run test:bifrost` and `test:bifrost:integration:server` fail with "no test target named `pg_eval_v1_protocol`". The orchestrator's verification list does not exercise this lane, so the break is not caught there.
- Correction: remove `--test pg_eval_v1_protocol` from `mise.toml:256` and the file from the detect-changes pattern. Verify with `mise run test:bifrost:integration:server`.

### TR-2 — MISSING — required RED coverage for binding registration refusals and forms
- Violated obligation: Scenario 1 RED ("accepted Verifier YAML and each refusal", including secret-bearing payloads); Scenario 2 RED ("real composite-registration cases for the three binding locations and all reference forms", with unresolved, wrong-kind, unauthorized, cross-tenant, duplicate Verifier, duplicate Operator, and Workflow-on-failure rejected atomically); Scenario 3 RED (regression assertions that retired routes/tables are absent); AC-004; AC-018; AGENTS §11 (user-facing negative flows need journey coverage).
- Location: new stable codes have no test anywhere (`git grep` finds them only in `error.rs`, `composition.rs`, `resolve.rs`, `error-codes.ts`): `WYRD_SPEC_400_UNSUPPORTED_OPERATOR_ACTION`, `WYRD_SPEC_400_TRIGGER_ACTIVATION_MISMATCH`, `WYRD_SPEC_400_DUPLICATE_BINDING_OPERATOR`, `WYRD_SPEC_400_INVALID_BINDING_REF_KIND`, `WYRD_SPEC_400_INVALID_VERIFIER_REF_KIND`. Server logic lives at `crates/wyrd/wyrd-server/src/components/cards/resolve.rs:66-176` (`validate_effective_bindings`, `check_activation`). Only `UNBOUND_VERIFIER_PEER` and `DUPLICATE_VERIFICATION_BINDING` are tested.
- Evidence:
  - The journey fixtures (`crates/wyrd/wyrd-cli/tests/fixtures/loader/end_to_end/service.yaml`, `.../typed_state/typed-service.yaml`) bind only on components. They use path Verifiers, path/sibling Triggers and Operators, and inline Triggers.
  - No fixture registers a Service-level `verified_by`, a standalone Agent `verified_by`, a CardRef to an already-registered Trigger/Operator (the external UID-pin path in `resolve.rs::EffectiveSpecs::load`), or an inline Operator.
  - No test covers: a referenced or inline Workflow Operator in `on_failure`; a schedule Trigger on an Eval Verifier; a wrong-kind or unresolved binding ref; a cross-tenant or unauthorized binding ref; a secret-bearing Verifier payload; `/v1/eval/runs` absence; `vala.eval.runs`/`vala.drift_alerts` absence.
- Consequence: the most important new server behavior has no executable proof. This includes the referenced-Operator Workflow rejection, which is only reachable after registry load, and pre-persistence atomicity. A regression in `validate_effective_bindings` or in the external-ref UID pinning of Trigger/Operator slots would pass every current lane.
- Correction:
  - Add real-server registration cases (`pg_card_registration_route.rs` or the CLI journey) for:
    - Service-level, component, and standalone-Agent bindings;
    - Verifier by path and by CardRef;
    - Trigger and Operator by path, by CardRef to a previously registered Card, and inline;
    - stored refs UID-pinned, and derived relationships including the Verifier/Trigger/Operator.
  - Add a refusal case, each asserting its stable code and that no Card row persisted, for:
    - an unresolved binding ref;
    - a wrong-kind binding ref;
    - a cross-tenant binding ref;
    - an under-privileged caller;
    - duplicate Verifier;
    - duplicate Operator;
    - an inline Workflow Operator and a referenced Workflow Operator;
    - a Trigger/implementation mismatch.
  - Add unit coverage for secret-shaped Verifier payload rejection.
  - Add regression assertions that `builtin_table("eval","runs"|"assertions")` is `None` and that `POST /v1/eval/runs` is 404.

### TR-3 — INCORRECT — Trigger and Verifier implementation mappings silently accept unknown fields
- Violated obligation: Scenario 1 ("unknown fields … fail before persistence"); AC-004 ("unknown fields"); REQ-093 (Trigger MUST NOT reference a Verifier/Operator or contain pass-rate criteria); REQ-046.
- Location: `crates/wyrd-spec/src/card/trigger.rs:15-43`. `deny_unknown_fields` was removed from `TriggerSpec` to allow `#[serde(flatten)]`. `TriggerActivation::ObservationsReady` is a unit variant of an internally tagged enum, so serde's `InternallyTaggedUnitVisitor` (serde 1.0.229 `private/de.rs` ~2990) drains and ignores remaining map entries. `crates/wyrd-spec/src/card/verifier.rs:40` puts no `deny_unknown_fields` on the adjacently tagged `VerifierImplementation`, and serde ignores unknown keys beside `kind`/`spec`.
- Evidence: base `TriggerSpec` carried `#[serde(deny_unknown_fields)]`. The only unknown-field test is `unknown_verifier_spec_field_fails_to_deserialize`, which covers top-level `VerifierSpec` only. `runs_on: {kind: observations_ready, verifier: {...}, pass_rate: 0.9}` and `implementation: {kind: eval, spec: {...}, thresholds: {...}}` both decode. The server re-serializes the typed spec (`service.rs:993-997`) and drops the content silently.
- Consequence: authored intent (a mistyped `descripton`, a misplaced `cron` on an Eval trigger, a verdict threshold, a credential-shaped key) is accepted with no diagnostic. The strictness regresses against base Trigger Cards and contradicts the closed-contract requirement.
- Correction: add `deny_unknown_fields` to `VerifierImplementation`. Make `TriggerActivation::ObservationsReady` reject extra keys, for example as an empty struct variant `ObservationsReady {}` with `deny_unknown_fields` (the wire stays `{kind: observations_ready}`), or add a validated decode. Add RED tests for an unknown key on an inline and a referenced `observations_ready` Trigger, and an unknown key inside `implementation`.

### TR-4 — MISSING — no boundary evidence that `kind: Drift` / `kind: Eval` registration is rejected
- Violated obligation: AC-021 ("Contract, loader, registry, schema, CLI, SDK, HTTP, and MCP evidence MUST show new `kind: Drift` and `kind: Eval` registration rejected"); REQ-109 ("the server MUST reject").
- Location: the only evidence is `crates/wyrd-spec/src/card/mod.rs` `removed_drift_and_eval_card_kinds_fail_to_deserialize`. Nothing covers `wyrd-loader`, `POST /v1/cards` (`pg_card_registration_route.rs`), `wyrd plan/apply` (`card_lifecycle.rs`), or Python.
- Consequence: the stable error a client receives for an old-kind submission is unpinned at every public surface.
- Correction: add one loader or CLI `plan` case and one HTTP registration case (plus a Python SDK case if it exposes registration) that submit `kind: Drift` and `kind: Eval` and assert the stable rejection code. MCP has no card-registration tool (`crates/wyrd/wyrd-mcp/src` exposes only Bifrost). Record that as N/A instead of adding a surface.

### TR-5 — INCORRECT — generated public schemas and public errors still advertise the old Card model
- Violated obligation: AC-021 ("No generated public schema … may advertise a second registrable Drift or Eval Card"); REQ-103; REQ-114.
- Location:
  - `crates/wyrd-spec/src/vala/eval/spec.rs:28-31` ("The typed spec body for an `Eval` card … envelope-level `kind: Eval`"). This propagates into generated `crates/wyrd-spec/schemas/{card,get_card_response,verifier_spec,verifier_implementation,eval_spec}.json`, the matching `tests/schemas`, `tests/fixtures/eval/schemas/eval_spec.schema.json`, and `docs/public/llms-full.txt:13600`.
  - `crates/wyrd-spec/src/card/drift.rs:1,15` ("DriftCard spec body").
  - `crates/wyrd-spec/src/vala/eval/record.rs:58` (`eval_ref` doc routes via "`publishes_to` binding"; generated into `eval_record_observation.schema.json` and `agent_turn_submission.schema.json`).
  - `crates/wyrd/wyrd-server/src/components/cards/service.rs:1342,1350`: the `UNBOUND_VERIFIER_PEER` message says "no submitted publisher", and details list `publisher_kinds: ["Data","Model","Agent","Service"]`. Data and Model cannot own `verified_by`.
  - `crates/wyrd/wyrd-cli/src/error.rs:134,139`: `WYRD_CLI_400_NOT_EVAL_CARD` says "card file must contain kind: Eval" and "Pass a Wyrd card envelope with kind: Eval", but `eval/run.rs` now requires `kind: Verifier` with `implementation.kind: eval`.
- Consequence: agents reading the generated schema, error details, or CLI remediation are told to author `kind: Eval` or to treat Data/Model as binding owners, which is the rejected model.
- Correction: rewrite those rustdocs and error texts to the Verifier and `verified_by` model (binding owners: Service, Service component, standalone Agent). Regenerate with `mise run codegen:check` and `docs:check`.

### TR-6 — MISSING — Eval pull-protocol contract remnants survive the route retirement
- Violated obligation: REQ-120; Scenario 3 ("`/v1/eval/runs`, old client/CLI pull mechanics … have no production or build references"); also contradicts `architecture/wyrd-design.md:1058` and `architecture/references/domain/evaluation.md:16` ("There is no Eval pull protocol").
- Location:
  - `crates/wyrd-spec/src/vala/eval/protocol.rs:1-8,48-78`: the module is documented as "Server-hosted eval pull-protocol wire shapes", and `EvalRunOpenRequest`/`EvalRunOpenResponse` "open and lease a new eval run against `eval_ref`".
  - `crates/wyrd-spec/src/vala/ids.rs:206-217` `LeaseToken`, used only by those types and their tests.
  - Re-exports at `crates/wyrd-spec/src/vala/eval/mod.rs:62,70-71`.
  - Schema generation at `crates/wyrd-spec/examples/gen_schemas.rs:71,240-241`, which produces `tests/fixtures/eval/schemas/eval_run_open_{request,response}.schema.json`.
  - Dead CLI catalog variants from the removed `--server`/agent pull mode at `crates/wyrd/wyrd-cli/src/error.rs`: `SimulatedUserScriptRequired`, `ScriptedTurnMissing`, `ServerRequiresAgentUrl`, `ServerRequiresToken`, `ServerRejectsRecords`, `RecordsRequireSubject`, `AgentTurnFailed`. Each had constructors at base and has none at HEAD.
- Consequence: the retired protocol's public contract (lease request/response, stable CLI error codes for `--server`) is still generated and published. That is the "hidden second Eval execution path" contract without a route, and the implementer lists it as unresolved.
- Correction: delete `EvalRunOpenRequest`, `EvalRunOpenResponse`, `LeaseToken` (once no consumer remains), their tests, gen_schemas lines, and fixture files. Rewrite the `protocol.rs` module doc to describe only the local orchestrator types that `vala-eval` still consumes (`TurnDirective`, `ConversationTurn`, `AgentTurnSubmission`, `SimulatedUserTurn`, and so on). Delete the seven unreferenced `WyrdCliError` variants. Regenerate.

### TR-7 — VIOLATION — new `#[allow(clippy::large_enum_variant)]` in production code
- Violated obligation: AGENTS.md §12 ("never … add `#[allow]`"). The sanctioned exception is test-only usage.
- Location: `crates/wyrd-spec/src/card/verifier.rs:41-43`.
- Evidence: the justification comment says boxing "would change the authored wire shape's ergonomics". That is false: `Box<DriftSpec>`/`Box<EvalSpec>` serialize identically under serde and schemars. Only Rust pattern-match ergonomics change.
- Consequence: a lint gate is suppressed on a new public type.
- Correction: box the large variant(s), e.g. `Drift(Box<DriftSpec>)` or `Eval(Box<EvalSpec>)` per clippy's report. Remove the `#[allow]` and pass `mise run lints`.

### TR-8 — DRIFT — third enumerator of binding locations
- Violated obligation: task constraint ("reuse its one canonical reference visitor … so loader resolution, UID pinning, auth scope, and relationships cannot drift"); AGENTS §15 (reuse the existing owner).
- Location: `crates/wyrd/wyrd-server/src/components/cards/resolve.rs:121-143` `owned_bindings` re-implements the Agent/Service/component enumeration already in `crates/wyrd-spec/src/graph/composition.rs:236` `verification_bindings`. The two differ only in field paths.
- Consequence: adding a binding location requires updating several parallel enumerations, and missing one silently skips server-side Trigger-pairing and Workflow checks for that location.
- Correction: expose one path-bearing enumerator from `wyrd_spec::graph` (for example, make `verification_bindings` return `(field, &VerificationBinding)`). Use it from `composition.rs` and `resolve.rs`, and delete `owned_bindings`.

## Deferrals judged acceptable
- REQ-114 Operator-connection prose: TASK-008 carries REQ-114 closure, and connections and the Slack/PagerDuty contract ship in TASK-007. Documenting them now would describe non-existent behavior.
- `EvalRecordObservation.eval_ref`/`drift_ref` removal: TASK-002 owns the record changes. TR-5 covers only the stale `publishes_to` text in its generated doc.
- AGENTS.md 15-kind catalog: already correct at base `5293546f3`, so no change was required.

## Verdict

**FAIL.** Implementing remediation of TR-1 through TR-8 is required. TR-1 (broken `test:bifrost` lane), TR-2 (no proof of the new server refusal paths or the non-component binding forms), and TR-3 (unknown fields silently accepted) block acceptance on their own. Verification lanes `ts:typecheck`, `ts:test:unit`, `lints`, and `test:wyrd` were still pending at review time.
