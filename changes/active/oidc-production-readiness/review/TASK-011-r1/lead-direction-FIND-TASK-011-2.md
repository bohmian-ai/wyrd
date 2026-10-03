# Lead direction: FIND-TASK-011-2 reversed

Standing human direction: Wyrd does what everyone else does.

FIND-TASK-011-2 asks the BFF to keep the session cookie when RFC 7009
revocation fails, so the user can retry. Standard practice for browser logout
is the opposite: the local session always ends, and server-side revocation is
best-effort. NextAuth, oauth2-proxy and the OIDC RP-initiated logout guidance
all work this way. Spec revision 8 chose the same rule for saved logins:
"delete locally, then revoke best-effort". A user who clicks "Sign out" and
stays signed in, for example on a shared machine, is the worse failure.

Correction: logout always clears the cookie and cache entry and shows the user
as signed out. It calls `openid-client.tokenRevocation` best-effort. A
revocation failure is logged with structured fields (no token values) and is
not shown as a failed logout. Add no retry, store, setting or endpoint.
FIND-TASK-011-1 stands as written.
