# TASK-001 Jev as a typed provider

Spec: `../spec.md` REQ-001, INV-001, INV-002, INV-004, AC-001..AC-003.

Rework the rejected Jev implementation (commit "Add the TypeSafe Jev gateway
provider" on this branch) so Jev is modeled exactly like the existing typed
providers: `ProviderRequest`/`ProviderResponse` variants with typed wire
structs in `skald-spec` (request: model, state, questions keyed by name with
typed noul/choice/score question variants, instructions, criteria; response:
model, typed answers per question type, usage), the provider client, the
gateway adapter and native `/v1/systemone` route, Prompt Card support for a
native Jev request dialect, generated schemas, and Rust/Python/TypeScript SDK
types. Remove the chat-completions JSON-in-message shim and every raw
`Value` body standing in for Jev.

A Verifier `llm_judge` with a Jev Prompt must work, and a dependent assertion
must threshold a typed answer field. Prompt variables containing quotes and
newlines must render a valid request (prove it end to end; the prior
implementation's quote refusal must become a success).

Evidence: unit tests for wire round-trips and error mapping; gateway journeys
in Rust, Python, and TypeScript (native route, judge Verifier pass and fail,
quote in variables, invalid question type, upstream 400/429/5xx); updated
fixtures; codegen check.

## Evidence

Reuse: Jev joins the existing typed-provider owners (`skald-spec` wire +
`ProviderRequest`/`ProviderResponse`, `skald-providers` client, `wyrd-gateway`
adapter, server `NativeCall`). The only new behaviour is the judge's Jev path
(`SkaldJudgeInvoker::ask_jev`): the Agent loop drives chat dialects only, so a
Jev Prompt renders once and dispatches through the same provider registry.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Typed Jev request/response variants and wire structs | `crates/skald/skald-spec/src/wire/jev.rs`, `request.rs`, `response.rs`, `adapter.rs` | `wire::jev::tests::{documented_forms_round_trip,bad_forms_and_bounds_are_rejected,documented_answers_decode_typed}`, `adapter::adapter_tests::jev_adapter_reads_the_typed_answer_as_structured_output` | PASS |
| Provider client sends typed body, maps refusals | `crates/skald/skald-providers/src/clients/jev.rs` | `clients::jev::tests::{systemone_posts_bearer_body_and_decodes_the_answer,refusal_is_a_status_error_and_other_dialects_mismatch}` | PASS |
| Gateway adapter + native route; chat shim removed | `crates/wyrd/wyrd-gateway/src/adapter/{mod,jev,http}.rs`, `routing.rs`, `wyrd-spec/src/gateway/deployment.rs`, server `gateway/workflow.rs` | `adapter::tests::{jev_bad_forms_are_rejected_before_provider_io,jev_answers_return_typed_with_usage,jev_omitted_usage_stays_unknown,jev_refusals_map_to_stable_wyrd_errors}`, `routing::tests::fallback_never_crosses_the_jev_protocol_boundary`, `pg_openapi_contract::gateway_ingress_publishes_typed_contracts` | PASS |
| Prompt Card native Jev dialect; quotes/newlines render | `skald-spec/src/prompt.rs`, `fixtures/cards/gateway_jev/jev-judge-prompt.yaml` | `prompt::prompt_render::jev_state_renders_quotes_and_newlines_verbatim` | PASS |
| `llm_judge` with Jev Prompt; assertion thresholds typed answer | `crates/vala/vala-eval/src/orchestrator/judge.rs` | `orchestrator_judge_skald::jev_judge_answers_typed_questions_with_quoted_state`; journeys `jev_judge_passes_a_helpful_polite_reply`, `jev_judge_fails_an_unhelpful_rude_reply`, `jev_judge_judges_a_reply_with_quotes_and_newlines` (Rust, Python, TS) | PASS |
| Journeys: native, invalid question type, chat refused, upstream 400/429/5xx | `sdks/wyrd-sdk-rust/tests/integration/gateway_jev.rs`, `sdks/wyrd-sdk-python/tests/integration/test_gateway_jev.py`, `sdks/wyrd-sdk-ts/wyrd/tests/integration/gateway-jev.test.ts` | `mise run test:gateway:journey` (Rust 7/7, Python 7/7, TS 7/7 Jev) | PASS |
| Generated schemas and SDK types | `crates/wyrd-spec/schemas/*.json`, `_card_types.py`, `card-types.ts` | `mise run codegen:check` | PASS |

Commands (all green): `mise run fmt`; `mise run lints`; `mise run codegen:check`;
`mise run py:format:check`; `mise run py:lints`; `mise run py:typecheck`;
`mise run py:test:unit`; `mise run ts:format:check`; `mise run ts:lints`;
`mise run ts:typecheck`; `mise run test:gateway:journey`;
`mise exec -- cargo nextest run --locked -p wyrd-spec -p vala-eval -p wyrd-skald-spec -p wyrd-skald-providers -p wyrd-skald-prompt -p wyrd-skald-agent -p wyrd-skald-runtime -p wyrd-skald-workflow -p wyrd-gateway` (1408 passed);
`wyrd-server --lib -E 'test(/components::gateway::/)'` and
`--test integration -E 'test(=pg_openapi_contract::gateway_ingress_publishes_typed_contracts)'` under the managed Postgres wrapper;
live smoke `tests/smoke/test_gateway_live_providers.py::test_jev_answers_live_typed_questions` (gated on `TYPESAFE_API_KEY` presence) passed; `git diff --check` clean.

Non-goals: no other task's files touched; Python `Prompt(provider="jev")` is refused (a Jev Prompt is authored as a native `request`).
