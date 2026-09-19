# Service Identity

A Wyrd principal exists independently of any credential. Principals are durable
rows keyed by tenant and principal type: a global administrative principal has
no tenant, and tenant administrative, human, and machine principals have exactly
one. Card binding is a property of a machine principal, not a precondition for
being one. Card-bound Service and Agent principals keep their structured
`CardRef`, stored as JSONB for lookup and policy ergonomics with the card UID
tuple as the durable key; a tenant administrative principal or a tenant-created
automation principal is representable with no Card.

Credentials are principal-generic, and one principal may hold several at once.
The server generates the secret, persists only an Argon2 verifier plus
non-secret lookup and lifecycle metadata, and returns the plaintext exactly once
in the response that created it. The non-secret prefix resolves the owning
principal — and, for a tenant-scoped principal, its tenant — before verification:
`wyrd_global_...` at platform scope, `wyrd_sk_<tenant>_...` within a tenant.
Rotation is overlap: issue B, verify B, revoke A, with no window in which the
principal holds no usable credential. Revoking a credential retires that
credential and advances the principal's authorization epoch; it leaves the
principal, its grants, and its other credentials intact. Every invalid
credential case performs exactly one verification and returns the same public
code: `WYRD_AUTH_401_API_KEY_INVALID`.

`POST /auth/token` supports Wyrd credential exchange and RFC 8693 token
exchange. Service and Agent access tokens carry a structured `card_ref` claim.
Delegated tokens extend the JWT `act` chain and require the caller to hold
`Permission { resource: Delegation, action: Issue }`.

Human OIDC login is a separate entry path, and the entry point selects the
connection, the connection selects the principal, and the principal carries the
scope. A tenant entry resolves only that tenant's connection. The deployment may
hold one platform-scope connection, which resolves only platform principals
pre-registered against an expected issuer and claim and pinned on
`(issuer, subject)` at first login; an unknown platform subject is denied and
never provisioned just in time. Platform authority is a grant held at platform
scope rather than a property of a principal type, so no tenant-plane operation —
tenant administration, principal management, role grant, or group mapping — can
create or elevate a platform principal, and the platform connection's absence or
outage never blocks administration through the global administrative credential.

Credential issuance, token exchange, and every platform or tenant authorization
decision stage a durable audit row through the one canonical append, in the same
transaction that made the decision. Fire-and-forget audit emission is not part
of this foundation.
