---
id: SPEC-jev-provider
revision: 2
status: approved
---

# Jev is a typed gateway provider

## Objective and user value

A Prompt, the gateway, and a Verifier `llm_judge` task use TypeSafe Jev's SystemOne API through fully typed request and response structures, on its native route and as a typed pass-through on the unified chat route.

## Approval

Approved by the human owner on 2026-10-14 as part of SPEC-example-journey-fixes
r6. Split into its own change by owner direction on 2026-10-14 so each problem
is reviewed and verified on its own. The requirement text is carried over
unchanged.

Revision 2 approved by the human owner on 2026-10-14: Jev onboards exactly
like every other provider and earns no special path. Its upstream refusals use
the shared refusal path, the Jev-only `WYRD_GATEWAY_429_UPSTREAM_THROTTLED`
code is removed, fallback isolation comes from deployment capabilities and
request representability rather than provider identity, and the `llm_judge`
requirement is stated as a provider-neutral capability.

## Required behavior

### REQ-001 Jev provider is typed

- Jev's SystemOne API (`POST https://api.typesafe.ai/v1/systemone`, request
  `{model, state, questions}` with question types `noul`, `choice`, `score`,
  each with `instructions` and `criteria`; response `{model, answers, usage}`
  where a noul answer carries its probability, a choice answer its choice,
  probabilities and confidence, a score answer its score and legend) is a
  first-class provider dialect with fully typed request and response
  structures, aligned with how OpenAI, Anthropic, Google, and Vertex are
  modeled (`ProviderRequest`/`ProviderResponse` variants, wire types, a
  standalone `JevAuth`, provider client, gateway adapter, schemas, and SDK
  types).
- (r2) There is one way to onboard a provider. Jev touches only the
  per-dialect owners every provider touches; no shared path (Agent loop,
  judge, gateway failure mapping, fallback planning) branches on Jev by name.
- A Prompt Card declares a Jev request natively (its own `request.provider`
  dialect and typed body). Prompt variables substitute into the request's
  string fields with the existing render escaping, so any variable value,
  including quotes and newlines, produces a valid request.
- No raw `serde_json::Value` stands in for the Jev request or response body,
  and no Jev path maps a chat conversation onto Jev questions. The rejected
  chat-route shim (single-message tunnel with an untyped answer) is removed.
- The gateway serves Jev on its native route (`/v1/systemone`) with typed
  validation; tenant credentials, deployments, governance, capture, fallback
  isolation, and stable error mapping behave as for other providers.
- (r3) The gateway also serves a Jev deployment on the unified route
  (`/v1/chat/completions`) as a typed pass-through, so stock OpenAI clients can
  reach it: the text of the last user message is the JSON of the typed Jev
  request `{state, questions}` and is decoded with the same typed structures;
  text that does not decode is refused with one typed bad-request error before
  any provider IO. The typed Jev answer returns serialized as JSON in
  `choices[0].message.content`, with Jev usage mapped to the completion usage.
  No schema mapping or chat-to-question translation is added.
- (r2) Every upstream refusal on both routes behaves exactly as for the other
  providers: the upstream status and body relay through the shared refusal
  path with the credential removed, and retry follows the shared status
  classification. No Jev-specific error code exists.
- (r2) Fallback never leaves the Jev protocol because only deployments
  declaring `system_one` serve a `systemone` question and a deployment whose
  adapter cannot represent a request body is dropped, the same rules that
  govern every provider.
- (r2) A Verifier `llm_judge` judge is one model call for every provider: its
  Prompt must yield structured output, either by a declared JSON schema or
  because its dialect answers with a typed object by protocol (Jev). A Jev
  Prompt therefore works as a judge, and a dependent assertion can threshold
  a typed answer field (for example
  `$.task.judge.parsed.answers.helpful.noul >= 0.5`). A judge provider
  refusal is retried by the shared classification for every provider.
- (r2) Out of scope: correlating a judge's gateway call to the judgment's
  Run and Card. That is SPEC-verify-attribution REQ-004 for every provider,
  and SPEC-sub-act-attribution reshapes it. The Jev journeys prove only the
  passing result and the judge's one recorded `system_one` call;
  verify-attribution's judge-correlation journey adds a Jev case when it
  rebases onto this change.

## Invariants

- INV-001 Tenant isolation holds on every changed path.
- INV-002 Provider credentials and keys are never logged or returned.
- INV-004 No compatibility aliases for the removed Jev chat shim; the r3
  unified-route pass-through is the only chat entry to Jev, and it carries the
  typed Jev request, not a translation.

## Acceptance obligations

- AC-001 Each REQ has a user-journey test (Rust, Python, and TypeScript where
  the surface ships in all three; MCP/HTTP where agent-facing) that fails on
  the base and passes on the candidate, booting the server the way users do
  where the defect was harness-masked (REQ-003 uses the official TypeScript
  MCP SDK; REQ-007 drives the CLI).
- AC-002 Negative flows: Jev invalid question type and upstream 400/401/429
  relayed like other providers (5xx pinned by adapter unit tests);
  Bifrost unauthorized-table errors leak nothing; malformed `traceparent`.
- AC-003 Format, lints, codegen check, and the owning `mise` lanes pass.

## Open material decisions

None.
