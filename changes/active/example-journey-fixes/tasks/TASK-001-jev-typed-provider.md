# TASK-001 Jev as a typed provider

Spec: `../spec.md` r2 REQ-001, INV-001, INV-002, INV-004, AC-001..AC-003.

r2 (2026-10-14, owner approval "I approve jev r2" relayed by the coordinator):
the gateway also serves a Jev deployment on `/v1/chat/completions` as a typed
pass-through (the last user message's text is the JSON of `{state,
questions}`; one typed bad-request error when it does not decode; the typed
answer returns as JSON in `choices[0].message.content` with usage mapped), and
an upstream Jev 401/403 relays through the shared `failure()` → `refusal()`
path like every other provider, unretried. INV-004 is relaxed to match.

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
| r2 unified-route pass-through (typed request from last user message, one typed 400 otherwise, streaming refused, typed answer JSON as content, usage mapped); Jev serves `system_one` + `chat_completions`; fallback still never crosses the Jev boundary | `adapter/jev.rs::{chat_request,chat_response}`, `adapter/mod.rs::{prepare,complete}`, `deployment.rs::ProviderAdapter::serves`, `routing.rs` | `adapter::tests::{jev_chat_pass_through_carries_the_typed_request_and_answer,jev_bad_forms_are_rejected_before_provider_io}`, `gateway::deployment::tests` (Jev serves chat), `routing::tests::fallback_never_crosses_the_jev_protocol_boundary`; journeys `chat_completions_pass_typed_jev_requests_through` (Rust via HTTP, Python and TS via the stock OpenAI client) | PASS |
| r2 upstream 401/403 relays like other providers (status + scrubbed body, no Wyrd code, not retried) on both routes; other refusals keep stable codes on both routes | `adapter/http.rs::jev_failure`, dispatch `translated` (Jev translated only on the chat route) | `adapter::tests::{jev_credential_refusals_relay_like_other_providers,jev_refusals_map_to_stable_wyrd_errors}`; journeys `jev_refusals_map_to_stable_errors` (Rust, Python, TS) | PASS |
| Agent loop refuses Jev session turns | `crates/skald/skald-agent/src/session.rs` | `session::tests::jev_session_turns_are_refused_as_not_a_chat_dialect` | PASS |
| Prompt Card native Jev dialect; quotes/newlines render | `skald-spec/src/prompt.rs`, `fixtures/cards/gateway_jev/jev-judge-prompt.yaml` | `prompt::prompt_render::jev_state_renders_quotes_and_newlines_verbatim` | PASS |
| `llm_judge` with Jev Prompt; assertion thresholds typed answer | `crates/vala/vala-eval/src/orchestrator/judge.rs` | `orchestrator_judge_skald::jev_judge_answers_typed_questions_with_quoted_state`; journeys `jev_judge_passes_a_helpful_polite_reply`, `jev_judge_fails_an_unhelpful_rude_reply`, `jev_judge_judges_a_reply_with_quotes_and_newlines` (Rust, Python, TS) | PASS |
| Journeys: native, invalid question type, unified pass-through, upstream 400/401/429/5xx | `sdks/wyrd-sdk-rust/tests/integration/gateway_jev.rs`, `sdks/wyrd-sdk-python/tests/integration/test_gateway_jev.py`, `sdks/wyrd-sdk-ts/wyrd/tests/integration/gateway-jev.test.ts` | `mise run test:gateway:journey` (Rust 7/7, Python 7/7, TS 7/7 Jev) | PASS |
| Generated schemas and SDK types | `crates/wyrd-spec/schemas/*.json`, `_card_types.py`, `card-types.ts` | `mise run codegen:check` | PASS |

Commands (all green): `mise run fmt`; `mise run lints`; `mise run codegen:check`;
`mise run py:format:check`; `mise run py:lints`; `mise run py:typecheck`;
`mise run py:test:unit`; `mise run ts:format:check`; `mise run ts:lints`;
`mise run ts:typecheck`; `mise run test:gateway:journey`;
`mise exec -- cargo nextest run --locked -p wyrd-spec -p vala-eval -p wyrd-skald-spec -p wyrd-skald-providers -p wyrd-skald-prompt -p wyrd-skald-agent -p wyrd-skald-runtime -p wyrd-skald-workflow -p wyrd-gateway` (1408 passed);
`wyrd-server --lib -E 'test(/components::gateway::/)'` and
`--test integration -E 'test(=pg_openapi_contract::gateway_ingress_publishes_typed_contracts)'` under the managed Postgres wrapper;
live smoke `tests/smoke/test_gateway_live_providers.py::test_jev_answers_live_typed_questions` (gated on `TYPESAFE_API_KEY` presence) passed; `git diff --check` clean.

r2 commands (all green): `mise run codegen:regen`; `mise run fmt`;
`mise run lints`; `mise run codegen:check` (no contract drift: `serves` is
not schema); `mise run py:format`; `mise run py:lints`;
`mise run py:typecheck`; `mise run ts:format`; `mise run ts:typecheck`;
`mise run test:gateway:journey`;
`mise exec -- cargo nextest run --locked -p wyrd-gateway -p wyrd-spec --lib`;
live smoke `test_jev_answers_live_typed_questions` now on both routes
(`/v1/systemone` and `/v1/chat/completions` through the stock OpenAI client),
gated on `TYPESAFE_API_KEY` presence, passed.

Journey note: an upstream 429/5xx cools the single Jev deployment down for 30
seconds (`wyrd-gateway::health`), so each journey proves 400 and 401 on both
routes, 429 on the chat route, and then 500/529 on the native route, whose
502 is the same stable error whether Jev or the cooldown refuses; the adapter
unit tests pin every mapping on both routes.

The r2 unified route ignores chat members other than the last user message
(including `tools` and generation controls) by design: it is a pass-through of
the typed Jev request, not a translation.

Non-goals: no other task's files touched; Python `Prompt(provider="jev")` is refused (a Jev Prompt is authored as a native `request`).
