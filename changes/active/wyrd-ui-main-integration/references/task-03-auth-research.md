# Signup, local login and personal-key research — Task 03

Repository source audited at `010ef9bab1932b299b8d72aae8f05fcfd7a3e15f`.
External primary guidance checked on 2026-11-02. Recommendations below are
proposed Wyrd policy until specification revision 19 is approved; no signup,
password or key behavior was implemented by this research.

## Existing workflow and gaps

| Capability | Reuse owner | Missing behavior / consequential boundary |
|---|---|---|
| Browser auth/session | `wyrd-spec/src/auth/browser.rs`; server `src/auth/browser.rs`; `wyrd-auth/src/browser_sessions.rs::BrowserSessions` | Only SSO and API-key authority currently. Add typed local-password signup/login and bind sessions to revocable local credential revision, retaining existing cookie/Origin/no-store/session owner. |
| User/password data | `wyrd-sql/src/queries/auth/users.rs`, `row_types/auth/users.rs` | Existing `password_hash` and auth_type scaffolding; no username or verifier. Tenant-local normalized username is a new persisted contract. Email uniqueness was intentionally removed for OIDC; do not restore it globally. |
| Password library | Workspace Argon20.5 and password-hash0.5 already installed | Extend owning auth crate with PHC verifier use. `wyrd-crypt::derive_key` is an encryption KDF, not a reusable login verifier. Unicode normalization is installed transitively but would need an explicit narrow dependency if adopted. |
| Signup/reset/guessing protection | No production invitation, reset or password throttle lifecycle found in scoped auth/server/SQL | Admission, recovery and bounded multi-replica abuse state need explicit contracts and a narrow SQL/auth extension. No Redis/auth framework justified. |
| User management permission | Existing `Permission::users_manage()` → users:write; role grants require existing wildcard admin authority | Use the appropriate human-administration permission, never OIDC-connection administration as signup authority. Signup cannot assign its own roles. |
| API-key generation | Existing principal credential issuer, SQL key verifier/prefix/lifecycle and existing token issuer | One 256-bit secret, SHA-256 verifier, one-time result already exist. Reuse; do not hash random API keys with the password KDF. |
| API-key durable owner | `queries/auth/service_accounts.rs::insert_api_key/api_key_by_prefix`; migration20260601000001/20 | Current owner FK and lookup join only service accounts, even though route-level require_principal recognizes Users. Extend same credential table/lookup to real User ownership with tenant-bound referential integrity; no surrogate Service account. |
| API-key token identity | `exchange_api_key.rs::ExchangeApiKey`; `issuance.rs::TenantTokenIssuer/TenantGrant` | Grant-family-based is_human chooses service table for API keys. Preserve verified owner kind through the shared resolver and issue a real User token with credential provenance and current roles. |
| Own-key authorization | Existing `/v1/principals/{principal_id}/credentials` admin owner | All operations currently require service_accounts:write. Ordinary User self-service needs an explicit owner-bound API/authorization contract, not broad admin grants. |
| Key label/scopes/expiry | Current tenant POST has no body; metadata has no label or per-key permission ceiling | New typed request/persisted fields needed for named, explicitly scoped and expiring personal keys. Preserve existing administrator/workload operations. |
| UI Settings | Current tenant Settings administers OIDC connections | Add personal account/key surface for any signed-in User while retaining separate tenant-admin SSO settings. Reuse session-bound BFF/SDK; no Node credential authority. |
| MCP | Existing principals MCP intentionally omits key issuance to avoid secrets in model transcripts | Keep create-secret operation out of MCP; expose metadata/revoke through existing owners and required scopes only. |
| Dev workflow | `mise.toml::dev:full`; `scripts/postgres/dev-server.sh`; existing setup/bootstrap/process owners | Seed real local human, disable fixture auth, check UI/login readiness and print entry/credentials before batch. Defaults 500/concurrency 8 already committed. |

## Primary guidance and selected proposal

[NIST SP 800-63B-4 final](https://csrc.nist.gov/pubs/sp/800/63/b/4/final), published
2025-07-31, and its [password requirements](https://pages.nist.gov/800-63-4/sp800-63b.html#passwords)
support a 15-character minimum for single-factor passwords, a generous maximum,
blocklisting common/compromised full passwords, no composition rules or routine
rotation, password-manager/paste support and rate limiting. NIST conformance is
a chosen security baseline, not a claim Wyrd has an assurance certification.

Proposed Wyrd password policy: 15–128 Unicode characters, NFC consistently at
creation/change/verification, no trimming/case folding, bounded input bytes,
no arbitrary character-class rules, bundled local common-password blocklist
with a documented source/license and maintenance owner, no external breach
service on signup's critical path. Exact username syntax/tenant uniqueness is
a product choice, not mandated by NIST; proposed tenant-local lowercase ASCII
handle3–64, separate from immutable User ID and optional SSO email.

[OWASP Password Storage](https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html)
recommends Argon2id; proposed starting parameters are memory 19456 KiB,
iterations 2, parallelism 1, fresh library salt and encoded PHC verifier. Use the
installed library verifier, bounded hash concurrency and rehash-on-success for
obsolete parameters. Store neither plaintext nor reversible password material.
A pepper/new vault is not required for this slice.

[OWASP Authentication](https://cheatsheetseries.owasp.org/cheatsheets/Authentication_Cheat_Sheet.html)
and [CSRF guidance](https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html)
support generic failure behavior, account plus source throttling, trusted Origin
checks even for login/signup and fresh post-auth session identifiers. OIDC
identity stays issuer+subject; matching email/username never silently links a
local account. MFA/account linking are separate outcomes, not silently added.

[OWASP recovery guidance](https://cheatsheetseries.owasp.org/cheatsheets/Forgot_Password_Cheat_Sheet.html)
requires expiring single-use securely generated reset tokens and ordinary login
after reset with existing sessions invalidated. Proposed initial recovery is
operator-assisted, since username-only signup supplies no verified email channel;
do not ship a fake email-based Forgot Password flow. Admin actions create reset
links without setting or seeing a user's chosen password.

## Personal-key patterns from existing developer products

| Primary source | Established pattern | Proposed Wyrd application |
|---|---|---|
| [GitHub personal access tokens](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens) | Named, expiring scoped tokens constrained by owner access | User-owned keys with explicit permission ceilings; current owner authority and selected scope intersect at token issuance. Existing short-lived bearer snapshot semantics remain intact. |
| [Hugging Face user tokens](https://huggingface.co/docs/hub/security-tokens) | Settings-based named tokens and separate per-purpose credentials | One shared personal Settings page for local and SSO User principals; independent revoke per key. |
| [Docker personal tokens](https://docs.docker.com/security/access-tokens/personal-access-tokens/) | Secret shown once; metadata/status/expiry/use management | One-time copy dialog, metadata-only lists, created/last-used/expiry/revoke. No plaintext recovery in page data, browser storage or logs. |
| [GitHub sudo mode](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/sudo-mode) | Protected credential actions use renewed authentication, not necessarily a local password | GitHub uses stronger renewed-auth verification. Wyrd proposes recent Rust-session creation after password verification or normal bound SSO completion; an existing IdP session may satisfy SSO. This does not claim forced provider credential re-entry or MFA. Never ask SSO-only users for a new local password. |
| [GitHub API credential security](https://docs.github.com/en/rest/authentication/keeping-your-api-credentials-secure) | Shortest necessary lifetime/least privilege | Proposed personal-key UI explicitly selects30 days, with shorter durations and maximum365 days. This is Wyrd policy, not a universal duration mandate. |

Preserve owning API-key semantics: default365-day general authority versus the
current90-day automation implementation is an existing inconsistency, not a
reason to silently change workload keys. Personal creation explicitly supplies
its bounded duration; no existing automation credential lifetime changes in
this task. Revocation stops new exchanges; already-issued tenant JWTs retain
existing short expiry/snapshot behavior. User suspension/tenant removal blocks
new exchanges. A password change ends local password browser sessions; keys are
independent credentials and require explicit revoke rather than an undocumented
rotation breaking automation. SSO-connection changes retain existing browser
revocation rules; disable User/revoke keys to end programmatic authority.

## Recommended admission model after caller clarification

The caller clarified that self-hosted installations generally have one tenant,
while SaaS teams receive separate tenants. Keep authentication shared and
separate tenant provisioning from account enrollment:

- Self-hosted: operator provisions the tenant and initial-owner enrollment;
  later people sign up by invitation. Never let the first anonymous visitor
  claim an existing installation.
- SaaS: protected onboarding provisions a distinct tenant for the team and
  authorizes initial-owner enrollment; later teammates join by invitation.
  Public self-service workspace creation may wrap that provisioning boundary,
  but it needs a separate verified-registration authority and delivery workflow.
- Dev:full: automate the self-hosted bootstrap with a real local human and
  generated password, preserving the same production credential/session code.

[GitLab initial sign-in](https://docs.gitlab.com/install/next_steps/) uses
operator-controlled initial administrator bootstrap. Its
[self-managed signup restrictions](https://docs.gitlab.com/administration/settings/sign_up_restrictions/)
make public registration/approval an administrator policy. For public SaaS,
[Slack workspace creation](https://slack.com/help/articles/206845317-create-a-slack-workspace)
verifies an email address before making its creator the new workspace's primary
owner, then invites coworkers. These support the distinction; they are product
patterns, not a universal standard requiring one deployment mode.

Repository reuse: server components/platform/provisioning.rs::TenantProvisioning
already authorizes a real PlatformCaller with tenants:write, claims/retries the
same tenant ID, establishes roles/headless admin/SYSTEM and built-in tables,
then promotes active. POST /platform/tenants, Platform::create_tenant, CLI
platform tenant create and server setup use this complete owner. A human owner
is a separate User, not that headless administrator. Signup cannot fabricate
PlatformCaller or give a browser tenant user platform authority.

No existing SMTP/mail verification or public-registration authority was found.
Task 03 therefore delivers real local user enrollment into protected-provisioned
workspaces in either deployment, plus dev bootstrap; it does not claim anonymous
SaaS self-service provisioning or invent email delivery as an incidental auth
helper. That would be separate platform scope. Development/Production profile
selects hardening/network policy and must not be overloaded as self-host/SaaS
admission. The proposed model still requires specification approval.
