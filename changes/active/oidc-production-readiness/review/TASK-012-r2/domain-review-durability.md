# Credential Durability and Refresh-Concurrency Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `adf349081077b3cfe0d56ab9a665cf01e2d86da4`
- Candidate: `5d9a3ddfad426eb545e866658a74428df72265ef`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-012-client-oauth2.md`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-012-r1/TASK-012-R1-client-oauth2-closure.md`
- Prior ledger: `changes/active/oidc-production-readiness/review/TASK-012-r1/findings-validation.md`
- Domain: local credential durability, refresh-token rotation, cross-process renewal, cancellation and crash recovery, and logout/renewal races

The candidate remained the named commit while this review was performed.

## Authority and Source Coverage

| Boundary | Authority | Source and consumer evidence | Result |
|---|---|---|---|
| One conventional credential store | REQ-012; INV-005; INV-007; task prohibition on a second store; research section 3.6 | `CredentialsFile` owns `credentials.toml`; `SavedLogins` owns its `[[logins]]`; `ClientConfig::resolve_credential` selects it for every first-class SDK through `wyrd-client`. The cumulative diff introduces no additional store, journal, recovery state, or configuration option. | PASS |
| Stable cross-process lock and reread | REQ-012; AC-004; task Scenario 2 | `CredentialsFile::lock` locks the configuration-directory inode, which survives atomic replacement (`credentials_file.rs:214-239`). `SavedLogins::renew` takes that lock, rereads the record, and reuses another process's fresh access token before considering refresh (`saved_login.rs:274-301`). | PASS |
| Refresh rotation remains inside the store owner | REQ-012; RFC 6749 section 6; RFC 9700 section 4.14.2; task and remediation preserved behavior | When stale, `SavedLogins::renew` calls the installed `oauth2`-backed `TokenExchange::refresh` while retaining the same lock (`saved_login.rs:302-319`; `auth.rs:372-393`). No public custom form path accepts refresh after remediation; no sibling SDK refresh writer exists. | PASS |
| Rotated pair is persisted atomically | REQ-012; task requirement to retain atomic replace | Renewal refuses a successful response without a successor refresh token, then updates refresh token, access token, and expiry before one write (`saved_login.rs:320-333`). `SavedLogins::write` preserves unrelated TOML and delegates to the same-directory temporary-file, file-`fsync`, rename, and directory-`fsync` owner (`saved_login.rs:357-383`; `credentials_file.rs:154-175`). | PASS |
| Crash and uncertain response behavior | REQ-012's explicit crash outcome; remediation FIND-TASK-012-3 | A cancelled or failed refresh can leave the server-rotated successor unseen, while the predecessor remains the last durable record; retry therefore presents the predecessor and server reuse detection retires the family, requiring login again. `TokenExchange::refresh` now documents exactly that standard consequence (`auth.rs:372-385`). The candidate adds no nonstandard retry, backup token, or recovery protocol. | PASS |
| Async cancellation and in-process single-flight | REQ-012; AGENTS async/runtime rules | `SavedLoginSource::mint` runs on Tokio's blocking pool through `AuthMiddleware::mint_into`; a dropped waiter leaves the spawned mint in `pending_mint`, and the next caller awaits it instead of spawning another (`auth.rs:926-972`). The blocking mint continues to own the OS lock until refresh and persistence finish. | PASS |
| Concurrent process and logout ordering | REQ-012; AC-004 | Save, remove, API-key cache writes, and saved-login renewal use the same directory lock. `remove` rereads under the lock and returns the record actually removed (`saved_login.rs:250-272`). If renewal wins, logout removes and revokes the rotated record; if logout wins, renewal rereads the missing record and returns `logged_out` rather than recreating it (`saved_login.rs:285-295`). | PASS |
| Local-delete-first, best-effort revocation | REQ-012; locked lead decision for RFC 7009 | CLI logout deletes under the store owner before constructing `TokenExchange` or sending revocation; revocation failure warns and never restores the local record (`wyrd-cli/src/auth/login.rs:158-198`). Revocation remains the approved one form POST through the redirect-free client and is idempotent (`auth.rs:395-427`). | PASS |
| Origin identity and stored-record selection | REQ-012 server identity; task Scenario 3; remediation FIND-TASK-012-2 | `canonical_origin`, `TokenExchange`, and `HttpTransport` now consume the same `HttpConfig::validate` origin. Saved-login keys therefore discard path/query/fragment, normalize case/default ports, and reject userinfo before persistence or selection (`saved_login.rs:108-128`; `auth.rs:220-284`). This changes no file format or concurrency semantic. | PASS |
| Direct CLI refresh | REQ-021; standard OAuth behavior | `wyrd auth refresh` takes one refresh token from the environment, performs the standard `oauth2` refresh, and prints the returned pair (`wyrd-cli/src/auth/refresh.rs:12-60`). It does not claim saved-login durability and adds no second persistent writer. An uncertain response has the same public-client rotation consequence documented by `TokenExchange::refresh`. | PASS |
| Store protection and replacement safety | REQ-012; task Scenario 3 | Directory ownership/write-bit checks precede locking and reading; credential files must be regular, owner-owned, and private (`credentials_file.rs:182-266`). Atomic replacement is the existing standard mechanism retained by the task, not new review-driven complexity. | PASS |

## Failure and Recovery Trace

- **Two stale processes:** the first process takes the directory lock, rereads, refreshes once, and atomically replaces the record. The next process takes the same stable lock, rereads the replacement, sees the access token outside the 60-second margin, and returns it without presenting the predecessor refresh token.
- **Crash or cancellation before the refresh request reaches the server:** no local mutation has occurred; the predecessor remains durable and a later call can retry it.
- **Crash, cancellation, or transport failure after server rotation but before the response is observed:** the predecessor remains durable. A later replay is deliberately contained by the server's refresh-family reuse detection, matching REQ-012; the client does not invent local recovery state.
- **Successful refresh followed by local encoding, write, rename, or `fsync` failure:** the call returns the credential-file error and does not report success. The last durable record remains the predecessor until atomic replacement completes; subsequent replay follows the same approved family-containment outcome.
- **Logout racing renewal:** both operations serialize on the directory lock. Removal cannot be undone by a late renewal because renewal rereads only after acquiring the lock; it either completed before removal or observes no record. The removed record is the current one used for best-effort revocation.
- **Revocation outage or caller cancellation:** the local deletion has already completed. The warning accurately reports that server revocation was not confirmed; RFC 7009 revocation is safe to repeat, but the CLI intentionally keeps no deleted-token recovery state.

## Prior-Finding Closure Relevant to This Domain

- `FIND-TASK-012-1` is closed without changing the durability owner: the public arbitrary `TokenRequest` exchange was deleted, so refresh remains reachable through the `oauth2` method used under `SavedLogins::renew`'s existing lock.
- `FIND-TASK-012-2` is closed by sharing one parsed origin. This preserves the existing `(origin, tenant_key)` saved-login identity and does not add a migration, alias, or second selection mechanism.
- `FIND-TASK-012-3` is closed for refresh, device redemption, and revocation uncertain completion. The documentation now matches the actual rotation, replay, and idempotency behavior; it adds no runtime mechanism.
- `FIND-TASK-012-4` concerns exact-selector evidence and does not alter persistence or concurrency behavior.

## Verification Evidence and Limits

Reviewed the complete cumulative base-to-candidate diff and the current bodies and callers of `CredentialsFile`, `SavedLogins`, `SavedLoginSource`, `TokenExchange::{refresh,revoke_refresh_token}`, `AuthMiddleware::mint_into`, CLI refresh/logout, shared credential resolution, and the relevant Rust/CLI journey tests.

The immutable task evidence records the focused `client/concurrent_saved_renewal` identity journey. That journey starts four separate processes on one stale record, verifies all succeed, proves the refresh token rotates, renews the winner again to demonstrate no predecessor replay retired the family, checks unsafe-store refusal, and races logout against three renewal processes (`pg_auth_e2e_against_fixture.rs:201-306`). The CLI journey proves logout revokes only the selected chain, preserves another login, and still deletes locally with a warning when the server is unavailable (`cli_login_journey.rs:372-436`). Store unit tests cover canonical origin identity, content preservation, private file/directory modes, symlink and corruption refusal, and save/select/remove.

The remediation did not modify the lock, reread, atomic-replace, refresh-persistence, or logout production owners; its relevant runtime change constrains the form grant surface and shares normalized origin construction. The remediation records passing owner tests, the one filtered CLI journey, narrow client library targets, focused Clippy, boundary checks, format, and exact selectors required by the prior ledger. This reviewer did not rerun broad journeys or aggregates, per the review direction; no additional durability-specific command was needed beyond the recorded focused concurrency and CLI evidence.

The accepted Windows proof is `cargo check -p webbrowser --target x86_64-pc-windows-msvc`; the unavailable Windows C cross-compiler and rejected `PLAT-001` are not durability gaps and were not reopened.

## Proposed Findings

None.

No material durability, persistent-state, concurrency, rotation/replay, cancellation, origin-keying, or logout-ordering defect was found. The candidate retains the conventional `credentials.toml` plus standard OS lock and atomic-replace design and adds no mechanism, check, file, setting, or option beyond the approved standard behavior.

## Overall Result

**PASS**
