---
title: Set up Wyrd
description: Run a Wyrd server on your machine, save its first credential, and make an authenticated request.
pillar: wyrd
section: Get started
order: 2
status: runnable
---

`wyrd server dev` starts a local Wyrd server, creates a tenant, optionally configures the LLM gateway from provider keys in your environment, and saves everything so every CLI and SDK process on your machine can call it. You supply PostgreSQL; the command does the rest.

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

`wyrd server dev` runs in the foreground and serves until you stop it. Run it in a terminal you keep open, or as a background job you own. It is a development server: a team or production deployment runs the standalone `wyrd-server` from its container image on a platform that manages its lifetime, as in [Deploy Wyrd for a team](/operate/deploy-wyrd-for-a-team/).

```bash
wyrd server dev
```

When a coding agent does the work, you start the server and the agent connects to it once it is ready, through MCP and the saved CLI and SDK settings. The agent does not start, stop, or restart it.

On a terminal, the command asks for what it cannot find: the PostgreSQL URL (typed with echo off, for example `postgresql://wyrd:wyrd@127.0.0.1:5432/wyrd`) and the tenant slug, such as `acme`. Each input is taken from, in order, a command argument, its environment variable, the saved setting, and finally that question:

| Input | Argument | Environment | Saved in |
| --- | --- | --- | --- |
| Database | none: never an argument | `WYRD_DATABASE_URL`, optional `WYRD_PLATFORM_DATABASE_URL` | `credentials.toml` `[dev]` |
| Tenant | `--tenant` | none | `config.toml` `[dev].tenant` |
| HTTP listener | `--bind` | `WYRD_SERVER_BIND` | `config.toml` `[dev].bind` |
| gRPC listener | `--grpc-bind` | `WYRD_GRPC_BIND` | `config.toml` `[dev].grpc_bind` |
| Object storage | `--storage-url` (no credentials) | `WYRD_STORAGE_URL` | `config.toml` `[dev].storage_url` |
| Bifrost data | none | `WYRD_BIFROST_DATA_DIR` | `config.toml` `[dev].bifrost_data_dir` |

Listeners default to `127.0.0.1:8080` (HTTP) and `127.0.0.1:50051` (gRPC) and must be loopback. Without `WYRD_STORAGE_URL`, the command says so and uses `server/data/storage` and `server/data/bifrost` under the config directory, saved as absolute paths, so the working directory no longer matters. An existing `./.wyrd` directory is never adopted silently; to keep using it, pass `--storage-url file:///abs/path/.wyrd/storage` and set `WYRD_BIFROST_DATA_DIR`.

Scripts and agents pass everything up front and add `--non-interactive`. A run without a terminal, or with that flag, never asks; it lists every missing input together and exits before downloading, migrating, or creating anything:

```bash
export WYRD_DATABASE_URL="postgresql://wyrd:wyrd@127.0.0.1:5432/wyrd"
wyrd server dev --tenant acme --non-interactive
```

For a background job a script can wait on, add `--format json`. The command still serves in the foreground of that job. Once the server is ready it prints exactly one line to stdout, the readiness result, and keeps serving; progress and the server's own output go to the log file the result names, not to either stream:

```bash
wyrd server dev --tenant acme --non-interactive --format json > ready.json 2> error.json &
until [ -s ready.json ] || [ -s error.json ]; do sleep 1; done
cat ready.json error.json
```

```json
{"ready":true,"endpoint":"http://127.0.0.1:8080","grpc":"http://127.0.0.1:50051","tenant":"acme","storage_url":"file:///home/you/.config/wyrd/server/data/storage","bifrost_data_dir":"/home/you/.config/wyrd/server/data/bifrost","config_dir":"/home/you/.config/wyrd","gateway":"skipped; configure it later with wyrd gateway deployment put --file","mcp":null,"log":"/home/you/.config/wyrd/server/logs/server-dev.log"}
```

`storage_url` never carries credentials, and `mcp` is `null` unless you passed `--host`. A failure, before readiness or after it, writes one error document to stderr instead, with its stable `code`, `remediation`, and `details.stage`, and the command exits nonzero. Stop the job with `kill -INT` or by ending the terminal that owns it.

The first run downloads the newest server release, migrates the database, starts the server, saves its settings, and creates the tenant. It ends with a summary:

```text
Local Wyrd ready at http://127.0.0.1:8080 for tenant acme.
  gRPC: http://127.0.0.1:50051
  Storage: file:///home/you/.config/wyrd/server/data/storage
  Bifrost data: /home/you/.config/wyrd/server/data/bifrost
  Saved settings: /home/you/.config/wyrd/config.toml ([client], [dev]); key and database URLs: /home/you/.config/wyrd/credentials.toml
  Gateway: skipped; configure it later with wyrd gateway deployment put --file
Fresh SDK and CLI processes use these saved settings without exporting a key or endpoint. Press Ctrl-C to stop.
```

The first run also creates the platform root, whose credential administers tenants. It is printed once, to your terminal only, never to stdout or stderr. Store it somewhere safe: it is how you recover a lost tenant credential.

### Optional: the LLM gateway

A first interactive run offers the provider keys it finds in the environment: `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `GOOGLE_API_KEY` (else `GEMINI_API_KEY`), and `TYPESAFE_API_KEY` for Jev. It shows only provider, variable, and proposed deployment name, never a value. Pick any subset, rename the deployments, and confirm; the server seals the keys and creates the credentials and deployments in one transaction. Declining, or having no keys, still ends ready. No provider is called, so the summary does not claim a key works.

Unattended runs skip the offer. To configure the gateway without questions, pass a document whose credentials name environment variables, such as [`examples/gateway-setup/gateway.json`](https://github.com/bohmian-ai/wyrd/tree/main/examples/gateway-setup); `--gateway-file` applies it on every run that passes it, restarts included. `--skip-gateway` skips the offer explicitly. See [Set up the LLM gateway](/connect/set-up-the-llm-gateway/).

Interactive runs then offer to connect the coding agents they find, such as Claude Code or Codex, through MCP. Accept, or skip and connect a host later. Without a terminal, name the host; the command never writes a credential into the host's configuration, and rerunning it changes nothing that is already in place:

```bash
wyrd mcp install --host claude-code --format json
```

Restart or reconnect the host afterwards so it loads the Wyrd server. A new agent then calls `wyrd.guide` with `{"topic":"start"}`, as the server's instructions tell it to. [Connect an agent through MCP](/connect/connect-an-agent-through-mcp/) lists every host.

What the command saved, in your Wyrd config directory (`WYRD_CONFIG_HOME`, else `$XDG_CONFIG_HOME/wyrd`, else `~/.config/wyrd`):

| File | Holds |
| --- | --- |
| `credentials.toml` | The tenant administrator's API key, as `[default].api_key`, and the database URLs, as `[dev]`, readable only by you |
| `config.toml` | The HTTP and gRPC endpoints, as `[client].http_url` and `[client].grpc_url`, and the non-secret `[dev]` settings above |
| `server/sealing.key` | The key that encrypts stored secrets such as gateway provider keys, readable only by you |
| `server/data/` | Local object storage and Bifrost data, unless you chose other locations |

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

Press Ctrl-C in the server terminal. To restart, from any directory, with nothing exported:

```bash
wyrd server dev
```

The restart reuses the saved database, listeners, tenant, data paths, key, and sealing key, and asks nothing. Migrations are safe to rerun, and an existing tenant keeps its key; the command never issues a replacement just because it restarted. An argument or environment variable overrides only its own saved setting. A different `--tenant` while a key is saved is refused, as is a database whose server does not accept the saved key (a saved `wyrd auth login` never stands in for it), a saved key that local setup did not create, or a config directory that already selects another Wyrd server; use a separate `WYRD_CONFIG_HOME` for another tenant or deployment. Changing only a listener, such as `--bind`, resumes the same deployment without repeating the gateway or MCP host offers.

Your data lives in two places; keep both: the PostgreSQL database and the config directory. Lose `server/sealing.key` and stored provider keys can no longer be decrypted, so you would put them again. A key you supply through `WYRD_SEALING_KEY_FILE` or `WYRD_SEALING_KEY_BASE64` on the first run is kept there, so later runs need no variable; a different key on a later run is refused rather than replacing it.

## If it fails

| You see | Cause | Fix |
| --- | --- | --- |
| `WYRD_CLI_503_SERVER_DEV` naming the `input` stage | An input is missing or invalid | Set the named variable or argument, or run on a terminal to be asked |
| `WYRD_CLI_503_SERVER_DEV` naming the `migration` stage | The database is unreachable, or the login does not own it | Check `WYRD_DATABASE_URL` and the `OWNER` of the `wyrd` database, then rerun |
| `WYRD_CLI_503_SERVER_DEV` naming the `listener` stage | Port 8080 is taken | Set `WYRD_SERVER_BIND` to a free loopback address and rerun |
| `is a superuser` or `BYPASSRLS` | `WYRD_DATABASE_URL` names an administrator login | Use the `wyrd` login created above |
| `WYRD_CLI_503_SERVER_DEV` naming the `local persistence` stage | The config directory is not writable | Restore its permissions and rerun; no tenant key was issued |
| `WYRD_CLI_503_SERVER_DEV` naming the `saved credential` stage after the tenant is active | The one-time tenant key was lost | Rerun on a terminal with `WYRD_PLATFORM_CREDENTIAL` set to the platform root and confirm rotation, or run `wyrd platform tenant recover-admin` |
| A gateway problem after the server is ready | Gateway setup did not complete; tenant setup remains. A refused document applies nothing, but if the response was lost the changes may have committed | Inspect with `wyrd gateway deployment list`, correct the document or keys, and rerun; reapplying the same named document is safe. Or pass `--skip-gateway` |
| `WYRD_CLI_400_SERVER_HOST_UNSUPPORTED` | No server release exists for this machine | Use a supported host from the prerequisites |
| `WYRD_CLIENT_401_NO_CREDENTIALS` from `wyrd list` or `connect` | No saved credential is visible to this process | Use the same `WYRD_CONFIG_HOME` as the server terminal |
| Calls go to the wrong server, or a key is refused | `WYRD_API_KEY`, `WYRD_ACCESS_TOKEN`, or `WYRD_SERVER_URL` is exported from other work, and it wins over the saved config | Unset them |

A failed run stops the server. Migrations and tenant creation that already committed stay, so rerunning the same command picks up where it stopped.

## Next

[Verify your first agent](/get-started/quickstart/): run a support agent against this server and get verdicts on its answers.
