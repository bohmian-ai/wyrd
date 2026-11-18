---
id: TASK-001
kind: implementation
status: implemented
spec: SPEC-verdict-response-mapping
spec_revision: 1
requirements: [REQ-001, REQ-002, REQ-003, REQ-005, REQ-006, REQ-007, REQ-008, REQ-010, REQ-011, REQ-012, REQ-013, REQ-014]
depends_on: []
---

# Author and execute Jev verdict checks

## Outcome and scope

An author uses the real Prompt class and typed question/transform values to
declare a Jev Agent check, catches locally knowable mistakes before declaring,
then receives and later inspects its canonical verdict. Deliver all three leaf
policies and their Rust/Python/TypeScript authoring, errors, evidence and
agent-facing projections together. Start with the smallest Choice journey;
extend it with probability and Score behavior in the ordered scenarios below.

This task supplies TASK-002 with a working native-response conversion and
recording path. It supplies TASK-003 with a concrete provider-dispatched facade,
shared question authoring and leaf policies. Flat AllTransform behavior belongs
to TASK-002; Google-specific lowering, transport and native inputs belong to
TASK-003. Follow the [approved specification](../spec.md),
[connected implementation flow](../design/notes.md#connected-implementation-flow),
[SDK interfaces](../design/sdk-interfaces.md), and
[question models](../design/question-models.md). The specification defines the
promises; the following entries define their executable proof.

## Existing owners and connected flow

Existing public `wyrd-client::authoring` builds and validates Card graphs through
the loader; `crates/wyrd-spec/src/card/verifier/{mod,validate}.rs` owns check
contracts and currently demands a Verdict response schema. Its AgentCall is
shared with actions. Introduce the agreed check-specific wrapper reusing and
flattening AgentCall, carrying the optional policy through authored/compiled
forms and reference traversal; keep actions unchanged. Extend the shared
validation path, including assembled local-graph validation, rather than
validating only at runtime or building an SDK-owned validator.

The existing `skald-prompt/src/builder.rs` constructs provider-shaped Prompts;
extend that owner for Jev and shared typed question lowering. Jev already has
typed wire requests/responses, safe state rendering, a native provider client
and gateway support. Do not rebuild these. New Python entry points belong in
`sdks/wyrd-sdk-python/src/prompt.rs` and its established registration boundary,
not new PyO3 in Skald. Existing Prompt registration already adds SDK-owned
methods to shared classes. Python Pydantic imports remain optional and direct
model coercion uses Rust behavior. TypeScript projects through
`native/src/authoring.rs` and `wyrd/src/definitions.ts`; Rust uses client/SDK
re-exports. Include exports, authored stubs, generated projections and public
examples/documentation for this delivered leaf workflow.

Input: a real configured Jev deployment, authored Prompt/Agent/Verifier graph
and execution input from the public SDK. Existing input binding sends actual
top-level state/questions/model. Skald's AgentRun retains final_response, but
`vala-verify/src/blocks/agent.rs::invoke` currently drops it and the check
executor insists on structured Verdict output. Preserve the existing shared
typed response there and apply the concrete Vala VerdictTransformer's leaf
policies. Output: existing CheckOutput and normal engine result; its actual
capture and provenance flow into the existing asynchronous recording/read path.
No second invocation, serialize/reparse adapter, new detail/table or handler
write. Correct the touched Jev Score scale comment to match its native levels.

Paths guide ownership, not an edit allowlist. Preserve the implementation and
performance constraints in the spec and [packet verification guidance](README.md).

Before extending Prompt/graph authoring, preserve the allocation practices in
[the spec](../spec.md#sdk-and-server-allocation-review): shared immutable
CardDefinition captures in wyrd-client authoring, lazy authored Prompt/reference
derivation in skald-agent, and shared native ProviderRequest storage/views at
language boundaries. Reuse the relevant current owner; these examples are not a
new universal Arc wrapper or a requirement to change untouched providers. New
question/transform coercion and check attachment must not eagerly duplicate the
Prompt or captured graph. Include ownership/copy review and any needed matched
allocation measurements in scenarios 2 and 4, alongside their existing snapshot,
read-back and validation proof. Follow the packet guidance for measurement
triggers and limitations; functional journeys alone cannot prove allocation cost.

## Ordered acceptance and implementation evidence

For every entry, record assertion locations, actual focused commands, observed
RED failure, GREEN result and material limitations in that entry. The expected
behavior stays fixed. Tests for unchanged behavior may already pass; record
that proof rather than manufacturing a RED result.

1. **One Jev Choice becomes a verdict** — REQ-001, REQ-003, REQ-005–006,
   REQ-012; J1. From a normal Jev deployment and a public Rust-authored Prompt
   with a canonical Choice question, declare an Agent/Verifier and verify a
   support answer through SDK → WyrdTestServer/Postgres → gateway → native mock.
   The mock records the bound literal state, declared question and resolved
   model at `/v1/systemone`; its selected `passed` answer reaches the requirement
   and execution as passed, with factual reason and empty citations. There is
   one upstream invocation and no caller parser or response schema. Add failed
   and inconclusive replies using the same assertion shape. The initial RED is
   the absent authoring/transform path or existing non-Verdict refusal, not a
   disconnected decoder assertion. This is a tier-1 Rust SDK journey.

   Evidence:
   - Journey: `sdks/wyrd-sdk-rust/tests/integration/gateway_jev.rs::jev_choice_check_becomes_the_verdict`.
     It covers `Prompt::jev`, `ChoiceQuestion`, `Check::agent(..).verdict_transform(VerdictTransform::Choice(..))`,
     the `jev` deployment and the wiremock `/v1/systemone` mock. It asserts passed, failed and
     inconclusive requirement and execution verdicts, the exact reason, empty citations, one upstream
     call per verification, and the exact upstream body (literal quoted/newline state, lowered question,
     `jev-latest` model).
   - Command: `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=gateway_jev::jev_choice_check_becomes_the_verdict)'"`
     (after `mise run build:server`).
   - RED 1 (missing authoring path): compile errors `unresolved imports wyrd_ai::verify::{ChoiceQuestion,
     ChoiceTransform, JevPromptOptions, VerdictTransform}` and `no method named verdict_transform`.
   - RED 2 (existing non-Verdict refusal): requirement `Inconclusive` with reason
     "the check Agent returned no structured Verdict". Declaration accepted the sibling Agent because
     validation resolves only inline Agents; scenario 4 owns that gap.
   - GREEN: `1 test run: 1 passed`.
   - Owner tests: `skald-prompt builder::builder_tests::{jev_typed_questions_lower_to_native_questions,
     jev_prompt_refuses_documented_bounds}` and `vala-verify blocks::transform::tests::*` (4) pass.
     Run with `mise exec -- cargo nextest run --locked -p <crate> --lib -E 'test(/.../)'`.
     Non-Postgres suites of wyrd-spec, vala-verify, skald-prompt, skald-spec and wyrd-client: 1366 passed.
     Clippy `-D warnings` is clean on the touched crates.
   - Reused seam tests pass. The task's first selector names a stale module; the test lives at
     `prompt::prompt_render::jev_state_renders_quotes_and_newlines_verbatim`.
   - Allocation: Vala shares the run's existing `Arc<ProviderResponse>` and serializes only the one
     selected answer for `actual`. `Prompt::jev` validates Jev bounds once at construction through the
     gateway's borrowed view. That costs one `to_value` of the authored request, at authoring time only.

2. **Public typed authoring survives declaration** — REQ-001–002, REQ-005,
   REQ-013; J1/J8. Repeat the connected Choice workflow through public Python
   and TypeScript imports, with direct Pydantic models and actual TypeScript
   question objects. Compare otherwise equivalent typed/native/mixed questions
   and policies through declaration/read-back/reconstruction/redeclaration.
   Include all three question shapes, omitted and supplied Noul criteria,
   exact Jev criteria lowering, unchanged Choice labels and Score order. The
   stored native form and semantics agree; no discriminator/criteria is lost,
   no manual model_dump is needed, and dictionary-only Python works without
   its optional extra. Type-only Rust/TS/Python assertions belong in compile/type
   fixtures. Ordinary Agent action construction accepts these questions without
   policy and preserves its existing output contract. Initial RED is missing
   public exports/coercion/constructors or lost canonical fields. Tier-1 SDK
   journeys are supported by owner construction and language-boundary tests.

   Evidence:
   - Python journey: `sdks/wyrd-sdk-python/tests/integration/test_gateway_jev.py::test_typed_jev_choice_check_becomes_the_verdict`.
     Typed (`wyrd.prompt.decisions`), native and mixed questions give equal `model_dump()` with body
     `{state, questions}` in the exact native form (Noul criteria lowered to `{true, false}`, omitted Noul
     criteria stay absent, Choice labels and Score order unchanged). The typed `ChoiceTransform` model and
     a native dict declare the same Card refs; the stored Verifier check holds the canonical
     `{"kind": "choice", "question": "refund_policy"}`; every stored Prompt spec equals the authored one.
     Each of the three replies gives the expected requirement and execution verdict, exact reason, empty
     citations, and one upstream `/v1/systemone` call with the exact native body.
   - Python command: `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -p no:cacheprovider tests/integration/test_gateway_jev.py -k becomes_the_verdict"`
     (after `uv run maturin develop --features testing`).
   - Python RED: `ModuleNotFoundError: No module named 'wyrd.prompt.decisions'`. GREEN: `1 passed, 4 deselected`.
   - Python boundary: `tests/unit/test_public_exports.py::test_decision_authoring_from_dictionaries_needs_no_pydantic`
     builds `Prompt.jev`, an ordinary `Action.agent` and a dict `verdict_transform` with Pydantic blocked,
     and the transform models refuse with `ImportError`. Unit exports: 33 passed. `py:format`, `py:lints`
     (ruff and stub check) and `py:typecheck` (ty) pass.
   - TypeScript journey: `sdks/wyrd-sdk-ts/wyrd/tests/integration/gateway-jev.test.ts` "typed jev choice check
     becomes the verdict". Same workflow with `new verify.ChoiceQuestion/NoulQuestion/ScoreQuestion`, native
     literals and a mix; the `verify.ChoiceTransform` literal is type-checked by `ts:typecheck`. Redeclaring
     native and mixed forms gives the same Card refs; read-back asserts the stored transform and Prompt body.
   - TypeScript command: `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/gateway-jev.test.ts -t 'typed jev choice'"`
     (after `mise run ts:build && mise run ts:build:testing`).
   - TypeScript RED: `TypeError: ChoiceQuestion is not a constructor`. Second failure (diagnosed): an omitted
     `NoulQuestion.criteria` was an own `undefined` property under define semantics and the JSON snapshot refused
     it; the field is now `declare`d. GREEN: `1 passed | 4 skipped`; the whole file: 5 passed.
   - TypeScript checks: `ts:format`, `ts:lints`, `ts:typecheck`, `codegen:check` pass; `ts:napi:check` passes
     with the regenerated `index.d.ts` committed. `cargo clippy -p wyrd-sdk-ts --all-targets -D warnings` clean.
   - Typed versus native is decided by language type (Python `model_dump_json`, TypeScript `instanceof`);
     a raw object is never translated. Lowering stays in Rust (`DecisionQuestion` → `JevQuestion`).
   - Allocation: one JSON conversion per question and per transform, at authoring time only.
   - Limitation: reconstruction from a saved bundle (`WyrdState.fromPath`) is not exercised here; redeclaration
     and read-back are.

3. **Acceptance policy controls the answer** — REQ-006–008, REQ-010,
   REQ-012–013; J3/J5. Extend the hosted Jev workflow with custom Choice
   labels, Noul for both expected polarities, and Score on three ordered levels.
   Exercise compliant/violation/unknown mapping; exact inclusive probability
   bounds and an intermediate value; expected-false 0.02/0.98/0.5; and
   `greater_than_or_equals: 2` passing at 2 and failing at 1.8. Reasons identify
   observed answer/policy and confidence never overrides it. Native answers
   and probabilities reach actual capture. Exhaust boundary arithmetic in
   Vala pure tests; representative positive/edge/negative cases must traverse
   the hosted consumer path in each SDK's shipped boundary. Initial RED is
   missing leaf dispatch or wrong interpretation, not unsupported fixture data.

   Evidence:
   - Journeys, one per SDK, on the same `policy-desk` workflow: a `desk-judge` Jev Agent asks a custom Choice
     (`compliant`/`violation`/`unknown`), two Nouls and a three-level Score, and `desk-policy` reads one
     answer through a Choice map, Probability expected true (0.75/0.25), Probability expected false (0.9/0.1)
     and Score `greater_than_or_equals: 2`.
     - Rust: `sdks/wyrd-sdk-rust/tests/integration/gateway_jev.rs::jev_policies_judge_choice_probability_and_score`.
     - Python: `sdks/wyrd-sdk-python/tests/integration/test_gateway_jev.py::test_jev_policies_judge_choice_probability_and_score` (3 parametrized cases).
     - TypeScript: `sdks/wyrd-sdk-ts/wyrd/tests/integration/gateway-jev.test.ts` "jev policies judge $verdict answers" (3 cases).
   - Cases: pass (`compliant` at confidence 0.3, Noul 0.75 at the inclusive pass bound, expected-false 0.02,
     Score 2 at its bound); fail (`violation` at confidence 0.99, Noul 0.25 at the inclusive fail bound,
     expected-false 0.98, Score 1.8); unknown (`unknown`, both Nouls 0.5 in the band, Score 2). Each case asserts
     every requirement outcome, the exact reason (answer, policy and, for expected false, the adjusted
     probability), the selected native answer as `actual`, empty citations, the execution verdict, and the
     exact native upstream body.
   - Commands: Rust `... cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=gateway_jev::jev_policies_judge_choice_probability_and_score)'`;
     Python `... pytest tests/integration/test_gateway_jev.py -k policies_judge`; TypeScript
     `... vitest run tests/integration/gateway-jev.test.ts -t 'jev policies'`, each under
     `scripts/postgres/with-test-postgres.sh` as in scenario 2.
   - GREEN: Rust `1 passed` (whole `gateway_jev` module: 6 passed); Python `3 passed` (whole file: 8 passed);
     TypeScript whole file `8 passed`.
   - TDD deviation: no RED was observed. Leaf dispatch and interpretation for all three policies shipped in
     scenario 1's commit (`vala-verify blocks::transform`, with its boundary unit tests
     `choice_maps_labels_and_ignores_confidence`, `probability_thresholds_are_inclusive_for_both_polarities`,
     `score_compares_level_indices`), so these journeys passed on first run. They pin the hosted path.
   - Checks: `cargo clippy -p wyrd-ai --all-targets -D warnings`, `py:lints`, `py:typecheck`, `ts:lints`,
     `ts:typecheck` pass.

4. **Correct invalid authoring before invocation** — REQ-002, REQ-005–008,
   REQ-011, REQ-013; J6. Using public language construction and local graph
   validate, submit invalid fields/types, invalid finite threshold ordering,
   selected-provider question/criteria limits, missing selected keys, wrong
   question/transform kinds and incomplete Choice maps. Construction catches
   self-contained typed-value errors; assembled validation catches locally
   resolvable relationships with field/remediation. Incremental Card edits
   remain possible before validation. Correct the same definition, revalidate,
   declare and complete the hosted journey. Separately use an exact registered
   Agent/Prompt unavailable locally to prove authoritative registration checks
   that relationship; bypass SDK validation through the shipped HTTP/MCP
   declaration boundary to prove the server still refuses an invalid graph.
   No invalid definition reaches upstream. Initial RED is a late runtime
   refusal, missed local graph error or ignored server error. Combine public
   SDK authoring tests and tier-1 declaration journeys; shared validator unit
   tests do not substitute for either boundary.

   Evidence:
   - Public SDK authoring: Python
     `sdks/wyrd-sdk-python/tests/unit/cards/test_verdict_transform_authoring.py`
     (construction refuses malformed thresholds, operators, blank questions,
     wrong value types, unknown kinds and unknown fields, naming the field;
     `Service.validate()` refuses an unknown question, wrong question kind,
     incomplete or extra Choice outcomes and a non-Jev Prompt with
     `requirements[0].check.verdict_transform.*`; the corrected definition
     revalidates). TypeScript
     `sdks/wyrd-sdk-ts/wyrd/tests/unit/verdict-transform-authoring.test.ts`
     covers the same cases.
   - Registration journey (Rust):
     `gateway_jev::jev_registration_refuses_policies_no_local_graph_can_check`.
     A local graph holding the judge refuses `tone`. A desk naming the
     registered `desk-judge` only by exact `CardRef` validates locally and
     registration refuses it with `WYRD_SPEC_400_VALIDATION` at
     `requirements[0].check.verdict_transform.question`. A raw
     `POST /v1/cards` with a probability policy on a Choice question is
     refused with 400 at `...verdict_transform.kind`. Jev receives nothing.
     The corrected definition validates, declares and judges `passed`.
   - Registration journeys (Python, TypeScript):
     `test_gateway_jev.py::test_jev_registration_refuses_policies_no_local_graph_can_check`
     and `gateway-jev.test.ts` "jev registration refuses policies no local
     graph can check". Each asserts the local refusal names the field, the
     `CardRef` desk validates locally and `WyrdState.declare` raises
     `WyrdError` `WYRD_SPEC_400_VALIDATION` naming
     `requirements[0].check.verdict_transform.question`, Jev receives
     nothing, and the corrected desk declares and judges `passed`.
     Commands: `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -p no:cacheprovider tests/integration/test_gateway_jev.py -k registration"`; `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/gateway-jev.test.ts -t 'jev registration'"`.
     GREEN: 1 passed each. The behavior shipped earlier in this scenario,
     so these cross-SDK journeys passed on first run.
   - Commands:
     `cd sdks/wyrd-sdk-python && uv run python -m pytest -q -p no:cacheprovider tests/unit/cards/test_verdict_transform_authoring.py`;
     `cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/unit/verdict-transform-authoring.test.ts`;
     `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=gateway_jev::jev_registration_refuses_policies_no_local_graph_can_check)'"`.
   - RED: Python had 12 failed ("DID NOT RAISE"). TypeScript on the previous
     native build had 12 failed ("nothing was refused"). Rust, with
     `validate_registered_transforms` disabled, panicked with "registration
     refuses the policy".
   - GREEN: Python 15 passed. TypeScript 12 passed. The Rust journey passed.
     Regressions: the whole `gateway_jev` module (7), Python
     `test_gateway_jev.py` with the authoring file (23), TS
     `gateway-jev.test.ts` (8), and `wyrd-spec`/`wyrd-loader` lib (846)
     all passed. Clippy `-D warnings` for wyrd-spec, wyrd-loader,
     wyrd-server, wyrd-sdk-ts, wyrd-ai and the Python crate, plus
     `py:lints`, `py:typecheck`, `ts:lints`, `ts:typecheck` and
     `ts:napi:check`, all passed.
   - Limitations: no MCP declaration write tool exists, so the bypass goes
     through HTTP `/v1/cards`. The TS native setter refusals are napi
     `InvalidArg` errors carrying the message, as other setter refusals are,
     not `WyrdError` instances. The raw HTTP bypass is pinned once, in the
     Rust journey, because it does not go through an SDK. Question/criteria limits rely on the scenario-2 Jev builder refusals.

5. **Malformed native output is an error** — REQ-010, REQ-012, REQ-014; J7.
   Declare a valid Jev leaf check, then script missing selected answer, wrong
   tag, unknown Choice label, out-of-range Noul/Score and nonfinite numeric
   failures through the native mock/decoder seam as representable. The hosted
   result follows the existing explicit check-error path, never an accepted
   pass or provider-authored failed claim. Assert native nonfinite validation
   directly where JSON cannot encode it. Exercise existing provider refusal
   and transient-retry behavior without adding conversion calls or changing
   authored retry policy. Initial RED is missing selected-answer validation or
   wrong error projection. Tier-1 representative negatives plus typed owner
   tests cover the complete input set.

   Evidence:
   - Journey (Rust): `gateway_jev::jev_malformed_answers_are_check_errors`.
     A declared valid policy desk is scripted with a missing `refund`
     answer, a Noul answer to the Choice question, an undeclared label
     `maybe`, Noul `1.5` and Score `3` on levels 0–2. Only the reading
     requirement becomes `inconclusive`, with the transformer's reason; the
     other three pass. The verdict is `inconclusive`, never a pass or a
     provider-authored failure. Jev is called once per check, and nothing is
     retried.
   - Journeys (Python, TypeScript):
     `test_gateway_jev.py::test_jev_malformed_answers_are_check_errors`
     (5 parametrized markers) and `gateway-jev.test.ts` "jev malformed %s
     answer is a check error" (5 cases) script the same five answers and
     assert the same outcomes, reasons, verdict and `+4` upstream calls.
     Commands: `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -p no:cacheprovider tests/integration/test_gateway_jev.py -k malformed"`; `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/gateway-jev.test.ts -t 'jev malformed'"`. GREEN:
     5 passed each.
   - Owner: `blocks::transform::tests::unusable_answers_are_check_errors`
     (vala-verify) covers the full input set, including NaN and infinity,
     which JSON cannot carry.
   - Commands: `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=gateway_jev::jev_malformed_answers_are_check_errors)'"`;
     `mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(/blocks::transform::tests::unusable_answers_are_check_errors/)'`.
   - TDD deviation: there was no RED. The selected-answer validation shipped
     with the scenario-1 transformer, so the first run found only a wrong
     test expectation, one call versus one per check, and the behavior
     passed. Provider refusal and retry are unchanged and stay covered by
     the existing native gateway tests (`gateway_jev` 401/400/429 cases);
     this task adds no conversion call that could retry. Per this section's
     rule ("record that proof rather than manufacturing a RED result"), no
     RED was forced by reverting the scenario-1 validation.

6. **Read the retained judgment under capture controls** — REQ-005, REQ-012,
   REQ-014; J5/J7. Execute a valid hosted leaf check with full capture and read
   after existing flush/wait controls establish asynchronous visibility. Assert
   canonical verdict/reason, selected native confidence/probabilities and exact
   Verifier/Agent/Prompt/route provenance through normal SDK result reads. Repeat
   under the existing hash/redaction setting; raw answer content is concealed.
   A second tenant's client cannot inspect it. Execute/read through the shipped
   HTTP/MCP/CLI owners as applicable to pin the projected contract, not a new
   provider-specific workflow. The execution response does not wait for record
   persistence. Initial RED is an absent actual capture or discarded typed
   answer/provenance; retained evidence must outlive the in-process response.
   This is tier-1 result-recording/inspection proof across the SDK boundaries.

   Evidence:
   - Journey (Rust): `gateway_jev::jev_judgments_are_retained_under_capture_controls`.
     `desk-policy` is declared as 1.0.0 with `context_capture: full` and as
     2.0.0 with `hash`. After `Execution::wait`, the `details()` read
     (HTTP `GET /v1/verification/executions/{id}/details`) returns the
     exact Verifier version. Each retained requirement's outcome and reason
     equal the returned ones, and its provenance is
     `CallProvenance::Agent { agent: desk-judge, route.deployment: jev }`.
     Its `actual` is the selected native answer (choice with
     confidence/probabilities, Noul, Score with legend) under `full`, and a
     64-hex digest only under `hash`. Another tenant's `details()` gets
     `WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND`.
   - Journeys (Python, TypeScript):
     `test_gateway_jev.py::test_jev_judgments_are_retained_under_capture_controls`
     and `gateway-jev.test.ts` "jev judgments are retained under %s %s
     capture", both parametrized over 1.0.0/`full` and 2.0.0/`hash`. Each
     reads `Execution.connect(id).wait(...)` then `details()` and asserts
     the Verifier version, the retained outcome and reason equal the returned
     ones, `actual` is the exact selected answer under `full` or a 64-hex
     digest under `hash`, provenance is
     `{kind: agent, agent.name: desk-judge, route: {kind: wyrd_gateway, deployment: jev}}`,
     and a second tenant's `details()` raises
     `WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND`. Commands: `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -p no:cacheprovider tests/integration/test_gateway_jev.py -k retained"`;
     `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/gateway-jev.test.ts -t 'jev judgments are retained'"`. GREEN: 2 passed each (the first
     Python run failed on a test defect, reading `each["result"]` where the
     Python details are flat per the `RequirementDetails` stub).
   - Command: `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=gateway_jev::jev_judgments_are_retained_under_capture_controls)'"`.
   - TDD deviation: there was no RED. Scenario 1 put the selected answer in
     the existing `CheckOutput.actual`, which already flows through the
     existing `capture_actual` and asynchronous recording, so this scenario
     pins that contract, not new behavior. Per this section's rule, no RED was
     forced by removing that capture.
   - MCP: the original claim that no MCP tool reads execution details was
     false; `verification.get_execution_details` serves them. Proof:
     `verification::pg_tests::jev_policy_judgments_read_back_through_mcp`
     (`crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs`), recorded under
     Remediation evidence (R1-7).
   - Limitations: no CLI command reads verification execution details.
     Whether execution waits for persistence is not measured here; the
     existing `verify_in_real_time` journeys own that property and this
     change does not touch it. `redact` shares `capture_actual` with
     `hash` and is covered by its owner unit test
     (`assertion::tests`).

7. **Retain existing direct-Verdict and gateway behavior** — REQ-005,
   REQ-014; J8. Reuse hosted direct-Verdict journeys to assert outcomes,
   citations, declared data and malformed-output handling when no transform is
   present. Existing native Jev gateway and Google generation tests remain
   unchanged in meaning. Confirm policies cannot be put on Agent actions and
   named dynamic formats remain refused on system_one. Include the leaf policy
   in served declaration/schema and runtime MCP contract checks, plus a
   journey-tested public example. Expected initial state is passing for existing
   behavior; new contract projection assertions initially fail where absent.

   Evidence:
   - Direct-Verdict and existing behavior: the `gateway_jev`,
     `verifier_building_blocks` and `authoring` journey modules ran
     together, 48 passed. They cover direct-Verdict outcomes, citations,
     data, malformed output and repair, plus the native Jev gateway
     refusal/retry cases. Python `test_gateway_jev.py` and TS
     `gateway-jev.test.ts` stay green (scenario 4 run).
   - Refusals: in `test_verdict_transform_authoring.py`,
     `test_an_agent_action_takes_no_verdict_transform` (`Action.agent`
     refuses the keyword) and
     `test_a_jev_prompt_refuses_a_named_response_format` (a `system_one`
     Prompt with `response_format: verdict` is refused with "this native
     format has no structured output"); 17 passed.
   - Schema contract: `card::verifier::transform::tests::verifier_schema_offers_verdict_transform_only_on_agent_checks`
     (wyrd-spec) asserts `VerdictTransform`'s variants are exactly
     choice/probability/score and that `AgentCheck` is the only schema
     definition carrying `verdict_transform`. `codegen:check` passes with `verdict_transform` in
     `card-types.ts`. No MCP tool or server route serves Card schemas
     separately; the runtime-derived schemars schema is the served contract.
   - Example: `examples/gateway-setup/verifiers/no-refund-promise.yaml` (the
     design's example) and `examples/gateway-setup/refund-desk.yaml`, run by
     `gateway_jev::jev_refund_example_judges_as_documented`: the promise
     fails and the compliant answer passes, each with the documented reason
     and no citations. Documentation is in the gateway-setup README step 6
     and `reference/verifier-card.md` ("Decision answers" plus the two
     rejection rules); `docs:generate` was run and `docs:check` passes.
   - Example journeys (Python, TypeScript):
     `test_gateway_jev.py::test_jev_refund_example_judges_as_documented`
     and `gateway-jev.test.ts` "jev refund example judges %s as documented"
     declare `examples/gateway-setup/refund-desk.yaml` from its path against
     a `judge-pool` deployment and assert the exact reason and verdict for
     the promise (`failed`) and the compliant answer (`passed`). Commands:
     `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -p no:cacheprovider tests/integration/test_gateway_jev.py -k refund_example"`; `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/gateway-jev.test.ts -t 'jev refund example'"`. GREEN:
     2 passed each. The TS run first failed in the full file with "spec hash
     mismatch": its file-scoped server had already registered the
     typed-check helper's own `refund-desk@1.0.0`. The helper's Service is
     now `typed-refund-desk`.
   - Whole files: Python `test_gateway_jev.py` 18 passed; TS
     `gateway-jev.test.ts` 18 passed; `py:format`, ruff, ty, `ts:format`,
     `ts:lints` and `ts:typecheck` clean.
   - RED: the refusals and the existing direct-Verdict behavior passed on the
     first run, as expected. The new example journey first failed on loader
     path escape (`../`), then on a mock marker that also matched the
     question text. Both were test/setup defects, fixed by moving the
     Service file and using unique markers.
   - Gates: clippy `-D warnings` for wyrd-spec and wyrd-ai, `check:deps`,
     `py:lints` and `docs:check` all pass.

## Remediation evidence (TASK-001-R1)

Candidate: `9a0b91125..519044207` (`99ebb9a91`, `262aa808a`, `9303d6367`,
`eb9845817`, `519044207`). Hosted Rust journeys ran after `mise run build:server`.

- **R1-1 Score metadata (FIND-TASK-001-1).** `VerdictTransformer` returns a
  check error when a selected Score answer's `confidence` or any probability
  leaves 0..1. Absent or valid low confidence keeps the comparison.
  - Unit: `mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::transform::tests::score_metadata_must_lie_in_the_unit_interval)'`.
    RED: "Some(1.5) must be a check error". GREEN: 1 passed (re-run at HEAD).
  - Hosted: case `SCORE-CONFIDENCE` (score 2, confidence 1.5) in
    `gateway_jev::jev_malformed_answers_are_check_errors` errors only
    `compliant`, with its reason and an unchanged call count.
    `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=gateway_jev::jev_malformed_answers_are_check_errors)'"`.
    GREEN: 1 passed.
- **R1-2 Typed Python construction (FIND-TASK-001-2).** The `wyrd.verify`
  transform models validate on construction through the native
  `validate_verdict_transform`, which uses the shared
  `VerdictTransform::authored`. Cases: reversed (0.2/0.8), out of range
  (1.5) and NaN. NaN is refused at JSON decode, so its path is
  `verdict_transform`. Each case asserts the code, `details` and
  `remediation`. The test `test_a_corrected_typed_probability_transform_validates`
  covers the corrected value. Mapping-only use is unchanged.
  - `cd sdks/wyrd-sdk-python && uv run python -m pytest -q -p no:cacheprovider tests/unit/cards/test_verdict_transform_authoring.py`
    after `mise run py:setup`. RED: "DID NOT RAISE". GREEN: 21 passed.
    `tests/unit/cards` passed 445; `test_public_exports.py` passed 33.
- **R1-3 TS error projection (FIND-TASK-001-3).** The native
  `setVerdictTransform` returns the shared validator's error through
  `NativeLifecycleResult::outcome`. `Check.agent` raises it as `WyrdError`:
  `WYRD_SPEC_400_VALIDATION`, status 400, `details {rule, path}` and
  `remediation`. A corrected policy is then accepted.
  - `cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/unit/verdict-transform-authoring.test.ts`
    after `mise run ts:build`. RED: "not an instance of WyrdError". GREEN: 14
    passed. `index.d.ts` and `index.d.cts` were regenerated by `ts:build`, and
    `ts:napi:check` is clean.
  - `observe.test.ts` failed on a stale export pin (it lacked `ChoiceQuestion`,
    `NoulQuestion` and `ScoreQuestion`, which are REQ-002 exports). A
    diagnostician placed the cause in the test list. Fixed in `9303d6367`;
    38 passed.
- **R1-4 Null policy (FIND-TASK-001-4).** The `AgentCheck` decoder reads
  `verdict_transform: null` as omitted. A malformed non-null policy is still
  refused. The TS setter's `null` gives the direct-Verdict check.
  - `mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::verifier::transform::tests::agent_check_reads_a_null_policy_as_omitted)'`.
    RED: "invalid type: null, expected internally tagged enum VerdictTransform".
    GREEN: 1 passed. The TS test "a null verdict_transform is the omitted
    policy" validates both the `{}` form and the `null` form.
    `codegen:check` is clean.
- **R1-5 Allocation evidence (FIND-TASK-001-5): measurements WAIVED.** On
  2026-11-18 the maintainer ruled that no benchmark or profiling runs happen
  for this remediation, so no before/after allocation or timing figures exist.
  Source analysis only:
  - Prompt construction (`skald-prompt/src/builder.rs` `jev`) serializes the
    request once to a `Value` and checks the borrowed `JevRequestView` of it.
    This happens at construction only, and the `Value` is dropped.
  - Python typed models now convert once more at construction, model to
    JSON, to validate. Attachment converts again, as before.
  - The TS `setVerdictTransform` clones the Check and the value exactly as the
    generic `set` (`self.inner.clone()`, `value.clone()`) did. There is no new
    copy.
  - `VerdictTransform::authored` consumes its `Value` without cloning.
  - Execution's selected-answer capture is unchanged.
  - Open: measured allocation volume and timing.
- **R1-6 Clean L1 benchmarks (FIND-TASK-001-6): WAIVED.** On 2026-11-18 the
  maintainer overrode this, so `bench:l1:verification` and `bench:l1:gateway`
  were not run. One interrupted, unapproved `bench:l1:verification` start
  wrote no record, and its build output was deleted. No VALID verdict or
  history comparison exists. Open for change review.
- **R1-7 MCP proof (FIND-TASK-001-7).**
  `verification::pg_tests::jev_policy_judgments_read_back_through_mcp` in
  `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs`.
  - Setup: the Jev deployment through Gateway administration, and
    `verification.execute` of a one-question Choice policy.
  - Assertions:
    - the `get_execution_details` output schema carries the closed
      `VerdictTransform` union (`choice`, `probability`, `score`);
    - verdict `failed`, and the exact reason on the execute result and in the
      details;
    - the retained `check.verdict_transform` equals the authored policy;
    - `actual` is the native answer under `full` and a 64-hex digest under
      `hash`;
    - provenance: Agent `mcp-jev-judge` 1.0.0, the registered Prompt Card
      `mcp-jev-judge-prompt`, and route `{wyrd_gateway, jev}`;
    - another tenant's read is refused with
      `WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND`.
  - `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(=verification::pg_tests::jev_policy_judgments_read_back_through_mcp)'"`.
    GREEN: 1 passed.
  - An early assertion wrongly expected no Prompt provenance. A diagnostician
    found that registration turns the inline Prompt into a Card
    (`wyrd-server/src/components/cards/service.rs` `inline_prompt_submission`),
    so the test was corrected to pin that Prompt Card.
  - TDD: not applicable. This was a proof gap with no behavior change. The
    false "no MCP details tool" claim is corrected above.
- **Gates:** `fmt`, `lints` (clippy `-D warnings`), `py:format`, `py:lints`,
  `py:typecheck`, `ts:format`, `ts:lints`, `ts:typecheck`, `ts:napi:check` and
  `codegen:check` are clean.

## Test owners and focused verification

Use SDK `tests/integration/authoring` and verification/gateway story owners:
Rust `sdks/wyrd-sdk-rust/tests/integration` in package `wyrd-ai`, target
`integration`, journey profile with ignored journeys included; Python
`sdks/wyrd-sdk-python/tests/integration` via pytest's integration marker;
TypeScript `sdks/wyrd-sdk-ts/wyrd/tests/integration` via Vitest. Existing
`test:rust-sdk:journey`, `py:test:integration`, and `ts:test:integration` task
bodies supply setup recipes for focused equivalents. Build Python with
`py:setup`, TypeScript with `ts:build` and `ts:build:testing` when needed.
Record each new test's exact selector after it exists, before RED, inside its
acceptance entry; do not run all three complete SDK gates after every cycle.

Pure contract/lowering/render tests belong in wyrd-spec, wyrd-client,
skald-prompt/skald-spec and Vala owning modules. Existing seam proofs to run by
their exact names when reused:

```bash
mise exec -- cargo nextest run --locked -p wyrd-skald-spec --lib -E 'test(=prompt::prompt_media::jev_state_renders_quotes_and_newlines_verbatim)'
mise exec -- cargo nextest run --locked -p wyrd-skald-providers --lib -E 'test(=clients::jev::tests::systemone_posts_bearer_body_and_decodes_the_answer)'
```

Use existing server integration owners for HTTP/declaration/OpenAPI, MCP's
`tests/bifrost/mcp/verification.rs` in target `mcp`, and the CLI's `cli` target
for shipped agent-facing contracts. Their setup comes from the owning mise
lanes, including `test:bifrost:journey:mcp` and `test:principals:integration`.
Select focused cases rather than add new runners.

Finish affected Rust format/lints; Python format/lints/typecheck; TypeScript
format/lints/typecheck/N-API declaration checks; codegen:check, check:deps and
the touched docs/example checks. Generated artifacts come from their owner;
Python stubs are authored. Hot-path inventory/benchmark and integrated lane
ownership are recorded once in the [packet guidance](README.md#verification-ownership).
