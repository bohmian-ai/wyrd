# Admin console

Status: finalized

## Problem

A tenant administrator who signs in and opens **Settings** lands on a single
company-SSO page. Everything else an administrator does sits behind inline words
in the page intro ("Accounts, roles and grants: Access", "Integrations: GitHub")
that look like body text. Observed in the live smoke on 2026-11-24:

- The administrator could not find how to invite a person.
- Every section is an identical bordered panel stacked in one column, with no
  primary action and no at-a-glance status. A one-time setup value (the callback
  URL) is first.
- Explanation comes before state and controls. "Require company SSO" shows four
  paragraphs and then says it cannot be changed.
- Internal terms lead: "Revision 0", "Direct bindings", Roles as raw
  `cards:read, executions:read, …` lists.
- Copy is wrong: "Sign-in for this tenant uses API keys until a tested candidate
  is activated" is false for a Google/GitHub-signed-in tenant. Account shows
  Username "—" for a social user. The sidebar shows a raw principal id.
- Stored secrets the server already manages per tenant (gateway provider
  credentials, Operator connections, company SSO and trusted-issuer client
  secrets, with replace, renew-encryption and revoke) have no UI.
- The platform console is four text-link pages with no structure and no
  light/dark control.

## Desired outcome

- An administrator finds every admin job from one visible section list, with no
  hunting through body text: invite a person in two clicks from Settings.
- Each admin page answers "what is the state?" before "what can I change?": a
  status strip, then the list or settings, with one primary action.
- Secrets owned by the tenant are listed in one place with their state and the
  rotate/renew/revoke actions the server already offers.
- The platform console uses the same shell pattern and has the mode control.
- People are shown by name or email, never "—" or a raw id.

## Constraints and non-goals

- Presentation only. Every action uses an existing HTTP endpoint; the UI owns
  no workflow (AGENTS.md § Agent-first; wyrd-ui skill § Product boundary).
- Evidence Thread (`brand/DESIGN.md`) stays: tokens only, 1px rules, 2px radius,
  no gradients/shadows/glow. Energy comes from hierarchy, not decoration.
- Reuse built components: `StatStrip`, `RecordTable`, `StatusPill`,
  `EmptyState`, `Drawer`, `Panel`, `ActionButton`.
- Follow conventional admin-console standards (GitHub, Linear, Vercel, Slack,
  Google Workspace) wherever this design leaves a choice (user, 2026-11-24).
- The UI never re-derives server-owned meaning: counts, thresholds and
  aggregation come from the server or are not shown.
- Non-goals: an editable display name (convention takes the name from the
  identity provider); a rotation-age threshold or "stale secret" warning (no
  server-owned threshold exists); per-method usage counts on Sign-in; any new
  admin capability the server lacks.

## Proposed implementation

### Tenant Admin (`/t/{tenant}/settings/*`)

Settings becomes a two-column area: a section list, then one page pattern.

| Section | Route | Shows | Primary action | Server source |
|---|---|---|---|---|
| People | `settings/people` | Status strip (active, invited, suspended); one table of users and pending invitations, filterable; row opens a drawer with roles, keys, suspend | Invite people (drawer) | `GET /v1/principals`, invitations, bindings (today's `settings/access`) |
| Service accounts | `settings/service-accounts` | Accounts, restriction, key count | New service account | same |
| Roles | `settings/roles` | One row per role: server description, member count (opens People filtered to the role); operations expand on demand | New role | `GET /v1/access/roles`, `PUT /v1/access/roles/{role}` |
| Sign-in | `settings/sign-in` | Method summary (Google, GitHub, password, company SSO); company SSO as a 4-step setup; "Require company SSO" as a switch with an impact confirmation; Danger zone for deactivate | Set up company SSO | `/v1/identity/oidc/*`, `/v1/identity/login-policy`, the method list the tenant sign-in page already reads |
| Secrets | `settings/secrets` | One inventory: gateway provider keys, Operator connections, company SSO client secret, trusted-issuer secret; state, last rotated; Replace, with Renew encryption and Revoke in a row menu | Add provider key | `/v1/admin/gateway/provider-credentials*`, `/v1/operator-connections*`, `/v1/identity/oidc/connections/{id}/renew-encryption`, `/v1/admin/trusted-issuers*` |
| Integrations | `settings/integrations` | GitHub, as today | — | existing |

Your account leaves Admin and opens from the name block for every user. The name
block and Your account show the person's name and email. `auth_users` gains a
provider-sourced `display_name` (Google `name`, GitHub `name` else `login`,
company OIDC `name`), refreshed on each sign-in and not user-editable; it is
added to the unshipped auth migration in place (user, 2026-11-24). Label order:
display name, username, email.

### Wire and storage additions



| Endpoint | Adds | Used by |
|---|---|---|
| `GET /v1/principals/me` (`CurrentUserProfile`) | `email`, `display_name` | Sidebar name block, Your account |
| `GET /v1/principals` (`PrincipalSummary`) | `authentication` (the same `AccountAuthentication` `/me` already returns); a role summary (role names granted, with scope) | People "Signs in with" and "Roles" columns |
| `GET /v1/principals` (`PrincipalPage`) | counts by status, for the active filter set | People status strip and filter labels |
| `GET /v1/principals` (`PrincipalQuery`) | `role` filter: holders at any scope, directly or through a group | Roles member count link |
| `GET /v1/access/roles` (`RoleView`) | `description`; `member_count` (distinct users and service accounts holding the role at any scope, directly or through a group) | Roles table |
| `PUT /v1/access/roles/{role}` (`PutRoleRequest`) | optional `description` for custom roles | New/edit role |

`wyrd.auth_roles` gains a nullable `description`, added in place to the
unshipped `20260601000001_auth.sql` like `display_name`. Built-in descriptions
are server-set from the mock's wording and read-only.

`CurrentUserProfile` currently documents "never an email"; the spec revises that
sentence for the caller's own email, which is not a disclosure to anyone else.
The lock-out count on Require company SSO already comes from the login-policy
read (`users_without_identity`).

### Platform console (`/platform/ui/*`)

Same shell: sidebar Overview, Workspaces, Sign-in providers, Mail, Admins,
Stored secrets (maintenance status and runs); mode control in the name menu.
This revises the approved `authorization-model` console mock.

### Page pattern

Title + one-line purpose + primary action (top right) → status strip → content.
Settings use rows (label and one-line help left, control right). Create/edit in a
drawer. Destructive actions in a bottom Danger zone with confirmation. Machine
values in a read-only field with Copy. Help text one line plus a docs link.

Mock: [admin-mock/](admin-mock/README.md): 24 states, light and dark, 1280 and 390
wide; `selfcheck.mjs` passes with no page errors, sideways scroll or outside
requests.

## Evidence and rationale

- Current pages: `wyrd-ui/src/routes/t/[tenantKey]/settings/{+page,access/+page,account/+page}.svelte`,
  `src/routes/platform/ui/*`.
- Wire shapes checked: `ProviderCredentialView` (`state`, `rotated_at`, `revoked_at`),
  `OperatorConnectionView` (`status`, `updated_at`), `TrustedIssuerView`,
  `UserInvitationView`, `PrincipalSummary` (`email`, `name`, `status`, `kind`).
- `PrincipalSummary.name` is `null` for SSO-only users, which is why "—" shows.

## Decisions and alternatives

- User: redesign admin now; stop the scenario 7 smoke at step 2 (2026-11-24).
- User: follow conventional standards; approved this design (2026-11-24).
- People shows sign-in method and roles; the name block shows your email. Both
  are standard in GitHub, Linear, Vercel, Slack and Google Workspace members
  lists, and the method column answers "who is locked out if I require SSO".
- Secrets inventory is composed in the host from the four existing owner
  endpoints. Alternative: one server inventory endpoint; not needed, since the
  UI only lists and links, and convention (GitHub, Vercel) lists per owner on
  one page.
- Platform console includes Admins and Stored secrets: both have endpoints
  today (`/platform/admins*`, `/platform/stored-secrets/maintenance`).
- Settings sections are sub-routes, not tabs in one page, so each is linkable
  and loads only its data. Implementation-owned.
- Rejected: a 90-day "not rotated" warning; it would be a UI-invented threshold.
- User agreed (2026-11-24): role member counts and descriptions are
  server-owned, as in AWS, Google Cloud and GitHub, so MCP and CLI see them too.
  Rejected: UI-only text for built-ins with "N operations" for custom roles;
  it hides meaning from agents. Rejected: a bindings count; it is not members.

## Standards conflicts

- Provider marks on Google/GitHub buttons (user, 2026-11-24). Google's
  branding guidelines require the official multicolor "G", which conflicts with
  `brand/DESIGN.md`'s tokens-only color rule. Disposition: allow provider marks
  as fixed SVG assets; the GitHub mark stays monochrome (`currentColor`).
  `brand/DESIGN.md` gains one sentence for this exception during TASK-005.
- Otherwise none. More visual personality (illustrated empty states, extra icons) would be
  a `brand/DESIGN.md` change; not requested.

## Open questions

None.

## Handoff

Recorded as REQ-021 and AC-016 in [spec revision 12](../spec.md); planned as
TASK-005. The mock in [admin-mock/](admin-mock/README.md) is the
look-and-wording specification; private component structure is
implementation-owned.
