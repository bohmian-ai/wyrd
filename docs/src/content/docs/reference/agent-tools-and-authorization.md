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
| `fs.write` | Creates or replaces one UTF-8 file under `/work`, creating its parent directories, and returns `path` and `bytes` | Workspace only |
| `code.run` | Runs sandboxed Python against the workspace | Workspace only |
| `publish` | Retains one `/work` file or directory, or one JSON value, under a logical `name` for the steps that read this step's publications; returns `{name, kind}` | Step Agents only |

Results are capped by `WYRD_VERIFICATION_TOOL_RESULT_MAX_KIB` ([limits](/reference/verification-runtime-limits/)). Each call is recorded in the step's `tool_calls` as `tool`, `arguments`, `output_sha256` (absent when nothing was returned), `returned_bytes`, and `error` (present when the call failed).

## Rules

- `fs.write`, `code.run`, and `publish` get private `/work` scratch without a `workspace`. `fs.list`, `fs.read`, `fs.search`, and `fs.extract` need a `workspace` or one of `fs.write` and `code.run` beside them. A `workspace` needs a tool that uses it.
- `/work` belongs to one invocation attempt. A later step, a retry, or a sibling never sees it; only what `publish` accepts reaches other steps.
- Each tool is named once. The provider-facing spelling, such as `fs_write`, is not a tool name.
- Step and check Agents follow the same rules, except that only a step Agent may declare `publish`. Each call still needs its grants.
- Nothing is parsed unless the Agent calls `fs.extract`; `fs.read` and `fs.search` serve raw text and refuse or skip binary files.

## Permissions

Every call needs both a `tool:invoke` grant and the tool's resource permission. Neither implies the other.

```json
{"resource":"tool","action":"invoke","scope":"all"}
{"resource":"tool","action":"invoke","scope":{"tool":"bifrost.query"}}
```

The scope is `all` or one exact tool name, never a pattern. A call runs as the principal that started the verification, or the binding owner for a scheduled one; delegation can narrow that authority but never widen it. `verifier:run` and declaring a tool grant nothing.

## Publishing

`publish` takes a `name` matching `^[A-Za-z0-9_-]{1,64}$` and exactly one source: `path`, an existing `/work` file or whole directory, or `value`, any JSON value including `null`. A path is snapshotted when the call runs, so later writes do not change the publication; directories keep empty subdirectories and their actual filenames. The receipt is `{"name": ..., "kind": "file" | "directory" | "json"}` and is returned only after the execution accepts the publication.

A name is unique within one step attempt: publishing it again in that attempt is refused, while a fresh retry of the step may reuse it and is attributed to its own attempt. The server records the producing step, item, iteration, and attempt; no argument can set them. A replayed provider tool call returns its original receipt. A final answer is not a publication.

A later step mounts `steps.<id>.publications.<name>` or the whole `steps.<id>.publications` collection, whose `manifest.json` lists each entry's actual path. A step whose named publication is missing fails before its model call. Publications read back through the execution's publication endpoints with their manifests and download plans.

Publications belong to queued executions. A `verify` call whose Verifier declares a step Agent with `publish`, or mounts `steps.<id>.publications`, is refused with `WYRD_VERIFICATION_422_INPUT_UNSUPPORTED` before any step runs; submit it instead. A `verify` execution has no invocation or publication history, so those endpoints answer it as not found.

## Failed tool calls

A failed call does not end the step. The model receives `code`, `error`, and, for a call it can repair, `details`, then may call again; the step's `tool_calls` record keeps the `error`. An `fs.*` or `publish` call that breaks one of the rules below returns `WYRD_SPEC_400_VALIDATION` with `details` naming the `rule`, the argument `field`, its `value`, and what was `expected`. A local IO failure returns `SKALD_TOOL_500_CALL` without `details`.

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
| `work_budget` | The file, the written content, or the extraction exceeds the `/work` write budget |
| `write_path` | `fs.write` names `/inputs`, `/work` itself, or a path outside `/work` or with an empty, `.`, or `..` segment |
| `write_parent` | An `fs.write` parent is a file or a link |
| `publication_path` | `publish` names a `path` outside `/work` |
| `publication_source` | `publish` has both or neither of `path` and `value` |
| `publication_name` | The `name` was already published in this step attempt |
| `publication_tree` | A published directory holds an entry the tree rules refuse |
| `publication_size` | A published `value` or file exceeds its limit |
| `publication_count`, `publication_bytes` | The execution's publications would exceed their count or retained-byte limit |

## Errors

| Code | Cause |
| --- | --- |
| `WYRD_PERMISSION_403_DENIED_RBAC` | The principal lacks `tool:invoke` or the resource permission; the call returns no data |
| `WYRD_SPEC_400_VALIDATION`, rule `agent_tools` | Unknown or repeated tool name, a reading `fs.*` tool without a workspace or scratch writer, a workspace without a tool that uses it, or `publish` on a check Agent |
| `WYRD_SPEC_400_VALIDATION`, rule `workspace_path` | A `workspace` key is not `/inputs/<name>[/<name>...]`, contains `.` or `..`, or overlaps another mount |
| `WYRD_SPEC_400_VALIDATION`, rule `workspace_artifact` | A workspace `artifact` selection does not reference an Artifact Card |
