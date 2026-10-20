# Independent findings validation

Subject: `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`, base `8651b96fd0c049ab5e63b0606c98326af1fc8848`, candidate `843484f6cceba1f48139349f66794966221c5d37`. Approved specification: `changes/active/wyrd-ui-main-integration/spec.md`, revision 9. Original tasks: TASK-001-distribution-login.md and TASK-002-remove-old-ui.md. First implementation review; no prior FIND IDs or remediation closure to preserve.

Recommendation: **TASK-001 SPEC_REVISION_REQUIRED** because its security prerequisite contradicts the inspected baseline; two bounded findings are independently retained below. **TASK-002 BLOCKED for required human acceptance evidence**, with an explicitly validated empty implementation-finding ledger. Combined acceptance cannot be PASS. Missing deployment runs are not recast as proven runtime bugs.

## Inputs and independent inspection

Read subject.md, both implementation reviews, standards-review.md, maintainer-review.md, system-review.md, domain-review-security.md, followup-review.md and claim-comparison.md. Applied the task-review and Ponytail skills, AGENTS.md, agent rules, spec-driven development and maintainer authorities, approved task obligations, relevant design/doctrine, security and deployment boundaries. No CodeGraph index exists. Reviewed the cumulative changed-file/deletion inventory and product diffs, not just the latest commit. Planning/reference additions are approved context, not shipped machinery. Unrelated working-tree trace edits are excluded.

Independent source inspection covered the entire installer owner, installation dispatch, full DevServer lifecycle and helpers, bundle/release/image producers, current adapter-node layout and package metadata, installer fixture and failure assertions, changed native/Python process proof, public auth hook/forwarder, full BrowserSessions/BrowserSession bodies, real production callers and shared Rust client-identification consumers, both identity-driver topology changes, startup common/dev/prod owners, nginx/entrypoint, and retained/deleted UI paths and regression tests. Baseline BrowserSessions and unchanged Rust auth/security authorities were checked separately from candidate changes. The generated local UI build was used only to understand the adapter's concrete shape; it is not an immutable-release acceptance result.

The existing build script, ServerInstaller, DevServer, BrowserSessions, BrowserSession, Rust RequestOrigin, image entrypoint and identity-fixture owners cover the workflows. No parallel build, installer, process manager, OAuth client implementation, key store or validation framework is warranted.

## Disposition of every discovery claim

| Discovery source IDs / claim | Disposition | Independent resolution |
|---|---|---|
| BEH-001, INV-001, MR-001, cache portion of SYS-001 | CONFIRMED, consolidated into FIND-TASK-001-1 | `install` bypasses extraction whenever only the server file exists; `installed` exposes the same incomplete cache; the sole activation writer then switches current. Native refusal cannot restore current or repair the advertised reinstall path. |
| INV-002, marker-only portion of SYS-001 | REVISED, consolidated into FIND-TASK-001-1 | The signed positive fixture is definitely not a BFF, and an actual entry with omitted generated support files still passes. Retain package eligibility failure. Reject the proposal to require a Node subprocess during installation: REQ-001 places system Node checks before launch, and Scenario 1 explicitly places actual Node validation in its owning runtime proof. Use concrete payload completeness plus the existing source-free runtime proof, without a general JavaScript/import-graph verifier or new install prerequisite. |
| SEC-001 | REVISED as FIND-TASK-001-2 | There is no executable federated-login proof through the official nginx topology. This is a missing required verification path, not proof that container SSO is broken. Running today's API-key startup probe cannot close it. Reuse existing fixture/driver and deployed ingress; do not duplicate the entire identity suite. |
| Security discrepancy / follow-up RESOLVED | CONFIRMED as authority/prerequisite conflict, no invented regression FIND ID | Both security properties are missing in unchanged main. REQ-006 and stronger design/security authority explicitly require them. Establishing secret/key lifecycle across Rust/BFF/native/container/replicas is not a bounded forwarding/removal correction. Route prerequisite/scope resolution through spec authority. |
| Standards report: no material findings | Validated within changed surfaces | No new SQL capability, binding, durable schema, dependency ownership or missing Rust documentation defect established. The task-ID test title and packet whitespace have no blocking consequence under this review's threshold. No style-only remediation. |
| TASK-002: no implementation defect | Validated empty ledger | Prescribed old owner graph is deleted, live consumers are closed, retained pages use plain markup and production auth/session authority is preserved. Human acceptance remains absent. |
| Missing native/image/visual execution | Acceptance limits, not extra FIND IDs | Required source-free native artifacts/digests, Linux native and selected official image runs, nginx identity and human checkpoints remain unverified. A static matrix does not prove them. |

No discovery finding is silently discarded because another reviewer omitted it. The focused follow-up was necessary: “existing behavior retained” and “confidential/encrypted protections pass” are different assertions. Its source resolution is independently corroborated below; no further discovery pass is needed.

## Final deduplicated finding ledger

### FIND-TASK-001-1 — REVISED — INCORRECT: installation eligibility admits incomplete UI across fresh and cached paths

Discovery IDs: BEH-001, INV-001, INV-002, MR-001, SYS-001. Violated obligations: REQ-002, TASK-001 Scenario 1, AC-001 and directly affected consumer closure.

Locations: `crates/wyrd/wyrd-cli/src/server/install.rs:190–197` (`installed`), `:228–249` (`install`), `:436–455` (`extract`); fixture `crates/wyrd/wyrd-cli/tests/server_install.rs:50–58`. Observable refusal consumer: `dev.rs:228–252` (`check_ui`).

**Producer → authority → consumer.** The baseline installer legitimately produced versions with only `wyrd-server` and an empty UI directory. The candidate's `install` still treats any cached `versions/<latest>/wyrd-server` as sufficient to skip `download_verified` and `extract`, then calls `activate` unconditionally. `activate` is the sole current-link writer. The Install dispatch reports success. `installed` also checks only the binary; DevArgs selects that result and `check_ui` refuses the absent UI entry while recommending another install. Reinstall selects the same cache and repeats successful activation. A healthy current A plus newer cached binary-only B therefore switches to unusable B, contrary to preservation. This is reachable through normal CLI commands, not an unused constructor or hostile source edit.

The fresh path has the same incomplete eligibility definition. `extract` checks only three regular-file names and one directory. `bundle(version, true)` in the changed signed fixture emits an empty entry, `{}` manifest and empty dependencies. `signed_release` authenticates those exact bytes. `install` accepts them and publishes current even though Node would exit with no listener. A real entry with its generated server chunks omitted also passes this predicate. Authenticating malformed package bytes does not make the BFF present. The official producer's handler import is useful but does not make the installer's explicit malformed-payload obligation disappear.

**Sibling/caller closure.** Public install dispatch and DevArgs automatic installation both call this ServerInstaller. `installed` has the DevArgs and installer-test consumers; `extract` has one installation caller; `activate` has one writer. Bundle and image producers use the same staged UI package. Runtime launch remains DevServer's responsibility. No other existing eligibility helper closes the gap. The full installer, dispatch and lifecycle bodies were read before selecting this boundary.

**Smallest correction.** Keep the existing installer-owned payload eligibility rule and make it govern freshly extracted candidates, cached candidates before activation and installed lookup/refusal consistently. Validate the concrete adapter-node production package rather than accepting empty sentinels: meaningful module package metadata, generated entry/supporting build payload and production dependency contents. Select ordinary static package/layout checks for this boundary; keep actual Node compatibility/load/start validation in the existing build/native runtime proof. Do not add an install-time Node requirement, generic import resolver, new package manifest/protocol or another lifecycle owner. This correction is limited to incomplete package cases; it is not a claim that a static predicate proves arbitrary signed JavaScript correct.

For invalid cached content, select an explicit unverified refusal before switching current, preserving both active/current data and the incomplete bytes for diagnosis. Give truthful recovery identifying that the named incomplete cached version must be removed/replaced before the existing signed install can be retried; do not report a successful no-op reinstall or silently delete an active version. Healthy complete cache reuse remains supported. The installer owns the pre-activation guard because activation commits its installation invariant. DevServer retains its launch prerequisite check; do not make downstream readiness or rollback the substitute for eligibility.

**Focused closure proof.** Extend the existing installer failure-preservation fixture to cover healthy current A with cached incomplete B (missing entry and dependencies), complete healthy cache reuse, the currently signed empty-marker payload as rejection, and an otherwise realistic adapter package missing required generated support content. Invalid candidates must preserve current and stable refusal, and not leave a newly runnable partial version. Cover an already-active incomplete cache's useful refusal/recovery rather than success followed by the same startup error. Use a positive package fixture matching the actual supported layout; the existing source-free native package smoke must separately load/start the accepted extracted production payload with system Node and obtain HTTP readiness without checkout dependencies. No generic new harness or full identity suite is required.

Existing exact focused installer command:

```bash
mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=server_install::server_install_preserves_previous_on_failure)'
```

If the healthy-cache assertion remains in the existing selection scenario, its exact command is:

```bash
mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=server_install::server_install_selects_latest_stable)'
```

The source-free runtime smoke remains a separate required runtime evidence class; these fixture tests do not certify it. Preserve signature/digest, compatibility, private staging, cancellation and atomic current publication. Narrow Rust formatting/lint verification applies to the actual remediation write set, without a broad journey aggregate.

### FIND-TASK-001-2 — REVISED — MISSING: no federated-login proof exercises official container ingress

Discovery ID: SEC-001. Violated obligations: REQ-006, AC-004, TASK-001 Scenarios 3/4.

Locations: `scripts/server/startup-common.sh:56–67`; `crates/wyrd/wyrd-server/tests/integration/identity_ui_e2e.rs:112–145` and `:485–490`; official ingress configuration in `docker/official/extras/entrypoint.sh:31–35` and nginx template's Rust/BFF route split.

**Producer → consumer and reachability.** The modified production identity fixture sets `ORIGIN` on each native BFF and the Vitest browser/gateway reaches `/auth/*` through those BFFs. The official entrypoint instead derives the BFF origin from nginx forwarding headers, while nginx routes `/auth/*` directly to Rust. `ui_sign_in` visits `/`, `/auth/device` and API-key recovery, then the signed-in page. It never initiates an IdP authorization, consumes a provider callback, refreshes an SSO session or revokes it. Neither startup caller adds those steps. Thus even a future green run of both existing startup scripts leaves the required official-container provider path unexercised. Existing native identity results cannot establish a different ingress/origin path.

**Consequence and limits.** A required deployment acceptance condition has no executable proof; callback/origin/session regressions in that deployment could pass current checks. No concrete container SSO failure or new token-validation flaw is asserted. This is distinct from tests that exist but could not run on this Mac.

**Smallest correction.** Extend the existing identity fixture/production-BFF driver and existing image/startup proof owners to exercise one focused federated flow at the actual nginx public origin of the reviewed official application image. Reuse repository-managed provider/tenant setup; drive browser login, provider return, session renewal and logout, plus the existing relevant invalid-origin/state refusal at that deployed boundary. Keep native configured-ORIGIN proof and real container header-derived topology separate; do not substitute direct BFF requests, synthetic routing, a mock provider or another auth owner. No dependency, generalized deployment harness, routing redesign or duplicated full identity suite is needed.

**Focused closure proof.** Execute that focused container identity scenario with the existing managed identity setup on a supported Linux runner, preserving immutable image/source identity and reporting the actual exercised public origin. API-key recovery and device-page checks remain useful siblings but are not substitutes. Reuse the existing exact production journey selector for the added deployed-mode case when that owner carries it:

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --test integration --run-ignored=all --no-capture -E 'test(=identity_ui_e2e::production_ui_bff_journey)'
```

Run with its existing managed Postgres/Keycloak/Dex setup and the actual official-image invocation; a run exercising only today's native fixture cannot close this finding. If the existing startup owner instead carries the focused new case, document its exact case invocation rather than claiming an unchanged native selector covers it. Only the focused new deployment case and checks for touched proof files belong in remediation; selected full matrix runs remain change acceptance evidence.

## Independently resolved specification/prerequisite conflict

REQ-006 explicitly names a configured confidential OAuth client and encrypted HttpOnly cookies. `architecture/wyrd-design.md:660` and security posture `:237–242`, `:257–263` independently require client_secret_basic and encrypted cookies; these authorities are unchanged in this range. Candidate and baseline BrowserSessions.configuration use `client.None()`, with no client secret. Establish serializes `{tenant,kind,credential}` directly with JSON.stringify; sealed parses JSON. Secure/HttpOnly/SameSite attributes protect different properties and do not encrypt that value.

These paths are live: tenant SSO action → begin; callback → complete → establish; API-key recovery → exchange/establish; request hook → read/access; root switch/logout → switch/logout. BrowserSession only returns safe principal/tenant metadata to layouts and settings, which supports page-data separation but cannot establish cookie encryption. Shared Rust `identify_client` rejects every Authorization header. Its production consumers are token dispatch, device authorization via require_client and revocation via require_client; code/refresh grants, CLI device flow and API-key/workload exchange siblings therefore cannot be corrected by setting Basic auth only on the BFF. All relevant client-identification and caller bodies were inspected. No existing configured secret/cookie-encryption owner was found.

The follow-up's conflict is confirmed. This is inherited implementation drift, not a newly introduced regression and not another FIND ID on every retained route. However it cannot be certified as REQ-006 PASS. The task explicitly says to preserve existing authentication and add only trusted forwarding/routing, and excludes auth redesign. Creating client registration/authentication and deciding cross-process secret/encryption-key delivery, lifetime and replica ownership would exceed that bounded correction. Return **SPEC_REVISION_REQUIRED** to establish the missing security prerequisite and its approved scope/dependency under the stronger authorities. Do not silently weaken the design/security authority to public-client/plaintext-cookie behavior. Do not package a fresh implementation agent's choice of secret/key lifecycle as a ready remediation task.

## Reconciled per-task acceptance

| Obligation | Validated evidence/result |
|---|---|
| TASK-001 REQ-001 / AC-001 | Production bundle producer stages build/package/production dependencies and no Node executable. Actual signed, source-free native launch/digests on all three targets remain unverified. |
| REQ-002 | FAIL, FIND-TASK-001-1. Authenticity/staging preserved, but incomplete fresh and cached payloads can be activated. |
| REQ-003 / AC-002 | Existing DevServer owns both children and settles them. Mac first/repeat/partial/interrupt/UI-exit evidence is bounded to fixture setup; Linux native execution remains missing. Reject the invariant report's implication that no native child-death evidence exists: changed Python exit case kills the UI and checks both listeners, and supplied evidence says it passed. |
| REQ-004 / native AC-003 | Actual forwarding and fixed public origin are implemented, with preserved redirects/cookies/errors and supplied identity regression. No validated introduced code defect. Stronger secret/session guarantee remains the separate authority conflict. |
| REQ-005 / AC-004 | Selected Debian amd64/arm64 and Rocky amd64 output/routing/process changes coherent in source. Runtime matrix unverified; FIND-TASK-001-2 identifies the additional missing executable provider proof. |
| REQ-006 | SPEC_REVISION_REQUIRED for inherited confidential/encrypted prerequisite mismatch; native login regression does not resolve it. |
| AC-005 / INV-003–006 | Existing owners extended; no deferred domain/client/analytical integration, native Node bundle, prohibited output or parallel controller established. |
| REQ-020 / INV-007 / AC-010 | References/provenance preserved; runtime and human proof explicitly incomplete. No visual pass inferred. |
| TASK-002 REQ-032 removal | PASS in source: delete/keep inventory, mock settings/locals removal, relocated browser-safe types, plain tenant/principal/sign-out/settings pages and affected consumer closure. Independent search found no surviving removed mock or deleted shared-component imports in UI source/config. |
| TASK-002 UI/identity regressions and exclusions | Available check/build/unit and real identity evidence supports retained behavior. No material implementation finding survives validation. No replacement shared shell/domain client/package introduced. |
| TASK-002 human checkpoint | BLOCKED for final acceptance: native/container chooser/login/recovery/signed-in/settings and required state/mode/responsive human inspection are not recorded. No implementation defect inferred from this absence. |

TASK-001 bounded ledger: FIND-TASK-001-1, FIND-TASK-001-2. TASK-002 independently validated bounded ledger: **empty**. Prior-finding closure: N/A. Spec/prerequisite resolution is necessary before complete TASK-001 acceptance; bounded findings remain recorded for subsequent authorized correction. Combined verdict recommendation: **SPEC_REVISION_REQUIRED**; TASK-002 stays unaccepted pending its human evidence rather than inheriting a duplicate security or installer finding.

## Verification limits and immutable subject

This validation ran read-only source/diff/inventory commands, no runtime tests, builds, shared service changes or source mutations. Discovery agents' independently executed UI/forwarder/session/shell/format checks are retained as their evidence, not claimed as rerun here. Linux native and images, real signed source-free bundles/digests, nginx provider identity and human checkpoints remain unverified. Reports only were written. HEAD rechecked as `843484f6cceba1f48139349f66794966221c5d37`; unrelated trace edits remain untouched.
