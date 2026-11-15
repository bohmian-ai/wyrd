---
title: Runtime limits
description: Server settings that bound Verifier files, extraction, workspaces, Python code, model calls, and direct and queued capacity, with defaults and startup rules.
pillar: wyrd
section: Reference
order: 4
---

Environment variables on `wyrd-server`, read once at startup; a change takes a restart. Direct and queued capacity, the queued share, and the queued execution timeout can also be set in the [server TOML file](#server-toml-file); the other settings are environment variables only. Each process applies them on its own.

## Settings

| Variable | Default | Bounds |
| --- | ---: | --- |
| `WYRD_VERIFICATION_ENABLED` | `1` | Whether an API-serving process runs Verifiers: `1` or `0`. Also `[verification] enabled` in TOML |
| `WYRD_VERIFICATION_TIMEOUT_MS` | 60000 | The whole `verify` call |
| `WYRD_VERIFICATION_EXECUTION_TIMEOUT_MS` | 300000 | One queued execution, from its first claim; waiting and retries do not reset it |
| `WYRD_VERIFICATION_DIRECT_EXECUTIONS` | 256 | `verify` calls running at once in one server process |
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
| `WYRD_VERIFICATION_DIRECT_MODEL_CALLS` | 256 | Gateway model calls `verify` calls make at once; another waits for one to finish |
| `WYRD_VERIFICATION_QUEUED_MODEL_CALLS` | 256 | Gateway model calls queued executions make at once; another waits for one to finish |
| `WYRD_VERIFICATION_QUEUED_CPU_PERCENT` | 50 | Percent of effective CPUs, at least one, queued executions run on: 1 to 99 |
| `WYRD_VERIFICATION_QUEUED_MEMORY_PERCENT` | 25 | Percent of process memory reserved for queued executions: 1 to 99 |
| `WYRD_VERIFICATION_QUEUED_SCRATCH_PERCENT` | 50 | Percent of process scratch reserved for queued executions: 1 to 99 |
| `WYRD_VERIFICATION_TOOL_ARGUMENT_MAX_KIB` | 64 | One tool call's arguments as compact JSON, `code.run` source included |
| `WYRD_VERIFICATION_AGENT_TOOL_MAX_CALLS` | 128 | Tool calls of one Agent step or check, from the model and from Python together, across its retries |
| `WYRD_VERIFICATION_MONTY_MIN_WORKERS` | 1 | Python workers prewarmed with the server in each of the direct and queued pools, as many as fit in that side's memory; stopped when an operation needs their memory |
| `WYRD_VERIFICATION_MONTY_MEMORY_MIB` | 256 | Memory one Python operation may allocate |
| `WYRD_VERIFICATION_CODE_TIMEOUT_MS` | 10000 | One Python operation, from waiting for a worker through compiling, running, its file calls, and its waits on `call_tool` |
| `WYRD_VERIFICATION_AGENT_CODE_BUDGET_MS` | 60000 | All `code.run` calls of one Agent step together, waits on `call_tool` included, across its retries |

`_MIB` and `_KIB` are binary units; `_MS` is milliseconds. A `media` slot's `max_bytes` can lower `FILE_MAX_MIB` for that slot, never raise it. No setting disables a limit or worker isolation.

## Direct and queued capacity

`verify` calls and queued executions each have their own execution slots and model-call admission, set by the `DIRECT_` and `QUEUED_` settings above. Queued executions also run on their own Tokio runtime and Python pool with `QUEUED_CPU_PERCENT` of the effective CPUs, and charge a reservation of `QUEUED_MEMORY_PERCENT` of process memory and `QUEUED_SCRATCH_PERCENT` of process scratch that nothing else can take and that they can never exceed. `verify` calls run on the request runtime and charge what remains of the process budgets, which every other server capability also charges. A full queue never takes what `verify` needs, and queued work never takes direct capacity. A queued execution resumed after it waited, was deferred, or was retried runs on queued capacity again. Both run the same engine with the same authority, evidence, billing, and cancellation rules, and every model call still passes the gateway's own tenant and subject limits and budgets.

Postgres, the gateway, and model providers stay shared, so a saturated queue can still slow `verify` calls through them; run separate server processes for stronger separation.

The defaults give each process headroom to start from, not measured throughput or latency guarantees, and not every mix of workloads is guaranteed to fit a host.

`EXECUTION_TIMEOUT_MS` is the ceiling of one queued execution from its first claim. Waiting for capacity, deferrals, and retries count against it and never reset it; anything an execution waits for, including an intended human approval period, must fit inside it.

## Server TOML file

`wyrd-server` reads the first file found, in this order. Environment variables override its values.

| Order | File |
| ---: | --- |
| 1 | The path in `WYRD_CONFIG_PATH`; startup fails if it does not exist |
| 2 | `/etc/wyrd/server.toml` |
| 3 | `~/.config/wyrd/server.toml` |

With no file, the server uses its defaults and environment variables.

Under `[verification]`, the file can set direct and queued capacity, the queued share, and the queued execution timeout. The matching environment variable overrides each value on its own, and a value left out keeps its default.

| TOML key | Environment variable |
| --- | --- |
| `[verification] direct_executions` | `WYRD_VERIFICATION_DIRECT_EXECUTIONS` |
| `[verification] queued_executions` | `WYRD_VERIFICATION_QUEUED_EXECUTIONS` |
| `[verification] execution_timeout_ms` | `WYRD_VERIFICATION_EXECUTION_TIMEOUT_MS` |
| `[verification] queued_cpu_percent`, `queued_memory_percent`, `queued_scratch_percent` | `WYRD_VERIFICATION_QUEUED_CPU_PERCENT`, `_MEMORY_PERCENT`, `_SCRATCH_PERCENT` |
| `[verification.direct] model_calls` | `WYRD_VERIFICATION_DIRECT_MODEL_CALLS` |
| `[verification.queued] model_calls` | `WYRD_VERIFICATION_QUEUED_MODEL_CALLS` |

```toml
[verification]
direct_executions = 256
queued_executions = 64
execution_timeout_ms = 300000
queued_cpu_percent = 50
queued_memory_percent = 25

[verification.queued]
model_calls = 256
```

## Startup rules

The server refuses to start, naming the variable, when a value is empty, not a positive integer, or overflows once scaled to bytes, or when `WYRD_VERIFICATION_ENABLED` is not `1` or `0`. A TOML value is checked only when no environment variable overrides it, so a valid variable replaces a stale file value; a winning TOML value that is zero or overflows is refused naming its key, and a negative one fails to parse. Whichever source wins is also refused, naming it, when an execution, workspace, or model-call count exceeds 2305843009213693951, the most permits the runtime can hold, or when the queued execution timeout would end after the latest timestamp the queue can represent, about the year 262000. The combinations below are checked after file and environment values are merged, whichever source set them, and are refused naming both variables (prefix `WYRD_VERIFICATION_` omitted):

- `FILE_MAX_MIB` above `FILES_MAX_MIB`
- `WORK_WRITE_MAX_MIB` above `SCRATCH_MAX_MIB`

A `QUEUED_*_PERCENT` of 0 or of 100 or more is refused, so `verify` calls always keep capacity of their own. A process that runs Verifiers also refuses to start when the direct or queued memory cannot hold one Python session envelope: twice `MONTY_MEMORY_MIB` + 4 MiB of allocator headroom, plus 32 MiB for the worker, which is 552 MiB at the defaults. There is no worker or waiter count. Each side's memory bounds how many of its operations run and wait.

## If a limit is reached

| Limit reached | Result |
| --- | --- |
| A file over its slot or `FILE_MAX_MIB`, or a request over `FILES_MAX_MIB` | `WYRD_VERIFICATION_413_INPUT_TOO_LARGE` before anything runs |
| The server's verification staging is full (`scribe.outbox_capacity` or `scribe.outbox_bytes`) | Direct and new queued calls: `WYRD_VERIFICATION_429_STAGING_FULL` with `Retry-After`, before anything is staged. Accepted queued executions stay queued until there is room for their result |
| `TIMEOUT_MS` in `verify` | `WYRD_VERIFICATION_504_EXECUTION_TIMED_OUT` |
| No direct execution slot, or direct memory or scratch cannot hold a Python operation's waiting bytes, frees too late for its envelope, or cannot hold a workspace | `verify`: `WYRD_SERVER_503_SERVICE_UNAVAILABLE`, even when queued capacity is free |
| No queued execution slot, or the queued reservation cannot hold the same | Queued: stays queued, attempt not counted, even when direct capacity is free |
| Every model call of the lane in use | The call waits for its lane, within the execution's deadline |
| Verification disabled on this process | `WYRD_SERVER_503_SERVICE_UNAVAILABLE` |
| A publication over `FILE_MAX_MIB`, `FILES_MAX_MIB`, or `PUBLICATIONS_MAX` | The publish call is refused with the limit and nothing is published; the execution's accepted publications and their totals survive retries and recovery |
| `AGENT_TOOL_MAX_CALLS` | The step fails or the check is `inconclusive`, with `WYRD_VERIFICATION_422_TOOL_BUDGET_EXCEEDED` in its `reason`; a program cannot catch it |
| Any other limit in a step or check, including `AGENT_CODE_BUDGET_MS` | The step fails or the check is `inconclusive`, with the limit in its `reason` |
