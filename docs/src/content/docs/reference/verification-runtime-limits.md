---
title: Verification runtime limits
description: Server environment settings that bound Verifier files, extraction, workspaces, Python code, and real-time deadlines, with their defaults and startup rules.
pillar: wyrd
section: Reference
order: 5
draft: true
status: partial
---

<Aside type="caution" title="Pending confirmation">These are the approved limits. Their parity with the server's startup configuration is confirmed when the verification runtime ships.</Aside>

These limits protect a server that runs Verifiers for many tenants. They are environment variables on `wyrd-server`, read once at startup.

## Settings

| Environment variable | Default | Bounds |
| --- | ---: | --- |
| `WYRD_VERIFICATION_TIMEOUT_MS` | 60000 | The whole `verify` call |
| `WYRD_VERIFICATION_FILE_MAX_MIB` | 20 | One input file |
| `WYRD_VERIFICATION_FILES_MAX_MIB` | 100 | All input files of one execution together |
| `WYRD_VERIFICATION_ARCHIVE_MAX_MIB` | 200 | Decompressed bytes of one archive |
| `WYRD_VERIFICATION_ARCHIVE_MAX_ENTRIES` | 10000 | Entries in one archive |
| `WYRD_VERIFICATION_ARCHIVE_MAX_RATIO` | 100 | Decompressed-to-compressed size ratio of one archive |
| `WYRD_VERIFICATION_EXTRACT_MAX_PAGES` | 500 | PDF pages or PPTX slides in one extraction |
| `WYRD_VERIFICATION_EXTRACT_TEXT_MAX_MIB` | 10 | Text produced by one extraction |
| `WYRD_VERIFICATION_SCRATCH_MAX_MIB` | 250 | Scratch bytes of one Agent invocation's workspace |
| `WYRD_VERIFICATION_WORK_WRITE_MAX_MIB` | 50 | Total bytes written to `/work` in one invocation |
| `WYRD_VERIFICATION_TOOL_RESULT_MAX_KIB` | 64 | One tool result, cut off while it is collected |
| `WYRD_VERIFICATION_MAX_WORKSPACES` | 16 | Workspaces open at once in one server process |
| `WYRD_VERIFICATION_MONTY_MIN_WORKERS` | 2 | Python workers kept warm |
| `WYRD_VERIFICATION_MONTY_MAX_WORKERS` | 8 | Python workers alive at once |
| `WYRD_VERIFICATION_MONTY_MAX_WAITERS` | 16 | Python operations waiting for a worker |
| `WYRD_VERIFICATION_MONTY_MEMORY_MIB` | 256 | Memory one Python operation may allocate |
| `WYRD_VERIFICATION_MONTY_PROCESS_MEMORY_MIB` | 384 | One worker process, including interpreter overhead |
| `WYRD_VERIFICATION_MONTY_POOL_MEMORY_MIB` | 2048 | All worker processes together, idle ones included |
| `WYRD_VERIFICATION_CODE_TIMEOUT_MS` | 10000 | One Python operation, from waiting for a worker through compiling, running, and its file reads |
| `WYRD_VERIFICATION_AGENT_CODE_BUDGET_MS` | 60000 | All Python operations of one Agent invocation together |

## Startup rules

- Values are positive integers. `_MIB` and `_KIB` are binary units (1 MiB = 1,048,576 bytes); `_MS` is milliseconds.
- An unset variable uses its default. Changing a value takes a restart.
- The server refuses to start, naming the variable, when a value is not a positive integer or overflows, or when settings contradict each other:
  - `MONTY_MIN_WORKERS` above `MONTY_MAX_WORKERS`;
  - `MONTY_MEMORY_MIB` at or above `MONTY_PROCESS_MEMORY_MIB`;
  - `MONTY_POOL_MEMORY_MIB` below `MONTY_PROCESS_MEMORY_MIB`;
  - `FILE_MAX_MIB` above `FILES_MAX_MIB`;
  - `WORK_WRITE_MAX_MIB` above `SCRATCH_MAX_MIB`.
- No setting disables a limit or a security protection.

A Verifier can lower a limit for one slot, such as `max_bytes` on a `media` slot, never raise it. Step, Agent, and request deadlines apply too; the earliest one wins.

## Python workers

`code` actions and checks, and an Agent's `code.run` calls, run in a pool of sandboxed Python worker processes. There is one pool per server process, shared by every tenant; no interpreter state is shared. Agent checks, model calls, and other tools never hold a worker.

- Each Python operation gets a fresh interpreter session. Files in the invocation's `/work` persist across its calls; nothing persists across invocations.
- An operation waits for a worker only within its own remaining deadline. When `MONTY_MAX_WAITERS` operations are already waiting, a new one is refused at once.
- Queued executions stay in the queue until capacity frees, instead of waiting in memory.
- Waiting and running time count against `CODE_TIMEOUT_MS` and `AGENT_CODE_BUDGET_MS`, and neither resets across tool calls or step retries.
- A session that breaches a limit is discarded. A crashed or hung worker is killed and replaced; the operation fails, and the server keeps running.

### Memory

`MONTY_POOL_MEMORY_MIB` caps all workers together, idle ones included. At the defaults, eight workers at 384 MiB each exceed 2048 MiB, so eight workers cannot all use their full allocation at once; under pressure a worker is terminated and its operation fails, never passes. Memory used by the server itself, by extraction, by model responses, and by file and tool buffers is outside this ceiling: give the server headroom beyond it.

### Isolation

Always on, whatever the settings:

- Workers run as a separate unprivileged identity with an empty credential environment.
- The operating system denies them network access.
- `/inputs` is read-only and `/work` is bounded; the server performs every file access on the worker's behalf, limited to the invocation's own files.
- No subprocesses, host Python packages, host files, or callbacks into the server.

## Errors

| Limit reached | Result |
| --- | --- |
| A file over its slot or `FILE_MAX_MIB` | `WYRD_VERIFICATION_413_INPUT_TOO_LARGE` before anything runs |
| `TIMEOUT_MS` in `verify` | `WYRD_VERIFICATION_504_EXECUTION_TIMED_OUT` |
| Any other limit inside a step or check | The step fails or the check is `inconclusive`, with the limit in its `reason` |
