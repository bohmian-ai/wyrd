---
title: Agent tools and authorization
description: The built-in tools a Verifier's Agent can call, how they are selected, and the permissions every call needs.
pillar: wyrd
section: Reference
order: 6
draft: true
status: runnable
---

An Agent that prepares evidence in a Verifier step can call tools that the server provides. You choose them by name; you never register or upload a callable.

## Built-in tools

| Tool | Does | Also needs |
| --- | --- | --- |
| `cards.list` | Lists Card summaries by kind, name, space, or metadata | `cards:read` |
| `cards.get` | Reads one Card by kind and UID | `cards:read` on it |
| `cards.get_by_ref` | Reads one Card by exact kind, name, version, and space | `cards:read` on it |
| `bifrost.list_tables` | Lists tables | `bifrost_table:read` |
| `bifrost.describe_table` | Reads one table's schema | `bifrost_table:read` on it |
| `bifrost.query` | Runs one read-only SQL query | `bifrost_query:read` on every table or schema it reads |
| `fs.list` | Lists files in the workspace | A workspace |
| `fs.read` | Reads a bounded span of a workspace file as text, with page, slide, or line locators | A workspace |
| `fs.search` | Finds matching locations in workspace files | A workspace |
| `code.run` | Runs Python in a sandboxed worker against the workspace | A workspace |

There is no fetch tool and no network access. Results are capped by `WYRD_VERIFICATION_TOOL_RESULT_MAX_KIB`; see [Verification runtime limits](/reference/verification-runtime-limits/).

## Selecting tools

List them in the Agent's `tool_names`:

```yaml
kind: Agent
spec:
  tool_names: [fs.read, fs.search, bifrost.query]
```

Before the Agent's first model call, the server resolves each name and attaches only those tools. An unknown name, or a filesystem tool without a workspace, fails the step; a tool is never silently dropped or replaced.

- A step's Agent with `fs.*` or `code.run` needs a `workspace`, and a `workspace` needs at least one of them.
- An Agent in a check position is tool-less: no `tool_names` and no workspace. Give it what it needs through `inputs`.
- Every call's tool name, arguments, output digest, and returned byte count is recorded in the step's `tool_calls`.

## Permissions

Every tool call passes two independent checks, and both must allow it.

**1. Invoke the tool.** A grant on the `tool` resource, either for every built-in:

```json
{"resource":"tool","action":"invoke","scope":"all"}
```

or for one by its exact name:

```json
{"resource":"tool","action":"invoke","scope":{"tool":"bifrost.query"}}
```

The scope is an exact built-in name, not a pattern or a path; each name covers only itself. The grant follows the same rules as every other Wyrd permission: global or exact scope, existing wildcard grants, intersection, and delegation attenuation.

**2. Use the resource.** The tool then runs the same permission check the operation has everywhere else. `bifrost.query` needs `bifrost_query:read` on every table it reads, including each table of a join; the `cards.*` tools need `cards:read`. The `fs.*` tools and `code.run` reach only the invocation's own workspace.

Neither check implies the other:

| Holds | Can call `bifrost.query` on `vala.datasets.tickets` |
| --- | --- |
| `tool:invoke` on `all` only | No: no read on the table |
| `bifrost_query:read` on `vala.datasets` only | No: no invoke on the tool |
| Both | Yes |

No tool permission comes from `verifier:run`, from the Agent declaring the tool, or from a data grant. A call without the invoke grant is refused before the tool runs. A call refused on its resource returns no partial data.

## Whose authority

A tool call runs as the principal that started the verification, never as the Agent or the server:

- A `verify` or `submit` call runs as its caller's subject and scopes, or as permissions delegated from that subject. Delegation can narrow that authority but never widen it.
- That authority travels with a queued execution through retries and server restarts, without credentials.
- A scheduled execution runs as the binding owner.
- The server may be the recorded executor, but that grants nothing extra.
- Model output and tool arguments cannot change who the call runs as. Credentials never reach prompts, workspace files, or Python workers.

Every allow and deny decision is written to the audit log without delaying the call.

## Errors

| Code | Cause |
| --- | --- |
| `WYRD_PERMISSION_403_DENIED_RBAC` | The verification's principal lacks `tool:invoke` for the tool, or the resource permission the tool needs |
| Registration refusal | An unknown tool name, a filesystem tool without a workspace, a workspace without a filesystem tool, or tools on a check-position Agent |
