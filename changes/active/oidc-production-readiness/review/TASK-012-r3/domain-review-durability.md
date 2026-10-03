# Credential Durability and Refresh-Concurrency Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `adf349081077b3cfe0d56ab9a665cf01e2d86da4`
- Candidate: `dc67bf1c31c3fc6f4e6e05744b75b9c83e9fa447`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 11, especially REQ-012 and AC-004
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-012-client-oauth2.md`
- Remediations: `review/TASK-012-r1/TASK-012-R1-client-oauth2-closure.md` and `review/TASK-012-r2/TASK-012-R2-documentation-closure.md`
- Reviewed domain: saved-login persistent state, cross-process refresh serialization, atomic replacement, restart behavior, logout ordering, and the interaction between file-level serialization and in-process cache/single-flight

The repository `HEAD` was still the named candidate immediately before this report was written.

## Reviewed Boundary, Authority, and Source Coverage

| Boundary | Governing obligation | Source and caller evidence | Result |
|---|---|---|---|
| One conventional saved-login store | REQ-012; original task prohibition on a second credential store; human direction against a retry journal, backup token, or recovery protocol | `CredentialsFile` owns the one `credentials.toml`; `SavedLogins` owns its `[[logins]]`; `ClientConfig::resolve_credential` selects that store for every SDK through `wyrd-client` (`credentials_file.rs:1-11`, `saved_login.rs:149-155`, `config.rs:161-204`). The cumulative diff adds no second store or recovery state. | PASS |
| Stable exclusive lock and reread | REQ-012; AC-004; task Scenario 2 | `CredentialsFile::lock` locks the configuration-directory inode, which remains stable across replacement (`credentials_file.rs:214-239`). `SavedLogins::renew` acquires it before rereading and keeps it through refresh and persistence (`saved_login.rs:274-334`). | PASS |
| Reuse another process's rotation | REQ-012: reread first and avoid replaying the predecessor | After the locked reread, a token outside the 60-second margin is returned without refresh (`saved_login.rs:285-301`). Therefore a waiting process consumes the pair the lock winner saved instead of presenting the old refresh token. | PASS |
| Rotating refresh remains inside the persistence owner | REQ-012; RFC 6749 section 6; RFC 9700 section 4.14.2; R1 preserved behavior | The task's runtime change replaces only the call below the lock with `TokenExchange::refresh`; that method is the `oauth2` refresh path. No SDK-specific or public arbitrary refresh writer survives (`saved_login.rs:302-329`, `auth.rs:372-393`). | PASS |
| Atomic, durable pair replacement | REQ-012; task requirement to retain atomic replacement | Renewal requires the successor refresh token, updates the refresh token, access token, and expiry together, and writes once before returning (`saved_login.rs:320-333`). The write preserves unrelated TOML and delegates to a same-directory `0600` temporary file, file `fsync`, rename, and directory `fsync` (`saved_login.rs:357-383`, `credentials_file.rs:154-175`). | PASS |
| Store safety | REQ-012 user-protected store; task Scenario 3 | The leaf directory must be owner-owned and not group/world writable; the credential file must be a regular, owner-owned, private file. Symlinks and unsafe modes fail closed before reads or writes (`credentials_file.rs:182-266`, `credentials_file.rs:307-331`). | PASS |
| Canonical server and tenant identity | REQ-012 server identity and tenant selection; AC-004; R1 FIND-TASK-012-2 closure | `canonical_origin` consumes the same `HttpConfig::validate` origin used by `TokenExchange` and `HttpTransport`, rejecting userinfo and collapsing case/default-port/path/query/fragment spellings (`saved_login.rs:108-128`). `SavedLogins::select` filters by that origin, selects the named tenant, refuses a missing named tenant, or returns the last matching record as the newest (`saved_login.rs:200-235`). | PASS |
| In-process cache and single-flight | REQ-012 automatic renewal; task requirement to retain `AuthMiddleware` caching/single-flight | The renewable source is cached behind `AuthMiddleware::cache`; a stale entry enters `mint_into`, whose `pending_mint` retains one blocking mint across waiter cancellation (`auth.rs:809-819`, `auth.rs:932-979`). `SavedLoginSource::mint` then enters the cross-process file-lock owner (`saved_login.rs:386-435`). Cache scope does not replace or bypass the durable reread. | PASS |
| Process restart recovery | REQ-012 routine reuse without an IdP visit | A new client resolves the current saved record through `ClientConfig::resolve_credential`; the source retains only origin and tenant and every mint rereads the store (`config.rs:194-200`, `saved_login.rs:386-435`). No in-memory generation is required to recover after restart. | PASS |
| Crash between rotation and save | REQ-012 explicitly accepted consequence; revision 8 decision | Until replacement completes, the predecessor remains the durable record. A crash or lost response after server rotation therefore causes a later replay, which server-side reuse detection treats as theft and forces login again. The candidate does not invent a nonstandard local recovery protocol (`saved_login.rs:308-333`; `auth.rs:372-385` rustdoc). | PASS |
| Logout racing renewal | REQ-012 local deletion before best-effort revocation | `remove` acquires the same directory lock, rereads, deletes, atomically writes, and returns the record actually removed (`saved_login.rs:250-272`). If renewal wins, removal gets the rotated record; if removal wins, renewal rereads no record and returns `logged_out` instead of recreating it (`saved_login.rs:285-295`). | PASS |
| Local-first revocation | REQ-012; locked RFC 7009 direction | CLI logout completes the blocking remove before building the exchange or issuing the one redirect-free RFC 7009 form POST. Failure warns and never restores the local record (`wyrd-cli/src/auth/login.rs:158-198`). | PASS |
| R2 documentation-only closure | TASK-012-R2 constraint against runtime changes | The latest fix changes Rustdoc, manifest wording, generated schema descriptions, and review evidence only. It does not change locking, persistence, selection, refresh, cache, or logout execution. | PASS |

## Failure and Recovery Trace

- **Two or more stale processes:** the first lock holder rereads, refreshes once, saves the rotated pair, and releases the directory lock. Each waiter then rereads the replacement and returns its still-fresh access token without replaying the predecessor refresh token.
- **Cancellation before refresh completes:** the spawned blocking mint continues to own the file lock. `pending_mint` keeps that join handle for the next caller, preventing a second mint in the same middleware; other processes wait on the OS lock.
- **Crash or lost response after server rotation:** the old durable token can remain. Retrying it may retire the refresh family, exactly the failure mode REQ-012 accepts. A fresh interactive login is the recovery; no journal, shadow token, retry marker, or backup store is warranted.
- **Successful response followed by a local write failure:** renewal does not return the new access token unless `write` succeeds. A later reread observes whichever atomic replacement is durable; replay of a predecessor follows the same approved server containment behavior.
- **Renewal versus logout:** both serialize through the directory lock. A late renewal cannot recreate a removed login because it rereads only after acquiring the lock. Logout revokes the current record when renewal completed first.
- **Revocation outage or cancellation:** local deletion is already complete. The CLI warns that revocation was not confirmed and retains no deleted secret for retry, matching the approved local-first, best-effort convention.

## Remediation Closure Relevant to This Domain

- `FIND-TASK-012-1` remains closed: the arbitrary public form exchange was deleted; saved-login renewal reaches refresh only through the installed `oauth2` method while the existing lock owner is held.
- `FIND-TASK-012-2` remains closed: auth transport and persistence use the same parsed origin, so process restarts and alternate URL spellings select the same `(origin, tenant)` record without an alias or migration mechanism.
- `FIND-TASK-012-3` remains closed for this boundary: refresh now documents server-side rotation before response receipt and the replay consequence; revocation documents idempotency. R2 corrects remaining documentation only.
- `FIND-TASK-012-4` was an evidence-recording issue and did not alter the durability or concurrency owners.

## Verification Evidence and Limits

Reviewed the complete base-to-candidate diff, the latest remediation diff, and current source/callers for `CredentialsFile`, `SavedLogins`, `SavedLoginSource`, `ClientConfig::resolve_credential`, `AuthMiddleware::{bearer,force_refresh,mint_into}`, `TokenExchange::{refresh,revoke_refresh_token}`, and CLI logout.

The immutable task evidence records the focused `client/concurrent_saved_renewal` identity journey as passing. Its current source launches four separate child processes against one stale saved login, proves all succeed, proves refresh rotation, rotates the winner again to demonstrate that no predecessor replay retired the family, checks unsafe-store refusal, and races three renewers against removal (`pg_auth_e2e_against_fixture.rs:201-306`). The CLI journey proves selected-chain revocation, preservation of another login, canonical-origin selection, and successful local deletion with a warning after server shutdown (`cli_login_journey.rs:372-436`). Saved-login unit coverage exercises canonical origin, named/newest selection, content preservation, directory/file permissions, symlink and corruption refusal, and save/remove behavior (`saved_login.rs:467-658`).

No command was rerun for this domain report. The candidate's latest remediation is documentation-only, and the prior immutable evidence contains the exact focused process-level and CLI proofs for the unchanged runtime owners. Per the human direction, this review neither requires nor recommends a full journey suite or aggregate. The accepted Windows `webbrowser` compile proof and rejected WSL proposal are outside this durability decision and were not reopened.

## Proposed Findings

None.

No reachable task-scoped defect was found in saved-login durability, process concurrency, restart behavior, canonical selection, logout ordering, or cache/single-flight interaction. The candidate retains the conventional file lock plus atomic replacement design and adds no nonstandard recovery complexity.

## Overall Result

**PASS**
