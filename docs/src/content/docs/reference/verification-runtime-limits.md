---
title: Runtime limits
description: Environment settings that bound Verifier files, extraction, workspaces, Python code, model calls, and direct and queued capacity, with defaults and startup rules.
pillar: wyrd
section: Reference
order: 4
---

Environment variables on `wyrd-server`, read once at startup; a change takes a restart. Apart from `WYRD_VERIFICATION_ENABLED`, none has a TOML equivalent. Each process applies them on its own.

## Settings

| Variable | Default | Bounds |
| --- | ---: | --- |
| `WYRD_VERIFICATION_ENABLED` | `1` | Whether an API-serving process runs Verifiers: `1` or `0`. Also `[verification] enabled` in TOML |
| `WYRD_VERIFICATION_TIMEOUT_MS` | 60000 | The whole `verify` call |
| `WYRD_VERIFICATION_EXECUTION_TIMEOUT_MS` | 300000 | One queued execution, from its first claim; waiting and retries do not reset it |
| `WYRD_VERIFICATION_DIRECT_EXECUTIONS` | 16 | `verify` calls running at once in one server process |
| `WYRD_VERIFICATION_QUEUED_EXECUTIONS` | 64 | Queued executions running at once in one server process; more wait in the queue |
| `WYRD_VERIFICATION_FILE_MAX_MIB` | 20 | One input file, and one file a publication retains |
| `WYRD_VERIFICATION_FILES_MAX_MIB` | 100 | All input files of one request or one workspace together, and all bytes one execution's publications retain |
| `WYRD_VERIFICATION_PUBLICATIONS_MAX` | 1000 | Publications one execution accepts across all its attempts |
| `WYRD_VERIFICATION_ARCHIVE_MAX_MIB` | 200 | Decompressed bytes of one archive, including DOCX, XLSX, and PPTX containers |
| `WYRD_VERIFICATION_ARCHIVE_MAX_ENTRIES` | 10000 | Entries in one archive |
| `WYRD_VERIFICATION_ARCHIVE_MAX_RATIO` | 100 | Decompressed-to-compressed size ratio of one archive |
| `WYRD_VERIFICATION_EXTRACT_MAX_PAGES` | 500 | PDF pages or PPTX slides in one extraction |
| `WYRD_VERIFICATION_EXTRACT_TEXT_MAX_MIB` | 10 | Text produced by one extraction |
| `WYRD_VERIFICATION_SCRATCH_MAX_MIB` | 250 | All bytes in one workspace: materialized `/inputs` plus `/work` |
| `WYRD_VERIFICATION_WORK_WRITE_MAX_MIB` | 50 | Bytes held under `/work` in one workspace |
| `WYRD_VERIFICATION_TOOL_RESULT_MAX_KIB` | 64 | One tool result or one Python operation's printed output, cut off while it is collected |
| `WYRD_VERIFICATION_DIRECT_WORKSPACES` | 4 | Workspaces `verify` calls hold open at once in one server process |
| `WYRD_VERIFICATION_QUEUED_WORKSPACES` | 12 | Workspaces queued executions hold open at once in one server process |
| `WYRD_VERIFICATION_DIRECT_PYTHON_WORKERS` | 2 | Python workers alive at once for `verify` calls |
| `WYRD_VERIFICATION_QUEUED_PYTHON_WORKERS` | 3 | Python workers alive at once for queued executions |
| `WYRD_VERIFICATION_DIRECT_PYTHON_WAITERS` | 4 | `verify` Python operations waiting for a worker |
| `WYRD_VERIFICATION_QUEUED_PYTHON_WAITERS` | 12 | Queued Python operations waiting for a worker |
| `WYRD_VERIFICATION_DIRECT_MODEL_CALLS` | 16 | Gateway model calls `verify` calls make at once; another waits for one to finish |
| `WYRD_VERIFICATION_QUEUED_MODEL_CALLS` | 48 | Gateway model calls queued executions make at once; another waits for one to finish |
| `WYRD_VERIFICATION_MONTY_MIN_WORKERS` | 1 | Python workers started with the server, in each of the direct and queued pools |
| `WYRD_VERIFICATION_MONTY_MEMORY_MIB` | 256 | Memory one Python operation may allocate |
| `WYRD_VERIFICATION_MONTY_PROCESS_MEMORY_MIB` | 384 | What one worker process is counted at against the pool ceiling |
| `WYRD_VERIFICATION_MONTY_POOL_MEMORY_MIB` | 2048 | All worker processes of both pools together, idle ones included |
| `WYRD_VERIFICATION_CODE_TIMEOUT_MS` | 10000 | One Python operation, from waiting for a worker through compiling, running, and its file calls |
| `WYRD_VERIFICATION_AGENT_CODE_BUDGET_MS` | 60000 | All `code.run` calls of one Agent step together, across its retries |

`_MIB` and `_KIB` are binary units; `_MS` is milliseconds. A `media` slot's `max_bytes` can lower `FILE_MAX_MIB` for that slot, never raise it. No setting disables a limit or worker isolation.

## Direct and queued capacity

`verify` calls and queued executions each have their own execution slots, workspaces, Python workers, and model-call admission, set by the `DIRECT_` and `QUEUED_` settings above. Neither borrows the other's free capacity: a full queue never takes what `verify` needs, and a burst of `verify` calls never takes what queued work was given. A queued execution resumed after it waited, was deferred, or was retried runs on queued capacity again. Both run the same engine with the same authority, evidence, billing, and cancellation rules, and every model call still passes the gateway's own tenant and subject limits and budgets.

The reservation covers these admissions only. CPU, memory outside the Python pools, Postgres, the gateway, and model providers are shared, so a saturated queue can still slow `verify` calls; run separate server processes for stronger separation.

`EXECUTION_TIMEOUT_MS` is the ceiling of one queued execution from its first claim. Waiting for capacity, deferrals, and retries count against it and never reset it; anything an execution waits for, including an intended human approval period, must fit inside it.

## Server TOML file

`wyrd-server` reads the first file found, in this order. Environment variables override its values.

| Order | File |
| ---: | --- |
| 1 | The path in `WYRD_CONFIG_PATH`; startup fails if it does not exist |
| 2 | `/etc/wyrd/server.toml` |
| 3 | `~/.config/wyrd/server.toml` |

With no file, the server uses its defaults and environment variables.

## Startup rules

The server refuses to start, naming the variable, when a value is empty, not a positive integer, or overflows once scaled to bytes, or when `WYRD_VERIFICATION_ENABLED` is not `1` or `0`. It also refuses these combinations, naming both variables (prefix `WYRD_VERIFICATION_` omitted):

- `MONTY_MIN_WORKERS` above `DIRECT_PYTHON_WORKERS` or `QUEUED_PYTHON_WORKERS`
- `MONTY_MEMORY_MIB` at or above `MONTY_PROCESS_MEMORY_MIB`
- `MONTY_POOL_MEMORY_MIB` below `MONTY_PROCESS_MEMORY_MIB`
- `MONTY_PROCESS_MEMORY_MIB` below `MONTY_MEMORY_MIB` + 4 MiB allocator headroom
- `DIRECT_PYTHON_WORKERS` + `QUEUED_PYTHON_WORKERS`, times `MONTY_PROCESS_MEMORY_MIB`, above `MONTY_POOL_MEMORY_MIB`
- `FILE_MAX_MIB` above `FILES_MAX_MIB`
- `WORK_WRITE_MAX_MIB` above `SCRATCH_MAX_MIB`

The defaults fit 2 + 3 = 5 workers of 384 MiB in the 2048 MiB pool.

## If a limit is reached

| Limit reached | Result |
| --- | --- |
| A file over its slot or `FILE_MAX_MIB`, or a request over `FILES_MAX_MIB` | `WYRD_VERIFICATION_413_INPUT_TOO_LARGE` before anything runs |
| The server's verification staging is full (`scribe.outbox_capacity` or `scribe.outbox_bytes`) | Direct and new queued calls: `WYRD_VERIFICATION_429_STAGING_FULL` with `Retry-After`, before anything is staged. Accepted queued executions stay queued until there is room for their result |
| `TIMEOUT_MS` in `verify` | `WYRD_VERIFICATION_504_EXECUTION_TIMED_OUT` |
| No direct execution slot, or no direct Python worker or workspace free | `verify`: `WYRD_SERVER_503_SERVICE_UNAVAILABLE`, even when queued capacity is free |
| No queued execution slot, or no queued Python worker or workspace free | Queued: stays queued, attempt not counted, even when direct capacity is free |
| Every model call of the lane in use | The call waits for its lane, within the execution's deadline |
| Verification disabled on this process | `WYRD_SERVER_503_SERVICE_UNAVAILABLE` |
| A publication over `FILE_MAX_MIB`, `FILES_MAX_MIB`, or `PUBLICATIONS_MAX` | The publish call is refused with the limit and nothing is published; the execution's accepted publications and their totals survive retries and recovery |
| Any other limit in a step or check, including `AGENT_CODE_BUDGET_MS` | The step fails or the check is `inconclusive`, with the limit in its `reason` |
