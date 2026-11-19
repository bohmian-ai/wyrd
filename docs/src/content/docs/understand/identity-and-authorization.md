---
title: Identity and access
description: Every call to Wyrd is made as an account whose roles decide what it may do; a running Service also names its exact release, from its bundle.
pillar: wyrd
section: Understand
order: 6
---

Every call to the Wyrd server is made as an account, and the account's roles decide what it may do. A running Service adds one fact the key cannot carry: which exact release it is, which comes from its bundle.

## Tenants

Every account belongs to exactly one tenant. A tenant is a hard isolation boundary: PostgreSQL row-level security keeps every query inside its tenant. Platform administrators create and suspend tenants but are never implicitly authorized over a tenant's resources.

## Accounts

| Account | Used by | Created |
| --- | --- | --- |
| User | A person | On first sign-in through the tenant's identity provider |
| Tenant administrator | The first administrator | When the tenant is set up |
| Service account | CI, a running Service, any automation | By an administrator, with `wyrd principal credential create-principal` |

Declaring a Card creates no account. One Service account serves every release of its Service, so a new release needs no new account and no new key. [Deploy a Service from CI](/operate/deploy-a-service-from-ci/) sets up the usual pair: one account for CI, one per running Service.

## The key and the release

A Service account's key says who is calling. The release is the exact Service Card version, which the server knows by its UID.

A running application gets the UID from its bundle. `WyrdState` sends it with the key on every call, and the server limits that call to the release's own Cards: an observation about a Model the release does not use is refused. Each row then records both, the account in `sub_principal_id` and `act_principal_id`, the release in `workload_card_uid`, and the Card observed in `subject_card_uid`.

Two releases can run at once with the same key. Each bundle names its own UID, so their evidence never mixes.

A User, or a Service account created without `--workload-name`, needs no release. It can record and verify evidence for any registered Card its roles allow; that evidence has no release.

## One account per Service

`--workload-name checkout` restricts an account to releases of the `checkout` Service. A restricted account must name a release to record or verify Card evidence, and only a `checkout` release.

Each Service has at most one restricted account, and its scheduled Verifiers run as that account. A schedule runs only while that account has used the release in the last 24 hours, so a retired release stops being checked on its own. A Service with no restricted account has no scheduled verification.

## Keys and tokens

The client exchanges a key for an access token that lasts 5 minutes, and renews it automatically. A role change reaches a running Service at its next renewal. An account can hold several keys, which is how keys rotate: issue a new key, deploy it, then revoke the old one.

## Roles

A permission is written `resource:action`, such as `cards:write`. A role is a set of permissions. Every tenant has four built-in roles, each including everything in the one above it:

| Role | Adds | Typical holder |
| --- | --- | --- |
| `viewer` | Reading Cards, results, the audit log, and Bifrost tables | A person who inspects results |
| `workload` | Recording Runs and observations, running Verifiers, calling the LLM gateway | A running Service |
| `editor` | Declaring and deleting Cards, uploading artifacts | CI, or a person who changes declarations |
| `admin` | Everything, including accounts, keys, and gateway settings | The tenant administrator |

Roles apply to the whole tenant. `workload` cannot change a Card, so a running Service cannot change what it is verified against.

## Every decision is audited

Wyrd checks the permission before the operation proceeds. Each `allowed` or `denied` decision is recorded in the background in `bifrost.system.audit_log`, a Bifrost table a `viewer` can query.
