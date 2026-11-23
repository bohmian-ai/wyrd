# Cross-tier and repository review

Reviewed immutable base `4355587f128a0ac1d05071be0a8a84ba53a52290` to candidate `b7206eafefbf7e981898eca3fc22a5d5ae76b815`, against SPEC-authorization-model revision 7 and TASK-003-carryover. This is a mid-task review: completed cycles 1–10 are assessed as implemented work; cycle 11 is an intentional RED and cycles 12–20 remain unimplemented. This report does not turn those scheduled cycles into newly discovered defects.

## Boundary coverage

- Read root/nested docs and TypeScript rules, task/spec, workflow, maintainer style, testing, Rust/error/agent-surface guides, and relevant human-authentication and deployment authority. Used CodeGraph before tracing source, then direct diff and targeted source where CodeGraph did not return the requested file.
- Traced changed wire contracts and exports in `wyrd-spec/src/auth/{browser,social,mod}.rs`, HTTP route registration and OpenAPI annotations, catalog/error mapper changes and generated TypeScript/docs error projections. Signup begin is a tagged union; completion owns optional local credentials/challenge; methods remain flat despite Rust grouping. No field-layout divergence was found in the changed BFF methods type.
- Traced the existing production login form → Svelte action → `BrowserSessions.password` → BFF `call` → Rust `RequestSource` → password-admission path. This reveals REPO-01 below. Origin forwarding does not forward a visitor address: the origin URL identifies the application host, and Rust source extraction reads only the TCP peer plus its configured client-address header.
- SDK/MCP methods/settings/signup projections and connected UI acquisition journeys remain absent as the task itself records. Cycles 15/16/20 own the outstanding UI and shipped-consumer work. They cannot be accepted now, but their planned absence is not a separate newly discovered implementation defect. Tenant retirement is referred to the lifecycle specialist.
- Existing owners are reused: shared password validator, `SocialSignIn`, `HumanOwner`/`TenantProvisioning`, browser-session opening, application mail, central error mapper and auth maintenance worker. The new request-source module is a coherent shared ingress capability used by signup and password admission. No material duplicate orchestration or unnecessary abstraction was established. `ipnet` replaces the workspace's unused `ipnetwork` declaration and is owned by the server; no dependency-tier violation was identified from this diff. Source shape and rustdoc were examined without raising preference-only findings.
- No defined verification, gateway, Oracle or Scribe caller-wait hot path is changed. Added/changed database waits belong to login/method reporting/signup/provisioning, and the purge is background work. Therefore this scope introduces no hot-path wait requiring a benchmark experiment; the onboarding spec expressly authorizes no such wait. No L1 benchmark is represented as run.

## Proposed findings

### REPO-01 — INCORRECT: production BFF loses the visitor source

**Authority:** REQ-014 “Trusted source”; TASK-003-carryover cycle 1; `architecture/wyrd-security-posture.md:508–514`. Password-login admission must use the same trusted client-source derivation as signup, with separate windows. Native deployment authority explicitly says the UI BFF forwards browser authentication requests (`architecture/operations/deployment-and-release.md:345–348`).

**Location:** `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/browser-sessions.ts:45–55,252–263`; caller `src/routes/t/[tenantKey]/login/+page.server.ts:37–50`; Rust consumer `crates/wyrd/wyrd-server/src/auth/source.rs:45–60,81–94` and changed `auth/browser.rs:671–685`.

**Starting state and trigger:** Two people use the existing production tenant password page through one Node BFF behind the configured ingress. Each submits password attempts from a different client address.

**Expected:** Their source windows are keyed by their respective trusted client addresses; forwarding headers supplied by an untrusted visitor cannot name another source.

**Actual and proof:** The action passes username/password, URL and cookies to `BrowserSessions.password`; no request source is passed. `call` constructs a fresh header map containing only Origin, forwarded host/protocol, selected cookie and content type. No client-address header can reach Rust through this path. With Node untrusted, Rust keys every visitor by Node's TCP address; with Node inside the configured ingress range, its missing address header yields `None`, and password admission's optional source window is skipped. The cycle-1 journey drives the Rust router directly with synthetic `ConnectInfo` and client-address headers, bypassing this production seam. Origin and forwarded host/protocol cannot repair that loss.

**Consequence:** A shared BFF collapses independent visitors into one password source budget, or omits that source budget when the BFF is trusted. The completed source-admission claim is not delivered through the existing browser path.

**Smallest correction:** Preserve the trusted visitor source through the existing BFF auth transport and its login/change callers, using the deployment's ingress trust boundary rather than forwarding arbitrary browser headers. Reuse Rust's one source derivation and existing separate admission windows; do not add another admission engine. Prove two real UI visitors retain separate source windows and that forged untrusted forwarding is ignored through this seam. This corrects the already-existing password UI path, independently of the future signup page work.

### REPO-02 — EVIDENCE_GAP: required verification for the changed UI is unavailable

**Authority:** `.agents/skills/wyrd-ui/SKILL.md`, Verification: “The minimum for any change under `wyrd-ui/` is `mise run ui:check`”; root AGENTS targeted affected lanes. The skill is applicable to the UI files actually changed, irrespective of later page delivery.

**Location:** changed `wyrd-ui/src/lib/server/auth/browser-sessions.ts:11–20` and tenant `signup/+page@.svelte:27`; evidence record `TASK-003-carryover.md:230–250,282–343,640–682`.

**Claim and missing proof:** The supplied verification describes Rust-focused lanes, Clippy, codegen, docs and isolation. The task records the two UI edits but no execution result for `ui:check`; its only `ui:check` mention is the later integrated checklist. No linked external execution record supplies the missing result. This report does not assert the command was never run or that the UI is broken.

**Consequence:** The required minimum type/test verification of the currently modified BFF contract and signup page is not established. Rust and generated-output checks do not execute that lane.

**Smallest correction:** Run the existing `mise run ui:check` against the candidate containing these changes and retain its result. Correct only failures actually demonstrated by that check. No new test or production edit is demanded by this evidence gap alone.

### REPO-03 — INCORRECT: disabled methods use the invalid-credential status

**Authority:** SPEC revision 7, “Revision-6 HTTP contracts”, `spec.md:731–735`, explicitly requires disabled methods to return `403`, distinct from invalid/expired proof (`400`) and unavailable dependencies (`503`); REQ-011 requires current methods to be enforced.

**Location:** `crates/wyrd/wyrd-server/src/auth/browser.rs:798–815`; signup owner `crates/wyrd/wyrd-auth/src/social.rs:1885–1916,2013–2015`; regression assertion `crates/wyrd/wyrd-server/tests/integration/platform_admin_e2e/methods.rs:245–255` and the cycle-5 evidence.

**Starting state and trigger:** A valid same-origin request submits password login/enrollment/change/reset after the operator disables password or the tenant requires company SSO; alternatively a public signup is submitted after its method is disabled.

**Expected:** A catalog-backed `403` identifies the disabled method, with no session or creation side effects.

**Actual and evidence:** The new `require_password` and provider-start guards call `method_unavailable`, which constructs `WyrdError::InvalidToken`, hence `401 WYRD_AUTH_401_INVALID_TOKEN`. Signup's `require_signup_provider` similarly calls `signup_unavailable` → `invalid_token`. The methods journey and its evidence explicitly expect `401`, so green execution proves the wrong wire contract. An expired credential is not the cause of these configured-policy refusals.

**Consequence:** HTTP consumers cannot use the approved status distinction to tell an unavailable sign-in method from a credential needing renewal. The implementation and its OpenAPI descriptions diverge from the settled response contract.

**Smallest correction:** Return a suitable catalog-backed `403` from the existing disabled-method guard owners, retaining current enforcement and side-effect prohibitions. Update affected response annotations and journey expectations, and regenerate any changed catalog projections. Keep genuine credential-invalid and dependency-unavailable responses separate; do not remap all `InvalidToken` failures globally.

## Other reconciliations and limits

TDD history includes the explicit cycle-1 violation (`task:133–134`), cycle-7 step 4b and cycle-9 Turnstile unit-tier violations. A later mutation check cannot manufacture historical RED. These process facts do not require historical replay or production churn. Cycle 11's observed missing-human-admin RED is expected unfinished work, not an unexpected failure diagnosis.

`git diff --check 4355587f12...b7206eafef` passed during this review. No Cargo, UI, live-provider or runtime journey commands were executed by this specialist, to avoid overlapping shared builds and to keep the immutable subject unchanged. Recorded green evidence at `4dd3b6b24a` is available implementation evidence, but is not a fresh run at the candidate; the candidate adds the intentional setup RED. No claim of full task acceptance is made.
