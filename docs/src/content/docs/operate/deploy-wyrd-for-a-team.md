---
title: Deploy for a team
description: Prepare PostgreSQL with one login, migrate it, start the server, and create the first tenant.
pillar: wyrd
section: Operate
order: 1
draft: true
status: partial
---

A Wyrd server needs one PostgreSQL login, set as `WYRD_DATABASE_URL`. For a self-hosted team that login is enough, because Wyrd keeps tenants apart inside PostgreSQL rather than with separate logins. A second login is optional hardening; step 1 explains when it is worth it.

Treat the database URL as a secret: anyone who can read the server's environment has platform access.

This page covers the database, the server's persistent settings, and the first tenant.
[Configure the verification runtime](/operate/configure-verification-runtime/)
covers the Python worker and the limits on Verifier execution.
TLS termination and multi-replica topology are not documented yet.

## Prerequisites

- PostgreSQL 16, and an administrator login that can create logins and databases.
- The `wyrd-server` binary for the release you are deploying, or the `bohmian/wyrd` container image from Docker Hub.
- An object store every server replica can reach: S3, Google Cloud Storage, or Azure Blob Storage.

## 1. Create the login and the database

As the administrator, create an ordinary login and a database it owns:

```sql
CREATE ROLE wyrd LOGIN PASSWORD '<password>';
CREATE DATABASE wyrd OWNER wyrd;
```

The name is yours to choose. The login must not be a superuser or have `BYPASSRLS`. PostgreSQL skips row-level security for both, so the server refuses to start with either.

### Why one login is enough

Every Wyrd table has row-level security forced on, which makes PostgreSQL apply its policies even to the login that owns the table. Each tenant request runs in a transaction bound to one tenant, and the policies return only that tenant's rows. A query that forgets its tenant filter still sees one tenant.

Platform work, such as migrations and creating tenants, uses the same login on separate connections marked as operator sessions. Only those connections can read across tenants.

### When to add a tenant login

With one login, tenant requests run as the owner of the tables. An owner can turn row-level security off, or mark its own session as an operator session. A server bug that let a request run arbitrary SQL could use either to read other tenants.

A separate tenant login removes that path. It owns no table and holds only read and write access, so it cannot change a policy. The operator policies admit only the owner, so marking its session as an operator session gives it nothing. Add one when tenants must not be able to reach each other even through a server bug, such as one server shared by separate customers. Most self-hosted deployments do not need it: they run a single tenant, so there is no other tenant to reach.

To add one, do this before [step 2](#2-migrate). As the administrator, create the tenant login, then, connected to the `wyrd` database, run:

```sql
CREATE ROLE wyrd_tenant LOGIN PASSWORD '<tenant-password>';
CREATE SCHEMA IF NOT EXISTS platform AUTHORIZATION wyrd;
CREATE SCHEMA IF NOT EXISTS wyrd AUTHORIZATION wyrd;
CREATE SCHEMA IF NOT EXISTS vala AUTHORIZATION wyrd;
GRANT USAGE ON SCHEMA platform, wyrd, vala TO wyrd_tenant;
ALTER DEFAULT PRIVILEGES FOR ROLE wyrd IN SCHEMA wyrd, vala
    GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO wyrd_tenant;
ALTER DEFAULT PRIVILEGES FOR ROLE wyrd IN SCHEMA wyrd, vala
    GRANT USAGE, SELECT ON SEQUENCES TO wyrd_tenant;
```

The schemas exist early only so the tenant login can be granted on them; `migrate` still creates every table, and each one it creates is granted to `wyrd_tenant` automatically. Then set two URLs wherever the steps below set one: `WYRD_DATABASE_URL` for `wyrd_tenant`, and `WYRD_PLATFORM_DATABASE_URL` for `wyrd`, which `migrate` uses. Grant `wyrd_tenant` nothing else: the server refuses to start if it owns a Wyrd table or holds `TRUNCATE`, `REFERENCES`, or `TRIGGER` on one.

## 2. Migrate

```bash
export WYRD_DATABASE_URL="postgresql://wyrd:<password>@<db-host>:5432/wyrd"
wyrd-server migrate
```

`migrate` creates Wyrd's schemas and tables, then prints `Wyrd and Vala schemas are migrated and valid.` Run it once per release, before you start or upgrade servers.

## 3. Start the server and check readiness

The server needs two keys of its own. Create them once, store them in your secret manager, and mount the same two files on every replica:

```bash
openssl genpkey -algorithm ed25519 -out signing.pem
openssl rand -base64 32 | tr -d '\n' > sealing.key
chmod 0600 signing.pem sealing.key
```

The server reads only regular files that the user running it owns and nobody else can read, so mount them with that owner and mode `0600`.

The signing key signs the access tokens Wyrd issues. Every request carries one, and a replica accepts it only if this key signed it. With one shared key, a token issued by any replica works on all of them and still works after a restart. With `APP_ENV=production` and no signing key, the server refuses to start.

The sealing key encrypts the secrets Wyrd stores in PostgreSQL, such as LLM provider keys stored as `managed_secret` and identity provider client secrets, so a copy of the database does not expose them. Back it up separately from the database. If it is lost, those secrets cannot be decrypted, and the server refuses to start while any are stored.

Set these variables, then start `wyrd-server` with the same `WYRD_DATABASE_URL`:

| Variable | Value | If unset |
| --- | --- | --- |
| `WYRD_STORAGE_URL` | `s3://bucket`, `gs://bucket`, or `az://account/container`; bucket only, no path. S3 reads its region from `AWS_REGION` | Data goes to a local directory that only this process sees |
| `WYRD_SIGNING_KEY_FILE` | Path to `signing.pem` | Startup fails when `APP_ENV=production`; otherwise each process makes a temporary key, and tokens stop working after a restart and on other replicas |
| `WYRD_SEALING_KEY_FILE` | Path to `sealing.key` | Storing a `managed_secret` fails with `gateway credential protection is unavailable` |

```bash
curl -s -o /dev/null -w '%{http_code}\n' http://<server-host>:8080/readyz
```

`200` means the server checked the schema and the login and is serving. If the server exits instead, its last log line says why; see [Troubleshoot a deployment](/operate/troubleshoot-a-deployment/).

## 4. Create the first tenant

With the server running, from a machine that has the same two database URLs exported:

```bash
wyrd-server setup --tenant acme --server http://<server-host>:8080
```

On a fresh database this creates the platform root and prints its `wyrd_global_...` credential, then creates the tenant and prints:

```text
tenant_id: <uuid>
slug: acme
admin_principal_id: <uuid>
admin_credential: <key>
This is the only time the tenant credential is shown. Store it now.
```

Store both credentials in your secret manager. The platform credential creates and recovers tenants; the tenant `admin_credential` is the tenant's first administrator. Rerunning `setup` for an active tenant prints `is already set up; nothing to do.` To add another tenant later, set `WYRD_PLATFORM_CREDENTIAL` and rerun `setup` with the new slug.

Give the administrator credential to the first person who will manage the tenant. They save it as `[default].api_key` in their `credentials.toml`, [connect an agent](/get-started/use-with-your-coding-agent/), and grant roles to others; [Identity and access](/understand/identity-and-authorization/) explains the model.

## Next

- [Add people](/operate/add-people/) connects your identity provider so your team can sign in.
- [Deploy a Service from CI](/operate/deploy-a-service-from-ci/) creates the CI and Service accounts and ships your first Service.
