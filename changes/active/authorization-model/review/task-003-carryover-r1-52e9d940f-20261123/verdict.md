# TASK-003-carryover R1 verdict: FIX_REQUIRED

Eight of nine prior findings close. FIND-003-carryover-7 remains partially corrected in the official nginx/Node deployment. Two independently discovered and source-validated integration defects remain: FIND-003-carryover-10 (remote wrapping inside configuration transactions) and FIND-003-carryover-11 (existing page actions lose canonical throttle/error metadata). Implement only [R2](TASK-003-carryover-R2-finish-configuration-and-bff-seams.md). All retained findings are defined in [findings-validation.md](findings-validation.md).

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd`.
- Original base: `4355587f128a0ac1d05071be0a8a84ba53a52290`; prior candidate: `b7206eafefbf7e981898eca3fc22a5d5ae76b815`; candidate: `52e9d940fd57c49a3ab5084fb319171e21eafb37`.
- Review uses `git diff <prior> <candidate>` with preserved ancestry. PR 242 entered in `70a56a136`; revision 8 was approved at `75b4e800b`. Review covers its auth port/affected seams, not unrelated merged work.
- Approved current contract: SPEC-authorization-model revision 9. It is an intentionally uncommitted authority input pinned to blob `55689bae96c62e78b1ea8d765a32629664b4e603`, with maintainer approval record dated 2026-11-23. Committed candidate contains revision 8. Security posture input is pinned to blob `0210235fb05322ef6e888526c4c9ba6bec02b234`. [Subject](subject.md) records the hook-held-file exception; neither file was edited, staged or committed by review.
- Governing references: root/applicable SQL rules, spec-driven workflow, maintainer style, testing guide, errors/agent-surface rules, security sign-in/mail/source authority, deployment topology and shared stored-secret REQ-008 adopted by REQ-019.

This is the requested bounded R1 current-work audit. Parent cycle 11 remains deliberately RED; cycles 12–20, their client/mock/provider/retirement proof and full parent completion remain excluded. Human-added REQ-020 is approved scope, not a new defect identifier.

## Independent assignments and coverage

| Fresh reviewer | Boundary and selection reason | Report |
|---|---|---|
| r1_validation | Prior closure, affected consumers/rules, proof strength and independent source validation of all specialist proposals | [remediation review](task-review-remediation.md), [findings validation](findings-validation.md) |
| r1_kms | Material secret-lifecycle/security boundary introduced by shared-KMS port: singleton/race/cache, rotation/retirement/refusal, cap key and transaction lifetime | [KMS specialist](task-review-kms.md) |
| r1_signin | Material new REQ-020 cross-route shared-admission and HTTP/OAuth refusal boundary, BFF source and public-error consumers | [sign-in specialist](task-review-sign-in.md) |

Both selected reports were read before this verdict. The orchestrator performed execution/coordination, not discovery or finding validation. KMS-1 independently validates to -10; SIGNIN-S1 independently validates to -11; SIGNIN-S2 revises original -7. No validator-origin new blocker requires another validator. Existing owner/recovery/maintainability coverage plus these specialists is sufficient; extra whole-PR, system or maintainability votes would duplicate it.

## Reconciled current-work matrix

Only this table is the acceptance matrix. Source/assertion details and closed-finding trace are in the linked reports; no new requirements are defined here.

| Existing promise / scenarios | Implementation and credible proof | Status |
|---|---|---|
| REQ-014 trusted source; R1 -7, cycles 1 / AC-005; REQ-020 source reuse | `stateSource` reaches both auth transports; standalone BFF proof asserts three real windows against forged header. Official startup does not configure adapter address derivation, so nginx loopback is forwarded instead. | FAIL: -7 |
| REQ-014/017 local admission before derivation; R1 -1 | `admit_signup` awaits both windows before `hash_password`; throttled-local journey asserts429 and no claim. Existing supplied journey evidence plus direct structural order. | PASS; -1 CLOSED |
| REQ-017 issued proofs after relay withdrawal; R1 -2 | `require_local_redemption` needs signup/password, no relay. Completion journey delivered-mail continuation verifies and creates after SMTP withdrawal; fresh starts still refuse. Fresh selected journey PASS. | PASS; -2 CLOSED |
| REQ-014 successful same-tenant retry; R1 -3 | `SignupAdmission::Created` and HTTP typed200 without new session; exact same tenant/one admin assertion and existing same-attempt/foreign/stale recovery. Fresh selected journey PASS. | PASS; -3 CLOSED |
| REQ-014/017 durable spent-proof retention; R1 -4 | Ten-minute post-completion retention at bounded SQL cleanup; live lookup/renewal still expire at five minutes. Actual purge then replay refuses new completion; eventual cleanup and email erasure remain asserted. Fresh selected journey PASS. | PASS; -4 CLOSED |
| REQ-014 persistent cap; REQ-019/AC-014 wrap-key rotation and key race; R1 -5 | One durable wrapped derivation key; exactly one shared key on concurrent initialization; unchanged creator count/lock key through maintenance and key retirement. File-backed cap/retirement/keyless journeys and derivation tests freshly PASS; specialist traces stored contexts/caches. | PASS; -5 CLOSED |
| REQ-019 shared secret-owner safe commits | Provider/challenge/SMTP stores and maintenance use canonical contexts and envelope operations, but resource configuration calls remote sealing while holding audited transaction. | FAIL: -10 |
| REQ-014 exclusive signup recovery; R1 -6 | Generic adoption checks durable creator marker for failed/stale before/after attempt purge; no operator User and unchanged inactive status. Existing operator-owned recovery retained. Fresh selected journey PASS. | PASS; -6 CLOSED |
| REQ-011/016 HTTP disabled-method403; R1 -8 | Derive-backed MethodUnavailable403, exact methods/signup/platform assertions, regenerated codes/docs; original direct wire-status defect corrected. Adjacent exact BFF projection is in -11. | PASS for diagnosed status correction; -8 CLOSED |
| Mandatory affected UI check; R1 -9 | Fresh `ui:check`: package/host0 errors0 warnings, 11 files101 tests PASS. | PASS; -9 CLOSED |
| REQ-020/AC-015 direct server start admission | Browser/platform/device begin after shared admission, before resolution/state/provider; 300/hour sign_in scope, shared SQL atomic count, source normalization reuse, separate signup windows, precise429 and503/OAuth mappings. Fresh exact journey PASS; unavailable-counter mapping reuses same source-proven SQL/helper tested by existing signup outage assertions. | PASS at direct server boundary |
| REQ-020 affected BFF action refusal; catalog projections | Existing `BrowserSessions.begin` consumer receives new429 but safe projection maps it to502 without wait; changed MethodUnavailable also lacks exact safe variant. Direct forwarded auth path remains correct. | FAIL: -11 |
| Preserved cycles 2–10 claims outside corrections: password policy, method advertisement, purpose/provider/revision/browser binding, challenge and verified contacts, RLS | Existing prior review source-backed coverage carries only where unchanged; six fresh key/proof/challenge journeys and specialist tracing cover affected dependencies. No new tenant identity merge, fail-open challenge or ownership substitution found. | PASS within delivered current-work scope |
| Parent cycle11 and cycles12–20 / full delivery | Intentional setup RED and unfinished acquisition/recovery/console/mock/linking/SSO/client/provider work are retained in original task. | UNRESOLVED — planned, excluded from bounded acceptance |

## Verification and limits

[Fresh verification](verification.md) records exact managed Postgres/Mailpit commands and run receipts: both migration/idempotence tests PASS, six exact signup/proof/cap/keyless/sign-in/challenge journeys PASS, two derivation tests PASS, UI check101/101 PASS. Reviewers independently read assertions; root executed sequentially. Supplied R1 SQL, format/lints, generated docs/contracts, dependency/tenant and actual BFF journey results remain supporting evidence where source/proof are unchanged, not fresh reruns of every gate.

Passing executed tests do not exercise the retained configuration-transaction, official-image adapter or page throttle paths. Cumulative diff whitespace output has unrelated merged trailing-blank issues and is not described as clean. No broad gate, full parent suite, live cloud/provider credential run or benchmark was run. The reviewed verification-future boxing does not add/move an IO wait; no new KMS/SQL wait enters verification/gateway/Oracle/Scribe hot paths. Control-plane transaction wait -10 remains a concrete explicit-boundary defect even without a benchmark.

## TDD and convergence

Behavioral acceptance and TDD are separate. Preserve original violations at cycle1, cycle7 step4b and cycle9. Preserve R1 items2/5/6 and REQ-020 violations: production preceded their missing-behavior execution. Item1 is supported by structural order and existing journey proof, not an invented historical RED; item9 is verification-only. Later GREEN/mutation proof does not repair history and requires no historical reconstruction or code churn.

This is the first remediation review. Eight original findings close; -7 is an incomplete original correction. -10 is a fix-induced remote-lifecycle violation from the KMS port; -11 is an omitted existing consumer of the new sign-in failure output. These are bounded shared-owner seams, not expanding product expectations. R2 reuses their current startup/auth/projection owners and focused proof; it does not repeat original discovery or reopen unrelated PR 242.
