# Jev is a typed gateway provider

| | |
|---|---|
| Change ID | `SPEC-jev-provider` |
| Completed | 2026-10-14 |
| Reviewed target | `41ee33588` (`split/jev-provider`) |
| Approved specification | revision 2, approved 2026-10-14 |
| Delivery reference | PR #155 (https://github.com/bohmian-ai/wyrd/pull/155) |
| Completion authority | **human override**. See "Completion authority". |

## Intent and user value

A Prompt, the gateway, and a Verifier `llm_judge` task can use TypeSafe Jev's
SystemOne API. They do so through fully typed request and response structures,
on Jev's native route and as a typed pass-through on the unified chat route.
Jev is onboarded the same way as every other provider and has no special path.

## Shipped externally observable behavior

- **Typed dialect.** Jev's SystemOne API is a first-class provider dialect.
  - Request `{model, state, questions}`, with question types `noul`,
    `choice`, and `score`.
  - Response `{model, answers, usage}`.
  - It is modeled with `ProviderRequest::JevSystemOne` and a `ProviderResponse`
    variant, plus typed wire types, a standalone `JevAuth`, `JevClient`, a
    gateway adapter, generated schemas, and SDK types.
  - No raw JSON value stands in for the request or response body.
- **Prompt Cards.** A Prompt Card declares a Jev request natively. Prompt
  variables substitute into the request's string fields with the existing
  render escaping, so quotes and newlines produce a valid request.
- **Native gateway route.** The gateway serves Jev on `/v1/systemone` with
  typed validation. Tenant credentials, deployments, governance, capture,
  fallback, and error mapping behave as for other providers.
- **Unified chat route.** `/v1/chat/completions` is a typed pass-through, so
  stock OpenAI clients can reach Jev.
  - The last user message is the JSON of a typed `{state, questions}`
    request. Text that does not decode is refused with one typed bad-request
    error before any provider IO.
  - The typed answer returns as JSON in `choices[0].message.content`, with Jev
    usage mapped to completion usage.
- **Refusals.** On both routes, upstream refusals relay through the shared
  refusal path: the upstream status and body pass through with the credential
  removed, and retry follows the shared status classification.
  `WYRD_GATEWAY_429_UPSTREAM_THROTTLED` was removed.
- **Fallback.** Fallback never leaves the Jev protocol. Only `system_one`
  deployments serve a SystemOne question, and a deployment whose adapter
  cannot represent the request body is dropped. These are the same rules that
  govern every provider.
- **Verifier judge.** An `llm_judge` judge is one model call for every
  provider.
  - Its Prompt must yield structured output, either through a declared JSON
    schema or because its dialect answers with a typed object by protocol, as
    Jev does.
  - A dependent assertion can threshold a typed answer field, for example
    `$.task.judge.parsed.answers.helpful.noul >= 0.5`.
  - Judge refusals follow the shared retry classification. This fixed a
    defect for every provider: 400 and 401 refusals were being retried.

## Lasting invariants and constraints

- There is one way to onboard a provider. Jev touches only the per-dialect
  owners that every provider touches. No shared path branches on Jev by name:
  not the Agent loop, the judge, gateway failure mapping, or fallback
  planning.
- Tenant isolation holds on every changed path.
- Provider credentials and keys are never logged or returned.
- No compatibility alias exists for the removed Jev chat shim. The unified
  route carries the typed Jev request, not a chat-to-question translation.

## Material decisions and rationale

- **Standalone auth, no exported constants.** Every provider owns its own
  `<Provider>Auth`, so Jev's base URL and key variable (`TYPESAFE_API_KEY`)
  live in `JevAuth`.
- **Shared refusal path.** Jev's refusals carry the same information as any
  provider's. A Jev-only error code and failure mapper did not earn an
  exception.
- **Provider-neutral judge.** A judge is constrained to one iteration, no
  tools, and no session, so it is one model call for every provider. Whether
  output is structured is a property of the request, exposed by
  `ProviderRequest::answers_structured` and `Prompt::yields_structured_output`.
  Provider identity no longer decides it.
- **Session seeding.** The Agent session refuses requests that cannot run as a
  loop through the shared `validate_prompt_loop_request`, not through a
  Jev-specific branch.

## Spec revisions and deviations

- **r1** carried the Jev requirements over unchanged from
  SPEC-example-journey-fixes r6.
- **r2** requires Jev to onboard like every provider:
  - refusals use the shared path;
  - the Jev throttle code is removed;
  - fallback isolation comes from deployment capability and request
    representability;
  - the judge requirement is provider-neutral.
- **Out of scope.** Correlating a judge's gateway call to the judgment's Run
  and Card belongs to SPEC-verify-attribution REQ-004, and
  SPEC-sub-act-attribution reshapes it. The Jev journeys prove the passing
  result and the judge's one recorded `system_one` call. They do not prove the
  Run join. verify-attribution's judge-correlation journey is to add a Jev
  case.

## Acceptance closure

| Obligation | Evidence |
|---|---|
| AC-001: user journeys | `mise run test:gateway:journey` passed. Rust SDK 23/23, Python 38/38, TypeScript 23/23, plus the native, Vault, and surface tiers. Jev journeys are in `sdks/wyrd-sdk-rust/tests/integration/gateway_jev.rs`, `sdks/wyrd-sdk-python/tests/integration/test_gateway_jev.py`, and `sdks/wyrd-sdk-ts/wyrd/tests/integration/gateway-jev.test.ts`. |
| AC-002: negative flows | Invalid question types are refused before Jev. Upstream 400 and 401 are relayed and not retried; 429 is relayed. The judge's 400 is not retried and leaks no key. All of this is covered in all three SDK journeys. 5xx retry classification is pinned by `jev_refusals_relay_like_other_providers` in `crates/wyrd/wyrd-gateway/src/adapter/tests.rs`. Judge retry classification is pinned by `provider_refusals_follow_the_shared_retry_classification` in `crates/vala/vala-eval/tests/integration/orchestrator_judge_skald.rs`. |
| AC-003: format, lints, codegen, lanes | `mise run fmt`, Clippy `-D warnings` on every touched crate and `wyrd-ai`, `mise run codegen:check`, Python format and lints, TypeScript format and typecheck, and the touched crates' unit tests all passed. |

## Completion authority

This record was written under an explicit human override on 2026-10-14. No
`$wyrd-task-review` or final `$wyrd-change-review` ran. The packet held only
the specification, and the acceptance evidence above is the implementer's own
verification, not an independent review.

## Links

- Architecture: [Wyrd design](../../../architecture/wyrd-design.md)
- Provider: `crates/skald/skald-providers/src/auth/jev.rs`,
  `crates/skald/skald-providers/src/clients/jev.rs`,
  `crates/skald/skald-spec/src/wire/jev.rs`
- Gateway: `crates/wyrd/wyrd-gateway/src/adapter/jev.rs`,
  `crates/wyrd/wyrd-gateway/src/routing.rs`,
  `crates/wyrd/wyrd-server/src/components/gateway/ingress.rs`
- Judge: `crates/vala/vala-eval/src/orchestrator/judge.rs`
- Fixtures: `fixtures/cards/gateway_jev/`
