# TASK-011 tenancy and authorization domain review

## Immutable subject

- Base: `7c48ac7c99f018d3993922e63875839f3695c503`
- Candidate: `0b8919fff090d4b0109a506711cb119214d231f2`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Task: `changes/active/oidc-production-readiness/tasks/TASK-011-bff-openid-client.md`
- Prior-finding closure in scope: `changes/active/oidc-production-readiness/review/TASK-010-r1/lead-direction-FIND-TASK-010-1.md`

The candidate commit resolved to the stated object throughout this review. The
reviewed source paths matched the candidate. No other review report was read.

## Reviewed boundary

This pass covered only tenant identity and authorization across the production
SvelteKit BFF: route tenant input, encrypted-cookie tenant metadata, Wyrd access
token tenant authority, `BrowserSession.context()` and API calls,
`hooks.server.ts` locals, tenant switching, forged and crossed cookies,
API-key exchange, settings reads and mutations, logout, and the relevant
real-server journey assertions.

The approved decisions were applied as authority:

- SSO is the routine primary action. API-key sign-in is a distinct recovery
  page.
- An API-key session's effective tenant is the exchanged key/token's tenant,
  not the recovery-page route. A route/token tenant mismatch is therefore not
  itself a refusal condition and does not require a new endpoint or claim.
- Logout revokes the refresh token and clears the cookie; an already issued
  bearer access token remains valid until its bounded expiry.

## Authority and source coverage

| Boundary | Authority | Source and consumer evidence | Result |
|---|---|---|---|
| Effective tenant comes from verified Wyrd authority, not a path or cookie label | `AGENTS.md` §§2, 9; `architecture/agent-rules.md`; `architecture/wyrd-security-posture.md` Security principles and Access and refresh tokens; spec INV-001 and INV-005 | `browser-sessions.ts:232-243,270-313` obtains access tokens only through `openid-client` token exchange or refresh. `BrowserSession.api` at `browser-sessions.ts:77-92` sends that bearer token to the server, whose verified principal remains the authorization boundary. | PASS |
| API-key recovery uses the key's tenant authority | Lead-approved TASK-011 decision 2; spec REQ-010; security posture `/auth/token` tenant derivation | `signInWithApiKey` does not send a tenant to token exchange (`browser-sessions.ts:232-243,281-292`). The recovery route contributes only the cookie/redirect key. Settings calls use `locals.browserSession.api`, not the route key (`settings/+page.server.ts:27-64,94-150`). The journey at `production-auth.integration.test.ts:535-545` proves another tenant's key cannot read or remove the route tenant's staged connection. | PASS |
| SSO and cookie sessions remain tenant-separated | Spec REQ-006, REQ-009, REQ-015, INV-001; task Scenario 1 and Scenario 3 | Login state is encrypted and binds tenant, PKCE verifier, and state (`browser-sessions.ts:186-229`). Each SSO session cookie seals its tenant; `sealed` requires equality with the requested cookie key (`browser-sessions.ts:315-335`). The journey refuses forged and copied cross-tenant cookies at `production-auth.integration.test.ts:351-405,599-611`. | PASS |
| Request locals and downstream authorization preserve server authority | Spec INV-005; security posture Authorization and policy | `hooks.server.ts:13-30` reads only the requested tenant cookie and installs the resulting `BrowserSession`. UI metadata is projected from access-token claims, while settings operations forward the private access token to Wyrd (`browser-sessions.ts:44-92`; `settings/+page.server.ts:27-64`). Permission display/gating cannot make a protected server operation succeed. | PASS |
| Tenant switch revalidates the target session | Spec REQ-015 | `BrowserSessions.switch` calls `read(target, ...)`, which decrypts the target cookie and renews/exchanges its credential before redirecting (`browser-sessions.ts:315-356`). The root action delegates to it (`routes/+page.server.ts:137-153`). The journey covers independent Keycloak/Dex sessions, a missing target session, forged metadata, and crossed cookies (`production-auth.integration.test.ts:427-431,548-611`). | PASS |
| Settings reads and mutations cannot cross tenant authority | Spec REQ-003, REQ-015, INV-001, INV-005 | All settings operations receive `event.locals.browserSession` and call Wyrd with its private bearer token (`settings/+page.server.ts:27-64,94-150`). No tenant header, route tenant, or browser field is forwarded as authority. Reader denial is exercised at `production-auth.integration.test.ts:413-417,495-503`; crossed-key isolation is exercised at lines 535-545. | PASS |
| Routine SSO and separate API-key recovery | Lead-approved TASK-011 decision 1; spec REQ-006 and REQ-010 | The SSO page owns only the `sso` action; the API-key form and action live at `/t/[tenantKey]/login/api-key` (`login/+page.server.ts`; `login/api-key/+page.server.ts:14-32`). The journey checks the primary SSO action, separate recovery link, and recovery availability with and without active SSO (`production-auth.integration.test.ts:419-425,463-496`). | PASS |
| Logout and bounded residual bearer authority | Spec REQ-009 and REQ-016; approved task material limit; security posture bearer-token replay model | `BrowserSessions.logout` clears the selected cookie, removes its local cached access token, and revokes only refresh credentials (`browser-sessions.ts:338-351`). API keys are intentionally not revoked. The two-login journey proves one refresh token is revoked without ending the sibling login (`production-auth.integration.test.ts:433-449`), while provider replacement proof accepts the bounded access-token lifetime before renewal refusal (lines 451-460 and 687-693). | PASS |
| FIND-TASK-010-1 closure through the production client | Lead direction `FIND-TASK-010-1`; TASK-011 acceptance evidence | `openid-client` performs authorization-code redemption, refresh, and revocation at `browser-sessions.ts:219-225,303-308,345-350`. The real-server journey drives all three at `production-auth.integration.test.ts:347-460`. | PASS |

## Trust-boundary trace

For SSO, the route tenant is carried only in encrypted, short-lived login state
and the Wyrd authorization request. `openid-client` checks state and PKCE at the
callback. Wyrd selects and verifies the tenant provider and issues the
tenant-bound refresh and access tokens. The BFF seals the refresh token and
route metadata, but every protected settings call is authorized again by Wyrd
from the access token.

For recovery, the BFF sends only the API key and the standard token type to
Wyrd. Wyrd derives tenant and principal from that key. The recovery route key
names the browser cookie and post-login route but is never sent as token
authority. Consequently, presenting tenant B's key on tenant A's recovery page
can create a browser session under that route label, but every server call is
tenant B-authorized and cannot read or mutate tenant A. That is the approved
contract, and the candidate's journey proves the crossed-key case.

For switching, the target route is opened only after the target's independently
sealed cookie is decrypted and its refresh token or API key produces a current
access token. A copied SSO cookie fails because the sealed tenant and cookie
tenant differ. Browser-visible tenant names are navigation metadata and do not
become server authority.

## Security Audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

None required by the approved task. No speculative or non-standard control is
recommended.

### Positive Controls

- Session and login cookies are encrypted with `jose` A256GCM and are Secure,
  HttpOnly, and SameSite=Lax.
- PKCE and state are generated and validated through `openid-client`; the BFF
  does not duplicate OAuth verification logic.
- API-key exchange supplies no browser-selected tenant parameter.
- The private bearer token is not serializable from `BrowserSession` and is
  forwarded only server-to-server.
- Forged, undecryptable, expired, and crossed tenant cookies are cleared and
  fail closed.
- SvelteKit's standard origin check protects form actions; the journey proves a
  cross-site recovery post and a cross-site settings mutation are refused.
- Protected settings operations rely on Wyrd's server authorization rather
  than UI permission gating.

## Material proposed findings

None.

No mechanism, check, file, setting, or option beyond the applicable standards
was identified in this tenancy/authorization boundary. In particular, this
review does not require route/token tenant equality for API-key recovery, a new
tenant-lookup endpoint or claim, server-side session state, immediate access-
token revocation, or another downstream tenant guard.

## Verification assessment and limits

The task records successful narrow write-set verification: the filtered
`production_ui_bff_journey`, UI check, UI test suite, formatting, lints, and
`git diff --check`. This domain pass independently inspected the exact
real-server journey assertions for crossed and forged cookies, SSO and recovery
separation, token-derived cross-tenant isolation, switch revalidation, denied
settings mutations, logout isolation, and old-connection renewal refusal.

This reviewer did not rerun the environment-owning journey. That is not an
acceptance gap for this discovery report because the candidate includes the
test source and the task supplies a successful result for its narrow owning
lane. Unfiltered identity journeys and cross-language suites remain change-
review work as directed.

## Overall result

**PASS**

The candidate preserves tenant authority at the Wyrd token/server boundary,
implements the approved API-key recovery semantics without a parallel trust
model, revalidates tenant sessions during switching, and includes credible
real-server proof for the material crossed-tenant and authorization paths. No
material tenancy or authorization finding is proposed.
