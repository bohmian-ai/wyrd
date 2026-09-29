# TASK-001-R1 — Verifier binding closure (remediation)

Route to `$wyrd-implement`. This is a remediation task, not a new plan. Do not
re-plan TASK-001, do not broaden its approved behaviour, and do not pick up work
owned by TASK-002…TASK-008.

## 1. Subject

- Approved spec: `changes/active/verified-change-contract/spec.md` (revision 32).
- Original task: `changes/active/verified-change-contract/tasks/TASK-001-verifier-contract-and-registration.md`.
- Reviewed candidate: base `5293546f3` → `2e09ae81213cb608253b75de276e3c946353ac35`.
- Review round 1 verdict: `FIX_REQUIRED` (`changes/active/verified-change-contract/review/TASK-001-r1/verdict.md`).
- Validated ledger with full evidence: `.../review/TASK-001-r1/findings-validation.md` §3.

## 2. Issue diagnosis

### FIND-TASK-001-2 — nested inline Agent bindings are a fail-open trust hole (highest severity)

`AgentSpec.verified_by` is `#[serde(default)]`, so it decodes inside every
`InlineableRef<AgentSpec>` — including a Workflow step Agent
(`crates/wyrd-spec/src/refs/mod.rs:291`) and an Eval LLM-judge `judge_ref`
(`:345`). The canonical visitor (`:534-543`) then hands those binding slots to
`validate_and_collect_refs` (resolution), `bind_card_references` (UID pinning)
and `scope_child_card_refs` (outbound relationships).

Every validator that enforces binding rules matches only the *top-level* spec:
`crates/wyrd/wyrd-server/src/components/cards/service.rs:1188-1199`,
`.../resolve.rs:121-146`, `crates/wyrd-spec/src/graph/composition.rs:236-246`,
and `crates/shared/wyrd-loader/src/validate.rs:64-78` are four independent
re-enumerations of the same three legal binding locations, and none of them
descends into an inline Agent body.

Consequence: a caller holding ordinary `card:write` registers a Workflow (or an
Eval Verifier) whose inline Agent carries a `verified_by` naming a `Model` or
`Data` Card as `verifier`, a `workflow` Operator in `on_failure`, a duplicate
Verifier, or a Trigger whose activation does not match its Verifier. The
contract-violating binding is resolved, UID-pinned, persisted, and given
relationship edges — REQ-090's location restriction, REQ-092's fail-closed
requirement and REQ-094/REQ-143's Workflow refusal are all bypassed. The four
duplicated enumerations are why: there is no single owner that a new binding
location can be added to once.

The candidate's proof cannot catch this — no fixture or test registers a nested
inline Agent carrying `verified_by`.

### FIND-TASK-001-3 — unknown and secret-shaped fields are silently accepted

`deny_unknown_fields` was dropped from `TriggerSpec`
(`crates/wyrd-spec/src/card/trigger.rs:15-22`) to allow `#[serde(flatten)]`, and
`TriggerActivation::ObservationsReady` (`:43`) is a *unit* variant. Serde's
internally tagged unit-variant visitor drains and discards every remaining map
entry, so `{kind: observations_ready, verifier: …, api_token: sk-live-…}`
deserializes `Ok` while the struct variant `Schedule` still rejects extras.
`VerifierImplementation` (`card/verifier.rs:39-49`) carries no
`deny_unknown_fields` either, so a sibling key beside `kind`/`spec` is ignored.
Base `TriggerSpec` had `deny_unknown_fields`, so this is a strictness
regression.

The server re-serializes the typed spec, so the dropped content vanishes without
a diagnostic. AC-004 requires unknown fields and secret-bearing payloads to be
rejected *before* persistence; INV-006 forbids secrets in the Verifier surface.
Consequence: a misplaced threshold, a mistyped field, or a pasted credential
returns HTTP 201 and the binding later runs with semantics the author never
wrote.

### FIND-TASK-001-4 — the new registration behaviour has no executable proof

Five of the seven new stable codes (`SpecInvalidVerifierRefKind`,
`SpecInvalidBindingRefKind`, `SpecDuplicateBindingOperator`,
`SpecUnsupportedOperatorAction`, `SpecTriggerActivationMismatch`) appear only at
their definition, their raise site, and in the generated TypeScript list — no
test anywhere asserts them.

The journey fixtures
(`crates/wyrd/wyrd-cli/tests/fixtures/loader/end_to_end/service.yaml:28-50`,
`.../card_lifecycle/typed_state/typed-service.yaml`) bind only on *components*,
using path refs plus one inline Trigger. No fixture registers a Service-level
binding, a standalone-Agent binding, a `CardRef` to an already-registered
Trigger or Operator, or an inline Operator — so the registry-backed branch of
`EffectiveSpecs::load`
(`crates/wyrd/wyrd-server/src/components/cards/resolve.rs:227-246`) and the
activation/action checks at `:153-181` never execute in any lane. Old-kind
refusal evidence is a single `wyrd-spec` deserialization test; the loader path
has none.

AC-018 and AC-004 require the path/CardRef/inline forms, UID-bearing resolution,
derived relationships, duplicate rejection and fail-closed refusals to be proven.
Consequence: every server-only refusal could regress silently.

### FIND-TASK-001-1 — `test:bifrost` cannot build

`mise.toml:256` still passes `--test pg_eval_v1_protocol` and
`.github/scripts/detect-changes.sh:64` still names the file, but
`crates/wyrd/wyrd-server/tests/pg_eval_v1_protocol.rs` and its `[[test]]` entry
were deleted by this candidate. `mise run test:bifrost:integration:server`
(reached from `test:bifrost` via `scripts/run-bifrost-tests.sh:11`) fails with
"no test target named `pg_eval_v1_protocol`", so the Bifrost capability gate and
`gate` are both broken and the lane was never run for this task. AC-022 forbids
leftover build references to retired paths.

### FIND-TASK-001-5 — published contracts still advertise the retired model

Doc comments that feed generated artifacts still describe the pre-change world:
`crates/wyrd-spec/src/vala/eval/spec.rs:27-31` ("envelope-level `kind: Eval`")
is emitted verbatim into `crates/wyrd-spec/schemas/card.json:2631` and
`schemas/verifier_spec.json:1784`; `card/drift.rs:1,15` ("DriftCard spec body")
into `card.json:2344` and `verifier_spec.json:1497`. Error payloads at
`crates/wyrd/wyrd-server/src/components/cards/service.rs:1342,1350` and
`crates/shared/wyrd-loader/src/order.rs:134` emit "no submitted publisher" with
`publisher_kinds: ["Data","Model","Agent","Service"]`, though REQ-090 forbids
`Data`/`Model` from owning a binding. `crates/wyrd/wyrd-cli/src/error.rs:134-141`
tells the caller the file "must contain kind: Eval" while
`eval/run.rs:90-95` requires `kind: Verifier` with `implementation.kind: eval`.
`card/trigger.rs:26-29` attributes activation pairing to
`crate::graph::composition`, which has no such check (enforcement is
`resolve.rs:153-181`). `crates/vala/vala-eval/src/executor.rs:86,119` still
names `vala.eval.runs`, a table this change drops.

Consequence: AC-021 is violated at exactly the surface that matters — an agent
reading the published schema, a registration error, or the CLI remediation is
instructed to author the model this change rejects.

### FIND-TASK-001-6 — the Eval pull protocol outlives its route

REQ-120 retired `/v1/eval/runs`, and both `architecture/wyrd-design.md` and
`architecture/references/domain/evaluation.md` now state there is no Eval pull
protocol. But `EvalRunOpenRequest`/`EvalRunOpenResponse`
(`crates/wyrd-spec/src/vala/eval/protocol.rs:48-72`) and `LeaseToken`
(`crates/wyrd-spec/src/vala/ids.rs:206-268`) survive with zero consumers outside
their own module, their round-trip test (`vala/eval/mod.rs:1222-1276`),
`examples/gen_schemas.rs:71,240-241` and the two generated fixture schemas. Seven
`WyrdCliError` variants (`SimulatedUserScriptRequired`, `ScriptedTurnMissing`,
`ServerRequiresAgentUrl`, `ServerRequiresToken`, `ServerRejectsRecords`,
`RecordsRequireSubject`, `AgentTurnFailed`) have no constructor left after
`eval/server.rs` and `eval/agent.rs` were deleted, yet still publish remediation
naming deleted flags (`--server`, `--agent-url`, `--simulated-user-script`).

Consequence: a retired protocol's lease contract and stale stable error codes
remain part of the published surface.

### FIND-TASK-001-7 / -8 / -9 / -10 — repository-rule violations in new code

- `crates/wyrd-spec/src/graph/composition.rs:435-436` bakes a planning artifact
  into permanent code ("the task verification list names it"), which
  `architecture/agent-rules.md` prohibits outright.
- `crates/wyrd-spec/src/card/drift.rs:440,459,490,505` (all four validators were
  rewritten here; `validate_condition` returns the new `ConditionNotStatistical`)
  and `resolve.rs:20-24` `resolve_card_references` (failure set grew by
  `WYRD_SPEC_400_UNSUPPORTED_OPERATOR_ACTION` and
  `WYRD_SPEC_400_TRIGGER_ACTIVATION_MISMATCH`) lack rustdoc and `# Errors`,
  which AGENTS §16 makes a hard blocker.
- `resolve.rs:66-118` `validate_effective_bindings(conn, submissions, resolved)`
  is a brand-new multi-step IO workflow that re-threads `conn` and `resolved`
  into `EffectiveSpecs::load` per lookup — the functional shape AGENTS §5 and
  `agent-rules.md:34` forbid for new code, while `EffectiveSpecs` already exists
  as the natural owner.
- Function-scoped `use` at `card/verifier.rs:58,82` and fully qualified types in
  new signatures at `verifier.rs:57,80,105,159`, `composition.rs:140`,
  `refs/mod.rs:54,66,78,93,109,122`, `resolve.rs:329-330` (which already imports
  `InlineableRef` at `:9`) and `service.rs:1157-1158` violate
  `agent-rules.md:9-10`; base `refs/mod.rs` had none.

## 3. Intended outcome

TASK-001's approved behaviour holds on every reachable path, is proven by
executable tests at the tier that owns it, the retired Eval/Drift/publication
model is absent from every published contract and build file, and the new code
obeys the repository's Rust standards.

## 4. Decision-complete recommendation

**FIND-2 (root cause, do this first).** Introduce exactly one owner in
`wyrd-spec` that enumerates every `verified_by` list reachable in a submitted
spec together with its field path, and make it the single source consumed by all
four current call sites (`service.rs:1188-1199`, `resolve.rs:121-146`,
`composition.rs:236-246`, `wyrd-loader/src/validate.rs:64-78`). Top-level
locations keep running the existing `binding_validation_errors` unchanged; any
binding list reached *through* an inline Agent body is refused. Reuse the
existing `WYRD_REGISTRY_400_INVALID_CARD_SPEC` refusal — do **not** mint a new
catalog code, which would be a public-contract decision outside this
remediation's scope. Delete `resolve.rs::owned_bindings` in favour of the shared
owner; that removes the four-way duplication in the same edit.
*Rejected alternative:* teaching each of the four validators to descend
independently — it recreates the drift that caused the defect.
Preserve unchanged: the canonical `ReferenceSlotVisitor` walk, UID pinning,
relationship derivation, rejection-before-transaction ordering, and the existing
top-level error codes and `details` payload shapes.

**FIND-3.** Make `TriggerActivation::ObservationsReady` a *struct* variant with
no fields so the enum-level `deny_unknown_fields` applies, and add
`deny_unknown_fields` to `VerifierImplementation`. Both were reproduced against
the workspace serde/serde_yaml: the wire shape stays `{kind: observations_ready}`,
`description` still decodes, extras are rejected, and a sibling key beside
`implementation.kind`/`spec` is rejected. *Rejected alternative:* a manual
`Deserialize` impl or a post-parse unknown-key scan — more code for the same
result. Preserve the flattened wire shape that keeps inline and referenced
bodies identical (INV-013). Regenerate schemas; never hand-edit them.

**FIND-9 (apply with FIND-2 — same function).** Let the existing `EffectiveSpecs`
own `resolved` (alongside the cache it already holds) and expose the workflow as
one inherent method taking `conn` and the submissions. `check_activation` stays
a stateless helper. Preserve the current rejection ordering and the fact that
all of it runs before `write_registration` opens its transaction.

**FIND-4.** Add proof at the tier that owns each behaviour, reusing existing
homes — no new harness, fixture tree, or dependency:
1. Extend the existing CLI/registration journey fixture so one registration also
   carries a Service-level binding, a standalone-Agent binding, a Verifier +
   Trigger + Operator referenced by `CardRef` to previously registered Cards,
   and one inline Operator; assert persisted refs are UID-pinned and the
   Verifier/Trigger/Operator relationships are derived.
2. Add unit branches in the owning `wyrd-spec` module for the untested pure
   checks: wrong-kind `verifier`, wrong-kind `runs_on`, wrong-kind `on_failure`,
   duplicate Operator, inline `workflow` Operator.
3. Add real-server cases to
   `crates/wyrd/wyrd-server/tests/pg_card_registration_route.rs`, reusing
   `seed_dependency` and `assert_no_registration_writes`, for the server-only
   branches: a referenced `workflow` Operator, a referenced Trigger whose
   activation does not match its Verifier, a binding ref seeded in another
   tenant, and an under-privileged caller. Each asserts its stable code and that
   nothing persisted.
4. Add one loader-level refusal case for `kind: Drift` and `kind: Eval`.
5. Add `("eval","runs")` and `("eval","assertions")` to the existing
   "removed table must not resolve" assertion list at
   `crates/vala/vala-bifrost-redux/src/tables/mod.rs:795-808`.

**FIND-1.** Remove `--test pg_eval_v1_protocol` from the existing
`test:bifrost:integration:server:inner` command and the `pg_eval_v1_protocol|`
alternative from the bifrost path regex in `detect-changes.sh`. The remaining
four targets keep their order and `--test-threads=1`.

**FIND-5.** Rewrite the listed doc comments and message/detail texts to the
Verifier + `verified_by` model (binding owners: Service, Service component,
standalone Agent), rename the `publisher_kinds` detail key to the
verifier-binding term at both raise sites, and point the Trigger doc at server
registration binding validation. Do **not** change error codes, statuses, or
detail payload structure. Then regenerate. Excluded: `EvalRecordObservation.eval_ref`
(TASK-002 deletes that field).

**FIND-6.** Delete `EvalRunOpenRequest`, `EvalRunOpenResponse`, `LeaseToken`,
their re-exports (`vala/eval/mod.rs:62,70`), their round-trip test, the two
`gen_schemas.rs` lines, the two generated fixture schema files, and the seven
unreachable `WyrdCliError` variants; rewrite the `protocol.rs` module doc to
describe only the local orchestrator turn types `vala-eval` still consumes.
Preserve every sibling type `crates/vala/vala-eval/src/orchestrator/*` still
uses (`TurnDirective`, `SimulatedUserMode`, `AgentTurnSubmission`,
`ConversationTurn`, `SimulatedUserTurn`, `MAX_HISTORY_TURNS`) and all remaining
CLI codes. Regenerate; do not hand-edit artifacts.

**FIND-7.** Delete the plan-referencing sentence at `composition.rs:435-436`;
keep the first doc line. Do **not** rename the test — its name is the task's
recorded verification command.

**FIND-8.** Add intent/invariant rustdoc plus `# Errors` to those five items
only. `resolve_card_references`'s `# Errors` names its three classes
(unresolved/path-only dependency, binding validation, registry read failure). Do
not document untouched neighbours.

**FIND-10.** Hoist the listed imports into each module's top-of-file `use` block
(utoipa imports behind the existing `#[cfg(feature = "server")]` gate) and use
bare names in the listed signatures. No behavioural change.

## 5. Constraints, preserved behaviour, non-goals

- Stay inside spec revision 32. No new public error code, permission, binding
  identity, persistent-data, or cross-service decision — if one appears
  unavoidable, stop and report `SPEC_REVISION_REQUIRED` rather than inventing it.
- Do not weaken or disable any check, add `#[allow]`, or `#[ignore]`/delete a
  test to make a lane pass.
- Preserve: the single canonical `ReferenceSlotVisitor`; UID pinning and
  relationship derivation semantics; pre-transaction rejection ordering; all
  existing stable codes, statuses and `details` payload shapes; the flattened
  Trigger/Operator wire shape (INV-013); `vala-drift`/`vala-eval` engine reuse.
- The `#[allow(clippy::large_enum_variant)]` at `card/verifier.rs:41-43` is
  sanctioned (`agent-rules.md:20`, matching the `envelope.rs::Spec` precedent) —
  leave it.
- Non-goals: the HTTP/CLI/SDK/MCP cross-surface half of AC-021 (TASK-008); any
  `/v1/eval/runs` 404 absence test (the route has no live owner);
  `EvalRecordObservation.eval_ref` (TASK-002); rustdoc on untouched or trivial
  items (`DriftMethod::name`, `DriftSignal::variant_name`,
  `vala-drift::signal_variant`, the `utoipa` impls);
  `docs/src/lib/components/CardTileGrid.svelte` (zero callers) and wyrd-ui mock
  labels; renaming any existing test; any projection, runtime, or executor work.

## 6. Acceptance criteria

| # | Criterion | Finding |
|---|---|---|
| AC-R1 | `mise run test:bifrost:integration:server` builds and exits 0; `git grep -n pg_eval_v1_protocol -- mise.toml .github scripts` is empty. | FIND-1 |
| AC-R2 | A submission whose inline Agent (Workflow step or Eval judge) carries `verified_by` is refused with the existing stable 400 before any write; one owner enumerates binding locations and `resolve.rs::owned_bindings` is gone. | FIND-2 |
| AC-R3 | An unknown key on `observations_ready` (inline and as a Trigger Card spec), on `schedule`, and beside `implementation.kind`/`spec` is rejected at deserialization; `{kind: observations_ready}` and `description` still decode. | FIND-3 |
| AC-R4 | Every new stable binding code has at least one asserting test; the journey covers Service-level, standalone-Agent, CardRef-to-registered Trigger/Operator, and inline-Operator forms with UID-pinned refs and derived relationships; server-only refusals (referenced `workflow` Operator, activation mismatch, cross-tenant ref, under-privileged caller) each assert their code and no writes; loader refuses `kind: Drift` and `kind: Eval`; `eval.runs`/`eval.assertions` are in the removed-table assertion list. | FIND-4 |
| AC-R5 | No generated schema, error payload, or CLI remediation refers to `kind: Eval`, "DriftCard", `publisher_kinds`, publisher vocabulary, or `vala.eval.runs`; codes, statuses and payload structure unchanged; `codegen:check` and `docs:check` clean. | FIND-5 |
| AC-R6 | `git grep -n 'EvalRunOpen\|LeaseToken\|ServerRequiresAgentUrl' -- crates sdks` is empty; `vala-eval` still compiles against the retained orchestrator types; generated artifacts regenerate cleanly. | FIND-6 |
| AC-R7 | `git grep -n -i 'task verification' -- crates` is empty and the test name is unchanged. | FIND-7 |
| AC-R8 | The five named items carry intent rustdoc and `# Errors`. | FIND-8 |
| AC-R9 | No free function in `resolve.rs` takes both `conn` and `resolved`; the workflow is an inherent method on `EffectiveSpecs`; rejection ordering unchanged. | FIND-9 |
| AC-R10 | No function-scoped `use` and no fully qualified types in the listed signatures; imports are top-of-module. | FIND-10 |

## 7. Verification

Focused proof (run each named test explicitly):

```bash
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=<exact name>)'
mise exec -- cargo nextest run --locked -p wyrd-server --test pg_card_registration_route -E 'test(=<exact name>)'
```

Lanes, all of which must be run and recorded:

```bash
mise run fmt
mise run lints
mise run codegen:check
mise run docs:check
mise run test:shared
mise run test:cards:unit
mise run test:cards:integration
mise run test:cli:journey
mise run test:wyrdstate:journey
mise run test:sql
mise run test:bifrost
mise run test:wyrd
mise run check:client-tier
mise run check:pyo3-scope
mise run py:test:unit
mise run py:typecheck
```

Plus the TypeScript lanes (`ts:typecheck`, `ts:test:unit`) and the owning
`vala-bifrost-redux` lane. `lints`, `test:wyrd`, `ts:typecheck`, `ts:test:unit`
never completed for round 1 and `test:bifrost` was never run — record their
actual results, do not carry forward the previous claim.

## 8. Implementation evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| AC-R1 | `mise.toml` `test:bifrost:integration:server:inner` and `.github/scripts/detect-changes.sh` no longer name `pg_eval_v1_protocol`; `crates/wyrd/wyrd-server/tests/pg_grpc_ingest_smoke.rs::ingest_valid_token_is_not_rejected_as_unauthenticated` now seeds its tenant (the revocation check resolves tenant admission, so an unseeded tenant invalidated its own token) | `mise run test:bifrost:integration:server` → 58/58, exit 0; `mise run test:bifrost` → 9/9 lanes, exit 0; `git grep -n pg_eval_v1_protocol -- mise.toml .github scripts` empty | PASS |
| AC-R2 | `crates/wyrd-spec/src/graph/composition.rs`: `BindingSite`, `Spec::binding_sites`, `spec_binding_errors`, `nested_binding_error` (reuses `WYRD_REGISTRY_400_INVALID_CARD_SPEC`); consumed by `composition::validate_composition`, `wyrd-loader/src/validate.rs::validate_card`, `wyrd-server/.../cards/service.rs::validate_request`, `.../cards/resolve.rs::EffectiveSpecs::validate_bindings`; `verification_bindings` and `owned_bindings` deleted | `mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=graph::composition::tests::spec_binding_errors_refuse_a_workflow_step_inline_agent_binding)'`, `…spec_binding_errors_refuse_an_eval_judge_inline_agent_binding`, `…spec_binding_errors_accept_an_inline_agent_without_bindings`; `mise run test:shared` 664 passed; `git grep owned_bindings -- crates` empty | PASS |
| AC-R3 | `crates/wyrd-spec/src/card/trigger.rs`: `TriggerActivation::ObservationsReady {}` is a fieldless struct variant so the enum `deny_unknown_fields` applies; `crates/wyrd-spec/src/card/verifier.rs`: `deny_unknown_fields` on `VerifierImplementation` | `mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::trigger::tests::observations_ready_decodes_with_optional_description)'` and `…observations_ready_rejects_unknown_fields`, `…schedule_rejects_unknown_fields`, `…trigger_card_spec_rejects_unknown_fields`; `mise run codegen:check` clean | PASS |
| AC-R4 | 5 pure-check unit branches in `graph/composition.rs`; `pg_card_registration_route.rs::referenced_binding_refusals_leave_no_writes` (+ `seed_card`/`bound_agent_request` helpers); loader `parse.rs::parse_rejects_retired_drift_and_eval_card_kinds`; journey fixtures `end_to_end_prerequisites/{retention-guardrail-verifier,retention-observations-trigger,retention-pager-operator}.yaml`, Service-level binding in `end_to_end/service.yaml`, standalone-Agent binding in `end_to_end/agent2/agent.yaml`; `card_lifecycle.rs` asserts UID-pinned refs and derived Verifier/Trigger edges; `("eval","runs")`/`("eval","assertions")` added in `vala-bifrost-redux/src/tables/mod.rs` | `mise run test:cards:unit` 4 passed; `mise run test:cards:integration` 24 passed; `mise run test:cli:journey` 23 passed / 5 ignored; `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=tables::tests::canonical_otel_registry_has_no_child_or_genai_tables)'` 1 passed | PASS |
| AC-R5 | Verifier-model prose in `card/drift.rs`, `vala/eval/spec.rs`, `error.rs`, `ids.rs`, `vala-drift/src/lib.rs`, `wyrd-cli/src/error.rs`, `vala-eval/src/executor.rs`; detail key `publisher_kinds` → `binding_owner_kinds: ["Service","Agent"]` in `cards/service.rs` and `wyrd-loader/src/order.rs`; `docs/scripts/generate_llms_txt.py` swaps drift/eval entries for `verifier` | `git grep -n 'publisher_kinds\|DriftCard\|vala.eval.runs' -- crates docs sdks` empty; `mise run codegen:check` and `mise run docs:check` clean | PASS |
| AC-R6 | `EvalRunOpenRequest`, `EvalRunOpenResponse`, `LeaseToken`, their re-exports, round-trip tests, the two `gen_schemas.rs` lines, the two generated fixture schemas, and the 7 unreachable `WyrdCliError` variants deleted; `protocol.rs` module doc rewritten | `git grep -n 'EvalRunOpen\|LeaseToken\|ServerRequiresAgentUrl' -- crates sdks` empty; `mise run codegen:check` clean; `mise run lints` exit 0 | PASS |
| AC-R7 | Plan-referencing sentence removed from `graph/composition.rs`; `publication_validation_rejects_duplicate_targets` name unchanged | `git grep -n -i 'task verification' -- crates` empty | PASS |
| AC-R8 | Intent rustdoc + `# Errors` on `drift.rs::{validate_signal_method,validate_signal,validate_condition,validate_profile_presence}` and `resolve.rs::resolve_card_references` | `mise run lints` exit 0; `mise run test:shared` 664 passed | PASS |
| AC-R9 | `EffectiveSpecs` owns `resolved: ResolvedRefs` with inherent `new`/`validate_bindings`/`validate_binding`/`load`; free `validate_effective_bindings` deleted; `check_activation` stays stateless; rejection still precedes `write_registration` | `git grep validate_effective_bindings -- crates` empty; `mise run test:cards:integration` 24 passed; `mise run check:client-tier`, `mise run check:pyo3-scope` exit 0 | PASS |
| AC-R10 | utoipa/`Cow`/`DriftValidationError` imports hoisted behind `#[cfg(feature = "server")]` in `card/verifier.rs`; bare names in `verifier.rs`, `composition.rs`, `refs/mod.rs`, `resolve.rs`, `service.rs` signatures | `mise run fmt`, `mise run lints` exit 0 | PASS |

### Lanes run in this session (all exit 0)

`fmt`, `lints`, `codegen:check`, `docs:check`, `check:client-tier`, `check:pyo3-scope`,
`test:shared` (664), `test:cards:unit` (4), `test:cards:integration` (24),
`test:cli:journey` (23 passed / 5 ignored), `test:wyrdstate:journey` (1), `test:sql` (2),
`test:bifrost` (9/9 lanes), `test:wyrd` (1975), `py:test:unit` (463), `py:typecheck`,
`ts:typecheck`, `ts:test:unit` (12), the `vala-bifrost-redux` removed-table test (1),
`git diff --check`.

### Material notes

- `test:bifrost` failed on its first complete run for a cause that predates this
  task: commit `7798918b6` (an ancestor of base `5293546f3`) added a
  `tenant_admits_credentials` gate to the revocation path, and
  `ingest_valid_token_is_not_rejected_as_unauthenticated` minted a token for a
  tenant it never seeded, so its own token read as revoked. The test now seeds
  its tenant through the existing `seed_tenant` helper, matching every sibling
  test in that file. No production code was changed for it.
- Two lanes flaked once each under disk pressure and passed on rerun:
  `bifrost::cluster::tests::cluster_restart_rederives_same_plan_from_retained_snapshot`
  (asserts two pool identities differ; allocator reused the addresses) and
  `oracle::distributed::pg_bifrost_selective_predicate_and_projection_prune_distributed_reads`
  ("Oracle runtime did not settle"). Both are pre-existing non-determinism
  unrelated to this change and were left untouched. The final `test:bifrost`
  aggregate ran 9/9 lanes, exit 0.
- Non-goals stayed excluded: no cross-surface AC-021 work, no `/v1/eval/runs`
  404 test, no `EvalRecordObservation.eval_ref` change, no renamed tests, no new
  error code, permission, or persistent-data decision. Spec revision 32 holds.
