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
| `code.run` | Runs sandboxed Python against the workspace, which can call the Agent's other declared tools except `fs.write` and `publish` | Workspace only; each call from Python needs that tool's grants |
| `publish` | Retains one `/work` file or directory, or one JSON value, under a logical `name` for the steps that read this step's publications; returns `{name, kind}` | Step Agents only |

Arguments are capped by `WYRD_VERIFICATION_TOOL_ARGUMENT_MAX_KIB`, results by `WYRD_VERIFICATION_TOOL_RESULT_MAX_KIB`, and one Agent step or check, across its retries, makes at most `WYRD_VERIFICATION_AGENT_TOOL_MAX_CALLS` calls, direct and from Python together ([limits](/reference/verification-runtime-limits/)).

Each call is recorded in `tool_calls` in the execution's details (`Execution.details`, or `verification.get_execution_details` over MCP): on the step for a step Agent, on the requirement for a check Agent. A record has `tool`, `arguments`, `recipient`, `output_sha256` (absent when nothing was returned), `returned_bytes`, and `error` (present when the call failed). `recipient` is `model` for a call the model made and `code` for a call a `code.run` program made; a record written before `recipient` existed reads as `model`. A requirement written before check records existed has no `tool_calls`.

## Tools from Python

A `code.run` program calls a declared tool with `await call_tool(name, arguments)` and gets the tool's result as Python values. It can call any tool in the Agent's `tool_names` except `code.run`, `fs.write`, and `publish`, which only the model calls. Each call runs exactly as if the model had made it, with the same grants, limits, and audit, and is recorded with recipient `code`.

The model sees the `code.run` result: `output`, what the program printed, and `value`, the repr of its last expression, each cut at `WYRD_VERIFICATION_TOOL_RESULT_MAX_KIB`, with `value_truncated` set when `value` was cut. Any other intermediate result stays in Python, so a program can read many files or rows and return a short answer in one model turn. A program that prints a tool result, or ends with an expression holding one, sends it to the model. Citations still point to the original pinned evidence, such as an `/inputs` file and its lines, never to the program's output.

<CodeFromFile file="fixtures/cards/verifier_building_blocks/programs/nested-investigation.py" lang="python" title="code.run source" />

A failed call raises `RuntimeError`. Its message is the call's problem JSON, with `code`, `status`, `title`, `detail`, `remediation`, and `details`; branch on `code` and carry on, as above. A denied call returns no data, so the program cannot read the protected value. Two failures end the step or check instead and cannot be caught: spending the call limit, and a server stop such as a capacity refusal.

Fewer model turns are not guaranteed to cost fewer tokens or less time: the program's source and its `code.run` result still pass through the model, and the Python worker stays busy while the program waits on tools.

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

Publications belong to queued executions. Registration marks a Verifier `queue_only` when one of its step or check Agents, inline or a referenced registered Agent, declares `publish`; the server sets this flag and ignores any value you send. A `verify` call for such a Verifier is refused with `WYRD_VERIFICATION_422_INPUT_UNSUPPORTED` before any step runs; submit it instead. A `verify` execution has no invocation or publication history, so those endpoints answer it as not found.

## Failed tool calls

A failed call does not end the step. The model, or the program for a call from Python, receives `code`, `error`, and, for a call it can repair, `details`, then may call again; the call's `tool_calls` record keeps the `error`. An `fs.*` or `publish` call that breaks one of the rules below returns `WYRD_SPEC_400_VALIDATION` with `details` naming the `rule`, the argument `field`, its `value`, and what was `expected`. A local IO failure returns `SKALD_TOOL_500_CALL` without `details`.

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
| `WYRD_AGENT_404_TOOL_NOT_IN_AGENT` | A program called a tool not in the Agent's `tool_names`, or `code.run`, `fs.write`, or `publish` |
| `WYRD_TOOL_422_INPUT` | A program passed arguments that are not a JSON object, or a call's arguments exceed `WYRD_VERIFICATION_TOOL_ARGUMENT_MAX_KIB` |
| `WYRD_VERIFICATION_422_TOOL_BUDGET_EXCEEDED` | The step or check spent `WYRD_VERIFICATION_AGENT_TOOL_MAX_CALLS`; the step fails or the check is `inconclusive`, with no further model turn |
| `WYRD_SPEC_400_VALIDATION`, rule `agent_tools` | Unknown or repeated tool name, a reading `fs.*` tool without a workspace or scratch writer, a workspace without a tool that uses it, or `publish` on a check Agent |
| `WYRD_SPEC_400_VALIDATION`, rule `workspace_path` | A `workspace` key is not `/inputs/<name>[/<name>...]`, contains `.` or `..`, or overlaps another mount |
| `WYRD_SPEC_400_VALIDATION`, rule `workspace_artifact` | A workspace `artifact` selection does not reference an Artifact Card |
