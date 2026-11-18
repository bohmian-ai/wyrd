---
id: TASK-001
kind: implementation
status: proposed
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
     not `WyrdError` instances. The Python and TS correction is revalidated
     locally; the declared-and-judged correction runs in the Rust journey.
     Question/criteria limits rely on the scenario-2 Jev builder refusals.

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

7. **Retain existing direct-Verdict and gateway behavior** — REQ-005,
   REQ-014; J8. Reuse hosted direct-Verdict journeys to assert outcomes,
   citations, declared data and malformed-output handling when no transform is
   present. Existing native Jev gateway and Google generation tests remain
   unchanged in meaning. Confirm policies cannot be put on Agent actions and
   named dynamic formats remain refused on system_one. Include the leaf policy
   in served declaration/schema and runtime MCP contract checks, plus a
   journey-tested public example. Expected initial state is passing for existing
   behavior; new contract projection assertions initially fail where absent.

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
