---
title: Identity and authorization
description: Every call to Wyrd is made as a principal whose roles decide what it may do, and every decision is audited.
pillar: wyrd
section: Understand
order: 5
---

Every call to the Wyrd server is made as a principal, and the principal's roles decide what it may do. Wyrd checks the permission before the operation proceeds and records the decision.

## Tenants

Every person, Service, Agent, and automation acting on Cards and evidence belongs to exactly one tenant. A tenant is a hard isolation boundary: PostgreSQL row-level security keeps every query inside its tenant. Platform administrators create and suspend tenants but are never implicitly authorized over a tenant's resources.

## Principals

| Principal | Created |
| --- | --- |
| A person | On first sign-in through the tenant's identity provider |
| A deployed Service or Agent | When its Card is declared, with the `workload` role |
| An automation, such as a CI job | By an administrator |

## Credentials

A caller presents a long-lived credential, such as an API key in `WYRD_API_KEY`, and the server exchanges it for an access token that lasts 5 minutes. The SDKs and CLI renew the token automatically. A role change reaches a running service when its token is next renewed.

## Roles

A permission is written `resource:action`, such as `cards:write`. A role is a set of permissions. Every tenant has four built-in roles, each including everything in the one above it:

| Role | Typical holder |
| --- | --- |
| `viewer` | A person who inspects results |
| `workload` | A deployed Service or Agent |
| `editor` | A person or CI job that changes declarations |
| `admin` | The tenant administrator |

The built-in roles apply to the whole tenant. A space groups Cards; it is not an access boundary.

## Every decision is audited

Wyrd checks the permission before the operation proceeds. Each `allowed` or `denied` decision is recorded in the background in `vala.system.audit_log`, a Bifrost table a `viewer` can query.
