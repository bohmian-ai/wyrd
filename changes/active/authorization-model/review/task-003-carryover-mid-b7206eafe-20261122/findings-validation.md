# Independent findings validation

Subject: `4355587f128a0ac1d05071be0a8a84ba53a52290...b7206eafefbf7e981898eca3fc22a5d5ae76b815`; approved [spec revision 7](../../spec.md), [original task](../../tasks/TASK-003-carryover.md). This fresh validator independently inspected source, consumers, governing requirements, actual assertions and all four discovery reports. HEAD remained the candidate. No runtime test was executed during validation; findings below distinguish source-proven defects from unavailable execution evidence.

## Proposal reconciliation

| Discovery proposal | Decision | Retained ID and reason |
|---|---|---|
| behavior B-001 | CONFIRMED | FIND-003-carryover-1: expensive local derivation precedes both completion windows. |
| behavior B-002, domain DOMAIN-2 | CONFIRMED, consolidated | FIND-003-carryover-2: the same start predicate incorrectly controls issued-proof redemption at both stages. |
| behavior B-003 | CONFIRMED | FIND-003-carryover-3: explicit typed successful retry contract contradicts the implemented 409. |
| domain DOMAIN-1 | CONFIRMED | FIND-003-carryover-4: cleanup removes the unique digest while the original sealed proof remains valid. |
| invariant INV-01 | CONFIRMED | FIND-003-carryover-5: documented rotation changes both count key and advisory-lock domain. Correction remains within existing pseudonym/claim owners; no new persistence mechanism is prescribed. |
| invariant INV-02 | CONFIRMED | FIND-003-carryover-6: sibling operator creation adopts signup claims without testing their durable ownership marker. |
| repo REPO-01 | CONFIRMED | FIND-003-carryover-7: the production BFF drops source information before Rust ingress. This affects existing password pages, independently of future acquisition pages. |
| repo disabled-method status reconciliation | CONFIRMED | FIND-003-carryover-8: the approved HTTP table explicitly requires 403; tests pin 401. This proposal was supplied by discovery, not newly discovered by the validator. |
| repo REPO-02 | CONFIRMED as evidence gap only | FIND-003-carryover-9: no available result establishes the mandatory UI lane. This does not prove a UI implementation defect. |

No proposed finding was rejected. SMTP duplicates are retained once. No new blocker was introduced by validation. Known unfinished cycles, missing signup projections, retirement surface and live-provider proof remain original task work or delivery limits, not independent discoveries assigned new IDs.

## Retained findings

### FIND-003-carryover-1 — INCORRECT: throttled local completion still hashes the password

**Authority:** REQ-014 admission before expensive work; REQ-017 shared budgets before hashing; task cycles 1–2 and 7. **Owner/location:** `crates/wyrd/wyrd-auth/src/social.rs:1640`, especially `hash_password` at 1673 and shared admission at 1726–1738; HTTP caller `crates/wyrd/wyrd-server/src/auth/social.rs:478`.

**Scenario and proof:** A browser holds a live email completion and submits valid local credentials after its source or identity completion budget is exhausted, or while admission is unavailable. The local branch validates credentials and awaits Argon2id before consulting either budget. Expected refusal precedes derivation. Actual refusal spends derivation capacity on every retry; bounded blocking concurrency does not enforce admission. Existing provider-based throttle assertions establish 429 metadata, not this local ordering.

**Material effect:** One proven mailbox can keep consuming password-derivation capacity after its completion budget is exhausted. **Smallest correction:** At `SocialSignIn::admit_signup`, preserve cheap credential validation but apply the existing completion budgets before invoking the existing password derivation owner. Keep one completion path, the same refusal metadata and no-claim behavior. Close with real local completion refusal assertions plus a focused owner-level assertion/seam proving derivation was not entered; do not use elapsed time or synthetic host load.

### FIND-003-carryover-2 — INCORRECT: withdrawing SMTP revokes issued local proof authority

**Authority:** REQ-017 issued signed proofs survive SMTP removal until deadline while signup/method stay enabled; security posture application-mail section; cycles 5–7 and 10. **Owner/location:** `crates/wyrd/wyrd-auth/src/social.rs:1500` and `1670`; `login_methods` at 733 and `available_methods` at 1072.

**Scenario and proof:** With signup/password/SMTP enabled, a visitor receives the real email. The operator removes SMTP before verification, or after verification before completion. The two redemption stages check `creation.local_signup`, which includes relay presence. Both now refuse despite an unexpired proof and enabled signup/password. Neither stage needs to send mail. The spec permits removing mail-dependent starts, not this revocation.

**Material effect:** A legitimate delivered proof becomes unusable for an unrelated configuration change. **Smallest correction:** Reuse `SocialSignIn` to distinguish new-start prerequisites from existing-proof eligibility. Keep deadlines, actual signup/password disablement and provider revision checks. Close with delivered Mailpit proof → relay removal → verification/completion, a separate removal-between-stages case, and fresh-start/real-method-disable negative cases.

### FIND-003-carryover-3 — INCORRECT: a successful completion retry is a 409 error

**Authority:** REQ-014 and revision-6 `/auth/signup/complete` contract: repeat successful completion returns the same tenant; task cycle 7. **Owner/location:** `crates/wyrd/wyrd-auth/src/social.rs:1654–1658`; HTTP `signup_complete` at `crates/wyrd/wyrd-server/src/auth/social.rs:454–547`.

**Scenario and proof:** Creation succeeds but the response is lost; the same browser retries with its live completion cookie. `proven_signup` finds the completed attempt, then `admit_signup` returns `Conflict` with the slug in problem details. The journey `completion_consumes_either_proof_and_recovers_its_own_claim` at `signup.rs:1181–1187` explicitly expects 409 and no session. The specified response is successful typed `{tenant}`, not an error a consumer must reinterpret.

**Material effect:** The ordinary retry workflow cannot recover its successful result through its declared contract. **Smallest correction:** Have the existing completion/provisioning owners return the completed same-attempt result without reinitializing. Preserve proof expiry, ownership, current-method checks and one-use proof recording. Close with lost-response retry and concurrent completion convergence on the same typed successful tenant, with exactly one tenant/admin/contact/binding and no duplicate initialization. Do not treat a concurrently in-progress response as permission to duplicate work.

### FIND-003-carryover-4 — INCORRECT: cleanup re-enables a spent email proof

**Authority:** REQ-017 fifteen-minute email proofs, five-minute completion, spent digest retained until original proof expiry; REQ-014 one-use/replay; cycles 3 and 6–7. **Owner/location:** `crates/wyrd/wyrd-auth/src/social.rs:1506–1519`; `crates/wyrd/wyrd-sql/src/queries/platform/signup.rs:98–103,327–331`; original email envelope deadline at `social.rs:1432`.

**Scenario and proof:** Start at t=0, verify and create tenant A near t=1 minute, retaining the original sealed email cookie and mailed proof. After t=6 minutes, maintenance may delete the attempt because its only `expires_at` was set from five-minute `SIGNUP_COMPLETION_TTL`. Before t=15 minutes, replay the original cookie/proof. Envelope validity passes; the unique digest is gone; `insert_proven_signup` creates a new attempt/completion that can create tenant B below the cap. Tenant FK nulling does not retain the digest. Clearing the browser cookie is not durable one-use enforcement against a caller replaying its own retained cookie.

**Material effect:** Bounded routine cleanup reissues spent creation authority. **Smallest correction:** Preserve consumed identifier metadata through the original email-envelope expiry independently of completion expiry, at the approved proof/consumption and bounded-cleanup owners. Preserve erased completed email and both existing lifetimes. Close with actual expiry/purge of completion while the original envelope remains live; replay must create no new attempt, completion authority or tenant. Also preserve same-browser lost-verification-response recovery and eventual bounded cleanup. Any supporting schema work must cite the already-approved proof/consumption purpose and must not edit an applied migration.

### FIND-003-carryover-5 — INCORRECT: sealing rotation resets and splits persistent cap accounting

**Authority:** REQ-014 per-identity persistent atomic cap across replicas; security posture creator accounting; documented supported keyring rotation in `AuthConfig`; cycles 3 and 8. **Owner/location:** `crates/shared/wyrd-crypt/src/lib.rs:301–316`; `crates/wyrd/wyrd-auth/src/social.rs:1725`; `crates/wyrd/wyrd-sql/src/queries/platform/provisioning.rs:110–127`.

**Scenario and proof:** At cap 1, an identity owns a non-retired signup tenant created under write key A; its proof is purged. Rotate to B retaining A using documented configuration (`config.rs:3183–3188`). `pseudonym` uses only `self.write.1`, so the next claim locks/counts HMAC-B while the prior tenant retains HMAC-A. It sees zero and admits a second tenant. During a rolling switch replicas also use different lock domains. Existing rewrap handles sealed values, not one-way creator keys. The task acknowledges this limitation; the approved cap has no rotation exception.

**Material effect:** Routine supported key maintenance clears an enduring resource limit and defeats cross-replica atomic accounting. **Smallest correction:** Keep creator identity matching and the claim lock domain coherent across the supported key lifecycle using existing keyring/pseudonym and claim owners. Merely retaining old keys or adding multiple counts without a common lock does not suffice. Preserve keyed pseudonyms, safe sealing rotation and separate email/provider namespaces; do not add a global person or prescribe an unapproved secret/persistence boundary. Close with cap filled → attempt purged → A-to-B rotation → same identity refused, plus a cross-replica rollout race and preserved old-cookie decryption/login.

### FIND-003-carryover-6 — INCORRECT: ordinary operator creation adopts an expired signup claim

**Authority:** REQ-014 crash recovery: only the owning live proven attempt may resume; after expiry the claim stays non-active until operator retirement; cycle 7. **Owner/location:** `crates/wyrd/wyrd-sql/src/queries/platform/provisioning.rs:45–57`; caller `TenantProvisioning::claim` at server `components/platform/provisioning.rs:203–237`, then `provision` at 262–298; public `create_tenant` route.

**Scenario and proof:** A signup claim is failed, or provisioning stale for fifteen minutes; the five-minute proof has expired. An authorized operator requests the same slug through ordinary `POST /platform/tenants`. Generic `insert_provisioning_tenant` adopts any failed/stale row, without checking signup ownership; generic provision then establishes its bootstrap owner and promotes the tenant. After attempt purge the creator marker still identifies signup origin, but the generic query ignores it too.

**Material effect:** A sibling supported write bypasses the settled exclusive-resume/expiry lifecycle. Operator authority permits retirement, not silent substitution of the documented recovery contract. **Smallest correction:** Guard generic adoption at the existing directory owner using ownership that survives attempt purge; retain ordinary operator-owned failed-claim recovery and same-attempt live signup recovery. Close with authenticated operator creation against an expired failed/stale signup claim before and after actual attempt purge: conflict, same non-active tenant and no new bootstrap/User/grant effects. This finding does not add a retirement surface.

### FIND-003-carryover-7 — INCORRECT: the production BFF drops the visitor's admission source

**Authority:** REQ-014 trusted source also applies to password admission; security posture 508–514; cycle 1; native deployment authority says the BFF forwards browser authentication. **Owner/location:** `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/browser-sessions.ts:45–55,252–263,283–296`; tenant login action passes only URL/cookies; Rust `auth/source.rs:45–60,81–90` and `auth/browser.rs:671–685` consume the source.

**Scenario and proof:** Two visitors sign in through the same production Node BFF. Its `call` constructs only origin, forwarded host/protocol, selected cookie and content type; no visitor source crosses the seam. Rust receives Node as peer. If Node is untrusted, both visitors spend Node's shared source window. If trusted, the configured source header is absent and source becomes `None`; optional password source admission is skipped. Origin does not encode the visitor address. Direct Rust-router tests supply synthetic peer/header context and omit this production seam.

**Material effect:** Existing password pages fail the completed separate-real-visitor-source promise. **Smallest correction:** Carry trusted visitor-source context through the existing BFF authentication transport and affected password callers with the deployment's trust boundary. Never accept arbitrary visitor forwarding headers as authority. Keep one Rust source derivation and rate-limit owner. Close with a real BFF-to-Rust journey for distinct visitors, shared IPv6 /64 and forged/untrusted forwarding, retaining independent password/signup budgets. Future signup-page delivery is still separate original task work.

### FIND-003-carryover-8 — INCORRECT: disabled methods return 401 instead of the approved 403

**Authority:** Spec revision-6 HTTP contracts, failures at `spec.md:731–735`; REQ-011/016; cycle 5. **Owner/location:** `crates/wyrd/wyrd-server/src/auth/browser.rs:811–815`; affected `require_password` and social-start callers; signup availability helper `crates/wyrd/wyrd-auth/src/social.rs:2013–2014` uses the same invalid-token class.

**Scenario and proof:** A rendered page offers password, then the operator disables it or the tenant requires company SSO. Submit the previously offered method. `method_unavailable` constructs `InvalidToken`, exposing `WYRD_AUTH_401_INVALID_TOKEN`. The methods journey helper at `methods.rs:250–257` pins 401. Enabled/disabled signup similarly uses `signup_unavailable` → invalid token. The approved failure class for a disabled method is 403; this is a settled wire contract, not wording ambiguity.

**Material effect:** Programmatic consumers cannot distinguish unavailable policy/method authority through the promised stable error class. **Smallest correction:** Use the existing derive-backed catalog and central response mapping for disabled-method refusals across delivered starts/redemptions, updating exact code/status assertions and affected projections/documentation through their generators as necessary. Preserve distinct invalid/expired proof, unavailable dependency, conflict and throttle outcomes. Close by changing the affected existing method-withdrawal journey to assert the exact 403 catalog outcome, absent sessions and no forbidden side effects.

### FIND-003-carryover-9 — EVIDENCE_GAP: changed UI lacks its mandatory lane result

**Authority:** Applicable `.agents/skills/wyrd-ui/SKILL.md` Verification requires `mise run ui:check` for every change under the UI tree; root targeted-lane rules. **Location:** changed `wyrd-ui/src/lib/server/auth/browser-sessions.ts` and tenant `signup/+page@.svelte`; task evidence for cycles 4–5 and 10.

**Missing proof:** The available results name Rust lanes, Clippy, codegen, docs and tenant isolation; no `ui:check` result or linked equivalent evidence is available. The task's later checklist is not an execution result. This finding does not assert that nobody ran the command, or that UI code is broken.

**Material effect:** Required verification of the changed BFF contract and Svelte page is unestablished. **Smallest correction:** Execute the existing `mise run ui:check` on the correction candidate and record its actual result. Production edits or additional tests are required only for failures the lane demonstrates, not by this evidence gap itself.

## Coverage/proof reconciliation

All selected discovery boundaries are present: behavior, security/proof, SQL/lifecycle/concurrency and cross-tier repository/maintainability. Their source-backed coverage includes unchanged consumers and the operator sibling writer, rather than only diff paths. The final matrix is maintained once in [verdict.md](verdict.md). Historical execution at `4dd3b6b24a` remains useful for unchanged production code because candidate adds only the setup RED/test handoff; it is not a fresh candidate run or a substitute for the missing scenarios above. The intentional cycle-11 RED and unstarted cycles cannot establish whole-task acceptance. No verification/gateway/Oracle/Scribe hot-path wait was added or moved in this reviewed onboarding/control-plane slice; no engine benchmark is applicable to these findings as currently bounded.
