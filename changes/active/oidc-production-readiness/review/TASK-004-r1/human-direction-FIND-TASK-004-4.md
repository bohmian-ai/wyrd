# Human direction: FIND-TASK-004-4 saved-login storage

Decided by: Steven Forrester (human owner), 2026-10-02.

Resolves the SPEC_REVISION_REQUIRED verdict of `TASK-004-r1`. No spec change:
REQ-012's "user-protected credential store" is satisfied as below.

- Wyrd has **one** local credential file: `~/.config/wyrd/credentials.toml`.
  Saved user logins live in it. Delete the separate `~/.config/wyrd/logins/`
  store. Add no other credential file.
- Protection is the file's user-only ownership and mode (0600), failing closed
  when unsafe — the same as the existing `[default].api_key`. No local
  encryption: drop "sealed" from TASK-004's renewal text. A pending renewal
  keeps its refresh token in the same file, unencrypted, for per-login logout.
- Renewal and logout keep their required semantics (exclusive lock, durable
  generation, `RefreshPending`, `LoggedOut` tombstone, atomic replace), now
  against `credentials.toml`. Writes must preserve the user's other content in
  that file.
