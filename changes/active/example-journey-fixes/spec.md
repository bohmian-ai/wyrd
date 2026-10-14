---
id: SPEC-example-journey-fixes
revision: 2
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
- REQ-007 Local server setup handles the gateway managed-secret keyring.

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
- No Jev path tunnels a JSON document through an OpenAI chat message string,
  and no raw `serde_json::Value` stands in for the Jev request or response
  body. The rejected chat-route shim is removed.
- The gateway serves Jev on its native route with typed validation; tenant
  credentials, deployments, governance, capture, fallback isolation, and
  stable error mapping (400/422 invalid request, 429 throttled, other
  upstream failures unavailable, upstream bodies never relayed) behave as for
  other providers.
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
- Every Verifier execution records its requesting principal: the
  verification result carries `requested_by_principal_id`, and
  `wyrd.verifier_runs` records the principal a scheduled or observation run
  executes as. The result joins its judge calls on
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

### REQ-007 Local keyring

- `wyrd server dev --tenant <slug>` leaves a deployment where
  `wyrd gateway credential put` with a `managed_secret` source succeeds with no
  manual configuration: it creates an owner-only per-tenant managed-secret key
  for the tenant it provisions, configures the server with it, and restarts
  the server when the key did not exist at start. Later runs reuse the key, so
  stored credentials keep decrypting. The key never appears in logs or output.
- The docs quickstart's credential step works as written.

## Invariants

- INV-001 Tenant isolation holds on every changed path.
- INV-002 Provider credentials and keys are never logged or returned.
- INV-003 Audit stays non-blocking; no new audit failure mode.
- INV-004 No compatibility aliases for the removed Jev chat shim.

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
- r2 2026-10-14: REQ-004 amended by owner directive 2026-10-14 (attribution
  from execution context): the requesting principal is always recorded;
  `run_id` and `card_uid` are independent and optional, recorded when an
  invocation carries them; judge calls carry the execution's subject Card and
  optional Run; results record the requesting principal and join their judge
  calls on the subject Card; no new authorization. The rule "without a Run,
  attribution stays absent" is dropped.
