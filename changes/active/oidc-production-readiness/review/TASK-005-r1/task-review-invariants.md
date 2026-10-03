# TASK-005 invariant review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `134f605367e65b41f1977d6c70ac8ca8b277a69e`
- Candidate: `e3a47a05d931c010f4c70c75edea2d23c447108b`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-005-qualification-and-docs.md`
- Candidate HEAD was `e3a47a05d931c010f4c70c75edea2d23c447108b` before and after source inspection.

## Invariant trace

The cumulative diff is documentation and source-documentation work. I traced the changed claims back to their runtime owners rather than accepting the implementation-evidence table as proof:

- Saved-login selection flows from `ClientConfig::resolve_credential` through `SavedLogins::select`; the origin is canonicalized by `canonical_origin`, a selector mismatch fails rather than selecting another tenant, and the selector-free path uses the newest login.
- Human authorization-code, device-code, refresh, revocation, API-key exchange, and JWT-bearer grants converge on the tenant OAuth endpoint, while platform credential exchange has its distinct production owner at `POST /auth/platform/token` in `components/platform/routes.rs`.
- Public-client refresh tokens rotate; the authenticated `wyrd-ui` refresh token is re-presented and has the bounded web-app lifetime. The BFF owns the encrypted cookie, caches access tokens per replica, revokes refresh authority best-effort on logout, and deliberately retains an already-issued access token's bounded validity.
- The operator-recovery cookie holds the API key and renews by the sanctioned RFC 8693 exchange using `urn:wyrd:oauth:token-type:api_key`; it is not a second server-side session authority.
- Trusted workload issuers remain capable of carrying `secret_basic` or `secret_post` client authentication, which is sealed and included by the production rewrap owner. Human trusted issuers are refused in favor of the tenant connection API.

Two documentation producer/consumer mismatches remain. They are semantic errors in task-owned public docs, not placement or wording preferences.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001: OIDC remains optional and machine/operator paths remain usable | `sso-and-oidc.svx` states OIDC is optional; UI recovery and machine paths are documented separately | Owner journeys are mapped in the task evidence; journeys intentionally not rerun in this task review | PASS |
| REQ-005: only stored provider/workload client secrets require the sealing key; recoverable tokens and API keys are not server-sealed | Security posture, boot rewrap rustdoc, authentication/configuration docs, and BFF cookie owner describe the boundary | `docs:check`, `fmt`, and `lints` recorded green, but semantic review found the contradictory workload statement below | FAIL (`INV-REV-001`) |
| REQ-018: self-hosted and hosted responsibilities, callback, inputs, one active connection, role mapping, rotation, failures, recovery, saved login selection, and separate human/workload paths are documented accurately | `self-hosting/sso-and-oidc.svx` and linked concept/configuration pages cover the required topics | `docs:check` recorded green; it does not validate semantic consistency | FAIL (`INV-REV-001`, `INV-REV-002`) |
| REQ-021: documented OAuth endpoints and wire behavior match the authorization-code, device, refresh, revocation, token-exchange, jwt-bearer, metadata, and platform surfaces | Architecture, SSO, authentication, errors, and curl examples document form bodies and RFC error responses | `docs:check`, `codegen:check`, and `ts:napi:check` recorded green | FAIL (`INV-REV-002`) |
| INV-001: tenant and connection selection remain server-bound | Docs consistently distinguish route context from server-owned state; no changed executable path weakens it | Existing owner journey mapping recorded | PASS |
| INV-003: platform, tenant-user, and workload planes remain distinct | Architecture and SSO docs keep the planes and workload path separate | Static source trace; no executable behavior changed | PASS |
| INV-004: OIDC trust remains fail closed | SSO failure and issuer-binding sections retain discovery, JWKS, SSRF, replay, and audit behavior | Existing owner security journey mapping recorded | PASS |
| INV-005: UI and SDKs project server-owned identity | BFF and all SDK docs point to the server/shared Rust client; no durable role mapping was added | `codegen:check` and `ts:napi:check` recorded green | PASS |
| INV-006 and task non-goals: no hosted signup, social login, commercial stub, SAML, SCIM, certified-provider list, or provider branch | Cumulative diff adds none; generic provider examples are documentation only | Diff/source inspection | PASS |
| AC-001: OIDC-off UI and SDK operation | Recovery and optional-OIDC docs mapped to the owner journey | TASK-003/011 evidence recorded; suite only listed here per task contract | PASS |
| AC-002: self-hosted provider setup and exact callback | Generic setup, exact callback, role mapping, and sign-in flow documented | TASK-009/011 owner evidence recorded | PASS |
| AC-003: hosted tenants/providers remain isolated | Hosted responsibility and one-active-connection text is explicit | TASK-002/011 owner evidence recorded | PASS |
| AC-004: Rust/Python/TypeScript saved-login selection and renewal | Shared-client rustdoc, PyO3 source/stubs, napi source/declarations, TypeScript source, CLI, and client configuration agree on newest/default and selected-tenant mismatch behavior | `codegen:check` and `ts:napi:check` recorded green; owner tests listed | PASS |
| AC-005: machine identity stays independent | Human SSO docs explicitly preserve API-key and workload paths | TASK-002/012 owner evidence recorded | PASS |
| AC-006: provider replacement and real test sign-in | Candidate test and activation lifecycle remain documented | TASK-002/011 owner evidence recorded | PASS |
| AC-007: fault, rotation, logout, and bounded-token behavior | Failure table, sealing rotation, best-effort logout, ingress device rate limit, and bounded access-token validity are documented | Owner evidence recorded; no broad journey rerun required | PASS |
| AC-008: provider-agnostic implementation and generic/common-provider documentation | Generic instructions plus short Okta, Entra ID, Google, Auth0, and Keycloak notes; no provider branch or certification record | Diff/source inspection and owner evidence | PASS |
| AC-009: public contracts, help, UI, docs, and declarations agree | Most surfaces agree, but two task-owned public-doc claims contradict shipped owners and sibling docs | Static semantic comparison; ordinary docs/codegen gates cannot catch these contradictions | FAIL (`INV-REV-001`, `INV-REV-002`) |
| FIND-TASK-004-8: SDK and generated declarations describe RFC 8628 login, newest-login default, and selected-tenant refusal | `ClientConfig::tenant`, PyO3 source and stubs, napi/TS source and generated declarations | `codegen:check` and `ts:napi:check` recorded green | PASS |
| Remove the obsolete login handoff, private BFF channel, browser-session rows, sealed completion, and session/login keyring model | Auth architecture/docs now describe standard grants and the BFF cookie; scoped searches found no remaining obsolete auth passage | Diff and scoped source search | PASS |
| Do not promise instant access-token revocation | Logout and connection lifecycle docs explicitly retain access-token validity until bounded expiry | Diff/source comparison | PASS |
| Verification stays limited to the write set | Task records docs/codegen/napi/format/lint checks and does not claim a task-level aggregate or full journey run | Recorded command set matches the human direction | PASS |

## Proposed findings

### INV-REV-001 — INCORRECT — trusted-issuer documentation contradicts the shipped workload-only and sealing contracts

- Violated obligation: REQ-005, REQ-018, AC-009, and the task outcome that public docs accurately describe the shipped standard flows.
- Locations:
  - `docs/src/content/docs/concepts/cloud-identity.svx:85-91`
  - `docs/src/content/docs/self-hosting/configuration.svx:78`
- Evidence: the cloud-identity page first says every trusted issuer has `principal_kind = Workload` and that `Human` is refused, then its field table says `principal_kind` may be `"workload" or "human"`, defaults to `"human"`, and presents human-only mapping/default-role fields as meaningful configuration. Production validation routes `Human` to `WYRD_AUTH_400_HUMAN_CONNECTION_REQUIRED` (`wyrd-spec/src/auth/human_connection.rs:309-319`). The configuration page also says workload federation carries no client secret immediately after acknowledging that a workload issuer can authenticate with a shared secret. The same cloud-identity table documents `secret_basic`/`secret_post`, server config accepts them (`wyrd-server/src/config.rs:3373-3382`), and the boot rewrap path includes workload-issuer secrets.
- Observable consequence: an operator following the field table can author a human trusted issuer that Wyrd deterministically refuses, or can omit the sealing key for a secret-bearing workload issuer and make boot fail closed. These are public operational failures, not wording preferences.
- Required testable correction: reconcile the existing docs with the shipped owner. Describe trusted issuers as workload-only, remove or clearly mark refused human-only field behavior, and state that a secret-bearing workload issuer requires the sealing key. Preserve the existing tenant connection path for human login and the existing supported workload client-auth variants. Prove closure with the narrow documentation check and a source comparison to the existing `Human` refusal and secret-bearing issuer configuration; no runtime or journey change is required.

### INV-REV-002 — INCORRECT — the operator authentication guide sends every grant to the tenant endpoint

- Violated obligation: REQ-018, REQ-021, and AC-009.
- Location: `docs/src/content/docs/self-hosting/authentication.svx:23`.
- Evidence: the changed text states, without a tenant-plane qualifier, that “Every grant ends at `POST /auth/token`.” The same page introduces both administration planes and the candidate's SSO guide correctly lists `POST /auth/platform/token`. The production platform route is mounted separately at `components/platform/routes.rs:60,79-94` and accepts the platform RFC 8693 API-key exchange there. The tenant endpoint does not replace it.
- Observable consequence: an operator using a platform credential can send it to the wrong authorization plane and receive a refusal, while the guide incorrectly collapses a deliberate plane boundary.
- Required testable correction: qualify the sentence and grant table as tenant-plane behavior and name the existing `POST /auth/platform/token` owner for platform credential exchange. Do not add a new route, alias, or abstraction. Prove closure with `mise run docs:check` and a direct comparison to the two existing route owners.

## Non-blocking notes

None. I did not retain placement, naming, structure, or wording preferences.

## Verification assessment

The recorded narrow lanes are appropriate for this documentation/source-documentation write set: `docs:check`, `codegen:check`, `ts:napi:check`, Rust and Python formatting/lints, and `git diff --check` all reportedly exited zero. The UI-specific check was reasonably omitted because the only UI-tree change is its README. Per the task and human direction, I did not run or require full journey suites or aggregate gates. The two findings are semantic documentation contradictions that those mechanical lanes are not designed to detect.

## Overall result

**FAIL**

The candidate preserves the runtime invariants and closes the saved-login/generated-declaration obligation, but the task is not acceptance-complete while task-owned public docs direct operators to configurations and endpoints that the shipped implementation refuses.
