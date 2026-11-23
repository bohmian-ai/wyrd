# Security boundary discovery — TASK-003-carryover mid-task

Immutable base `4355587f128a0ac1d05071be0a8a84ba53a52290`; candidate `b7206eafefbf7e981898eca3fc22a5d5ae76b815`; approved SPEC-authorization-model revision 7. Read the cumulative diff directly. HEAD stayed unchanged. Scope is implemented cycles 1–10, with cycle 11 RED only and cycles 12–20 unfinished.

## Scope, authority and evidence

Independently examined sealed signup/provider/email proofs, browser/origin/purpose/revision/replay binding, provider verified contacts, fallback contact proofs, optional Turnstile and secret exposure. Read root AGENTS.md, SQL AGENTS.md, spec-driven development, maintainer style, testing and relevant Rust/error guides; governing spec REQ-011/012/014/017 and persistence section; security posture social-admission/application-mail/browser protections. CodeGraph preceded source tracing; actual diff and source ranges not returned by CodeGraph were read directly.

| Boundary | Owners and actual assertions inspected | Assessment |
|---|---|---|
| Pre-proof state and browser round-trip | SocialSignIn::begin_signup/begin_email_signup, SignupEnvelope, server signup/callback/signup_verify; signup_start_keeps_its_round_trip_in_a_sealed_cookie; email_signup_proves_a_mailbox_in_the_browser_that_asked | No pending signup row. Sealed purpose variants, fixed redirect, PKCE/nonce/provider revision, path-scoped HttpOnly/SameSite cookies, same-origin POST and duplicate-cookie refusal. Unique insert protects simultaneous social callbacks. Email digest retention fails below. |
| Provider contact provenance | SocialSignIn::verify_code/prove_signup; GithubOAuth::authorize/identity/api_get; every_signup_creator_gets_a_verified_contact | Google email retained only with validated email_verified=true. GitHub acquisition alone requests user:email and reads authenticated verified email records, primary first. Returning/invitation use read_email=false. No provider token persistence introduced. |
| Missing provider contact | begin_email_signup completion lookup, sealed attempt binding, verify_email_signup, attach_signup_contact, admit_signup | Mail proof attaches only to existing proven/unexpired social attempt, rotates completion, preserves social identity and absence of password. No-contact completion refuses. |
| Turnstile | require_challenge, Turnstile::verify, server signup_complete, challenge journey | After admission, before claim; success, exact own hostname and signup action required. Missing/invalid/outage refuses. Secret sealed in storage, omitted from read response. Operator programmatic provisioning does not invoke challenge. |
| Finished signup retry | admit_signup completed branch, server signup_complete | Conflict includes tenant slug. Primary reviewer covers explicit same-tenant-success requirement; not duplicated here. |
| Proof strength | Task execution record and actual journey assertions | Prior execution results are recorded evidence, not rerun here. Immediate replay assertions do not cover cleanup before original email-envelope expiry or SMTP withdrawal after issue. |

No added verification/gateway/Oracle/Scribe hot-path wait was found in this boundary; signup/auth control-plane calls are not those hot paths. No benchmark claim. No production/test edits, external provider calls or Cargo commands were performed.

## Proposed findings for independent validation

### DOMAIN-1 — INCORRECT: email spent digest is purged before its envelope expires

**Requirement:** REQ-017: email proofs last fifteen minutes, unique identifier digests enforce one-use across replicas, and spent identifiers remain until expiry. REQ-014: replay creates no second tenant. Task cycle 6 explicitly requires spent retention preventing replay.

**Locations:** `crates/wyrd/wyrd-auth/src/social.rs:1506–1519` inserts the proof using five-minute SIGNUP_COMPLETION_TTL. `crates/wyrd/wyrd-sql/src/queries/platform/signup.rs:98–103` sets the sole expires_at to now plus that duration; `:327–331` deletes that unique digest after expiry. The original envelope deadline at social.rs:1432 uses fifteen-minute SIGNUP_EMAIL_TTL, checked at :1483.

**Concrete path:** At t=0 start local signup and retain the flow cookie and mailed proof. At t≈1m verify and complete workspace A. At t≈7m, after auth maintenance purges the expired five-minute completion attempt, replay the same original cookie/proof to `/auth/signup/verify`. The fifteen-minute envelope is still valid.

**Expected:** A spent email identifier cannot produce another attempt/workspace. Five-minute completion expiry and fifteen-minute spent-proof retention are separate properties.

**Actual/source proof:** `insert_proven_signup` finds no conflicting proof_hash and inserts a new id and fresh five-minute completion. That completion can create workspace B while below the identity cap. `platform.tenants.signup_attempt_id` uses ON DELETE SET NULL, so retaining A does not retain the unique digest. Cookie clearing does not enforce durable one-use: a replay caller can retain its own cookie. Existing immediate-replay tests do not reach this path.

**Consequence:** Routine cleanup makes a spent one-use proof usable for another creation and renews expired completion authority. The cap bounds quantity, not one-use semantics.

**Smallest correction:** Preserve consumed email-identifier metadata until the original envelope deadline independently of completion expiry, through the existing approved proof/consumption and bounded-cleanup owner. Keep completed email erased and lifetimes unchanged. Add a focused HTTP/server+Postgres scenario that expires/purges completion while its original envelope remains valid; replay must create no new attempt/authority/workspace. Preserve legitimate same-browser lost-response recovery and eventual bounded purge at the true proof deadline. No new mail engine is needed.

### DOMAIN-2 — INCORRECT: removing SMTP revokes already issued local proofs

**Requirement:** REQ-017 explicitly permits existing signed proofs until their deadline when signup/their method remain enabled; removing SMTP prevents mail-dependent starts. Security posture application-mail section agrees.

**Locations:** `crates/wyrd/wyrd-auth/src/social.rs:1500` (`verify_email_signup`) and `:1674–1676` (`admit_signup` local branch) require `login_methods(None).creation.local_signup`. `login_methods` reads relay presence at :748–752; local_signup includes it.

**Concrete path:** With password/signup/SMTP enabled, issue the mailbox proof. Remove SMTP without changing password/signup. Verify the received proof, or verify first and remove SMTP before `/auth/signup/complete`.

**Expected:** New mail-dependent starts disappear; valid issued proof/completion still works until its deadline. Actual password/signup disablement still refuses.

**Actual/source proof:** Both redemption stages use the start-advertisement predicate, return signup_unavailable (401), and strand valid proof holders even though neither stage needs SMTP IO. Current tests cover absent SMTP before starting, not removal after issue.

**Consequence:** Relay configuration changes revoke already issued proof contrary to the agreed lifecycle.

**Smallest correction:** At the existing SocialSignIn owner distinguish start availability from redemption eligibility, preserving current signup/password toggles and provider checks. Add focused HTTP+Postgres+Mailpit tests for removal before verification and between verification/completion: valid creation succeeds, new starts refuse, actual signup/password disablement still refuses. No new response shape or persistence purpose is required.

## Limits and process

This is source-backed discovery, not an independently rerun GREEN claim. Live providers, browser UI and unfinished cycles are not accepted here. TDD history is assessed separately by the primary reviewer; acknowledged cycle-7/Turnstile ordering violations cannot be repaired retroactively and do not replace behavioral findings.
