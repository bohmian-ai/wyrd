# Onboarding and account recovery

Status: finalized

The maintainer approved all eight decisions below exactly as recommended on
2026-11-21 and said “i approve the spec revision”. They are recorded in approved
[specification revision 6](../spec.md); the owning architecture sections are
reconciled with that approval. This document records the agreed design, not
implementation evidence.

## Problem and outcome

Today a provider-free visitor needs an operator-created workspace and an
administrator-created or invited User. Setup creates credentials but no human
administrator; root entry requires a workspace key; recovery always needs an
administrator; login renders unavailable methods; password-only authentication
allows eight characters. Staged social signup improves acquisition only after an
operator configures a provider and does not yet supply the public UI.

Make a fresh installation usable by its operator immediately. Let an operator
offer local self-service workspace creation without buying or configuring social
authentication. Let returning people recover workspace links and passwords through
verified email when mail is available. Every successful journey ends in an
ordinary tenant-local User and session with the existing authorization rules.

**Default configuration deliberately remains private:** password enabled, public
signup disabled, no SMTP or social credentials. An anonymous visitor cannot create
a workspace under these defaults. Setup can. Public local signup requires enabling
signup and configuring SMTP; no design can both prove mailbox ownership and send
no mail without another trusted proof source. This is the approved default,
not a claim that anonymous zero-configuration creation works. If anonymous creation
under untouched defaults is required, this recommendation needs a different
maintainer decision; it must not silently weaken verification.

## Constraints and non-goals

Rust owns durable authentication, provisioning and recovery; `wyrd-spec` owns
typed contracts. UI, CLI, HTTP, MCP and SDK projections consume those owners.
Self-hosted, SaaS and single-tenant installations use the same settings and code.
Keep tenant-local Users, independent passwords, grants and sessions. No global
person account, identity broker, automatic email linking, OAuth password grant,
billing work, new mail queue or automatic retry service is needed.

Customer copy says workspace; its identity remains the existing tenant ID/slug.
Platform authority and tenant authority remain separate. Authentication is control
plane work; this proposal adds no wait to verification, gateway, Oracle or Scribe
hot paths. Permission audits still stage on the process Scribe outbox.

## Six corrections and their conventional basis

| Deviation | Convention and concrete Wyrd proposal |
|---|---|
| No local public signup | [GitLab local registration](https://docs.gitlab.com/user/profile/account/create_accounts/) and [Keycloak registration](https://www.keycloak.org/docs/latest/server_admin/index.html#_user-registration) support local accounts independently of federation. Add verified-email/password acquisition to the existing signup/provisioning flow. Keep signup off by default, as [Gitea's registration setting](https://docs.gitea.com/administration/config-cheat-sheet/#service-service) does. Verification is mandatory for public local acquisition, not an optional weak mode. |
| Setup creates no human admin | [GitLab initial sign-in](https://docs.gitlab.com/install/next_steps/) establishes an immediately usable administrator. Extend operator-authorized setup to establish a named tenant-local human admin as part of provisioning, alongside its existing automation/recovery credentials. Never give the first anonymous visitor administrative authority. |
| Root requires remembered tenant key | [Slack email-first sign-in](https://slack.com/help/articles/212681477-Sign-in-to-Slack) demonstrates verified-email workspace access. Root offers email-based workspace discovery, direct workspace URL/key entry and available creation methods. Discovery reveals links only after proof; each workspace still requires its own allowed login. This is deliberately narrower than Slack's email sign-in. |
| Admin-only reset with SMTP | [OWASP Forgot Password](https://cheatsheetseries.owasp.org/cheatsheets/Forgot_Password_Cheat_Sheet.html) recommends a generic request followed by a short-lived, single-use recovery secret. Add email reset requests for existing password Users with verified recovery addresses; reuse code redemption and password-revision invalidation. Keep administrator-issued codes for offline recovery. |
| UI ignores method flags | [Keycloak registration/login configuration](https://www.keycloak.org/docs/latest/server_admin/index.html#_user-registration) and Gitea condition their login presentation on configuration. Consume the complete Rust methods response; show password, company SSO and each social provider only when advertised. Rust rechecks policy on every start and completion. |
| Eight-character password-only minimum | [NIST SP 800-63B §3.1.1.2](https://pages.nist.gov/800-63-4/sp800-63b.html#passwordver) requires fifteen characters for single-factor passwords; [OWASP Authentication](https://cheatsheetseries.owasp.org/cheatsheets/Authentication_Cheat_Sheet.html) gives the same guidance without MFA. Use 15–128 Unicode characters after existing NFC normalization, existing blocklist and Argon2id. No composition rules, truncation or periodic forced rotation; permit paste/password managers. Apply one validator to setup, creation, invitation, change, reset and adding a password. |

These products establish conventions, not Wyrd's exact routes, durations or
authority model. The details below are the approved Wyrd choices.

## End-to-end journeys

### Fresh self-hosted installation

The operator runs `wyrd-server setup --tenant acme --server https://wyrd.example`
with a new `--admin-username alice` input and a password supplied through a hidden
terminal prompt or `--admin-password-stdin`. An optional `--admin-email` records
an unverified contact. Never accept a password in an argv flag or environment
dump. Non-interactive setup without human-admin inputs fails before claiming the
tenant; automation supplies stdin explicitly.

Extend the existing provisioner's human-owner path to accept local credentials;
reuse roles, SYSTEM, builtin tables and claim/initialize/promote handling. Setup
also retains its existing bootstrap administrative credential result for automation
and recovery. Activation requires a usable human administrator, not just a key.
Return the workspace login URL and username, never the password. The operator
signs in without SMTP or an IdP. Setup does not mark the optional email verified.
Rerunning a completed setup returns its safe status and never resets a password
or creates another admin. An interrupted attempt resumes through existing
operator-authorized provisioning recovery; it cannot activate ownerless.

### Public visitor, defaults, SMTP and social methods

With untouched defaults, root offers Sign in and workspace-key entry, explaining
that new workspaces require an operator. There is no dead Create workspace button
and no tenant write. The installer uses setup above; an unrelated visitor asks
for an invitation. This preserves conventional private-install defaults.

With signup enabled and SMTP configured, root offers Create workspace → enter
email → receive verification message → submit its proof in the initiating browser
→ choose workspace name/slug, local username and password → confirm. No password
is collected before email proof. Rust admits requests, verifies the proof, records
one proven creation attempt and runs `TenantProvisioning`. The tenant gets its
own User, Argon2id credential, verified recovery address and admin binding. Only
after promotion does the response set an ordinary password-anchored session and
return the workspace key. Email proof is acquisition authority for this attempt,
not a permanent platform account. A claimed slug returns a conflict; choosing
another slug before a claim does not require another mailbox message.

With signup enabled but no SMTP, local public signup is unavailable and explains
that email delivery is required. SMTP secrets are not bundled with Wyrd. If no
social provider is configured either, no public creation method is advertised;
operator settings show the missing prerequisite. Invitations and setup still work.

Configured Google/GitHub remain additional acquisition and login methods. Reuse
the staged provider callback and completion cookie. Provider identity proves the
social account, not an arbitrary email: accept a Google `email_verified` claim
only after OIDC validation. GitHub signup may obtain a verified address through
its authenticated [email API](https://docs.github.com/en/rest/users/emails), requiring
`user:email`; otherwise ask for a separate Wyrd email challenge. Public signup
requires a verified contact before provisioning by either method. A provider
that supplies it permits social creation without SMTP. Returning social login
and invitation acceptance do not gain an email prerequisite. Store issuer/subject
as identity, never use email to find or attach another User. Do not require a
password for social-created Users. Adding one later follows REQ-015's fresh proof.

### Invitation join

The administrator issues the existing recipient-addressed invitation. With SMTP
it is sent; without SMTP the administrator copies the returned fragment link.
Opening it consumes nothing. The recipient enrolls with an advertised method;
`POST /auth/browser/password/signup` remains invitation-only. Rust atomically
consumes the invitation, creates the tenant User and scoped bindings, and creates
the session. No extra personal workspace is created.

Keep revision 4's bearer-link admission: forwarding the live link transfers its
enrollment opportunity. The intended recipient address is not verified merely
by possession. After joining, an authenticated User can verify a recovery address
separately. This optional step never blocks offline invitation enrollment. An
SSO-required workspace offers only company SSO; it gets no password fallback.

### Returning sign-in and forgotten workspace

On a new browser, a person who knows the workspace opens `/t/acme/login` and
uses that workspace's local username/password, linked social identity or company
SSO. Independent Users can have different passwords in different workspaces.

At root, Find my workspaces requests email proof. The same generic response is
returned for known and unknown addresses. After consuming the browser-bound
proof, Rust returns only active workspace links associated with verified recovery
contacts for that exact address. Selecting a link starts that workspace's normal
sign-in; it creates no session, User, grant or account link. Multiple Users sharing
a verified address may yield one deduplicated workspace entry; do not expose their
usernames or principal IDs. No membership search by public email or domain exists.

Without SMTP the discovery method is absent, even when social login is configured;
use a saved URL, a copied invitation or an administrator. Without a verified
contact the directory has no result; show that limitation during contact setup.
Existing live-session enumeration remains independent and can offer already
signed-in workspaces without sending mail.

### Forgotten password and CLI login

With SMTP, `/t/acme/login` offers Forgot password only for a workspace permitting
password auth. Submit workspace + username; Wyrd sends a reset code only to that
User's verified recovery address. The public response discloses neither account
existence nor delivery outcome. The email contains the workspace URL and a code
to paste, preserving the existing rule that reset codes never appear in URLs.
Redemption at `/t/acme/reset` changes the existing password and invalidates its
local sessions/CLI refresh chains. It never signs in, creates a password for a
social-only User, changes API keys or overrides required SSO. Notify the verified
address after success without including a password.

Without SMTP or a verified address, the same page explains administrator-assisted
recovery; an administrator uses the existing fifteen-minute reset-code operation.
Lost company/social authentication follows that provider's recovery or Wyrd's
existing operator recovery boundary, not email-based account linking.

`wyrd auth login --server URL --tenant acme` keeps the existing
[RFC 8628 device flow](https://www.rfc-editor.org/rfc/rfc8628.html): open the browser,
sign in with an allowed method, inspect and approve the device code, then let the
CLI save its tenant-bound credential. SMTP and an IdP are unnecessary for local
password approval. A forgotten key uses discovery first, then the unchanged CLI
command. No password grant, browser token handoff or new CLI identity is added.

## HTTP contracts and configuration

The approved shapes below are new/revised contracts, not claims about mounted routes.
Bodies deny unknown fields; secrets use existing secret types. Browser mutations
require trusted Origin and existing CSRF/cookie protections. Responses and pages
carrying proof or workspace results use `Cache-Control: no-store`.

| Route | Request → response and authority |
|---|---|
| `GET /auth/browser/methods?tenant=acme` (tenant optional) | Retain `{password, company_sso, providers, signup}`; add `{local_signup, email_discovery, password_reset, email_verification_required:true, signup_providers:[...]}`. `providers` means returning/invitation methods; `signup_providers` means eligible creation methods. Without tenant, password is not a global login promise; root uses discovery/key entry. `signup` means at least one creation method is usable now, not merely that its operator switch is on. No secrets or account counts. |
| `POST /auth/signup` | Tagged union `{method:"email", email}` or `{method:"social", provider}` replaces mandatory provider-only input. Email: `202 {status:"check_email"}`, sets sealed flow cookie. Social: `200 {authorization_url}`, sets existing sealed provider-check cookie. Local requires password enabled, signup enabled and SMTP; provider paths enforce configured enablement. |
| `POST /auth/signup/verify` | `{proof}` + initiating cookie → `200 {status:"proven"}` and opaque completion cookie. Proof comes from an email fragment link or pasted token, never a password-bearing URL. Social callback reaches the same proven-attempt state. A provider lacking verified contact uses the email branch bound to that provider attempt before completion. |
| `POST /auth/signup/complete` | `{slug, display_name, local_credentials?:{username,password}, challenge_token?}` + completion cookie → `200 {tenant}` + ordinary tenant session cookie. Credentials required for local proof and forbidden for social proof. Email, issuer, subject, role and tenant ID come from server-held proof. Repeat successful completion returns the same tenant; no second creation. |
| `POST /auth/browser/workspaces/request` | `{email}` → `202 {status:"check_email"}` and sealed browser binding; uniform for known/unknown addresses. SMTP required. Mail goes to the submitted address with no workspace names. |
| `POST /auth/browser/workspaces/verify` | `{proof}` + binding → `200 {workspaces:[{tenant,display_name,login_url}],next_cursor?}`. Bounded page, same short-lived discovery cookie for pagination via `GET /auth/browser/workspaces?cursor=...`. Result URLs are fixed same-origin workspace-login paths; proof grants directory read only. Empty result is normal after proof. |
| `POST /auth/browser/password/reset/request` | `{tenant,username}` → `202 {status:"check_email"}` for every syntactically valid admitted request. Send only for an active eligible password User with verified contact. SMTP failure is not exposed as an account-specific error. |
| Existing password routes | `/signup {tenant,invitation_code,username,password}` and `/login {tenant,username,password}` establish their existing session; `/change {tenant,current_password,new_password}` and `/reset {tenant,reset_code,new_password}` retain existing response contracts and invalidation. Reset never signs in. |
| `POST /auth/browser/email/request` and `/verify` | Fresh allowed-method tenant session: `{tenant,email}` → `202 {status:"check_email"}`; `{tenant,proof}` → `204`. Bind proof to that User and purpose. On verification atomically replace the verified contact/revision and directory-update intent; notify the old verified address. No grant, identity or password change. |
| Existing platform tenant creation and settings | Extend typed create-tenant input with optional `{human_admin:{username,password,email?}}`; setup requires it, existing machine-only administrative creation may omit it. Password never appears in response. Reuse existing platform login-settings and mail management operations, not a parallel setup API or configuration source. |

New failures enter the stable Wyrd error catalog: distinguish disabled method
(`403`), invalid/expired proof (`400`), unavailable dependency (`503`), claimed slug
(`409`) and admission throttle (`429`, `Retry-After` header and equal whole-second
problem field). Preserve `WYRD_AUTH_403_SIGNUP_WORKSPACE_LIMIT`. Account-sensitive
request routes keep their generic `202`; a deployment-wide SMTP outage may return
uniform `503`, never condition that response on an account lookup.

Use existing operator-managed settings and secret storage:

| Setting | Recommended default and enforcement |
|---|---|
| `password_enabled` | `true`; tenant policy can narrow it. Disabling also disables local public signup and local reset execution. |
| `signup_enabled` | `false`; enabling permits only methods with valid prerequisites. Rechecked on completion. No SaaS-only switch. |
| Public email verification | Required policy, not a bypass setting. Setup/admin-created and bearer-invited accounts remain permitted without verified email, but lack email discovery/reset until verification. |
| SMTP relay | Absent. Existing host, explicit port, TLS mode, sender and optional sealed credentials; recommend validated STARTTLS or TLS, plaintext only for an explicitly trusted local relay. Test-send before use; no built-in mail credentials. |
| Shared social providers | None; existing tested/enabled metadata and revision lifecycle. A provider can remain a returning-login method without being usable for acquisition. |
| Trusted ingress/source, Turnstile, owned-workspace cap | Retain REQ-014 revision-5 settings and defaults, including optional Turnstile and cap 3. No new challenge platform or deployment-wide cap. |

Settings remain revisioned platform administration, with current policy rechecked
at execution. Removing SMTP disables mail-dependent starts, not ordinary password
login or redemption of already-issued valid reset codes. Existing signed proofs
remain usable to their deadline if signup and their method remain enabled.

## Proofs, persistence and abuse

Reuse sealed flow cookies, purpose-bound secrets, the auth maintenance owner and
shared admission counters. Email proofs expire after fifteen minutes;
the signup completion proof retains the existing five-minute lifetime. Discovery
read cookies expire after five minutes. These approved durations are Wyrd choices,
not standards mandates. PostgreSQL derives persisted deadlines from durations
using `statement_timestamp()`; pre-proof sealed envelopes use the existing
server clock anchored to PostgreSQL.

Email initiation stores no pending User, tenant or signup-attempt row. It sends
an authenticated encrypted envelope containing a random identifier, normalized
address, purpose, expiry and initiating-browser binding; store no password in it.
After successful email proof, insert a unique proof-identifier digest in the
existing proven-attempt owner. Unique consumption prevents replay across replicas;
retaining spent identifiers until proof expiry is necessary for email tokens,
because email has no provider enforcing one-use authorization codes. A lost
verification response can resume only with the same bound browser. Separate
signup, discovery, contact-verification and reset purposes are not interchangeable.

Keep original email for delivery; trim surrounding input whitespace, canonicalize
the domain and preserve local-part case. Do not collapse dots, plus tags or
provider aliases. Use the same comparison at registration, verification and
discovery, following [OWASP email comparison guidance](https://cheatsheetseries.owasp.org/cheatsheets/Email_Validation_and_Verification_Cheat_Sheet.html).
This conservative policy can miss a differently cased address; it cannot silently
merge accounts. Social namespace/subject remains independent of email.

| Approved revision-6 persistence purpose | Boundary and reuse |
|---|---|
| Proven email signup/discovery/contact proof and one-use redemption | Genuine pre-tenant platform records may omit `data_tenant_id`, but enable and force RLS and permit only the existing platform owner. Extend proven signup state; no pre-proof pending row except shared abuse counters. Retain only bounded proof/consumption metadata, expiry, browser digest and necessary temporary address. No global person or global password store. |
| Tenant-local verified recovery contact | Every tenant table leads with `data_tenant_id`, forces RLS and uses tenant-qualified User references. Track address, verification time and replacement revision separately from unverified profile/invitation email. Duplicate emails remain allowed across and within tenants. Reuse User/auth owners rather than changing identity uniqueness. |
| Workspace discovery directory | Minimal platform control-plane projection: HMAC of canonical verified email → tenant ID and User reference/contact revision. Needed because tenant-RLS connections cannot scan every tenant at root; not an authorization or identity table. No passwords, grants, sessions or evidence. Tenant transaction updates a contact revision and notifies the existing reconciliation machinery; reconciliation reads through authorized tenant connections and updates/removes projection entries. Before revealing any result, revalidate current verified contact, live User and live tenant through those owners; stale or unavailable validation fails closed. Bound pages and cleanup. |
| Recovery and admission metadata | Reuse tenant password-reset verifiers/revisions and shared counters, adding email-purpose/source HMAC scopes and spent-proof retention where needed. No raw reset secret or mail body in PostgreSQL; new purpose/index/constraint requirements must be named before implementation. |
| Provisioning and creator accounting | Existing tenant claim/proven-attempt ownership, initialization and activation. Generalize human owner to local credential or external identity. Keep creator HMAC after proof purge for cap enforcement; use verified email namespace for local creators and provider namespace/subject for social creators. Cap is per proven identity, not per human across methods. |

The discovery projection is the only new cross-tenant lookup purpose. It is
needed for verified-email discovery, not a reason to add a global account. Its
eventual update may temporarily omit a new workspace; stale entries must never
disclose a revoked association. Disabled/deleted Users, changed contacts and
retired tenants disappear upon validation even before reconciliation. Contact
replacement invalidates outstanding reset proofs bound to the old revision.

Preserve every revision-5 signup control: derive source from peer or explicitly
trusted ingress, normalize mapped IPv4 and group IPv6 by `/64`; refuse an absent
source; source admission before provider discovery, mail or hashing; separate
completion source/identity windows; unavailable shared admission fails closed;
bounded HMAC-SHA-256 counter keys derived from the sealing keyring; bounded expiry
cleanup; optional server-validated Turnstile after admission and before claiming;
atomic owned-workspace cap; completion-cookie-only stale-claim recovery. Extend
identity accounting to verified local email. Never silently unify social identities
by email to defeat duplicate creation. Multiple identities remain a known cap
limitation, as with multiple social accounts.

Apply independent source and address/account budgets to verification sends,
discovery and reset requests, including resend, before SMTP or Argon2id work.
Reuse the shared limiter; implementers select measured windows within the revised
spec's bounded-admission purpose. Do not lock an existing account because someone
requested resets. Generic response bodies, comparable timing distributions and
recipient-specific throttles must not expose existence; source throttles can
return `429`. No unauthenticated response returns membership counts or reveals
whether mail was sent. Public workspace creation proves a mailbox, not whether
that mailbox already belongs to any tenant.

Send through REQ-017's generic SMTP owner after transactions, outside locks.
Explicit retries regenerate bounded proofs; no durable plaintext spool or automatic
queue. Unknown discovery addresses can receive the same neutral proof message;
membership is queried only after proof. Avoid account-dependent synchronous SMTP
timing on reset requests: use bounded process-owned send execution with the same
public response path for all accounts. A crash can lose that send; request again.
This is not a promise of durable delivery. Authenticated operator test-send and
invitation issuance retain honest relay outcomes; recovery endpoints do not.

Retain production TLS, host-only Secure/HttpOnly/SameSite cookies, trusted Origin,
fixed destinations and secret redaction. Mail links carry proofs in fragments,
not query strings; GET/opening a link consumes nothing. Page code posts proof
then clears the fragment. Reset codes remain manual input. Never log passwords,
codes, addresses or proofs; counters use keyed pseudonyms. Reuse Argon2id's
existing bounded blocking strategy. Social redirects retain
[RFC 9700](https://www.rfc-editor.org/rfc/rfc9700.html) state/PKCE and issuer binding.

## Required authority and implementation changes

These reconciliations are authorized by the maintainer’s 2026-11-21 approval and recorded in specification revision 6.

| Owner | Required reconciliation |
|---|---|
| [Human sign-in](../../../../architecture/wyrd-design.md#human-sign-in) | Replace “setup creates no User” and the public-password-signup prohibition with named human setup and verified local acquisition. Retain the OAuth password-grant prohibition. Specify discovery, verified contacts and self-service reset requests alongside existing redemption. |
| [Security posture](../../../../architecture/wyrd-security-posture.md#social-admission-account-linking-and-application-mail), first-party browser sessions, credentials and Postgres tenant binding | Extend social-only acquisition and five-minute proof ownership to local email; raise minimum to 15; define verified-contact and discovery purposes, proof replay protection, reset enumeration controls and platform projection RLS. Permit narrowly scoped GitHub email retrieval when acquisition needs it. Preserve revision-4 invitation behavior and all revision-5 controls. |
| [Operations configuration](../../../../architecture/operations/deployment-and-release.md#configuration-authority) and setup/deployment guides | Document setup inputs, secret stdin, safe reruns, private defaults, missing-SMTP behavior and verification/recovery limitations. No new Node secret or second config authority. |
| [Spec](../spec.md) REQ-002 | Keep tenant-bound RFC 8628 and local approval; add onboarding-to-device-login regression observations, not a new grant. |
| REQ-011/012 | Return complete method/capability metadata and enforce it on all surfaces; distinguish returning providers from acquisition prerequisites. Retain tenant-local identity and no automatic linking. |
| REQ-013/015/016 | Keep bearer invitation enrollment, explicit linking and required-company-SSO safeguards. Add optional verified-contact enrollment; prohibit reset from adding a password or bypassing SSO. |
| REQ-014 | Add verified local signup and operator human setup, generalized owner proof, exact mail/proof contracts and local creator accounting; preserve revision-5 source, counters, challenge, cap and crash recovery. |
| REQ-017 | Extend SMTP beyond invitations to signup/contact verification, discovery, reset and reset notification; distinguish generic public responses from operator delivery outcomes and explicitly permit bounded transient reset sends. Name persistence purposes and constraints above, with new acceptance observations. |
| Excluded list | Remove public password signup from Excluded. Keep cross-tenant person accounts, automatic merging, OAuth password grant, new mail queues and provider-token vaults excluded. Explicitly permit this bounded discovery projection and email proof metadata; they are not global identity accounts. |
| [Social design](social-login.md) and design notes | Replace social-only acquisition scope, admin-only recovery and stale pre-revision-5 abuse text. Remove withdrawn recipient-proof paragraphs rather than leave competing guidance. Reconcile “no email API access” for GitHub acquisition. Preserve invitation-link and explicit-linking decisions. |
| Staged contracts/auth/provisioning/SQL | Generalize `SignupBeginRequest`, completion and `HumanOwner` beyond mandatory social provider. Reuse `TenantProvisioning`, password validation/reset and session owners. Bring staged `begin_signup`/SQL into revision 5: no pending pre-proof row, trusted-source start/completion admission, HMAC keys, creator cap and crash recovery. The staged revision-4 migration is evidence of current shape, not authority to add these schema purposes or edit an applied migration. |
| UI and all client projections | Root creation/discovery/key entry; public signup and workspace-confirmation pages; verification/contact/reset-request views; full methods flags on login/invitation/device pages. Remove unconditional SSO/password controls. Expose prerequisites and offline recovery honestly; UI owns no auth state machine. Thin HTTP/CLI/SDK operations share Rust owners; MCP exposes safe method/settings metadata and authorized management with explicit write scopes, never passwords/reset secrets in tool results. Keep human proof/device approval in its deliberate auth channel. |

No shipped data migration, backfill, dual contract, legacy alias or rollout bridge
is required: nothing shipped. Schema implementation still requires an approved
spec naming every new/changed table purpose, column, index and policy; never edit
an applied migration. This design alone authorizes none.

## Acceptance observations and evidence

Exercise real SDK → `WyrdTestServer` + repository PostgreSQL → SDK journeys on
each shipped programmatic surface, plus browser journeys through Rust. Use a
local SMTP test relay and provider fixtures; no external mail account is required.

- Fresh setup without SMTP/IdP yields a named human admin who signs in and
  approves CLI device login. Missing non-interactive credentials create nothing;
  crash/rerun cannot duplicate an admin or activate an unusable tenant.
- Untouched defaults refuse anonymous workspace creation and advertise no signup.
  Enabling signup plus SMTP permits local email proof → password → one active
  workspace/admin/session. Before proof there is no User, tenant or pending signup
  row; bounded admission metadata is expected. No bootstrap key reaches the visitor.
- SMTP absent hides local creation/discovery/reset request while invitation,
  password login, admin-issued reset and CLI login work. Configured social signup
  without SMTP succeeds only with a verified provider contact. Social-only Users
  remain password-free; invitation acceptance never creates an extra workspace.
- New-browser discovery discloses nothing before email proof, then only verified
  active workspace links. Shared email does not merge Users; direct login preserves
  separate passwords/grants. Revoked contacts, disabled Users, stale projection
  rows, cross-tenant IDs and unavailable validation cannot reveal workspaces.
- Reset known/unknown/unverified accounts has indistinguishable public responses
  and timing distributions. Single-use/expired/replaced/replayed codes and contact
  revisions behave correctly. Success invalidates local sessions and CLI renewal,
  leaves keys alone, requires subsequent login and cannot bypass required SSO.
- Methods flags govern every rendered control and Rust enforcement even when
  configuration changes between page load and POST. Test no providers, password
  disabled, SSO required, unavailable SSO and no usable methods.
- Fourteen-character normalized passwords fail everywhere; fifteen-character
  passphrases pass unless blocklisted; 128-character Unicode and paste work;
  oversized values fail without truncation. No password/code/proof enters logs.
- Across replicas, replayed email/provider proof, concurrent completion, lost
  session responses and interrupted claims yield one owned tenant. Exercise every
  revision-5 abuse control, source spoofing, `/64` grouping, counter failure,
  `Retry-After`, challenge failure and atomic cap; check bounded cleanup and RLS.

Evidence: the requested prior diagnosis was read first at
`/tmp/claude-1000/-home-thorrester-Documents-GitHub-wyrd/8bfbba4d-fa9a-4465-9fa3-41bdee7ffde1/scratchpad/codex-onboarding.md`.
Repository evidence inspected on 2026-11-21 includes the working-tree spec/security
authority, `git diff --cached`, CodeGraph sources for signup, provisioning, browser
methods and setup, and UI root/login loaders. Primary documentation above was
checked that day. This is source-based design; no server journey or runtime test
was executed.

Read-only enterprise input: sections 3 and 8 of
`/home/thorrester/Documents/GitHub/wyrd-enterprise/changes/active/enterprise-readiness/design/notes.md`
support verified acquisition before tenant creation and explicit create-versus-join.
Its commercial account is not a Wyrd person identity and supplies no authority
here. Its mandatory hosted Turnstile recommendation does not replace revision 5's
optional core setting; an operator may configure it through that existing path.

## Decisions for the maintainer

All eight recommendations were accepted exactly by the maintainer on 2026-11-21. They are settled decisions.

1. **Default access:** retain `signup_enabled=false`, password enabled, no bundled
   SMTP/providers. Enable public local signup with SMTP explicitly. This closes the
   missing capability while keeping private installations private. If anonymous
   creation under untouched defaults is mandatory, decide explicitly between
   unverified local creation and requiring an external proof service; recommend
   neither over the verified design above.
2. **Public verification:** require a verified email contact before any public
   local/social tenant creation; accept validated provider proof or Wyrd mail.
   Permit setup/invitations without email verification. Request GitHub `user:email`
   only where acquisition needs it. This keeps offline join usable and follows
   OWASP's ownership guidance for public acquisition.
3. **First administrator:** require setup's human username and secret stdin/prompt,
   reuse provisioning, retain separate automation/recovery credentials and never
   assign authority to the first anonymous visitor. Prefer this direct path over
   inventing another bootstrap enrollment token/workflow.
4. **Discovery:** retain tenant-local Users; add the minimal verified-email
   directory projection, never a global person account. Proof reveals workspace
   links only; require normal tenant authentication afterward. Accept temporary
   omission while projection catches up, with current revalidation before disclosure.
5. **Recovery/contact:** add self-service reset requests to verified tenant-local
   contacts, keep admin-issued codes and code-not-URL redemption. Require username
   within a workspace because email is not unique. Use explicit contact verification
   after invitation/setup; never infer it from recipient metadata.
6. **Presentation:** consume and enforce Rust's complete methods contract, including
   acquisition/mail capabilities; remove unconditional login buttons. No auth
   decision belongs in Node.
7. **Password policy:** adopt 15–128 normalized characters for all local password
   operations, with existing blocklist/Argon2id; do not add MFA in this change.
8. **Mail/proofs/persistence:** approve the bounded email-proof, contact, discovery
   and recovery-admission purposes before schema work, fifteen-minute email proofs,
   existing five-minute completion proof and bounded transient reset sends. Keep
   no pre-proof signup row, raw mail spool or new queue. Preserve revision-5 abuse
   controls and accept that per-identity caps do not deduplicate humans across methods.

## Handoff

The approved agreement is [specification revision 6](../spec.md). Remaining
implementation and ordered test-first acceptance cycles belong to
[TASK-003-carryover](../tasks/TASK-003-carryover.md), preserving TASK-003's completed
evidence. No implementation, Git action or enterprise change follows from this
documentation update. Private layout and measured admission windows remain
implementation-owned within the approved purposes. New-page mock extension and
maintainer approval remain prerequisites to production UI implementation.
