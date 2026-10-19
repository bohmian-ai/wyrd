# Wyrd UI development

Run `mise run dev:frontend` from the repository root and open
<http://127.0.0.1:3000>.

With `WYRD_UI_LOCAL_AUTH=true` (the `dev:frontend` default), **Sign in with
SSO** simulates authentication with a development identity. The default user
has access to its first tenant only and goes directly to it. The global
**DEV · Test login** control selects one tenant, multiple tenants, or no access.
Applying it signs out the current development session and returns to login;
only the next sign-in exposes that user's authorized memberships. Local sign-in
is unavailable in production builds, even if its environment flag or cookies are
present. The UI shows no domain data; there is no mock data mode.

Production builds sign in through the Wyrd server, which is the OAuth 2.0
authorization server. The web app is its public client `wyrd-ui` and uses
`openid-client` for the authorization code flow with PKCE. It keeps the
server-validated refresh token (or API key) in an HttpOnly cookie that page
scripts cannot read, and holds access tokens only in memory. It needs only
`WYRD_SERVER_URL`; its public origin is whatever address the browser used. Only sign-in, the
signed-in tenant page and Settings are currently implemented.
