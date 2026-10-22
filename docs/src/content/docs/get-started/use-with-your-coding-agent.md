---
title: Use with your coding agent
description: Give your coding agent the Wyrd docs through llms.txt and Wyrd's tools through MCP with wyrd mcp install.
pillar: wyrd
section: Get started
order: 4
status: runnable
---

Point your coding agent at the Wyrd docs, then connect it to your Wyrd server so it can read your declarations, verdicts, and evidence.

## Give your agent the docs

| File | URL | Holds |
| --- | --- | --- |
| [`llms.txt`](/llms.txt) | `https://docs.bohmian.ai/wyrd/llms.txt` | An index of every page, with where to start |
| [`llms-full.txt`](/llms-full.txt) | `https://docs.bohmian.ai/wyrd/llms-full.txt` | Every page in full, with its YAML and code |

Paste either URL into your agent's chat, or add it to the agent's project instructions.

## Connect your agent over MCP

Starting from scratch? [`wyrd init guided`](/get-started/setup/#agent-guided-setup) starts the server, connects your agent, and has it set up your project. To connect an agent to a server that is already running, you need the agent installed and opened once.

```bash
wyrd mcp install --host claude-code
```

```text
claude-code: added /Users/you/.claude.json
```

Run `wyrd mcp install` without `--host` to pick from the agents it finds. Restart the agent to load the entry. The agent starts `wyrd mcp proxy`, which signs each call with the credential `wyrd init` saved; the agent's configuration file never holds it.

| Agent | `--host` |
| --- | --- |
| Claude Code | `claude-code` |
| Codex CLI and IDE | `codex` |
| Cursor | `cursor` |
| GitHub Copilot CLI | `copilot-cli` |
| GitHub Copilot in VS Code | `vscode` |
| Pi | `pi` |
| Hermes Agent | `hermes` |

## Check it worked

Ask your agent:

```text
Call the wyrd.guide tool with topic "start" and summarize what it says.
```

The agent calls `wyrd.guide` and describes the Declare → Observe → Verify workflow. Read tools work at once; write tools need a credential with the matching role. [MCP tools](/reference/mcp-tools/) lists every tool and its permission.

## Advanced

### Connect to a deployed server

Sign in to the server, then name it when you install:

```bash
wyrd auth login --server https://wyrd.example.com --tenant acme
wyrd mcp install --host claude-code --server https://wyrd.example.com
```

The URL is stored in that agent's entry only. Remote servers must use `https://`.

## If it fails

| You see | Fix |
| --- | --- |
| `WYRD_CLI_400_MCP_HOST_SELECTION`, `detected: none` | Install and open the agent once, then rerun |
| `<host>: conflict: … has a wyrd entry that does not run wyrd mcp proxy` | Rename or remove the other `wyrd` entry, then rerun |
| The agent log has `WYRD_CLI_401_NO_CREDENTIALS` | Run `wyrd init`, or `wyrd auth login` for a deployed server |
| The agent log has `WYRD_CLIENT_503_TRANSPORT_DOWN` | Start the server, then restart the agent |

## Next

[Run your first verification](/get-started/quickstart/), then ask your agent to explain its verdicts.
