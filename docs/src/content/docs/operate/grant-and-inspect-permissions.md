---
title: Permissions
browserTitle: Grant and inspect permissions | Wyrd
description: See a principal's access, bind or remove a role at an explicit scope, cut a principal off, and confirm the result in the audit log.
pillar: wyrd
section: Operate
order: 4
---

Bind a role at a scope to give a principal more access, remove the binding to take it away, and read the audit log to confirm what the server decided. Every grant names its scope: `all` for the whole tenant, or one Card family such as `Repository/payments`.

## Prerequisites

- A credential with the `admin` role, such as the tenant `admin_credential` from [Deploy Wyrd for a team](/operate/deploy-wyrd-for-a-team/#4-create-the-first-tenant), or the key `wyrd init` saved.
- The principal's ID. `wyrd principal list` shows it.

If you are not sure which role you need, [Identity and access](/understand/identity-and-authorization/#roles) lists what each built-in role allows.

## 1. Find the principal and its access

The CLI uses your saved endpoint, or `--server` / `WYRD_SERVER_URL`:

```bash
wyrd principal list --kind service
wyrd principal access show --principal <principal-id>
```

`show` prints one `grant` line per binding, with its binding ID, `role:scope`, and source (`direct`, `provider`, or `group:<name>`), then the combined permissions. A Service account holds the grants it was created with; [Deploy a Service from CI](/operate/deploy-a-service-from-ci/) creates one with `editor:all` and one with `workload:all`. A tenant administrator holds `admin:all`.

## 2. Bind or remove a role

A grant is `ROLE:SCOPE`. The scope is required: `all`, `KIND/NAME` for a Card family and its future versions, or `KIND/NAME@UID` for one exact version.

```bash
wyrd principal access grant --principal <principal-id> --kind service --grant editor:all
wyrd principal access grant --principal <principal-id> --kind user --grant viewer:Repository/payments
wyrd principal access revoke --binding <binding-id>
```

`grant` prints the binding: its ID, `role:scope`, subject, and source. `revoke` takes that binding ID, which `show` also lists.

Every SDK's `Principals` handle makes the same three calls, `bind`, `access`, and `unbind`, with the same request fields, and so do the `principals.bind`, `principals.access`, and `principals.unbind` [MCP tools](/reference/mcp-tools/#principal-access).

A bind is idempotent: binding the same role at the same scope again returns the same binding.

The change reaches running servers within about 5 seconds, including on credentials and tokens the principal already holds; nothing needs to sign in again or renew a token. Removing one binding leaves any other binding that grants the same access, such as `admin:all` or a group or identity-provider binding. A provider binding cannot be removed here; change the group mapping instead.

## In the UI

**Settings**, then **Access**, does the same work in the browser for a signed-in tenant administrator. It has four tabs:

- **Users** and **Service accounts** list the accounts. Select one to see each binding with its role, scope and source, and the combined permissions. Grant a role at a scope there, or remove a direct binding. A new Service account's key, and each key you issue later, is shown once.
- **Roles** lists the built-in roles and creates, edits and deletes custom ones.
- **Groups** creates groups, changes their members, and grants a group a role at a scope. Members inherit it.

No scope is preselected. A Card family covers every version of that Card, including later ones; an exact version covers only that one. A grant the server refuses saves nothing.

A person who opens a page they have no access to sees **Copy access request**. It copies their account, the tenant, the action and the link they opened, for them to send you in your own channel. Wyrd sends nothing and keeps no record of it, and the text is the same whether the target is hidden or does not exist.

## 3. Cut a principal off

To stop a principal entirely, suspend it:

```bash
wyrd principal revoke <principal-id> --kind service --reason "credential leaked in CI logs"
```

New tokens are refused at once, and tokens already issued expire within 5 minutes. To stop one key but keep the account, revoke just that key with `wyrd principal credential revoke`, or the `principals.revoke_credential` [MCP tool](/reference/mcp-tools/#principal-credentials).

## 4. Confirm the decision

Every permission check is recorded in the audit log. To see what was refused recently:

```sql
SELECT created_at, sub_principal_id, permission, outcome
FROM bifrost.system.audit_log
WHERE outcome = 'denied'
ORDER BY created_at DESC
LIMIT 20
```

Run it with any SDK's `Bifrost.sql`, or the `bifrost.query` MCP tool. A refused call returns `WYRD_PERMISSION_403_DENIED_RBAC`, and the error names the permission the principal lacked.

## What can go wrong

| Error code | Cause | Fix |
| --- | --- | --- |
| `WYRD_PERMISSION_403_DENIED_RBAC` on `grant` or `revoke` | Your credential is not a tenant administrator's | Use the tenant administrator's credential |
| `WYRD_AUTH_401_CREDENTIAL_REVOKED` | The principal or its tenant is suspended | Ask a tenant or platform administrator to restore it |
| The principal still has the old access after 5 seconds | Another binding still grants it | Run `wyrd principal access show` and remove or change the binding that still grants it |
