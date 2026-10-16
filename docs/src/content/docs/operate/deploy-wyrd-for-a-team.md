---
title: Deploy Wyrd for a team
description: Prepare PostgreSQL with a platform login and a tenant login, migrate it, start the server, and create the first tenant.
pillar: wyrd
section: Operate
order: 1
status: partial
---

A shared Wyrd server runs with two PostgreSQL logins. The platform login owns Wyrd's tables, runs migrations, and handles platform operations such as creating tenants. The tenant login serves tenant requests and owns no table, so PostgreSQL row-level security confines every tenant query to one tenant, even one that forgets its tenant filter.

The server keeps both logins connected while it runs. Treat both database URLs as secrets: anyone who can read the server's environment has platform access.

This page covers the database, the server's persistent settings, and the first tenant.
[Configure the verification runtime](/operate/configure-verification-runtime/)
covers the Python worker and the limits on Verifier execution.
TLS termination and multi-replica topology are not documented yet.

## Prerequisites

- PostgreSQL 16, and an administrator login that can create logins and databases.
- The `wyrd-server` binary for the release you are deploying, or the `bohmian/wyrd` container image from Docker Hub.
- An object store every server replica can reach: S3, Google Cloud Storage, or Azure Blob Storage.

## 1. Create the logins and the database

As the administrator, create two ordinary logins and a database the platform login owns:

```sql
CREATE ROLE wyrd_platform LOGIN PASSWORD '<platform-password>';
CREATE ROLE wyrd_tenant LOGIN PASSWORD '<tenant-password>';
CREATE DATABASE wyrd OWNER wyrd_platform;
```

The names are yours to choose. Neither login may be a superuser or have `BYPASSRLS`; the server refuses both, because PostgreSQL exempts them from row-level security.

## 2. Grant the tenant login its access

Connected to the `wyrd` database as `wyrd_platform`, run:

```sql
CREATE SCHEMA IF NOT EXISTS platform;
CREATE SCHEMA IF NOT EXISTS wyrd;
CREATE SCHEMA IF NOT EXISTS vala;
GRANT USAGE ON SCHEMA platform, wyrd, vala TO wyrd_tenant;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA wyrd, vala TO wyrd_tenant;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA wyrd, vala TO wyrd_tenant;
ALTER DEFAULT PRIVILEGES IN SCHEMA wyrd, vala
    GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO wyrd_tenant;
ALTER DEFAULT PRIVILEGES IN SCHEMA wyrd, vala
    GRANT USAGE, SELECT ON SEQUENCES TO wyrd_tenant;
```

This covers the tables that exist now and every table a later `migrate` creates. It is safe to rerun. Grant the tenant login nothing else: the server refuses to start if it owns a Wyrd table or holds `TRUNCATE`, `REFERENCES`, or `TRIGGER` on one.

## 3. Migrate

```bash
export WYRD_DATABASE_URL="postgresql://wyrd_tenant:<tenant-password>@<db-host>:5432/wyrd"
export WYRD_PLATFORM_DATABASE_URL="postgresql://wyrd_platform:<platform-password>@<db-host>:5432/wyrd"
wyrd-server migrate
```

`migrate` connects as the platform login and prints `Wyrd and Vala schemas are migrated and valid.` Run it once per release, before you start or upgrade servers.

## 4. Start the server and check readiness

Set the persistent settings, then start `wyrd-server` with the same two database URLs:

| Variable | Value | If unset |
| --- | --- | --- |
| `WYRD_STORAGE_URL` | `s3://bucket`, `gs://bucket`, or `az://account/container`; bucket only, no path. S3 reads its region from `AWS_REGION` | Data goes to a local directory that only this process sees |
| `WYRD_SIGNING_KEY_FILE` | Path to an Ed25519 private key in PEM form, the same file on every replica. `WYRD_SIGNING_KEY_PEM` takes the PEM inline instead; set one, not both | Startup fails when `APP_ENV=production`; otherwise each process signs with its own temporary key, and tokens do not survive a restart or work across replicas |
| `WYRD_SEALING_KEY_FILE` | Path to a file holding a base64 32-byte key, the same file on every replica. It encrypts stored secrets, including gateway provider keys put as `managed_secret`. `WYRD_SEALING_KEY_BASE64` takes the key inline instead; set one, not both. | Storing a `managed_secret` credential fails with `gateway credential protection is unavailable`, and startup fails while encrypted secrets are stored |

```bash
curl -s -o /dev/null -w '%{http_code}\n' http://<server-host>:8080/readyz
```

`200` means the server checked the schema and both logins and is serving. If the server exits instead, its last log line says why; see [Troubleshoot a deployment](/operate/troubleshoot-a-deployment/).

## 5. Create the first tenant

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

Give the administrator credential to the first person who will manage the tenant. They save it as `[default].api_key` in their `credentials.toml`, [connect an agent](/connect/connect-an-agent-through-mcp/), and grant roles to others; [Identity and authorization](/understand/identity-and-authorization/) explains the model.
