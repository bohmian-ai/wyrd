---
title: Set up Wyrd
description: Run a Wyrd server on your machine, save its first credential, and make an authenticated request.
pillar: wyrd
section: Get started
order: 2
draft: true
status: partial
---

<Aside type="tip" title="Setting up Wyrd for a team?">

This page runs a server on your own machine. To run a shared server, or to deploy the `wyrdai/wyrd` container, follow [Deploy Wyrd for a team](/operate/deploy-wyrd-for-a-team/). If you just want to try Wyrd locally, stay here.

</Aside>

`wyrd server dev --tenant acme` starts a local Wyrd server, creates the `acme` tenant, and saves its credential and endpoint so every CLI and SDK process on your machine can call it. You supply PostgreSQL; the command does the rest.

<Aside type="caution" title="Needs a published release">On first run the command downloads the newest stable server release. Until the first official release is published, that download fails, and the CLI does not build a server from a source checkout.</Aside>

## Prerequisites

- macOS or glibc Linux, on x86_64 or arm64.
- The `wyrd` CLI, which ships in the Python and TypeScript packages:

  ```bash
  pip install wyrd          # or: npm install @wyrd/sdk
  ```

- PostgreSQL 16 with an ordinary login that owns the `wyrd` database. The server refuses superuser and `BYPASSRLS` logins.

If you have Docker, this gives you a suitable database. The image's `postgres` user is a superuser, so the commands create a separate `wyrd` login:

```bash
docker run -d --name wyrd-postgres -e POSTGRES_PASSWORD=postgres -p 5432:5432 postgres:16
docker exec -i wyrd-postgres psql -U postgres <<'SQL'
CREATE ROLE wyrd LOGIN PASSWORD 'wyrd';
CREATE DATABASE wyrd OWNER wyrd;
SQL
```

A local server uses this one login for everything. A shared deployment splits it into a platform login and a tenant login; see [Deploy Wyrd for a team](/operate/deploy-wyrd-for-a-team/).

## Start local Wyrd

Run this from a directory you will keep. Local artifacts and Bifrost data live under `.wyrd/` there.

```bash
export WYRD_DATABASE_URL="postgresql://wyrd:wyrd@127.0.0.1:5432/wyrd"
wyrd server dev --tenant acme
```

The command migrates the database, starts the server, waits until it is ready, and creates the `acme` tenant. When it finishes you see:

```text
Local Wyrd ready at http://127.0.0.1:8080 for tenant acme. The saved endpoint and credential are available to fresh clients; press Ctrl-C to stop.
```

The server listens on `127.0.0.1:8080` for HTTP and `127.0.0.1:50051` for gRPC. To change them, set `WYRD_SERVER_BIND` or `WYRD_GRPC_BIND` to another loopback address before starting.

What it saved, in your Wyrd config directory (`WYRD_CONFIG_HOME`, else `$XDG_CONFIG_HOME/wyrd`, else `~/.config/wyrd`):

| File | Holds |
| --- | --- |
| `credentials.toml` | The tenant administrator's API key, as `[default].api_key`, readable only by you |
| `config.toml` | The HTTP and gRPC endpoints, as `[client].http_url` and `[client].grpc_url` |

Other settings in those files are left alone, and an existing key for a different tenant is never overwritten. To run a second local deployment, give it its own `WYRD_CONFIG_HOME`.

The first run also creates the platform root, whose credential administers tenants. It is printed once, to your terminal only, never to stdout or stderr. Store it somewhere safe; you need it to recover a lost tenant credential.

Interactive runs then offer to connect the coding agents they detect, such as Claude Code or Codex, through MCP. Accept, or skip and run [`wyrd mcp install`](/connect/connect-an-agent-through-mcp/) later. In a script, name the agents explicitly: `wyrd server dev --tenant acme --host codex`.

## Check it works

Keep the server running. In a second terminal with the same config directory:

```bash
wyrd list
```

A new tenant has no Cards, so the list is empty, and the command exits 0. You did not export a key or URL; the CLI read both from the saved config. If you have `WYRD_API_KEY`, `WYRD_ACCESS_TOKEN`, or `WYRD_SERVER_URL` exported from other work, unset them first. They take precedence over the saved config.

## Stop and restart

Press Ctrl-C in the server terminal. The CLI stops the server and waits for it to drain. To restart, run the same command from the same directory, with the same database and config directory:

```bash
wyrd server dev --tenant acme
```

Migrations are safe to rerun, and an existing tenant keeps its key. To keep your data, keep all three: the PostgreSQL database, `.wyrd/`, and the config directory.

Access tokens are signed with a key generated at startup unless you set `WYRD_SIGNING_KEY_FILE`, so a restart invalidates tokens issued before it. The CLI and SDKs exchange the saved API key for a new token on their next call; you do nothing.

## If it fails

A failed run prints `WYRD_CLI_503_SERVER_DEV`, names the stage that failed, and stops the server. Migrations and tenant creation that already committed stay. [Troubleshoot a deployment](/operate/troubleshoot-a-deployment/#local-startup) lists the fix for each stage, including how to recover a lost tenant credential.
