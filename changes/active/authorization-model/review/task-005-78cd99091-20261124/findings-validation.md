# TASK-005 independent findings validation

Subject: base `f7e0d67b2e007a1262fed34d45e5ebd2629e9bee`, candidate `78cd99091c81e5a7d706ac6f54a8222e62705e0a`. Authority: approved authorization-model revision 12, REQ-021 and AC-016, original TASK-005, applicable repository and architecture rules. All four selected discovery reports were read before reconciliation. This validator is separate from discovery and checked the proposed findings against candidate source and affected consumers. No production source, authority, or scenario expectation was changed.

## Dispositions

| Discovery proposal | Validation | Retained ID |
|---|---|---|
| B-002: immediate destructive company-SSO submission | CONFIRMED | FIND-TASK-005-1 |
| UI-01: modal background remains keyboard-active | CONFIRMED | FIND-TASK-005-2 |
| UI-02: four tables lack labeled focusable scroll regions | CONFIRMED | FIND-TASK-005-3 |
| B-001: no connected Secrets lifecycle journey | CONFIRMED | FIND-TASK-005-4 |
| B-003: no provider-name production-consumer proof | CONFIRMED | FIND-TASK-005-5 |
| D1: served-contract assertions lost in merge | CONFIRMED | FIND-TASK-005-6 |
| REPO-1: final required verification incomplete | REVISED: qualification gap only; no attributed timeout defect or historical assertion about whether missing checks ran | FIND-TASK-005-7 |

No new finding originated in validation. The twelve maintainer-approved differences remain accepted inputs. Source/capture limitations outside the retained findings are recorded in the verdict, not converted into optional remediation.

## Retained findings

### FIND-TASK-005-1 — INCORRECT: Deactivate and Remove company SSO bypass confirmation

**Authority:** REQ-021, page pattern: destructive actions confirm and sit in a Danger zone; TASK-005 scenarios 6 and 10; finalized design Page pattern.

**Location:** production UI `src/routes/t/[tenantKey]/settings/(admin)/sign-in/+page.svelte:261` and sibling `+page.server.ts` actions `deactivate`/`remove`.

**Starting conditions and trigger:** an authorized administrator has an Active company connection with company SSO optional and selects Deactivate or Remove.

**Expected / actual:** an impact confirmation must allow cancellation before mutation. Both controls are direct POST forms. The BFF immediately calls POST `/identity/oidc/active/deactivate` or DELETE `/identity/oidc/connections/{id}`. The policy switch's separate `confirm` dialog does not guard these forms. The existing owner retires/tombstones the connection and can wipe its secret; a normal first click can therefore end company sign-ins/sessions or remove configuration.

**Independent evidence:** inspected both form declarations and action handlers, the separate policy-confirmation branch, and production SSO tests which submit these actions directly. Static source establishes the missing confirmation; no runtime failure is claimed.

**Smallest correction:** add confirmation at the Sign-in presentation owner using the shared modal mechanism, with actual impact and write-free cancellation. Preserve server authority, proof, policy, revisions and existing lifecycle endpoints. Prove first click/cancel changes nothing and explicit confirmation applies the requested action through the real production BFF.

### FIND-TASK-005-2 — INCORRECT: shared modal overlays do not contain keyboard focus

**Authority:** REQ-021's drawer/presentation requirement; TASK-005 scenario 10 keyboard/focus behavior; UI accessibility rules and the existing built modal owner.

**Location:** production UI `src/lib/Overlay.svelte:25`, both drawer and dialog branches; consumers include invitation/account/create/edit drawers and destructive confirmations.

**Starting conditions and trigger:** open a normal administration overlay, then press Shift+Tab from its initial focus or Tab past its final control.

**Expected / actual:** keyboard interaction remains in the active modal until dismissal. Overlay only calls `node.focus()` on a `div`, declares `aria-modal=true`, and handles Escape globally. Its scrim is a focusable anchor, and no ancestor makes background content inert. Tabbing can reach the scrim and background controls while the modal remains open. The ARIA declaration does not implement modality. Keyboard users can leave pending forms or activate obscured page controls.

**Independent evidence:** complete Overlay and caller structure inspected; `packages/ui/src/lib/components/Drawer.svelte` already uses native `dialog.showModal()` for this property. Existing keyboard journeys verify section navigation/focus appearance, not modal containment.

**Smallest correction:** restore true modality once at the shared owner, reusing/adapting the native-dialog mechanism rather than fixing every caller or adding a dependency. Preserve mock geometry and URL-owned state. Test forward/backward tab containment, Escape/cancel without writes, and usable focus after dismissal through a focused production drawer/confirmation journey.

### FIND-TASK-005-3 — VIOLATION: four dense tables omit the labeled keyboard scroll region

**Authority:** production `brand/DESIGN.md:235` requires tables to retain widths inside a focusable, labeled horizontal scroll region; TASK-005 scenario 10 and UI accessibility rules.

**Location:** production routes `t/[tenantKey]/settings/(admin)/secrets/+page.svelte:72`, `platform/ui/workspaces/+page.svelte:19`, `platform/ui/admins/+page.svelte:13`, and `platform/ui/secrets/+page.svelte:53`.

**Starting conditions and trigger:** a keyboard/assistive-technology user reads a populated table at phone width and needs its overflowing columns, including the read-only platform-admin or maintenance-failure rows.

**Expected / actual:** the scrolling container is labeled and focusable. All four use a plain `div.tablewrap`. Shared CSS supplies horizontal overflow, nowrap headers and column padding; the Secrets inventory also has nowrap names/actions. The wrappers have no region semantics, accessible label or tabindex. Browser-specific automatic scrolling-container focus does not supply the explicit label or the portable repository contract. Data/actions extending beyond the viewport lack the required named keyboard region.

**Independent evidence:** inspected the four wrappers and shared `console.css:60`; People, Service accounts and Roles implement the required sibling pattern with `role=region`, `aria-label` and `tabindex=0`. Existing responsive coverage exercises those compliant siblings, not these four.

**Smallest correction:** apply that existing wrapper pattern to the four affected tables. A bounded phone-width check must find/focus the named populated regions and verify horizontal scrolling stays inside them without document overflow. No row, server workflow or visual redesign is required.

### FIND-TASK-005-4 — EVIDENCE_GAP: Secrets lifecycle is unproved through the new UI/BFF

**Authority:** REQ-021 Secrets inventory/lifecycle, AC-016, TASK-005 scenario 7, root AGENTS tier-1 journey requirement.

**Location:** production UI Secrets `+page.server.ts:170` onward and page form consumers; `src/lib/ui-pages.integration.test.ts:2986`; TASK-005 scenario 7 evidence.

**Starting conditions and trigger:** an admin manages real tenant-owned provider, Operator, company-SSO or trusted-issuer secrets using the newly exposed controls, including cancel, non-admin/cross-tenant direct requests and owner failure.

**Expected / actual proof:** production form → BFF → actual owner → reload must prove dispatch, mutation, redaction and refusal/no side effects. Candidate UI tests contain only a mocked loader composition test (`ui-pages.test.ts:1255`) and optional capture navigation to Secrets. The cited GitHub journey exercises Integrations, not these new actions. No connected Secrets add/replace/renew/revoke/refusal/cancel journey was located by path, action outcome or control searches. Owner-level lifecycle tests cannot prove new form payload/path dispatch.

**Consequence:** the required shipped administration flow lacks credible executable proof; this does not establish that any mapping is incorrect.

**Smallest correction:** extend the existing real production-page fixture with focused Secrets journeys. Assert each distinct supported owner dispatch/payload mapping, accepted reload, redacted inventory, write-free cancel, non-admin and foreign-tenant refusal, and safe failure. Group equivalent cases and reuse owner encryption/lifecycle tests; do not change production solely to satisfy an evidence finding.

### FIND-TASK-005-5 — EVIDENCE_GAP: provider display names are not proved in rendered identity consumers

**Authority:** REQ-021 name/email/fallback promise, AC-016, TASK-005 scenarios 2 and 4.

**Location:** TASK-005 scenarios 2/4 evidence; production `src/lib/ui-pages.integration.test.ts:2687` (Alice), `:3460` (Google), `:1215` (password account), and `src/lib/server/auth/production-auth.integration.test.ts:790` (SSO account).

**Starting conditions and trigger:** a social/company User without a username signs in with a provider display name, later signs in with an updated name, then reads the shell, own account and People.

**Expected / actual proof:** these actual consumers show the persisted/refreshed name with the approved fallback priority and own email second line without foreign-tenant disclosure. The task claims Alice proves stored provider names, but Alice joins with a password/username. Google fixture assignments have only `sub`/`email`; their assertions establish enrollment/continuity, not a displayed provider name. SSO account assertions check method and key restrictions. The only `display_name` use in the UI integration test selects a suspension target, not an identity assertion. Server tests credibly prove name parsing/storage/refresh, and source uses appropriate fallbacks, but the required predecessor-output-to-rendered-consumer connection is absent.

**Consequence:** the central social-identity correction lacks proof at the promised UI boundary; no faulty fallback expression is alleged.

**Smallest correction:** extend the controlled-provider production fixture and existing account/People journeys with distinguishable names/emails, a later successful refresh, supported fallback cases and tenant confinement. Verify the real shell/account/People output and record exact focused passing execution. Reuse server-level provider variants where behavior is equivalent; no new identity capability is required.

### FIND-TASK-005-6 — EVIDENCE_GAP: merge removed required served-contract assertions

**Authority:** REQ-021 read-projection table; TASK-005 scenario 3 served OpenAPI proof and scenario 6 preservation; root AGENTS prohibition on deleted/weakened test coverage.

**Location:** `crates/wyrd/wyrd-server/tests/integration/pg_openapi_contract.rs:389` and the missing `principal_read_projections_publish_their_contract`, formerly in `cc5cea627` immediately after tenant-login contract coverage.

**Starting conditions and trigger:** run the documented OpenAPI/principal qualification after merging PR 244.

**Expected / actual proof:** served schema assertions must retain principal profile/directory fields, required counts, authentication enum, Role fields, query filter and description refusal, plus predecessor login-policy route/schema checks. `git diff cc5cea627 78cd99091 -- .../pg_openapi_contract.rs` deletes the complete principal projection test and removes the login-policy GET/PUT entries, typed request assertion and safe view field assertion from the identity contract test. Candidate contains none of the principal field/count assertions. TASK-005 still cites the missing test as passed. The passing lane cannot establish assertions absent from its source; MCP catalog tests cover another projection.

**Consequence:** a required public-contract regression could escape claimed qualification. Actual served schema correctness is not disproved.

**Smallest correction:** restore the lost assertions at the existing served-document test owner while retaining PR 244's platform-cookie scheme/tests. Run both restored/affected tests by exact selectors in repository Postgres and correct evidence. Production changes are needed only if restored assertions expose a defect.

### FIND-TASK-005-7 — EVIDENCE_GAP: required integrated qualification remains incomplete

**Authority:** TASK-005 Verification and handoff explicitly requires `test:wyrd`, Rust lints and affected SDK type/N-API checks; root AGENTS forbids waiving a red gate based on its origin. Review skill preserves mandatory gates while using focused remediation checks.

**Location:** TASK-005 post-merge verification, lines 455–461 and 508–514, and supplied handoff.

**Starting conditions and trigger:** accepting the integrated candidate on its supplied final results.

**Expected / actual proof:** required gates have successful candidate-bound results. The recorded aggregate is 3032/3034, followed by a capacity-clock fix and an isolated pass of `pg_verification_routes::staging_refusal_recovers_the_same_enqueue_identity`; no later clean aggregate is recorded. The 504 remains explicitly open. Supplied results also lack Rust Clippy, TypeScript typecheck and N-API check receipts. Fresh repository-review checks close format/Python/TS-format/lint gaps, but not these remaining checks. A missing receipt is not proof a command never ran, and an isolated pass is not proof the load-dependent aggregate is green.

**Consequence:** the explicit task qualification cannot be marked passed. No claim is made that TASK-005 caused the 504, nor that its root cause has been diagnosed.

**Smallest correction:** obtain valid evidence for outstanding required checks on the integrated candidate. Use focused tracing and the existing verification-route/outbox test for any reproducible timeout; change code only for a demonstrated cause and preserve assertions/timeouts. Retain the clean aggregate as integrated qualification rather than adding full-suite runs to every repair scenario. If the maintainer explicitly reallocates that gate to change review, record the reallocation; none is presently recorded. No benchmark, migration, skip/ignore, timeout widening or new workflow is authorized.

## Coverage reconciliation and limits

The primary report's complete scenario matrix, domain producer/consumer paths, UI presentation checks and repository merge/gate checks jointly cover the materially affected boundaries. Principal storage/filter/count/Role semantics, self-only authorization, tenant isolation, typed SDK/MCP/CLI propagation and hand-merged provider helpers have source-backed and connected-test evidence; no implementation defect was established there. Their missing served-schema proof and provider UI-consumer proof are retained above without invalidating unrelated assertions.

All twelve approved mock deviations are accepted. Historical captures corroborate structure but have no immutable candidate manifest; the capture helper does not establish every authentication-page variant. This limits any comprehensive fresh visual-parity claim. This validation did not invent a separate production defect from missing capture files. The required modal/scroll corrections need focused light/dark phone/desktop visual checks.

TASK-005 adds no wait on a defined verification/direct-verify/gateway/Oracle/Scribe hot path. Its waits belong to admin/authentication workflows. Separate merged Bifrost performance qualification is outside this task verdict; no benchmark was requested or run. TDD is **NOT_APPLICABLE** under approved revision 12's explicit waiver, not VIOLATED.

Validation reran no Cargo, database or browser suite. Fresh nonmutating format/lint/type checks and execution limitations are recorded in `task-review-repo.md`; task/maintainer runtime results remain attributed receipts. HEAD remained candidate; tracked source remained unchanged. Behavioral recommendation: **FIX_REQUIRED**.
