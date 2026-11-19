---
title: Verification runtime
browserTitle: Configure the verification runtime | Wyrd
description: Choose which processes run Verifiers, size the worker pool, and diagnose startup refusals.
pillar: wyrd
section: Operate
order: 4
---

Decide which processes run Verifiers and how much Python each runs at once. Every value is in [Verification runtime limits](/reference/verification-runtime-limits/).

## Choose which processes run Verifiers

Every API-serving process runs Verifiers by default. To keep one out, set `WYRD_VERIFICATION_ENABLED=0`, or `enabled = false` under `[verification]` in the [server TOML file](/reference/verification-runtime-limits/#server-toml-file). Route verification traffic to the processes that keep it on.

## Keep `verify` calls apart from queued work

`verify` calls and queued executions on one process share its Python pool, CPU, model calls, memory, and scratch, so a busy queue can slow `verify`. `WYRD_VERIFICATION_DIRECT_EXECUTIONS` and `WYRD_VERIFICATION_QUEUED_EXECUTIONS` cap how many of each run at once, and `WYRD_VERIFICATION_MODEL_CALLS` caps model calls for both together; see [runtime limits](/reference/verification-runtime-limits/#direct-and-queued-capacity) or the [server TOML file](/reference/verification-runtime-limits/#server-toml-file). To keep `verify` calls apart from the queue, run `direct` and `worker` processes on separate pods (see [deployment targets](/understand/architecture/#one-binary-several-roles)). Postgres, the gateway, and model providers stay shared.

`WYRD_VERIFICATION_EXECUTION_TIMEOUT_MS` (`execution_timeout_ms` under `[verification]`) bounds each queued execution from its first claim, waits and retries included. Raise it to fit the longest wait an execution is meant to survive, such as an intended human approval period.

## Size the worker pools

The server starts its own Python workers; there is nothing to install. A process that runs Verifiers prewarms up to `WYRD_VERIFICATION_MONTY_MIN_WORKERS` workers, as many as fit in its memory, before it reports ready.

There is no worker count to set. Python operations share the process's memory with everything else the server runs. Each operation is charged one session envelope: twice its allocator ceiling (`WYRD_VERIFICATION_MONTY_MEMORY_MIB` plus 4 MiB of headroom), plus 32 MiB for the worker itself. That is 552 MiB at the defaults. A worker keeps that charge until its process exits, because a reset returns no memory to the system. Idle workers are stopped whenever an operation is waiting for memory.

- To run more Python at once, give the process more memory.
- To give each operation more memory, raise `WYRD_VERIFICATION_MONTY_MEMORY_MIB`. Fewer operations then fit at once.
- Operations waiting for memory are served first-in first-out within a tenant and round robin across tenants.
- Workspaces are written under `verification-scratch/` in the Bifrost data directory. Each one charges the process scratch budget for the bytes it holds plus each `/work` write as it is admitted, up to `WYRD_VERIFICATION_SCRATCH_MAX_MIB`. A write past the workspace's own `/work` limit fails with `OSError` in Python or a `work_budget` tool error. A write that budget cannot hold now stops the operation as busy instead, so a queued execution is deferred and a direct one returns 503; process load never reaches the program or the model. A workspace streams each input from storage straight to disk, verifying its length and SHA-256 as it arrives, so process memory holds only the chunk in flight; scratch is charged for the declared size before the transfer starts, and an archive input holds its compressed and unpacked bytes together until it is unpacked. Every other evidence read (native media, the `extract` action, citations, Artifact code) and `fs.extract` charges process memory for its bytes before it reads them. There is no workspace count.

A `code.run` program keeps its worker and its memory while it waits on `call_tool`, so a program that calls many tools holds them longer than one that only computes. While it waits, it frees its CPU turn: at most one Python turn per effective CPU runs at once. Each call from Python is authorized and audited on its own, like a call from the model, so fewer model turns do not mean fewer permission checks or audit rows.

Settings are read at startup; restart to apply a change.

## Upgrade before using `call_tool`

Only a server that runs this release's Verifiers offers `call_tool` to `code.run` programs; on an older server the program fails at its first `call_tool`. Upgrade every process that runs Verifiers before declaring Agents or prompts whose programs depend on it, following your [deployment's](/operate/deploy-wyrd-for-a-team/) normal release order.

## If it fails

| You see | Fix |
| --- | --- |
| `hosted Verifier blocks could not start` | Fix the cause the rest of the message names, then restart |
| `environment variable WYRD_VERIFICATION_... could not be parsed` | Set a positive integer |
| Two variable names in a startup error | Fix the pair; see [startup rules](/reference/verification-runtime-limits/#startup-rules) |
| `WYRD_SERVER_503_SERVICE_UNAVAILABLE` from `verify` | The process has verification off or no free worker; retry or route to another process |
| `WYRD_VERIFICATION_422_TOOL_BUDGET_EXCEEDED` in a step or check `reason` | The Agent made more than `WYRD_VERIFICATION_AGENT_TOOL_MAX_CALLS` tool calls; reduce them inside one `code.run` program, or raise the limit |

## Next

[Verification runtime limits](/reference/verification-runtime-limits/) lists every setting, its default, and what happens when a limit is reached.
