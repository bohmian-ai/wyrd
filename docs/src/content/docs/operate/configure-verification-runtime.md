---
title: Verification runtime
description: Choose which processes run Verifiers, size the worker pool, and diagnose startup refusals.
pillar: wyrd
section: Operate
order: 4
status: runnable
---

Decide which processes run Verifiers and how much Python each runs at once. Every value is in [Verification runtime limits](/reference/verification-runtime-limits/).

## Choose which processes run Verifiers

Every API-serving process runs Verifiers by default. To keep one out, set `WYRD_VERIFICATION_ENABLED=0`, or `enabled = false` under `[verification]` in the [server TOML file](/reference/verification-runtime-limits/#server-toml-file). Route verification traffic to the processes that keep it on.

## Size the worker pool

The server starts its own Python workers; there is nothing to install. Each process has its own pool, and a process that runs Verifiers starts its first `WYRD_VERIFICATION_MONTY_MIN_WORKERS` workers before it reports ready.

- To run more Python at once, raise `WYRD_VERIFICATION_MONTY_POOL_MEMORY_MIB` and `WYRD_VERIFICATION_MONTY_MAX_WORKERS` together.
- To give each operation more memory, raise `WYRD_VERIFICATION_MONTY_MEMORY_MIB` with `WYRD_VERIFICATION_MONTY_PROCESS_MEMORY_MIB`, which must exceed it by at least 4 MiB.
- Leave memory beyond the pool for the server, extraction, and model responses, and temporary-directory space for `WYRD_VERIFICATION_MAX_WORKSPACES` workspaces of up to `WYRD_VERIFICATION_SCRATCH_MAX_MIB` each.

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
