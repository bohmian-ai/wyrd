---
title: Verification runtime limits
description: Environment settings that bound Verifier files, extraction, workspaces, and Python code, with defaults and startup rules.
pillar: wyrd
section: Reference
order: 5
status: runnable
---

Environment variables on `wyrd-server`, read once at startup; a change takes a restart. Apart from `WYRD_VERIFICATION_ENABLED`, none has a TOML equivalent. Each process applies them on its own.

## Settings

| Variable | Default | Bounds |
| --- | ---: | --- |
| `WYRD_VERIFICATION_ENABLED` | `1` | Whether an API-serving process runs Verifiers: `1` or `0`. Also `[verification] enabled` in TOML |
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

`_MIB` and `_KIB` are binary units; `_MS` is milliseconds. A `media` slot's `max_bytes` can lower `FILE_MAX_MIB` for that slot, never raise it. No setting disables a limit or worker isolation.

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

- `MONTY_MIN_WORKERS` above `MONTY_MAX_WORKERS`
- `MONTY_MEMORY_MIB` at or above `MONTY_PROCESS_MEMORY_MIB`
- `MONTY_POOL_MEMORY_MIB` below `MONTY_PROCESS_MEMORY_MIB`
- `MONTY_PROCESS_MEMORY_MIB` below `MONTY_MEMORY_MIB` + 4 MiB allocator headroom
- `FILE_MAX_MIB` above `FILES_MAX_MIB`
- `WORK_WRITE_MAX_MIB` above `SCRATCH_MAX_MIB`

Live workers are capped by `MONTY_MAX_WORKERS` and by `MONTY_POOL_MEMORY_MIB / MONTY_PROCESS_MEMORY_MIB`, rounded down: 5 at the defaults.

## If a limit is reached

| Limit reached | Result |
| --- | --- |
| A file over its slot or `FILE_MAX_MIB`, or a request over `FILES_MAX_MIB` | `WYRD_VERIFICATION_413_INPUT_TOO_LARGE` before anything runs |
| Verification staging past three quarters of `scribe.outbox_capacity` | `WYRD_VERIFICATION_429_STAGING_FULL` with `Retry-After` |
| `TIMEOUT_MS` in `verify` | `WYRD_VERIFICATION_504_EXECUTION_TIMED_OUT` |
| No Python worker or workspace free | Direct call: `WYRD_SERVER_503_SERVICE_UNAVAILABLE`. Queued: stays queued, attempt not counted |
| Verification disabled on this process | `WYRD_SERVER_503_SERVICE_UNAVAILABLE` |
| Any other limit in a step or check, including `AGENT_CODE_BUDGET_MS` | The step fails or the check is `inconclusive`, with the limit in its `reason` |
