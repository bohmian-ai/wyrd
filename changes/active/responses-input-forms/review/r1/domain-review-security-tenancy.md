# Security and tenancy domain review

Result: **PASS** for the reviewed security/tenancy boundary. No material security or tenancy findings proposed. This is a source-backed boundary assessment, not a claim that the required new-form user journey or aggregate gates have passed.

## Immutable subject and scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/responses-input-forms`.
- Base: `807c4fe68`.
- Candidate: `6294bf6bbc0acb9abc37e4e4d617e2638f9dc3f7`; HEAD matched this candidate at review completion.
- Approved specification and original task: `changes/active/responses-input-forms/spec.md`, revision 1.
- Reviewed complete cumulative diff: `candidate.diff` and `git diff 807c4fe68 6294bf6 -- crates/skald`, with generated projection changes inspected in the cumulative diff.
- Boundary: newly accepted Responses input crosses untrusted request decoding into the existing authenticated, authorized, tenant-bound gateway and provider forwarding path. No implementation edits made.

## Governing authority and coverage

| Authority | Applicable requirement | Assessment |
|---|---|---|
| Spec INV-001 | Tenant isolation holds on every changed path | PASS by source tracing below |
| `AGENTS.md` §§2, 9 and `architecture/agent-rules.md` | Server owns authz/tenancy; tenant SQL uses `TenantConn`; permissions precede work; audit stages without blocking | Existing owner remains authoritative; no alternate database, identity, or audit path added |
| `architecture/wyrd-security-posture.md`, Security principles / Trust boundaries / Authorization / Tenant and data isolation | Tenant comes from verified credentials, never body; typed permission gates gateway; RLS bounds SQL; provider secrets remain server-selected | PASS; decoded messages and tool outputs cannot change effective principal or tenant |
| `architecture/wyrd-design.md` Doctrine 18 and 20, `architecture/wyrd-doctrine.mdx` | Auth remains separate from application data; new user-facing behavior requires journey proof | No identity-plane change; journey proof limits recorded below |
| `architecture/references/doctrine/architecture-constraints.md` | Multi-tenant logical server, SSRF-screened/pinned endpoint transport, real journeys | No endpoint transport changes; input content remains provider data |
| `architecture/references/languages/spec-driven-development.md` and `maintainer-style.md` | Source-backed acceptance; no speculative hardening or scope expansion | Review restricted to reachable changed behavior |

The reference router was inspected. CodeGraph was used before source searches for the gateway route, validation, invocation, and dispatch owners. Where its exploration did not show the requested source body, the owning file was read directly.

## Source and caller coverage

| Surface | Source inspected | Boundary conclusion |
|---|---|---|
| Input decoding | `crates/skald/skald-spec/src/wire/openai_responses.rs:282-335`, `:387-458`, `:473-490`, `:683-717` | `easy_items` rewrites only a local serde value, then uses the existing `OpenAiResponseItem` decoder. Output form is a closed text/parts enum. No retained state, IO, identity resolution, credentials, or SQL are introduced. |
| Changed sibling producers/consumer | `skald-agent/src/request_builder.rs`, `skald-prompt/src/prompt.rs`, `skald-prompt/src/python/openai_responses.rs`, changed `loop_responses.rs` test | Existing string-producing callers wrap results as text. Python's text projection is application data, not authorization context. No changed path owns durable tenancy. |
| Generated contracts | Nine Wyrd schemas, two eval fixture schemas, Python `_card_types.py`, TypeScript `card-types.ts` | Added type describes request content only; no tenant/principal/credential capability fields added. |
| Authentication | `crates/wyrd/wyrd-server/src/components/auth/caller_extractor.rs:33-74` | `Caller::from_authenticated` takes tenant and principal together from one verified `AuthenticatedPrincipal`; payload input is not consulted. |
| Public Responses ingress | `gateway/routes.rs:915-929`, `:1341-1355`, `:1723-1739`, `:1779-1819` | Typed validation is mandatory; invalid decoding returns request error. Success returns the original JSON `Value`, rather than re-encoding normalized Skald items. Authentication failure never calls invocation. |
| Authorization and admission | `gateway/invocation.rs:423-485`, `:505-556`, `:649-682`, `:701-775` | Requested-model permission is checked/audited before snapshot, admission, credentials or dispatch. Fallback candidates get their own checks. Principal, tenant, attribution, body and selected model remain separate fields. |
| Tenant persistence | `gateway/service.rs:656-678`; `gateway/invocation.rs:742-801` | Snapshot, Card attribution, admission and accounting acquire `tenant_conn` from authenticated tenant. Decoder adds no SQL and widens no connection capability. |
| Provider execution and credentials | `crates/wyrd/wyrd-gateway/src/engine.rs:576-631`; `adapter/http.rs:98-209`, `:238-263`, `:339-360`, `:638-676` | Resolver receives admitted tenant and snapshot. Dispatch constructs provider client from deployment and selected credential; caller headers are not forwarded. Endpoint policy transport is unchanged. |
| Body preservation | `adapter/mod.rs:149-222`, `:531-541` | OpenAI/compatible Responses uses native JSON preparation. Only model projection is rewritten to deployment model; easy input and parts remain structurally unchanged through raw provider dispatch. No URL in a content part is fetched by the new decoder. |
| Existing security/forwarding proofs | `gateway/pg_invocation_tests.rs:647-756`, `:763-840`, `:1855-1941` | Existing tests cover tenant-specific managed keys, copied-envelope refusal, denial before dispatch, and typed Responses raw-body preservation. These are supporting source evidence, not newly executed results. |

## Reachable path assessment

1. A client submits the new easy message and/or content-parts function result to `/v1/responses`. The unchanged HTTP stack bounds the request body (`http/router.rs:199`); the typed contract delegates input items to the changed decoder.
2. Decoder normalization changes only its private temporary values. A malformed item still must decode into the existing tagged item/content-part enums. The returned body at `typed_body` is the caller's original JSON value.
3. Authenticated `Caller` determines effective principal and `data_tenant_id`; neither an added message field nor a content-part field reaches tenant selection. Model projection remains separately parsed and governed.
4. Requested model is authorized and audit is staged. Snapshot/attribution/ledger operations use the authenticated tenant through `TenantConn`. Unauthorized fallback models are removed before provider work.
5. The retained admitted tenant selects credentials. Native provider request preparation copies the body and changes only `model`. Buffered and streamed dispatch serialize that prepared raw value.
6. Failed decoding, authentication, authorization, missing deployment, or credential resolution refuse the applicable request/attempt. New decoding introduces no shared state, worker, cache, durable transition, or process failure branch.

Reuse assessment: the candidate extends the existing Skald wire decoder consumed by `GatewayResponsesRequest`; it does not duplicate the existing `GatewayInvocation`, tenant snapshot, ledger, or provider pipeline. Existing tagged-item decoding still enforces item shape after easy-message normalization. There is no reason to add a second tenant guard or security owner for these pure transformations.

## Verification limits

No tests or generators were run by this domain reviewer, to avoid duplicate verification with the orchestrator. Initial supplied verification evidence was empty; centrally run checks should be attached to the final verdict. Existing gateway forwarding tests exercise text shorthand and typed item arrays, but not the newly accepted easy-message/content-parts combination. The approved spec itself records the absent TypeScript Agents SDK gateway journey. That limits runtime acceptance evidence; it does not establish a changed tenant escape or authorization bypass. The task and standards reviewers must reconcile the journey requirement against stronger repository authority.

No external/version-sensitive protocol claim is needed for this security conclusion: it rests on repository source and approved authority. This report makes no claim that all provider-accepted input variants are modeled correctly.

## Proposed findings

Empty. No changed reachable security, tenant-isolation, credential-selection, or audit regression was found. Optional hardening and unrelated existing debt were excluded.
