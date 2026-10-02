# Persistent data and tenancy domain review — TASK-004 R2

## Subject and reviewed boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `06f134dc14164c040c0e5014d21de29c240f4116`
- Candidate: `362878494ed80ca5c5533a4364f744bf92dd1e06`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`
  revisions 8 and 9, plus the complete binding direction in
  `review/TASK-004-r1/human-direction-FIND-TASK-004-4.md`
- Original task: `tasks/TASK-004-laptop-clients.md`
- Prior review: `review/TASK-004-r1/verdict.md` and
  `review/TASK-004-r1/findings-validation.md`

Revision 10's REQ-021 wire encoding is owned by TASK-008 and was excluded. I
reviewed the cumulative base-to-candidate change, using the R1-to-candidate
diff only to locate remediation owners. The boundary covers tenant route-key
resolution, RLS and SQL capabilities, device authorization/login-state
persistence, saved `credentials.toml` state, and logout revocation/audit
transactionality. `.codegraph/` is absent, so source navigation used Git,
`rg`, and direct caller inspection.

## Authority and source coverage

| Boundary | Authority and source traced | Result |
|---|---|---|
| Tenant route key to durable tenant authority | REQ-011, INV-001, INV-007; `CliLogins::{authorize,approve,deny,tenant}`; `WyrdPostgres::resolve_tenant_slug`; device-code tenant prefix; `TenantConn` acquisition | PASS — the route key resolves a `DataTenantId`; device and login rows are then accessed under that tenant's RLS transaction. The device-code tenant prefix is routing only, while the hash lookup under RLS is authority. |
| SQL capability and RLS shape | `AGENTS.md` §§2, 3, 9; `architecture/agent-rules.md`; `architecture/v1/00-foundations/sql-foundation.md`; device/login-state migrations and query modules | PASS for runtime capabilities — production query functions take `&mut TenantConn<'_>`, callees do not commit, and no raw pool or cross-tenant `OperatorPool` escape entered the device path. Migration evolution fails separately as `DATA-TEN-R2-001`. |
| Device authorization through approval, callback, poll, expiry, denial, and deletion | REQ-011, AC-004, AC-007; `auth_device_authorizations`; `auth_login_state`; `CliLogins::{approve,deny,redeem}`; `HumanConnections::begin_bound`; callback session issuance | FAIL — `DATA-TEN-R2-002`. The live device row and its login state are not one durable authority, so expiry/deletion can be followed by a new orphan login state that still mints a renewable session. |
| Saved credential identity and selection | REQ-012; revision 8; R1 human direction and addenda; `ClientConfig::resolve_credential`; `SavedLogins::{save,select,renew,remove}` | PASS — records are keyed by canonical origin plus tenant route key; a selector matches only that key, absence selects the last record for the origin, and a selected workload tenant wins over ambient `WYRD_TENANT`. Self-naming bearer/API-key credentials refuse a simultaneous selector. |
| `credentials.toml` ownership, preservation, locking, and atomicity | R1 human direction as superseded by revision 8; `CredentialsFile`; `SavedLogins::{read,write}`; API-key token cache | PASS — one 0600 owner-only file is used; unsafe files fail closed for saved-login resolution; writes take the directory lock, reread, replace atomically, sync file and directory, and preserve unrelated TOML content. No second login/token file, encryption layer, generation marker, tombstone, lock deadline, or per-login version remains. |
| Renewal and logout local lifecycle | REQ-012 revision 8; `SavedLogins::renew`; CLI `logout`; `TokenExchange` | PASS — renewal rereads and refreshes under the blocking file lock, stores the returned rotation, and asks for login again on refusal. Logout removes locally before best-effort server revocation and does not restore the record after failure. |
| Per-login revocation scope and canonical audit transaction | REQ-012, REQ-017; repository audit authority; `CliLogins::end`; refresh-family queries; `logout_revokes_only_its_own_chain` | PASS — RLS hash lookup establishes the stored tenant/principal, the principal-family lock covers chain revocation, one canonical audit append shares the `TenantConn` transaction, append failure rolls back, and a sibling login chain remains active. |

## Prior-finding closure

| Prior finding | Source evidence | Closure |
|---|---|---|
| `FIND-TASK-004-1` — tenant selector did not bind all credential tiers | `ClientConfig::refuse_selector` refuses a selector beside bearer/API-key sources; `workload_token_from_env` gives the explicit selector precedence; `SavedLogins::select` accepts only a tenant route key and does not fall through on mismatch. | CLOSED under the binding addendum. No tenant-id selector or token-claim comparison remains. |
| `FIND-TASK-004-4` — saved-login storage lacked approved protection/ownership | `CredentialsFile` and `SavedLogins` use only `credentials.toml`, enforce owner/mode checks, lock and atomic replacement, and preserve unrelated TOML. Revision 8 removed the earlier generation/pending/tombstone requirements. | CLOSED under the human direction and revision 8. |
| `FIND-TASK-004-7` — logout revocation lacked canonical audit | `CliLogins::end` lines 417-452 appends `auth.cli_login.logout` before the shared transaction commits; `logout_revokes_only_its_own_chain` proves one event, sibling-chain survival, and rollback on refused audit insert. | CLOSED. |

## Material findings

### DATA-TEN-R2-001 — registered migrations are rewritten instead of evolved

- **Classification:** VIOLATION / persistent-data upgrade regression.
- **Violated obligation:** `AGENTS.md` §11 and
  `architecture/v1/00-foundations/sql-foundation.md` require registered
  migration files to remain immutable and checksum-verifiable. An existing
  deployment must apply later migrations; it cannot receive a different body
  for a migration already recorded in its ledger.
- **Location:**
  `crates/wyrd/wyrd-sql/migrations/20260925000001_auth_login_state_binding.sql:9-16,33-45`
  and
  `crates/wyrd/wyrd-sql/migrations/20261001000002_auth_connection_test_state.sql:3-18`.
- **Evidence:** Both files exist at the immutable base. The candidate changes
  the first migration's stored column from `cli_handoff_id` to `device_id`,
  its check expression and index, then changes the later migration to refer to
  that rewritten column. The new
  `20261002000000_auth_device_authorizations.sql` creates the device table but
  contains no schema evolution from the base column. A database whose ledger
  contains the base checksums therefore fails schema validation; if checksum
  validation were bypassed, it would still have `cli_handoff_id` while the
  candidate queries bind `device_id`.
- **Observable consequence:** Upgrading an existing base deployment cannot
  boot the candidate or serve login. Fresh-database test success does not prove
  the supported upgrade path.
- **Required testable correction:** Restore both registered migrations exactly
  to their base contents. Express the handoff-to-device schema change in the
  new later migration using ordinary PostgreSQL schema evolution, preserving
  existing ledger checksums and ending with the schema current queries expect.
  Do not weaken checksum validation or add an alternate migration ledger.
  Focused proof must migrate a database through the base set, then apply the
  candidate set and validate its schema and ledger; the ordinary clean-database
  migration test must remain green.

### DATA-TEN-R2-002 — device approval can mint a credential after its authorization expired or was deleted

- **Classification:** INCORRECT / durable lifecycle violation.
- **Violated obligation:** REQ-011 and AC-007 require an expired, denied, or
  already-ended device code to yield no Wyrd credential or browser session.
  Revision 8 requires conventional RFC 8628 device-grant behavior rather than
  a parallel custom lifecycle.
- **Location:**
  `crates/wyrd/wyrd-auth/src/cli_logins.rs:194-218,327-369`;
  `crates/wyrd/wyrd-auth/src/login.rs:100-158`;
  `crates/wyrd/wyrd-auth/src/callback.rs:293-344`;
  `crates/wyrd/wyrd-sql/migrations/20261002000000_auth_device_authorizations.sql:13-33`.
- **Evidence:** `CliLogins::approve` reads an unexpired, undenied device row in
  one transaction, commits it, performs provider discovery, and only later
  calls `begin_bound`, which opens another transaction and inserts a login
  state containing the bare `device_id`. There is no foreign key or live-row
  predicate connecting that insert to `auth_device_authorizations`. During
  that gap a denial or an expired-code poll can delete the device row and any
  login state then present; `begin_bound` can subsequently insert a new orphan
  state. Even without that race, approval just before device expiry gives the
  new login state its own five-minute lifetime. The callback then calls
  `issue_human_session`, which inserts a durable refresh row, before sealing
  the completion. A later token poll observes the device expiry and deletes
  only the device/login-state rows; the issued refresh row is left active but
  unreachable.
- **Observable consequence:** A browser can complete a login after the device
  grant has expired or been denied, creating a tenant User/session refresh
  credential that the CLI never receives and cannot revoke. This contradicts
  the required terminal device-code outcome and leaves orphan renewable
  authority in persistent state.
- **Required testable correction:** Keep one conventional device-authorization
  lifecycle authoritative from user approval through token redemption: an
  expired, denied, deleted, or already-redeemed device authorization must not
  admit a later login-state/session issuance, and Wyrd credentials must be
  issued only while the token endpoint atomically redeems the still-live
  authorization. Reuse the existing tenant RLS transaction, login verification,
  and human-session issuer; do not add a second grant, store, cleanup service,
  or custom replay mechanism. Focused Postgres proof must pause approval
  between its lookup and binding, expire or deny and poll/delete the device
  authorization, then resume approval and complete the provider callback; it
  must show no login completion, browser session, or `auth_refresh_tokens` row.
  A normal approved code must still redeem exactly once.

## Verification limits

- I inspected the full cumulative diff and the complete bodies and sibling
  consumers named above. `git diff --check` passed, and `HEAD` remained
  `362878494ed80ca5c5533a4364f744bf92dd1e06` through the review.
- The task packet records revision-8 format, lint, codegen, boundary, Rust,
  Python, TypeScript, Postgres, CLI, and identity lanes as exit 0. I did not
  rerun those lanes independently.
- The recorded migration tests exercise the candidate migration set on a clean
  database, not upgrade from the immutable base ledger. The device tests cover
  sequential pending, slow-down, approval, denial, expiry, replay, and logout,
  but do not interleave expiry/deletion between approval's first transaction
  and `begin_bound`, nor assert that the terminal path leaves no refresh row.

## Result

**FAIL** — `DATA-TEN-R2-001` and `DATA-TEN-R2-002` are material persistent-data
and lifecycle defects. The R1 tenant-selector, credential-file, and logout-audit
findings are closed.
