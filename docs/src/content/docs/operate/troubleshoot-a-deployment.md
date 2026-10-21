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
| `WYRD_CLI_409_SERVER_INSTALL_INCOMPLETE` | The newest version is already unpacked on this machine but part of it, such as the UI or its dependencies, is missing | Delete the directory the message names, then rerun `wyrd server install` to download it again. Nothing was activated |
| `WYRD_CLI_409_SERVER_VERSION_INCOMPATIBLE` | The newest server is on a different version line than your `wyrd` CLI | Upgrade the CLI (`pip install -U wyrd-ai` or `npm install -g wyrd-ai@latest`), then rerun |

## Local startup

`WYRD_CLI_503_SERVER_DEV` identifies the failed stage and a redacted recovery action. The CLI stops its serving child on failure. Committed migrations or tenant provisioning remain.

| Failed stage | Recovery |
| --- | --- |
| Input or listener | Supply every input the message lists (an unattended run lists all of them before doing anything), or choose an available loopback address, then rerun. |
| Migration | Check connectivity and that the migration login owns the database objects. Rerun with the same database. |
| Readiness | Read the server's diagnostics, correct its configuration, and rerun. Production hardening still applies when `APP_ENV=production`. |
| Local persistence | Restore safe directory/file permissions and rerun. Settings are saved before the tenant is created, so a failure there issues no key; a key already saved before an endpoint-write failure is retained. |
| Saved credential | Restore the requested tenant's original key. Rerunning never issues a replacement for an active tenant on its own. If the one-time key was lost, rerun on a terminal with `WYRD_PLATFORM_CREDENTIAL` set and confirm rotation, or follow the steps below. |
| Gateway | Gateway setup did not complete; tenant setup remains. A refused document applies nothing, but if the response was lost the changes may have committed. Inspect with `wyrd gateway deployment list`, then rerun; reapplying the same named document is safe. Or pass `--skip-gateway`. |

If the platform root exists but tenant creation failed, rerun `wyrd server dev` with the stored `WYRD_PLATFORM_CREDENTIAL` set. Retries never recover or reissue a credential on their own.

### Recover a lost tenant credential

On a terminal, `wyrd server dev` does this for you: with `WYRD_PLATFORM_CREDENTIAL` set, it asks before rotating the tenant administrator's key through the platform and saves the new one. Unattended runs never rotate a key; they stop with this guidance.

To recover by hand: a failed `server dev` stops its server, so recovery starts the installed server directly.

1. Start the installed server with the same database URL.

   ```bash
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
5. Stop the directly started server, then rerun `wyrd server dev`.

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

## Stored secrets

| Message contains | Cause | Fix |
| --- | --- | --- |
| `gateway credential protection is unavailable` | The server has no sealing key, so it cannot encrypt a `managed_secret` provider key | Set `WYRD_SEALING_KEY_FILE` as in [Deploy Wyrd for a team](/operate/deploy-wyrd-for-a-team/#4-start-the-server-and-check-readiness) and restart. `wyrd server dev` creates one for you |
| `exist but no sealing key is configured` at startup | Encrypted secrets are stored, and the server started without the key they were sealed under | Start with the same `WYRD_SEALING_KEY_FILE` as before |
| A gateway call fails with an upstream-unavailable error after a key change | The provider key was sealed under a key the server no longer holds | List the old key in `WYRD_SEALING_RETAINED_KEYS_FILE` and restart, or put the credential again |
