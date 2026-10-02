# TASK-004 round-2 behavior review

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `06f134dc14164c040c0e5014d21de29c240f4116`
- Candidate: `362878494ed80ca5c5533a4364f744bf92dd1e06`
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-004-laptop-clients.md`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, with
  revisions 8 and 9 superseding conflicting TASK-004 text
- Prior review: `changes/active/oidc-production-readiness/review/TASK-004-r1/`
- Binding human direction:
  `review/TASK-004-r1/human-direction-FIND-TASK-004-4.md`, including all
  addenda

The candidate remained `HEAD` while this report was prepared. `.codegraph/` is
absent, so navigation used the cumulative Git diff, `rg`, and direct
source/caller inspection. The `7996daaab..362878494` range was used only to
locate remediation; the acceptance audit covered the full base-to-candidate
range. Spec revision 10 REQ-021 and its OAuth endpoint wire-format changes are
owned by TASK-008 and were excluded from this review.

## Navigation and behavior paths reviewed

| Path | Owners and consumers inspected | Principal proof |
|---|---|---|
| Device-code login | `wyrd-client::TokenExchange` -> server auth router -> `CliLogins::{authorize,approve,deny,redeem}` -> tenant-scoped device/login rows -> common OIDC callback -> CLI poll/save | CLI real-server journey; `cli_logins` PostgreSQL tests; route/OpenAPI registration |
| Saved credential selection | Rust/Python/TypeScript constructors -> `client_from_options` / `ClientConfig::resolve_credential` -> environment tiers, `SavedLogins::select`, credentials-file floor | Shared-client selector tests and all three language journeys |
| Renewal and concurrency | `AuthMiddleware` -> `SavedLoginSource::mint` -> `SavedLogins::renew` -> refresh grant -> locked atomic credential-file replacement | Separate-process `concurrent_saved_renewal`; three SDK renewal journeys |
| Logout and revocation | CLI local selection/removal -> `TokenExchange::revoke_refresh_token` -> `CliLogins::end` -> refresh-family lock/revoke plus canonical audit | CLI journey and transactional PostgreSQL logout test |
| Credential persistence | `CredentialsFile` plus `SavedLogins` over the single `credentials.toml` | Content-preservation, unsafe-store, API-key cache, and saved-login tests |
| First-class SDK projection | Rust SDK public client; Python PyO3, wrappers, package declarations; TypeScript N-API and public declarations | Rust, Python, and TypeScript identity journeys; public constructor/type checks |

Sibling callers and consumers inspected included ordinary API-key/workload
exchange, browser login state and callback completion, browser refresh,
transport reactive refresh, Bifrost/Cards/Gateway/State/Verification client
construction, and the identity-lane target wiring. No second identity,
credential-store, token-renewal, or tenant-selection owner was introduced.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-011: human CLI login uses the RFC 8628 device grant, opens the system browser, prints no credential, and needs no second IdP registration | `crates/wyrd/wyrd-cli/src/auth/login.rs:58-178`; `crates/wyrd/wyrd-auth/src/cli_logins.rs:100-398`; `crates/wyrd/wyrd-server/src/auth/cli_login.rs:45-260` | `cli_device_login_journey`; `cli_logins::pg_tests::device_codes_poll_approve_deny_and_expire`; recorded identity target `cli` | PASS |
| REQ-011 / AC-007: wrong, pending, denied, expired, cross-origin, and redeemed device codes return no credential | Tenant-routed hash lookup, one-use completion redemption, denial/expiry deletion, poll cadence, and same-origin approval in the sources above | CLI journey covers pending, wrong code, cross-origin/unknown approval, replay, denial, and expiry; owner test covers `authorization_pending`, `slow_down`, wrong tenant/code, denial, expiry, and one-use completion | PASS |
| Browser/provider material never becomes SDK authority or appears in output, redirect URLs, or page data | Device code is stored only as SHA-256; callback stores a sealed Wyrd completion; CLI saves only the redeemed Wyrd token response; completion page is static | CLI journey checks access/refresh token absence from transcript, verification URL, provider URL, and browser page | PASS |
| REQ-005 / INV-005: provider secrets remain server-side and all SDKs project server-owned Wyrd identity | Client/device contracts contain no provider credential; OIDC exchange and role issuance remain in server-owned auth paths | Device login plus three SDK real-server journeys; existing OIDC identity coverage recorded in the task | PASS |
| REQ-012: one user-protected credential file, no local encryption or second token/login store, preserving other content | `crates/shared/wyrd-client/src/credentials_file.rs`; `saved_login.rs:1-17,143-378`; saved records are `[[logins]]` in `credentials.toml` | `logins_live_in_credentials_toml_beside_user_content`; unsafe/corrupt/symlink tests; recorded shared-client lane | PASS |
| REQ-012: explicit credential wins; environment tier order remains; saved login sits above the API-key file floor | `ClientConfig::resolve_credential` and `CredentialChain` | Shared config precedence tests; explicit override phase in Rust/Python/TypeScript journeys | PASS |
| Binding selector direction: `tenant` is a route key; it selects saved/workload authority and is refused beside a self-naming bearer or API key | `config.rs:148-238`; `credential.rs:227-350`; `SavedLogins::select` | `tenant_selector_is_refused_beside_a_self_naming_credential`; unmatched/selected tenant cases in all three SDK journeys | PASS |
| REQ-012 / AC-004: no selector uses the most recently saved login for that canonical server; an unmatched selector does not fall through | `SavedLogins::{save,select}` removes/re-appends replacements and selects the last same-origin record | `selection_picks_the_named_or_newest_login`; newest and `tenant_mismatch` phases in all three SDK journeys | PASS |
| REQ-012 / AC-004: short-lived user access renews through Wyrd without revisiting the IdP | `AuthMiddleware` renewable path -> `SavedLoginSource::mint` -> `SavedLogins::renew` -> refresh grant | Rust/Python/TypeScript stale-login journeys verify the refreshed record; recorded identity targets | PASS |
| REQ-012 / AC-004: concurrent local processes lock, reread, reuse a winner's token, and do not replay the predecessor | `CredentialsFile::lock`; `SavedLogins::renew` holds it across reread, exchange, and atomic write | `concurrent_saved_renewal` starts four processes and proves one rotation followed by another usable rotation | PASS |
| Revision-8 accepted crash behavior: a lost response may leave the old token; the next replay is handled by server reuse detection | Client leaves the prior record unchanged on transport/write failure; server refresh-family serialization and reuse containment remain authoritative | Server refresh replay/containment tests and recorded identity/shared lanes; no superseded `RefreshPending` machinery remains | PASS |
| REQ-012: refused refresh asks for login again without falling through | `SavedLogins::renew` maps a server refusal to stable `refresh_refused`; selection errors are terminal | Revoked-login phase in each language journey | PASS |
| REQ-012: logout deletes locally first, revokes best-effort, and warns on failure | `wyrd-cli/src/auth/login.rs:180-220`; removal and revocation are deliberately ordered | CLI journey proves per-chain revocation, preservation of another login, offline local deletion, and warning | PASS |
| FIND-7 / audit: successful CLI logout revocation and its canonical audit commit transactionally | `CliLogins::end` takes the family lock, revokes the selected chain, appends `auth.cli_login.logout`, then commits one `TenantConn` | `cli_logins::pg_tests::logout_revokes_only_its_own_chain` covers one event and audit-failure rollback | PASS |
| REQ-015 / INV-001 / INV-003: same-server tenants remain separate and route/path data never becomes effective authority | Saved login selects by canonical origin plus route key; device prefix only routes to a tenant RLS transaction; stored hash and server-bound login state authorize redemption | Two-tenant cases in all language journeys; wrong-tenant/code and RLS device tests | PASS |
| REQ-016: inactive/replaced human connection cannot renew the saved login | Refresh rows retain exact connection ID/revision; `RefreshTokens::execute` reissues through `issue_human_session`, which requires the bound revision still active | `refresh::pg_tests` inactive-connection coverage plus recorded identity lanes | PASS |
| First-class Rust, Python, and TypeScript surfaces delegate to the shared Rust owner and expose `tenant` consistently | Shared `client_from_options`; Python PyO3/wrappers/stubs; TypeScript N-API/public options/declarations | Language identity journeys; Python public-constructor forwarding test; `py:typecheck`, `ts:typecheck`, `ts:napi:check`, `codegen:check` recorded passing | PASS |
| AC-009: CLI help, docs, contracts, and generated declarations describe device login, newest-login selection, and failure behavior consistently | CLI args/docs; auth/device/token contracts; authentication and client-configuration docs; generated schemas/declarations | Recorded `codegen:check`, `docs:check`, CLI tests, Python/TS type checks | PASS |
| TLS and secret handling: remote cleartext is refused before any device, refresh, or revoke request; loopback HTTP remains usable | `TokenExchange::new` reuses `HttpConfig::validate`; secrets remain redacted wrapper types | `token_exchange_refuses_remote_cleartext` and CLI remote-cleartext checks | PASS |
| Non-goals: no provider-token authority, human/workload conflation, language-specific store, second IdP app, custom handoff, speculative renewal state, local encryption, or provider-specific login branch | Custom handoff contracts/table/query were deleted; revision-8 renewal state was deleted; device flow reuses the common provider-agnostic OIDC owner | Full cumulative diff and symbol search; Keycloak-backed automation exercises standard behavior without a provider-specific production branch | PASS |
| Revision 9: TASK-004 remains provider agnostic and adds no live-provider qualification mechanism or record | Device flow depends only on `HumanConnections` and the standard OIDC callback; no provider name is branched on in production | Generic path plus repository Keycloak/Dex identity evidence owned by the broader change; no TASK-004-only qualification artifact | PASS |
| Revision 10 REQ-021 | Explicitly owned by TASK-008 and excluded by the human instruction for this review | N/A | PASS (out of scope) |

## Prior finding closure under revised authority

| Prior finding | Round-2 disposition | Evidence |
|---|---|---|
| `FIND-TASK-004-1` | Closed by binding human direction | Tenant is route-key-only; it routes saved/workload selection and is refused beside self-naming credentials in `ClientConfig::refuse_selector`; shared and three-language proofs cover it. |
| `FIND-TASK-004-2` | Superseded by spec revision 8 | Per-request generation revalidation and durable generation were explicitly removed. The conventional path caches a fresh access token and rereads the locked file when renewal is due. |
| `FIND-TASK-004-3` | Superseded by spec revision 8 | `RefreshPending`, custom lock timeout, and their bespoke proof were explicitly removed. The current separate-process journey proves the approved blocking-lock/reread behavior. |
| `FIND-TASK-004-4` | Closed by human direction, then simplified by revision 8 | The sole store is user-owned `0600` `credentials.toml`; there is no local encryption. Revision 8 removed pending state entirely. |
| `FIND-TASK-004-5` | Closed | `TokenExchange::new` applies the existing remote-cleartext transport rule once for every direct caller; unit and CLI journey coverage exercise it. |
| `FIND-TASK-004-6` | Closed | Python public Bifrost wrappers, PyO3 owners, package declarations, and the remaining public constructors accept and forward the shared tenant option; generated parity checks passed. |
| `FIND-TASK-004-7` | Closed | `CliLogins::end` appends exactly one canonical logout event in the revocation transaction; audit failure rolls the revocation back. |

## Proposed findings

None. I found no reachable MISSING, INCORRECT, DRIFT, VIOLATION, or REGRESSION
after applying the revised human/spec authority. In particular, I did not
retain round-1 requirements for the deleted handoff, generation,
`RefreshPending`, tombstone, or lock-deadline mechanisms; requiring any of
them would contradict revision 8 and the standing conventional-practice
direction.

## Verification assessment

The implementation record reports final successful runs of the five focused
identity targets and the unfiltered identity journey, shared/client/CLI/server
lanes, all three SDK lanes, Python and TypeScript integration/type lanes,
codegen, docs, boundary checks, formats, and lints. This review independently
ran:

```text
git diff --check 06f134dc14164c040c0e5014d21de29c240f4116..362878494ed80ca5c5533a4364f744bf92dd1e06
mise exec -- cargo nextest run --locked -p wyrd-client --lib \
  -E 'test(=auth::tests::token_exchange_refuses_remote_cleartext) | test(=config::tests::tenant_selector_is_refused_beside_a_self_naming_credential) | test(=saved_login::tests::selection_picks_the_named_or_newest_login)'
```

The diff check was clean and all three focused tests passed. I did not rerun
the Docker/Keycloak-backed identity matrix; its candidate-recorded results and
source assertions are the available heavy-lane evidence. That is a review
verification limit, not a missing task proof.

## Overall result

**PASS**

The cumulative candidate satisfies TASK-004 under the binding human direction
and approved spec revisions 8 and 9. The prior findings are either closed in
the shared owners or explicitly superseded by the approved conventional model,
and no new behavior finding remains.
