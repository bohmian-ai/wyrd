---
id: SPEC-gateway-deployment-access-priority
revision: 1
status: draft
---

# Tenant deployment access and production priority

## Intent

Two services in one tenant can use the same provider/model through different team-owned provider credentials without reaching each other's deployment. A production service can receive preferential admission when it shares a constrained provider quota. This backlog draft does not authorize implementation.

## Definitions

- **Deployment:** tenant-administered gateway configuration binding a model, provider adapter, capabilities, and one provider credential; it is not a Card or a provider key supplied by a caller.
- **Quota pool:** the provider capacity shared by deployments according to the provider's actual quota boundary, which may be an account, organization, project, region, model, or purchased reservation; credential identity alone does not define it.
- **Production class:** a tenant-administered admission class assigned through Wyrd authority, not a caller-declared request flag.

## Required behavior

- **REQ-001 — Multiple tenant credentials.** The tenant administrator can create separate credentials and deployments for service A and service B, including Google credentials from different projects. Credentials remain write-only/redacted and tenant-bound. A deployment continues to bind exactly one credential.
- **REQ-002 — Deployment RBAC.** Extend the existing typed `gateway:invoke` object scope with an exact tenant deployment grant. The administrator can create/update tenant roles and change an existing service principal's role assignments through audited, headless management surfaces. Service A granted only deployment A can invoke its model through A and never through B; service B has the converse behavior. Existing provider/model grants remain deliberately broad over matching deployments, so a service meant to be isolated must not also hold such a broad grant. No deployment-owned principal allowlist or second authorization system is introduced.
- **REQ-003 — Default selection.** A client may request only the model and operation. Before admission or provider IO, the server selects among compatible deployments covered by that caller's verified RBAC grants. Every fallback and retry is confined to authorized deployments. Zero eligible deployments fails with a stable authorization outcome and no upstream attempt. Listing/discovery exposes a model only when the caller has at least one eligible deployment.
- **REQ-004 — Optional explicit selection.** A client may additionally request one exact deployment through an optional `Wyrd-Deployment` HTTP header. Wyrd SDK inference calls, where exposed, project the same selector as an optional typed call option; this change does not require a new SDK inference API solely for selection. The selector is a routing preference, never authority: the server verifies tenant, model, operation, active state, and deployment-scoped or broader applicable permission before admission or provider IO. If authorized, the call is pinned to that deployment and cannot silently fall back to a sibling or another model. A missing, foreign, disabled, incompatible, or unauthorized selection fails with a stable non-disclosing error. Omitted selection retains REQ-003 behavior. Unmodified provider SDK calls need no selector.
- **REQ-005 — Authorized model fallback.** An unpinned request may use the tenant's ordered fallback policy for the exact requested model, operation, or global default, including a different provider such as OpenAI to an OpenAI-compatible GLM deployment. Every candidate must support the requested operation and ingress body and be covered by the caller's deployment or broader gateway grant. A retryable outage, timeout, throttling, or pre-dispatch failure may advance to the next eligible deployment/model within the original deadline; non-retryable validation/authentication rejection and failure after visible stream output do not silently switch providers. If no authorized, compatible fallback remains, return the stable terminal error. A pinned deployment retains REQ-004's no-fallback semantics.
- **REQ-006 — Quota-aware limits.** Admission can group deployments that share a real upstream quota pool even if they use different keys. Different Google projects can remain separate pools; keys in one Gemini API project cannot claim independent capacity. Tenant administrators can define bounded provider/account/project capacity without exposing secret material to callers.
- **REQ-007 — Production priority.** For a shared pool, the administrator can assign a production class and reserve configurable portions of concurrent slots and known request/token throughput for it. A bounded waiting queue serves production requests before standard requests when capacity becomes available, FIFO within each class; standard requests may use only unreserved capacity. Accepted work is never preempted. Queue length, wait deadline, cancellation, shutdown, and provider throttling have explicit finite outcomes; queued work does not consume a provider slot or spend budget until admitted. Admission order and reservations hold across server replicas. Distinct quota pools do not block each other.
- **REQ-008 — Accountability.** Decisions and outcomes identify the caller, requested model, resolved model/deployment, quota pool, class, queue delay, rejection reason, and credential reference without logging provider secret bytes. Every upstream attempt retains its outcome and usage. A cross-model fallback is explicitly reportable when resolved model differs from requested model, separate from a retry on another deployment of the same model; no redundant persisted boolean is required when those identities and attempts are retained. Gateway invocation authorization uses the existing non-blocking canonical audit path; administrative role, grant, and deployment writes use transactional audit.

## Constraints and expensive decisions

- **INV-001:** Tenant identity comes only from the verified Wyrd principal. Neither selector nor model name chooses a tenant, credential, or permission.
- **INV-002:** Deployment grants use the existing role-derived permission set and typed object scope. A caller must not gain another deployment through routing weights, fallback, retry, batch continuation, or stale cached administration state.
- **INV-003:** Priority cannot promise provider-side service priority or exceed an upstream quota. Unknown capacity requires conservative admission or explicit best-effort status, never a fabricated guarantee.
- **INV-004:** Waiting is bounded across replicas; process-local queue ordering alone cannot be presented as tenant-wide priority.
- **INV-005:** This draft intentionally changes the gateway-port spec's current rule that public callers do not select deployments. Only an **authorized optional** selector is added; provider credentials remain unselectable and secret.

## Acceptance

- **AC-001:** Real service principals A and B, the same Gemini model, and two deployments with distinct mock provider keys prove default routing, explicit selection, denial, fallback, model listing, and cross-tenant isolation. Upstream never receives the other team's key.
- **AC-002:** The administrator can create/update the required roles and assignments using public headless management; a refreshed token reflects grant changes while an already-issued token retains its documented lifetime semantics.
- **AC-003:** With shared quota and multiple gateway replicas, production requests obtain reserved capacity under standard load; standard traffic has a finite admission path, and cancellation/timeout releases queue state. Metrics distinguish queue wait from execution.
- **AC-004:** Distinct Google-project pools and multiple keys in one project behave according to their configured shared-capacity identity, not key count.
- **AC-005:** A real client requests an OpenAI chat model, a mock OpenAI deployment returns a retryable outage, and an authorized OpenAI-compatible GLM deployment completes the call without changing the client request. The call record preserves requested OpenAI and resolved GLM identities, both attempt outcomes and usage, and a distinct cross-model fallback metric. A caller without GLM access never reaches it; a pinned OpenAI deployment does not fall back. A stream that already emitted content terminates instead of switching models.

## Open material decisions

None. The queue storage/wake mechanism and exact administrative configuration shape are implementation choices, provided the public selector and the stated cross-replica semantics hold.

## Authority and revision

- `AGENTS.md` §§2, 9, 11; `architecture/wyrd-design.md`; `architecture/wyrd-security-posture.md`; `architecture/operations/reliability-and-recovery.md`; `changes/active/wyrd-gateway-port/spec.md`.
- Provider quota grounding: [Gemini API rate limits](https://ai.google.dev/gemini-api/docs/rate-limits) and [Vertex Provisioned Throughput](https://docs.cloud.google.com/vertex-ai/generative-ai/docs/provisioned-throughput/measure-provisioned-throughput).
- Example fallback provider grounding: [Z.AI GLM Chat Completions](https://docs.z.ai/guides/capabilities/mcp-call). A supported request must still qualify through Wyrd's actual adapter and operation checks.
- Revision 1 — draft, 2026-09-25: team-isolated RBAC, optional authorized selection, and production admission.
