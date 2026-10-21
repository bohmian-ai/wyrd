---
title: Connect an agent through MCP
description: Give Claude Code, Codex, Cursor, Copilot, Pi, or Hermes Agent Wyrd's tools with wyrd mcp install, against a local or deployed server.
pillar: wyrd
section: Connect
order: 3
draft: true
status: runnable
---

`wyrd mcp install` adds Wyrd to your coding agent's MCP servers. The agent then launches `wyrd mcp proxy`, which serves your Wyrd server's tools over stdio and signs each request with your ordinary Wyrd credential. The agent's configuration file never holds that credential.

If you ran [`wyrd server dev`](/get-started/setup/) and accepted its offer to connect agents, those agents are already connected with a saved credential. Restart them and skip to [step 4](#4-check-the-connection).

## Prerequisites

- The `wyrd` CLI on your `PATH` (`pip install wyrd-ai` or `npm install wyrd-ai`).
- A Wyrd server you can reach: the local one from [Set up Wyrd](/get-started/setup/), or a deployed one.
- At least one supported agent installed and opened once, so its configuration directory exists.

| Agent | `--host` | File `install` edits |
| --- | --- | --- |
| Codex CLI and IDE | `codex` | `~/.codex/config.toml` (or `$CODEX_HOME`) |
| Claude Code | `claude-code` | `~/.claude.json` |
| GitHub Copilot CLI | `copilot-cli` | `~/.copilot/mcp-config.json` (or `$COPILOT_HOME`) |
| GitHub Copilot in VS Code | `vscode` | your VS Code user profile's `mcp.json` |
| Cursor | `cursor` | `~/.cursor/mcp.json` |
| Pi | `pi` | `~/.pi/agent/mcp.json` (or `$PI_CODING_AGENT_DIR`) |
| Hermes Agent | `hermes` | `~/.hermes/config.yaml` (or `$HERMES_HOME`) |

## 1. Store your credential where the agent can find it

Agents started from the Dock or a launcher do not see variables you exported in a shell, so `WYRD_API_KEY` in your terminal is not enough. Save the credential in Wyrd's client configuration instead.

For a deployed server with single sign-on, sign in. The saved login applies only to that server:

```bash
wyrd auth login --server https://wyrd.example.com --tenant acme
```

For the local server, `wyrd server dev` already saved the tenant key in `credentials.toml`; skip to step 2. For any other server where you hold an API key, such as the `admin_credential` from [Deploy Wyrd for a team](/operate/deploy-wyrd-for-a-team/#5-create-the-first-tenant), put the key in `credentials.toml` in your Wyrd config directory, readable only by you. It is used for every server you have no saved login for. This overwrites the file, so check that it holds no key you still need:

```bash
umask 077
WYRD_DIR="${WYRD_CONFIG_HOME:-${XDG_CONFIG_HOME:-$HOME/.config}/wyrd}"
mkdir -p "$WYRD_DIR"
printf '[default]\napi_key = "%s"\n' "$WYRD_API_KEY" > "$WYRD_DIR/credentials.toml"
chmod 600 "$WYRD_DIR/credentials.toml"
```

The proxy reads the same sources as every Wyrd client, in this order: `WYRD_ACCESS_TOKEN`, workload identity, `WYRD_API_KEY`, your saved login for that server, then `credentials.toml`. It resolves them each time the agent starts it, so a renewed login or a rotated key takes effect without reinstalling.

## 2. Connect your agents

On a terminal, run `install` and pick from the agents it detects:

```bash
wyrd mcp install
```

In a script, or to skip the prompt, name each agent:

```bash
wyrd mcp install --host claude-code --host cursor
```

```text
claude-code: added /Users/you/.claude.json
cursor: added /Users/you/.cursor/mcp.json
```

Only the agents you choose change, and only their `wyrd` entry. Running the command again reports `unchanged` and rewrites nothing. Restart the agent, or reload its MCP servers, to pick up the entry.

## 3. Choose the server

Without `--server`, the agent follows your global client endpoint: `[client].http_url` in `config.toml`, which `wyrd server dev` sets to `http://127.0.0.1:8080`, else `WYRD_SERVER_URL`, else `http://localhost:8080`.

To point an agent at a deployed server, pass it explicitly:

```bash
wyrd mcp install --host claude-code --server https://wyrd.example.com
```

The URL is stored in that agent's entry; open the file from the table above to see which server an agent uses. Rerunning `install` with a different `--server`, or without one, reports `updated`. Your global `config.toml` is not changed, so other tools keep using the local server. Remote servers must use `https://`. Plain `http://` is accepted only for `localhost` and loopback addresses.

## 4. Check the connection

Ask the agent to list its MCP tools, then call `wyrd.guide` with `{"topic":"overview"}`. The guide returns `topic`, ordered `steps`, and `examples` containing tool names and argument objects. Its other topics are `start`, `cards`, `verification`, and `bifrost_sql`. A coding agent new to Wyrd starts with `{"topic":"start"}`, which the server's initialization instructions name: it teaches the whole Declare → Observe → Verify workflow, including the CLI and SDK steps that MCP does not perform.

Try a question about a Service you declared:

```text
Analyze service support-desk in space default. Discover its exact version, follow its component
references, and explain what the retained evidence supports. State missing
evidence and permission refusals explicitly.
```

The agent should discover versions with `cards.list`, read one with `cards.get` and its `card_ref`, and follow the exact references in `card.relationships.outbound_refs` and `inbound_refs`. Each edge stores its identity under `ref`. It should discover and describe Bifrost tables before querying evidence.

Read the [MCP tool reference](/reference/mcp-tools/) for every shipped capability and its permissions. For complete procedures, see [Write and query custom data with Bifrost](/build-and-verify/observe/write-and-query-custom-data-with-bifrost/) and [Run a verification and inspect its evidence](/build-and-verify/verify/run-a-verification-and-inspect-its-evidence/).

Tool discovery grants no permission. The server checks your credential's roles on each call, exactly as for any other Wyrd client. A query error or empty result does not establish that a component passed verification.

## What the entry contains

`install` writes the absolute path of your `wyrd`, the arguments `mcp proxy` (plus `--server <url>` when you chose one), and one environment variable, `WYRD_CONFIG_HOME`, set to your Wyrd configuration directory. The variable matters because some agents start MCP servers with a reduced environment; it keeps the proxy reading your saved login and `credentials.toml`. Neither value is a secret.

If you move or reinstall `wyrd` elsewhere, rerun `install`. It reports `updated` and rewrites the path.

## What can go wrong

`install` prints one line per agent and exits non-zero if any agent was not connected. An agent that failed keeps its file exactly as it was; the others still install.

| Symptom | Cause | Fix |
| --- | --- | --- |
| `WYRD_CLI_400_MCP_HOST_SELECTION`, `detected: none` | No supported agent's configuration directory exists | Install and open the agent once, then rerun |
| `WYRD_CLI_400_MCP_HOST_SELECTION`, `detected: …` | A script ran `install` without `--host` | Pass `--host` for each agent the message lists |
| `<host>: not detected; nothing changed` | You named an agent that is not installed for this user | Install and open it, or drop that `--host` |
| `<host>: conflict: … has a wyrd entry that does not run wyrd mcp proxy` | Another MCP server already uses the name `wyrd` | Rename or remove that entry, then rerun |
| `<host>: cannot edit …` | The file is not valid JSON, TOML, or YAML, or Hermes's `mcp_servers` is written inline, such as `mcp_servers: {}` | Fix the file. For Hermes, put `mcp_servers:` on its own line with each server indented under it, then rerun |
| `<host>: cannot write …` | The file or its directory is read-only | Fix the permissions, then rerun |
| `WYRD_CLI_400_CLIENT_CONFIG`, `remote cleartext HTTP is not allowed` | `--server` is `http://` on a non-loopback host | Use the server's `https://` URL |
| Agent shows the Wyrd server as failed; its log has `WYRD_CLI_401_NO_CREDENTIALS` | The proxy found no credential | Do step 1. A shell `export` does not reach an agent started from the Dock |
| Agent log has `WYRD_CLIENT_503_TRANSPORT_DOWN` | The server is not running or not reachable at that URL | Start the server. Check the URL in the agent's entry (step 3) and your network |
| A tool call returns `WYRD_PERMISSION_403_DENIED_RBAC` | Your credential lacks the role for that tool | Use a credential with the role, or ask an administrator to grant it |

`wyrd mcp install --help` lists every flag.
