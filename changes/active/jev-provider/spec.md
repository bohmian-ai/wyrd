---
id: SPEC-jev-provider
revision: 1
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

## Required behavior

### REQ-001 Jev provider is typed

- Jev's SystemOne API (`POST https://api.typesafe.ai/v1/systemone`, request
  `{model, state, questions}` with question types `noul`, `choice`, `score`,
  each with `instructions` and `criteria`; response `{model, answers, usage}`
  where a noul answer carries its probability, a choice answer its choice,
  probabilities and confidence, a score answer its score and legend) is a
  first-class provider dialect with fully typed request and response
  structures, aligned with how OpenAI, Anthropic, Google, and Vertex are
  modeled (`ProviderRequest`/`ProviderResponse` variants, wire types, provider
  client, gateway adapter, schemas, and SDK types).
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
- Upstream error mapping on both routes: 400/422 invalid request, 429
  throttled, other non-auth upstream failures unavailable, with those upstream
  bodies never relayed. (r3) An upstream 401 or 403 behaves exactly as for the
  other providers: the upstream status and body are relayed through the shared
  refusal path with the credential removed, and are not retried.
- A Verifier `llm_judge` task can use a Jev Prompt as its judge, and a
  dependent assertion can threshold a typed answer field (for example
  `$.task.judge.parsed.answers.helpful.noul >= 0.5`).

## Invariants

- INV-001 Tenant isolation holds on every changed path.
- INV-002 Provider credentials and keys are never logged or returned.
- INV-004 No compatibility aliases for the removed Jev chat shim; the r3

## Acceptance obligations

- AC-001 Each REQ has a user-journey test (Rust, Python, and TypeScript where
  the surface ships in all three; MCP/HTTP where agent-facing) that fails on
  the base and passes on the candidate, booting the server the way users do
  where the defect was harness-masked (REQ-003 uses the official TypeScript
  MCP SDK; REQ-007 drives the CLI).
- AC-002 Negative flows: Jev invalid question type and upstream 400/429/5xx;
  Bifrost unauthorized-table errors leak nothing; malformed `traceparent`.
- AC-003 Format, lints, codegen check, and the owning `mise` lanes pass.

## Open material decisions

None.
