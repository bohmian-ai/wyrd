# Cycles 12–16 primary behavior discovery

## Subject and coverage

Repository `/home/thorrester/Documents/GitHub/wyrd`; base `63ee31a0205c9b59a11877d5cf571d7bd0632cd6`; candidate `dfb3599d5fbfb3d863ecb68382257cb30abfd79b`; approved SPEC-authorization-model revision 11; original `tasks/TASK-003-carryover.md`. HEAD still equals candidate at the end of source inspection. Read the actual three-dot diff. The verification-workflow-ui document changes introduced by `05fc05138` are excluded. Earlier carryover verdicts accept earlier bounded work, not cycles 12–16. Cycles 17–18 are deferred and 19–20 belong to TASK-004.

This is discovery, not independent validation or the final verdict. Proposed IDs below are report-local. Retain stable FIND numbers only after validation.

Read root AGENTS, the task-review skill, spec-driven development (authority, ambiguity, TDD and review), maintainer style, TESTING, agent-docs index/agent surfaces, applicable UI skill, docs AGENTS and TypeScript SDK AGENTS. Relevant authority is REQ-011–014/017/018, AC-004/005/007–013, persistence purposes, security posture's social admission/mail and browser boundaries. The domain reviewer owns cycles 12–13 security/persistence; the invariant reviewer owns cycle 14 session lifecycle; system review owns official routing/deployment. Their rows below await their reports rather than asserting duplicated specialist coverage.

Navigation map, established through CodeGraph before source search: `SocialSignIn` owns contact/discovery/proof behavior; `PlatformLogin`/`PlatformSessions` own platform authentication; `TenantProvisioning::claim_for_signup` owns reserved-name refusal; Rust browser and platform handlers expose typed contracts; UI `browser-sessions.ts`/`platform-sessions.ts` forward those contracts; onboarding `Proof.svelte`, `onboarding.ts`, signup/workspace and workspaces pages compose browser flows. `identity_ui_e2e` hosts the production BFF/browser/Mailpit journeys; `platform_admin_e2e` hosts Rust HTTP journeys; UI `ui-pages.integration.test.ts` owns console journeys and the existing controlled Google invitation/return fixture.

## Acceptance matrix for reconciliation

Statuses describe available implementation/assertions and recorded execution; they are not fresh execution claims. No tests were run by this reviewer, to avoid overlapping shared Cargo/UI builds.

| Approved promise / existing scenario | Implementation and inspected proof | Status |
| --- | --- | --- |
| REQ-017, AC-009/010; C12 authenticated recovery contact, replacement and stale-revision refusal | `social/contacts.rs`, browser contact handlers and `platform_admin_e2e/contacts.rs`; task C12 records mail-derived proofs, same-browser/session checks, replacement, notification and preserved password. Independent domain coverage required. | UNRESOLVED pending specialist |
| REQ-011/017; C12 bounded discovery, live revalidation, purpose, RLS and maintenance | `social/discovery.rs`, auth/platform SQL owners and contact journey/SQL assertions. C12 S6 explicitly lacks pre-proof envelope expiry proof. | UNRESOLVED; B-3 |
| REQ-002/013/017, AC-010; C13 private reset, stale/foreign/offline codes | reset request/redemption owners and `platform_admin_e2e/resets.rs`; task records real-mail, stale-revision, privacy and transient-send execution. Independent domain coverage required. | UNRESOLVED pending specialist |
| REQ-018, AC-013; C14 isolated password/OIDC sessions, replica deadlines/revocation and recent same-operator proof | platform login/session/extractor/console Rust owners, console HTTP tests and C14 execution records. Independent invariant coverage required. | UNRESOLVED pending specialist |
| REQ-011/014; C15 mock approval and reserved signup slugs | Task records maintainer approval of mock and seven readings before production pages. `TenantProvisioning::claim_for_signup` refuses the exact 18 addendum slugs before claim; operator path unaffected. Signup test loops through all reserved names; real browser retries `platform` with its original mailed proof. | PASS except quota behavior below |
| REQ-011; C15/16 narrow card and creation at cap | `signup/workspace/+page.svelte` maps cap refusal but immediately re-enables the same creation form. The mock repeats this omission; spec still expressly requires disable/replace. | FAIL; B-1 |
| REQ-011/014/017; C16 root/private defaults, email signup, proof method and retry | Root and signup loads read Rust methods; browser POST consumes mailed fragment through `Proof`; workspace asks local credentials only for `method=email`. Onboarding journey drives actual email proof, reserved-slug refusal, one-mail retry, signed-in administrator and subsequent password sign-in. Proof-method HTTP test checks email/Google/GitHub, absence/forgery/expiry and repeated non-consuming read. | PASS for these cases |
| REQ-011–014; C16 provider signup and GitHub contact branch / provider failure and retry | UI has provider-start buttons and provider completion/contact branches. Browser onboarding host exercises email signup only. Existing `withGoogle` journey in `ui-pages.integration.test.ts` exercises invitation and returning login, not workspace signup. Rust signup tests cannot prove new page integration. | UNRESOLVED; B-2 |
| REQ-014; C16 configured Turnstile completion | Root methods publishes public site key, UI loads/render/removes widget and sends/reset tokens. Component test stubs the entire Turnstile global and `/auth/*` responses; separate HTTP test exercises Siteverify. No connected production browser→Rust→controlled Siteverify proof exists. | UNRESOLVED; B-2 |
| REQ-017; C16 discovery page and pasted proof | Onboarding browser journey reads actual Mailpit proof, asserts exact one workspace with same-origin login URL, clears fragment, and separately pastes code from a second requested message/browser. Pagination UI consumes only server cursor; backend negative paths remain domain-owned. | PASS for delivered page flow |
| REQ-002/013/017; C16 reset, recovery contact, invitation/device | Onboarding browser tests independently create their workspace; reset consumes mailed code and confirms new password login; recovery flows from Account through actual mail; signed-out proof has a separate case; invitation enrolls teammate then device token identifies teammate. `PasswordInput` supports paste/show and local policy helper counts NFC codepoints. | PASS for delivered flows |
| REQ-018; C16 console sign-in, re-authentication, provider/relay/signup views | Six `withConsole` browser tests assert production BFF→Rust behavior: password sign-in/cookie isolation, canceled/same-operator recent proof, SSO principal mismatch, provider revision tests/races, omitted relay password/removal and signup settings secret omission/keep. Records report 6/6 after native test addon rebuild. Inspectable assertions exist rather than names alone. | PASS for these bounded flows; underlying lifecycle pending specialist |
| REQ-018/AC-013; C16 official host routing | Nginx stub proof is recorded; official startup lane added but did not reach container startup. Independent deployment coverage determines adequacy. | UNRESOLVED pending specialist |
| C16 docs handoff and changed public relay contract | Setup/password stdin/private defaults/recovery docs were extended, but relay update text omits kept-password and username-change refusal; docs gate explicitly failed. | FAIL; B-4 |
| C16 mock alignment / approved later rulings | Task records four-size/mode sweep, final deviations, Show on all human password fields and styled device page, with relevant assertions/screenshots reported. No fresh visual rerun; quota omission is not a justified visual deviation. | PASS for recorded alignment except B-1 |

Browser proof pages receive `private, no-store` in `hooks.server.ts`; `Proof.svelte` clears fragment before POST and purpose-specific callbacks select the Rust endpoint. UI does not persist proof/password/provider secrets or run token exchange. Added public browser contracts live in wyrd-spec and Rust handlers; no adjacent browser-auth SDK facade existed, so absent new SDK convenience methods alone is not a defect. No new material abstraction/maintainability defect found in this assigned scope. These onboarding/control-plane pages do not add verification/gateway/Oracle/Scribe hot-path waits; no benchmark demand is created here.

## Proposed blocking findings

### B-1 — INCORRECT: workspace cap leaves creation usable

- Authority: SPEC REQ-011 (`spec.md:329–330`), repeated C15 prerequisite and C16 quota/refusal flow.
- Location: `crates/wyrd/wyrd-server/wyrd-ui/src/routes/signup/workspace/+page.svelte:40`, `:98`, `:128`, `:167–205`; mock `design/mock/src/Workspace.svelte:43`.
- Starting state/trigger: a visitor has a live proven signup, completes the form, and Rust refuses with `WYRD_AUTH_403_SIGNUP_WORKSPACE_LIMIT`.
- Expected: disable or replace creation form, showing refusal and recovery guidance.
- Actual: `completionRefusal` selects `PROBLEMS.limit`; `ended` excludes it and `phase` returns to `idle`. Form fields and Create workspace remain active, and another submit clears the refusal and repeats a refused creation. Mock currently has the same defect and cannot override the approved behavioral requirement.
- Evidence: source control flow above; no UI cap assertion exists. Backend cap enforcement is not claimed broken.
- Consequence: the refused user receives another unusable creation action rather than the required recovery path.
- Smallest correction: treat the cap at the existing workspace page's terminal/refusal boundary; preserve server refusal and offer existing-workspace sign-in/operator guidance. Align mock's cap state and add a focused production browser assertion against a real server cap response. Verify the form cannot submit while capped and no extra tenant is created. No admission/provisioning redesign.

### B-2 — EVIDENCE_GAP: new provider/challenge signup pages lack connected journey proof

- Authority: C16 proof and retained provider-creation journey (`TASK-003-carryover.md:1252–1265`), REQ-011–014/AC-005; AGENTS testing tier rule and TESTING's no-unit-substitution rule.
- Location: `wyrd-ui/src/lib/server/auth/onboarding.integration.test.ts:1–404`, `ui-pages.integration.test.ts:3498–3563`, `ui-pages.test.ts:1323–1382`, new signup/workspace pages.
- Starting state/trigger: signup is usable through a configured Google/GitHub acquisition method, or browser completion requires configured Turnstile; visitor uses the shipped production pages.
- Expected proof: production browser/BFF/Rust/Postgres plus controlled provider/mail/challenge dependencies demonstrate page outputs are consumed by the actual downstream owner, including the retained failure/retry branches.
- Missing proof: onboarding host has no provider or Siteverify fixture and tests only email acquisition; existing controlled Google browser journey proves invitation/return only. Turnstile component test replaces both widget and Rust endpoint, while the separate Rust Siteverify test starts downstream of the browser. Thus neither proves the new provider/contact page or widget token reaches and satisfies actual signup completion. Task Part B/challenge evidence explicitly acknowledges these disconnections.
- Consequence: browser-user acquisition variants assigned to C16 remain unaccepted even with passing HTTP and component tests.
- Smallest correction: extend the existing production onboarding/browser journey host with its existing supported provider/challenge configuration seams and controlled endpoints. Prove provider acquisition→method read→correct completion credentials→usable tenant; GitHub lacking contact→actual mail proof→same provider attempt; recoverable refusals/retry already required by C16; configured challenge token→real Rust Siteverify decision→refusal/fresh-token retry/success without duplicate creation. Reuse existing backend scenario assertions where they already prove backend-only race/failure behavior; do not invent hosted-provider credential requirements or replay every backend case through the UI. No requirement to use a live third-party account is inferred.
- Runner: focused cases through `mise run test:identity:ui` / its existing environment-owning suite; `ui:check` complements but cannot replace them.

### B-3 — EVIDENCE_GAP: pre-proof email expiry remains unproven

- Authority: REQ-017's fifteen-minute email proof deadline, security posture email envelope deadline, C12 expired-proof refusal.
- Location: task C12 S6 `TASK-003-carryover.md:924`; `wyrd-auth/src/social.rs:2157–2163` (`require_live`) and signup/discovery/contact envelope consumers.
- Starting state/trigger: a browser retains its validly sealed envelope and actual mailed proof beyond the original fifteen-minute pre-proof deadline and POSTs verification.
- Expected: refusal with no newly proven signup/contact/discovery authority and no new read cookie/result.
- Missing proof: task explicitly excludes this case; aging SQL spent-proof/read rows proves a different stage. Source calls to `require_live` suggest enforcement but runtime deadline behavior has no executable assertion. No implementation error is alleged.
- Smallest correction: provide focused executable proof at the required owner boundary for the expired pre-proof envelope and its no-side-effects result, reusing the existing clock/envelope/test ownership. Do not demand a new production clock API or a persistence change. Domain reviewer may consolidate with its proof-gap finding.

### B-4 — MISSING / EVIDENCE_GAP: relay-update documentation and required docs gate unfinished

- Authority: C16 documentation handoff, approved reading 7 / Part A public update contract, docs AGENTS done-when gate.
- Location: `docs/src/content/docs/operate/add-people.md:222`; task `:1410`; changed generated `docs/public/llms-full.txt`.
- Starting state/trigger: operator has a stored authenticated relay and updates configuration following the public guide.
- Expected: guide explains password omission keeps the stored credential for the same username, changing username requires its password, and omitting username explicitly removes authentication; generated corpus agrees. Required changed-docs checks have passing evidence.
- Actual/missing proof: guide only says omit username/password for unauthenticated relay, omitting the new update behavior/refusal; task expressly says the relay docs line was left unwritten because generation failed. Current snippet still works, so this is a bounded missing contract explanation, not a claim that the example fails. No docs gate pass is available for the changed guides.
- Smallest correction: add the one bounded update-contract explanation at the existing relay guide owner, regenerate its text corpus through existing generator, and obtain `mise run docs:check` evidence. No new guide, docs refactor or production change required.

## Process and verification limits

TDD status is **VIOLATED**, independently of eventual behavioral acceptance. Preserve C12's first-contact race fix and S5 pagination/S6 purpose/read expiry/S7 reconciliation written before their tests; mutation checks strengthen assertions but do not supply historical RED. Also preserve C16 Part B recovery-email/verify-page scenario 5, explicitly recorded written before RED, and C16 Part C S3 callback/SSO/pop-up code written before its test. The later code-removal/restoration experiment cannot turn S3 into test-first history. C16 invitation/device test corrections without production changes are verification-only, not manufactured RED. Earlier task violations remain in earlier verdicts.

Task execution records and the user's handoff report platform HTTP 97/97, console 6/6, onboarding 2/2, ui/clippy/codegen/tenancy clean. These are inspected reported results, not fresh reviewer runs; root/specialist verification receipts may strengthen them. Uncaptured concurrent-build diagnosis for the old onboarding invitation flakes remains unproven rather than accepted as cause. Official startup OOM prevented that lane from reaching nginx; no runtime host-routing pass is claimed here. No live Cloudflare/Google/GitHub smoke, broad change-level suite, docs gate rerun, or benchmarks were executed by this reviewer.
