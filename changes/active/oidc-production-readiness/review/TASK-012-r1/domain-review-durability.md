# Credential Durability and Refresh-Concurrency Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `adf349081077b3cfe0d56ab9a665cf01e2d86da4`
- Candidate: `29f7ae0ce8580cafc4873705b4c913e93bf7464f`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Task: `changes/active/oidc-production-readiness/tasks/TASK-012-client-oauth2.md`
- Domain: the local credential store, refresh-token rotation, cross-process renewal, logout/renewal races, and crash/retry behavior

The candidate was still the named commit when this review finished.

## Boundary and Authority Coverage

| Boundary | Authority | Source and consumer coverage | Result |
|---|---|---|---|
| One shared credential store | REQ-012; INV-005; INV-007; task prohibition on a second store | `credentials_file.rs` owns `credentials.toml`; `saved_login.rs` owns its `[[logins]]`; `ClientConfig::resolve_credential` selects it for every first-class SDK through `wyrd-client` | PASS |
| Exclusive refresh and reread | REQ-012; AC-004; task Scenario 2 | `SavedLogins::renew` takes `CredentialsFile::lock`, rereads the selected record, reuses a newly fresh access token, or calls `TokenExchange::refresh` while retaining the lock (`saved_login.rs:281-335`) | PASS |
| OAuth library migration does not bypass the owner | Task outcome and Scenario 2 | The only candidate change in the renewal owner replaces the old `TokenRequest::RefreshToken` exchange with `exchange.refresh(&login.refresh_token)` at `saved_login.rs:310-311`; the `oauth2` call remains below the existing lock/reread workflow | PASS |
| Rotated-pair persistence | REQ-012; task requirement to keep atomic replace | A successful response must contain a replacement refresh token, then updates both tokens and expiry and writes them before returning (`saved_login.rs:322-335`). `SavedLogins::write` preserves non-login TOML content and delegates to the existing same-directory temporary-file, file-`fsync`, rename, and directory-`fsync` implementation (`saved_login.rs:359-385`; `credentials_file.rs:154-175`) | PASS |
| Crash, timeout, and uncertain refresh response | REQ-012 accepted behavior | Until atomic replacement, the old record remains. A crash, timeout after server rotation, missing replacement refresh token, or write failure therefore leaves/retries the predecessor; server-side reuse detection is the specified containment behavior rather than a client-side recovery invention | PASS |
| Lock lifetime and cancellation | REQ-012; AGENTS async/runtime rules | `SavedLoginSource::mint` runs on the middleware blocking pool; `renew` holds the OS lock across the bounded HTTP refresh and local replace (`saved_login.rs:287-335`, `auth.rs:880-926`). A cancelled async waiter leaves the one spawned mint pending for the next caller instead of starting another same-middleware mint | PASS |
| Concurrent processes and logout race | REQ-012; AC-004 | The directory inode is the stable lock target across atomic file replacement (`credentials_file.rs:214-239`). Save, remove, API-key cache writes, and renewal all use that same lock. Logout selects then removes under the store owner; `remove` rereads under lock and returns the actually removed current record for revocation (`saved_login.rs:258-274`; `login.rs:167-198`) | PASS |
| Local-delete-first logout | REQ-012; locked RFC 7009 direction | CLI deletion completes before the best-effort form POST. Revocation failure warns and does not restore the local record (`login.rs:161-198`). The custom single form POST is the lead-approved loopback-compatible RFC 7009 path and was not reopened | PASS |
| Store protection and origin identity | REQ-012; TASK-004 closure incorporated by TASK-012 | Directory ownership/write-bit checks occur before lock/read; credential files must be regular, owner-only files (`credentials_file.rs:182-266`). `canonical_origin` delegates to `HttpsOrigin::of_url`, refusing userinfo and normalizing to the URL origin (`saved_login.rs:108-130`) | PASS |
| Readers and writers outside the immediate diff | REQ-012; INV-005 | Readers: `SavedLogins::{list,select}`, `ClientConfig::resolve_credential`, CLI status/logout, and Rust/Python/TypeScript clients through shared `wyrd-client`. Writers: CLI/test login via `SavedLogins::save`, renewal via `SavedLogins::renew`, logout via `remove`, and the API-key cache via the same `CredentialsFile`. No SDK-specific refresh or persistence writer was introduced | PASS |

## Failure and Recovery Evidence

- If another process already renewed, the next process acquires the same directory lock, rereads the replaced file, observes an access token outside the 60-second renewal margin, and returns it without presenting the predecessor refresh token (`saved_login.rs:287-303`).
- If the refresh endpoint refuses the token, the record is not modified and the error becomes the stable `refresh_refused` saved-login result directing the user to log in again (`saved_login.rs:310-321`).
- If the server returns success without the replacement refresh token required by the rotating public-client profile, the candidate refuses the response before mutating or writing the in-memory document (`saved_login.rs:322-327`).
- If persistence fails after rotation, no downstream guard or alternate store is introduced. The caller receives the IO failure; any later replay of the old durable token follows the approved server-side family-revocation behavior.
- A logout/renewal race serializes at the same lock. Renewal cannot recreate a record after `remove`: it rereads after acquiring the lock and returns `logged_out` when the record is absent (`saved_login.rs:287-297`). If renewal wins first, removal reads and revokes the rotated record.
- The migration keeps standard behavior: `oauth2::BasicClient::exchange_refresh_token` performs RFC 6749 refresh, while store locking, rotation persistence, and retry consequences remain with the existing credential owner. No new setting, recovery token, journal, backup store, retry loop, or bespoke concurrency mechanism was added.

## Verification Evidence and Limits

Reviewed source evidence includes the complete base-to-candidate diff, the full current bodies of `TokenExchange`, `SavedLogins`, `CredentialsFile`, `ClientConfig::resolve_credential`, CLI login/logout, the middleware renewable-mint lifecycle, and the Rust/Python/TypeScript saved-login consumers.

The task records these relevant successful checks:

- focused identity lane `client/concurrent_saved_renewal`;
- Rust, Python, and TypeScript saved-user-auth journeys;
- CLI device-login/logout journey;
- `mise run test:shared` (732 passed);
- `mise exec -- cargo nextest run --locked -p wyrd-cli --lib` (60 passed);
- client-tier, CLI-client-tier, SDK-client-tier, unwrap, format, lint, codegen, N-API, and workspace-hack checks.

`concurrent_saved_renewal` exercises four separate processes against one stale record, verifies all succeed, verifies the refresh token rotates, then refreshes the winner again to prove no predecessor replay revoked the family (`pg_auth_e2e_against_fixture.rs:201-253`). It also checks an unsafe file fails closed and that concurrent logout leaves no restored record (`pg_auth_e2e_against_fixture.rs:255-304`). Store unit coverage checks origin normalization, content preservation, owner-only file modes, unsafe directory modes, symlinks, corruption, save/select/remove, and the absence of an auxiliary lock file (`saved_login.rs:469-660`).

Per orchestration direction, this reviewer did not start Cargo or `mise` commands while the independent reviews were running. The review therefore relies on the immutable source plus the task's recorded results. No broad journey suite or aggregate is required for this domain review. The accepted Windows proof is limited to `cargo check -p webbrowser --target x86_64-pc-windows-msvc`; the unavailable Windows C cross-compiler is not a durability finding and the lead decision was not reopened.

## Proposed Findings

None.

No material durability, refresh-concurrency, credential-origin, or persistence finding was found. In particular, the library migration does not create a second refresh path or move refresh outside the lock, and it adds none of the non-standard recovery or configuration mechanisms the human direction classifies as drift.

## Result

**PASS**
