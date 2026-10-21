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
uv add "wyrd-ai==0.3.0"
```

</LangTab>
<LangTab lang="rust">

```bash
cargo install wyrd-cli --version 0.3.0 --locked
cargo add wyrd-ai@0.3.0
```

</LangTab>
<LangTab lang="typescript">

```bash
npm install wyrd-ai
```

Run the CLI as `npx wyrd`.

</LangTab>
</LangTabs>

## Start the server

In its own terminal:

```bash
wyrd server dev
```

When asked, enter your database URL (`postgresql://USER:PASSWORD@HOST:5432/DATABASE`) and a tenant name, such as `acme`. Decline the other offers. When it prints this, it's ready:

```text
Local Wyrd ready at http://127.0.0.1:8080 for tenant acme.
```

It also prints a root credential once. Save it; it recovers a lost tenant key.

## Check it works

In a second terminal, from `quickstart`:

```bash
wyrd list
```

An empty list means you're connected.

## If it fails

| You see | Fix |
| --- | --- |
| `is a superuser` or `BYPASSRLS` | Use a database user that isn't a superuser |
| `WYRD_CLI_503_SERVER_DEV` naming `listener` | Port 8080 is taken: set `WYRD_SERVER_BIND=127.0.0.1:18080` and rerun |
| `WYRD_CLIENT_401_NO_CREDENTIALS` | Unset `WYRD_CONFIG_HOME`, `WYRD_API_KEY`, and `WYRD_SERVER_URL`, or match the server terminal |

## Next

[Run your first verification](/get-started/quickstart/).
