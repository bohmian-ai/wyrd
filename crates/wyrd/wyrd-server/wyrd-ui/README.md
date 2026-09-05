# Wyrd UI development

Run `mise run dev:frontend` from the repository root and open
<http://127.0.0.1:3000>.

The shared **DEV · Mock data: On/Off** button applies to every UI route. It
submits a server action and reloads the current page, without restarting the
dev server. Its HttpOnly session cookie records the selection for this browser;
it does not change another developer's session. Other open tabs use the selection
on their next request. `WYRD_UI_MOCK_DATA=false mise run dev:frontend` changes
the initial default when no selection cookie exists.

All page loads and actions must use `locals.wyrd`. The request hook resolves the
mode once and constructs that server-only client with the authorized tenant.
Keep fixtures in `src/lib/server/mock.ts`; add each new workspace's typed
fixtures there and expose them through the same client. Components must not
import fixtures, select transports, or fall back to mock responses on errors.
Mock writes must remain in the mock path and must never call the real server.

**Sign in with SSO** simulates authentication when mocks are on. The default
fixture user has access to Acme only and goes directly to its Home. Set
`WYRD_UI_MOCK_TENANT=research` to use the other configured mock organization.
The global **Test login** control selects one tenant, multiple tenants, or no
access. Applying it signs out the current fixture session and returns to login;
only the next sign-in exposes that user's authorized memberships. These controls
are separate from the normal login form.

Mock sign-in requires both mock mode and `WYRD_UI_LOCAL_AUTH=true`.
`WYRD_UI_LOCAL_AUTH=false` disables fixture authentication independently.
Turning mocks off preserves an existing session but cannot create or refresh a
fixture identity.
Both local sign-in and the mock switch are unavailable in production builds,
even if their environment flags or cookies are present.

Rust OIDC already exists; its browser-session integration and the UI domain
transport are not connected yet. Turning mocks
off therefore shows an unavailable state, not live data or an empty fixture.
The later server integration belongs behind the same client boundary; it must
retain server-side credentials and tenant authorization. Only Home is currently
implemented; linked workspaces arrive with their owning tasks.
