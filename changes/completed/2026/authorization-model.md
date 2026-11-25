# Scoped authorization, onboarding and administration

- Change: `authorization-model` (`SPEC-authorization-model`), approved through revision 12.
- Completed: 2026-11-24.
- Completion target: `581a88a78b9464ed3989e07a675e8483c8479243`.
- Final change review: skipped at the maintainer's explicit direction. This is administrative closeout, not a claim of integrated verification or deployment readiness.
- Latest independently accepted admin-console target: `9a5e481fc73021738b522de22ca79980651e9d90`.
- Delivery reference: not supplied.

Administrators can give a User or Service account access to a particular resource and its retained evidence, with the same boundary across the server, SDKs, CLI, MCP and UI. People can join an invited workspace, create one when signup is enabled, and use local or configured provider sign-in. Workspace administration and platform operation remain separate.

## Delivered behavior

- Roles contain typed operations; scoped bindings assign them to Users or Service accounts, directly or through optional local groups. Scopes include a tenant, Card family or exact version, Bifrost schema/table, gateway deployment/model, GitHub connection and supported built-in tool. Family grants cover future versions. Removing a direct binding leaves independent group/provider grants intact.
- Server-owned cached authority governs current access. Tokens identify callers and retain credential/delegation ceilings rather than authoritative grant snapshots. Role changes affect existing credentials after bounded propagation; credentials cannot create authority their principal lacks.
- A family-bound Service account and credential work across releases while requests retain exact workload attribution. Registering a Card grants no access and creates no principal.
- Resource access includes authorized derived results and retained verification evidence. Point reads, lists, counts, downloads and Oracle queries enforce visibility before user filtering or aggregation, including peer execution. GitHub connection administration remains distinct from Repository Card visibility.
- Named human setup works without an identity provider or SMTP. CLI login uses explicit RFC 8628 device approval and rotating refresh credentials through the shared Rust client. Browser and refresh credentials retain their original authentication anchor.
- Operators configure Google/GitHub and generic SMTP; tenant administrators configure company OIDC. Addressed individual/bulk invitations carry scoped access and single-use copyable links, with optional email delivery and honest delivery outcomes.
- Enabled signup creates an independent workspace and its administrator through the existing provisioning owner. Verified local email supports provider-free acquisition; social acquisition requires the approved contact proof. Discovery reveals verified workspace links after proof, then requires ordinary tenant login. Verified contacts support private password recovery.
- Company-SSO policy reads expose current revisions, readiness and the server-owned count of active Users without a usable company identity. Enable/disable requires fresh company-admin proof and explicit confirmation; stale proof or revisions refuse. Enforcement covers existing human login, device and renewal paths. Disabling policy does not resurrect invalidated credentials; provider outage or replacement never enables weaker fallback.
- Platform operators use a separate console with registered platform OIDC or platform-local password sign-in. Tenant cookies cannot authorize platform operations, and platform sessions cannot authorize tenant work. Destructive platform changes require fresh same-operator proof.
- Settings exposes People, Service accounts, Roles, Sign-in, Secrets and Integrations. Your account opens from the identity block. The platform console shares its shell and mode control. Dialogs use native modality, contain keyboard focus, restore focus on close and confirm destructive SSO changes. Dense tables expose labeled keyboard scrolling regions.
- Provider names are stored and refreshed at successful sign-in; labels prefer display name, then username, then email. `/me` returns only the caller's own email. Principal reads expose authentication, scoped Roles and filtered status counts; Role reads expose descriptions and distinct member counts. Rust/Python/TypeScript and MCP project the additions; CLI custom-Role authoring accepts descriptions.
- Tenant Secrets composes redacted metadata from existing provider-credential, Operator-connection, company-SSO and trusted-issuer owners. Replace, renew-encryption and supported revoke operations use those owners. Official Google/GitHub marks identify provider choices and methods in both modes.

## Lasting constraints and decisions

Rust owns authorization, authentication, credentials and durable workflows. The UI renders server results; it never computes authoritative counts, invents secret-age thresholds or implements a separate workflow. Pure contracts remain in `wyrd-spec`, transport/auth in `wyrd-client`, persistence in the owning SQL layer, and SDKs project those contracts.

Grants are additive; credential ceilings and delegation only narrow them. A warm permission check uses process memory. Healthy replicas enforce committed changes within five seconds; an unknown invalidation or expired/unavailable snapshot fails closed. An otherwise valid snapshot cannot survive more than five minutes without successful confirmation. Permission auditing stages on the process Scribe outbox and never blocks the operation. PostgreSQL retains catalog/coordination data; Bifrost retains evidence and audit history.

Users and identities remain tenant-local. Matching email never merges accounts, transfers grants or links authentication methods. Company adoption uses a company User and existing group mapping or grant administration. Self-service linking, adding a password to a social-only User and removing methods were explicitly deferred; they are not delivered launch features. Existing provider-disablement and credential-invalidation safeguards still apply.

Private defaults remain password enabled, signup disabled, no social credentials and no SMTP. Copied invitations and local setup/login work offline. SMTP supports invitations, contact verification, discovery and reset without a provider-specific integration or durable plaintext mail queue. Public recovery/discovery responses conceal membership; transient sends may be lost on interruption and retried by the user. Password operations share the fifteen-character minimum and existing blocklist/size safeguards.

Signup admission uses trusted-source windows, bounded pseudonymous counters, optional server-validated challenge and an atomic owned-workspace cap. Sign-in starts are separately admitted before pending records: the 301st start in an hour is refused with typed retry information. Missing source or unavailable admission fails closed. Public signup cannot claim reserved system slugs; privileged operator provisioning remains separate.

Platform sessions have fifteen-minute idle and eight-hour absolute limits, distinct host-only protected cookies and no refresh-token flow. Tenant browser sessions retain their twelve-hour absolute lifetime. Original provider/password revisions, purpose/browser binding, one-use state and fresh-proof rules remain authoritative. In-flight platform OIDC completion now depends on durable login-state consumption in the granting transaction, so removing/reinstalling a connection cannot revive an old callback; no database lock spans provider IO.

Provider, mail and challenge secrets use shared `WyrdKms` stored-secret encryption. One wrapped platform derivation key keeps proof/counter/cap identity stable across wrapping-key rotation. Owners retain their lifecycle and authorization; no generic decryption surface or browser/Node secret is introduced.

The change revises an unshipped system directly, with no mixed-token compatibility layer or upgrade subsystem. Revision 12 explicitly authorized nullable `auth_users.display_name` and `auth_roles.description` edits in the unshipped auth migration. Built-in descriptions remain server-set; custom descriptions are administrator-set. Omitted provider names preserve the stored value; signup creators receive a name at their next provider sign-in. No user-editable display name is added.

## Approved revisions and presentation deviations

Revision 2 settled scoped grants, local device anchors and workload preservation. Revisions 3–5 added social acquisition, copyable invitations with optional SMTP, and bounded signup abuse controls. Revisions 6–7 added verified local acquisition, human setup, discovery/recovery and the separate platform console. Revisions 8–10 adopted shared stored-secret encryption, admitted sign-in starts and reserved signup slugs. Revision 11 deferred self-service method management while retaining company-SSO enforcement and connected verification. Revision 12 added the admin console and identity/read projections, including Role descriptions/counts, and waived test-first ordering for that bounded redesign.

The admin redesign preserved Evidence Thread geometry and tokens, with an explicit exception for fixed official provider-mark SVG colors. The maintainer accepted twelve differences from the illustrative mock:

1. The shell retains its disabled Find or ask control.
2. Phone layout retains top navigation rather than hiding the rail without a replacement.
3. The section list omits a Secrets count because no server count exists.
4. People omits Last sign-in and a Suspended segment, exposes Suspended as a stat, and searches exact email/username.
5. Service accounts shows Roles and a Suspended stat, without unsupported key/last-use columns or key stats.
6. Your account includes password, API-key and session controls, without illustrative Roles or Verified by rows.
7. Roles retains server name ordering.
8. Secrets shows last-rotated only for provider keys; inventory stats count the composed rows.
9. Platform omits unsupported rewrap status, new-workspace/add-admin controls and workspace People/Created columns; Mail reports whether its password is set.
10. Invitation expiry is an absolute instant.
11. API-key sessions are labeled API key.
12. Signup display names arrive at the next provider sign-in.

## Acceptance evidence and remaining qualification

These are retained implementation/test records and prior task-review conclusions, not a new final review. Paths below remain available after packet retirement.

| Acceptance | Durable evidence | Recorded closure and limits |
|---|---|---|
| AC-001–003: scoped resources, two releases, current credentials | Backend authorization/cache, retained-parent and peer tests; SDK principal/resource journeys; production access journeys | Backend task accepted after remediation. Access/UI corrections and peer Variant metadata proof recorded; pinned codec-owner format/all-feature lint qualification subsequently recorded passing. No integrated whole-change acceptance is implied. |
| AC-004–005: providers, invitations and workspace creation | Server `platform_admin_e2e` provider/signup cases, production onboarding and Google invitation journeys, real SQL/SMTP fixtures | Controlled-provider and mail proof recorded. Real Google/GitHub compatibility smoke remains unfinished. |
| AC-006: policy and original credential anchors | Company-SSO HTTP tests and two-replica production provider journeys; shared Rust renewal consumer | Read/warning, proof/revision refusal, enforcement and non-resurrection proof recorded. End-to-end CLI binary and Python/TypeScript company-policy qualification were left open in the retained delivery record. |
| AC-007–012: local acquisition, setup, discovery, recovery, methods and passwords | Server setup/signup/contacts/reset/methods suites and production onboarding fixture | Implementation and focused proof recorded. Earlier official-image startup verification did not reach runtime after resource failures; later startup-script changes are outside the admin-console acceptance and no new runtime success is inferred here. |
| AC-013: platform isolation and fresh proof | Platform console/session/SQL journeys, including held-provider-response removal/reinstall regression | Console lifecycle and invalidation proof recorded. The in-flight callback correction has focused passing evidence; the reverse grant/removal ordering is supported by transaction ordering rather than a dedicated runtime test. |
| AC-014–015: encrypted secrets and bounded starts | Server stored-secret rotation and sign-in admission journeys | Recorded controlled-provider/SQL proof covers stable derivation across rotation and bounded refusal; no deployment qualification is claimed. |
| AC-016: admin console and read projections | Principal SQL/server/Rust/Python/TypeScript/MCP/CLI tests; production UI/provider journeys; served OpenAPI assertions | Admin remediation independently accepted at `9a5e481fc`. Connected Secrets mutations/refusals/redaction and persisted name rendering/refresh are asserted. Merge-lost principal/policy OpenAPI assertions were restored. Fresh review component checks passed 24/24; recorded UI qualification passed 107 tests, Clippy, TS type/N-API and formatting checks. |

The full `test:wyrd` aggregate remains open for integrated verification. Its previously reported staging-refusal timeout was traced to a test gate counting permission audits as well as reserved inputs; the test now probes actual reservation refusal without weakening its typed refusal, retry identity or replay assertions. Five focused trace-enabled passes were recorded; they do not substitute for a clean aggregate.

Final change review was skipped by maintainer instruction, and no complete final acceptance matrix or aggregate pass is invented. Other full identity/platform/language and deployment lanes not subsequently demonstrated in the retained records remain unqualified. The 390px full-page capture area below a native dialog appeared dark while viewport interactions were correct; no comprehensive fresh screenshot certification is claimed. These limits survive administrative retirement of the packet.

## Current owners and references

- Authority: [runtime identity and human sign-in](../../../architecture/wyrd-design.md#runtime-identity), [authorization](../../../architecture/wyrd-security-posture.md#authorization-and-policy), [social admission and mail](../../../architecture/wyrd-security-posture.md#social-admission-account-linking-and-application-mail), [platform sessions](../../../architecture/wyrd-security-posture.md#platform-console-browser-sessions), [stored secrets](../../../architecture/wyrd-security-posture.md#stored-secrets), [Bifrost](../../../architecture/bifrost-design.md), and [deployment/setup configuration](../../../architecture/operations/deployment-and-release.md#human-setup-and-acquisition-configuration).
- Contracts: [access](../../../crates/wyrd-spec/src/auth/access.rs), [principal projections](../../../crates/wyrd-spec/src/auth/tenant_principals.rs), [human connections](../../../crates/wyrd-spec/src/auth/human_connection.rs), and [social contracts](../../../crates/wyrd-spec/src/auth/social.rs).
- Behavior: [authentication owners](../../../crates/wyrd/wyrd-auth/src), [SQL authorization](../../../crates/wyrd/wyrd-sql/src/queries/auth), [platform identity SQL](../../../crates/wyrd/wyrd-sql/src/queries/platform/identity.rs), [shared principal client](../../../crates/shared/wyrd-client/src/principals), and [UI host](../../../crates/wyrd/wyrd-server/wyrd-ui/src/routes).
- Proof: [platform/server journeys](../../../crates/wyrd/wyrd-server/tests/integration/platform_admin_e2e), [served contracts](../../../crates/wyrd/wyrd-server/tests/integration/pg_openapi_contract.rs), [production UI journeys](../../../crates/wyrd/wyrd-server/wyrd-ui/src/lib/ui-pages.integration.test.ts), [production provider journeys](../../../crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/production-auth.integration.test.ts), [Rust SDK](../../../sdks/wyrd-sdk-rust/tests/integration/principal_roles.rs), [Python SDK](../../../sdks/wyrd-sdk-python/tests/integration/test_principal_roles.py), and [TypeScript SDK](../../../sdks/wyrd-sdk-ts/wyrd/tests/integration/principal-roles.test.ts).
