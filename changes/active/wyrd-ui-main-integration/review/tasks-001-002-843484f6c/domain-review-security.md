# Security and authentication domain review

Subject: base `8651b96fd0c049ab5e63b0606c98326af1fc8848`, candidate `843484f6cceba1f48139349f66794966221c5d37`. The complete cumulative diff was inspected for changed authentication, credential, origin and tenant boundaries; planning/reference files are context. HEAD remained the candidate. No source was edited.

## Boundary and authority coverage

| Boundary | Authority | Source and caller coverage | Assessment |
|---|---|---|---|
| Native browser → BFF → Rust authorization server | Spec REQ-004/006, AC-003; security posture OAuth authorization server; AGENTS server/client ownership | `DevServer::ui_command`, `hooks.server.ts::handle`, `upstream.ts::{serverUrl,forwardAuth}`, Rust `RequestOrigin`, authorize/callback endpoints; upstream tests; real identity fixture and production-auth integration driver | Fixed `ORIGIN` establishes the native public origin. Forwarding strips incoming origin-chain headers, supplies the established host/scheme, targets only configured upstream, streams request body and preserves redirects/errors/Set-Cookie. Redirect following is disabled. No alternative authentication issuer or session owner was introduced. |
| Official ingress → nginx → Rust/BFF | Spec REQ-005/006, AC-004; deployment/release network boundary | Debian/Rocky images, entrypoint, nginx maps/locations, startup-common proof; native fixture differences | Rust auth/platform/API routes retain their owner; BFF receives forwarded public origin. The actual container mode differs from the native identity fixture and lacks the required federated identity proof (SEC-001). |
| Per-tenant browser session → retained UI/settings | Spec REQ-006/032; security posture principal planes and tenant authority; agent rules | Full `BrowserSessions` and `BrowserSession` bodies; hook; root load/actions; tenant layout load/markup; login/API-key/callback routes; settings server actions and changed markup; session/routing/production auth tests | Session, token renewal, API key exchange, token-scoped settings calls, logout and tenant switch continue through existing owners. Type relocation does not expose a token in page data. Root local login still requires both Svelte development mode and explicit local-auth enablement; deleting the mock switch does not enable local login in a production build. |
| Platform authentication | Spec REQ-005/006, platform/tenant separation | nginx `/platform` ownership, unchanged Rust auth/platform surfaces; full changed route inventory | No new platform credential processing or cross-plane session fallback was added. BFF `/auth` forwarding retains Rust endpoint ownership. Native platform behavior is not independently exercised in this review. |

Reviewed applicable authority: AGENTS instructions, agent rules, spec-driven development and maintainer references, approved revision 9 and both tasks, security posture identity/OAuth/trust sections, deployment/release network guidance and relevant design/doctrine ownership rules. No SQL authority or token verification implementation changed.

Existing alternatives: `BrowserSessions` remains the complete credential/session workflow; `RequestOrigin` remains Rust's public-origin derivation. `forwardAuth` fills the missing transport leg without duplicating signature verification, refresh, PKCE or provider orchestration. Nginx remains the container routing owner. No second auth implementation is warranted.

## Proposed material finding

### SEC-001 — MISSING — Official-container federated login proof is absent

Task: TASK-001. Obligation: REQ-006 explicitly requires real login through the container public path using repository identity fixtures; AC-004 calls for production identity exercise through nginx.

Exact locations: `scripts/server/startup-common.sh:56–67` (`ui_sign_in`); `crates/wyrd/wyrd-server/tests/integration/identity_ui_e2e.rs:128–132` and `485–490`; `docker/official/extras/entrypoint.sh:31–35`.

Evidence: `ui_sign_in` fetches root and `/auth/device`, exchanges an API key and checks the signed-in landing page. It never visits an IdP, redeems a provider callback or exercises a refresh-token login. Meanwhile the actual identity driver now starts both BFFs with fixed `ORIGIN`; its requests go directly to those BFFs, whose `forwardAuth` carries `/auth/*`. The official container instead derives origin from nginx forwarding headers and routes `/auth/*` directly to Rust. Thus neither the supplied passing native journey nor the added startup probe establishes the new container's federated callback/origin/session behavior. In addition, the startup lanes have not been executed on any official image.

Observable consequence: acceptance can presently pass source review while container SSO callbacks, origin handling or cookie behavior fail for an installed image. This is a required proof gap, not a claim that those runtime paths are known to fail.

Smallest correction boundary: reuse the repository identity fixture/production-BFF journey and the existing image/startup owner to drive the required federated login at nginx's public origin, with image/source identity recorded. Keep both deployed routing paths intact; do not replace container nginx with the native fixed-ORIGIN harness or create another auth/session implementation. Exercise the actual provider round trip and a rejected cross-origin/state path plus refresh/logout through that deployment. The native tests need not be duplicated wholesale. Execute the focused container proof on a supported Linux image and retain its results; running only today's API-key probe cannot close this finding.

## Authority discrepancy requiring orchestrator reconciliation

REQ-006 describes retaining a configured **confidential** OAuth client and **encrypted** cookies, consistent with security posture's `client_secret_basic` and encrypted-session requirements. However baseline and candidate `BrowserSessions.configuration` use `client.None()` (`browser-sessions.ts:150–152`), and `establish` stores the credential as JSON in an HttpOnly cookie. These bodies were not changed by either task (only a type import moved). This review does not reclassify inherited auth behavior as an introduced regression or prescribe an adjacent auth redesign. It also cannot honestly certify that the encrypted/confidential text passes merely because existing tests pass. Resolve that acceptance/authority mismatch explicitly; a new security-contract decision is outside a bounded transport/removal correction.

Similarly, nginx's maps accepting existing upstream forwarding values predate this candidate. The new BFF is loopback-bound behind nginx. No new exploit is established here solely by restating those pre-existing maps, and no speculative forwarded-header remediation is proposed.

## Verification and limits

Independently executed: `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/server/upstream.test.ts src/lib/server/auth/session.test.ts` — 2 files, 11 tests passed. This covers configured-target/origin forwarding, separate cookies, unchanged redirects/errors, invalid upstream refusal and session/local-identity rejection. The task files' real identity journey evidence was inspected as supplied evidence, not rerun or treated as container evidence.

No live provider, official image, native Linux, browser visual or platform journey was run in this domain audit. Container SSO and TLS-terminated cookie behavior remain unproved. No new source finding is asserted for unchanged token/credential mechanics.

## Results

TASK-001: **FAIL** — SEC-001; encrypted/confidential authority text additionally remains unresolved as described above.

TASK-002: **PASS** for this authentication/tenant boundary: removal preserves existing production authentication ownership and local-login production gating; this domain result does not substitute for its outstanding human visual checkpoint or the orchestrator's complete task acceptance.

Overall domain result: **FAIL**.
