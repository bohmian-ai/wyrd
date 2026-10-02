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

## Addendum: tenant selector and token cache (same date)

- The `tenant` selector takes a **tenant key only** (the `/t/{tenantKey}/login`
  slug). It picks the saved login, and for a workload token it is the tenant
  the jwt-bearer exchange requests. With an explicit, bearer, or API-key
  credential, a set `tenant` is refused: that credential already names its
  tenant. No tenant-id selector. This resolves FIND-TASK-004-1 per REQ-012.
- Delete `~/.config/wyrd/tokens`. The access token exchanged from an API key
  is cached in `credentials.toml` next to that key, through the same locked
  writer as saved logins.
- Saved-login renewal does what gh, gcloud, az, and aws sso do: send the
  refresh token, store what the server returns, use it. The client does not
  decode the renewed access token to compare its tenant; the server already
  binds a refresh token to its login. Delete that check and its test.

## Addendum: standard practice (spec revision 8, same date)

Spec revision 8 supersedes the "Renewal and logout keep their required
semantics" bullet above. Drop `RefreshPending`, the durable generation and
per-request revalidation, the `LoggedOut` tombstone, the custom lock deadline
and `lock_timeout` error, and the per-login `format_version`. Keep the plain
blocking lock, atomic replace, 0600 fail-closed file checks, and preservation
of the user's other content. Default to the most recent login for a server
when no `tenant` is given. Replace the custom CLI handoff (TASK-002 server
handoff rows and claim polling included) with the RFC 8628 device-code grant;
the browser handoff is unchanged.
