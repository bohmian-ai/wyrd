# Human direction: FIND-TASK-003-18 logout scope

Decided by: Steven Forrester (human owner), 2026-10-02.

This direction replaces the "Required correction" and "Focused closure proof"
of FIND-TASK-003-18 in `TASK-003-R5-logout-refresh-family-retirement.md`.
Authority: approved spec revision 7, REQ-009 ("Logout ends the Wyrd browser
session"); RFC 7009 §2.1 (revocation is scoped to the same authorization
grant); RFC 9700 §4.14.2 (principal-wide revocation is the response to
detected refresh reuse, already implemented by FIND-TASK-003-14).

Browser logout revokes **this login's refresh chain only**, never every refresh
token the `User` holds. Other browser sessions and CLI/SDK logins stay valid.

Required correction:

- OIDC mode: inside the existing logout transaction, take the existing
  `lock_refresh_family` lock, then revoke every active refresh row in this
  session's chain — its starting token and all descendants via the existing
  `rotated_from` link — before wiping the browser row. Do not call
  `revoke_refresh_family` from logout.
- Logout must not depend on decrypting the stored refresh envelope. If the
  browser row cannot identify its chain without decryption, store the
  non-secret refresh token id on the browser row; that is the only permitted
  new state. No new table, API, abstraction, or audit owner.
- API-key mode: unchanged (browser-only wipe; operator key stays valid).

Closure proof (existing module, Postgres): rotate the browser session's
refresh token through the ordinary refresh owner, make the stored refresh
envelope unreadable, log out, and prove the browser row is wiped, every token
in that chain is revoked and the successor cannot rotate, while a second
refresh token for the same `User` from a separate login stays active. Retain
the API-key logout proof.
