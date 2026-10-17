---
title: Verification runtime limits
description: Server environment settings that bound Verifier files, extraction, workspaces, Python code, and real-time deadlines, with their defaults and startup rules.
pillar: wyrd
section: Reference
order: 5
status: runnable
---

These limits protect a server that runs Verifiers for many tenants. They are environment variables on `wyrd-server`, read once at startup. They have no TOML equivalent. [Configure the verification runtime](/operate/configure-verification-runtime/) covers installing the worker and sizing a deployment.

## Settings

| Environment variable | Default | Bounds |
| --- | ---: | --- |
| `WYRD_VERIFICATION_ENABLED` | `1` | Whether an API-serving process runs Verifiers. `1` or `0`; also `[verification] enabled` in TOML |
| `WYRD_VERIFICATION_MONTY_WORKER` | `wyrd-monty-worker` beside the running `wyrd-server` | Path of the Python worker binary |
| `WYRD_VERIFICATION_TIMEOUT_MS` | 60000 | The whole `verify` call |
| `WYRD_VERIFICATION_FILE_MAX_MIB` | 20 | One input file |
| `WYRD_VERIFICATION_FILES_MAX_MIB` | 100 | All input files of one request or one workspace together |
| `WYRD_VERIFICATION_ARCHIVE_MAX_MIB` | 200 | Decompressed bytes of one archive, including DOCX, XLSX, and PPTX containers |
| `WYRD_VERIFICATION_ARCHIVE_MAX_ENTRIES` | 10000 | Entries in one archive |
| `WYRD_VERIFICATION_ARCHIVE_MAX_RATIO` | 100 | Decompressed-to-compressed size ratio of one archive |
| `WYRD_VERIFICATION_EXTRACT_MAX_PAGES` | 500 | PDF pages or PPTX slides in one extraction |
| `WYRD_VERIFICATION_EXTRACT_TEXT_MAX_MIB` | 10 | Text produced by one extraction |
| `WYRD_VERIFICATION_SCRATCH_MAX_MIB` | 250 | All bytes in one workspace: materialized `/inputs` plus `/work` |
| `WYRD_VERIFICATION_WORK_WRITE_MAX_MIB` | 50 | Bytes held under `/work` in one workspace |
| `WYRD_VERIFICATION_TOOL_RESULT_MAX_KIB` | 64 | One tool result or one Python operation's printed output, cut off while it is collected |
| `WYRD_VERIFICATION_MAX_WORKSPACES` | 16 | Workspaces open at once in one server process |
| `WYRD_VERIFICATION_MONTY_MIN_WORKERS` | 2 | Python workers started with the server |
| `WYRD_VERIFICATION_MONTY_MAX_WORKERS` | 8 | Python workers alive at once, before the memory cap below |
| `WYRD_VERIFICATION_MONTY_MAX_WAITERS` | 16 | Python operations waiting for a worker |
| `WYRD_VERIFICATION_MONTY_MEMORY_MIB` | 256 | Memory one Python operation may allocate |
| `WYRD_VERIFICATION_MONTY_PROCESS_MEMORY_MIB` | 384 | What one worker process is counted at against the pool ceiling |
| `WYRD_VERIFICATION_MONTY_POOL_MEMORY_MIB` | 2048 | All worker processes together, idle ones included |
| `WYRD_VERIFICATION_CODE_TIMEOUT_MS` | 10000 | One Python operation, from waiting for a worker through compiling, running, and its file calls |
| `WYRD_VERIFICATION_AGENT_CODE_BUDGET_MS` | 60000 | All `code.run` calls of one Agent step together, across its retries |

## Startup rules

- Limits are positive integers. `_MIB` and `_KIB` are binary units (1 MiB = 1,048,576 bytes); `_MS` is milliseconds.
- An unset variable uses its default. Changing any value takes a restart.
- The server refuses to start, naming the variable, when a value is empty, is not a positive integer, or overflows once scaled to bytes. `WYRD_VERIFICATION_ENABLED` accepts only `1` or `0`.
- It also refuses to start when settings contradict each other:
  - `MONTY_MIN_WORKERS` above `MONTY_MAX_WORKERS`;
  - `MONTY_MEMORY_MIB` at or above `MONTY_PROCESS_MEMORY_MIB`;
  - `MONTY_POOL_MEMORY_MIB` below `MONTY_PROCESS_MEMORY_MIB`;
  - `MONTY_PROCESS_MEMORY_MIB` too small for `MONTY_MEMORY_MIB` plus Monty's 4 MiB allocator headroom;
  - `FILE_MAX_MIB` above `FILES_MAX_MIB`;
  - `WORK_WRITE_MAX_MIB` above `SCRATCH_MAX_MIB`.
- When the runtime is enabled, the server refuses to start with `hosted Verifier blocks could not start` if the worker binary is missing or the first `MONTY_MIN_WORKERS` workers cannot start.
- No setting disables a limit or an isolation protection.

A Verifier can lower a limit for one slot, such as `max_bytes` on a `media` slot, never raise it. Step, Agent, and request deadlines apply too; the earliest one wins.

## Which processes run Verifiers

A process hosts the runtime when `WYRD_VERIFICATION_ENABLED` is `1` and its role serves the API. Every other process starts no worker, and its verification routes answer `WYRD_SERVER_503_SERVICE_UNAVAILABLE`. Each hosting process has its own limits, workspaces, and worker pool.

## Python workers

`code` actions and checks, and an Agent's `code.run` calls, run in a pool of `wyrd-monty-worker` processes. There is one pool per server process, shared by every tenant; no interpreter state is shared. Agent checks, model calls, and other tools never hold a worker.

- Each Python operation gets a fresh worker, which is killed when the operation ends. Files in `/work` persist across the `code.run` calls of one step attempt; nothing persists across attempts, steps, or executions.
- At most `MONTY_MAX_WORKERS` operations run, after the memory cap below, and `MONTY_MAX_WAITERS` more wait. An operation beyond that is refused at once; nothing queues in memory.
- An operation waits for a worker only within its own remaining deadline, at most `CODE_TIMEOUT_MS`.
- A refused operation, one that found no worker in time, or a step that found every workspace (`MAX_WORKSPACES`) taken means the process is busy, not that the code failed. Nothing is judged and no `retries` are spent: the whole execution stops. A queued execution stays queued under the same ID, its attempt is not counted, and it runs again when capacity frees. A direct `verify` call answers `WYRD_SERVER_503_SERVICE_UNAVAILABLE`; retry it later or on another process. An Agent's `code.run` refusal ends the Agent step the same way, cancelling its other pending tool calls.
- `AGENT_CODE_BUDGET_MS` is reserved before each `code.run` call waits, and does not reset across tool calls or step retries. Once it is spent, `code.run` returns an error to the Agent.
- A Python exception, memory exhaustion, a timeout while running, or a crashed worker fails the operation without a retry. The server keeps running.

### Memory

Memory is bounded the same way on every platform, without OS limits:

- Each worker's allocator stops its heap at `MONTY_MEMORY_MIB` plus Monty's 4 MiB headroom above the worker's own baseline. Code that allocates past it raises `MemoryError` and the operation fails, never passes.
- The server counts every live worker, idle or busy, at `MONTY_PROCESS_MEMORY_MIB`, and keeps at most `MONTY_POOL_MEMORY_MIB / MONTY_PROCESS_MEMORY_MIB` alive, rounded down. At the defaults that is 2048 / 384, so 5 workers live at once even though `MONTY_MAX_WORKERS` is 8. `MONTY_MIN_WORKERS` is lowered to the same cap.

Memory used by the server itself, by extraction, by model responses, and by file and tool buffers is outside the pool ceiling: give the server headroom beyond it.

### Isolation

Always on, whatever the settings, and identical on macOS and Linux:

- Workers start with an empty environment, so no credential reaches them.
- Sandboxed Python has no host files, network, subprocesses, environment variables, host packages, or callbacks into the server. Every operating-system call is a request the server answers or refuses.
- The server answers file calls only from the step's own workspace: `/inputs` is read-only, and `/work` writes stop at the space left under `WORK_WRITE_MAX_MIB` and `SCRATCH_MAX_MIB`.
- A worker serves one operation and exits.

## Errors

| Limit reached | Result |
| --- | --- |
| A file over its slot or `FILE_MAX_MIB`, or a request over `FILES_MAX_MIB` | `WYRD_VERIFICATION_413_INPUT_TOO_LARGE` before anything runs |
| The server's verification staging is full (`scribe.outbox_capacity`) | `WYRD_VERIFICATION_429_STAGING_FULL` with `Retry-After`, before anything is staged |
| `TIMEOUT_MS` in `verify` | `WYRD_VERIFICATION_504_EXECUTION_TIMED_OUT` |
| No Python worker or workspace free | Queued: the execution stays queued and its attempt is refunded. Direct: `WYRD_SERVER_503_SERVICE_UNAVAILABLE` |
| Any other limit inside a step or check, including a spent `AGENT_CODE_BUDGET_MS` | The step fails or the check is `inconclusive`, with the limit in its `reason` |
