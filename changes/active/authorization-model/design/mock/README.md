# Onboarding and platform console design mock

One self-contained preview of the public onboarding pages and the platform console (REQ-002, REQ-011–REQ-014,
REQ-017, REQ-018): workspace signup by email or provider, email proofs, workspace discovery, recovery contact
verification, password reset, and `/platform` sign-in with its console views. Signed-out pages reuse the tenant sign-in
card (`crates/wyrd/wyrd-server/wyrd-ui/src/lib/Entry.svelte`, `src/app.css`); the account section and console reuse
the Account page's panels. The data is sample data, every address is `example.test`, and server responses are
simulated from the wire contracts in `crates/wyrd-spec/src/auth/{browser,social,platform_identity,tenant_admin}.rs`.
It is not product code.

## Build and check

From the repository root:

```bash
mise exec -C changes/active/authorization-model/design/mock -- node build.mjs             # writes dist/
mise exec -C changes/active/authorization-model/design/mock -- node selfcheck.mjs         # real-browser checks
mise exec -C changes/active/authorization-model/design/mock -- node selfcheck.mjs --shots # screenshots only
```

`build.mjs` borrows Vite, Svelte and the fonts from `crates/wyrd/wyrd-server/wyrd-ui/node_modules`; the mock has no
dependencies of its own, so there is no `package.json`. The self-check serves the repository root on an ephemeral port
and drives `dist/` with the Playwright that wyrd-ui already installs. It fails on any page error, script dialog,
sideways page scroll, or request outside `design/mock/`. Both modes capture every preview state in light and dark at
1280 and 390 wide into a fresh `$TMPDIR/wyrd-signup-mock-*` directory, which never enters the source tree.

Preview: serve the repository root with any static server and open
`/changes/active/authorization-model/design/mock/dist/`. Add `?mode=light` before the hash for light mode.

## Routes and preview states

The dashed strip at the top is not product chrome. Its **State** menu opens every state below, and the mode button
switches light and dark.

| Hash | Shows |
|---|---|
| `#/signup` | "Create a workspace": email (`local_signup`), then Continue with Google and Continue with GitHub |
| `#/signup?providers=github` | Email and only the provider the operator enabled |
| `#/signup?local=off` | No mail relay: `local_signup` false, providers only |
| `#/signup?preview=sent` | `202 check_email`: "Check your email" with the code-paste field |
| `#/signup/verify` | No fragment: paste the code from the message |
| `#/signup/verify?preview=checking` | The page POSTs the fragment proof (busy); `?proof=…` does it live and clears it |
| `#/signup/verify?preview=proven` | `200 proven`: continue to name the workspace |
| `#/signup/verify?preview=invalid` | `401 WYRD_AUTH_401_ONE_TIME_CODE_INVALID` |
| `#/signup/verify?preview=expired` | `400 WYRD_AUTH_400_INVALID_STATE` |
| `#/signup?preview=disabled` | Signup turned off: explanation and a link to sign in |
| `#/signup?signup=failed` | The provider proof was refused (the server's own redirect query); retry |
| `#/signup?preview=throttled` | `429 WYRD_AUTH_429_SIGNUP_ATTEMPTS`, "Try again in 23 minutes." |
| `#/signup/workspace?preview=checking` | The page reads the proof method before it shows the form |
| `#/signup/workspace` | Proven Google identity, empty form, no challenge |
| `#/signup/workspace?identity=email` | Email proof: the form adds the administrator username and password (`local_credentials`) |
| `#/signup/workspace?identity=github` | Proven GitHub identity without an email |
| `#/signup/workspace?identity=github&preview=contact` | `409` `details.reason: contact_required`: confirm an email bound to the provider attempt |
| `#/signup/workspace?identity=github&local=off&preview=contact` | The same without a relay: the attempt ends, Start again |
| `#/signup/workspace?challenge=on` | The form with the Turnstile placeholder; production renders the real widget from the published site key |
| `#/signup/workspace?preview=invalid` | Inline slug validation |
| `#/signup/workspace?preview=taken` | `409`: the slug is taken |
| `#/signup/workspace?preview=throttled` | `429` with the retry time |
| `#/signup/workspace?preview=limit` | `403 WYRD_AUTH_403_SIGNUP_WORKSPACE_LIMIT`; no form, only "Sign in to an existing workspace" |
| `#/signup/workspace?challenge=on&preview=challenge` | The challenge was not passed |
| `#/signup/workspace?preview=disabled` | Signup was turned off after proof; the form is replaced by a way out |
| `#/signup/workspace?preview=expired` | The proof expired; Start again |
| `#/signup/workspace?preview=unavailable` | Any 5xx (here `502 WYRD_SPEC_502_UPSTREAM_FAILURE`): the production card's "✕ Sign-in unavailable", "Please try again later." and the code; the page's forms are hidden |
| `#/signup/workspace?preview=creating` | Creation in progress: busy, disabled button and read-only fields |
| `#/signup/workspace?preview=created` | "Workspace created", held still for review |
| `#/t/<slug>` | Minimal placeholder for the workspace landing page |
| `#/workspaces` | "Find your workspaces": email request |
| `#/workspaces?preview=sent` | The same `202` answer for every address |
| `#/workspaces?preview=throttled` | `429 WYRD_AUTH_429_EMAIL_ATTEMPTS` with the retry time |
| `#/workspaces?preview=unavailable` | `403 WYRD_AUTH_403_METHOD_UNAVAILABLE` without a relay |
| `#/workspaces/verify?preview=proven` | First page of 20 links (display name, route key, login link only); More workspaces reads the next |
| `#/workspaces/verify?preview=empty` | No workspaces, in the same tone as a list |
| `#/workspaces/verify?preview=invalid` / `read-expired` | `401` proof refused / `400` five-minute read expired |
| `#/t/<slug>/settings/account` | New "Recovery email" section of the existing Account page |
| `…/settings/account?preview=sent` | `202 check_email` with the code-paste field |
| `…/settings/account?preview=recent` | `403 WYRD_AUTH_403_RECENT_LOGIN_REQUIRED`: Sign in again |
| `…/settings/account?preview=unavailable` / `throttled` | `403 METHOD_UNAVAILABLE` without a relay / `429 EMAIL_ATTEMPTS` |
| `#/t/<slug>/contact/verify?preview=proven` | `204`: recovery email verified |
| `#/t/<slug>/contact/verify?preview=invalid` / `signed-out` | `401 ONE_TIME_CODE_INVALID` / `401 INVALID_TOKEN` |
| `#/t/<slug>/reset` | Request a mailed code by username, then paste a code and set a new password |
| `#/t/<slug>/reset?preview=sent` | The same `202` answer for every account |
| `#/t/<slug>/reset?preview=throttled` | `429 WYRD_AUTH_429_PASSWORD_ATTEMPTS` |
| `#/t/<slug>/reset?preview=nomail` | `password_reset` false: paste an administrator's code only |
| `#/t/<slug>/reset?preview=invalid` | `401 ONE_TIME_CODE_INVALID` on redemption |
| `#/t/<slug>/reset?preview=sso` | Company SSO required: `403 METHOD_UNAVAILABLE`, no form |
| `#/t/<slug>/login?reset=done` | Back at sign-in after `204`; reset never signs in |
| `#/platform` | Entry, heading "Platform admin": operator username and password, then Sign in with platform SSO when platform OIDC is configured |
| `#/platform?sso=off` | No platform OIDC: the SSO button is not shown |
| `#/platform?preview=refused` / `throttled` / `nosso` / `ended` / `unavailable` | `401 UNAUTHENTICATED` / `429 SIGN_IN_ATTEMPTS` / `404` platform OIDC removed since the page loaded (no button) / session ended / a sign-in answered with any 5xx |
| `#/platform/ui/callback` | Platform SSO return: a sign-in opens the console; `?preview=refused` is an unregistered identity (`401`), `reauth` a confirmed re-authentication, `popup` the re-authentication pop-up handing back to the console |
| `#/platform/ui` | Console overview: safe session metadata and workspaces with Suspend and Resume |
| `#/platform/ui?preview=reauth` (`&method=oidc`) | Fresh-proof prompt before a destructive change, by password or forced platform SSO |
| `#/platform/ui?preview=reauth-refused` | Re-authentication refused; nothing changed |
| `#/platform/ui/providers` | Google and GitHub.com: callback URL, instructions, client, Test sign-in, Enable only when tested |
| `#/platform/ui/providers?preview=conflict` | `409`: a newer revision was saved |
| `#/platform/ui/mail` (`?preview=none`, `test-failed`) | SMTP relay, its replace form and test send; no relay; relay refused the test |
| `#/platform/ui/signup` | Password and signup switches, workspace cap, Turnstile challenge |
| `#/` | Placeholder for the existing sign-in page, with Create a workspace and Find my workspaces |

Setup (`wyrd-server setup`, REQ-014) is CLI-only; it has no page here. Its administrator signs in on the existing
tenant sign-in page.

The pages also work by hand. An email link's fragment is the router's hash here, so the proof rides as `?proof=` and
the page removes it before posting, as the real page clears `#proof=`. The password `wrong` is refused on the platform
pages. The mock session starts without fresh proof, so the first destructive console change asks to confirm. A provider button leads to the form. The name fills the slug until the slug is edited.
Create workspace is busy for about a second, then shows "Workspace created" and opens `#/t/<slug>`. The slugs `acme`,
`wyrd` and `demo` are taken. With `challenge=on`, creating without ticking the placeholder's box fails the challenge.

## Guesses

Wording and styling that no owner fixes yet; each needs a ruling before production copies it.

- **Card.** The pages use the tenant sign-in card (`Entry.svelte`), not the root page's wider card with the thread
  backdrop and mode button, although `/signup` is a root-level page.
- **Headings and intro.** "Name your workspace" for the second page, and the intro line "Confirm who you are with …,
  then name your workspace. You become its administrator."
- **Provider buttons** are equal-weight plain controls, as on the tenant sign-in page; neither is primary.
- **Problem titles and remediations** other than the 429 title are this mock's. The server's catalog text for
  `WYRD_SPEC_409_CONFLICT`, `WYRD_AUTH_401_INVALID_TOKEN` and `WYRD_AUTH_400_INVALID_STATE` is generic ("Conflict",
  "Invalid token", "Invalid OIDC state"), so the page words them for this journey. The 409 also means "already
  created" and "creation in progress" on the server; the mock words only the taken slug.
- **Codes.** The challenge refusal shows `WYRD_AUTH_403_SIGNUP_CHALLENGE` and the cap
  `WYRD_AUTH_403_SIGNUP_WORKSPACE_LIMIT`, both in the catalog.
  `?signup=failed` is a redirect without a problem body, so it shows no code.
- **Slug field.** A refused slug gets a `--danger` border plus a "⊘" line beneath; the existing auth pages have no
  field-level error style to copy. The slug rule shown is the one in the mock brief (no underscore), which is narrower
  than the root page's tenant-key pattern.
- **Identity line** names the provider only ("Signed in with GitHub."); the proof read carries no address.
- **Turnstile** is a dashed placeholder box; production renders Cloudflare's widget, which brings its own look.
- **Success** is a "✓ Workspace created. Opening /t/<slug>…" status line under the button before navigation.
- **Email proof pages.** One "Check your email" answer, worded the same for every address, with a paste field because
  the message also carries the code. The workspace list shows display name and route key per link.
- **Reset request** lives on `/t/<slug>/reset` above the existing paste-code form; no contract names its own page.
- **Console** layout (top bar, four views, panels) and the in-place "Confirm it is you" prompt are this mock's.
  Workspace "retirement" is the wire's `suspended` status, so the console says Suspend and Resume.

## Contract readings (ruled in cycle 15)

- **Proof kind on `/signup/workspace`.** `GET /auth/signup/proof` reports the method; the page names the provider and
  never an address, and shows "Checking this browser's sign-in…" while it reads.
- **Contact verification availability.** The recovery-email section appears only when `email_discovery` is offered.
- **Current recovery contact.** No read returns it, so the section never shows the current address.
- **Platform SSO button.** `GET /platform/browser/methods` says whether platform OIDC exists; the button shows only
  then. A page loaded before OIDC was removed still gets the `404` (`?preview=nosso`).
- **`/platform` vs `/platform/ui`.** Sign-in on exact `/platform`, the console under `/platform/ui`.
- **Retirement** is Suspend and Resume; the platform's own `wyrd-system` workspace never suspends.
- **Relay password on save.** A blank password keeps the stored one; "Remove the stored username and password" drops
  both.
