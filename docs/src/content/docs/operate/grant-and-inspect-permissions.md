---
title: Grant and inspect permissions
description: See which roles a principal holds, grant or revoke a role, cut a principal off, and confirm the result in the audit log.
pillar: wyrd
section: Operate
order: 3
draft: true
status: partial
---

Grant a role to give a principal more access, revoke it to take access away, and read the audit log to confirm what the server decided. Every command here changes access for the whole tenant; roles are not scoped to a space.

## Prerequisites

- A credential with the `admin` role, such as the tenant `admin_credential` from [Deploy Wyrd for a team](/operate/deploy-wyrd-for-a-team/#5-create-the-first-tenant), or the key `wyrd server dev` saved.
- The principal's ID. `wyrd principal list` shows it.

If you are not sure which role you need, [Identity and authorization](/understand/identity-and-authorization/#roles-and-permissions) lists what each built-in role allows.

## 1. Find the principal and its roles

The CLI uses your saved endpoint, or `--server` / `WYRD_SERVER_URL`:

```bash
wyrd principal list --kind service
wyrd principal role list --principal <principal-id>
```

A Service or Agent principal created by `wyrd apply` starts with `workload`. A tenant administrator holds `admin`.

## 2. Grant or revoke a role

```bash
wyrd principal role grant --principal <principal-id> --role editor
wyrd principal role revoke --principal <principal-id> --role editor
```

The same call from an SDK:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
from wyrd.principals import Principals

principals = Principals()
result = principals.grant_role(principal_id, "editor")
print(result["changed"], result["roles"])
```

</LangTab>
<LangTab lang="rust">

```rust
use wyrd_sdk::{Principals, WyrdClient};

let principals = Principals::with_client(WyrdClient::from_global()?);
let result = principals.grant_role(&principal_id, "editor").await?;
println!("{} {:?}", result.changed, result.roles);
```

</LangTab>
<LangTab lang="typescript">

```typescript
import { Principals } from "wyrd-ai";

const principals = Principals.connect();
const result = await principals.grantRole(principalId, "editor");
console.log(result.changed, result.roles);
```

</LangTab>
</LangTabs>

A grant is idempotent: granting a role the principal already holds returns `changed: false` and the unchanged role list.

The change reaches a running service within 5 minutes, when its access token is next renewed. Revoking a role that a person also gets from an identity-provider group does not remove it; change the group mapping instead.

## 3. Cut a principal off

To stop a principal entirely, suspend it:

```bash
wyrd principal revoke <principal-id> --kind service --reason "credential leaked in CI logs"
```

New tokens are refused at once, and tokens already issued expire within 5 minutes. To stop one credential but keep the principal, revoke just that credential with the `principals.revoke_credential` [MCP tool](/reference/mcp-tools/#principal-credentials).

## 4. Confirm the decision

Every permission check is recorded in the audit log. To see what was refused recently:

```sql
SELECT created_at, audit_principal_id, permission, outcome
FROM vala.system.audit_log
WHERE outcome = 'denied'
ORDER BY created_at DESC
LIMIT 20
```

Run it with any SDK's `Bifrost.sql`, or the `bifrost.query` MCP tool. A refused call returns `WYRD_PERMISSION_403_DENIED_RBAC`, and the error names the permission the principal lacked.

## What can go wrong

| Error code | Cause | Fix |
| --- | --- | --- |
| `WYRD_PERMISSION_403_DENIED_RBAC` on `grant` or `revoke` | Your credential does not hold `admin` | Use the tenant administrator's credential |
| `WYRD_AUTH_401_CREDENTIAL_REVOKED` | The principal or its tenant is suspended | Ask a tenant or platform administrator to restore it |
| The service still has the old access | Its token was issued before the change | Wait up to 5 minutes for the next renewal |
