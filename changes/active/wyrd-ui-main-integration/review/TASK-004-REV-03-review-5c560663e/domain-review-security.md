# Security domain review

Result: **FAIL**. Two source-backed proposed findings remain for independent validation: SEC-1 and SEC-2.

Subject: `/home/thorrester/Documents/GitHub/wyrd`, base `e0354f9bdf8608046833029252d8a526e0bd999d`, candidate `5c560663e77ae07c33d198862ba6d795b656097a`. HEAD was checked and still matches the candidate. First full review; no carried-forward acceptance. Read-only implementation review; only this report was written.

## Boundary and authority

Reviewed the complete local-human/password/session and personal-key authority chain, with surrounding unchanged owners and the actual base-to-candidate diff. Governing authority: root AGENTS.md; SQL AGENTS.md; approved spec revision 19 (revision-19 decisions, REQ-041/042/043 and INV-002); original task `tasks/TASK-004-revision/TASK-03-local-users-personal-keys-and-observe.md`, especially local signup, sessions, personal keys and public contracts; `architecture/wyrd-security-posture.md` API keys, access tokens, browser sessions and Postgres binding; task-review skill and spec-driven development rules. Applied Ponytail and Rust/Python review guidance subject to the narrower repository authority. CodeGraph was consulted before locating source.

Coverage includes:

- Pure auth request types, SecretBearer handling, canonical usernames, raw-input and NFC/password policy, common-password lookup, Argon2 parameters, dummy verification and rehash.
- BrowserSessions signup/login/change/reset, invitation/reset SQL, enrollment role issuer revalidation, transaction boundaries, password revisions and live-session SQL.
- Browser HTTP origin/cookie/provenance flow, selected tenant and explicit bearer precedence, recent-human marker producer and credential-create consumer.
- Real User self capabilities before ceiling intersection in TenantTokenIssuer, SQL exactly-one key ownership and lookup, current-grant/ceiling resolution, exchange and recovery sibling consumers.
- Principal HTTP/me/list/create/revoke, administrative issuance preservation, tenant-bound cursors; MCP discovery/dispatch and shared metadata/revoke authorization, with no secret-creation MCP tool.
- UI BFF hooks, server form and browser-session forwarder, login/signup/reset/account actions and pages, one-time issued result, safe load metadata, and no browser storage calls in the changed flow.
- Controlled setup/dev password producer and protected-file/console consumer; modified dependency declarations reuse already-resolved workspace Argon2 and Unicode normalization.

No external standard comparison was needed: the approved admission, password and session profile is explicit, and this slice does not add an OIDC/JWT validation profile.

## Security Audit

### Critical

None established.

### High

- **SEC-1 — INCORRECT: concurrent requests bypass the approved password guessing limits.** `crates/wyrd/wyrd-auth/src/passwords.rs:535-551`, sibling password-change path `:613-630`, and `crates/wyrd/wyrd-sql/src/queries/auth/local_passwords.rs:228-249`.

  **Obligation:** spec revision-19 Abuse and recovery and task Local signup require shared atomic admission, at most ten failed attempts per tenant/name and one hundred per trusted source per fixed fifteen-minute window, checked before KDF work. The same limits apply to password change and repeat local login for key creation.

  **Producer-to-consumer evidence:** `Windows::admit` only reads current failure rows via `attempt_refusal`; login commits that transaction before calling `check_password`. The four-permit KDF semaphore is acquired inside that later call. There is no admission reservation and no recheck after waiting for a permit. Only after verification fails does `Windows::refuse` atomically increment the counters. The SQL upsert correctly counts completed failures, but does not serialize or bound admissions. Password change uses the same read/commit/verify/count sequence. The normal reauthentication path is another password login and shares the defect.

  **Plausible exploit:** an unauthenticated remote caller submits a concurrent batch of guesses for a known tenant and username while its counter is empty or below ten. All requests that complete the admission SELECT before the threshold is recorded are accepted and queue at the semaphore. They continue verifying even after ten failures are recorded; extra replicas multiply available admission/KDF paths. A targeted dictionary of organization-specific passphrases that are absent from the common-password list therefore receives far more trials than the approved online limit. The four-operation cap limits instantaneous memory use, but does not limit these guesses. This undermines the primary brute-force control and exposes User access and recent-login personal-key creation after a successful guess.

  **Smallest correction boundary:** fix admission in the existing shared auth/SQL window owner so a request cannot obtain a KDF check from an unreserved stale window read. Bound in-flight attempts together with completed failures atomically across replicas, settle successful attempts without charging them as failures, and use database time/fixed expiry so abandonment cannot create permanent lockout. Reuse the same owner for login/change; preserve KDF offloading and avoid holding pooled connections open throughout password derivation. A second route-level check or an atomic failure increment alone cannot close this producer defect. No new service, general rate-limit framework, deployment setting or auth profile is needed.

  **Focused closure proof:** deterministic concurrent admission at the existing owner with a nearly exhausted account window and a nearly exhausted source window, including two database clients/replicas. Verify excess work is refused with the stable 429/retry metadata before KDF verification, accepted work cannot overrun the window, successful work does not consume failure allowance, and the fixed expiry releases abandoned work. Keep existing sequential V2 and password-change negative checks. Current V2 only sends failures sequentially (`pg_browser_sessions.rs:1377-1406`) and cannot detect this race.

### Medium

- **SEC-2 — VIOLATION: the personal credential HTTP response omits required no-store protection.** `crates/wyrd/wyrd-server/src/components/principals/routes.rs:469`; metadata sibling `:504-520`.

  **Obligation:** spec revision-19 One-time key secret and task Personal Settings require a no-store authenticated creation response. Security authority First-party browser sessions also requires credential-bearing responses to be no-store.

  **Producer-to-consumer evidence:** the new interactive User issuance branch creates an `IssuedCredential` with the plaintext secret, commits, and returns bare `Json(issued)`. `principals_router` does not attach a no-store layer, and the HTTP middleware does not add one. The listing similarly returns bare JSON. The UI hook adds `private, no-store` to the BFF action response, but that does not protect the underlying approved `/v1/principals/{id}/credentials` browser-auth HTTP protocol when used directly. The nearby invitation/reset response owners explicitly reuse `auth::oauth::no_store`; the credential owner does not.

  **Consequence and materiality:** the one-time secret response lacks the required instruction forbidding clients/intermediaries from storing its response; the safety contract is only enforced for the shipped BFF consumer. This is a material explicit secret-handling requirement, not a claim that every ordinary POST is automatically cached or that an exposure was observed. Credential metadata responses have the same owner-level privacy gap.

  **Smallest testable correction:** reuse the existing `auth::oauth::no_store` response mechanism at the credential HTTP response owner for secret creation and credential metadata; retain current JSON shape, status, admin-target behavior and shared domain return types used by MCP. Assert the headers directly in the existing User-key HTTP journey and existing admin credential response checks. No proxy-only fix, new cache layer or new credential endpoint is needed.

### Low / Defense In Depth

No optional hardening proposed.

### Positive Controls

- Invitation consumption, User creation, grants and session creation commit together; single conditional consume plus relational ownership prevents concurrent reuse and cross-tenant partial enrollment.
- New passwords use NFC consistently, raw-byte and character bounds, a local full-password blocklist and Argon2id with random salts; local and SSO identity cannot auto-link by matching username/email.
- Password revision changes invalidate local sessions/reset authority while leaving independent API keys intact. Recovery does not create a human-fresh session.
- The recent-human marker is derived from validated Rust password/SSO session creation and database time; explicit bearer and recovery sessions cannot supply it.
- User keys retain true relational User ownership; current User grants are intersected with stored ceilings after adding self capabilities. Service/Admin key ownership and issuance remain on their original path.
- Self list/revoke checks both verified real-User ownership and the corresponding permission. Other targets use principal administration; other-User issuance additionally requires tenant administration. Revoke also constrains the credential to the target principal.
- Forced RLS and tenant-scoped connections govern new records; foreign invitation/role/cursor identities do not grant authority.
- MCP exposes safe profile/metadata and permission-gated revoke, without creation secrets or invitation/reset codes in model results.
- BFF page loads contain safe metadata; supplied passwords/codes remain POST inputs; no changed storage/history/log/URL producer was found that deliberately persists those supplied secrets. Deliberate ephemeral dev-password console/file delivery is specifically approved and distinct from production setup.

## Acceptance matrix

| Requirement / constraint | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Protected tenant enrollment; no anonymous tenant/admin creation; one-use tenant/issuer/Role binding; no partial User | passwords.rs signup/issue_invitation; local_passwords conditional consume; accounts authorize_user_admin; SQL RLS | V1/V2 rerun PASS reported by orchestrator; source traces | PASS |
| Canonical tenant-local username; no SSO linking; verifier-only NFC/length/blocklist/Argon2 policy | passwords.rs canonical_username/new_password/check_password; users.rs; forward migration | Inline policy/verifier tests inspected; V1/V2 rerun PASS | PASS |
| Shared atomic 10/name and 100/source windows before verification on every password comparison | passwords.rs Windows/login/change; local_passwords attempt_refusal/record_attempt_failure | V2 sequential flow passes; no concurrent admission proof, SEC-1 | FAIL |
| Same BrowserSessions owner, local revision, fixed expiry, status revalidation, origin/cookie/bearer boundary | browser_sessions.rs; SQL live_browser_session; HTTP middleware and browser handler | V1/V2/V3 rerun PASS; existing negative session tests inspected; V12 real provider/replica journey PASS | PASS for inspected implementation |
| Reset/code purpose and expiry; no automatic login; API keys independent | password_reset/change; reset SQL; revision update | V1 rerun PASS | PASS |
| Real User self-only capability through configured permission owner; Service gains no self ownership | issuance resolve before ceiling; accounts is_self; shared HTTP/MCP authorize paths | V3/V6/V9 rerun PASS | PASS |
| Required name/ceiling/expiry, ceiling within both parties; exchange uses current grants; fresh human creation, recovery/bearer refusal | accounts mint_user_credential; routes issue_credential; API-key owner/lookup; issuance; recent marker | V3 rerun PASS including role growth, fresh/stale/recovery, revoke and suspension; V6/V9 PASS | PASS |
| Preserve administrator-targeted Service/Admin creation and metadata; no MCP secret issuance | routes own/admin dispatch, no-body branch; MCP descriptors and shared dispatch | V3/V6/V9 rerun PASS | PASS |
| Metadata bounded for personal consumer, opaque tenant/principal-bound cursor and owner-constrained revoke | credential_page; routes list/revoke; SQL page/revoke; SDK page helper | V3/V6/V9 rerun PASS; V7/V8 Python/TS journeys PASS | PASS for inspected implementation |
| Personal Settings without SSO admin permission; local/SSO normal login and safe UI POST/load flow | account load/actions; browser-sessions forwarder; hook; local entry routes | V4 PASS and 16 affected library/page tests PASS reported; V12 SSO account run PASS | PASS for inspected implementation |
| One-time key response no-store across approved interactive HTTP protocol | routes issue/list bare JSON; BFF hook only protects BFF | SEC-2; existing tests do not assert direct credential headers | FAIL |
| Controlled initial local bootstrap, ephemeral dev password delivery and private record | main setup/create_local_admin; scripts/postgres/dev-server.sh | Source reviewed; V5 lifecycle journey PASS | PASS for inspected security boundary |

## Verification and proof limits

The orchestrator alone runs Cargo, build and generator commands. It reported individual managed-Postgres V1, V2, V3, V6 and V9 passing, UI check/build passing, 16 affected library/page tests passing, and sequential V10/V4/V11(3) passing. It also reported Python formatting/lint/type checks and tenant-isolation checks passing. The final orchestrator update reports V1–V12 passing, including V5 and real Keycloak/Dex/replica V12; Rust format/lints/dependency/codegen checks also pass. `verification.md` records the exact selectors/results, including an existing exchange regression that fails on the new owner constraint during fixture insertion; the separately selected invariant reviewer owns that gap. The new V3 administrative scenarios passing does not establish that every old SQL writer migrated. These results corroborate their actual exercised branches, not the missing concurrent admission/no-store properties.

No implementation or tests were modified. No password, code, key or cookie value was printed or recorded. No synthetic host load, attack batch, live credential capture, external dependency vulnerability lookup, or broad unrelated security audit was performed. Browser back/forward cache behavior and complete secret removal from framework action state were not dynamically measured; no finding is asserted on those speculative paths. The auth attempt state is currently purged on subsequent writes rather than through an independently located maintenance sweeper; no separate material exploit is asserted from that alone. KDF cancellation/permit lifetime was raised with the orchestrator for the separately selected system reviewer, avoiding duplicate findings without a fully established HTTP cancellation path here.

No new security/product decision is required for SEC-1/SEC-2: both correct explicitly approved controls in their current owners. The report does not require speculative adjacent hardening.
