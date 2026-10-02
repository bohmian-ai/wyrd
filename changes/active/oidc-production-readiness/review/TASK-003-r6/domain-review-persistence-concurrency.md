# Persistence, concurrency, and durability review

## Immutable subject and authority

- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `ad3b92ad0f326917c731383fd3b57cb7ac6a8c82`
- Latest remediation range inspected: `989d0734b0a9b04f314ef4b52aa7d8510f26fe11..ad3b92ad0f326917c731383fd3b57cb7ac6a8c82`
- Approved authority: `changes/active/oidc-production-readiness/spec.md` revision 7; `TASK-003-production-ui.md`; R2, R3, R4, and R5 remediation tasks; and the three supplied human directions. The R5 direction supersedes family-wide logout revocation: logout must retire only the current browser session's refresh chain while RFC 9700 reuse containment remains principal-wide.
- Repository authority: `AGENTS.md`, `architecture/agent-rules.md`, `architecture/references/languages/spec-driven-development.md`, `architecture/references/languages/maintainer-style.md`, and the applicable Bifrost shutdown authority.

The candidate commit was still `ad3b92ad0f326917c731383fd3b57cb7ac6a8c82` after source inspection and focused verification.

## Boundary and source coverage

| Boundary | Source and caller coverage | Result |
|---|---|---|
| Browser-session schema and tenant isolation | `20261001000001_auth_browser_sessions.sql`, `20261001000003_auth_browser_session_refresh_chain.sql`, `wyrd-sql/src/queries/auth/browser_sessions.rs`, and the `WyrdPostgres` hash-to-tenant resolver | PASS — opaque hash lookup exposes only the owning tenant; all row reads and writes then use forced-RLS `TenantConn`. Live-row mode constraints, the tenant-scoped refresh FK, and sealed-value wipe rules preserve the durable session shape. |
| Connection-test persistence | `20261001000002_auth_connection_test_state.sql` and `wyrd-sql/src/queries/auth/login_state.rs` | PASS — exactly one initiation binding is stored, callback consumption remains atomic and single-use, and test-bound state cannot carry a redeemable completion. |
| Browser session creation and completion | `wyrd-auth/src/browser_sessions.rs::complete`, `::exchange_api_key`, `::insert`; refresh-row issuance and lookup owners | PASS — completion redemption, refresh-chain identification, sealed credential storage, and session insertion share one caller-owned tenant transaction and one commit. An error drops or explicitly resolves the transaction without a partial session. |
| Renewal and replica races | `BrowserSessions::current`/`renew`, `lock_browser_session`, `rotate_browser_session`, `RefreshTokens::execute`, and `lock_refresh_family` | PASS — the browser row lock serializes replicas; OIDC renewal takes row then family lock consistently; the advisory family lock encloses classification, rotation, replay containment, and logout chain retirement. Refusal, containment, and internal-failure paths retain their distinct commit/rollback semantics. |
| Logout chain retirement | `BrowserSessions::logout`, `revoke_refresh_chain`, refresh-token `rotated_from` FK and issuance | PASS — logout locks the live browser row, takes the existing user-family advisory lock, recursively revokes the stored root and all descendants, wipes the browser row, and commits once. API-key mode has no chain id and only wipes browser state. Unknown, expired, and already-ended sessions remain idempotent no-ops. |
| Transaction ownership and lock order | All changed auth query functions and their `BrowserSessions`, refresh, callback, and issuance callers | PASS — query functions do not commit or roll back `TenantConn`; workflow owners retain the boundary. Browser renewal and logout use browser row then family lock; public refresh uses family lock and does not acquire a browser row, so no reverse-order cycle was found. |
| Sealing-key persistence and replica rotation | `wyrd-auth/src/sealing.rs`, `SealedSecretTable` inventory/CAS queries, browser-session sealed columns, and boot rewrap checks | PASS — all four browser ciphertext columns, including expired-but-unpurged rows, are inventoried. Per-column compare-and-swap prevents a rewrap pass from overwriting concurrent renewal or logout, and a lost race remains visible for a later pass. |
| Migration application and compatibility | Migrations `000001` through `000003` and `wyrd-sql/tests/pg_migration.rs` | PASS — the cumulative base-to-candidate deployment creates the browser-session table before adding its chain column and constraints; the full empty-database migration set is idempotent. No supported pre-candidate schema contains production browser-session rows requiring backfill. |
| In-process harness shutdown and database-drop ordering | `wyrd-testing/src/server.rs::shutdown`, `AppState::shutdown_token`, `Bifrost::shutdown`, production shutdown/boot rollback patterns, and `PgFixture`-using server tests | PASS — in-process shutdown now cancels shared background work and awaits Bifrost's bounded drain/abort before moving the harness into its final blocking drop, preventing Oracle/Scribe work from surviving into forced database deletion. Bound mode retains its production serve-task drain. |

## Failure and recovery assessment

- A SQL, advisory-lock, recursive revocation, browser wipe, or commit failure in logout returns before a successful commit; dropping `TenantConn` rolls the complete operation back. A retry either performs the complete logout or observes the already-ended row.
- A refresh rotation committed before logout is visible after the family lock is acquired and is included through `rotated_from`. A rotation that reaches the family lock after logout observes revoked state and cannot create an escaping successor. Browser-side renewal also holds the browser row, so logout and renewal serialize across replicas.
- The durable `refresh_chain_id` is non-secret, tenant-bound by FK, retained after browser revocation, and independent of opening the refresh envelope. Losing an old sealing key therefore cannot skip logout retirement.
- An API-key session has no refresh-chain identity. Logout wipes its recoverable copy without mutating the underlying API-key row, preserving the mode-specific contract.
- The in-process test harness uses Bifrost's existing shutdown owner, whose failure path awaits abort. The fixture is dropped only afterwards, so a failed graceful drain does not leave the known reader-epoch failure path running against a removed database.

## Material proposed findings

None.

The only apparent broad-revocation edge is a later presentation of a token from the logged-out chain through the public refresh endpoint. That path is not logout's revocation scope: it is the separately approved RFC 9700 reuse-containment authority, and the supplied human direction explicitly preserves its principal-wide response. It is therefore not a persistence finding against R5.

## Verification evidence and limits

Executed against the immutable candidate with repository-managed Postgres:

- `migrations_apply_and_are_idempotent` for `wyrd-sql`: PASS.
- `migrations_apply_and_are_idempotent` for `vala-sql`: PASS.
- `browser_sessions::pg_tests::oidc_logout_retires_only_its_refresh_chain_without_opening_it`: PASS.
- `pg_card_registration_route::card_reads_list_latest_and_delete_are_tenant_safe`: PASS, including explicit in-process `WyrdTestServer::shutdown` without the prior reader-epoch abort.

The task evidence also records successful `fmt`, `lints`, `codegen:check`, `check:tenant-isolation`, `test:sql`, `test:wyrd`, and `test:identity:journey` runs after the R5 and harness changes. I did not independently rerun those full aggregate lanes or inject SQL/commit failures. Atomicity conclusions for those failure paths are based on the single `TenantConn` boundary, statement ordering, and rollback-on-drop semantics in current source. The focused logout proof uses a committed successor rather than an active two-task scheduler race; the shared transaction-scoped family lock is the production serialization primitive, and its concurrent rotation/reuse behavior is covered by the existing refresh tests.

## Overall result

**PASS**

No material persistence, tenancy, concurrency, migration, sealing, or shutdown-lifecycle defect remains within the approved TASK-003 scope.
