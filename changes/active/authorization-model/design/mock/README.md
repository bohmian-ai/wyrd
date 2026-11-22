# Workspace signup design mock

One self-contained preview of the two public workspace signup pages (REQ-014): choosing a provider at `/signup`, then
naming the workspace at `/signup/workspace`. It reuses the look of the existing sign-in card
(`crates/wyrd/wyrd-server/wyrd-ui/src/lib/Entry.svelte`, `src/app.css`). The data is sample data and the server
responses are simulated. It is not product code.

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
| `#/signup` | "Create a workspace" with Continue with Google and Continue with GitHub |
| `#/signup?providers=github` | Only the provider the operator enabled |
| `#/signup?preview=disabled` | Signup turned off: explanation and a link to sign in |
| `#/signup?signup=failed` | The provider proof was refused (the server's own redirect query); retry |
| `#/signup?preview=throttled` | `429 WYRD_AUTH_429_SIGNUP_ATTEMPTS`, "Try again in 23 minutes." |
| `#/signup/workspace` | Proven Google identity with email, empty form, no challenge |
| `#/signup/workspace?identity=github` | Proven GitHub identity without an email |
| `#/signup/workspace?challenge=on` | The form with the Turnstile placeholder |
| `#/signup/workspace?preview=invalid` | Inline slug validation |
| `#/signup/workspace?preview=taken` | `409`: the slug is taken |
| `#/signup/workspace?preview=throttled` | `429` with the retry time |
| `#/signup/workspace?preview=limit` | `403 WYRD_AUTH_403_SIGNUP_WORKSPACE_LIMIT` |
| `#/signup/workspace?challenge=on&preview=challenge` | The challenge was not passed |
| `#/signup/workspace?preview=disabled` | Signup was turned off after proof; the form is replaced by a way out |
| `#/signup/workspace?preview=expired` | The proof expired; Start again |
| `#/signup/workspace?preview=creating` | Creation in progress: busy, disabled button and read-only fields |
| `#/signup/workspace?preview=created` | "Workspace created", held still for review |
| `#/t/<slug>` | Minimal placeholder for the workspace landing page |
| `#/` | Placeholder for the existing sign-in page |

The pages also work by hand. A provider button leads to the form. The name fills the slug until the slug is edited.
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
- **Codes not in the catalog.** The challenge refusal has no published code, so its problem shows none.
  `WYRD_AUTH_403_SIGNUP_WORKSPACE_LIMIT` is named by the spec; its title "Workspace limit reached" is a guess.
  `?signup=failed` is a redirect without a problem body, so it shows no code.
- **Slug field.** A refused slug gets a `--danger` border plus a "⊘" line beneath; the existing auth pages have no
  field-level error style to copy. The slug rule shown is the one in the mock brief (no underscore), which is narrower
  than the root page's tenant-key pattern.
- **Identity line** shows the email in the strong text colour, and "Signed in with GitHub." when no email was proven.
- **Turnstile** is a dashed placeholder box; the real widget brings its own look.
- **Success** is a "✓ Workspace created. Opening /t/<slug>…" status line under the button before navigation.
