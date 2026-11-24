---
title: Deploy for a team
description: Prepare PostgreSQL with one login, migrate it, start the server, and create the first tenant.
pillar: wyrd
section: Operate
order: 1
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

The server needs two keys of its own. Create them once, store them in your secret manager, and mount the same files on every replica:

```bash
openssl genpkey -algorithm ed25519 -out signing.pem
mkdir kms
openssl rand -base64 32 | tr -d '\n' > kms/v1
chmod 0600 signing.pem kms/v1
```

The server reads only regular files that the user running it owns and nobody else can read, so mount them with that owner and mode `0600`.

The signing key signs the access tokens Wyrd issues. Every request carries one, and a replica accepts it only if this key signed it. With one shared key, a token issued by any replica works on all of them and still works after a restart. With `APP_ENV=production` and no signing key, the server refuses to start.

The `kms/v1` key encrypts the secrets Wyrd stores in PostgreSQL, such as LLM provider keys stored as `managed_secret`, identity provider client secrets, and Operator connection credentials, so a copy of the database does not expose them. Back it up separately from the database. If it is lost, those secrets cannot be decrypted, and the server refuses to start while any are stored. To replace it later, see [Rotate or recover the wrapping key](/reference/stored-secret-encryption/#rotate-or-recover-the-wrapping-key).

<Aside type="note" title="Use a cloud KMS or Vault instead">
To keep this key in Google Cloud KMS, AWS KMS, or Vault Transit, set the `WYRD_KMS_*` variables in [Stored-secret encryption](/reference/stored-secret-encryption/) instead of the three below.
</Aside>

Set these variables, then start `wyrd-server` with the same `WYRD_DATABASE_URL`:

| Variable | Value | If unset |
| --- | --- | --- |
| `WYRD_STORAGE_URL` | `s3://bucket`, `gs://bucket`, or `az://account/container`; bucket only, no path. S3 reads its region from `AWS_REGION` | Data goes to a local directory that only this process sees |
| `WYRD_SIGNING_KEY_FILE` | Path to `signing.pem` | Startup fails when `APP_ENV=production`; otherwise each process makes a temporary key, and tokens stop working after a restart and on other replicas |
| `WYRD_KMS_BACKEND` | `file` | Storing a secret fails, such as a `managed_secret` with `gateway credential protection is unavailable` |
| `WYRD_KMS_KEY_DIR` | Path to the `kms` directory | Startup fails when `WYRD_KMS_BACKEND=file` |
| `WYRD_KMS_ACTIVE_KEY` | `v1`, the key's file name | Startup fails when `WYRD_KMS_BACKEND` is set |
| `WYRD_OTLP_ENDPOINT` | Your OpenTelemetry collector's OTLP/gRPC URL, such as `http://collector:4317` | The server exports no traces to you; tenants' own timelines still work |
| `WYRD_OTLP_SAMPLE_RATIO` | The share of traces exported to your collector, from `0` to `1` | `0.01`: 1% of traces |
| `WYRD_TRACE_FILTER` | Which spans reach your collector, such as `info,wyrd_server=debug` | `info` |
| `WYRD_LOG` | Which log lines the server writes. Keep `opentelemetry_sdk` warnings: they report dropped trace exports | `info` |

```bash
curl -s -o /dev/null -w '%{http_code}\n' http://<server-host>:8080/readyz
```

`200` means the server checked the schema and the login and is serving. If the server exits instead, its last log line says why; see [Troubleshoot a deployment](/operate/troubleshoot-a-deployment/).

<Aside type="caution" title="Put the image behind a proxy that keeps each visitor's address">
Wyrd limits sign-in and signup starts per visitor address. The `bohmian/wyrd` image takes each visitor's address from the TCP connection that reaches its port 8080. Behind a load balancer that opens its own connections, every visitor arrives from the balancer's address and shares one limit. Use a balancer that preserves the client address, or set `WYRD_CLIENT_ADDRESS_HEADER` to a single-address header your edge always overwrites, such as `CF-Connecting-IP`, and make sure nothing else can reach port 8080.
</Aside>

## 4. Create the first tenant

With the server running, from a machine that has the same two database URLs exported:

```bash
wyrd-server setup --tenant acme --server http://<server-host>:8080 \
  --admin-username alice --admin-password-stdin < alice-password.txt
```

`--admin-username` names the tenant's first person, who signs in with a password and needs neither a mail relay nor an identity provider. The password is read from the first line of standard input, so it never enters the command line or your shell history; without `--admin-password-stdin`, `setup` asks for it at a hidden prompt. It must be 15 to 128 characters; a passphrase works well.

On a fresh database this creates the platform root and prints its `wyrd_global_...` credential, then creates the tenant and prints:

```text
tenant_id: <uuid>
slug: acme
admin_principal_id: <uuid>
admin_credential: <key>
This is the only time the tenant credential is shown. Store it now.
admin_username: alice
login_url: https://wyrd.example.com/t/acme/login
```

Store both credentials in your secret manager. The platform credential creates and recovers tenants; the tenant `admin_credential` is the tenant's first administrator. Give `login_url` and the password to the person named by `--admin-username`.

`setup` is safe to rerun. For an active tenant it prints `is already set up; nothing to do.` and discloses nothing. If a run was interrupted, rerun the same command: it finishes the tenant and keeps the administrator the first run created. To add another tenant later, set `WYRD_PLATFORM_CREDENTIAL` and rerun `setup` with the new slug.

## 5. Decide who can create a workspace

A new deployment is private. Nobody can create a workspace from the sign-in page, and the root page offers only sign-in to a workspace someone already knows. People join a workspace with an invitation from its administrator, which is a different step from creating one: [Add people](/operate/add-people/) covers joining.

To let visitors create their own workspaces, a platform administrator turns signup on:

```bash
curl -X PUT https://wyrd.example.com/platform/login-settings \
  -H "X-Wyrd-Access-Token: Bearer $PLATFORM_TOKEN" -H 'Content-Type: application/json' \
  -d '{"password_enabled":true,"signup_enabled":true,"signup_workspace_cap":1}'
```

The root page then shows **Create a workspace**. A visitor proves an email address or a Google or GitHub account, names the workspace and its URL, and becomes its administrator. `signup_workspace_cap` limits how many workspaces one proven identity may create; the page says so when it is reached.

Email signup, **Find my workspaces**, recovery email and mailed password reset codes need a mail relay ([Email invitations](/operate/add-people/#email-invitations)). Without one, Wyrd sends no mail and the pages say so instead of offering those steps: invitations are copied links, and an administrator issues password reset codes.

Give the administrator credential to the first person who will manage the tenant. They save it as `[default].api_key` in their `credentials.toml`, [connect an agent](/get-started/use-with-your-coding-agent/), and grant roles to others; [Identity and access](/understand/identity-and-authorization/) explains the model.

## Next

- [Add people](/operate/add-people/) connects your identity provider so your team can sign in.
- [Deploy a Service from CI](/operate/deploy-a-service-from-ci/) creates the CI and Service accounts and ships your first Service.
