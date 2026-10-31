---
title: Agent tools
description: Built-in tools a Verifier's Agent can call and the permissions each call needs.
pillar: wyrd
section: Reference
order: 3
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
| `fs.read` | Reads a UTF-8 workspace file's actual lines, with `lines` locators and a resumable `next` | Workspace only |
| `fs.search` | Finds matching lines: literal and case-insensitive by default, or `regex`, `case_sensitive`, a `path` file or directory scope, and a filename `glob` | Workspace only |
| `fs.extract` | Extracts one supported `/inputs` document to a new `/work` `output_path` as extraction JSON | Workspace only |
| `code.run` | Runs sandboxed Python against the workspace | Workspace only |

Results are capped by `WYRD_VERIFICATION_TOOL_RESULT_MAX_KIB` ([limits](/reference/verification-runtime-limits/)). Each call is recorded in the step's `tool_calls` as `tool`, `arguments`, `output_sha256` (absent when nothing was returned), `returned_bytes`, and `error` (present when the call failed).

## Rules

- A step Agent with `fs.*` or `code.run` needs a `workspace`, and a `workspace` needs one of them.
- Step and check Agents follow the same rules: a check Agent may declare any tool and a `workspace`, and each call still needs its grants.
- Nothing is parsed unless the Agent calls `fs.extract`; `fs.read` and `fs.search` serve raw text and refuse or skip binary files.

## Permissions

Every call needs both a `tool:invoke` grant and the tool's resource permission. Neither implies the other.

```json
{"resource":"tool","action":"invoke","scope":"all"}
{"resource":"tool","action":"invoke","scope":{"tool":"bifrost.query"}}
```

The scope is `all` or one exact tool name, never a pattern. A call runs as the principal that started the verification, or the binding owner for a scheduled one; delegation can narrow that authority but never widen it. `verifier:run` and declaring a tool grant nothing.

## Failed tool calls

A failed call does not end the step. The model receives `code`, `error`, and, for a call it can repair, `details`, then may call again; the step's `tool_calls` record keeps the `error`. An `fs.*` call that breaks one of the rules below returns `WYRD_SPEC_400_VALIDATION` with `details` naming the `rule`, the argument `field`, its `value`, and what was `expected`. A local IO failure returns `SKALD_TOOL_500_CALL` without `details`.

| Rule | Cause |
| --- | --- |
| `workspace_path` | A path is not under `/inputs` or `/work`, or has an empty, `.`, or `..` segment |
| `workspace_missing` | No file or directory exists at the path |
| `regular_file` | A file argument names a directory or a non-regular file |
| `utf8_text` | `fs.read` names a file that is not UTF-8 text |
| `result_limit` | The `fs.read` path is too long for any line to fit within the tool result limit |
| `search_query` | `fs.search` has an empty `query` |
| `search_pattern` | A `regex` query is not valid Rust regex syntax |
| `search_glob` | `glob` is not a relative glob without `..` segments |
| `extractable_document` | `fs.extract` names a file that is not an extractable document |
| `extraction_refused` | `fs.extract` names a malformed document or one over the extraction limits |
| `output_path` | `output_path` is not a new file path under `/work` |
| `output_exists` | `output_path` already exists |
| `work_budget` | The file or the extraction exceeds the `/work` write budget |

## Errors

| Code | Cause |
| --- | --- |
| `WYRD_PERMISSION_403_DENIED_RBAC` | The principal lacks `tool:invoke` or the resource permission; the call returns no data |
| `WYRD_SPEC_400_VALIDATION`, rule `agent_tools` | Unknown tool name, `fs.*` without a workspace, or a workspace without `fs.*` or `code.run` |
| `WYRD_SPEC_400_VALIDATION`, rule `workspace_path` | A `workspace` key is not `/inputs/<name>[/<name>...]`, contains `.` or `..`, or overlaps another mount |
| `WYRD_SPEC_400_VALIDATION`, rule `workspace_artifact` | A workspace `artifact` selection does not reference an Artifact Card |
