---
id: SPEC-gateway-providers
revision: 1
status: draft
---

# Gateway providers: Azure OpenAI check and Amazon Bedrock

## Problem

The gateway has native adapters for OpenAI, Anthropic, Gemini, Vertex and Jev,
plus `openai_compatible`, which reaches any provider serving the standard
OpenAI routes under a base URL. Two providers that enterprise ML teams are
often required to buy through are not proven or not reachable:

- **Azure OpenAI / Azure AI Foundry.** Azure's `/openai/v1/` routes appear to
  be OpenAI-compatible, with the key sent in an `api-key` header. If they are,
  `openai_compatible` with `ApiKeyHeader { header: api-key }` already serves
  Azure, but nothing proves or documents it.
- **Amazon Bedrock.** Bedrock serves Claude, Llama, Mistral, Nova and others
  through its own Converse API, its own streaming framing and AWS
  authentication. No adapter reaches it.

## Change

**REQ-001 — Azure OpenAI is proven through `openai_compatible`.** A contract
test drives the gateway's `openai_compatible` adapter against Azure's v1
request and response shapes: Chat Completions (plain, streamed, with tools),
Responses, and Embeddings, authenticated by an `api-key` header. Usage and
model identity are recorded as for OpenAI.

- If every shape passes, Azure needs no new adapter. The provider docs show an
  Azure deployment, and the change adds no code beyond tests.
- If a shape fails, the failure is recorded and this spec is revised with the
  smallest adapter change that fixes it before any implementation.

Microsoft Entra ID token authentication for Azure is out of scope for this
revision. API keys are Azure's default for these routes.

**REQ-002 — A Bedrock adapter.** A new built-in adapter, `bedrock { region }`,
with reserved credential provider identity `bedrock`:

- carries `ChatCompletions` only, as Anthropic, Gemini and Vertex do;
- translates a Chat Completions request to Bedrock `Converse`, and a streamed
  one to `ConverseStream`, including system messages, multi-turn messages,
  tool definitions, tool calls and tool results;
- translates the Bedrock response or stream back to Chat Completions, with
  token usage and stop reason mapped;
- addresses the model by Bedrock model ID or inference profile ID, such as
  `anthropic.claude-...` or `us.anthropic.claude-...`;
- maps Bedrock throttling, validation, access-denied and model-not-ready
  errors onto the gateway's existing retryable and terminal refusal classes,
  so fallback and failover behave as for other adapters.

**REQ-003 — Bedrock authentication.** A Bedrock deployment authenticates with a
Bedrock API key sent as `Authorization: Bearer`, using the existing `Bearer`
auth and a `bedrock` credential. See the open decision on IAM credentials.

**REQ-004 — Pricing.** Bedrock calls are priced like any other call, through
the gateway's pricing selection by model and admission time. The pricing-feed
change maps the `bedrock` provider to the genai-prices `aws` provider; until
then, tenant pricing overrides price Bedrock calls.

## Invariants

- **INV-001.** Adding the Bedrock adapter changes no existing adapter's
  behavior, wire contract, or published operation-support table entry.
- **INV-002.** A `bedrock` credential authenticates only a `bedrock`
  deployment, and no other adapter accepts it, as for every built-in adapter.
- **INV-003.** Tenancy, authorization, audit, capture, limits and budgets apply
  to Bedrock calls exactly as to other adapters.
- **INV-004.** No platform-specific mechanism; the adapter works identically
  on all three native targets.

## Scope

Non-goals:

- provider presets (named shortcuts that fill in base URL and auth for
  OpenAI-compatible providers);
- Azure Entra ID authentication;
- Bedrock embeddings, images, batch inference, agents, knowledge bases,
  guardrails, and the `InvokeModel` API;
- Anthropic on Vertex.

## Expensive-to-reverse decisions

- `bedrock` joins the reserved built-in provider identities, and
  `ProviderAdapter` gains a `bedrock { region }` variant. Both are public
  deployment contract and appear in generated schemas and SDK types.
- `ConverseStream` uses the AWS event-stream binary framing, not server-sent
  events. The adapter decodes it in Rust. Whether that is a small in-tree
  decoder or an existing crate is an implementation choice, but it must add no
  cloud SDK to a client-tier crate.

## Acceptance

- **AC-001.** Mock-server contract tests in the owning provider crate prove
  `openai_compatible` with an `api-key` header against Azure v1 Chat
  Completions (plain, streamed, tools), Responses and Embeddings shapes. The
  result is recorded in this packet.
- **AC-002.** Mock-server contract tests prove Bedrock `Converse` and
  `ConverseStream` translation in both directions: system, multi-turn, tools,
  usage, stop reasons, and each error class.
- **AC-003.** A gateway user journey registers a `bedrock` deployment and
  credential against a mock Bedrock endpoint, then makes plain and streamed
  calls through the Rust, Python and TypeScript SDKs, and observes the
  captured attempt with usage and cost. A negative path shows a `bedrock`
  credential refused for a non-Bedrock deployment.
- **AC-004.** `mise run codegen:check` passes with the new adapter variant,
  and the served OpenAPI contract test covers it.
- **AC-005.** Docs show an Azure deployment through `openai_compatible` (if
  AC-001 passes) and a Bedrock deployment.

## Open decisions

1. **Bedrock IAM credentials.** Bedrock API keys reuse existing `Bearer` auth
   and need no new dependency. Many AWS organizations instead require IAM
   roles: access key, secret and session token, or role credentials from the
   environment, signed per request with AWS SigV4. Supporting IAM adds a new
   auth kind and either a signing dependency such as `aws-sigv4` or an
   in-tree signer.
2. **Real-cloud verification.** Mock contract tests prove shapes, not that the
   live service still matches. A credentialed real-cloud lane for Azure and
   Bedrock, like `storage-integration-cloud`, would need cloud accounts and
   secrets.

## Authority

- `crates/wyrd-spec/src/gateway/deployment.rs`, `ProviderAdapter`.
- `AGENTS.md` §2 (client-tier dependency limits, native targets) and §11.

## Revision history

- Revision 1 (2026-10-22): draft.
