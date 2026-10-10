---
title: Deploy Wyrd for a team
description: Prepare PostgreSQL with a platform login and a tenant login, migrate it, and confirm the server is ready.
pillar: wyrd
section: Operate
order: 1
draft: true
status: partial
---

A shared Wyrd server runs with two PostgreSQL logins. The platform login owns Wyrd's tables and runs migrations. The tenant login serves requests and does not own any table, so PostgreSQL row-level security keeps it inside one tenant at a time, even if the server process is compromised.

This page covers the database. Set `WYRD_STORAGE_URL` to an object store every replica can reach; when it is unset, the server stores data in a local `.wyrd/storage` directory that only that process sees. The signing key and the container image are not documented here yet.

## Prerequisites

- PostgreSQL 16, and an administrator login that can create logins and databases.
- The `wyrd-server` binary for the release you are deploying.

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

Start `wyrd-server` with the same two URLs, then:

```bash
curl -s -o /dev/null -w '%{http_code}\n' http://<server-host>:8080/readyz
```

`200` means the server checked the schema and both logins and is serving. If the server exits instead, see [Troubleshoot a deployment](/operate/troubleshoot-a-deployment/).
