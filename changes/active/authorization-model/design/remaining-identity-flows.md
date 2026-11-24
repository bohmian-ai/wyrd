# Company SSO and connected launch verification

Status: finalized

The maintainer approved deferring carryover cycles 17–18 and accepted the
recommendations for cycles 19–20 on 2026-11-23, requesting one new execution task.
This document replaces the earlier proposal to deliver all four cycles together.
[Spec revision 11](../spec.md) records the approved scope and REQ-016 details;
[TASK-004](../tasks/TASK-004-company-sso-and-launch-verification.md) owns execution.

## Problem and outcome

Further account-management features would delay deploying SaaS without an
established launch need. Deliver enforceable company SSO and credible connected
proof for the retained launch surfaces. Do not build self-service linking,
password addition to social-only Users or method removal. Retain invalidation
required by shipped operator controls and required SSO; deferral is not permission
to keep authenticating through a disabled provider.

## Agreed implementation

Reuse tenant Authentication settings and the current company connection
candidate/test/activate lifecycle. Rust provides an admin-only read of required
state/policy revision, current connection ID/revision/readiness and a count of
active Users without a usable identity for that connection. Disabled Users and
Services do not count. Show the count and explain the access consequence before
explicit Enable confirmation; unlinked Users do not block enablement.

The enabling administrator must already have a company-authenticated tenant User.
Use existing company first-login provisioning and verified group-role mapping or
existing explicit grant administration to establish that admin. Do not depend on
linking a setup/password/social User. Different methods can yield separate Users;
grants, keys and principal IDs never automatically transfer by matching email.
Document this limitation and prove this actual no-linking adoption journey.

Enable rechecks current tenant-admin authority, fresh company proof, the current
tested Active connection and submitted policy/connection revisions at mutation.
Stale settings conflict and require reload; refusal changes nothing. Ordinary
Disable also requires fresh company-admin proof. Unavailable SSO uses existing
independent operator-assisted recovery, not weaker ordinary login proof.

Fresh means the existing tenant successful provider sign-in within five minutes;
it is not a new universal guarantee of upstream password/MFA re-entry. Stale proof
uses the existing company login, then a return to settings and explicit
confirmation. No pending mutation is automatically replayed after login.
Connection-test proof, bearer/API-key possession and MCP scopes do not replace
human proof. This avoids a new tenant reauthentication engine or credential channel.

Reuse HumanConnections and the existing policy/method evaluator, BrowserSessions,
callback/device/refresh owners and their SQL/query/revision mechanisms. Enforce
company-only policy at starts and racing completions, invitations, browser use,
device approval/redemption and renewal. Invalidate affected password/social browser
sessions and refresh families without changing API-key policy or inventing
instantaneous bearer revocation. Disabling enforcement allows new sign-ins but
cannot revive old credentials, even ones unused while enforcement was enabled.
Policy remains required across company replacement, deactivation and outage.
Tenant isolation, platform independence and issuer/subject ownership remain intact.

Human proof and its mutations remain in Rust-owned browser HTTP. Required safe
metadata and existing authorized management project through the agreed
HTTP/CLI/SDK/MCP owners, preserving explicit write scopes. No unrestricted setter
on another protocol may bypass fresh-proof checks. The UI renders Rust results.
The approved controls extend the existing settings page; no additional mock
approval round or second operator console is required.

## Connected proof

TASK-003-carryover cycles 1–16 supply actual operator setup, signup, invitations,
platform sessions and production pages; TASK-001/002 supply grants and resource
consumers. Their completion is a dependency, not asserted by this design.

Prove policy read/warning and human enable/disable through production UI and Rust
HTTP against repository Postgres. Cover stale/wrong proof and revisions, direct
bypasses and callbacks begun before enablement, invitation/device enforcement,
original-credential invalidation and non-resurrection, outage/replacement,
independent recovery, replicas, other tenants and platform login.

Connect real onboarding outputs to production login, explicit CLI device approval
and real Rust/Python/TypeScript resource access, including no-IdP/no-SMTP local
use and company-policy use. Exercise required safe metadata and management denials
through typed HTTP/MCP consumers. Extend existing identity/UI/platform/SQL/SDK/MCP
journey owners. Carry forward valid unchanged evidence; new behavior is test-first.
Keep generated contracts and instructions accurate.

Real Google/GitHub consent and callback smoke remains separate from controlled
provider proof. Record secret-free configuration and observations; missing access
is a material unfinished verification item, not a silent waiver or a redesign
reason. Local Mailpit is sufficient SMTP proof; no hosted SMTP requirement.

## Evidence and boundaries

Source inspection supports reuse: HumanConnections already owns company lifecycle;
company ensure_user_identity provides first-login provisioning; verified group
mapping or explicit grants supplies authority; login_policy.rs currently reads
required state; the tenant policy table already stores state/revision. Existing
browser sessions record original proof anchors and recent-sign-in state. Extend
these owners rather than rebuilding authentication. Inspection is not runtime proof.

The approved spec names identity, policy, session/device/refresh and authorization
revision persistence purposes. Use current fields when sufficient; any necessary
new migration must cite an explicitly approved purpose and never edit an applied
migration. No new persistence purpose, dependency, credential channel or recovery
system is approved. Architecture rules for future linking/removal remain intact;
those capabilities are deferred, not implemented with weaker safeguards.

The prior research used [OWASP account linking](https://cheatsheetseries.owasp.org/cheatsheets/Authentication_Cheat_Sheet.html#secure-federated-account-linking),
[OIDC Core](https://openid.net/specs/openid-connect-core-1_0.html#AuthRequest),
[Google's reference](https://developers.google.com/identity/openid-connect/reference)
and [GitHub OAuth](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps).
Their mature authentication features do not establish a need to ship self-service
method management before this launch.

## Handoff

No material design decision remains for the retained scope. Private route/helper
names, module layout, transaction mechanics and new test selectors remain
implementation-owned within the approved contracts. One task delivers old 19–20;
old 17–18 remain explicitly deferred in the packet. Production/runtime/deployment
readiness is not claimed. No production edit, migration, staging, commit or push
is part of this planning handoff.
