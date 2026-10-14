---
id: SPEC-example-journey-fixes
revision: 3
status: approved
---

# Make the end-to-end TypeScript example journey work on the real product

## Objective and user value

Building `bohmian-ai/wyrd-example-ts` (a TypeScript support agent: WyrdState
Runs, OpenAI Agents SDK through the Wyrd gateway, real-time Verifiers including
a Jev LLM judge, a Bifrost table, OTLP gRPC telemetry, Bifrost queries, and an
agent investigating evidence over Wyrd MCP) exposed product defects that the
test suite did not catch, because the harness performs setup the product does
not and our own clients are more lenient than common third-party ones. Every
defect below is fixed together, each proven by a journey that drives the
product the way a user or common framework does.

## Approval

Approved by the human owner on 2026-10-14 by explicit directive: Jev must be a
typed provider with full request and response shapes like every other
provider ("raw json is unacceptable"; the prior Jev implementation is
rejected); fix Bifrost error surfacing; Wyrd MCP must be compatible with common
frameworks; attach run and Card attribution to real-time judge calls; the
local server must handle the gateway keyring itself, restarting as needed;
fix trace-context linking and live-table subqueries; "everything gets fixed
together".

## Scope

- REQ-001 Jev provider is typed end to end.
- REQ-002 Bifrost returns actionable caller errors.
- REQ-003 Wyrd MCP works with common MCP clients and model tool-calling.
- REQ-004 Real-time Verifier model calls are attributed to the Run and Card.
- REQ-005 Gateway spans join the caller's trace.
- REQ-006 Bifrost subqueries over live tables work.
- REQ-007 Gateway managed secrets use the deployment sealing keyring, and
  local server setup provides it.

Non-goals: DeepSeek or other new providers; new Verifier semantics; a public
release.

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

### REQ-002 Bifrost caller errors

- A query that fails planning because of the caller's SQL (unknown column,
  table, or function; type mismatch; ambiguous reference) returns a 4xx
  `WyrdError` whose detail carries the planner's message (including its
  suggestion such as "Did you mean 'vr.verdict'?"), consistently for single
  table queries, joins, and subqueries, over HTTP, SDK, and MCP.
- Internal failures remain 5xx and never expose internals. Messages never
  reveal tables or columns the caller is not authorized to see.

### REQ-003 MCP compatibility

- `tools/list` results satisfy MCP protocol revision 2026-07-28, including
  `ttlMs` and `cacheScope` (already implemented on the change branch).
- Every advertised tool, read and write, has an input schema accepted by
  common tool-calling consumers: a top-level `type: object` with no top-level
  `oneOf`, `anyOf`, `allOf`, `enum`, `const`, or `not` (OpenAI function tool
  rules), and output schemas accepted by the official MCP SDK.
- The official TypeScript MCP SDK and the OpenAI Agents SDK can connect to
  `/mcp`, list the full catalog for an administrator, convert every tool, and
  call a read tool.

### REQ-004 Real-time judge attribution

- When a Verifier runs in real time for a Run (`run_id` and subject Card
  supplied with the verify request), every gateway model call it makes, such
  as an `llm_judge` call, is recorded in `vala.gateway.calls` with that
  `run_id` and the subject `card_uid`, so it joins the Run's other evidence.
  Without a Run, attribution stays absent rather than invented.

### REQ-005 Trace linking

- The gateway honors W3C `traceparent` (and `tracestate`) on inbound
  inference requests: its spans become children of the caller's span, so an
  application span and the gateway's model-call span share a trace ID in
  `vala.traces.spans`. In-process calls made on behalf of a traced request
  (such as judge calls) inherit the active trace context. Malformed headers
  are ignored, never fatal.

### REQ-006 Live-table subqueries

- Scalar, `IN`, and `EXISTS` subqueries over live (not yet compacted) Bifrost
  tables succeed and return the same rows as the equivalent join, including
  correlated predicates on `run_id`, with tenant isolation preserved.

### REQ-007 Deployment sealing keyring for managed secrets

Revised in r2 per owner directive 2026-10-14: reuse the deployment sealing
keyring.

- Gateway `managed_secret` credentials are sealed and opened with the
  deployment `SealingKeyring` (`WYRD_SEALING_KEY_FILE` /
  `WYRD_SEALING_KEY_BASE64`, retained keys for rotation), exactly as the other
  stored sealed secrets: same envelope, same boot rewrap. Each sealed value is
  bound to its tenant, name, and provider, so it opens only in its own row.
- The separate gateway keyring (`gateway.managed_secret_keys` and its
  per-tenant key plumbing) is removed with no compatibility path.
- `wyrd server dev --tenant <slug>` gives a fresh deployment a sealing key
  before first boot: unless the caller supplies one, it creates an owner-only
  key in the installed server directory on first run and reuses it on later
  runs, so stored credentials keep decrypting. No restart is needed. The key
  and stored secrets never appear in logs or output.
- The docs quickstart's credential step works as written.

## Invariants

- INV-001 Tenant isolation holds on every changed path.
- INV-002 Provider credentials and keys are never logged or returned.
- INV-003 Audit stays non-blocking; no new audit failure mode.
- INV-004 No compatibility aliases for the removed Jev chat shim; the r3
  unified-route pass-through is the only chat entry to Jev, and it carries the
  typed Jev request, not a translation.

## Acceptance obligations

- AC-001 Each REQ has a user-journey test (Rust, Python, and TypeScript where
  the surface ships in all three; MCP/HTTP where agent-facing) that fails on
  the base and passes on the candidate, booting the server the way users do
  where the defect was harness-masked (REQ-003 uses the official TypeScript
  MCP SDK; REQ-007 drives the CLI).
- AC-002 Negative flows: Jev invalid question type and upstream 400/429/5xx;
  Bifrost unauthorized-table errors leak nothing; malformed `traceparent`.
- AC-003 Format, lints, codegen check, and the owning `mise` lanes pass.
- AC-004 `wyrd-example-ts` runs clean against the candidate with no
  workarounds (no client-side escaping, no tool allow-list needed for schema
  compatibility).

## Open material decisions

None.

## Revision history

- r1 2026-10-14: initial, from the owner's directive.
- r2 2026-10-14: owner directive 2026-10-14: reuse the deployment sealing
  keyring. REQ-007 replaces the per-tenant gateway keyring and the
  restart-on-new-key behavior with the deployment sealing keyring, which
  `wyrd server dev` provisions before first boot.
- r3 2026-10-14: owner approval, given directly: "I approve jev r2". Allows the
  unified-route Jev pass-through, relays an upstream Jev 401/403 like other
  providers, and relaxes INV-004 to match.
