---
id: SPEC-gateway-prompt-caching
revision: 1
status: draft
---

# Provider prompt caching through the gateway

## Intent

Let teams reduce repeated prompt-token cost through provider-supported caching while seeing cache usage and charges accurately in Wyrd. This backlog draft does not authorize implementation.

## Required behavior

- **REQ-001 — Native cache controls.** For each supported provider and ingress dialect, Wyrd forwards valid inline provider-native prompt-cache controls or translates a documented equivalent when semantics match. It rejects unsupported or non-equivalent controls with a stable error; it does not silently discard them or claim a cache hit. Normal routing, fallback, and streaming preserve the chosen provider's cache semantics.
- **REQ-002 — Provider capability.** The gateway publishes which provider/model/operation combinations support automatic prefix caching, inline cache breakpoints, TTL or retention choices, and cache-key controls. Models without a documented control continue to work without one. Provider-specific controls stay provider-specific when no honest common contract exists.
- **REQ-003 — Accounting.** Capture provider-reported cache-read tokens, cache-write tokens, uncached input tokens, output tokens, and applicable storage/retention charges when supplied. Pricing and budgets apply the configured provider/model rates to those distinct dimensions. Unknown or omitted cache usage is reported as unknown, not zero or a free hit. Cache savings reporting compares documented billed cost with the corresponding uncached price, with assumptions visible.
- **REQ-004 — Isolation and safety.** No request may expose another tenant's prompt or credential. A caller-supplied cache key or control is validated and sent only through its authorized deployment/provider. Authorization, audit, admission, and usage accounting run for every invocation whether the provider reports a cache hit or miss.
- **REQ-005 — Provider evolution.** Provider cache metadata and usage parsing tolerate a provider omitting optional fields while rejecting malformed values. Documentation states the provider's cache lifetime, scope, hit guarantees, quota behavior, and any write/read/storage charges as versioned external facts, without promising savings or hits Wyrd cannot control.

## Constraints and decisions

- **INV-001:** The initial capability uses provider-managed prompt caching and inline request controls. It does not create or manage explicit provider cache resources, retain prompt bodies or completed responses in a gateway response cache, or synthesize a prior inference response.
- **INV-002:** Secrets, raw prompts, provider cache handles, and tenant identities are not metric labels or publicly exposed cross-tenant values. Provider-specific payloads remain behind their owning adapters.
- **INV-003:** Existing valid native provider requests are not broken by a gateway-level cache switch. Unsupported cross-provider translation fails explicitly rather than changing semantic meaning.
- **INV-004:** A provider cache hit does not bypass Wyrd limits, audit, capture, or budget accounting; provider quotas may still count cached tokens.

## Acceptance

- **AC-001:** Credential-free mock-provider journeys prove supported native controls and a cache hit/miss usage sequence through HTTP and first-class SDK surfaces that expose the operation.
- **AC-002:** Unsupported controls, invalid cache keys, and malformed usage fail or remain unknown as specified, without leakage or false savings.
- **AC-003:** Accounting and cost reports distinguish cached reads, writes, ordinary input, and output, including a provider response that omits cache fields.

## Open material decisions

None. Explicit provider cache-resource management or a gateway-owned response cache would require separate public lifecycle, identity, safety, and billing contracts.

## Authority and revision

- `AGENTS.md` §§9–11; `architecture/wyrd-security-posture.md`; `architecture/references/architecture/patterns.md`.
- Provider grounding: [OpenAI](https://developers.openai.com/api/docs/guides/prompt-caching), [Anthropic](https://platform.claude.com/docs/en/build-with-claude/prompt-caching), [Gemini](https://ai.google.dev/gemini-api/docs/generate-content/caching), [Vertex](https://docs.cloud.google.com/gemini-enterprise-agent-platform/models/context-cache/context-cache-overview), [DeepSeek](https://api-docs.deepseek.com/guides/kv_cache/).
- Revision 1 — draft, 2026-09-25: provider-managed prompt caching and accurate accounting.
