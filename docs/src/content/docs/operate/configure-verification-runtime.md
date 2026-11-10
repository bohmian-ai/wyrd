---
title: Verification runtime
description: Choose which processes run Verifiers, size the worker pool, and diagnose startup refusals.
pillar: wyrd
section: Operate
order: 4
---

Decide which processes run Verifiers and how much Python each runs at once. Every value is in [Verification runtime limits](/reference/verification-runtime-limits/).

## Choose which processes run Verifiers

Every API-serving process runs Verifiers by default. To keep one out, set `WYRD_VERIFICATION_ENABLED=0`, or `enabled = false` under `[verification]` in the [server TOML file](/reference/verification-runtime-limits/#server-toml-file). Route verification traffic to the processes that keep it on.

## Split direct and queued capacity

Each process reserves capacity for `verify` calls separately from queued executions: execution slots, workspaces, Python workers, and model calls. Neither side borrows the other's, so a full queue cannot block `verify`. Size each side for its own traffic with the `WYRD_VERIFICATION_DIRECT_*` and `WYRD_VERIFICATION_QUEUED_*` settings in [runtime limits](/reference/verification-runtime-limits/#direct-and-queued-capacity), or with `[verification.direct]` and `[verification.queued]` in the [server TOML file](/reference/verification-runtime-limits/#server-toml-file). CPU, Postgres, the gateway, and model providers stay shared.

`WYRD_VERIFICATION_EXECUTION_TIMEOUT_MS` (`execution_timeout_ms` under `[verification]`) bounds each queued execution from its first claim, waits and retries included. Raise it to fit the longest wait an execution is meant to survive, such as an intended human approval period.

## Size the worker pools

The server starts its own Python workers; there is nothing to install. Each process has a direct and a queued pool, and a process that runs Verifiers starts `WYRD_VERIFICATION_MONTY_MIN_WORKERS` workers in each before it reports ready.

- The defaults, sized for a 32 GiB server, allow 4 direct and 12 queued workers counted at 384 MiB each against an 8192 MiB pool. The pool is startup accounting, not memory set aside, so size the host for the workload rather than for the pool alone.
- To run more Python at once, raise `WYRD_VERIFICATION_MONTY_POOL_MEMORY_MIB` (`monty_pool_memory_mib` under `[verification]`) with `WYRD_VERIFICATION_DIRECT_PYTHON_WORKERS` or `WYRD_VERIFICATION_QUEUED_PYTHON_WORKERS`; both pools' workers together must fit the pool memory.
- To give each operation more memory, raise `WYRD_VERIFICATION_MONTY_MEMORY_MIB` with `WYRD_VERIFICATION_MONTY_PROCESS_MEMORY_MIB`, which must exceed it by at least 4 MiB.
- Leave memory beyond the pools for the server, extraction, and model responses, and temporary-directory space for `WYRD_VERIFICATION_DIRECT_WORKSPACES` plus `WYRD_VERIFICATION_QUEUED_WORKSPACES` workspaces of up to `WYRD_VERIFICATION_SCRATCH_MAX_MIB` each.

Settings are read at startup; restart to apply a change.

## If it fails

| You see | Fix |
| --- | --- |
| `hosted Verifier blocks could not start` | Fix the cause the rest of the message names, then restart |
| `environment variable WYRD_VERIFICATION_... could not be parsed` | Set a positive integer |
| Two variable names in a startup error | Fix the pair; see [startup rules](/reference/verification-runtime-limits/#startup-rules) |
| `WYRD_SERVER_503_SERVICE_UNAVAILABLE` from `verify` | The process has verification off or no free worker; retry or route to another process |

## Next

[Verification runtime limits](/reference/verification-runtime-limits/) lists every setting, its default, and what happens when a limit is reached.
