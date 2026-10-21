---
title: Set up Wyrd
description: Install the Wyrd 0.3.0 CLI and SDK for Python, Rust, or TypeScript and start a local server.
pillar: wyrd
section: Get started
order: 2
status: runnable
---

Install Wyrd, start a local server, and leave it running. Your code finds the server on its own.

## What you need

| | Minimum |
| --- | --- |
| OS | macOS on Apple silicon, or Linux (x86_64 or arm64) |
| PostgreSQL | 16, with a user that owns the database (not a superuser) |
| Language | Python 3.11, Rust 1.99, or Node.js 22.18 |

## Install

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```bash
uv init quickstart && cd quickstart
uv add "wyrd-ai==0.3.0"
```

Run the CLI as `uv run wyrd`.

</LangTab>
<LangTab lang="rust">

```bash
cargo install wyrd-cli --version 0.3.0 --locked
cargo new quickstart && cd quickstart
cargo add wyrd-ai@0.3.0 tokio --features tokio/full
```

</LangTab>
<LangTab lang="typescript">

```bash
mkdir quickstart && cd quickstart
npm init -y && npm pkg set type=module
npm install wyrd-ai
```

Run the CLI as `npx wyrd`.

</LangTab>
</LangTabs>

## Start the server

In its own terminal, in `quickstart`:

<Tabs>
<Tab label="Interactive">

```bash
wyrd server dev
```

When asked, enter your database URL (`postgresql://USER:PASSWORD@HOST:5432/DATABASE`) and a tenant name, such as `acme`. Decline the other offers.

</Tab>
<Tab label="Programmatic">

```bash
export WYRD_DATABASE_URL=postgresql://USER:PASSWORD@HOST:5432/DATABASE
wyrd server dev --tenant acme --non-interactive
```

It never prompts. If an input is missing, it names every missing input and exits before changing anything. Add `--format json` to print one line of JSON when ready, for scripts and agents.

<Aside type="note" title="wyrd server dev options">

| Option | Meaning |
| --- | --- |
| `--tenant <TENANT>` | Tenant to create on first run. Defaults to the saved tenant. |
| `--non-interactive` | Never prompt. Reports every missing input together and exits before any download, migration, or setup. |
| `--format <text\|json>` | `json` never prompts, prints one line of JSON on stdout when ready, keeps serving, and writes progress to the log file it names. Default `text`. |
| `--bind <ADDR>` | HTTP listen address, `127.0.0.1:<port>`. Overrides `WYRD_SERVER_BIND`. Default `127.0.0.1:8080`. |
| `--grpc-bind <ADDR>` | gRPC listen address, `127.0.0.1:<port>`. Overrides `WYRD_GRPC_BIND`. Default `127.0.0.1:50051`. |
| `--storage-url <URL>` | Storage URL without credentials, query, or fragment. Overrides `WYRD_STORAGE_URL`. Default is a local directory. |
| `--host <HOST>` | MCP host to connect, repeatable: `codex`, `claude-code`, `copilot-cli`, `vscode`, `cursor`, `pi`, `hermes`. None unless named. |
| `--gateway-file <PATH>` | Gateway document to apply once ready, and on every restart. |
| `--skip-gateway` | Skip the gateway offer. |

The database URL comes only from `WYRD_DATABASE_URL` or the saved setting, never an option. Each input resolves as option, then environment variable, then saved setting, then default. Settings are saved, so later runs reuse them.

</Aside>

</Tab>
</Tabs>

When it prints this, it's ready:

```text
Local Wyrd ready at http://127.0.0.1:8080 for tenant acme.
```

It also prints a root credential once. Save it; it recovers a lost tenant key.

## Check it works

In a second terminal, in `quickstart`:

```bash
wyrd list
```

An empty list means you're connected.

## If it fails

| You see | Fix |
| --- | --- |
| `is a superuser` or `BYPASSRLS` | Use a database user that isn't a superuser |
| `WYRD_CLI_503_SERVER_DEV` naming `listener` | Port 8080 is taken: rerun with `--bind 127.0.0.1:18080` |
| `WYRD_CLIENT_401_NO_CREDENTIALS` | Unset `WYRD_CONFIG_HOME`, `WYRD_API_KEY`, and `WYRD_SERVER_URL`, or match the server terminal |

## Next

[Run your first verification](/get-started/quickstart/).
