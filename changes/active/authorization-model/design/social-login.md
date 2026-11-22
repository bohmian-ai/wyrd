# Social sign-in and signup

Status: finalized

The maintainer locked this addition to authorization-model on 2026-11-21,
intended as task 003. Tasks 001 and 002 are complete according to the maintainer.
This is the agreed design for specification, not implementation authority; it
preserves the revision-2 scope and completed evidence; approved revision 6 now owns the additions.


Revision note (2026-11-21): approved revision 6 and
[onboarding](onboarding.md) extend this design with verified local acquisition,
named human setup, verified-email workspace discovery, verified-contact reset and
a 15-character password minimum. All eight onboarding decisions are settled.
Invitations retain revision 4's single-use bearer-link admission without recipient
proof; this remains independent of mandatory public-acquisition contact verification.
Revision 5's abuse controls remain required.

## Problem

A person cannot use Google or GitHub to accept a Wyrd invitation or create a
workspace. A password User who independently signs in through company SSO gets
a second User instead of keeping their existing grants, groups and keys.

## Desired outcome

An invited person chooses an allowed sign-in method, receives exactly the
invitation's scoped access, and returns to the same tenant-local User. If the
operator enables signup, a visitor can instead create a new tenant and become
its administrator. An existing User can deliberately connect another method
without changing identity or access. A tenant can require company SSO.

## Constraints and non-goals

Maintainer decisions: social login is open source, without an enterprise gate;
Users remain tenant-local, with no platform account spanning tenants. One code
path serves self-hosted and SaaS deployments. The maintainer added a third task;
the earlier two-task limit does not constrain this new scope.
The maintainer also requires UI-based operator onboarding for social connections.

The maintainer approved explicit linking within a tenant: after proving both
methods, password, Google/GitHub and company SSO can reach the same User without
changing that User's grants, groups or keys. Accounts in different tenants stay
separate.

The maintainer approved public signup creating a new tenant with its creator as
tenant administrator, reusing existing tenant creation. Joining an existing
tenant through social login requires an invitation; returning login resolves an
existing identity. Creating the tenant grants administrator access, not merely
being the first person to sign into an existing tenant. Operator-created tenants
and administrator invitations remain supported alongside public signup.

Delivery scope: Google then GitHub.com within the addition, not a generic social
provider framework. No email-based account merging, automatic duplicate-account
merge, SCIM, continuous IdP offboarding, repository authorization, provider-token
vault or new billing system. Verified local public signup and self-service
verified-contact reset are included through onboarding; invited password enrollment
and administrator-assisted password reset remain supported.

## Agreed implementation

### Availability and provider ownership

The operator configures a Google web client and a separate GitHub sign-in OAuth
client through a platform administration UI backed by Rust management APIs.
The onboarding flow shows the exact callback URL and provider registration
instructions, accepts client ID/secret, tests sign-in, then explicitly enables
the tested configuration. Registering the OAuth application at Google/GitHub
remains an operator action in that provider's console.

Use platform-owned persisted provider metadata with secrets encrypted by the
existing sealing keyring, following platform OIDC's stored-secret pattern.
Read APIs omit secrets. The UI and any CLI/API client use the same management
operations; do not create competing environment and UI configuration authorities.
Keep the encryption key in deployment configuration, not in the onboarding UI.
Replicas observe provider revision changes through existing invalidation patterns.

Administration boundary: operator administrators manage shared social
providers and the SSO connection used to sign into platform administration;
tenant administrators manage only their tenant's company SSO and login policy.
Platform authority does not implicitly grant tenant administration. An operator
helping configure a tenant uses explicit tenant authority or the existing recovery
workflow; operator SSO is not an inherited SSO connection for every tenant.
Do not reuse the GitHub App installation credential: repository integration and
human authentication have different consent, access and lifecycle boundaries.
Keep clients and metadata caches process-owned, not constructed per request.

Social providers sit beside the tenant's one Active company OIDC connection;
they neither consume its slot nor create a connection row in every tenant.
The existing Candidate/test/activate lifecycle remains for company SSO.
Password is available by default; provide an operator password-enable setting
for deployments that offer only configured providers. Actual available methods
are operator configuration intersected with tenant policy. Return this safe
metadata from Rust to the login page; hiding a button never enforces policy.

Public signup is a separate operator setting, false by default. SaaS enables
it through configuration, with no SaaS-specific code. An enabled provider appears
on eligible tenant login/invitation pages; an SSO-required tenant offers company
SSO only. No provider configuration means no corresponding button.

### One callback workflow with an explicit purpose

Extend the existing Rust-owned login transaction to distinguish returning login,
invitation acceptance, new-tenant signup and linking. Bind purpose, provider,
provider configuration revision, tenant when present, browser, trusted origin,
and invitation or current User when relevant before redirecting. Callback query
parameters cannot choose or change any of these. Keep five-minute expiry,
single-use completion, server-held PKCE, state, OIDC nonce and fixed redirects.

Use fixed provider-specific social callback routes to distinguish operator
providers from arbitrary tenant-configured endpoints. This prevents expanding
the existing shared-callback mix-up exposure to the operator's social clients.
Retain company-SSO callback behavior unless its own flow needs a bounded change.
The external provider credentials are transient login inputs, never Wyrd API
credentials or browser storage. Provider disablement or revision change refuses
in-flight completion and invalidates sessions/refresh anchored to that revision.
All replicas must share the configured provider identity/revision semantics.

Google uses the existing OIDC relying party: validate signature, issuer,
audience/authorized party, expiry and nonce, and use `sub` as identity.
Request only `openid email profile`; request no Google API access or offline
refresh. Account/domain hints are presentation, never admission authority.
Compatibility with real Google has not been experimentally demonstrated.

GitHub uses authorization code plus S256 PKCE and state, followed by authenticated
`GET /user`; use the numeric user ID as the subject under a fixed GitHub.com
identity namespace, never the mutable login name. This is OAuth 2.0, not OIDC;
do not manufacture an ID token or apply JWT validation to its access token.
Fetch `/user/emails` with `user:email` only if verified contact email is needed.
No repository or organization scope is needed for sign-in. Drop the provider
token after identity resolution; do not retain a provider refresh token.

### Returning login and invitation acceptance

Returning social login looks up `(tenant, issuer/provider namespace, subject)`.
An absent identity refuses login and offers invitation/signup guidance; it
creates no User. A suspended/deleted User or inactive tenant remains refused.
Social login writes no role mappings and must not clear company-derived or
direct bindings. Existing company-SSO group mapping remains source-qualified.
Company SSO retains its existing just-in-time behavior unless explicitly revised;
the new no-automatic-creation rule applies to social admission.

Invitation acceptance reuses the current opaque, expiring, single-use invitation
and its scoped assignments. After provider proof, one tenant transaction
revalidates invitation, issuer authority, tenant policy and provider revision;
creates the User, attaches the unique identity, applies the assignments, consumes
the invitation and creates the session, or commits none of them. Concurrent
redemption cannot create two accounts or spend the invitation twice.

Invitations name the administrator's intended recipient. Possession of the live
single-use fragment link admits enrollment with any allowed method; a differing
provider email is permitted. Forwarding transfers the enrollment opportunity.
Opening consumes nothing. Recipient metadata never verifies a recovery address
or selects/links/merges an existing User. Separate authenticated contact verification
is optional after joining and never blocks offline invitation enrollment.

If the provider identity already belongs to a User, refuse this new-User
enrollment flow with guidance to sign in and request access. Never create a
duplicate or silently transfer an identity. Grants for an existing User continue
through the existing administrator binding flow.

### New-tenant signup

The agreed journey is: prove Wyrd email or Google/GitHub plus a verified contact → enter a workspace name →
Wyrd creates the tenant with that User as administrator → enter the workspace.
Workspace slug validation follows existing tenant creation. Signup must remain
enabled at completion. This purpose creates a new tenant only; it never joins
an existing tenant by email, domain, slug selection or provider organization.

Extend the existing tenant provisioning owner: claim the directory entry,
initialize builtin roles/SYSTEM/tables, create a tenant-local User with the
verified local credentials or external identity and a tenant-scoped admin binding, then promote the
tenant only after it is usable. Return an ordinary tenant browser session after
activation. Do not call the existing privileged endpoint with a fabricated
platform principal or return its administrative bootstrap key to a signup user.
One social account in two tenants produces two independent Users.

Provisioning already crosses platform and tenant transactions: do not promise
one cross-plane transaction. Bind retry/resumption to the original signup
attempt and proven identity; another caller cannot adopt a failed slug claim.
Callback replay creates no second tenant. Failure leaves no active ownerless
tenant. A lost session response can be recovered by signing into the completed
tenant rather than provisioning it again.

The implementation should be one additional human-owner initialization path in
the existing provisioner: local/email or social proof → workspace name/slug → existing tenant
initialization with this User as admin → ordinary sign-in. Keep existing
provisioning states and failure handling; no second onboarding service or new
general workflow engine. Reuse the existing lifecycle rather than rebuilding it.

Public acquisition follows [onboarding](onboarding.md#proofs-persistence-and-abuse)
and approved REQ-014: source before provider/mail work, trusted ingress plus IPv6
`/64`/missing-source refusal, sealed pre-proof cookies without pending rows,
`Retry-After`, keyring-derived HMAC counters and bounded purge, completion-only
owner crash recovery, atomic owned-workspace cap (default 3) and optional validated
Turnstile. Public local/social creation requires verified contact; invitation join
does not. Exact private admission windows remain implementation-owned. No global
person or deployment-wide total cap is added.

### Explicit linking and removal

In account settings, require fresh proof of the current User's existing allowed
method within the existing five-minute human-proof window, then prove the new
provider identity in the same bound linking transaction. An old cookie or
recovery-key session alone is insufficient. Atomically insert an identity for
that same tenant/User, or refuse if it belongs to another User. Concurrent
linking has one winner. Linking to company SSO runs its validated group mapping
for that connection without replacing direct bindings or local memberships.

No existing grants, groups, keys, workload attribution or principal IDs move.
Linking prevents future duplicates; it does not repair two accounts already
created. An already-linked target on the same User can report success without
creating another row. Password setup on a social-only account uses fresh proof
of its current allowed method and existing password validation, never email reset
as an implicit linking shortcut.

Require fresh proof through a remaining allowed method for removal. Refuse
removing the last usable method and end sessions/refresh anchored to the removed
identity. This requires anchors to include the identity association, not merely
the global provider. Keep last-usable-admin and operator recovery safeguards.

### Require company SSO

Use one tenant policy switch, enabled only with an Active tested company
connection and a freshly authenticated company-SSO administrator. Users link
their existing accounts before the switch; Wyrd must warn about accounts without
a usable company identity. Password/social starts and callbacks then refuse;
old password/social sessions and their CLI refresh chains also become unusable
under the established freshness bounds. Company-SSO invitations follow that
same policy, rather than offering a social enrollment bypass. Disabling policy
does not resurrect already-revoked sessions.

Keep Wyrd's existing API-key policy; the maintainer explicitly rejected reopening
it in this design. The maintainer approved operator-assisted recovery when SSO
breaks, consistent with the existing independent platform recovery boundary.
Reuse existing recovery mechanics; do not invent a replacement recovery system.
Service credentials are independent machine identities. Even with human SSO
required, upstream IdP removal is not an immediate Wyrd revocation signal:
existing company sessions, tokens, direct grants and local memberships can
survive until Wyrd observes a lifecycle change or they expire. Local suspension
is the existing immediate administrative action, subject to documented freshness.
Do not describe this switch as centralized continuous offboarding.

## Evidence and rationale

Repository inspection on 2026-11-21, not runtime verification:

- [Identity table](../../../../crates/wyrd/wyrd-sql/migrations/20260601000010_auth_federated_identity.sql)
  already keys tenant/issuer/subject and supports multiple identities per User.
  This preserves existing SSO behavior; it is not a new identity model or a
  finding that current SSO matches accounts by email.
- [Email uniqueness removal](../../../../crates/wyrd/wyrd-sql/migrations/20260925000001_auth_login_state_binding.sql)
  and [local credentials](../../../../crates/wyrd/wyrd-sql/migrations/20261102000000_local_users_personal_keys.sql)
  establish that two Users may share email; username is separately tenant-unique.
- [Callback](../../../../crates/wyrd/wyrd-auth/src/callback.rs), particularly
  `ensure_user_identity` and `finish_id_token_exchange`, currently provisions
  unknown identities and syncs company roles. Reuse verification/session
  mechanics, but branch admission by explicit purpose before that behavior.
- [Invitations](../../../../crates/wyrd/wyrd-auth/src/passwords.rs) carry scoped
  assignments and no recipient address. Reuse consumption/authority checks.
- [Provisioning](../../../../crates/wyrd/wyrd-server/src/components/platform/provisioning.rs)
  requires platform authority and creates an administrative credential; signup
  needs a deliberate bounded entry and human-owner result in that same owner.
- [Configuration](../../../../crates/wyrd/wyrd-server/src/config.rs) and
  [browser sessions](../../../../crates/wyrd/wyrd-auth/src/browser_sessions.rs)
  own secrets and session lifecycle. Node remains a projection/forwarder.
- [Platform identity management](../../../../crates/wyrd/wyrd-server/src/components/platform/identity.rs)
  already authorizes platform connection writes and seals persisted client
  secrets; reuse that pattern for UI-managed social providers.

Primary guidance checked on 2026-11-21:

- [OIDC claim stability](https://openid.net/specs/openid-connect-core-1_0.html#ClaimStability)
  and [Google OIDC](https://developers.google.com/identity/openid-connect/openid-connect):
  issuer/subject identifies the account; email is not a durable identity key.
- [OAuth security BCP](https://www.rfc-editor.org/rfc/rfc9700.html): authorization
  code, PKCE, redirect validation and issuer binding protect redirect flows.
- [GitHub OAuth](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps)
  documents code flow, PKCE and authenticated identity retrieval;
  [email API](https://docs.github.com/en/rest/users/emails) supplies verified flags.
- [OWASP authentication guidance](https://cheatsheetseries.owasp.org/cheatsheets/Authentication_Cheat_Sheet.html)
  supports reauthentication before sensitive account changes. Applying this to
  linking is the proposal's security inference, not a provider feature guarantee.

## Decisions and alternatives

Extend existing
auth, SQL, configuration and provisioning owners rather than add an identity
broker, platform-person account, per-tenant social connections or a provider
plugin framework. GitHub needs a concrete OAuth identity adapter because the
OIDC verifier cannot validate an identity token GitHub does not issue.

Use recipient-addressed, expiring, single-use bearer invitation links. They may
be copied without SMTP; possession admits one enrollment and forwarding transfers
that opportunity. No recipient-email proof/match is required. Separate contact
verification enables discovery/reset without changing invitation admission.

For onboarding ten or more people, support pasting multiple email addresses,
selecting scoped access once, and issuing a separate invitation to each person.
Provide pending, resend and revoke operations with per-recipient outcomes.
Mail delivery uses the operator-configured mechanism below; its
addition must be explicit in the spec. Company SSO already provides its
own first-login/group-mapping route. SCIM is for automated joiner/leaver lifecycle
management and remains outside this task; headcount alone does not require it.

Evidence: [Auth0 invitation email behavior](https://support.auth0.com/center/s/article/invitation-links-default-to-the-signup-prompt),
[Slack invitations](https://slack.com/help/articles/201330256-Invite-new-members-to-your-workspace),
[Microsoft emergency access guidance](https://learn.microsoft.com/en-us/entra/identity/role-based-access-control/security-emergency-access),
and [SCIM provisioning](https://learn.microsoft.com/en-us/entra/identity/app-provisioning/isv-automatic-provisioning-multi-tenant-apps).
These illustrate common practices; the recommended Wyrd scope is a design choice.

### Email delivery

Use one operator-configured SMTP relay for application mail on the deployment.
Self-hosters supply their company's relay or a transactional-mail provider;
SaaS configures its own provider through the same path. Email support ships in
open source; Wyrd does not bundle a public mail service or provider credentials.
No provider-specific API integrations are needed initially.

Operator setup supplies host, port, TLS mode, optional relay credentials and
sender address, with a test-send action. Reuse the existing secret-handling
pattern. Tenant administrators send invitations but cannot read or change relay
credentials. The maintainer approved SMTP delivery alongside recipient-addressed
invitation creation and bulk entry. Configure credentials through the existing
encrypted-secret pattern; expose no stored secret in read responses.

Resend is one possible SMTP provider, not a separate integration or delivery
requirement. Generic host, port, TLS, credential and sender settings accommodate
it and other compatible SMTP services. No provider SDK, preset, adapter or
provider-specific acceptance test is required. Operators perform any provider
account and sending-domain setup outside Wyrd.
Example: [Resend's SMTP documentation](https://resend.com/docs/send-with-smtp).

Wyrd can run without SMTP; the UI must explain that emailed invitations require
operator setup. Each single-use bearer link can be copied and delivered without SMTP;
there is no shared multi-use invitation. Report relay acceptance or send failure per
recipient; SMTP acceptance is not proof of inbox delivery. A send failure leaves
the invitation pending, creates no active User, and permits explicit resend.
Do not hold a tenant
transaction open while sending mail. Inspect existing background-work owners
before specifying any retry persistence or queue; no new queue is approved here.

Inspection on 2026-11-21 found no SMTP/mailer implementation or dependency in
the current crates, workspace manifests, architecture or docs searched. Email
delivery therefore needs an explicit addition rather than an assumed existing
capability. [GitLab's self-managed SMTP configuration](https://docs.gitlab.com/omnibus/settings/smtp/)
is an example of this open-source deployment model.
Keep company SSO's existing provisioning policy separate from social admission.
Clerk or other additional operator OIDC providers are outside this addition;
the existing tenant company-OIDC connection remains available independently.

## Standards conflicts

The maintainer approved the PostgreSQL platform-storage distinction on
2026-11-21. The owning security posture now records shared operator providers,
purpose-bound social admission, public signup through the existing provisioner,
identity anchors, bearer invitation enrollment, SMTP and tenant SSO policy. Its Postgres
section permits genuine platform/pre-tenant tables without `data_tenant_id`
while requiring forced RLS on every table. The human-sign-in architecture agrees.
No unresolved authority conflict remains for this design. Production code and
applied migrations were not changed; implementation must follow these owners.

## Open questions

No product or storage-boundary decision remains open. The maintainer approved
this PostgreSQL-specific distinction on 2026-11-21:

- Tenant-scoped PostgreSQL tables lead with `data_tenant_id` and retain
  tenant-qualified references.
- Genuine deployment-wide and pre-tenant PostgreSQL records may live in the
  existing platform plane without a tenant key, under narrowly authorized
  platform operations. No fabricated tenant is needed.
- Every PostgreSQL table enables and forces RLS, including platform tables.

This revises the broader every-new-table tenant-key rule for platform records;
it does not relax tenant isolation or grant signup visitors platform authority.
It does not define Bifrost or object-storage layout. Reuse existing owners before
adding physical tables.

Previously agreed shared providers, public signup, identity anchors, bearer invitation
enrollment, SMTP and company-SSO policy remain unchanged. The owning architecture and approved revision 6 record the approved storage
distinction and current onboarding behavior. The maintainer explicitly approved revision 3 on
2026-11-21.

## Handoff

Finalized design is recorded in [approved specification revision 6](../spec.md).
Authority reconciliation and explicit specification approval are recorded on
2026-11-21. [Task 003 carryover](../tasks/TASK-003-carryover.md) carries the remaining agreement
into an implementation proposal and ordered acceptance scenarios. Design
finalization alone did not approve the spec. Existing tasks and evidence are preserved.

Implementation reaches server auth/config/callback/provisioning, SQL metadata,
wire contracts and stable errors, client projections where exposed, login,
invitation/account/access-policy UI, device approval/refresh, fixtures and docs.
It preserves completed scoped-authorization work. Private module/type names,
config spelling and cache layout remain implementation-owned.

Proof must connect provider redirects through real Rust endpoints and repository
Postgres to UI/HTTP and CLI/SDK credentials. Exercise returning login; stranger
refusal without account creation; scoped invitation success, expiry, replay and
concurrent redemption; linking with preserved grants/keys, stolen-cookie and
identity-conflict refusal; two tenants with the same person; signup disabled,
throttled, replayed and failed midway; SSO-policy and provider revision changes
during callback and with existing browser/device/refresh credentials. Verify
social never clears company bindings and provider outages fail closed. Cover
permitted recipient mismatch and forwarded-link enrollment, optional separate
verified contacts, bulk
per-recipient results, SMTP absence/failure, resend and revocation. A local SMTP
test server proves message contents and send-failure handling through the generic
SMTP settings. No particular hosted email provider is required for verification.

Use controlled OIDC/OAuth servers for deterministic negative and concurrency
proof, plus a real Google/GitHub operator smoke test for actual registration,
consent and callback compatibility. The smoke does not replace journeys.
Discover exact focused checks in mise; existing identity and platform journey
lanes are relevant. Run required contract/export, format and lint checks when
implemented. If policy enforcement changes hot paths, enumerate every added or
moved wait and provide the required clean, VALID engine benchmark comparison.
