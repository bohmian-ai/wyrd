# TASK-004 round-2 invariant review

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `06f134dc14164c040c0e5014d21de29c240f4116`
- Candidate and review-time `HEAD`: `362878494ed80ca5c5533a4364f744bf92dd1e06`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revisions 8 and 9 for this task; revision 10's REQ-021 is assigned to TASK-008 and was excluded.
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-004-laptop-clients.md`
- Prior review and binding direction: `review/TASK-004-r1/verdict.md` and all decisions/addenda in `review/TASK-004-r1/human-direction-FIND-TASK-004-4.md`
- Reviewed range: the complete `base..candidate` range, using `7996daaab9788f5d5c8fd2a40fb3bbf126aa0d84..candidate` only to locate round-2 remediation.

The candidate remained unchanged while this report was prepared. `.codegraph/` is absent, so the review used the cumulative Git diff, `rg`, and direct source/caller inspection. The review applied the human owner's standing decision to follow standards and comparable widely used projects and did not retain or recommend any nonstandard extra mechanism.

## Producer-to-sink navigation and invariant trace

| Authority or value | Producer and durable owner | Consumers and sibling paths checked | Result |
|---|---|---|---|
| Device code and user code | `CliLogins::authorize`; `wyrd.auth_device_authorizations` | verification page, approve/deny, login-state device binding, callback completion, token polling, one-use deletion | Tenant routing is only a hint; the code hash under forced RLS is authoritative. Expiry, denial, cadence, completion, and deletion are transactionally ordered. |
| Device login completion | ordinary `HumanConnections` callback and sealed `auth_login_state.completion_sealed` | `CliLogins::redeem`, access-token verification, allowed audit, CLI save | Bound to one `device_id`, sealed at rest, deleted once, and returned only to the device-code holder. |
| Saved-login identity | `SavedLogin::from_token`; `SavedLogins::save` in `credentials.toml` | shared Rust selection/renewal, CLI status/logout, Rust/Python/TypeScript constructors and journeys | Tenant-key selection and newest-login default hold. The supposed canonical origin retains URL userinfo and path, so it is not an origin and can print embedded URL secrets. `INV-R2-001`. |
| Credential file and lock | `CredentialsFile`; atomic replacement under a directory lock | API-key disk cache, saved-login save/select/renew/remove, CLI status/logout, all SDKs | One file, blocking lock, reread-before-refresh, 0600 file, symlink/owner checks, and content preservation hold. A group-writable directory is expressly accepted, leaving a replacement race between path metadata validation and path-based read. `INV-R2-002`. |
| Refresh token rotation | `SavedLogins::renew` while holding the credential-directory lock | competing processes, `AuthMiddleware`, refused refresh, crash between server rotation and local save | One process refreshes and saves; peers reread and reuse it. A crash leaves the predecessor, whose replay reaches the server's standard reuse containment as revision 8 accepts. |
| Logout and revocation | `SavedLogins::remove`, then `CliLogins::end` | concurrent renewal, per-chain revoke, canonical audit, offline warning | Local deletion wins under the same lock; remote revocation is best-effort and limited to the selected login chain. Audit commits with a known revocation. |
| Credential precedence | `ClientConfig::resolve_credential` | explicit, environment, workload, saved login, credentials-file floor; all SDK projections | Human-directed key-only selection and refusal beside self-naming bearer/API-key credentials are consistently owned in shared Rust. |

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-011: RFC 8628 device authorization, browser approval, polling, and no pasted callback or printed token | `wyrd-auth/src/cli_logins.rs:100-398`; `wyrd-server/src/auth/cli_login.rs:45-240`; `wyrd-cli/src/auth/login.rs:58-168` | `cli_logins::pg_tests::device_codes_poll_approve_deny_and_expire`; `cli_device_login_journey` | PASS |
| Wrong, expired, denied, and redeemed device codes issue no credential | hash lookup and row lock in `poll_device_authorization`; refusal-before-completion and delete-on-terminal-state in `CliLogins::redeem_in` | CLI journey covers pending, wrong code, cross-origin/unknown approval, denial, expiry, and replay | PASS |
| Device login tenant and connection remain server-bound | device code prefix only routes to `TenantConn`; forced RLS and stored login-state connection/device binding own authority | owner PG tests plus provider-backed CLI journey | PASS |
| Poll cadence follows RFC 8628 without a custom second mechanism | persisted `last_polled_at`; `slow_down`; CLI increases subsequent interval by five seconds; shared route governor supplies admission control | owner PG test and CLI journey | PASS |
| Successful credential handoff and logout produce canonical audit evidence | allowed device grant uses `append_auth_audit` before commit; logout revocation uses the same transactional append | `logout_revokes_only_its_own_chain`; device owner tests | PASS |
| REQ-012: one local credential file, preserved user content, atomic replacement, blocking exclusive lock | `credentials_file.rs`; `SavedLogins::{save,renew,remove}` | saved-login unit tests and `concurrent_saved_renewal` | FAIL — directory protection is not fail-closed (`INV-R2-002`) |
| REQ-012: record is keyed by canonical server origin and tenant, and CLI output contains no secret | `canonical_origin`; `SavedLogins::select`; CLI summary/status | normalization and selection unit tests omit userinfo and non-root paths | FAIL — the key is a canonical base URL, not an origin, and can expose URL credentials (`INV-R2-001`) |
| REQ-012: newest login default; tenant key selects an exact login; explicit credential wins | `SavedLogins::select`; `ClientConfig::resolve_credential`; `refuse_selector` | shared-client tests and three language journeys | PASS |
| REQ-012: concurrent clients reread under lock and do not replay a rotated predecessor | `SavedLogins::renew` holds the directory lock across reread, refresh, and atomic save | four-process `concurrent_saved_renewal` | PASS |
| Revision 8 crash behavior: an uncertain post-rotation crash may replay the predecessor and server reuse containment handles it | no pending marker or retry-specific local state; failed transport leaves the stored predecessor unchanged | accepted by approved revision 8; ordinary refusal/reuse server coverage exists | PASS |
| Revision 8 logout: delete locally, revoke best-effort, warn offline | CLI `logout`; `SavedLogins::remove`; `CliLogins::end` | CLI journey covers online per-chain revoke and offline deletion/warning | PASS |
| AC-004: Rust, Python, and TypeScript use the same CLI-established authority for newest/exact selection, allowed/denied calls, renewal, override, and revocation refusal | shared `wyrd-client` owner with thin PyO3/N-API projections | Rust, Python, and TypeScript saved-user journeys | PASS, subject to the two shared-store failures above |
| REQ-016: old connections cannot mint renewable authority after deactivation/replacement | device approval begins an ordinary connection-bound login; existing refresh authority retains its server-side connection binding | inherited TASK-002/003 connection and refresh coverage; device journey uses the same owner | PASS |
| Non-goals: no custom handoff, second IdP app, provider-token authority, language-specific store, or workload/human conflation | handoff code/table removed; RFC 8628 and shared client paths only | cumulative diff inspection | PASS |
| REQ-021 OAuth form/error wire format | Assigned to TASK-008 by the human owner | Not reviewed | OUT OF SCOPE |

## Prior-finding closure

| Prior finding | Round-2 evidence | Result |
|---|---|---|
| `FIND-TASK-004-1` tenant binding | Human addendum defines a tenant-key-only selector; `ClientConfig::refuse_selector` rejects it beside bearer/API-key authority, while workload exchange uses it as routing authority. | CLOSED |
| `FIND-TASK-004-2` per-request generation revalidation | Approved revision 8 removes durable generation and per-request revalidation in favor of conventional refresh-under-lock and reread-before-refresh. Candidate removes both mechanisms. | CLOSED BY SUPERSEDING AUTHORITY |
| `FIND-TASK-004-3` pending and lock deadlines | Approved revision 8 removes `RefreshPending` and the custom lock deadline. Candidate uses a plain blocking OS lock and conventional uncertain-crash behavior. | CLOSED BY SUPERSEDING AUTHORITY |
| `FIND-TASK-004-4` local sealing decision | Binding human direction selects the single 0600 `credentials.toml` file with no local encryption. The separate login store is gone and other content is preserved. | CLOSED, but file protection still fails under `INV-R2-002` for a different cause |
| `FIND-TASK-004-5` remote cleartext | `TokenExchange::new` reuses `HttpConfig::validate`, refusing remote cleartext before building any secret-bearing request. | CLOSED |
| `FIND-TASK-004-6` Python projection | Public Python constructors and Bifrost wrappers accept/forward `tenant`; declarations and journeys cover the public path. | CLOSED |
| `FIND-TASK-004-7` logout audit | `CliLogins::end` appends `auth.cli_login.logout` in the revocation transaction; the PG test proves append failure rolls back. | CLOSED |

## Proposed findings

### INV-R2-001 — INCORRECT: `canonical_origin` retains URL userinfo and path

- **Violated obligation:** TASK-004 and REQ-012 key a saved login by canonical server **origin**; TASK-004 also forbids printing secrets and limits status output to safe identity metadata.
- **Location:** `crates/shared/wyrd-client/src/saved_login.rs:106-121`; consumers at `crates/shared/wyrd-client/src/config.rs:193-200` and `crates/wyrd/wyrd-cli/src/auth/login.rs:76-83,156-167,189-220,267-271`.
- **Producer-to-consumer evidence:** `canonical_origin` parses the URL, removes only query and fragment, and serializes the rest. Standard URL origin is scheme, host, and effective port; username, password, and path are not part of it. The retained string becomes the persistent selection key and is printed by login, logout, and status. Every Rust/Python/TypeScript saved-login consumer resolves through that same key. The existing normalization test covers case/default port/query/fragment only.
- **Observable consequence:** a valid server URL such as `https://user:password@wyrd.example.com/` persists and prints `user:password`; path spelling also splits what the approved contract defines as one server origin into separate login sets, so a saved login may not resolve or log out under another URL for the same origin.
- **Required testable correction:** use the URL library's native origin serialization as the one saved-login key (or reject URL userinfo before any credential operation if the shared transport contract intentionally retains a deployment base path). Do not add another identifier or compatibility store. Add one shared-client test proving userinfo is absent from the key/summary and equivalent URLs for one origin select the same record; exercise CLI status to prove the secret is absent from output.

### INV-R2-002 — INCORRECT: a group-writable credential directory defeats the file checks

- **Violated obligation:** the binding human direction requires user-only ownership/mode and fail-closed unsafe storage; TASK-004 requires symlink and unsafe-permission refusal. `credentials_file.rs` itself states that the credential file is inside a directory only the user can change.
- **Location:** `crates/shared/wyrd-client/src/credentials_file.rs:182-200,242-265`; the acceptance is pinned by `crates/shared/wyrd-client/src/saved_login.rs:562-610`.
- **Producer-to-consumer evidence:** `check_dir` passes only `0o002` to `check_owned`, so mode `0775` is expressly accepted. A different member of that group can rename or replace `credentials.toml`. `read_text` checks `symlink_metadata(path)` and then separately calls `read_to_string(path)`, so a replacement between those operations bypasses the ownership, regular-file, mode, and symlink result that was just checked. `save`, renewal, logout, API-key cache reads/writes, CLI status, and all SDK credential resolution share this path. The directory lock does not exclude a process that does not cooperate with that lock.
- **Observable consequence:** on a shared-group directory, another local account can race in an attacker-controlled credential file after validation, causing Wyrd to consume or overwrite authority that was not protected by the promised user-only boundary. The existing test treats `0775` as safe and therefore cannot catch the defect.
- **Required testable correction:** use the conventional permission boundary already present: refuse group- as well as world-writable configuration directories (forbidden mode `0o022`) before reading or writing. This is smaller than inventing a new store or lock and preserves the single-file design. Update the focused Unix test so `0775` fails closed while an owner-only directory and 0600 regular file still support save, renewal, and logout.

## Verification assessment and limits

- The implementation packet records all identity targets, the unfiltered identity journey, shared/client/SDK/Python/TypeScript lanes, codegen, boundary, docs, format, and lint lanes as passing after Docker was restored. Source inspection confirms the named device, concurrency, and three-language journeys exist and exercise the stated cases.
- This review ran `git diff --check` successfully and inspected the complete cumulative source/diff. It did not rerun the long provider-backed lanes.
- Existing green tests do not cover URL userinfo/non-root origin equivalence or a group-writable credential directory replacement boundary; the current permission test affirmatively accepts `0775`.
- No REQ-021 request/response-format issue is included because that approved revision-10 work belongs to TASK-008.

## Overall result

**FAIL**

The candidate correctly replaces the custom handoff and renewal state with the approved conventional RFC 8628 and refresh-under-lock flows, and all seven round-1 findings are closed under binding revisions 8 and 9. Two shared local-authority invariants remain incorrect: the persisted/printed key is not a canonical origin and can contain URL credentials, and the credential file is trusted inside a group-writable directory whose path-based validation can be raced.
