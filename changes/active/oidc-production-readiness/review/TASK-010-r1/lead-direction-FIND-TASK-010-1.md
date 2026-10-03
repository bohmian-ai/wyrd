# Lead direction: FIND-TASK-010-1 routed to TASK-011

FIND-TASK-010-1 asks TASK-010 to add its own `openid-client` 6.8.8 driver
against the real server. TASK-011 builds the production `wyrd-ui` client on
`openid-client` 6.8.8, and its BFF journey drives code + PKCE, refresh and
revoke against this same server. A second, test-only `openid-client` driver
here would duplicate that proof.

Correction in TASK-010: none. TASK-011's BFF journey is the closure proof,
and the change review checks it on the integrated candidate. Every other
finding in `TASK-010-R1-authorization-server-corrections.md` stands.
