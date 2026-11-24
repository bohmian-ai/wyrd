---
title: Set up Wyrd
description: Install the Wyrd 0.4.2 CLI and SDK for Python, Rust, or TypeScript, start a local server with your coding agent or by hand, and check it with a verification that needs no model.
pillar: wyrd
section: Get started
order: 2
---

Install Wyrd, then set up the rest with your coding agent or by hand. Either way you end with a local server running and your code connected to it.

## What you need

| | Minimum |
| --- | --- |
| OS | macOS on Apple silicon, or Linux (x86_64 or arm64) |
| PostgreSQL | 16, with a user that owns the database (not a superuser) |
| Language | Python 3.11, Rust 1.95, or Node.js 22.18 |

## Install

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```bash
uv init quickstart && cd quickstart
uv add "wyrd-ai==0.4.2"
```

Run the CLI as `uv run wyrd`.

</LangTab>
<LangTab lang="rust">

```bash
cargo install wyrd-cli --version 0.4.2 --locked
cargo new quickstart && cd quickstart
cargo add wyrd-ai@0.4.2 tokio serde schemars --features tokio/full,serde/derive
```

</LangTab>
<LangTab lang="typescript">

```bash
mkdir quickstart && cd quickstart
pnpm init && pnpm pkg set type=module
pnpm add wyrd-ai@0.4.2
```

Run the CLI as `pnpm wyrd`.

</LangTab>
</LangTabs>

## Agent-guided setup

Let your coding agent finish the setup. Wyrd starts a local server and connects your agent to it, and the agent writes and runs your first verification. Without a coding agent, use [Manual setup](#manual-setup).

In `quickstart`, run:

```bash
wyrd init guided
```

It asks for:

- the coding agent to use
- your database URL, `postgresql://USER:PASSWORD@HOST:5432/DATABASE`
- a tenant name, such as `acme`

If a model provider key such as `OPENAI_API_KEY` is set, it also offers to add that provider to the gateway. Accept it to skip the provider step of the [quickstart](/get-started/quickstart/).

It then starts a local server and hands over to the agent, which runs the [check](#3-check-it-end-to-end) below and reports one `passed` and one `failed` verdict. Leave the terminal open; the server runs until you press Ctrl-C.

<Aside type="note" title="Another agent, or your own">

Name the agent with `--host`, such as `wyrd init guided --host claude-code`. For an agent `wyrd` does not launch, start the server by hand, then paste the output of `wyrd init --print-prompt`, or select the Wyrd MCP prompt `setup`.

</Aside>

## Manual setup

### 1. Start the server

In its own terminal, in `quickstart`:

```bash
wyrd init
```

Enter your database URL and a tenant name when asked, and decline the other offers. When it prints this, it's ready:

```text
Local Wyrd ready at http://127.0.0.1:8080 for tenant acme.
```

It also prints two secrets once, on the terminal only. Save both:

- a root credential, which recovers a lost tenant key;
- the password of the tenant's `admin` user, with the page where you sign in to the Wyrd UI.

<Aside type="note" title="Start it without prompts">

```bash
export WYRD_DATABASE_URL=postgresql://USER:PASSWORD@HOST:5432/DATABASE
wyrd init --tenant acme --non-interactive
```

If an input is missing, it names every missing input and exits before changing anything. Add `--format json` to print one line of JSON when ready.

`wyrd init --help` lists every option.

</Aside>

### 2. Check it works

In a second terminal, in `quickstart`:

```bash
wyrd list
```

An empty list means you're connected. Your code finds the server the same way, with no configuration.

### 3. Check it end to end

This check declares a requirement and verifies two support answers against it. It calls no model.

Save the Verifier in `quickstart` as `no-refund-promise.yaml`, and the Service it verifies as `support-reply.yaml`:

<Tabs>
<Tab label="no-refund-promise.yaml">

<CodeFromFile file="examples/setup-check/no-refund-promise.yaml" lang="yaml" title="no-refund-promise.yaml" />

</Tab>
<Tab label="support-reply.yaml">

<CodeFromFile file="examples/setup-check/support-reply.yaml" lang="yaml" title="support-reply.yaml" />

</Tab>
</Tabs>

Save the program beside them and run it:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

<CodeFromFile file="examples/setup-check/check.py" lang="python" title="check.py" />

```bash
uv run check.py
```

</LangTab>
<LangTab lang="rust">

<CodeFromFile file="examples/setup-check/check.rs" lang="rust" title="src/main.rs" />

```bash
cargo run
```

</LangTab>
<LangTab lang="typescript">

<CodeFromFile file="examples/setup-check/check.ts" lang="typescript" title="check.ts" />

```bash
node check.ts
```

</LangTab>
</LangTabs>

```text
passed: the comparison held
  subject:  Service/support-reply@1.0.0#<uid>
  verifier: default/Verifier/no-refund-promise@1.0.0#<uid>
failed: operator NotContains returned false
  subject:  Service/support-reply@1.0.0#<uid>
  verifier: default/Verifier/no-refund-promise@1.0.0#<uid>
```

A `failed` verdict is a result, not an error: the second answer promises a guaranteed refund. Your `uid` values differ; the server assigns them.

## If it fails

| You see | Fix |
| --- | --- |
| `is a superuser` or `BYPASSRLS` | Use a database user that isn't a superuser |
| `WYRD_CLI_503_SERVER_DEV` naming `listener` | Port 8080 is taken: rerun with `--bind 127.0.0.1:18080` |
| `WYRD_CLIENT_401_NO_CREDENTIALS` | Unset `WYRD_CONFIG_HOME`, `WYRD_API_KEY`, and `WYRD_SERVER_URL`, or match the server terminal |
| `WYRD_CLI_503_SERVER_DEV` naming `guided setup` | Install the agent so its command is on `PATH`, or name an installed one with `--host` |

## Next

[Run your first verification](/get-started/quickstart/) verifies a finance report against a policy PDF and releases it only when it passes.
