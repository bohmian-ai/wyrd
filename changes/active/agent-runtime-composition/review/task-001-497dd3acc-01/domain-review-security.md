# Domain review: security, authorization, and correlation

Subject: `06a368906187be20364a2d959da311151964b617...497dd3acc8fd06567a0e0ff01baf599f9af1711b`. Candidate HEAD remained `497dd3acc8fd06567a0e0ff01baf599f9af1711b` during this review.

Result: **FAIL** — two bounded migration regressions proposed for independent validation. No Critical or High exploit was identified.

## Reviewed boundary and authority

This review follows the removed SDK execution callers into application-owned requests, then through native ingress credential verification, tenant/model permission checks, asserted Card scope checks, audit staging, provider dispatch and error return. It also checks that deleting Python runtime exposure does not change the surviving server-controlled execution trust boundary.

| Authority | Applied obligations |
|---|---|
| `AGENTS.md`, `architecture/agent-rules.md` | Server-owned permissions/tenant decisions; secret handling; nonblocking audit; preserve meaningful surviving journey assertions. |
| `architecture/wyrd-security-posture.md` (complete) | Verified credentials select tenant; Card assertions require authorization; provider credentials stay server-owned; sensitive errors/telemetry do not expose secrets. |
| `architecture/wyrd-design.md` (identity, client, Agent, observation and gateway sections), `architecture/wyrd-doctrine.mdx` | Declarative Cards and application-owned inference; credential-derived identity and authorized optional Run/Card correlation. |
| Approved revision 3 `spec.md` | REQ-014/016/017; INV-001–005; AC-008/009/010. |
| Original task at base | Migrate real consumers with caller/Run/Card capture and server subject refusals; do not discard assertions about surviving behavior. |
| Reference router; `languages/spec-driven-development.md`, `languages/maintainer-style.md`, `languages/errors.md`, `languages/agent-harness.md`, `domain/telemetry-observations.md` | Acceptance scope, boundary error projection, sensitive payload handling, attributable evidence and unchanged MCP permission/audit ownership. |

## Source coverage

- Inspected the complete cumulative change inventory, all added execution/request/error hunks, security-bearing deleted `observe/invoke.rs`, `gateway_caller.rs` and `HttpTransport::post_native`, and their removed regression contracts. Reviewed runtime cfg/glue/dependency removals for changes to surviving Rust authority. Runtime-only Python deletion does not add a new execution path.
- Migrated owners: `examples/support-desk/{python/support_desk.py,rust/support_desk.rs,typescript/support-desk.ts}`, `examples/support-desk-app/rust/lib.rs`, SDK Rust/Python/TypeScript gateway-inference and support-desk journeys, and Python local-development/installed-CLI migration. Reviewed gateway-setup and observation guidance changes.
- Credential paths: existing `AuthMiddleware::bearer`/cached-token freshness in `crates/shared/wyrd-client/src/auth.rs`, surviving HTTP transport, Python `client/_http_auth.py`, and TypeScript `WyrdClient.accessToken/httpAuth`. Support-desk Rust requests fetch a current bearer per call; Python and TypeScript use existing per-request authentication adapters. Python gateway test helper constructs a short-lived client with a freshly requested token. No new token cache or issuer is introduced.
- Server receiving boundary: `components/gateway/ingress.rs` authenticates before handler execution, rejects ambiguous credentials, parses optional Run/Card/provider headers, and builds `GatewayCallRequest`; `routes.rs` is the OpenAI route owner. No server source changes occur in this range.
- `GatewayInvocation::admit_call` (`invocation.rs:452–521`) derives the tenant from `Caller`, takes requested-model permission before subject attribution/provider work, and retains verified caller identity. `decide` (`:706–755`) checks deployment/model grants and stages both allow/deny decisions on the existing outbox. `attribute` (`:771–788`) checks signed Card scope or reads observation-target membership under the caller's tenant; `is_observation_target` uses `TenantConn`. Run IDs remain optional opaque correlation, not authority.
- Provider boundary: `wyrd-gateway/src/adapter/http.rs::client` reconstructs provider clients from server-owned deployment credentials rather than copying incoming request headers; refusal handling calls existing `adapter/mod.rs::refusal`. Existing refusal scrubbing removes the resolved provider credential, including decoded/encoded cases. The migrated support-desk Python test adds upstream Wyrd-header absence assertions; Rust/TS gateway tests preserve upstream-secret/header checks.
- Internal sibling: unchanged `ServerWyrdGatewayCaller` retains accepted caller authority, resolved subject provenance, provider compatibility checks and redacted non-success handling. The candidate removes Python projections but does not change Vala/server call ownership, tools, audit, or route restrictions. No new MCP, tenant store, SQL, webhook, CORS, crypto or deployment configuration surface is added.
- Manifest and lock changes remove SDK runtime dependencies/features; no new package or version is added.

## Security Audit

### Critical

None identified.

### High

None identified.

### Medium

**SEC-001 — REGRESSION: migrated Python gateway Agent calls lose their authorized subject correlation.**

- Location: `sdks/wyrd-sdk-python/tests/integration/test_gateway_inference.py:49–55` (`answer`), consumed at `:209–210`; the weakened readback is `:217–220`.
- Violated obligation: REQ-016 and AC-009 require the migrated gateway-inference workflow to preserve Run/Card correlation; original task scenario 1 requires application-owned calls with captured Run/Card identity, rather than merely retaining caller identity.
- Producer-to-sink evidence: previously `state.run(alias).invoke(...)` created a Run and `PublicWyrdGatewayCaller.with_subject` sent both headers. New `answer` neither creates/accepts a Run nor sets either header. The same four successful model requests now enter `requested_subject` with no headers, producing `None`; gateway capture consequently has no Run/Card subject for them. The query checks only caller and deployment. The independent lone-Run/Card test exercises different manually constructed requests and cannot supply correlation to these requests.
- Reachability/consequence: the required `test_agent_prompt_routes` journey invokes this helper for both aliases and both deployments. Calls retain correct caller authorization but lose the Agent/Run evidence link, and those requests no longer exercise asserted-Card authorization. This is a required migrated workflow regression, **not** a claim that optional uncorrelated native calls are an authorization bypass.
- Minimum testable correction: create/use the alias's Run and send existing `wyrd-run-id`/`wyrd-card-uid` headers from it, following the existing Rust/TS migrated helpers or Python support-desk owner. Extend this journey's existing capture readback to match each successful call's Run and Agent Card and retain the existing grant-denial/no-upstream assertions. No new correlation state or SDK API is needed.

**SEC-002 — REGRESSION: the support-desk HTTP app now publishes arbitrary provider refusal details.**

- Location: `examples/support-desk-app/rust/lib.rs:147–150`; public response sink `:403–406`.
- Violated obligation: preserve the existing security/error boundary during REQ-016/017 consumer migration; `architecture/references/languages/errors.md` requires public-boundary conversion and forbids directly publishing provider error strings. The telemetry reference treats prompt text and related payloads as sensitive.
- Producer-to-sink evidence: `POST /ask` (`:260–268`) and `/reviews` call `SupportDesk::answer`, which now copies `reply["error"]` into `ApiError`; `IntoResponse` returns it to the app caller verbatim. The native gateway deliberately preserves provider error semantics: `GatewayInvocation::invoke` returns a final provider refusal as status/body, and `adapter/mod.rs:313–360` scrubs the resolved provider key but preserves the remaining native error document. It does not remove arbitrary internal provider diagnostics or echoed Prompt/system content. Previously this consumer used `Run.invoke`; deleted `gateway_caller.rs::Ingress::problem` replaced recognized codes' text with catalog messages and uncoded refusals with a fixed status message, explicitly keeping upstream text out. `ApiError::from(WyrdError)` therefore received the sanitized result.
- Reachability/consequence: a caller of the app's public routes can trigger a provider validation/refusal response; if that provider returns an error object containing hidden system Prompt text, deployment diagnostics or other upstream operational details, the app now discloses those fields. The app caller need not hold the Wyrd principal whose gateway token is used. This is a confidentiality regression at the application response boundary. The existing provider-key scrubbing means this report does **not** claim a demonstrated provider-key leak.
- Minimum testable correction: keep native gateway passthrough unchanged and restore safe projection at this application boundary. Preserve HTTP status and recognized stable Wyrd codes using the existing catalog; use safe catalog/fixed messages and omit arbitrary upstream fields. Do not restore SDK execution or introduce a general transport abstraction. A focused app-route check with a mock upstream error containing a sensitive marker should prove the marker is absent from the public response; a gateway permission refusal must still expose its stable code/status. The same check should include a provider body carrying a Wyrd-looking code so its arbitrary message cannot masquerade as trusted catalog text.

### Low / Defense In Depth

None proposed; optional hardening is outside this acceptance audit.

### Positive Controls

- Tenant and principal identity continue to come from verified Wyrd credentials, never correlation headers.
- Model/deployment permission and asserted Card scope checks precede provider dispatch. Denied grants remain rejected with no upstream request.
- Allowed and denied gateway invoke decisions use the existing nonblocking audit outbox.
- Provider credentials remain resolved server-side; upstream requests are reconstructed and do not forward client bearer or Run/Card headers.
- Python/TS authentication adapters fetch current Wyrd tokens per request and strip placeholder provider headers; explicit native gateway requests carry the original caller rather than substituting an internal principal.
- Internal server-controlled execution retains its accepted authority and existing redacted error path.
- Dependency removal reduces shipped Python execution reachability without adding supply-chain inputs.

## Verification limits

Source/evidence review only. No Cargo, build, runtime test or live provider calls were run, per orchestrator coordination. Candidate Implementation Evidence reports successful J-R/J-P/J-T, preservation, package and owner checks; these are claims, not independently executed evidence. Green current tests cannot disprove SEC-001 because their query omits the lost fields. The task evidence explicitly says the support-desk HTTP app is docs-only and is not compiled by a lane; SEC-002 is established from its public route/error source and the unchanged gateway refusal behavior, not a running deployment.

No unrelated pre-existing security debt or new authorization design is proposed. Full runtime deletion parity and non-security behavioral acceptance remain the other reviewers' scope.
