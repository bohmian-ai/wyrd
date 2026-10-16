---
title: Set up Wyrd
description: Run a Wyrd server on your machine, save its first credential, and make an authenticated request.
pillar: wyrd
section: Get started
order: 2
status: runnable
---

`wyrd server dev --tenant acme` starts a local Wyrd server, creates the `acme` tenant, and saves its credential and endpoint so every CLI and SDK process on your machine can call it. You supply PostgreSQL; the command does the rest.

## Prerequisites

- macOS on Apple silicon, or glibc Linux on x86_64 or arm64.
- The `wyrd` CLI, which ships in the Python and TypeScript packages:

  ```bash
  pip install wyrd-ai          # or: npm install -g wyrd-ai
  ```

- PostgreSQL 16 with an ordinary login that owns the `wyrd` database. The server refuses superuser and `BYPASSRLS` logins.

With Docker, this gives you a suitable database. The image's `postgres` user is a superuser, so the commands create a separate `wyrd` login:

```bash
docker run -d --name wyrd-postgres -e POSTGRES_PASSWORD=postgres -p 5432:5432 postgres:16
docker exec -i wyrd-postgres psql -U postgres <<'SQL'
CREATE ROLE wyrd LOGIN PASSWORD 'wyrd';
CREATE DATABASE wyrd OWNER wyrd;
SQL
```

## Start local Wyrd

Run this from a directory you will keep. Local artifacts and Bifrost data live under `.wyrd/` there.

```bash
export WYRD_DATABASE_URL="postgresql://wyrd:wyrd@127.0.0.1:5432/wyrd"
wyrd server dev --tenant acme
```

The first run downloads the newest server release, migrates the database, starts the server, and creates the `acme` tenant. It ends with:

```text
Migrating the installed Wyrd server <version>.
Tenant acme (<tenant-id>) is active; saving local client configuration.
Local Wyrd ready at http://127.0.0.1:8080 for tenant acme. The saved endpoint and credential are available to fresh clients; press Ctrl-C to stop.
```

The server listens on `127.0.0.1:8080` for HTTP and `127.0.0.1:50051` for gRPC; set `WYRD_SERVER_BIND` or `WYRD_GRPC_BIND` to another loopback address to change them.

The first run also creates the platform root, whose credential administers tenants. It is printed once, to your terminal only, never to stdout or stderr. Store it somewhere safe: it is how you recover a lost tenant credential.

Interactive runs then offer to connect the coding agents they find, such as Claude Code or Codex, through MCP. Accept, or skip and run `wyrd mcp install` later.

What the command saved, in your Wyrd config directory (`WYRD_CONFIG_HOME`, else `$XDG_CONFIG_HOME/wyrd`, else `~/.config/wyrd`):

| File | Holds |
| --- | --- |
| `credentials.toml` | The tenant administrator's API key, as `[default].api_key`, readable only by you |
| `config.toml` | The HTTP and gRPC endpoints, as `[client].http_url` and `[client].grpc_url` |
| `server/sealing.key` | The key that encrypts stored secrets such as gateway provider keys, readable only by you |

## Check it works

Keep the server running. In a second terminal:

```bash
wyrd list
```

A new tenant has no Cards, so the list is empty and the command exits 0. You exported no key or URL; the CLI read both from the saved config.

The SDKs read the same config, so code needs no URL or key either:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
from wyrd.bifrost import Bifrost

bifrost = Bifrost.connect()
print([table["name"] for table in bifrost.list_tables()])
```

</LangTab>
<LangTab lang="rust">

```rust
use wyrd_ai::{Bifrost, bifrost::BifrostOptions};

let bifrost = Bifrost::connect(BifrostOptions::default()).await?;
for table in bifrost.list_tables().await? {
    println!("{}.{}", table.namespace, table.name);
}
```

</LangTab>
<LangTab lang="typescript">

```typescript
import { Bifrost } from "wyrd-ai";

const bifrost = await Bifrost.connect();
console.log((await bifrost.listTables()).map((table) => table.name));
```

</LangTab>
</LangTabs>

Even a new tenant lists Wyrd's built-in tables. `connect` checks the credential before it returns, so a missing or refused key fails here, not on your first real call.

## Stop and restart

Press Ctrl-C in the server terminal. To restart, run the same command from the same directory:

```bash
wyrd server dev --tenant acme
```

Migrations are safe to rerun, and an existing tenant keeps its key. Your data lives in three places; keep all of them: the PostgreSQL database, `.wyrd/`, and the config directory. Lose `server/sealing.key` and stored provider keys can no longer be decrypted, so you would put them again.

## If it fails

| You see | Cause | Fix |
| --- | --- | --- |
| `WYRD_CLI_503_SERVER_DEV` naming the `migration` stage | The database is unreachable, or the login does not own it | Check `WYRD_DATABASE_URL` and the `OWNER` of the `wyrd` database, then rerun |
| `WYRD_CLI_503_SERVER_DEV` naming the `listener` stage | Port 8080 is taken | Set `WYRD_SERVER_BIND` to a free loopback address and rerun |
| `is a superuser` or `BYPASSRLS` | `WYRD_DATABASE_URL` names an administrator login | Use the `wyrd` login created above |
| `WYRD_CLI_400_SERVER_HOST_UNSUPPORTED` | No server release exists for this machine | Use a supported host from the prerequisites |
| `WYRD_CLIENT_401_NO_CREDENTIALS` from `wyrd list` or `connect` | No saved credential is visible to this process | Use the same `WYRD_CONFIG_HOME` as the server terminal |
| Calls go to the wrong server, or a key is refused | `WYRD_API_KEY`, `WYRD_ACCESS_TOKEN`, or `WYRD_SERVER_URL` is exported from other work, and it wins over the saved config | Unset them |

A failed run stops the server. Migrations and tenant creation that already committed stay, so rerunning the same command picks up where it stopped.

## Next

[Set up the LLM gateway](/connect/set-up-the-llm-gateway/) to call a model through Wyrd with this credential.
