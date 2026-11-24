# R3 remediation verdict: FIX_REQUIRED

Findings FIND-003-carryover-13 through -17 close. **FIND-003-carryover-12 remains partially fixed:** removal revokes existing sessions and pending stored logins, but an already-running callback can finish after removal/reinstall and create a new session. No new finding ID is assigned. [R4 remediation](TASK-003-carryover-R4-fence-inflight-platform-login.md) contains the remaining bounded scope.

## Subject, authority and independent coverage

Prior candidate `dfb3599d5fbfb3d863ecb68382257cb30abfd79b`; candidate `5ea7aa8ce310b33dc8cebb139aa5202c7b67b5a4`; cumulative base `63ee31a0205c9b59a11877d5cf571d7bd0632cd6`. Eight commits, unchanged ancestry. Reviewed direct `git diff dfb3599d5 5ea7aa8ce`, with cumulative source available. Original TASK-003-carryover cycles12–16, approved authorization-model revision11, prior validated findings/verdict and R3 packet remain the subject. See [subject](subject.md). Cycles17–18 deferred,19–20 TASK-004 and unrelated verification-workflow-ui documents remain excluded.

Authority is unchanged: REQ-011–014/017/018 and mapped acceptance, approved persistence purposes, root/nested SQL/docs/TypeScript AGENTS, security posture social mail/platform browser/tenant binding, operations browser deployment, Wyrd UI skill, workflow, maintainer style and TESTING. No migration, new persistence purpose or product-contract decision was introduced.

Fresh `r3_validation` independently reviewed all prior closures, direct changes, affected callers/consumers/invariants, assertions, evidence and carried-forward coverage in [remediation review](task-review-remediation.md) and [findings validation](findings-validation.md). Its proposed incomplete -12 closure was separately checked by fresh `r3_oidc_focus` in [focused source validation](oidc-closure-validation.md). Both independently trace the same supported interleaving. Root orchestrated and executed checks; it did not discover or source-validate findings. No new discovery sweep or general specialist was needed: existing-owner corrections are bounded, with prior specialist evidence retained. The newly exposed concurrency seam received the required separate independent check.

## Reconciled closure matrix

| Existing finding / scenario / authority | Implementation and credible proof | Status |
|---|---|---|
| -12; R3S1; REQ-018/AC-013 original OIDC authority invalidation | SQL removal atomically revokes existing OIDC sessions/deletes outstanding state; real HTTP test proves sequential same/different-issuer reinstall, pending callback refusal, fresh login and password/API preservation. Fresh exact test passes. Callback already consuming/exchanging its state can still issue after removal; independently confirmed complete source path. | FAIL; partial closure |
| -13; R3S2; REQ-017 recipient privacy | Shared bounded16-permit transient send removes SMTP outcome/wait from discovery public response, reuses reset mechanism. Delayed/refused/healthy real-relay assertions and mail counts; fresh exact journey passes. | PASS; CLOSED |
| -14; R3S3; REQ-011 cap presentation | Production/mock cap selects existing terminal/sign-in branch; actual browser checks refusal, absent form/button and no second workspace. Fresh onboarding11/11 includes case. | PASS; CLOSED |
| -15; R3S4–5; REQ-011–014/AC-005 connected page proof | Production browser/BFF/Rust/Postgres with controlled Google, real Mailpit contact continuation and actual Rust Siteverify fail/reset/pass. Fresh onboarding11/11, BFF5/5, Rust entry points2/2. Readiness probes at existing lifecycle; original SMTP cause remains unproven. | PASS; CLOSED |
| -16; R3S6; REQ-017 first-use original expiry | Real envelope resealed with expired deadline and binding retained, contact/discovery refusal/no effects plus live controls; fresh exact HTTP journey passes. | PASS; CLOSED |
| -17; R3S7; relay update/docs owner | Existing guide/corpus explain keep-password/changed-username/removal; fresh `docs:check` exit0 including48-page light/dark accessibility. | PASS; CLOSED |

Unchanged original acceptance rows from the prior cycles12–16 verdict carry forward: contact freshness/replacement, live bounded directory revalidation/RLS/reconciliation, private reset and stale/foreign/offline codes, platform password/session-plane/TTL/replica/suspension/reauth, reserved slugs, bounded host routing and prior recorded mock sweep. Review traced affected dependencies rather than treating the old pass as unconditional. The callback issuance path is affected by this fix and explains why -12 stays open; it is neither a new product requirement nor unrelated debt. Findings1–11 retain their earlier bounded closure.

## Retained risk and smallest next scope

The remaining finding is recorded once in [findings-validation.md](findings-validation.md), with complete source proof in the focused report. A normal callback takes/deletes its state before waiting on the provider. During that wait, removal cannot delete the already-consumed state or revoke a not-yet-created session. Reinstall reuses revision1; final insert accepts it and creates fresh browser authority from the old callback. Existing sequential removal tests never place removal inside the provider wait.

Complete final issuance against durable original-login/connection validity at the existing owners, without holding a SQL transaction/lock across provider IO. Preserve one-use redemption, password/API availability and current sequential invalidation. The R4 task calls for a controlled delayed-token response journey and only this correction; it demands no migration or whole-task restart.

## Verification, TDD and limits

[Fresh execution](verification.md): identity UI2/2 (onboarding11/11, BFF5/5); exact console-removal/discovery-privacy/first-use-expiry3/3 plus migration1/1; docs gate exit0; tenant-isolation and direct whitespace pass. Existing assertions do not exercise the retained in-flight race; no failing runtime reproduction is claimed. Source inspection establishes its full supported path. R3's fmt/lints/UI105/codegen and contacts/resets11/11 remain implementation receipts, not fresh root reruns.

No defined verification/gateway/Oracle/Scribe hot-path PostgreSQL/Scribe/object-store/Vault wait is added or moved. Shared transient SMTP waits are background work outside those paths. No benchmark applies. No production/source/migration fix, commit, push, merge or deployment occurred during review; HEAD and tracked source stayed immutable, unrelated edits remained intact.

Behavioral acceptance remains FIX_REQUIRED; **TDD history remains violated separately.** Preserve original late-test history and R3 -13 production-before-test/stash-recovered RED. Evidence-only -15/-16 first-run passes and mutation checks strengthen proof without establishing historical RED or requiring production churn. R3 closes five findings but incompletely fixes the original OIDC lifecycle finding; this is convergence on one unchanged obligation, not expanding expectations after multiple earlier repairs.

No new visual-match ruling is claimed: cap behavior now selects existing DOM/classes/wording, with functional proof; the handoff's absent side-by-side light/dark comparison stays disclosed. Full image startup, live Google/GitHub/Cloudflare smoke and final launch qualification remain outside this bounded review. No R3 finding is retained solely for those exclusions.
