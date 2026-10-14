---
title: Troubleshoot a deployment
description: Find the cause of a Wyrd server that refuses to migrate or start, from the message it prints.
pillar: wyrd
section: Operate
order: 5
draft: true
status: partial
---

Match the code or message you see below. This page covers server installation, local setup recovery, and database failures.

## Server installation

`wyrd server install` prints a stable code. None of these failures changes the server you already have installed: it stays under the existing Wyrd config directory's `server/current` and keeps running.

| Code | Cause | Fix |
| --- | --- | --- |
| `WYRD_CLI_400_SERVER_HOST_UNSUPPORTED` | Wyrd publishes servers only for macOS on arm64 and glibc Linux on x86_64 or arm64 | Run the server on a supported host, or use the `bohmian/wyrd` container image |
| `WYRD_CLI_503_SERVER_RELEASE_UNAVAILABLE` | GitHub could not be reached, a download broke off, no stable release is published, or the newest release has no bundle for this machine | Check access to `github.com` and retry. The message says which one |
| `WYRD_CLI_422_SERVER_RELEASE_UNVERIFIED` | The signature on `checksums.txt` is missing or wrong, the download does not match its signed SHA-256, or the bundle did not unpack into a runnable server | Retry once. If it fails again, do not work around it: report the release |
| `WYRD_CLI_409_SERVER_VERSION_INCOMPATIBLE` | The newest server is on a different version line than your `wyrd` CLI | Upgrade the CLI (`pip install -U wyrd` or `npm install wyrd-ai@latest`), then rerun |

## Local startup

`WYRD_CLI_503_SERVER_DEV` identifies the failed stage and a redacted recovery action. The CLI stops its serving child on failure. Committed migrations or tenant provisioning remain.

| Failed stage | Recovery |
| --- | --- |
| Input or listener | Set the database URL or choose an available loopback address, then rerun. |
| Migration | Check connectivity and that the migration login owns the database objects. Rerun with the same database. |
| Readiness | Read the server's diagnostics, correct its configuration, and rerun. Production hardening still applies when `APP_ENV=production`. |
| Local persistence | Restore safe directory/file permissions and rerun. A key already saved before an endpoint-write failure is retained. |
| Saved credential | Restore the requested tenant's original key. Rerunning never issues a replacement for an active tenant. Use explicit platform tenant administration recovery if the one-time key was lost. |

If the platform root exists but tenant creation failed, rerun `wyrd server dev` with the stored `WYRD_PLATFORM_CREDENTIAL` set. Retries never recover or reissue a credential on their own.

### Recover a lost tenant credential

A failed `server dev` stops its server, so recovery starts the installed server directly.

1. Start the installed server with the same database URL. On macOS, also set `WYRD_BIFROST_MEMORY_LIMIT_BYTES` to the output of `sysctl -n hw.memsize`; `server dev` normally supplies it.

   ```bash
   export WYRD_SERVER_BIND=127.0.0.1:8080
   ~/.config/wyrd/server/current/wyrd-server
   ```

   With `WYRD_CONFIG_HOME` or `XDG_CONFIG_HOME` set, use that directory's `server/current/wyrd-server`.

2. In a second terminal, export the same database URL. If the platform credential is also lost, run the installed server's `recover-root` command and store its one-time output securely.
3. Set `WYRD_PLATFORM_CREDENTIAL` in the environment (not on the command line), then issue a replacement administrator credential for the tenant UUID printed before the failure:

   ```bash
   wyrd platform tenant recover-admin --tenant YOUR_TENANT_UUID --server http://127.0.0.1:8080
   ```

   The new credential prints once.
4. Under `umask 077`, write it to `[default].api_key` in `credentials.toml`, and delete any old `access_token` and `access_expires_at` values. Confirm the file mode is `0600`.
5. Stop the directly started server, then rerun `wyrd server dev --tenant acme`.

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
