# Concurrency and durability domain review

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `06f134dc14164c040c0e5014d21de29c240f4116`
- Candidate: `362878494ed80ca5c5533a4364f744bf92dd1e06`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`
  revision 10, with revisions 8 and 9 superseding conflicting TASK-004 text;
  `TASK-004-laptop-clients.md`; the complete round-1 verdict and findings; and
  `human-direction-FIND-TASK-004-4.md`, including its addenda.
- Scope exclusion: revision 10 REQ-021's OAuth form encoding and RFC 6749 wire
  envelopes belong to TASK-008 and were not evaluated here.
- Candidate stability: `HEAD` was the requested candidate before and after
  this review. `.codegraph/` is absent, so source navigation used Git, `rg`,
  and direct caller inspection.

## Reviewed boundary and source coverage

| Boundary | Source and consumer paths inspected | Result |
|---|---|---|
| One credential file and writer serialization | `CredentialsFile::{lock,document,replace,read_text,cache_api_key_token}` in `crates/shared/wyrd-client/src/credentials_file.rs:65-267`; `SavedLogins::{save,remove,renew,read,write}` in `saved_login.rs:151-379`; API-key persistence in `auth.rs:603-617,800-821` | PASS. Every saved-login and API-key-cache mutation uses the same blocking OS lock on the stable configuration directory, rereads after acquiring it, atomically renames a same-directory `0600` temporary file, and syncs both file and directory. The editable TOML write preserves unrelated user content. |
| Saved-login refresh across processes | `SavedLogins::renew` and `SavedLoginSource::mint` in `saved_login.rs:268-430`; `AuthMiddleware::{bearer,force_refresh,mint_into}` in `auth.rs:493-680`; configuration callers and all SDK consumers | PASS. A stale process takes the file lock, rereads, and either reuses the access token another process saved or performs one refresh while retaining the lock through the atomic save. Process-local single-flight prevents sibling callers from starting duplicate refreshes. A cancelled waiter leaves the blocking mint running and joinable by a later caller; it does not begin a second mint. |
| Crash and refresh refusal | `saved_login.rs:290-328`; spec REQ-012 and revision-8 history at `spec.md:154-167,413-423` | PASS. Server refusal asks for login again. Transport failure leaves the old record unchanged. A crash after server rotation but before save can replay the old refresh token and trigger server reuse containment; revision 8 explicitly accepts that conventional consequence. No `RefreshPending`, generation, tombstone, deadline, or retry state remains. |
| Logout racing refresh | `SavedLogins::remove` in `saved_login.rs:244-266`; CLI `logout` in `crates/wyrd/wyrd-cli/src/auth/login.rs:180-220`; server `CliLogins::end` in `crates/wyrd/wyrd-auth/src/cli_logins.rs:400-453` | PASS. Removal and renewal serialize on the same file lock. Whichever local mutation wins is preserved: removal after renewal returns the rotated refresh token for revocation; renewal after removal observes `logged_out` and cannot recreate the record. Local deletion precedes best-effort server revocation, as revision 8 requires. Server revocation retains the existing per-refresh-family serialization and transactionally commits its audit event. |
| Device authorization persistence and poll cadence | Migration `20261002000000_auth_device_authorizations.sql:1-33`; `device_authorizations.rs:21-193`; `CliLogins::{authorize,approve,deny,redeem}` in `cli_logins.rs:110-398` | PASS. The tenant row holds a device-code hash, one user code, denial, expiry, and the RFC 8628 poll timestamp. PostgreSQL owns time, forced RLS owns tenant visibility, and the token poll locks the row before updating cadence or consuming state. `last_polled_at` and `slow_down` implement RFC 8628 section 3.5 rather than a Wyrd-specific coordination mechanism. |
| One-use redemption and replica concurrency | `POLL_DEVICE_AUTHORIZATION_SQL` and `DELETE_DEVICE_AUTHORIZATION_SQL` in `device_authorizations.rs:52-81`; `REDEEM_LOGIN_COMPLETION_SQL` in `login_state.rs:81-93`; `CliLogins::redeem_in` in `cli_logins.rs:310-398` | PASS. Concurrent replicas serialize on the device row. Completion redemption, device/login-state deletion, allowed audit append, and commit share one tenant transaction. One committing poll returns the credential; later or concurrent polls find no device row. Pending and `slow_down` polls commit only their cadence update. Expired or denied rows and their bound login state are removed together. |
| Durable and journey proof | `saved_login::tests::{selection_picks_the_named_or_newest_login,logins_live_in_credentials_toml_beside_user_content,unsafe_and_corrupt_stores_fail_closed}`; `auth::tests::concurrent_bearer_callers_single_flight`; `pg_auth_e2e_against_fixture::concurrent_saved_renewal`; `cli_logins::pg_tests::device_codes_poll_approve_deny_and_expire`; `cli_login_journey` | PASS. Unit proof covers content preservation and fail-closed file handling. The real-server multi-process journey proves one rotation, reread/reuse by the other processes, continued renewability, unsafe-file refusal, and logout/renewal serialization. Device tests cover pending, cadence, denial, expiry, wrong code, one binding, successful redemption, and replay. |

## Prior finding closure under revised authority

- **FIND-TASK-004-2 / round-1 DCD-1:** closed by superseding authority, with
  source closure. Revision 8 explicitly removed durable generations and
  per-request revalidation (`spec.md:413-423`); candidate source has no
  generation field or cache-revalidation hook. Fresh access tokens are reused
  in memory until ordinary refresh skew (`auth.rs:510-520`), matching the
  approved conventional client behavior. Requiring the old mechanism would be
  DRIFT.
- **FIND-TASK-004-3 / round-1 DCD-2:** closed by superseding authority, with
  source closure. Revision 8 removed `RefreshPending` and the custom lock
  deadline and accepted crash-after-rotation reuse containment. The candidate
  has a plain blocking file lock (`credentials_file.rs:214-239`) and no
  `RefreshPending`, generation, tombstone, `lock_timeout`, or per-login format
  version in the reviewed client sources. The earlier uncertain-timeout and
  lock-timeout proof obligations therefore no longer apply.

## Standard-practice and drift assessment

No nonstandard concurrency or durability mechanism remains in this boundary.
The candidate deletes the custom handoff rows, refresh-pending state machine,
generation revalidation, tombstone, lock deadline, and per-login version. The
remaining mechanisms are the conventional minimum: one OS file lock with
reread/save for local refresh, atomic file replacement for credentials, RFC
8628 device authorization rows with a poll interval, and a database row lock
plus one transaction for single-use redemption. I found no extra check, file,
setting, or option to classify as DRIFT.

## Material proposed findings

None.

## Verification limits

- I inspected the complete cumulative base-to-candidate diff and the focused
  revision-8 remediation diff. I ran `git diff --check`; it passed. I did not
  rerun the Postgres/Keycloak lanes.
- The task records isolated exit 0 for `fmt`, `lints`, boundary/codegen checks,
  `test:shared` (723 passed), `test:cli:journey` (33 passed),
  `test:principals:integration`, every identity target including `client`, and
  the unfiltered identity journey (`TASK-004-laptop-clients.md:353-385`).
- Device redemption replay is driven sequentially in the journeys rather than
  by a deliberately scheduled simultaneous two-replica poll. The common
  `FOR UPDATE` row lock and single tenant transaction establish the winner in
  source; no reachable atomicity gap was found, so this is a residual proof
  limit, not a finding.
- The accepted crash-after-server-rotation consequence is not treated as a
  missing recovery test or as a requirement to restore the deleted custom
  state machine.

## Overall result

**PASS**

The candidate satisfies the approved concurrency and durability boundary. The
round-1 cache-coherence and uncertain-timeout findings are superseded and
their nonstandard mechanisms are absent; the remaining file and SQL paths use
the standard owners and serialize refresh, logout, polling, and one-use
redemption without a material defect found.
