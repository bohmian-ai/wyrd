---
id: SPEC-gateway-jev-deepseek-providers
revision: 1
status: draft
---

# Jev and DeepSeek gateway providers

## Intent

A tenant administrator can configure Jev and DeepSeek as provider-backed gateway deployments. Authorized callers use them through Wyrd's existing authenticated, governed, accounted gateway path. Jev support is required as a provider, not conditional on a verification use case. This backlog draft does not authorize implementation.

## Required behavior

- **REQ-001 — DeepSeek direct OpenAI-format routes.** A tenant can register a DeepSeek deployment with its own credential and provider-native model ID. DeepSeek documents `POST /chat/completions` and `POST /responses` in OpenAI format; Wyrd exposes both through its existing `/v1/chat/completions` and `/v1/responses` routes with `model: deepseek/<model>`, forwarding to the corresponding DeepSeek route through the qualified OpenAI-compatible adapter. Buffered and streaming responses, supported tools, usage, cache-hit metadata, throttling, and errors retain their documented behavior. DeepSeek model IDs are not hardcoded to one current marketing name. Unsupported DeepSeek protocol features are rejected or left out of the advertised capability, never silently claimed.
- **REQ-002 — Jev direct route.** Add a built-in Jev provider identity and a typed, non-streaming `POST /v1/systemone` gateway route. It accepts Jev's `state`, `model`, and keyed `noul`, `choice`, or `score` questions and returns Jev's resolved model, intact answers, and usage. The gateway forwards to TypeSafe's `POST /v1/systemone` with the deployment's stored bearer credential. Invalid question forms, provider bounds, and model/deployment mismatch fail before provider IO. This route uses the same gateway invocation path as other providers.
- **REQ-003 — Jev OpenAI Chat Completions adapter.** The existing `POST /v1/chat/completions` route also accepts `model: jev/<model>` under an explicit structure: exactly one user message whose text content is a JSON object containing `state` and a non-empty `questions` map in the Jev forms. The top-level model supplies Jev's native model; message JSON cannot override it. Reject additional turns, non-text content, tools, streaming, and generation controls Jev cannot honor. The adapter sends the validated Jev request to `/v1/systemone` and returns a valid non-streaming Chat Completions envelope whose sole assistant `message.content` is JSON text containing the resolved model and intact keyed answers. Map Jev input/output tokens to OpenAI usage and Wyrd accounting; do not fabricate conversational prose or missing Jev fields. An ordinary chat message lacking the required structure fails before provider IO.
- **REQ-004 — Shared gateway behavior.** Native and OpenAI-shaped Jev calls share tenant authentication, deployment RBAC when available, routing, admission, audit, accounting, capture, pricing, request bounds, timeout, and credential rotation. A Jev question request may route only to deployments that honor the same question/answer contract; ordinary chat deployments cannot become its fallback and Jev cannot become an ordinary chat fallback. Jev does not advertise embeddings, tools, streaming, or batches. Explicit deployment selection, if enabled by the deployment-access change, applies the same authorization and pinning rules to Jev and DeepSeek.
- **REQ-005 — Provider failures and usage.** Map Jev's documented authentication, validation, throttling, and overload responses to stable Wyrd errors without exposing credentials or raw provider payloads. Retry behavior is bounded and deliberate; validation and authentication failures are not retried. Record Jev input/output token usage and configured pricing without inventing unsupported cache or billing fields.
- **REQ-006 — Public surfaces.** The native Jev and constrained Chat Completions contracts are documented in typed HTTP/OpenAPI; the latter is usable with standard OpenAI clients. First-class Wyrd SDKs project the native operation through the shared client and the adapted operation wherever they expose gateway inference. Provider support tables, generated schemas, docs, CLI/MCP administration projections, and stable errors agree with runtime behavior. DeepSeek administration and both supported inference routes are documented as qualified OpenAI-compatible behavior.

## Constraints and decisions

- **INV-001:** One `wyrd-server` listener and the existing gateway invocation/dispatch path remain authoritative. No Jev-specific gateway service, new Card kind, or second credential system is introduced.
- **INV-002:** Provider secrets remain write-only/redacted, tenant-bound, and resolved by the gateway; requests never supply provider credentials or tenant identity.
- **INV-003:** Jev's different wire protocol is represented honestly. An ordinary OpenAI chat request cannot be silently interpreted as a Jev question operation, and Jev's structured answer is not flattened into fabricated conversational text. Native and adapted routes produce equivalent Jev answers and usage.
- **INV-004:** DeepSeek compatibility is established by behavior and tests, not by assuming every OpenAI-shaped endpoint or response is identical.
- **INV-005:** DeepSeek's documented Anthropic-format endpoint and beta FIM Completions endpoint are outside this initial capability; neither is exposed by a misleading route alias. DeepSeek's direct OpenAI-format API uses the same two public Wyrd routes as other OpenAI-format providers, so no duplicate provider-specific route is added.

## Acceptance

- **AC-001:** An administrator registers DeepSeek with a tenant credential; an authorized client completes Chat Completions and Responses, including supported streaming, through Wyrd with correct upstream routes, model, key isolation, usage, cache-hit fields, and bounded errors. The existing DeepSeek issue is closed only after this public journey passes.
- **AC-002:** An administrator registers Jev; a real client calls `/v1/systemone` and an OpenAI-compatible client calls `/v1/chat/completions` with the required single-message JSON shape for each Jev question form. Both return equivalent intact answers and normalized usage with attributable accounting and audit.
- **AC-003:** Unsupported chat features, malformed or ordinary free-form messages, unauthorized or cross-tenant deployment, invalid credential, 429/529, timeout, and omitted usage produce documented outcomes without another tenant's secret or data. Fallback cannot turn a Jev question into an ordinary chat answer or the reverse.
- **AC-004:** Public HTTP and first-class client journeys, plus generated contract checks, prove both Jev routes and the qualified DeepSeek routes; mock providers keep routine tests credential-free.

## Open material decisions

None. Both Jev ingress forms are required. The single-message JSON shape is the explicit Chat Completions projection; ordinary conversational chat remains outside Jev's documented capability.

## Authority and revision

- `AGENTS.md` §§2, 9–11; `architecture/wyrd-design.md`; `architecture/wyrd-security-posture.md`; `architecture/references/architecture/patterns.md`.
- Provider grounding: [TypeSafe Jev API](https://docs.typesafe.ai/api), [Jev models](https://docs.typesafe.ai/models), [DeepSeek Chat Completions](https://api-docs.deepseek.com/api/create-chat-completion/), [DeepSeek Responses](https://api-docs.deepseek.com/api/create-response/), [DeepSeek Anthropic format](https://api-docs.deepseek.com/guides/anthropic_api/), [DeepSeek issue #35](https://github.com/bohmian-ai/wyrd/issues/35).
- Revision 1 — draft, 2026-09-25: native Jev provider and qualified DeepSeek support.
