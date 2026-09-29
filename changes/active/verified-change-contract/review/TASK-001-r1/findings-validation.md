# TASK-001 r1 — Wave 2 findings validation (`ponytail-rev`)

## 1. Subject and inputs

- Repo: `/Users/stevenforrester/Documents/GitHub/verified-change-contract` (worktree).
- Base `5293546f3` → candidate `2e09ae81213cb608253b75de276e3c946353ac35`; HEAD confirmed at the candidate at the start and end of this validation.
- Read in full: `tasks/TASK-001-verifier-contract-and-registration.md` (its Implementation Evidence used only to locate claims, never as evidence), TASK-002..TASK-008 (deferral boundaries), the cited requirement blocks of `spec.md` rev 32 (REQ-045/046/056/090–094/102/103/109/110/114/116/120/143/144, INV-001/006/013/014, AC-004/018/021/022), `AGENTS.md`, `architecture/agent-rules.md`, and all four Wave 1 reports.
- Source traced directly: `crates/wyrd-spec/src/card/{verifier,trigger,operator,drift}.rs`, `graph/composition.rs`, `refs/mod.rs`, `vala/eval/{protocol,spec,record,mod}.rs`, `vala/ids.rs`, `examples/gen_schemas.rs`, `crates/shared/wyrd-loader/src/{validate,parse,order}.rs`, `crates/wyrd/wyrd-server/src/components/cards/{resolve,service,routes}.rs`, `crates/wyrd/wyrd-cli/src/{error.rs,eval/run.rs}`, CLI journey fixtures, `crates/wyrd/wyrd-server/tests/pg_card_registration_route.rs`, `crates/vala/vala-bifrost-redux/src/tables/mod.rs`, `mise.toml`, `.github/scripts/detect-changes.sh`, `docs/src/lib/components/CardTileGrid.svelte`.
- One serde behaviour claim (TR-3/DS-3) was verified empirically in a throwaway crate outside the repository (workspace serde 1.0.x + serde_yaml 0.9); no repository file was modified and no test lane was run.

## 2. Per-Wave-1-finding validation

| Source ID | Status | Reason (evidence) |
|---|---|---|
| TR-1 / SR-1 | **CONFIRMED** (→ FIND-1) | `mise.toml:256` still passes `--test pg_eval_v1_protocol`; the target and its `[[test]]` entry are deleted, so `test:bifrost:integration:server` (reached from `test:bifrost` via `scripts/run-bifrost-tests.sh:11`) cannot build. `.github/scripts/detect-changes.sh:64` also still names the file. |
| TR-2 / DS-1 | **REVISED** (→ FIND-4) | Gap confirmed: `git grep` finds `SpecInvalidVerifierRefKind`, `SpecInvalidBindingRefKind`, `SpecDuplicateBindingOperator`, `SpecUnsupportedOperatorAction`, `SpecTriggerActivationMismatch` only in `error.rs`, `graph/composition.rs`, `resolve.rs` and the generated TS code list — no test anywhere. Journey fixtures (`wyrd-cli/tests/fixtures/loader/end_to_end/service.yaml:28-50`, `.../card_lifecycle/typed_state/typed-service.yaml`) bind only on components with path refs plus one inline Trigger, so the registry-backed `EffectiveSpecs::load` branch (`resolve.rs:227-246`) never executes. Revised to the smallest proof set; the `/v1/eval/runs` 404 assertion is dropped. |
| TR-3 / DS-3 | **CONFIRMED** (→ FIND-3) | `trigger.rs:15-22` lost `deny_unknown_fields` for `#[serde(flatten)]`; `TriggerActivation::ObservationsReady` (`:43`) is a unit variant. Reproduced: `{kind: observations_ready, verifier: x, api_token: sk}` → `Ok`, while struct variant `Schedule` still rejects extras. `VerifierImplementation` (`verifier.rs:39-49`) has no `deny_unknown_fields`; `{kind: eval, spec: {...}, thresholds: 1}` → `Ok`. Both proposed fixes reproduce as working: `ObservationsReady {}` under the enum-level `deny_unknown_fields` rejects extras and still accepts `description`; `deny_unknown_fields` on the adjacently tagged enum rejects a sibling key. |
| TR-4 | **REVISED** (→ folded into FIND-4) | Only old-kind refusal evidence is `card/mod.rs::removed_drift_and_eval_card_kinds_fail_to_deserialize`. TASK-001 Scenario 1 RED names *contract and loader* coverage, so one loader refusal stays in scope. The HTTP/CLI/SDK/MCP half of AC-021 is TASK-008's (`requirements: [... AC-021, AC-022 ...]`, "HTTP/MCP ... refusals") → rejected as later-task-owned. MCP N/A is correct: `wyrd-mcp` exposes no card-registration tool. |
| TR-5 | **REVISED** (→ FIND-5) | Confirmed and generated-artifact-visible: `vala/eval/spec.rs:27-31` appears verbatim in `schemas/card.json:2631` and `schemas/verifier_spec.json:1784`; `card/drift.rs:1,15` ("DriftCard spec body") in `card.json:2344`, `verifier_spec.json:1497`; `service.rs:1342,1350` and `wyrd-loader/src/order.rs:134` emit "no submitted publisher" + `publisher_kinds: ["Data","Model","Agent","Service"]`, contradicting REQ-090; `wyrd-cli/src/error.rs:134-141` still says "kind: Eval" while `eval/run.rs:90-95` requires `Spec::Verifier`/`VerifierImplementation::Eval`. `EvalRecordObservation.eval_ref` sub-item dropped (TASK-002). |
| TR-6 / SR-6 | **CONFIRMED** (→ FIND-6) | `EvalRunOpenRequest`/`EvalRunOpenResponse` (`protocol.rs:48-72`) and `LeaseToken` (`vala/ids.rs:206-268`) have zero consumers outside their own module, its round-trip test, `gen_schemas.rs:71,240-241` and the generated fixtures; sibling protocol types (`TurnDirective`, `SimulatedUserMode`, `AgentTurnSubmission`, `ConversationTurn`, `SimulatedUserTurn`, `MAX_HISTORY_TURNS`) are still consumed by `vala-eval/src/orchestrator/*` and must stay. The seven `WyrdCliError` variants have zero constructors after `eval/server.rs`/`eval/agent.rs` deletion. |
| TR-7 | **REJECTED** | `agent-rules.md:20` is the specific authority over AGENTS §12's general clause: `#[allow(clippy::...)]` is permitted with an immediately preceding `// justification:` line, enforced by `check:clippy-allow-audit`. `verifier.rs:41-43` has it and mirrors the unchanged, identical suppression on `Spec` (`envelope.rs:226-228`). Wording is imprecise; substance (flat pattern-match shape for consumers) matches the precedent. repo-rev independently rated PASS. |
| TR-8 | **REVISED** (→ folded into FIND-2) | Duplication confirmed — `resolve.rs:121-146`, `composition.rs:236-246`, `service.rs:1188-1199`, `wyrd-loader/src/validate.rs:64-78` are four parallel enumerations of the same three binding locations. Not a separate defect: the missing owner is exactly what makes the DS-2 fail-open path reachable. |
| SR-2 | **CONFIRMED** (→ FIND-7) | `composition.rs:435-436`: "The name is retained from the retired publication check because the task verification list names it". The proposed test *renaming* is rejected. |
| SR-3 | **REVISED** (→ FIND-8) | Confirmed for materially rewritten fallible items: `drift.rs::validate_signal_method` (:440), `validate_signal` (:459), `validate_condition` (:490, new `ConditionNotStatistical`), `validate_profile_presence` (:505); and `resolve.rs:20-24` `resolve_card_references` (failure set grew, no `# Errors`). Rejected as paperwork: `DriftMethod::name`, `DriftSignal::variant_name`, `vala-drift::signal_variant`, the `utoipa::PartialSchema`/`ToSchema` impls. `resolve.rs::indexed` disappears with FIND-2. |
| SR-4 | **CONFIRMED** (→ FIND-9) | `resolve.rs:66-70` `validate_effective_bindings(conn, submissions, resolved)` re-threads `conn`/`resolved` into `EffectiveSpecs::load` (`:227-232`) per lookup. agent-rules.md:34 makes struct-centered ownership a hard criterion for new code; `EffectiveSpecs` already exists as the owner. |
| SR-5 | **CONFIRMED** (→ FIND-10) | agent-rules.md:9-10 explicit; sites are new: function-scoped `use` at `verifier.rs:58,82`; fully qualified types at `verifier.rs:57,80,105,159`, `composition.rs:140`, `refs/mod.rs:54,66,78,93,109,122` (base `refs/mod.rs` had none), `resolve.rs:329-330` (despite the `:9` import), `service.rs:1157-1158`. |
| domain-review-data (PASS) | **CONFIRMED PASS** | Re-checked independently: both migrations are new files and highest-versioned in their own migrator; `cards_kind_check` is explicitly named by `20260601000006_cards.sql`; `BUILTIN_TABLES` is now `[…; 6]` with no seeded catalog rows. Its non-blocking note (`vala-eval/src/executor.rs:86,119` naming `vala.eval.runs`) is folded into FIND-5 only because that file is already touched. |
| DS-2 | **CONFIRMED** (→ FIND-2) | Traced from `refs/mod.rs:534-543` (`visit_agent` visits `verified_by`) through inline callers at `:291` (Workflow step Agent) and `:345` (Eval LLM-judge `judge_ref`); all four validators match only top-level `Spec::Agent`/`Spec::Service`. `AgentSpec.verified_by` is `#[serde(default)]`, so it decodes inside any `InlineableRef<AgentSpec>`. |

## 3. Final deduplicated ledger

### FIND-TASK-001-1 — `test:bifrost` names a deleted test target
- Sources: TR-1, SR-1. **CONFIRMED**. **REGRESSION**.
- Obligation: AC-022 / Scenario 3 (no build references to retired paths); AGENTS §11 (`test:bifrost` is the Bifrost capability gate), §12.
- Location: `mise.toml:256`; `.github/scripts/detect-changes.sh:64`.
- Evidence: `crates/wyrd/wyrd-server/tests/pg_eval_v1_protocol.rs` and its `[[test]]` entry are deleted at HEAD; `git grep pg_eval_v1_protocol` still returns those two build files.
- Consequence: `mise run test:bifrost:integration:server`, `test:bifrost` and `gate` fail with "no test target named `pg_eval_v1_protocol`"; the lane was never run for this task.
- Correction: drop `--test pg_eval_v1_protocol` from the existing `test:bifrost:integration:server:inner` command and the `pg_eval_v1_protocol|` alternative from the existing bifrost path regex. Nothing else changes; remaining four targets keep their order and `--test-threads=1`.
- Closure proof: `mise run test:bifrost:integration:server` exits 0; `git grep -n pg_eval_v1_protocol -- mise.toml .github scripts` empty.

### FIND-TASK-001-2 — `verified_by` on a nested inline Agent is resolved, UID-pinned and persisted without any binding validation
- Sources: DS-2 (TR-8 folded in). **CONFIRMED**. **INCORRECT** (fail-open).
- Obligation: REQ-090 (bindings only on Service / Service component / standalone Agent), REQ-092 (wrong-kind, unauthorized, cross-tenant fail closed), REQ-094/REQ-143 (`workflow` rejected in `on_failure`), REQ-056, and the TASK-001 constraint that one canonical enumeration keeps resolution, pinning, authz scope and relationships from drifting.
- Location: `crates/wyrd-spec/src/refs/mod.rs:534-543`, reached from `:291` (inline Workflow step Agent) and `:345` (inline Eval LLM-judge Agent). Validators that never see those bindings: `crates/wyrd/wyrd-server/src/components/cards/service.rs:1188-1199`, `.../resolve.rs:121-146`, `crates/wyrd-spec/src/graph/composition.rs:236-246`, `crates/shared/wyrd-loader/src/validate.rs:64-78`.
- Evidence: `AgentSpec.verified_by` is `#[serde(default)]` (`card/agent.rs:47-49`), so it decodes inside every `InlineableRef<AgentSpec>`. The visitor hands those slots to `validate_and_collect_refs` (resolution), `bind_card_references` (UID pinning) and `scope_child_card_refs` (outbound relationships), while all four validators match only the top-level specs.
- Consequence: a caller with `card:write` successfully registers a Workflow with an inline step Agent, or an Eval Verifier with an inline judge Agent, whose `verified_by` names a `Model`/`Data` Card as `verifier`, carries a `workflow` Operator in `on_failure`, duplicates a Verifier, or mismatches its activation. The contract-violating binding persists with pinned refs and relationship edges, and an observation-target kind enters that Card's outbound edges.
- Correction (smallest safe boundary): give `wyrd-spec` one owner that enumerates every `verified_by` list reachable in a submitted spec together with its field path, and make it the single source for all four call sites. Top-level locations keep running the existing `binding_validation_errors`; any binding list reached through an inline Agent body is refused with a stable 400 — reuse the existing `WYRD_REGISTRY_400_INVALID_CARD_SPEC` refusal rather than minting a new catalog code, so no new public contract decision is needed. Preserve unchanged: the canonical `ReferenceSlotVisitor` walk, UID pinning, relationship derivation, pre-transaction rejection ordering, and existing top-level codes and `details` payload shapes. `resolve.rs::owned_bindings` is deleted in favour of the shared owner (the TR-8 duplication).
- Closure proof: `mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=graph::composition::tests::<new nested-binding refusal test>)'` plus a real-server refusal case in the existing registration suite asserting the stable code and no writes (`mise run test:cards:integration`).

### FIND-TASK-001-3 — `observations_ready` Triggers and `implementation` silently accept unknown fields
- Sources: TR-3, DS-3. **CONFIRMED**. **INCORRECT**.
- Obligation: AC-004 (unknown fields and secret-bearing payloads rejected before persistence), REQ-093, REQ-046, INV-006.
- Location: `crates/wyrd-spec/src/card/trigger.rs:15-22,43`; `crates/wyrd-spec/src/card/verifier.rs:39-49`.
- Evidence: `deny_unknown_fields` was removed from `TriggerSpec` to permit `#[serde(flatten)]`, and serde's internally tagged unit-variant visitor drains and ignores remaining map entries, so `{kind: observations_ready, verifier: …, api_token: sk-live-…}` decodes `Ok` (reproduced against workspace serde/serde_yaml). The adjacently tagged `VerifierImplementation` likewise ignores keys beside `kind`/`spec`. Base `TriggerSpec` carried `deny_unknown_fields`, so this is a strictness regression. The server re-serializes the typed spec, so authored content is dropped silently.
- Consequence: a misplaced threshold, a mistyped field or a pasted credential returns success with no diagnostic, and the binding later runs with semantics the author did not write.
- Correction: make `TriggerActivation::ObservationsReady` a struct variant with no fields so the enum-level `deny_unknown_fields` applies (wire shape stays `{kind: observations_ready}` and `description` still decodes — both reproduced), and add `deny_unknown_fields` to `VerifierImplementation`. Preserve the flattened wire shape that keeps inline and referenced bodies identical (INV-013). Regenerate schemas; never hand-edit.
- Closure proof: new `wyrd-spec` unit tests for an unknown key on `observations_ready` (inline and as a Trigger Card spec), on `schedule`, and beside `implementation.kind`/`spec` (this last also serves as AC-004 secret-rejection evidence), via `mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::tests::<names>)'`, then `mise run codegen:check`.

### FIND-TASK-001-4 — the new binding registration paths have no executable proof
- Sources: TR-2, DS-1, TR-4 (loader portion). **REVISED**. **MISSING**.
- Obligation: AC-004, AC-018 (path/CardRef/inline forms, UID-bearing resolution, derived relationships, duplicate and fail-closed refusals), AC-022, Scenario 1/2/3 RED, AGENTS §11.
- Location: `crates/wyrd/wyrd-server/src/components/cards/resolve.rs:66-181` and `crates/wyrd-spec/src/graph/composition.rs:104-218` are untested; fixtures `wyrd-cli/tests/fixtures/loader/end_to_end/service.yaml:28-50` and `.../card_lifecycle/typed_state/typed-service.yaml` cover component bindings with path refs and one inline Trigger only.
- Evidence: five of the seven new stable codes appear nowhere but their definition, raise site and the generated TypeScript list. No fixture registers a Service-level binding, a standalone-Agent binding, a CardRef to an already-registered Trigger or Operator (the `EffectiveSpecs::load` registry branch), or an inline Operator. Old-kind refusal evidence is one `wyrd-spec` deserialization test.
- Consequence: the registry-backed refusals (referenced `workflow` Operator, referenced Trigger/implementation mismatch) and the external-ref UID pinning of Trigger/Operator slots would pass every current lane if they regressed.
- Correction (smallest sufficient set, reusing existing homes/helpers):
  1. Extend the existing CLI/registration journey fixture so one registration also carries a Service-level binding, a standalone-Agent binding, a Verifier + Trigger + Operator referenced by CardRef to previously registered Cards, and one inline Operator; assert persisted refs are UID-pinned and the Verifier/Trigger/Operator relationships derived.
  2. Add unit branches in the owning `wyrd-spec` module for the untested pure checks: wrong-kind `verifier`, wrong-kind `runs_on`, wrong-kind `on_failure`, duplicate Operator, inline `workflow` Operator.
  3. Add real-server cases to `crates/wyrd/wyrd-server/tests/pg_card_registration_route.rs` (reusing `seed_dependency` and `assert_no_registration_writes`) for the server-only branches: a referenced `workflow` Operator, a referenced Trigger whose activation does not match its Verifier, a binding ref seeded in another tenant, and an under-privileged caller. Each asserts its stable code and that nothing persisted.
  4. Add one loader-level refusal case for `kind: Drift` and `kind: Eval`.
  5. Add `("eval","runs")` and `("eval","assertions")` to the existing "removed table must not resolve" assertion list at `crates/vala/vala-bifrost-redux/src/tables/mod.rs:795-808`.
  Adjacent behaviour preserved: no new harness or fixture tree, no change to registration ordering or the pre-transaction rejection point.
- Closure proof: `mise run test:cards:integration`, `mise run test:cli:journey`, `mise run test:shared`, the owning `vala-bifrost-redux` lane, and each new named test via `mise exec -- cargo nextest run --locked -p <crate> --lib|--test <target> -E 'test(=<exact name>)'`.

### FIND-TASK-001-5 — public prose, generated schemas and error payloads still describe the retired model
- Sources: TR-5, SR-7, SR-8 (partial), data-review observation. **REVISED**. **INCORRECT**/**DRIFT**.
- Obligation: AC-021 (no generated public schema may advertise a second registrable Drift or Eval Card), REQ-090, REQ-103, REQ-114, REQ-116, AGENTS §16 (rustdoc must state the correct invariant).
- Locations/evidence:
  - `crates/wyrd-spec/src/vala/eval/spec.rs:27-31` — "The typed spec body for an `Eval` card … envelope-level `kind: Eval`", emitted verbatim into `crates/wyrd-spec/schemas/card.json:2631`, `schemas/verifier_spec.json:1784` (and mirrored `tests/schemas` fixtures, `docs/public/llms-full.txt`).
  - `crates/wyrd-spec/src/card/drift.rs:1,15` — "DriftCard spec body" → `card.json:2344`, `verifier_spec.json:1497`.
  - `crates/wyrd/wyrd-server/src/components/cards/service.rs:1342,1350` and `crates/shared/wyrd-loader/src/order.rs:134` — "no submitted publisher" + `publisher_kinds: ["Data","Model","Agent","Service"]`; `Data`/`Model` cannot own `verified_by`.
  - `crates/wyrd/wyrd-cli/src/error.rs:134-141` — `WYRD_CLI_400_NOT_EVAL_CARD` says "card file must contain kind: Eval" while `eval/run.rs:90-95` requires `kind: Verifier` with `implementation.kind: eval`.
  - `crates/wyrd-spec/src/card/trigger.rs:26-29` — attributes activation pairing to `crate::graph::composition`, which has no such check; enforcement is `resolve.rs:153-181`.
  - `crates/vala/vala-eval/src/executor.rs:86,119` — rustdoc still names `vala.eval.runs`, a table this change drops; the file is already touched here.
- Consequence: an agent reading the published schema, the registration error details or the CLI remediation is told to author `kind: Eval`, or that `Data`/`Model` may own a binding — the exact model this change rejects.
- Correction: rewrite those doc comments and message/detail texts to the Verifier + `verified_by` model (binding owners: Service, Service component, standalone Agent), rename the `publisher_kinds` detail key to the verifier-binding term at both raise sites, point the Trigger doc at server registration binding validation, regenerate. Do not change error codes, statuses or detail payload structure. Excluded: `EvalRecordObservation.eval_ref` (TASK-002 deletes that field).
- Closure proof: `mise run codegen:check`, `mise run docs:check`, `mise run test:cards:integration`, `mise run test:shared`.

### FIND-TASK-001-6 — the Eval pull-protocol contract outlives its retired route
- Sources: TR-6, SR-6. **CONFIRMED**. **MISSING** (incomplete retirement).
- Obligation: REQ-120 and Scenario 3; AGENTS §12; `architecture/wyrd-design.md` and `architecture/references/domain/evaluation.md` both state there is no Eval pull protocol.
- Location: `crates/wyrd-spec/src/vala/eval/protocol.rs:1-12,48-72`; `crates/wyrd-spec/src/vala/ids.rs:206-268` (`LeaseToken`); re-exports at `vala/eval/mod.rs:62,70`; round-trip test at `mod.rs:1222-1276`; `examples/gen_schemas.rs:71,240-241` and generated `tests/fixtures/eval/schemas/eval_run_open_{request,response}.schema.json`; dead CLI variants `SimulatedUserScriptRequired`, `ScriptedTurnMissing`, `ServerRequiresAgentUrl`, `ServerRequiresToken`, `ServerRejectsRecords`, `RecordsRequireSubject`, `AgentTurnFailed` in `crates/wyrd/wyrd-cli/src/error.rs`.
- Evidence: `git grep` shows those three types have no consumer outside their own module, that test, the generator and the generated fixtures; each of the seven CLI variants has zero constructors after `eval/server.rs`/`eval/agent.rs` deletion. Sibling orchestrator types are still consumed by `crates/vala/vala-eval/src/orchestrator/*`.
- Consequence: the retired protocol's lease request/response contract and stable CLI codes whose remediation names deleted flags (`--server`, `--agent-url`, `--simulated-user-script`) remain published.
- Correction: delete `EvalRunOpenRequest`, `EvalRunOpenResponse`, `LeaseToken`, their re-exports, their round-trip test, the two generator lines, the two generated fixture files, and the seven unreachable `WyrdCliError` variants; rewrite the `protocol.rs` module doc to describe only the local orchestrator turn types `vala-eval` still consumes. Preserve every type `vala-eval` uses and all remaining CLI codes. Regenerate; do not hand-edit artifacts.
- Closure proof: `mise run codegen:check`, `mise run docs:check`, `mise run test:shared`, `mise run lints`, and `git grep -n 'EvalRunOpen\|LeaseToken\|ServerRequiresAgentUrl' -- crates sdks` empty.

### FIND-TASK-001-7 — code references the task plan
- Sources: SR-2. **CONFIRMED**. **VIOLATION**.
- Obligation: agent-rules.md — never mention plans, tasks or other agents in the codebase.
- Location: `crates/wyrd-spec/src/graph/composition.rs:435-436`.
- Evidence: "The name is retained from the retired publication check because the task verification list names it".
- Consequence: an ephemeral planning artifact baked into permanent code.
- Correction: delete that sentence; keep the first doc line. Do **not** rename the test — its name is the task's recorded verification command.
- Closure proof: `git grep -n -i 'task verification' -- crates` empty; `mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=graph::composition::tests::publication_validation_rejects_duplicate_targets)'`.

### FIND-TASK-001-8 — materially rewritten fallible items lack rustdoc and `# Errors`
- Sources: SR-3 (revised). **REVISED**. **VIOLATION**.
- Obligation: AGENTS §16 (hard blocker).
- Location: `crates/wyrd-spec/src/card/drift.rs:440,459,490,505`; `crates/wyrd/wyrd-server/src/components/cards/resolve.rs:20-24`.
- Evidence: all four drift validators were rewritten here (`validate_condition` now returns the new `ConditionNotStatistical`; the method/signal/profile matrices lost variants) and carry no doc; `resolve_card_references` now additionally fails with `WYRD_SPEC_400_UNSUPPORTED_OPERATOR_ACTION` and `WYRD_SPEC_400_TRIGGER_ACTIVATION_MISMATCH` with a one-line doc and no `# Errors`.
- Consequence: merge-blocking documentation criterion fails; the public resolver's failure set is undocumented.
- Correction: add intent/invariant rustdoc plus `# Errors` to those five items only; `resolve_card_references`'s `# Errors` names its three classes (unresolved/path-only dependency, binding validation, registry read failure). Do not document untouched neighbours.
- Closure proof: `mise run lints`, `mise run fmt`.

### FIND-TASK-001-9 — the new binding workflow threads dependencies through free functions
- Sources: SR-4. **CONFIRMED**. **VIOLATION**.
- Obligation: AGENTS §5 Required Struct-Centered Rust Style; agent-rules.md:34.
- Location: `crates/wyrd/wyrd-server/src/components/cards/resolve.rs:66-118` and `:227-246`.
- Evidence: `validate_effective_bindings(conn, submissions, resolved)` is a new multi-step IO workflow re-passing `conn` and `resolved` into `EffectiveSpecs::load` on every lookup.
- Consequence: new server orchestration extends exactly the functional shape the rule forbids for new code.
- Correction: let the existing `EffectiveSpecs` own `resolved` (and the sibling cache it already holds) and expose the workflow as one inherent method taking `conn` and the submissions; `check_activation` stays a stateless helper. Apply together with FIND-2, which removes `owned_bindings` from the same function. Preserve the current rejection ordering and that it all runs before `write_registration` opens its transaction.
- Closure proof: `mise run test:cards:integration`, `mise run lints`, and no free function in `resolve.rs` taking both `conn` and `resolved`.

### FIND-TASK-001-10 — function-scoped `use` and fully qualified types in new signatures
- Sources: SR-5. **CONFIRMED**. **VIOLATION**.
- Obligation: agent-rules.md:9-10.
- Location: function-scoped `use` at `crates/wyrd-spec/src/card/verifier.rs:58,82`; fully qualified signature types at `verifier.rs:57,80,105,159`, `graph/composition.rs:140`, `refs/mod.rs:54,66,78,93,109,122`, `components/cards/resolve.rs:329-330`, `components/cards/service.rs:1157-1158`.
- Evidence: base `refs/mod.rs` had no fully qualified signature types (its `use` block at `:7-23` is the pattern); `resolve.rs` already imports `InlineableRef` at `:9` then spells it out at `:329`.
- Consequence: incomplete module dependency manifests; signatures hide type ownership.
- Correction: hoist those imports into each module's top-of-file `use` block (utoipa imports behind the existing `#[cfg(feature = "server")]` gate) and use bare names in the listed signatures. No behavioural change.
- Closure proof: `mise run fmt`, `mise run lints`, `mise exec -- cargo nextest run --locked -p wyrd-spec --lib`.

## 4. SPEC_REVISION_REQUIRED

None. FIND-2 is the only finding near a contract decision and is resolvable with the existing `WYRD_REGISTRY_400_INVALID_CARD_SPEC` refusal, so no new public error, permission, binding identity or persistent-data decision is required.

## 5. Rejected Wave 1 findings

- **TR-7**: sanctioned by agent-rules.md:20 with the required `// justification:` line and matching the unchanged `envelope.rs::Spec` precedent.
- **TR-4, cross-surface half**: owned by TASK-008 (AC-021 integrated closure); loader half retained in FIND-4.
- **TR-2, `/v1/eval/runs` 404 absence assertion**: route, component and `AppState` field are deleted; an absence test guards a property with no live owner, and TASK-008 owns "no stale old table/route/crate path". The built-in-table assertion is kept because an in-pattern assertion list already exists.
- **TR-5, `EvalRecordObservation.eval_ref` doc**: the field is removed by TASK-002.
- **TR-8 standalone**: folded into FIND-2 (same missing owner).
- **SR-3, trivial items**: `DriftMethod::name`, `DriftSignal::variant_name`, `vala-drift::signal_variant`, the `utoipa` trait impls — one-line matchers and trait-documented impls with no behaviour change.
- **SR-8, `docs/src/lib/components/CardTileGrid.svelte`**: zero callers — no `.svx` page uses `<CardTileGrid`; only re-exported by `docs/src/lib/mdsvex/components.js`, so its `Eval`/`Drift` tiles render nowhere.
- **SR-8, wyrd-ui mock `'publication'` labels**: dev-UI mock data; AGENTS §2 makes the UI explicitly not a source of truth.
- **SR-2, test renaming**: those names are the task's recorded verification commands.

## 6. Recommended outcome

**FIX_REQUIRED.** FIND-1 (broken capability lane), FIND-2 (fail-open nested binding), FIND-3 (silent unknown-field acceptance) and FIND-4 (no executable proof of the new registration behaviour) each block acceptance on their own.

Verification note for the remediation: `mise run lints`, `mise run test:wyrd`, `ts:typecheck` and `ts:test:unit` had not completed when the Wave 1 reports were written, and the orchestrator run log is no longer present in the tree, so those lanes must be re-run and recorded alongside `mise run test:bifrost`, which was never scheduled.
