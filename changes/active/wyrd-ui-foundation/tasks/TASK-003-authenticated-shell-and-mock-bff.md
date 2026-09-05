---
id: TASK-003
title: Authenticated tenant shell and mock BFF
kind: implementation
status: review
spec: SPEC-wyrd-ui-foundation
spec_revision: 6
requirements: [REQ-001, REQ-002, REQ-006, REQ-007, REQ-008, REQ-009, REQ-010, REQ-011, REQ-012, REQ-013, REQ-066, REQ-081, REQ-092, REQ-093, REQ-094, REQ-095, REQ-100, REQ-101, REQ-106, REQ-129, INV-001, INV-002, INV-008, INV-009, INV-012, INV-014, INV-021, AC-001, AC-002, AC-003, AC-010]
depends_on: [TASK-002]
parent_task:
remediates: []
---

# Outcome and value

Create the trusted application boundary used by every workspace: local
authentication, tenant routing, five-entry responsive shell, Home, typed mock
data, and one server-only Wyrd client seam.

# Owner and write set

- Own root/tenant layouts, `/` and `/t/[tenantKey]`, `hooks.server.ts`, session
  and tenant server modules, shared app chrome, and mock-client infrastructure.
- Implement an opaque HttpOnly local session containing subject, authorized
  tenants, scopes, expiry, and CSRF context; expose only safe page metadata.
- Use one concrete server-only client. Future transport replacement must not
  change browser component contracts or add a duplicate `/api/ui` tree.
- Keep shell, identity, route ownership, and server access out of the component
  catalog created by TASK-002.

# Locked decisions and non-goals

- The URL requests a tenant; server-side authorization establishes it. `space`
  is a page filter, never another global switcher.
- Tenant switching is a same-origin action that revalidates membership and
  returns to the destination Home without retargeting other tabs.
- No production OIDC, durable domain connection, global Inbox/Connections,
  browser-held Wyrd token, auth dependency, or alternate error vocabulary.

# Ordered test scenarios

1. Unauthenticated and expired sessions yield safe access states.
2. Root resolution handles zero, one, and multiple authorized tenants.
3. Unknown or unauthorized tenant keys fail without returning tenant data.
4. The responsive shell shows exactly Home, Cards, Observe, Changes, and Query,
   always identifies the tenant, and exposes switching only when applicable.
5. The switch action revalidates authorization and rejects missing CSRF context.
6. Home and safe `WyrdProblem` failures flow through real server loads/actions.

# Red-Green-Refactor

Drive one boundary scenario at a time. Consolidate repeated session and tenant
logic on one concrete server owner; do not introduce provider abstractions.

# Exact verification

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/server/auth/session.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/server/routing/tenant.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/components/app/Shell.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise run check:tokens
```

# Evidence and stop conditions

Record tenant/session cases, route/load/action ownership, safe error examples,
and desktop/mobile theme captures. Stop before production identity, durable
domain APIs, or any browser credential translation is invented.

# Execution skills

Use `$wyrd-implement` and `wyrd-ui`.

# Execution evidence — 2026-09-04

Readiness: **READY — no blocking readiness findings.** Revision 6 is approved;
the integrated TASK-002 component implementation and remediation are present at
base `284b07223`. Authority, coverage, cohesion, dependencies, ordered scenarios,
command precision, evidence, material decisions, and remediation integrity were
checked before implementation. No product or contract revision was needed.

Implementation: **COMPLETE**, working-tree candidate over that base; ready for
`$wyrd-task-review`. This record does not approve the implementation.

## Delivered boundary and obligation trace

- **REQ-001, REQ-002, REQ-006–008, REQ-092–095; AC-001:** `/` resolves zero,
  one, and multiple authorized tenants; recent selection is revalidated.
  `/t/[tenantKey]` owns Home. The trusted app shell has exactly Home, Cards,
  Observe, Changes, and Query. Tenant identity is static for one membership
  and a searchable chooser for multiple memberships. Display-name changes
  preserve the routing key. Space never becomes global context.
- **REQ-009–013, REQ-100, REQ-106; INV-001, INV-002, INV-014:**
  `hooks.server.ts` resolves the opaque cookie and binds the URL's tenant on
  every request, including data requests and actions. `LocalSessions` owns the
  development principal, membership/permission snapshot, current membership
  checks, expiry, CSRF, and tenant-specific reauthentication. Each domain load
  receives one server-only `WyrdClient` with explicit tenant, subject, and
  effective permissions. There is no domain transport, browser Wyrd credential,
  cookie-to-token translation, or duplicate API namespace. Attaching the
  canonical `X-Wyrd-Access-Token` belongs inside this client when real transport
  replaces the mock; no fictitious token or HTTP call was added.
- **REQ-011, REQ-012, REQ-094; AC-002:** 256-bit opaque HttpOnly session IDs,
  SameSite=Lax, Secure on HTTPS, 15-minute expiry, sign-out invalidation,
  same-origin action checks, session-bound CSRF, revoked-membership denial,
  and per-tenant permission intersection. Switching changes only the recent
  entry preference; another tab's URL still selects its own authorized tenant.
  A tenant policy can require explicit local reauthentication before binding.
  Page data exposes only subject identity, tenant labels/keys, expiry, and CSRF.
- **REQ-101; INV-009:** BFF problems retain the existing seven-field
  `WyrdProblem` envelope and known catalog codes. Unknown failures map to
  `WYRD_SPEC_500_INTERNAL`; known upstream failure retains
  `WYRD_SPEC_502_UPSTREAM_FAILURE`. Raw messages, details, connections, and
  credentials are discarded. Unauthorized tenant requests return the same
  safe denial without revealing whether the tenant exists. Responses are
  private/no-store. Framework error pages additionally carry SvelteKit's
  required `message` field; action problem values retain the wire shape.
- **REQ-066, REQ-081, REQ-129; INV-008, INV-012, INV-021; AC-003, AC-010:**
  Home leads with attention, followed by linked numeric summaries and recent
  work, with Card lookup and Change Request creation links. It reuses Panel,
  Badge, Button, StateBlock, format helpers, ModeProvider, and brand/logo.svg.
  Trusted Shell and TenantChooser stay outside the semantic catalog. The
  approved spec's accessible mobile-navigation exception to the generic brand
  shell applies. Both modes use identical structure and token-based styling;
  the app font import now loads the brand's actual four typography roles.

## Scenario evidence

1. Expiry first failed because an expired session was still returned; the
   expiry guard made it pass. Missing/forged/revoked sessions and safe metadata
   also pass. Direct hook coverage proves expiry and disabled local identity
   stop tenant loads before resolution.
2. Root resolution initially had no destination behavior. Zero/one/many and
   recent-selection tests now pass. An additional RED exposed stale tenant
   display names; metadata now projects the current authorized display name.
3. Tenant binding initially had no implementation. Unknown, unauthorized,
   revoked, and expired contexts now fail before a client is supplied.
4. The first shell had only static identity; the multi-tenant test failed for
   missing searchable switching. Both shell tests now pass, including the
   five destinations, active route, CSRF form, skip link, and menu disclosure.
5. Switching initially had no implementation. Membership/CSRF/origin tests now
   pass. A further RED proved tenant policy did not require reauthentication;
   it now challenges and completes through the local server action.
6. The live SvelteKit journey first failed on the missing sign-in workflow.
   Sign-in → chooser → switch → tenant Home → independent-tab read → denial →
   sign-out now passes through HTTP. Home error RED showed upstream codes were
   collapsed; safe normalization now preserves known codes. A least-privilege
   RED showed Card-read alone could expose observation summaries; Home now
   also requires the existing `bifrost_query:read` and `evals:read` permissions.

Initial missing-module failures were setup checks, not claimed behavioral RED
proof. Focused tests and prior green scenarios were rerun during each cycle.
The live HTTP test was corrected to send `Accept: text/html` for native-form
semantics: generic fetch negotiation yields SvelteKit JSON action redirects.
A type-check failure exposed inconsistent action result shapes; all failures
now explicitly carry nullable reauthentication metadata. No assertion or gate
was weakened.

## Final verification

Commands run through the repository toolchain:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/server/auth/session.test.ts
# PASS: 3 tests
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/server/routing/tenant.test.ts
# PASS: 7 tests
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/components/app/Shell.test.ts
# PASS: 2 tests
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/server/routing/journey.test.ts
# PASS: real SvelteKit HTTP journey, child server cleaned up by the test
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test
# PASS: 91 tests in 19 files
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
# PASS: 0 errors, 0 warnings
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
# PASS: adapter-node production build
mise run check:tokens
# PASS: generated targets match palette.json
git diff --check
# PASS
```

Touched TypeScript/CSS and embedded Svelte script/style blocks were formatted
with the already-installed Prettier. The UI package has no separate lint task;
Svelte/TypeScript checking supplies its declared static lane. No Rust, Python,
SDK, generated-contract, dependency, lockfile, or shared build infrastructure
changed, so their unrelated aggregate lanes were not run.

Production Chrome inspection passed at 1440, 768, and 390 pixels in both modes:
HttpOnly containment, hydrated forms, five destinations, keyboard menu toggle,
visible tenant identity, searchable switching, empty Home, independent tabs,
no horizontal page overflow, and no browser exceptions. Captures:

- [Desktop light](../evidence/TASK-003-home-1440-light.jpg)
- [Desktop dark](../evidence/TASK-003-home-1440-dark.jpg)
- [Mobile light](../evidence/TASK-003-home-390-light.jpg)
- [Mobile dark](../evidence/TASK-003-home-390-dark.jpg)

## Local use and limitations

```bash
WYRD_UI_LOCAL_AUTH=true mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui dev
```

Local authentication is opt-in and uses a bounded, process-local store with
Acme and Research fixtures. Research demonstrates an empty workspace. Tenant
counts, expiry, reduced permissions, policy challenges, and upstream failures
are also covered at their real hook/load/action functions without shipping
browser-controlled authorization fixtures. This is development authentication,
not production identity or persistence. Replacing the mock transport and local
identity remains server-only work under the approved future boundary.

Card, Observe, Changes, and Query destination pages remain owned by their
subsequent tasks; this task establishes their canonical shell links, not their
workspace implementations. Full integrated workspace journeys remain TASK-020.
The final tracked/untracked audit preserves all pre-existing architecture and
skill edits. No commit, merge, push, or deployment was performed.

## User-directed visual and development-mode corrections

The first implementation did not faithfully reproduce H-02 in
`brand/renders/product/home.svg`: it omitted Recent changes and Recently viewed
Cards, moved summaries below the main content, and used a looser shell. The
original screenshot review was insufficient. Home now restores the two tables,
raised attention panel, stacked right-hand summaries and recent-work panel,
header search, and compact shell. H-01 now supplies the entry page's centered
brand/title and active outcome panel; the mock's simultaneous outcome examples
and explanatory annotations are not rendered as product UI. The Cards summary
uses 16 registrable kinds per current architecture rather than the mock's stale
17. Shared catalog Badge treatment and the actual brand logo remain authoritative.

The logo was also broken specifically in development: Vite returned 403 because
`brand/logo.svg` is outside SvelteKit's static serving allowlist. Both consumers
now import the same asset with `?inline`, avoiding a broader filesystem allowlist
or copied asset. The HTTP journey verifies that the rendered logo source loads;
Chrome verifies successful image decoding on Home and entry.

Per direct user instruction, one global development button in the root layout
controls mock data for every route, including future workspaces through the
same request-bound `locals.wyrd` client. A same-origin action (also validating
session CSRF when authenticated) sets an HttpOnly browser-session cookie and
reloads the current route. No process restart, per-page fixture selection, or
fallback from server errors to mocks is allowed. Fixtures now live separately
in the server-only mock module. Turning mocks off currently reports that the
real transport is not connected. The setting does not implement future routes
or transport. The UI README documents this extension boundary.

Local sign-in and mock selection are now restricted by the framework's actual
development mode, even if environment flags or cookies are present in a
production build. Anonymous entry returns no tenant memberships. The chooser
projects only the authenticated principal's authorized memberships. The current
identity is explicitly a fixed local-development identity, not enterprise SSO.
The user requested an enterprise login solution; production identity-provider
integration remains a material follow-up to the approved local-auth scope.

Verification:

- Extended the HTTP journey; RED: missing Recent changes, then missing global
  mock button. GREEN: restored tables, anonymous membership containment, valid
  logo response, same-origin rejection, mock-off removes fixtures, mock-on
  restores the path, existing tenant/session checks retained.
- Extended tenant-load coverage; RED: disabled mocks still returned fixtures.
  GREEN: false/unset settings cannot return fixtures through the client.
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test`: 92 tests,
  19 files passed.
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check`: zero errors
  and warnings; `build` passed; `mise run check:tokens` and `git diff --check`
  passed. Changed TS and Svelte script/style blocks formatted with existing
  Prettier; no dependencies added.
- Dev Chrome checks at 1440/768/390 in both themes: image decoding, right rail
  beside attention, both tables, no page overflow, keyboard navigation, tenant
  switching, empty tenant. Entry checks at 1440/390 in both themes: no anonymous
  tenant choices, logo, global toggle survives login and changes Home data
  without signing out. Desktop screenshots were compared against a rendered
  crop of H-02 from the exact approved SVG.
- Built adapter-node server launched with both development flags set true:
  anonymous HTML contains neither local sign-in nor mock controls or tenant
  names; `POST /?/login` and `POST /?/mockData` both return 403, including with
  the mock cookie present. This supersedes the earlier production mock-browser
  check: production can no longer enable development identity.
- Existing Home screenshots refreshed; added `TASK-003-entry-{1440,390}-{light,dark}.jpg`.

Implementation remains uncommitted and awaiting task review; these corrections
do not approve the task or the broader change.

## Mock SSO login follow-up

The user approved a generic SSO mock: no vendor picker or public tenant list,
default configured organization, and development-only access scenarios. The
customer-facing form now says “Sign in with SSO”; it never displays the fixture
principal before authentication. Default mock sign-in creates an Acme-only
membership snapshot and redirects directly to Home. `WYRD_UI_MOCK_TENANT` may
select another configured fixture organization. The global DEV “Test login”
control exercises one tenant, multiple authorized tenants, or no access. Applying
a scenario invalidates the existing local session and returns to sign-in;
scenario selection does not change the process-global principal or another
browser's memberships. Mutation validates origin and authenticated-session CSRF.

Mock sign-in/reauthentication requires development mode, local-auth opt-in, and
mock mode. With mocks off, new login reports the unconnected browser SSO seam
and issues no fixture cookie. Existing Rust generic OIDC is retained; this
follow-up does not implement its browser-session integration or select an IdP.

Focused HTTP journey RED: missing generic SSO form and default direct redirect.
GREEN: single-tenant Home, denied sibling tenant, multi-tenant chooser, no-access
state, CSRF refusal, invalid scenario refusal, session invalidation on scenario
change, and mock-off login rejection without a new session. Existing journey
explicitly selects the multi-tenant scenario to preserve its isolation coverage.
A test fixture was updated to supply request cookies required by reauthentication;
its partial RequestEvent cast was corrected after type checking. Chrome caught
an unreliable select interaction during hydration; native option selection
replaced the controlled select value and the end-to-end browser flow passed.

Verification: full UI suite 93 tests/19 files passed; focused auth/session,
tenant, Shell and HTTP journey files passed (15 tests); Svelte check zero errors
and warnings; production build, check:tokens and git diff --check passed.
Chrome exercised all three scenarios and mock-off refusal, plus both themes at
1440/390 with decoded logo and no page overflow. Captures are
`TASK-003-login-{1440,390}-{light,dark}.jpg`. The UI README documents controls
and the remaining real SSO integration. No dependencies or commits added.

## Login spacing and error-state correction

User screenshots exposed the oversized login panel, repeated headings/status
copy, missing error/action separation, and 502 failures incorrectly labelled
unauthorized. Removed the login panel's forced height and redundant headings;
login now uses one compact 360px content column and a full-width SSO action.
Content flows through explicit grid gaps, including 20px between error and action.
Server failures use the error state with a short message; canonical codes remain
under Technical details. Unconnected integration diagnostics live under the
global DEV Connection status disclosure instead of repeating in login copy.

Chrome checked normal and failed sign-in at 1440/390 in light/dark, measured
compact content and action width, verified the error/action gap, no horizontal
overflow and no visible duplicate diagnostics. Captured eight
`TASK-003-login-{normal,error}-{1440,390}-{light,dark}.jpg` images and visually
inspected desktop error and mobile normal captures. Focused HTTP journey (2
tests), Svelte check (zero errors/warnings), build and diff whitespace check
passed; journey asserts 502 renders error rather than unauthorized. No auth
policy or token handling changed.

## Landing-brand login background and favicon

Per user direction, login now uses the blue/lime connection-web geometry from
`brand/renders/landing.html`, expressed as a static decorative SVG with live
brand tokens. A content-sized sign-in panel keeps text clear of the background;
there is no forced panel height or animated/blurred effect. The shared root head
now imports `brand/app-icon.svg?inline` as the favicon, covering login, Home,
and subsequent routes without copying assets or reintroducing the dev asset 403.

Verified normal/error login in light/dark at 1440/390, including spacing and no
horizontal overflow; reviewed desktop dark and mobile light screenshots. Chrome
successfully decoded the shared favicon on login and Home. The HTTP journey
now verifies the favicon source serves valid SVG. Focused journey (2 tests),
Svelte check (zero errors/warnings), production build, check:tokens, and
`git diff --check` passed. Normal/error screenshot evidence refreshed.
