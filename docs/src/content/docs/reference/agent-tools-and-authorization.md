---
title: Agent tools
description: Built-in tools a Verifier's Agent can call and the permissions each call needs.
pillar: wyrd
section: Reference
order: 3
status: runnable
---

A Verifier's Agent calls server built-in tools, named in its `tool_names`. There is no fetch tool and no network access.

## Built-in tools

| Tool | Does | Resource permission |
| --- | --- | --- |
| `cards.list` | Lists Card summaries by kind, name, space, or metadata | `cards:read` |
| `cards.get` | Reads one Card by exact kind, name, version, and space, or by kind and UID | `cards:read` |
| `bifrost.list_tables` | Lists tables | `bifrost_table:read` |
| `bifrost.describe_table` | Reads one table's schema | `bifrost_table:read` |
| `bifrost.query` | Runs one read-only SQL query | `bifrost_query:read` on every table it reads |
| `fs.list` | Lists workspace files | Workspace only |
| `fs.read` | Reads a bounded span of a workspace file, with locators | Workspace only |
| `fs.search` | Finds matching locations in workspace files | Workspace only |
| `code.run` | Runs sandboxed Python against the workspace | Workspace only |

Results are capped by `WYRD_VERIFICATION_TOOL_RESULT_MAX_KIB` ([limits](/reference/verification-runtime-limits/)). Each call is recorded in the step's `tool_calls` as `tool`, `arguments`, `output_sha256` (absent when nothing was returned), `returned_bytes`, and `error` (present when the call failed).

## Rules

- A step Agent with `fs.*` or `code.run` needs a `workspace`, and a `workspace` needs one of them.
- A check Agent may declare only `bifrost.query` and has no workspace.

## Permissions

Every call needs both a `tool:invoke` grant and the tool's resource permission. Neither implies the other.

```json
{"resource":"tool","action":"invoke","scope":"all"}
{"resource":"tool","action":"invoke","scope":{"tool":"bifrost.query"}}
```

The scope is `all` or one exact tool name, never a pattern. A call runs as the principal that started the verification, or the binding owner for a scheduled one; delegation can narrow that authority but never widen it. `verifier:run` and declaring a tool grant nothing.

## Errors

| Code | Cause |
| --- | --- |
| `WYRD_PERMISSION_403_DENIED_RBAC` | The principal lacks `tool:invoke` or the resource permission; the call returns no data |
| `WYRD_SPEC_400_VALIDATION`, rule `agent_tools` | Unknown tool name, `fs.*` without a workspace, or a workspace without `fs.*` or `code.run` |
| `WYRD_SPEC_400_VALIDATION`, rule `check_agent_tools` | A check Agent declares a tool other than `bifrost.query` |
| `WYRD_SPEC_400_VALIDATION`, rule `check_agent_workspace` | A check Agent has a workspace |
| `WYRD_SPEC_400_VALIDATION`, rule `workspace_path` | A `workspace` key is not `/inputs/<name>[/<name>...]`, or contains `.` or `..` |
