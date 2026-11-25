# TASK-005 primary behavior review

Subject: base `f7e0d67b2e007a1262fed34d45e5ebd2629e9bee`, candidate
`78cd99091c81e5a7d706ac6f54a8222e62705e0a`, approved authorization-model spec
revision 12, original `tasks/TASK-005-admin-console.md`. HEAD was unchanged
through inspection. No production source was changed and no runtime tests were
run by this reviewer.

## Scope and authority

Reviewed REQ-021 and AC-016 independently of the task evidence, the revision-12
persistence authorization, finalized admin-console design, approved mock source,
root AGENTS.md, spec-driven development, maintainer style, TESTING.md, wyrd-ui,
brand/DESIGN.md, architecture/wyrd-design.md's self-profile paragraph and the
first-party browser/platform session boundaries in wyrd-security-posture.md.
No `.codegraph/` exists. Principal producer/projection correctness and visual
alignment are independently assigned specialist boundaries. This report does
not claim their inspection as its own.

The cumulative diff contains merged PR244 Bifrost/benchmark changes, earlier
authorization remediation, and development-environment changes. These are
dependencies/context, not additional TASK-005 product obligations. This review
traced the actual candidate at the UI/producer seams without attempting another
acceptance audit of those separate tasks.

Navigation: tenant layout `/me` -> shared Shell -> settings section layout ->
People/Services/Roles loads and shared access actions; Sign-in load/actions ->
existing identity owner; Secrets composition/actions -> four existing owner APIs;
account load -> own profile/credentials; platform pages -> platform session and
console change owner. Runtime proof owners are ui-pages.integration.test.ts,
production-auth.integration.test.ts, onboarding.integration.test.ts, and
platform_admin_e2e. The matrix records scope and known gaps; specialist rows
must be reconciled before final acceptance.

## Acceptance matrix

All rows map REQ-021 / AC-016 and the existing numbered task scenario, rather
than defining new obligations. Paths below are relative to
`crates/wyrd/wyrd-server/wyrd-ui` unless otherwise stated.

| Task scenario | Implementation / consumer | Assertions and evidence assessed | Assessment |
|---|---|---|---|
| 1 Authority first | architecture/wyrd-design.md:1040; CurrentUserProfile | Approved wording is present, preserves self-only disclosure and secret exclusions; first-parent history places f1efb9a5f before 1a4fefbad. | PASS |
| 2 Provider name persistence and consumer reload | Provider/SQL owners assigned to principal specialist; tenant +layout.server.ts identity; account +page.svelte:61; People AccountDrawer.labelOf | Server/SQL storage journeys cited in task are distinct from production UI consumption. No UI fixture supplies a named social identity and asserts provider-name rendering/refresh. Alice is a password fixture, not the claimed stored-name proof. See B-003. | FAIL: UI evidence gap; producer assessment delegated |
| 3 Safe projections across shipped clients | Principal/spec/SQL/SDK/MCP owners assigned specialist; UI uses typed fields | Connected principal and language tests are present and evidence is recorded; detailed producer/projection/tenant coverage belongs to specialist. UI consumes page.counts and role.member_count directly. | UNRESOLVED pending specialist |
| 4 Settings / account identity | settings/(admin)/+layout; tenant layout; account page/load | ui-pages.integration.test.ts:1215-1221 proves password username and ordinary User account entry; SSO account test :790 proves method/key restrictions. It does not prove social display-name priority or own email second line. See B-003. | FAIL: missing provider identity consumer proof |
| 5 People / Services / Roles | admin access.server.ts; People, service-accounts, roles pages; AccountDrawer | ui-pages :2719-2789 invitations/denial; :3020-3105 actual direct/group grants, refused changes and custom role creation; :3108 suspension/keys; :3207 runtime Service account; :3337 denial vs unavailable; :3386 keyboard/modes/width. Role description producer semantics delegated. | PASS for traced existing access workflow; specialist projection proof pending |
| 6 Sign-in lifecycle / policy | sign-in +page.server.ts forwards existing endpoints; +page.svelte opens explicit policy confirmation | production-auth :832-902 exercises unconfirmed refusal, company proof, non-admin, cross-replica policy state, stale revision and explicit disable. Existing replacement/SSO journeys trace actual identity lifecycle. Destructive Deactivate/Remove bypass confirmation. See B-002. | FAIL |
| 7 Secrets / Integrations | secrets +page.server.ts composes redacted four-owner inventory and routes add/replace/renew/revoke; integrations delegates GitHub | GitHub journey :2370 onward proves actual connection/refusal/cancel/disconnect. Secrets has only captureAdmin navigation :2986, no mutation/refusal/cancel/reload journey. Owner tests do not execute this new BFF mapping. See B-001. | FAIL: Secrets evidence gap |
| 8 Platform console | platform/ui layouts/pages; existing platformSession/consoleChange owners | ui-pages :3601 proves plane separation, operator identity, destinations and actual workspace/admin reads; :3694 / :3781 proof-resumed workspace suspension; :3854 provider lifecycle; :3949 mail password preservation; :4019 signup config; actual actions remain in platform plane. | PASS for traced consumer workflows; hand-merged fixture seam assigned specialist |
| 9 Provider marks | Mark.svelte, signup/login/invitation, People/Sign-in | Fixed official-colored Google / currentColor GitHub assets and brand exception present; ui-pages :3460-3510 exercises Join with Google / later Continue with Google through real Rust/BFF fixture. Visual parity assigned specialist. | PASS structurally / interaction; visual assessment delegated |
| 10 Mock states / modes / accessibility | shared Shell/Overlay/console.css; admin pages | Maintainer approved twelve differences in task; source-based review does not reopen those decisions. Capture helper visits widths/modes; real keyboard tests cover People/Services. Full visual comparison and Overlay accessibility assigned specialist. | UNRESOLVED pending specialist |

## Proposed blocking findings

### B-001 — EVIDENCE_GAP: new Secrets administration has no connected journey

- Authority: REQ-021 Secrets lifecycle, AC-016, TASK-005 scenario 7 and root
  AGENTS.md's journey requirement.
- Locations: `src/routes/t/[tenantKey]/settings/(admin)/secrets/+page.server.ts:170-219`
  and its page form consumers; `src/lib/ui-pages.integration.test.ts:2986`;
  TASK-005 scenario 7 evidence.
- Start/trigger: a tenant admin opens Secrets with existing provider,
  Operator, company-SSO or trusted-issuer secrets, then adds/replaces a value,
  renews encryption or revokes an eligible provider key; cancellation or a
  direct request by another tenant/non-admin must preserve state.
- Expected proof: production page/form -> real BFF -> actual Rust owner ->
  reload, with redaction, successful mutation, supported owner dispatch,
  cancellation, refusal and safe failure demonstrated.
- Actual proof: the sole Secrets reference in the UI test owner is a capture
  navigation. Searching the UI tests for the Secrets path and the new action
  outcomes/controls finds no journey. Task evidence cites the GitHub integration
  journey and a Secrets capture. GitHub does not exercise Secrets. Existing Rust
  secret-owner tests prove owner behavior, not these new form-to-wire mappings.
- Consequence: no executable evidence establishes that administrators can manage
  these credentials through the newly shipped UI safely; green rendering and
  existing owner suites cannot establish the connected capability.
- Smallest correction: extend the existing production UI fixture with bounded
  Secrets form journeys over real owner metadata and endpoints. Group equivalent
  owner cases while asserting each distinct dispatch/payload mapping, accepted
  reload, no plaintext in inventory, cancel/no mutation, non-admin and
  cross-tenant refusal, and safe failure. Reuse existing Rust owner tests for
  internal encryption/lifecycle details. No production changes are demanded by
  this finding alone.

### B-002 — INCORRECT: destructive company-SSO actions submit immediately

- Authority: REQ-021's mandatory page pattern: "destructive actions confirm and
  sit in a Danger zone"; finalized design Page pattern; TASK-005 scenarios 6/10.
- Locations: `src/routes/t/[tenantKey]/settings/(admin)/sign-in/+page.svelte:261-276`,
  its `+page.server.ts` deactivate/remove actions;
  `crates/wyrd/wyrd-auth/src/connections.rs:746-784`.
- Start/trigger: an authorized administrator has an Active company connection
  and company SSO is optional. The administrator clicks Deactivate or Remove.
- Expected: a confirmation states the impact and allows cancellation before
  issuing the destructive request.
- Actual: each is a plain POST submit; neither enters a confirmation state.
  The BFF immediately calls the existing owner. The owner retires the Active
  connection or tombstones it and wipes its secret. Existing production SSO
  tests submit these forms directly and do not assert confirmation/cancel.
- Consequence: one click can end company-backed sessions/sign-ins or remove
  their configured connection. Merely placing the button in Danger zone and
  showing help text does not implement the specified confirmation.
- Smallest correction: use the existing confirmation mechanism at the Sign-in
  page; retain existing Rust authorization, policy/proof and lifecycle behavior.
  A focused production journey should assert opening/cancelling writes nothing,
  then confirmed action reaches the existing owner and reloads the real state.
  This does not require a new server workflow.

### B-003 — EVIDENCE_GAP: provider names are not proven in production identity UI

- Authority: REQ-021 name/email and fallback promises; AC-016; TASK-005 scenarios
  2/4 require successful provider sign-in -> persistence -> actual name block,
  account and People consumers.
- Locations: task scenarios 2/4 evidence;
  `src/lib/ui-pages.integration.test.ts:2687-2988` (claimed Alice proof),
  `:3460-3510` (Google fixture), `:1215-1221` (password account assertions),
  `src/lib/server/auth/onboarding.integration.test.ts:498-528`,
  `src/lib/server/auth/production-auth.integration.test.ts:790-830`.
- Start/trigger: a social/company User without a username signs in with a
  provider display name, later refreshes it, and reads the actual tenant shell,
  own account and People.
- Expected proof: actual rendered display-name priority, refreshed name, own
  email second line and supported fallback paths, without disclosing another
  tenant's identity; the consumers use the predecessor's real persisted value.
- Actual: Alice is created with a local password/username and the named journey
  has no provider-name assertion. The Google fixtures omit `name` and assert
  landing/account continuity, not displayed name/email. SSO account assertions
  prove method/key behavior, not identity labels. No UI-test `display_name`
  assertion exists; the sole use is fallback when selecting a suspension target.
  Correct-looking fallback source and the server storage journey are not
  connected rendering proof.
- Consequence: the task's central social-identity correction lacks executable
  evidence at the required consumer boundary. This is a proof gap, not a claim
  that the fallback expressions are wrong.
- Smallest correction: extend existing controlled-provider production fixture
  and account/People journey, use distinguishable provider name and email,
  verify refreshed rendered identity and fallbacks, then record actual focused
  execution evidence. Reuse server-level parsing/storage proof where equivalent;
  no new identity feature or production churn is required.

## Verification and process limits

The recorded post-merge codegen, OpenAPI/platform, principal, identity UI, UI
checks, docs and development lifecycle results are evidence supplied for this
candidate; this reviewer did not rerun them. The task explicitly lists
`test:wyrd` as required, but records 3032/3034 and the staging-refusal 504 plus
standalone pass rather than a clean final lane. That is an open verification
limit, not source proof of a TASK-005 regression. The orchestrator/validator
must reconcile the explicit task gate with focused remediation requirements.
Unrecorded format/lint evidence may be closed by the repository specialist.

TDD: NOT_APPLICABLE to these ordered entries under the task's explicit approved
instruction that none requires RED/GREEN ordering. Storage/read additions still
require contract/connected proof; a later passing test is not historical RED,
but this task does not require one. No fabricated TDD violation is proposed.

Provisional behavioral recommendation: FIX_REQUIRED for B-001, B-002 and B-003,
subject to independent validation and reconciliation with specialist reports.
Source remained immutable. Skipped implementation and broad suite reruns; the
unproven Secrets and provider-identity UI journeys remain material limits.
