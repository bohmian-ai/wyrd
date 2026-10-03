# OAuth/OIDC Security Domain Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `134f605367e65b41f1977d6c70ac8ca8b277a69e`
- Candidate: `323ce32118ec72752a7736b8d42dd957abf6a094`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-005-qualification-and-docs.md`
- Remediation: `changes/active/oidc-production-readiness/review/TASK-005-r1/TASK-005-R1-doc-contract-accuracy.md`

The candidate remained at the stated commit throughout review. The repository has no `.codegraph/` directory, so review used the cumulative Git diff, remediation diff, `rg`, and direct source inspection.

The standing decisions were treated as closed: RFC 8693 API-key exchange with `urn:wyrd:oauth:token-type:api_key`; ingress rate limiting for `POST /auth/device`; best-effort RFC 7009 logout revocation; origin-normalized client URLs; and self-contained access-token validity through expiry.

## Reviewed boundary

This review traced:

- the four OAuth form endpoints and their client-identification, request, success, refusal, caching, and logging contracts;
- tenant versus platform credential exchange and authorization planes;
- workload-only trusted issuers, human-issuer refusal, secret-bearing issuer sealing, and machine bindings;
- the BFF’s encrypted cookie, client-secret-derived encryption key, access-token cache, logout, and cross-replica statelessness;
- tenant connection testing, persisted exact-revision test stamps, activation, provider outages, and recovery-key enforcement; and
- the security and operator documentation consumed by OAuth clients, agents, maintainers, and deployment operators.

## Authority and source coverage

| Boundary | Authority | Source and consumer evidence | Result |
| --- | --- | --- | --- |
| OAuth wire and errors | REQ-021, AC-009; security posture | `auth/oauth.rs`, tenant/platform token handlers, device and revocation handlers, shared-client error mapping, API and agent docs | PASS except `SEC-R2-001` |
| Platform/tenant separation | INV-003, REQ-018, AC-009 | tenant router, platform router, operator and concepts docs | FAIL — `SEC-R2-001` |
| Trusted issuer scope and sealing | REQ-005, REQ-013, INV-003 | boot seeding, admin creation, issuer serialization/sealing, cloud-identity and configuration docs | PASS |
| BFF credential custody | REQ-009, REQ-010, AC-007 | `browser-sessions.ts`, authentication and SSO docs, security posture | PASS |
| Connection activation | REQ-003, REQ-014, REQ-018 | connection owner, SQL test-stamp queries, lifecycle documentation | PASS |
| Closed operational decisions | User standing direction and remediation constraints | OAuth, BFF, CLI, client, and documentation paths | PASS |

## Prior-finding closure

| Prior finding | Closure evidence | Result |
| --- | --- | --- |
| `FIND-TASK-005-1` | OAuth pages and rustdoc now distinguish protocol-native errors from `WyrdError` conversions; only the latter promise a logged Wyrd code. Generator and generated pages agree. | PASS |
| `FIND-TASK-005-2` | Trusted issuers are described as workload-only; Human entries are routed to tenant connections; secret variants require the sealing key. | PASS |
| `FIND-TASK-005-3` | The operator authentication guide now distinguishes tenant `/auth/token` from platform `/auth/platform/token`, but sibling public/source documentation still collapses the planes. | INCOMPLETE — `SEC-R2-001` |
| `FIND-TASK-005-4` | Token, device-authorization, and revocation successes now name their distinct RFC response contracts. | PASS |
| `FIND-TASK-005-5` | Documentation now distinguishes the stateless Wyrd API boundary from the encrypted cookie owned by the BFF. | PASS |
| `FIND-TASK-005-6` | Activation documentation now states that activation reads the persisted 15-minute stamp and does not re-probe provider liveness. | PASS |

## Security Audit

### Critical

None.

### High

None.

### Medium

- `SEC-R2-001` — [`docs/src/content/docs/concepts/identity-and-auth.svx:88`](../../../../../docs/src/content/docs/concepts/identity-and-auth.svx) still says all machine-facing token issuance uses `POST /auth/token`, while platform credentials are accepted only by `POST /auth/platform/token`. In addition, [`crates/wyrd/wyrd-server/src/auth/oauth.rs:3`](../../../../../crates/wyrd/wyrd-server/src/auth/oauth.rs) says all four OAuth form endpoints identify a registered client through `OAuthClients`, but `platform_token` has no `OAuthClients` input or client registration: it directly validates an RFC 8693 API-key subject token through `PlatformSessions`. A platform operator or maintainer following these descriptions can send a platform credential to the tenant endpoint or incorrectly add `wyrd-cli`/`wyrd-ui` client authentication to the platform exchange. The request fails closed, but the documentation still misstates the security-plane and credential boundary that `FIND-TASK-005-3` was meant to reconcile. Qualify the concepts page as tenant-plane issuance and document the separate platform route; qualify the shared OAuth rustdoc so `/auth/platform/token` is form-encoded but authenticates the presented platform credential rather than a registered OAuth client. Reuse the existing route owners and add no alias, fallback, client registration, or shared-plane abstraction.

### Low / Defense In Depth

None. No placement, naming, wording, or optional-hardening preferences are reported.

### Positive Controls

- Protocol-native OAuth errors remain standard RFC error JSON without a fabricated Wyrd identity; catalog-originated refusals retain their existing structured log correlation.
- The token endpoints, device authorization, and revocation preserve distinct standard success shapes and `no-store` responses.
- The platform credential exchange is physically separate from tenant issuance and rejects unsupported grant shapes.
- Human trusted issuers fail before provider or storage IO; workload issuer secrets are sealed before persistence, and plaintext is never written.
- The BFF uses `openid-client`, PKCE S256, state, exact redirect binding, and `jose` A256GCM encrypted Secure, HttpOnly, SameSite=Lax cookies.
- Logout clears local state before best-effort revocation and does not promise that issued access tokens disappear.
- Candidate activation requires an exact, current test stamp and a live tenant recovery key, under the connection-slot transaction, without falsely treating activation as a provider liveness probe.
- No dependency manifest or lockfile changed, so this task introduces no new supply-chain surface.

## Material proposed finding

### SEC-R2-001 — INCORRECT: sibling documentation still collapses platform and tenant credential exchange

- **Violated obligation:** INV-003, REQ-018, REQ-021, AC-009, and remediation acceptance criterion 3 require platform and tenant authorization planes and routes to remain distinct and accurately documented.
- **Locations:**
  - `docs/src/content/docs/concepts/identity-and-auth.svx:86-92`
  - `crates/wyrd/wyrd-server/src/auth/oauth.rs:3-6`
- **Evidence:** `components/auth/routes.rs:70-188` owns tenant `/auth/token`. `components/platform/routes.rs:54-142` separately mounts `/auth/platform/token`, accepts no `OAuthClients`, and exchanges only the platform API-key subject token through `PlatformSessions`.
- **Observable consequence:** A platform operator can follow the concepts guide to the tenant route and receive a refusal. A maintainer or generated-document consumer can incorrectly infer that platform exchange requires one of the tenant OAuth client registrations, weakening understanding of the deliberate plane boundary.
- **Required testable correction:** Correct both sibling descriptions at their current owners. Scope the concepts-page dispatch to tenant-plane issuance and name `/auth/platform/token` for platform credentials. State in the OAuth module rustdoc that the platform endpoint shares the form/error/cache wire but authenticates the platform subject credential directly, not through `OAuthClients`. Preserve the two route owners and existing RFC 8693 API-key exchange; add no alias, fallback, or client requirement.
- **Focused proof:** Statically compare the corrected text with both router builders and handler inputs. Run only `mise run docs:check`, `mise run fmt`, and `mise run lints`. No journey or aggregate is required.

## Verification limits

Recorded focused checks passed: `mise run docs:check`, `mise run codegen:check`, `mise run fmt`, `mise run lints`, and the recorded candidate diff check. These prove rendering, generation, formatting, and lint consistency, not semantic documentation accuracy.

No full journey, aggregate, live IdP, database, or browser suite was run or required. The cumulative candidate changes documentation and source documentation, not executable OAuth/OIDC behavior.

## Overall result

**FAIL**

The remediation closes the prior security findings except for the platform/tenant route finding’s sibling consumers. `SEC-R2-001` is a bounded documentation correction: the implementation fails closed, but the remaining text is wrong about the shipped authentication-plane contract.
