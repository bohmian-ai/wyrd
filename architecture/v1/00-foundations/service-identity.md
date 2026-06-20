# Service Identity

Wyrd non-human identity is card-bound. Service and Agent principals are durable
rows in `wyrd.auth_service_accounts`, keyed by tenant, principal kind, card
kind, and card UID. The structured `CardRef` is stored as JSONB for lookup and
policy ergonomics, but the durable identity key remains the card UID tuple.

API keys are bootstrap credentials only. The server returns plaintext once from
`POST /auth/issue-key`, stores only an Argon2 hash, and uses a tenant-bearing
prefix to enter a tenant-scoped `TenantConn` before lookup. Every invalid key
case returns the same public code: `WYRD_AUTH_401_API_KEY_INVALID`.

`POST /auth/token` supports Wyrd API-key exchange and RFC 8693 token exchange.
Service and Agent access tokens carry a structured `card_ref` claim. Delegated
tokens extend the JWT `act` chain and require the caller to hold
`Permission { resource: Delegation, action: Issue }`.

Credential issuance and token exchange write durable audit rows in the same
tenant transaction as the credential or refresh-token row. Fire-and-forget audit
emission is not part of this foundation.
