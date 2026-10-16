---
title: Identity and authorization
description: Every call to Wyrd is made as a principal whose roles decide what it may do, and every decision is audited.
pillar: wyrd
section: Understand
order: 5
---

Every call to the Wyrd server is made as a principal: a person, an administrator, or a deployed Service or Agent. A principal belongs to exactly one tenant, holds roles, and those roles decide what it may read, write, and run. Wyrd checks the permission before the operation proceeds and records the decision in the audit log. Identity is not optional, and there is no anonymous access to tenant data.

This page explains how those pieces fit together. The examples continue the `support-desk` service from [Cards, versions, and relationships](/understand/cards-versions-and-relationships/).

## Tenants and the platform

Wyrd separates two planes of authority.

| Plane | Who acts in it | What it controls |
| --- | --- | --- |
| Platform | Platform administrators | Creating, suspending, and recovering tenants |
| Tenant | Everyone else | Cards, observations, verdicts, the gateway, and the tenant's own principals |

A tenant is a hard isolation boundary. Every tenant-plane principal, Card, observation, and audit record belongs to one tenant, and the database enforces it on every query. A platform administrator holds no authority inside a tenant, and neither plane accepts the other's credentials.

PostgreSQL row-level security enforces that boundary inside the database, so a query that forgets its tenant filter still cannot read another tenant's data. Isolation is the only rule the database enforces; what a principal may do inside its tenant is decided by the roles below and nowhere else.

Creating a tenant also creates its tenant administrator and prints that administrator's first credential once. On a fresh self-hosted server, `wyrd-server setup --tenant <slug>` creates the platform root and the first tenant in one step.

## Principals

A principal is an identity Wyrd can authorize and attribute. Each has a kind:

| Kind | Who it is | How it is created |
| --- | --- | --- |
| `tenant_admin` | The tenant's administrator | When the tenant is created |
| `user` | A person | On first sign-in through the tenant's identity provider |
| `service` | A deployed Service, or an automation such as a CI job | Registering a Service Card, or creating an automation principal |
| `agent` | A deployed Agent | Registering an Agent Card |

Registering a Card is what gives a deployed workload its identity. When `wyrd apply` registers `support-desk`, Wyrd creates a `service` principal bound to that Card and grants it the `workload` role. Re-registering the Card keeps the same principal and never grants the role again, so an administrator's later change stands.

An automation principal, such as a CI runner that registers Cards, is a `service` principal with no Card. An administrator creates it directly.

## How a caller proves who it is

A caller presents a long-lived credential, and the server exchanges it for an access token that lasts 5 minutes. The SDKs and CLI do the exchange and renew the token for you.

| Caller | Credential | Obtained with |
| --- | --- | --- |
| A person | A saved sign-in through the tenant's identity provider | `wyrd auth login` |
| A deployed Service or Agent | An API key bound to its Card | `wyrd auth issue-key` |
| An automation principal | An API key | `wyrd principal credential create-principal` |
| A workload in a cloud platform | The platform's identity token, mapped to a Card | `wyrd auth trusted-issuer` and `wyrd auth workload-binding` |

Keep an API key in your secret store and inject it as `WYRD_API_KEY`. A key is shown once when it is issued, and Wyrd stores only a hash of it.

Each SDK and the CLI look for a credential in the same order and use the first one they find:

1. A credential passed in code, such as `WyrdClient(credential=...)`
2. `WYRD_ACCESS_TOKEN`
3. `WYRD_WORKLOAD_TOKEN`, with `WYRD_TENANT`
4. `WYRD_API_KEY`
5. A saved `wyrd auth login` for that server
6. `api_key` under `[default]` in `credentials.toml`, in your Wyrd config directory (`WYRD_CONFIG_HOME`, else `$XDG_CONFIG_HOME/wyrd`, else `~/.config/wyrd`)

Pass a tenant only with a workload token; combining `WYRD_TENANT` with an API key or access token is an error.

A permission snapshot is taken when the token is issued, so a role change reaches a running service within 5 minutes, when its token is next renewed.

## Roles and permissions

A permission names a resource and an action, written `resource:action`, such as `cards:write` or `verifier:run`. A role is a named set of permissions, and a principal's permissions are the union of its roles.

Every tenant has four built-in roles. Each includes everything in the role above it:

| Role | Adds | Typical holder |
| --- | --- | --- |
| `viewer` | `cards:read`, `artifacts:read`, `audit:read`, `operators:read`, `gateway:read`, `gateway_payload:read`, `bifrost_table:read`, `bifrost_query:read` | A person who inspects results |
| `workload` | `bifrost_table:write`, `bifrost_record:write`, `evals:run`, `verifier:run`, `triggers:write`, `operators:invoke`, `gateway:invoke` | A deployed Service or Agent |
| `editor` | `cards:write`, `cards:delete`, `artifacts:write`, `policy:lock`, `services:install` | A person or CI job that changes declarations |
| `admin` | Every permission (`*:*`), including the administrative ones no other role holds: `service_accounts:write`, `operators:write`, `gateway:write`, and `gateway:delete` | The tenant administrator |

Roles are granted per tenant, not per space. A space groups and names Cards; it is not an access boundary. A few permissions can be narrowed to one object, such as `verifier:run` on a single Verifier or `gateway:invoke` on one model.

A person's roles can come from two places: roles mapped from groups in the identity provider, and roles granted directly in Wyrd. Revoking a direct grant does not remove a role the identity provider supplies.

The four built-in roles are the only roles today. There is no API to create a custom role.

## What a service may write about

Roles decide what a principal may do; a Card binding also decides what it may write about. A principal bound to `support-desk` may record observations only for that Service and the Cards it references, such as `support-agent` and `support-prompt`. Anything else is refused with `WYRD_VALA_403_BIFROST_CARD_SCOPE`, even though the `workload` role allows writing observations in general. That is what stops one service from writing evidence about another.

A principal with no Card, such as a person or an automation principal, may attribute observations to any registered Card in its tenant that its roles allow.

## Ownership

Wyrd has no per-Card owner. Who may change or delete a Card is decided by role alone: any principal with `cards:write` may change any Card in the tenant. Each Card records the principal that created it, for provenance, not for access.

The one ownership relationship Wyrd enforces is between a Service or Agent and its verification bindings: the bindings in a Card's `verified_by` belong to that Card and change only when it is re-registered.

## Acting on behalf of someone

A Service that handles a request for a person can act as that person instead of as itself. It exchanges the person's token, with its own, for a delegated token. The delegated token carries the intersection of both principals' permissions, so the service can never do more than the person could, and the audit log records both identities.

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
as_user = client.on_behalf_of(user_token)
```

</LangTab>
<LangTab lang="rust">

```rust
let as_user = client.on_behalf_of(user_token, TokenAudience::Bifrost).await?;
```

</LangTab>
<LangTab lang="typescript">

```typescript
const asUser = await client.onBehalfOf(userToken);
```

</LangTab>
</LangTabs>

Only a Service or Agent bound to a Card can delegate, and a chain of delegations is limited to 5 hops.

## Every decision is audited

Every permission check produces an `allowed` or `denied` decision, and Wyrd records it in `vala.system.audit_log` with the principal, its kind, the permission, the outcome, and the request. Token issuance, key issuance, and role changes are recorded the same way. Recording never slows or blocks the request: the permission check itself blocks, and the audit write happens in the background.

The audit log is a Bifrost table, so a `viewer` can query it like any other:

```sql
SELECT created_at, sub_principal_id, permission, outcome
FROM vala.system.audit_log
WHERE outcome = 'denied'
ORDER BY created_at DESC
LIMIT 20
```

## Changing access

Only the `admin` role can grant and revoke roles or manage credentials. [Grant and inspect permissions](/operate/grant-and-inspect-permissions/) has the commands and SDK calls.

## What can go wrong

| Error code | Cause |
| --- | --- |
| `WYRD_CLIENT_401_NO_CREDENTIALS` | The client found no credential in any of the places listed above |
| `WYRD_PERMISSION_401_UNAUTHENTICATED` | The request reached the server without a valid access token |
| `WYRD_AUTH_401_TOKEN_EXPIRED` | The access token expired and was not renewed |
| `WYRD_AUTH_401_CREDENTIAL_REVOKED` | The credential, its principal, or its tenant was revoked or suspended |
| `WYRD_PERMISSION_403_DENIED_RBAC` | The principal's roles do not include the permission; the error names the permission it needed |
| `WYRD_VALA_403_BIFROST_CARD_SCOPE` | A Card-bound principal wrote an observation about a Card outside its declaration |
