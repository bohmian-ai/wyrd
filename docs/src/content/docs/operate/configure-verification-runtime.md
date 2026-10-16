---
title: Configure the verification runtime
description: Install the Python worker, choose which processes run Verifiers, size the worker pool, and diagnose startup refusals.
pillar: wyrd
section: Operate
order: 4
draft: true
status: runnable
---

The verification runtime runs Verifier steps and checks inside `wyrd-server`: file extraction, scratch workspaces, Agent tools, and Python code in a pool of `wyrd-monty-worker` processes. Every setting, default, and startup rule is in [Verification runtime limits](/reference/verification-runtime-limits/); this page covers the decisions an operator makes. For database and server setup, see [Deploy Wyrd for a team](/operate/deploy-wyrd-for-a-team/).

## Install the worker

`wyrd-server` runs Python in a separate `wyrd-monty-worker` binary. Release bundles ship it beside `wyrd-server`, and the official images install both in `/usr/local/bin`, so the default needs no setting.

If you install the binaries yourself, keep the two side by side, or point the server at the worker:

```bash
export WYRD_VERIFICATION_MONTY_WORKER=/opt/wyrd/bin/wyrd-monty-worker
```

Use the worker from the same release as the server. A server that runs Verifiers refuses to start when the worker is missing or cannot start:

```text
hosted Verifier blocks could not start: ...
```

## Choose which processes run Verifiers

Every process whose role serves the API runs Verifiers by default. To keep a process out, set `WYRD_VERIFICATION_ENABLED=0` (or `enabled = false` under `[verification]` in TOML). That process starts no worker, and its verification routes answer `WYRD_SERVER_503_SERVICE_UNAVAILABLE`, so route verification traffic to processes that keep it on.

## Size the worker pool

Each process has its own pool and its own limits; settings do not add up across replicas. Three settings decide how many Python operations run at once:

- `WYRD_VERIFICATION_MONTY_POOL_MEMORY_MIB / WYRD_VERIFICATION_MONTY_PROCESS_MEMORY_MIB`, rounded down, caps live workers. At the defaults that is 5.
- `WYRD_VERIFICATION_MONTY_MAX_WORKERS` caps them further.
- `WYRD_VERIFICATION_MONTY_MAX_WAITERS` operations may wait beyond that. Anything more is refused at once and fails as retryable.

To run more Python at once, raise the pool ceiling and the worker cap together. Raise `WYRD_VERIFICATION_MONTY_MEMORY_MIB` only with `WYRD_VERIFICATION_MONTY_PROCESS_MEMORY_MIB`, which must exceed it by at least 4 MiB.

The pool ceiling covers the workers only. Leave memory for the server itself, file extraction, model responses, and tool buffers on top of it. Each open workspace also uses up to `WYRD_VERIFICATION_SCRATCH_MAX_MIB` of the server's temporary directory, with at most `WYRD_VERIFICATION_MAX_WORKSPACES` open at once.

## Change a setting

The server reads these settings once at startup, so a change takes a restart. A bad value stops startup with the variable's name, for example:

```text
environment variable WYRD_VERIFICATION_FILE_MAX_MIB could not be parsed: expected a positive integer, got "0"
```

A pair of settings that contradict each other, such as `WYRD_VERIFICATION_FILE_MAX_MIB` above `WYRD_VERIFICATION_FILES_MAX_MIB`, stops startup with both names. The [startup rules](/reference/verification-runtime-limits/#startup-rules) list every pair.

Settings can tighten or loosen limits, but never turn off a limit or the worker isolation. Built-in Agent tools and the permissions they need are in [Agent tools and authorization](/reference/agent-tools-and-authorization/).
