# Human direction: FIND-TASK-003-1 issuer binding

Decided by: Steven Forrester (human owner), 2026-10-01.

Wyrd human login must stay provider agnostic. This direction replaces the
FIND-TASK-003-1 section and R1-AC-01 of
`TASK-003-R1-production-ui-remediation.md`; every other finding stands as
written.

Do not refuse connection test or activation because a provider does not
advertise `authorization_response_iss_parameter_supported`. Microsoft Entra,
Okta, and Auth0 do not advertise it, and approved REQ-007 does not require it.

Apply RFC 9207 conditionally in the existing `AuthorizationCodeExchange` owner,
after resolving state and before any token-endpoint IO:

- `iss` present: it must equal `LoginState.issuer` by exact string comparison,
  else consume and refuse the flow (audited, no token call, no session).
- `iss` absent and the provider's metadata advertises support: refuse the same
  way.
- `iss` absent and support not advertised: proceed; server-bound state, PKCE,
  and ID-token issuer validation remain the controls.

`iss` stays a typed callback field; unrelated provider parameters stay
tolerated. Document the residual mix-up exposure for non-advertising providers
(it requires a malicious tenant connection on the shared callback) in the
security documentation. Per-connection callback URLs are the future upgrade if
that exposure ever needs closing; they are not part of this change.

Revised R1-AC-01: matching `iss` completes; mismatched `iss`, or missing `iss`
from an advertising provider, consumes and refuses the flow before any
token-endpoint call and creates no completion or session; missing `iss` from a
non-advertising provider completes; a non-advertising provider can be tested
and activated; unrelated response parameters remain tolerated; schemas
regenerate.
