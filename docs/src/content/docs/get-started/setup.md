---
title: Set up Wyrd
description: Run a Wyrd server on your machine, create its first credential, and make an authenticated request.
pillar: wyrd
section: Get started
order: 2
draft: true
status: partial
---

`wyrd server dev --tenant acme` starts a local Wyrd server and saves its tenant credential and endpoint for fresh CLI and SDK processes. Supply PostgreSQL first and start the command from a directory you will keep: local artifacts and Bifrost state live under `.wyrd/` there.

## Prerequisites

- macOS or glibc Linux, on x86_64 or arm64.
- An installed Python or TypeScript Wyrd package, which includes `wyrd`.
- PostgreSQL 16 with an ordinary login that owns the database objects. The server refuses superuser and `BYPASSRLS` logins.

The command uses an existing installed compatible server. When none exists, it downloads and verifies the newest stable release. It never updates an existing installation implicitly; `wyrd server install` is the explicit update command. Before the first official release, a download cannot succeed. An existing development installation can exercise the journey, but the CLI does not fall back to building a server from a checkout.

## Start local Wyrd

Set the URL of your external database; replace the login and password with yours:

```bash
export WYRD_DATABASE_URL="postgresql://wyrd:YOUR_PASSWORD@127.0.0.1:5432/wyrd"
wyrd server dev --tenant acme
```

The command migrates the database, starts the ordinary server, waits for readiness, and provisions `acme` through the server. On macOS it supplies the same host-memory input as the repository's development launcher when you have not set `WYRD_BIFROST_MEMORY_LIMIT_BYTES`.

The default HTTP endpoint is `http://127.0.0.1:8080`. To use a different local port, set `WYRD_SERVER_BIND` to a loopback address before starting. The default gRPC port remains `50051`.

On first use, the tenant key is saved in `credentials.toml`, with owner-only permissions. The endpoint is saved in `config.toml`. Both files use the existing Wyrd config directory (`WYRD_CONFIG_HOME`, otherwise `XDG_CONFIG_HOME/wyrd` or `~/.config/wyrd`). Unrelated settings and comments remain intact; another tenant's default key is never overwritten. Use a separate `WYRD_CONFIG_HOME` for another local deployment.

If the platform root is newly initialized, its one-time administrative credential is disclosed only to the invoking terminal. Store it securely. When no terminal is available, the command prints recovery guidance without printing the root credential to stdout or stderr. Tenant credentials are never printed.

Interactive runs offer detected MCP hosts. Select the hosts you want, or decline and keep using the CLI and SDKs. Scripts connect no hosts unless selected explicitly:

```bash
wyrd server dev --tenant acme --host codex
```

Keep this process running. After the `Local Wyrd ready at …` message, open a second terminal with the same configuration directory:

```bash
wyrd list
```

A new tenant has no Cards, so the list is empty. No exported API key or server URL is needed. Explicit credentials and endpoint overrides still take precedence for ordinary clients; remove unrelated overrides when testing the saved local configuration.

## Stop and restart

Press Ctrl-C in the server terminal. The CLI forwards termination and waits for the server to drain. Restart from the same working directory, database, tenant, and config directory:

```bash
wyrd server dev --tenant acme
```

Migrations are safe to rerun. An active tenant keeps its existing key, and the installed release is retained. PostgreSQL, `.wyrd/`, and the config directory must all survive if you want to keep the deployment. Without a configured persistent signing key, a restart invalidates previously issued access tokens; clients can exchange the saved API key again.

## Recover a failed setup

`WYRD_CLI_503_SERVER_DEV` identifies the failed stage and a redacted recovery action. The CLI stops its serving child on failure. Committed migrations or tenant provisioning remain.

| Failed stage | Recovery |
| --- | --- |
| Input or listener | Set the database URL or choose an available loopback address, then rerun. |
| Migration | Check connectivity and that the migration login owns the database objects. Rerun with the same database. |
| Readiness | Read the server's diagnostics, correct its configuration, and rerun. Production hardening still applies when `APP_ENV=production`. |
| Local persistence | Restore safe directory/file permissions and rerun. A key already saved before an endpoint-write failure is retained. |
| Saved credential | Restore the requested tenant's original key. Rerunning never issues a replacement for an active tenant. Use explicit platform tenant administration recovery if the one-time key was lost. |

For a lost credential or partially provisioned tenant, follow [Troubleshoot a deployment](/operate/troubleshoot-a-deployment/#local-startup). Automatic retries never recover or reissue credentials.
