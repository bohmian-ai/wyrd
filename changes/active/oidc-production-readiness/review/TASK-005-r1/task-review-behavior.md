# TASK-005 behavior review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `134f605367e65b41f1977d6c70ac8ca8b277a69e`
- Candidate: `e3a47a05d931c010f4c70c75edea2d23c447108b`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Task: `changes/active/oidc-production-readiness/tasks/TASK-005-qualification-and-docs.md`

The candidate was still checked out at the stated commit after source inspection. The complete base-to-candidate diff contains documentation, architecture authority, CLI/source documentation, and generated Python and TypeScript declarations; it adds no production executable behavior.

## Caller-path coverage

The review followed these observable paths from documentation to the shipped owner:

- tenant operator setup -> public origin and callback -> candidate connection lifecycle -> provider validation and activation;
- web user -> `wyrd-ui` authorization-code flow -> encrypted BFF cookie -> confidential refresh and best-effort logout revocation;
- CLI user -> RFC 8628 device authorization -> saved login -> Rust/Python/TypeScript credential selection and renewal;
- machine caller -> API-key RFC 8693 exchange or workload RFC 7523 assertion -> access-only renewal;
- OAuth client refusal -> authorization-server endpoint -> RFC 6749 error body;
- workload operator -> trusted-issuer documentation -> boot/runtime trusted-issuer validation.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 / AC-001: human OIDC remains optional; OIDC-off UI, SDK, provisioning, and machine paths remain documented | `docs/src/content/docs/self-hosting/sso-and-oidc.svx:21`, `docs/src/content/docs/concepts/authentication.svx:178-181`, and the UI README describe operator API-key recovery without an IdP | TASK-005 records the owning OIDC-off UI journey and TASK-003/011 evidence; journeys correctly were listed, not rerun | PASS |
| REQ-005: provider secrets alone require the sealing key; secrets remain outside public/browser artifacts | `docs/src/content/docs/self-hosting/authentication.svx:56-69`, `docs/src/content/docs/self-hosting/sso-and-oidc.svx:37-53`, and `crates/wyrd/wyrd-server/src/boot/mod.rs:1506-1528` limit the keyring to provider/workload-issuer secrets | Recorded `docs:check`, `fmt`, and `lints` passed | PASS |
| REQ-018: operator/IdP ownership, exact callback and inputs, one active connection, role mapping, rotation, failures, recovery, CLI selection, and separate human/workload paths are documented | `docs/src/content/docs/self-hosting/sso-and-oidc.svx:23-168` supplies the main operator guide and common-provider notes | Recorded `docs:check` passed, but the cloud-identity field reference contradicts the shipped workload-only boundary (BHV-002) | FAIL |
| REQ-021: Wyrd documents the authorize, token, platform-token, device, revocation, metadata, token-exchange, and JWT-bearer standard surfaces and their form/JSON wire | `docs/src/content/docs/self-hosting/sso-and-oidc.svx:139-152`, `docs/src/content/docs/concepts/authentication.svx:52-66`, and `architecture/wyrd-security-posture.md:190-220` describe the standard surface, including the sanctioned error exception | Recorded `docs:check` and `codegen:check` passed, but two global references still tell consumers every error is Problem Details (BHV-001) | FAIL |
| INV-001 / INV-004: route keys and browser input are not effective authority; callback, provider validation, and failure behavior remain fail closed | `docs/src/content/docs/self-hosting/sso-and-oidc.svx:35, 111-127, 154-164` documents deployment-owned callback state, issuer binding, validation, and failures | TASK-005 records the callback, rotation, and refusal owner evidence | PASS |
| INV-003 / INV-005: platform, tenant-user, and workload planes remain distinct; SDK/UI only project server authority | `architecture/wyrd-design.md:555-573`, `docs/src/content/docs/concepts/identity-and-auth.svx:17-20`, and `docs/src/content/docs/self-hosting/sso-and-oidc.svx:77-81, 129-137` preserve the split | Static source comparison; no executable authorization owner changed | PASS |
| INV-006 and task non-goals: no hosted signup, social login, commercial stub, SAML, SCIM, provider-specific branch, credentials, or certified-provider list | Complete diff contains documentation/source-doc changes only; common-provider material is short configuration guidance in `sso-and-oidc.svx:67-75` | Complete diff and targeted search | PASS |
| AC-002: self-hosted provider setup, exact callback, UI sign-in, role mapping, allowed/denied behavior have an owner | `sso-and-oidc.svx:33-127` | TASK-005 lists `tenant_human_login_journey` and TASK-009/011 evidence | PASS |
| AC-003: hosted tenants may use independent providers and wrong-tenant/same-issuer behavior has an owner | `sso-and-oidc.svx:23-31, 121-127` | TASK-005 lists the multi-provider, callback-refusal, issuer-binding, and same-issuer isolation journeys | PASS |
| AC-004 and FIND-TASK-004-8: CLI establishes the saved login; Rust/Python/TypeScript select the named tenant or newest login, surface mismatch, and use RFC 8628 only | `docs/src/content/docs/get-started/client-configuration.svx:109-148`, `docs/src/content/docs/reference/cli.svx:25-85`, `crates/shared/wyrd-client/src/config.rs:50-60`, Python source/generated stubs, and TypeScript source/native/generated declarations | Recorded `codegen:check` and `ts:napi:check` passed; owner journeys were listed rather than rerun | PASS |
| AC-005: API-key and workload identities stay independent from human SSO and workload binding is exact | `sso-and-oidc.svx:174-211` and `concepts/authentication.svx:183-247` | TASK-005 lists machine-independence, workload JWT-bearer, and federated-cloud owner evidence | PASS |
| AC-006: provider replacement requires a real test sign-in and preserves recovery | `sso-and-oidc.svx:83-127` | TASK-005 lists provider-switch and test-sign-in journey evidence | PASS |
| AC-007: operator-visible fault/security behavior, secret rotation, refresh cutoff, and bounded access-token validity are documented | `sso-and-oidc.svx:46-55, 111-168` and `concepts/authentication.svx:163-181, 249-264` | TASK-005 lists the relevant refusal, rotation, mapping, revocation, and key-rotation evidence | PASS |
| AC-008: provider-agnostic standard OIDC plus short Okta, Entra ID, Google, Auth0, and Keycloak examples; no qualification list | `sso-and-oidc.svx:15-19, 57-75` | Complete diff contains no provider-specific executable branch | PASS |
| AC-009: architecture, public contracts, CLI help, UI, self-hosted/hosted docs, and generated declarations agree | Architecture, CLI help, UI README, SDK source docs, and generated declarations were updated coherently for the main flows | Recorded static lanes passed, but BHV-001 and BHV-002 are public-contract/documentation contradictions | FAIL |
| Required deletion: no login handoff, private BFF channel, browser-session rows, sealed completion, or session/login keyring description remains | Targeted search of the reviewed surfaces found none of the prohibited identity meanings; unrelated uses of “handoff” concern other domains | Static source search | PASS |
| Logout/revocation standing decisions: RFC 7009 revocation is best-effort and already-issued self-contained access tokens live until expiry | `docs/src/content/docs/reference/cli.svx:53-59`, `concepts/authentication.svx:172-176`, and `sso-and-oidc.svx:137` say exactly this | Static source comparison | PASS |
| Standard mechanisms only: RFC 8693 API-key exchange, ingress-owned `POST /auth/device` rate limit, origin-normalized saved-login identity, and vetted libraries are not replaced by custom mechanisms | `concepts/authentication.svx:202-213`, `sso-and-oidc.svx:17, 55, 152`, `architecture/wyrd-design.md:570-573`, and existing `wyrd-client` origin normalization | Complete diff adds no protocol implementation or dependency | PASS |
| Verification scope stays narrow; no task-level journey or aggregate run is required | Task records `docs:check`, `codegen:check`, `ts:napi:check`, format/lint lanes, and `git diff --check`; UI runtime check was omitted because only its README changed | `git diff --check base..candidate` was independently clean; owner journeys were only listed | PASS |

## Proposed findings

### BHV-001 — INCORRECT: global API and agent references erase the OAuth error exception

- **Violated obligation:** REQ-021, AC-009, and TASK-005 Approach 3 require the OAuth endpoints' RFC 6749 §5.2 error wire to be documented as the one exception to Wyrd Problem Details.
- **Location:** `docs/src/content/docs/api/openapi.md:26-28`; `docs/src/content/docs/for-agents/error-remediation.svx:13-16` and the global `code`-based instructions that follow.
- **Evidence:** These pages say every operation/error has `application/problem+json` and a stable Wyrd `code`. The shipped owner, `crates/wyrd/wyrd-server/src/auth/oauth.rs:3-9, 127-146`, explicitly renders `/auth/token`, `/auth/platform/token`, `/auth/device_authorization`, and `/auth/revoke` refusals as `OAuthErrorResponse` JSON carrying the registered OAuth `error` and optional `error_description`, not a Wyrd problem body. The candidate correctly documents this in `docs/src/content/docs/api/errors.md:11-15` and `docs/src/content/docs/self-hosting/sso-and-oidc.svx:139-152`, which makes the remaining global guidance internally contradictory.
- **Observable consequence:** An agent or generated-client consumer following the affected references can branch on a nonexistent `code` field or require the wrong media type when authentication fails, losing the standard OAuth refusal it needs to handle.
- **Required testable correction:** Scope Problem Details and Wyrd `code` guidance to non-OAuth operations in both global references, explicitly identify the four form endpoints' RFC 6749 error response, and point consumers to the OAuth contract already documented by the SSO guide/live OpenAPI. Prove the corrected pages contain no unconditional “every/all error is Problem Details” claim and run `mise run docs:check` only.

### BHV-002 — INCORRECT: the workload trusted-issuer field reference still advertises rejected human issuers

- **Violated obligation:** REQ-018, AC-009, and TASK-005 Approach 2 require documentation to separate tenant human connections from machine identity and to match the shipped configuration boundary.
- **Location:** `docs/src/content/docs/concepts/cloud-identity.svx:73-93`, especially lines 86 and 88-91.
- **Evidence:** The page first says human sign-in is configured separately and a Human trusted issuer is refused (`:15-19`), then its field table says `principal_kind` may be `"workload" or "human"`, defaults to human, and describes email, group mapping, and default roles as Human trusted-issuer behavior. Both trusted-issuer entry paths reject that configuration: `wyrd_spec::auth::refuse_human_trusted_issuer` in `crates/wyrd-spec/src/auth/human_connection.rs:309-322`, the runtime handler call in `crates/wyrd/wyrd-server/src/components/admin/routes.rs:300-303`, and boot rejection in `crates/wyrd/wyrd-server/src/boot/issuer.rs:105-124`.
- **Observable consequence:** An operator following the table can configure a Human trusted issuer and receive a deterministic boot or API refusal instead of the documented human federation behavior.
- **Required testable correction:** Make the cloud-identity field reference describe only accepted workload trusted-issuer configuration: require/select `workload`, stop advertising Human/default-human semantics or Human-only mapping behavior there, and route human setup to the tenant OIDC connection guide. Preserve the stable Human discriminator only as a documented refusal if it must be mentioned. Prove the workload page no longer presents a Human trusted issuer as usable and run `mise run docs:check` only.

## Non-blocking notes

None. Wording, placement, and page structure were not treated as findings.

## Verification assessment

TASK-005 records successful exits for `mise run docs:check`, `mise run codegen:check`, `mise run ts:napi:check`, `mise run fmt`, `mise run lints`, `mise run py:format`, `mise run py:lints`, and `git diff --check`. Those lanes credibly prove rendering, generated-declaration parity, formatting, and static build health, but they do not compare prose semantics across pages; therefore they do not close BHV-001 or BHV-002. Per the task and standing direction, no full journey or aggregate suite was run or required in this review.

## Overall result

**FAIL**

The core docs and declarations describe the shipped standard flows well, but the candidate does not yet satisfy the task exactly because two public references still contradict the shipped OAuth error contract and workload-only trusted-issuer boundary.
