# TASK-003-carryover mid-task behavioral discovery

## Subject and boundary

Repository `/home/thorrester/Documents/GitHub/wyrd`; base `4355587f128a0ac1d05071be0a8a84ba53a52290`; candidate `b7206eafefbf7e981898eca3fc22a5d5ae76b815`. Approved contract: `changes/active/authorization-model/spec.md`, revision 7. Original task: `changes/active/authorization-model/tasks/TASK-003-carryover.md`. Reviewed cumulative `git diff 4355587f12...b7206eafef` and current source. HEAD remained the candidate during discovery; production source was not modified.

This is the requested mid-task audit of current work. Cycles 1–10 claim implementation; cycle 11 contains an intentional RED and handoff; cycles 12–20 remain assigned unfinished work. A finding below concerns current completed work, rather than relabelling every unfinished cycle a newly discovered defect. No task-completion PASS is possible at this checkpoint.

Primary boundary: methods advertisement/enforcement, trusted source, password policy/admission, email start and proof/completion consumers, observable assertions, TDD evidence and overall coverage mapping. Separate specialists own proof/provider security, SQL claim/cap/provisioning invariants, and cross-tier repository coverage. Those reports must supply independent acceptance evidence for their assigned boundaries.

Authority consulted: root AGENTS.md; changes/spec-driven-development.md; agent_docs/index.md, rust.md, errors.md, maintainer-style.md; TESTING.md; owning human-sign-in design and security-posture social-admission/browser-session sections; approved spec REQ-002, REQ-011–018, persistence and acceptance; original task cycles 1–20. CodeGraph was used before source navigation. No Cargo execution was started by this reviewer.

## Navigation and assertion coverage

| Owner and producer | Consumer and observable assertion |
|---|---|
| `auth/source.rs::TrustedIngress::source`, `RequestSource`; config `HttpConfig::trusted_ingress`; shared `admission::source_key` | Server password and signup handlers receive actual ConnectInfo + headers; `signup::admission_keys_the_trusted_source` checks trusted/untrusted peers, IPv4 mapping, IPv6 /64, missing-source 503 and no tenant. Unit source tests cover duplicate/invalid headers. |
| `SocialSignIn::admit`, SQL `admit_signup`; shared HTTP error renderer | `signup::every_signup_throttle_carries_retry_after` checks 429 header equality for start, completion identity and completion source, plus unavailable admission. Email-address throttling is checked separately through real Mailpit sends. |
| `SocialSignIn::pseudonym`, keyring pseudonym, auth-maintenance worker | `signup_state_is_pseudonymous_and_purged_in_batches` checks actual 32-byte counter/creator storage and bounded maintenance; SQL lifecycle specialist must qualify cap/retention and rotation behavior. |
| shared `new_password`, `hash_password`; browser password handlers | `pg_browser_sessions::every_password_path_holds_the_fifteen_character_minimum` drives creation/invitation/change/admin-reset; asserts normalized 14, 129 and oversized refusals, 15 and Unicode 128 acceptance. Local signup short-password refusal also exists. |
| `HumanConnections::tenant_login`, `SocialSignIn::login_methods` / pure `available_methods`; server `available_methods` / `require_password` | `methods_report_exactly_the_usable_capabilities` checks full flat response across defaults, SMTP/provider/password/signup settings, required SSO with/without Active connection and root. `delivered_paths_refuse_a_method_no_longer_offered` checks password disabled/SSO required before delivered password operations and returning provider start; unchanged User count and no cookie. Callback/session/renewal race proof belongs to later cycles. |
| `begin_email_signup`, ApplicationMail, `EmailAddress` | `email_signup_proves_a_mailbox_in_the_browser_that_asked` checks actual fragment-linked message, preserved delivery case, canonical comparison, no pre-proof record, same-browser/wrong proof behavior and recipient admission before SMTP. |
| `verify_email_signup`, `admit_signup`, server `signup_complete` | `completion_consumes_either_proof_and_recovers_its_own_claim` checks credentials per proof kind, usable local login/admin permissions, completed email clearing, owner-only stale resume and expired refusal. Its replay assertion currently encodes the wrong 409 behavior (B-003). |
| provider proof/contact → `SignupCredential` → tenant provisioner → ordinary session | `every_signup_creator_gets_a_verified_contact` checks Google verified/no-SMTP, refusal and bound mailbox fallback for unverified Google, stored tenant contacts, local admin actually issuing invitation, GitHub verified address and acquisition-only user:email. Provider/claim specialists qualify their invariants. |

Tests are real Rust HTTP/server/Postgres integration paths using `WyrdTestServer`, Mailpit and controlled provider servers. They are not Rust-client projections: that acknowledged gap remains material for full-task completion and is assigned to the surface work, including task cycle 10's explicit Rust-client proof. Direct SQL stages failures and retirement; it does not prove a supported operator retirement journey.

## Proposed blocking findings

### B-001 — INCORRECT: local signup hashes before completion admission

- Requirement: REQ-017 says separate shared source/address-account budgets apply before SMTP or hashing; REQ-014 requires admission before expensive creation and cheap checks first. Task cycles 1–5 establish these foundations for later consumers.
- Exact location: `crates/wyrd/wyrd-auth/src/social.rs:1673`, `SocialSignIn::admit_signup`; completion source/identity admission is at lines 1726–1738. Caller: server `auth/social.rs:478–480`.
- Starting conditions/trigger: a visitor possesses a live local-email completion cookie; its source or proven-identity completion budget is already exhausted (or shared admission is unavailable). Submit otherwise valid local credentials repeatedly.
- Expected: refuse before Argon2id derivation, with the normal 429/Retry-After or unavailable-state response and no claim.
- Actual: `hash_password(password).await?` executes before `self.admit(...)`. Every refused retry reaches the bounded, expensive hash work before its budget is consulted. The blocking pool bounds concurrency but does not enforce the required admission boundary.
- Evidence: direct caller-to-result source ordering. Existing throttle journeys use provider proofs and establish response semantics, not absence of password hashing on refused local completion.
- Consequence: a single proved mailbox can continue spending password-derivation capacity after throttling; the advertised cheap admission boundary is ineffective for this local branch.
- Smallest correction: retain cheap credential validation at the existing owner, derive identity and apply shared source/identity admission before invoking the existing hash owner. Preserve local/social union, caller errors and single claim path. Do not add a new rate limiter or password engine.
- Closure: focused real local-email completion journey exhausts each applicable budget and verifies the refused request never enters password derivation; also cover unavailable shared admission. Use an existing observable derivation seam or narrowly scoped owner-level assertion plus the HTTP journey, not latency timing or synthetic host load. Verify accepted local completion still signs in; 429 and Retry-After remain identical.

### B-002 — INCORRECT: removing SMTP invalidates already issued local signup proofs

- Requirement: REQ-017: removing SMTP disables mail-dependent starts, while signed proofs retain their deadline if signup/method remain enabled. Security posture's application-mail authority repeats that behavior.
- Exact locations: `crates/wyrd/wyrd-auth/src/social.rs:1500–1502` (`verify_email_signup`), `1670–1672` (`admit_signup`). Both consult `login_methods(None).creation.local_signup`; `available_methods` line 1072 requires SMTP configuration for this advertisement.
- Starting conditions/trigger: operator has enabled password/signup and SMTP. A visitor receives the real signup email. Operator removes SMTP before the visitor verifies, or removes it after verification before local completion. Signup and password stay enabled.
- Expected: new mail-dependent starts refuse; the already issued signed proof and resulting live completion remain usable within their existing deadlines and yield the ordinary local tenant/admin/session.
- Actual: verification and completion refuse `signup_unavailable` solely because the relay record is absent. No SMTP send is needed at either step. Requiring a currently advertised start capability incorrectly revokes issued authority.
- Evidence: `available_methods` computes `local_signup = settings.signup_enabled && settings.password_enabled && mail`; both consumers use that exact field. Existing email journey covers missing SMTP before start but never removal after delivery.
- Consequence: legitimate users lose a proof they were explicitly told remains valid, even though no disabled authentication method or expired proof justifies refusal.
- Smallest correction: keep the one advertised-method evaluator for new starts; existing proof/complete validity should check current signup/password enablement separately from mail-send prerequisites through the existing SocialSignIn owner. Preserve current provider revision and deadline checks and keep disable-signup/disable-password refusals.
- Closure: real Mailpit/server journey removes relay after delivered proof and proves verification/completion; second case removes relay after proof and completes. In each case a fresh start refuses, no second mail is sent and the created User/session/contact is correct. Disable signup/password variants must still refuse with no tenant.

### B-003 — INCORRECT: successful completion retry is returned as a conflict

- Requirement: REQ-014 explicitly says successful completion retries return the same tenant; task cycle 7 says replay/concurrent completion returns the same successful tenant. One-use proof consumption does not require changing a successful idempotent result into an error.
- Exact location: `crates/wyrd/wyrd-auth/src/social.rs:1654–1658` returns `WyrdError::Conflict` for completed proof; server `signup_complete` exposes only the error rather than `SignupCompleteResponse`.
- Starting conditions/trigger: a creation succeeds but its HTTP/session response is lost; browser still holds its valid completion cookie and retries within the proof deadline.
- Expected: same successful tenant result, no second tenant/User/grant/claim and no foreign adoption. Session issuance remains subject to current permitted method rules.
- Actual: 409 `workspace_created`, tenant named only inside problem details. The caller must treat an error payload as success and change workflow to returning login. The implementation ledger calls this an interpretation, but it contradicts the explicit approved success-retry promise.
- Evidence: `completion_consumes_either_proof_and_recovers_its_own_claim` step 4 expressly asserts CONFLICT, details.tenant and no session; task cycle 7 records the same. This is strong evidence for current behavior, not acceptance of that behavior.
- Smallest correction: make the existing completion/provisioning owner recognize completed same-attempt results and return the same typed successful tenant result without creating again. Preserve one-use provider/email proof recording and all identity, expiry and ownership checks. Coordinate with lifecycle specialist's race/recovery findings; do not build a second provisioner.
- Closure: lost-response replay and racing completion against two replicas converge on the same typed successful tenant; assert exactly one tenant, one human admin/contact/binding and no second initialization. Keep foreign/expired attempts refused and returning login usable.

## Requirement checkpoint map

This is a primary discovery map, not the final reconciled verdict matrix. PASS below is source/assertion coverage qualified by historical execution evidence, not a new runtime execution result. Specialist findings may alter rows.

| Assigned contract / task references | Current implementation and proof assessment | Checkpoint |
|---|---|---|
| REQ-014 source, retry metadata; cycles 1–2 / AC-005 | Trusted source and normal shared HTTP rendering have concrete negative-path assertions. | PASS |
| REQ-014/017 pseudonyms and bounded retention; cycle 3 | Runtime storage/maintenance assertions exist; cap/key lifecycle covered separately. | UNRESOLVED pending specialist |
| REQ-014/015/017 password minimum; cycle 4 / AC-012 | Existing callers reuse shared validator and have boundary journey assertions; later new callers remain assigned to their cycles. | PASS for delivered callers |
| REQ-011/012/014/016/017 methods; cycle 5 / AC-011 | HTTP advertisement and delivered early-refusal assertions exist. Rust-client/MCP projections and later credential consumers remain unbuilt. SMTP-issued-proof rule violated. | FAIL B-002; remainder unfinished |
| REQ-012/014/017 pre-proof sealing and email proof; cycle 6 / AC-005/007 | Real message/browser/record assertions exist; security specialist qualifies proof secrecy/replay and deadlines. | FAIL B-002; specialist pending |
| REQ-012/014/017 final completion/recovery; cycle 7 / AC-005/007 | Unified flow exists; local hash admission and replay semantics fail. SQL specialist qualifies interruption/concurrency. | FAIL B-001/B-003 |
| REQ-014 cap; cycle 8 | Real two-replica race and purge assertions; retirement only SQL. | UNRESOLVED pending specialist |
| REQ-014 optional Turnstile; cycle 9 | Browser completion calls mock Siteverify after admit and before claim; missing/wrong/used/unavailable behavior asserted. Provider specialist qualifies transport and secret boundary. | UNRESOLVED pending specialist |
| REQ-011/012/013/014/017 local/social contact acquisition; cycle 10 | Real contacts and session invitation use are asserted; missing Rust client remains acknowledged surface gap. | FAIL current completion findings; projection unfinished |
| REQ-014 named-human setup; cycle 11 / AC-008 | Intentional journey RED only; production implementation not claimed. | UNRESOLVED: planned unfinished |
| REQ-011/017 contact replacement/discovery/reset; cycles 12–13 / AC-009/010 | No completion claim; existing contact table is a foundation, not these outcomes. | UNRESOLVED: planned unfinished |
| REQ-018 platform human console; cycle 14 / AC-013 | No completion claim; existing API-token sessions do not substitute. | UNRESOLVED: planned unfinished |
| REQ-002/011–014/017/018 mock/pages/docs; cycles 15–16 / AC-004/005/007–013 | Mock gate and production pages remain assigned; no invented approval gap finding. | UNRESOLVED: planned unfinished |
| REQ-012/015/016 linking/removal/SSO; cycles 17–19 / AC-006 | No completion claim; later connected credentials/race tests remain necessary. | UNRESOLVED: planned unfinished |
| REQ-002/011–018 all delivered surfaces and live providers; cycle 20 | SDK/MCP/CLI/browser connected delivery and real provider smoke have not been claimed. | UNRESOLVED: planned unfinished |

## Behavioral evidence and TDD are separate

Historical execution record: task reports targeted 1138/1138 at `4dd3b6b24a` plus clippy -D warnings, codegen:check, docs:check, check:tenant-isolation. Candidate `b7206eafef` adds the intentional setup RED and task handoff only; no new runtime result was produced by this reviewer. Recorded evidence is not silently promoted to full candidate acceptance. No relevant verification/gateway/Oracle/Scribe hot-path change was identified in primary scope; no L1 benchmark assertion is made.

Recorded TDD violations must remain visible: cycle 1 says production preceded the first journey run (no observed RED); cycle 7 step 4b fix preceded the assertion; Turnstile client/unit tests have acknowledged test-first violation. Cycle 7's initial RED proves cookie TTL only; later stale-resume mutation adds assertion strength but is not historical RED for that branch. Do not demand historical RED reconstruction or production churn as remediation. The remaining cycles have not completed and cycle 11's expected RED is not an unexpected-failure diagnosis.

Primary recommendation: FIX_REQUIRED for current completed work, subject to independent validation and reconciliation with specialist findings. Continue original unfinished cycles separately after correcting retained owner-level defects. No production implementation, git commit, broad suite rerun or live-provider test was performed in this review.
