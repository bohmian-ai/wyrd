---
title: "Discussion"
description: "Tradeoffs in the v1 evaluation runtime primitive."
---

# Discussion

## In-memory `vala-http` state in v1

The v1 HTTP protocol stores active runs in process memory with a map protected
by an async lock. That is the right boundary for a single-replica dev and CI
surface: the turn protocol can validate lease auth, advance run state, score
scenarios, and prove the wire contract without introducing a durable control
plane before it is needed.

The tradeoff is explicit. In-memory state is not a multi-replica production
backend. The `vala-control` followup moves run state, leases, and result lookup
into durable storage while preserving the client-owned pull protocol.

## Sync Python protocol client

The v1 Python surface is synchronous because the caller contract is a normal
Python callable: `agent_fn(message, history)`. There is no need to expose an
async bridge at this layer while the server owns orchestration and the Python
client only follows directives.

That also keeps PyO3 ownership narrow. The protocol client lives in the Vala
client boundary, while `python/py-wyrd` remains the thin package aggregator.
No Python module duplicates the orchestration engine.

## Why `--records` and `--agent-url` are both first-class

`--records` and `--agent-url` answer different questions. Record replay is
fast, deterministic, and useful when you already captured `EvalRecord`
observations. It is the right loop for scoring changes, baseline checks, and
trace replay.

Agent driving is broader. It exercises scenario loading, turn state,
conversation history, simulated-user behavior, agent HTTP integration, record
capture, scoring, and aggregation. It costs more, but it covers failures that
record replay cannot see.

## Why no `local=True` Python entry

The no-server Python path today is the CLI: `wyrd eval run --local`. A Python
embedded entry would need its own lifecycle decisions around callbacks,
runtime ownership, local result files, and optional server parity. That is a
separate surface, not a keyword on the thin protocol client.

## Future evaluation surfaces

The first followup is `source_ref` plus online and archived evaluation. That
work ties production observations and archived records back to registered Wyrd
identity without changing the `EvalSpec` task model documented here.

Other followups include durable `vala-control` run storage, a result-fetch
route for `RunSummary`, richer Python convenience helpers, annotation and
review workflows, scheduled sampling, and UI pages over `EvalResults` and
`EvalComparison`. Each followup should preserve the same boundary: Cards
declare, services execute and record, surfaces project the service without
renaming the contract.
