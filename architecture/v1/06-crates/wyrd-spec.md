# wyrd-spec

`wyrd-spec` owns pure auth wire contracts. It remains IO-free, async-free, and
PyO3-free.

Auth DTOs:

- `SecretBearer`: string-shaped secret field with redacted `Debug`,
  password/write-only schema metadata, and explicit `expose()` call sites.
- `IssueKeyRequest` and `IssueKeyResponse`: card-bound API-key issuance
  contracts.
- `TokenRequest`: `grant_type` tagged union for Wyrd API-key exchange and RFC
  8693 token exchange.
- `RequestedSubject`: token-exchange target by principal id or structured
  `CardRef`.
- `TokenResponse`: access token, refresh token, token type, and expiry. It does
  not duplicate `card_ref`; the binding lives inside the JWT claims.

`CardRef` also supports canonical `Display` and `FromStr` for CLI and
agent-authored workflows.
