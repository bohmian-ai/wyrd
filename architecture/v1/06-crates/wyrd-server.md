# wyrd-server

The server owns auth side effects and tenant-scoped storage orchestration.

Auth modules added for the security foundation:

- `auth::issue_api_key`: resolves active Service or Agent principal rows,
  hashes API keys on the Tokio blocking pool, inserts `auth_api_keys`, and
  writes credential-issuance audit rows.
- `auth::exchange_api_key`: exchanges Wyrd API keys for Service or Agent JWTs,
  inserts principal-generic refresh-token rows, and supports delegated token
  issuance through the verifier and RBAC permission check.
- `auth::seed` and `auth::roles`: seed and protect builtin RBAC roles.
