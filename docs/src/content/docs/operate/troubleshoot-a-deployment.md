---
title: Troubleshoot a deployment
description: Find the cause of a Wyrd server that refuses to migrate or start, from the message it prints.
pillar: wyrd
section: Operate
order: 2
draft: true
status: partial
---

Match the code or message you see below. This page covers server installation and database failures for now.

## Server installation

`wyrd server install` prints a stable code. None of these failures changes the server you already have installed: it stays at `~/.config/wyrd/server/current` and keeps running.

| Code | Cause | Fix |
| --- | --- | --- |
| `WYRD_CLI_400_SERVER_HOST_UNSUPPORTED` | Wyrd publishes servers only for macOS and glibc Linux on x86_64 or arm64 | Run the server on a supported host or in the official container image |
| `WYRD_CLI_503_SERVER_RELEASE_UNAVAILABLE` | GitHub could not be reached, a download broke off, no stable release is published, or the newest release has no bundle for this machine | Check access to `github.com` and retry. The message says which one |
| `WYRD_CLI_422_SERVER_RELEASE_UNVERIFIED` | The signature on `checksums.txt` is missing or wrong, the download does not match its signed SHA-256, or the bundle did not unpack into a runnable server | Retry once. If it fails again, do not work around it: report the release |
| `WYRD_CLI_409_SERVER_VERSION_INCOMPATIBLE` | The newest server is on a different version line than your `wyrd` CLI | Upgrade the CLI (`pip install -U wyrd` or `npm install @wyrd/sdk@latest`), then rerun |

## Database

When the server refuses its database, it exits and its last log line says why.

| Message contains | Cause | Fix |
| --- | --- | --- |
| `wyrd._sqlx_migrations is unreadable` | The database was never migrated | Run `wyrd-server migrate` |
| `migration <version> … is not applied` | The database was migrated by an older release | Run `wyrd-server migrate` with this release |
| `is a superuser` or `BYPASSRLS` | A database URL names an administrator login | Use the ordinary logins from [Deploy Wyrd for a team](/operate/deploy-wyrd-for-a-team/) |
| `platform login <name> does not own <table>` | The two URLs are swapped | Swap them back |
| `holds TRUNCATE, REFERENCES, or TRIGGER` | The tenant login was granted too much | `REVOKE TRUNCATE, REFERENCES, TRIGGER ON ALL TABLES IN SCHEMA wyrd, vala FROM wyrd_tenant` |
| `permission denied for schema vala` | The tenant login has no grants | Run step 2 of [Deploy Wyrd for a team](/operate/deploy-wyrd-for-a-team/#2-grant-the-tenant-login-its-access) |
| `permission denied for database` during `migrate` | The platform login does not own the database | `ALTER DATABASE wyrd OWNER TO wyrd_platform` as the administrator |
