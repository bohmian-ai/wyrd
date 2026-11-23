# R1 independent findings validation

Immutable subject and approved authority are pinned in [subject.md](subject.md). This fresh validator traced all nine prior findings, actual production consumers and assertions, and independently checked the material specialist proposals. The review does not change production code. Findings below are recorded once; [verdict.md](verdict.md) owns the reconciled matrix.

## Reconciliation

| Proposal / prior finding | Decision | Evidence and disposition |
|---|---|---|
| FIND-003-carryover-1 | CLOSED | Shared control flow awaits source/identity admission before password hashing; local refusal asserts no claim. |
| FIND-003-carryover-2 | CLOSED | Issued proof redemption checks enabled signup/password without SMTP; real delivered-proof continuation succeeds after relay withdrawal. |
| FIND-003-carryover-3 | CLOSED | Typed successful same-tenant replay, no additional session or administrator. |
| FIND-003-carryover-4 | CLOSED | Durable spent digest survives completion cleanup until maximum original proof lifetime; actual purge/replay assertion refuses new authority. |
| FIND-003-carryover-5 | CLOSED | One durable derivation key preserves creator/count/lock identity through wrap-key retirement; fresh cap/rotation/race and keyless proof pass. New configuration transaction violation is separately classified below. |
| FIND-003-carryover-6 | CLOSED | Generic adoption excludes durable signup creator marker before/after attempt purge; failed/stale claims stay non-active with no operator User. |
| FIND-003-carryover-7 / sign-in specialist official-image proposal | REVISED, OPEN | Forwarding code fixes a correctly configured native BFF. Official bundled nginx/Node startup still resolves visitor as loopback; same source-loss cause and stable ID retained. |
| FIND-003-carryover-8 | CLOSED | Central derive-backed MethodUnavailable403 and exact existing assertions/projections. |
| FIND-003-carryover-9 | CLOSED | Fresh candidate UI check: 0 errors/warnings, 101/101 tests. |
| KMS specialist KMS-1 | CONFIRMED | FIND-003-carryover-10: three new remote seal calls occur inside open audited transactions. |
| Sign-in specialist BFF throttle projection proposal | CONFIRMED | FIND-003-carryover-11: existing sign-in page actions convert the new canonical 429 to upstream502 and drop retry metadata. |

The [remediation report](task-review-remediation.md) records source/assertion closure for the eight closed findings. No blocker was discovered solely by this validator; specialist proposals have a separate independent source check. REQ-020 is maintainer-added approved scope, not a newly discovered defect.

## Retained findings

### FIND-003-carryover-7 — INCORRECT: bundled BFF still loses the real visitor source

**Authority:** REQ-014 trusted source and existing password admission; REQ-020 extends the same source to sign-in starts. Security posture trusted-ingress section and deployment authority require this in the supported official deployment. This retains original -7 rather than assigning a new number.

**Locations:** `docker/official/extras/entrypoint.sh:30–34`; `docker/official/extras/nginx/nginx.conf.template:73–76`; UI `src/lib/server/upstream.ts:59–61`; actual consumer `auth/browser-sessions.ts:54`; test `identity_ui_e2e.rs:177–201`.

**Starting conditions / trigger:** Run the official image with its bundled nginx/Node UI and configure Rust's trusted loopback ingress plus the common client-address header. Two visitors submit the existing password or SSO/social login page actions. No extra adapter-specific environment setting has been supplied; the deployment documentation names only Rust/common trusted-ingress settings.

**Expected:** The trusted visitor address crosses nginx → Node → Rust, so distinct visitors have distinct source windows and forged browser headers cannot select them.

**Actual and independently traced proof:** nginx supplies forwarding headers but reaches Node on loopback. The entrypoint sets `PROTOCOL_HEADER` and `HOST_HEADER`, not `ADDRESS_HEADER`. Installed adapter-node's `files/handler.js:20,113–150` defaults that setting to empty and returns the socket peer without it. New `stateSource` consequently overwrites the configured Rust header with loopback. Without the common header setting it adds no source instead. The added BFF journey explicitly supplies `ADDRESS_HEADER=x-forwarded-for` and `XFF_DEPTH=1`, so its green result does not cover bundled startup. Direct `/auth` nginx requests bypass Node; the supported failing consumer is page actions.

**Consequence:** Visitors share a proxy source or skip optional password-source admission; the new shared sign-in-start window can block unrelated visitors after one visitor's flood. No claim is made that the correctly configured standalone BFF fails.

**Smallest correction:** Complete source derivation at existing nginx/Node startup and its deployment guidance, preserving Rust as the only admission owner. Use an address established by trusted deployment ingress, never an arbitrary browser-supplied header. Align the existing production BFF proof with the supported startup configuration and assert distinct visitor windows plus forged-header refusal. No new proxy, limiter or deployment matrix.

### FIND-003-carryover-10 — VIOLATION: remote wrapping holds a platform transaction

**Authority:** Authorization REQ-019 uses ordinary shared stored-secret semantics. SPEC-stored-secret-encryption REQ-008 explicitly requires encryption preparation outside database locks/transactions waiting on remote key calls. This is a mandatory lifecycle boundary, not a latency preference.

**Locations:** `wyrd-auth/src/social.rs:226–233` provider seal and `:669–675` challenge seal; `wyrd-auth/src/mail.rs:146–147` SMTP seal. Callers `wyrd-server/src/components/platform/social.rs:196–206,382–392,462–470`; transaction owner `wyrd-auth/src/platform_authz.rs:155–163,211–215`.

**Starting conditions / trigger:** An authorized operator writes a provider client, SMTP password or challenge secret using a supported remote KMS backend. The wrap request waits on the remote service.

**Expected:** Stage the permission decision, prepare the complete envelope with no held database transaction, then perform its short authorized write. Encryption failure preserves committed configuration.

**Actual / source validation:** Each route awaits `authorize`, which returns a real `TenantConn` from `begin_platform_audited`; it passes that connection into the auth/mail writer, which awaits `WyrdKms::seal` before route commit. The call was local before the port, so this is a new remote wait inside that transaction. The independent check followed all three writers and the actual authorization constructor rather than inferring transaction lifetime from the variable name.

**Consequence:** Remote key latency holds checked-out platform database connections throughout the call, violating the explicit safe-commit contract. No measured outage or held row lock is alleged.

**Smallest correction:** Reuse the existing `PlatformAuthorization::decide` / `begin` and platform OIDC configuration pattern: authorize without holding the transaction, prepare in the same auth/mail owner, begin for the complete existing write. Preserve one decision audit, validation, challenge-secret retention, redaction, rollback and existing concurrency semantics. One focused existing remote-key/Postgres boundary proof plus structural trace across the three equivalent writers suffices; no new framework or load test.

### FIND-003-carryover-11 — REGRESSION: sign-in page throttles become 502 without retry guidance

**Authority:** REQ-020/AC-015 mandates browser sign-in refusal429 with canonical `WYRD_AUTH_429_SIGN_IN_ATTEMPTS` and equal whole-second retry metadata. Existing UI authority requires stable safe problem projection and meaningful distinct refusal/unavailable states. This affects already shipped company/social sign-in page actions, not the deferred acquisition pages.

**Locations:** UI `src/lib/server/problem.ts:25–35,47–59`; Rust UI example generator `wyrd-spec/examples/gen_schemas.rs:386–483`; `auth/browser-sessions.ts:205–218`; tenant login `+page.server.ts:72–99` SSO/social catches.

**Starting conditions / trigger:** A source has exhausted the new 300/hour shared sign-in window. Its visitor submits an existing SSO or social login page action.

**Expected:** The page action preserves canonical admission refusal and usable retry metadata; it sets no login cookie or session and does not claim the backend is unavailable.

**Actual / independent source proof:** `BrowserSessions.begin` forwards the genuine Rust429 to `refuse` → `upstreamProblem`. Generated safe examples contain password attempts but no sign-in attempts; `safeProblem` returns internal for the new code. The fallback status map has no429, so the result becomes the upstream502 example. The page catches return502, and unlike the password action they do not preserve a retry header. Adding only the example would still drop the positive retry detail because `safeProblem` preserves that detail only for `passwordAttempts`. The specialist also identifies the changed `MethodUnavailable` catalog variant at this same generated-projection owner: absent its known example, status403 falls back to generic RBAC denial. The independent trace confirms that bounded adjacent projection loss; it is not another blocker or a general status-code audit.

**Consequence:** A normal enforced admission window appears as an upstream failure, with no indication when the user may retry. Direct `/auth/browser/login` is correct; the defect is its existing BFF/page consumer introduced by the new failure outcome.

**Smallest correction:** Extend the existing generated-safe-problem mechanism for the affected sign-in and MethodUnavailable catalog variants, retain sign-in retry details, and preserve the refusal in affected existing SSO/social actions. Reuse the password action's safe retry behavior; do not accept arbitrary upstream diagnostics or hand-edit generated files. One focused page/BFF throttle assertion can establish exact status/code/wait and no cookie; preserve existing direct backend coverage.

## Proof limits

Fresh root execution passes six exact signup/cap/keyless/sign-in/challenge journeys, two derivation tests and `ui:check`; [verification.md](verification.md) records commands and receipts. Those tests do not exercise the three retained paths. Existing unchanged tests/results carry only where source/dependencies and asserted behavior remain applicable. No broad parent completion, live cloud/provider run, benchmark or globally clean cumulative whitespace check is claimed. TDD violations remain separate in the verdict.
