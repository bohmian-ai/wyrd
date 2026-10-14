---
id: SPEC-example-journey-fixes
revision: 6
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
- REQ-004 Verifier executions are attributed from their execution context.
- REQ-005 Gateway spans join the caller's trace.
- REQ-006 Bifrost subqueries over live tables work.
- REQ-007 Gateway managed secrets use the deployment sealing keyring, and
  local server setup provides it.
- REQ-008 Stock clients authenticate through the standard credential chain.
- REQ-009 Callers address a gateway deployment by name, and a deployment
  serves every model of its provider unless it lists models.
- REQ-010 Wyrd ships an OpenAI Agents SDK trace processor the user passes to
  the SDK.

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

### REQ-004 Verifier execution attribution

Owner directive 2026-10-14 (attribution from execution context). Verifiers may
execute at any time, not only within a Run, so attribution derives from the
execution's context, not from whether a Run exists.

- If an invocation carries a `card_uid` and/or a `run_id`, it is recorded;
  the requesting principal is always recorded. `run_id` and `card_uid` are
  independent: either, both, or neither may be present, an absent one is
  recorded as null, and neither is inferred or required to record the other.
- A gateway call in `vala.gateway.calls` records its `caller_principal_id`
  plus whichever of the `wyrd-run-id` and `wyrd-card-uid` headers it carried.
  The gateway's existing card-scope authorization applies to a `card_uid`
  whenever one is present.
- Every gateway model call a Verifier execution makes (for example an
  `llm_judge` call through `ServerWyrdGatewayCaller`) records the
  execution's subject Card as `card_uid` and its `run_id` when present, for
  direct, observation, and scheduled executions alike.
- Every Verifier execution records its actor as
  `requested_by_principal_id` on the verification result and on
  `wyrd.verifier_runs`. Actor = the authenticated caller for a manual run or
  a direct execution; actor = the tenant SYSTEM principal for work Wyrd
  starts internally (a scheduled or observation-triggered run). Origin =
  references only: `run_id`, subject `card_uid`, `source_record_id`, and the
  binding, owner, and Trigger; no second principal (such as the observation
  writer) is copied onto runs or results. An observation-triggered
  execution's span links to the observation's stored trace and span when the
  record has them. The result joins its judge calls on
  `subject_card_uid` = `card_uid`.
- No binding enforcement or new authorization is added; direct-execution
  authorization is unchanged.

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

### REQ-008 Stock clients authenticate through the standard credential chain (r5)

Owner directive 2026-10-14: "stop TASK-007 and do the accessToken exports".
The server accepts no new credential form. Stock OpenAI clients and stock MCP
clients reach the gateway and `/mcp` with the Wyrd access token the SDK already
exchanges, caches, and renews.

- TypeScript `wyrd-ai` exports `accessTokenSource(options?: ClientOptions)`.
  It resolves the client once (the ambient client when `options.client` is
  omitted) and returns `() => Promise<string>`, the current access token on
  each call. It plugs directly into the OpenAI SDK:
  `new OpenAI({ baseURL, apiKey: accessTokenSource() })`.
- TypeScript `wyrd-ai` exports `wyrdFetch(options?: ClientOptions)`, a `fetch`
  that resolves the client once and sets `X-Wyrd-Access-Token: Bearer <token>`
  on every request. It plugs into any client that takes a `fetch`, including
  the Agents SDK `MCPServerStreamableHttp`.
- `gatewayFetch` is removed with no alias; `accessTokenSource` covers its use.
- Python `GatewayAuth(client=None)` resolves the ambient client when `client`
  is omitted, once, at construction.
- Every credential source the client chain supports works through these
  helpers: `WYRD_ACCESS_TOKEN`, workload identity, `WYRD_API_KEY`, saved login.
- An unresolvable credential fails with the existing client error
  (`WYRD_CLIENT_401_NO_CREDENTIALS`), not an opaque provider error.

### REQ-009 Deployments are addressed by name and open by default (r6)

Owner directive 2026-10-14: an active provider makes all of its models usable;
a team can hold a second key for the same provider (for example provisioned
throughput) and address it separately; `gateway:invoke` is open by default and
a deployment can require an additional scope, like Bifrost permissions.

Deployment shape:

```yaml
name: openai-pt          # the prefix callers write: openai-pt/gpt-6-luna
adapter: openai          # upstream protocol
auth: {bearer: {credential: openai-pt}}
models: [gpt-6-luna]     # optional; omitted means every model
access: granted          # optional; omitted means open
```

- A caller's model string is `<deployment>/<model>`. The gateway resolves the
  one deployment with that name. The adapter, not the prefix, decides the
  upstream. A missing deployment, or a model outside a declared `models`
  list, fails with `GatewayModelUnavailable` before any provider IO.
- `models` is optional. Omitted, the deployment serves every model of its
  provider. `capabilities` is optional. Omitted, it is every operation the
  adapter serves.
- A deployment name is any valid deployment identifier. A deployment using a
  built-in adapter may have any name (`openai-pt`); the rule that a built-in
  adapter requires its reserved provider name is removed.
- Weighted selection is removed: `routing_weight` and weighted ordering no
  longer exist. Each model string names exactly one deployment. Failover
  between deployments is a fallback rule.
- Fallback rules, pricing, and governance limits name `<deployment>/<model>`,
  so a provisioned-throughput deployment can carry its own fallback (for
  example `openai-pt/gpt-6-luna` falls back to `openai/gpt-6-luna`) and its
  own prices.
- Aliases: a tenant may define an alias, a name that resolves to one
  `<deployment>/<model>` target. A caller may send the alias as its model
  string. Authorization, pricing, and fallback apply to the target. An alias
  name never contains `/` and never shadows a deployment.
- `/v1/models` lists the models of deployments that declare `models`, plus
  aliases. A deployment that serves every model is not enumerated; the
  gateway never probes upstream.
- A Prompt Card may name `spec.deployment`. Omitted, the deployment is the
  built-in name of the Prompt's provider (`openai`, `anthropic`, `gemini`,
  `vertex`, `jev`, or the custom provider's name). Workflow and judge calls
  use that deployment, and the Workflow chooses the Google dialect from the
  deployment's adapter, not its name.

Authorization:

- `gateway:invoke` scopes are `all`, `{deployment}`, and
  `{deployment, model}`. The `{deployment}` scope replaces today's provider
  scope.
- An `open` deployment (the default) is covered by `all`, by a grant naming
  it, or by a grant naming one of its models. Built-in roles keep `all`, so
  every Service can call every open deployment.
- A `granted` deployment is covered only by a grant that names it or one of
  its models. `all` does not cover it. This mirrors the extra
  `gateway_payload_read` permission Bifrost requires for payload columns.
- The check runs against the call's subject principal. An uncovered requested
  target is refused with the existing invoke permission error; an uncovered
  fallback target is dropped from the plan. Both decisions are audited as
  today.

### REQ-010 OpenAI Agents SDK trace processor (r6)

Owner directive 2026-10-14: Wyrd works with OpenTelemetry instrumentation as
a standard OTLP backend, and provides a trace processor that the user gives
to the Agents SDK. Wyrd stays framework agnostic and never registers it.

- TypeScript `wyrd-ai` exports `WyrdAgentsTraceProcessor`, implementing the
  `@openai/agents` `TracingProcessor` interface. Python `wyrd` exports the
  same processor for `openai-agents`. Each converts Agents SDK traces and
  spans into OpenTelemetry spans on the active global tracer provider,
  preserving parent and child structure. Generation and response spans carry
  OpenTelemetry GenAI attributes (`gen_ai.request.model`,
  `gen_ai.usage.input_tokens`, `gen_ai.usage.output_tokens`).
- The user installs it with the SDK's own API, for example
  `setTraceProcessors([new WyrdAgentsTraceProcessor()])`. `startTelemetry()`
  neither exports nor registers it, and Wyrd never patches the Agents SDK.
- `@openai/agents` and `openai-agents` are optional dependencies; importing
  `wyrd-ai` or `wyrd` without them works.
- With `startTelemetry()` running inside `Run.scope`, the converted spans
  carry `wyrd.run_id` and `wyrd.card_uid` like any other span and land in
  `vala.traces.spans`.

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
- AC-005 (r5) The example constructs no `WyrdClient`: the gateway OpenAI
  client uses `accessTokenSource()` and the MCP connection uses `wyrdFetch()`.
  Journeys prove a stock OpenAI client (TypeScript and Python) and the
  official TypeScript MCP SDK succeed with only the ambient environment, and
  that a revoked API key is refused.
- AC-006 (r6) Gateway journeys in Rust, Python, and TypeScript: a deployment
  without `models` serves a model it never named; two deployments with the
  same adapter and different credentials are each reached by their prefix; a
  `granted` deployment refuses a principal holding only `all` and serves a
  principal granted that deployment; an uncovered fallback is dropped; an
  alias routes to its target; a Prompt with `spec.deployment` reaches that
  deployment from a Workflow or judge call.
- AC-007 (r6) TypeScript and Python journeys: an Agents SDK agent run with
  `WyrdAgentsTraceProcessor` installed and `startTelemetry()` running inside
  `Run.scope` produces spans in `vala.traces.spans` carrying the Run ID and
  GenAI model attribute. Importing the SDK without the Agents SDK installed
  succeeds.
- AC-008 (r6) `wyrd-example-ts` sets up the gateway with one credential and
  one deployment per provider, no capture step, and installs
  `WyrdAgentsTraceProcessor` instead of disabling tracing.

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
- r4 2026-10-14: REQ-004 amended by owner directive 2026-10-14 (attribution
  from execution context): the requesting principal is always recorded;
  `run_id` and `card_uid` are independent and optional, recorded when an
  invocation carries them; judge calls carry the execution's subject Card and
  optional Run; results record the requesting principal and join their judge
  calls on the subject Card; no new authorization. The rule "without a Run,
  attribution stays absent" is dropped.
  Actor and origin per owner-decisions.md OD-1: internal runs record the
  tenant SYSTEM principal as actor; origin is recorded as references.
- r5 2026-10-14: owner directive "stop TASK-007 and do the accessToken
  exports". Adds REQ-008 (ambient `accessTokenSource` and `wyrdFetch`, Python
  `GatewayAuth()` default, `gatewayFetch` removed) and AC-005. The server does
  not accept raw API keys on the gateway or `/mcp`.
- r6 2026-10-14: owner approval, given directly: "yes and then execute it",
  of the gateway and tracing recommendation. Adds REQ-009 (deployments
  addressed by name, every model by default, `granted` access, aliases,
  weighted routing removed, Prompt `spec.deployment`), REQ-010 (Agents SDK
  trace processor, independent of `startTelemetry`), and AC-006 to AC-008.
