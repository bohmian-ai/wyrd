# Agent check latency and Gateway usage-cache research

Research date: 2026-11-10 (local). Source/artifact inspection only; no new
benchmark, build, production change, or test execution.

## The saved timings already split the Gateway work

Unprofiled reference:
`target/benchmarks/verification/direct-verify/20261111T001720Z-7b66f9ea4.json`.
Commit: `7b66f9ea447e31222e4e5909dc5e957b3f82f0fa`; recorded dirty diff:
`ae6c4007b521bfa0b7977c3ce8484bb75ccd9abeeb92901b8ecf5c2ba7e87804`.
The local date and UTC artifact date differ across midnight.

Each raw window is already saved under
`target/benchmarks/verification/direct-verify/20261111T001720Z/timings/<step>/repetition-<n>.json`.
These contain measured server and driver `before`/`after` snapshots, process
identity, histogram buckets, and reconciliation. Gateway phases and waits are
present even though Verification's current `BREAKDOWN` omits them. Do not run
another baseline to obtain these existing observations.

The following are approximate diagnostic means in microseconds, computed from
the difference of matching cumulative histogram buckets. Bucket upper bounds
were weighted by interval counts, so these are rounded upper estimates, not
new benchmark results or replacements for the canonical report. Process IDs
matched; all bucket deltas were nonnegative and their sums matched count deltas.

| Existing boundary | c1 R1 / R2 / R3 | c64 R1 / R2 / R3 |
|---|---|---|
| Agent block | 521.8 / 540.8 / 556.1 | 2790.3 / 2739.4 / 3062.4 |
| Gateway provider headers | 63.4 / 68.4 / 67.1 | 1388.0 / 1353.5 / 1545.7 |
| Gateway provider body | 1.86 / 1.94 / 1.92 | 3.31 / 3.32 / 3.48 |
| Gateway dispatch wait | 12.4 / 12.6 / 13.0 | 24.1 / 23.6 / 25.9 |
| Gateway caller resume wait | 12.7 / 13.2 / 13.4 | 36.4 / 35.7 / 39.4 |
| Gateway adapter translation | 26.2 / 27.1 / 27.1 | 41.8 / 41.5 / 43.7 |

c1 repetition counts are 6396 / 6140 / 6009; c64 counts are
69022 / 70212 / 62899 for every boundary above. Server PID is 1109587.
This establishes a shared measured-window population for these diagnostics,
not cross-process request tracing. Phases are nested in the Agent total;
translation and wait semantics come from `ExecutionTiming`/`CallTelemetry`.
Never sum their percentiles or treat all timing families as disjoint.

In c1 R1, snapshot/authorize/route/admit are approximately
0.30 / 3.56 / 2.31 / 2.48 us; credential resolution is 0.05 us and
accounting 1.59 us. This fixture configures no limits/budgets and skips ledger
SQL. Its timings cannot establish the latency of a tenant with quotas.

The provider-header and handoff observations do not explain approximately
500 us of waiting at c1. At c64 the provider-header interval is material, but
includes connection-pool, HTTP, provider-process scheduling, and server runtime
work; it does not identify an external provider compute bottleneck. The fake
provider's handler timer starts after body collection and ends before response
transport (`gateway_capture.rs::PacedUpstream` handler near lines 592–613).

## Repeated schema preparation is reachable and sampled

The call path inspected in current source is:

`AgentExecutor::resolve/bind/invoke` → `Agent::run_prompt` →
`loop_runtime::run_loop` → `StructuredOutput::new/check` → in-process
`ServerWyrdGatewayCaller` → `GatewayInvocation` → pooled provider HTTP →
structured output → `verdict_from` → citation validation.

- `vala-verify/src/blocks/agent.rs::bind` clones a NativePrompt and narrows
  reachable citation slots through `edit_response_schema_mut`.
- `skald-spec/src/prompt.rs::edit_response_schema_mut` rederives response type
  and calls `check_output_schema`, which performs meta-schema validation.
- `skald-agent/src/loop_runtime.rs` derives the schema after rendering and
  `before_model` callbacks, then creates `StructuredOutput`. It already reuses
  that validator across iterations of one run while the exact schema matches.
  It does not reuse it across new Agents/runs.
- `skald-agent/src/structured.rs::StructuredOutput::new` compiles a validator.
- `vala-verify/src/blocks.rs::verdict_from` compiles another validator before
  decoding the Verdict and checking custom-data bounds. The code-check caller
  in `verification/hosted/sandbox.rs` also relies on this validation and must
  not become an unchecked path.
- `verification/hosted/blocks.rs::HostedBlocks::engine` constructs a new
  `AgentExecutor` for each execution. A cache owned only by that executor
  would not remove preparation across direct requests. `HostedBlocks` is the
  existing process-lifetime owner; exact retained private state is not selected.

Previously inspected CPU capture:
`target/benchmarks/verification/direct-verify/20261111T001926Z-7b66f9ea4.json`,
with server profiles under its sibling timestamp directory's
`profiles/direct-verify-model-{c1,c64}/cpu-server-1112569.json.gz` and matching
symbols in `target/benchmarks/task005/result-compact/bin/wyrd-server.syms.pkl`.
Whole-step warmup/repetitions/settling samples show `validator_for` at
12.81% / 15.67% of sampled server CPU, split between `verdict_from` and
`StructuredOutput::new`; `check_output_schema` is 4.49% / 6.43%. These are
CPU-stack shares, not R1 wall-time percentages or promised savings. Broad JSON
and allocator categories overlap this work. A single async parent's stack
share omits work sampled under spawned tasks and cannot establish that the
remaining elapsed time is all waiting.

Preparation reuse must be keyed by the exact effective schema and validation
settings, respect hook changes and reachable slots, remain bounded, and retain
no caller evidence, media, tools, workspace or authority. Public/decoded
`AgentRun.structured_output` is an ordinary map, not proof that a particular
schema validated it; never trust it solely because the field is populated.
No wire-visible trusted flag or process-global unbounded schema registry is
needed. Whether duplicate response validation can safely be avoided is a
separate question from eliminating repeated compilation.

Other observed mechanisms are Prompt/request cloning and native typed request
→ `Value` → provider bytes → typed answer conversions in `gateway/caller.rs`.
They are candidates after schema preparation, not proven explanations of the
whole residual. No extra HTTP Gateway hop or recreated provider connection
pool exists. The fixture has inline Cards, one iteration and no tools; tool
sandbox startup and Agent/Prompt Card lookups do not explain this case.

## Gateway's cache does not currently contain usage

`GatewaySnapshots` caches tenant deployments, credential resolution metadata,
fallback, governance/pricing rules and capture policy. PostgreSQL triggers on
configuration tables notify `wyrd_gateway_snapshot` at commit. Each server
invalidates the tenant and the next request synchronously reloads it.
`NotificationListener` checks listener health every 30 s, times probes out
after 5 s and retries subscriptions after a 1 s delay. This is event-driven
invalidation, not periodic background data refresh.

`GatewayLedger::admit` awaits PostgreSQL before execution when a limit/budget
covers a candidate. It can retry admission after removing exhausted candidates.
`CallTask::settle_buffered` awaits settlement before sending its answer;
streaming sends first and settles after stream completion. The tracked task
protects cancellation/shutdown ownership, not buffered latency.

The existing admission SQL serializes tenant admissions across replicas and
reserves bounded token/spend exposure. PostgreSQL cannot simply check these
limits after provider dispatch: a late refusal cannot undo a billed attempt.
An asynchronous design needs local reservations, shared-usage refresh, and
reconciliation that neither loses pending charges nor counts them twice.
The current settlement SQL's token delta is applied on replay even though
accounting-entry inserts deduplicate; verify/redesign replay at the durable
owner before using it in a retrying background writer.

Follow the existing Gateway owner/cache/listener/task lifecycle. Bifrost's
cut cache provides generation fencing and listener-health precedent; Oracle's
pod-local scheduling is not a tenant-wide spending guarantee. Do not import
Bifrost domain caches into Gateway or silently use the loss-tolerant capture
outbox as the quota ledger.

## Approved follow-up, 2026-11-11

[Spec revision 7](../spec.md) resolves the initial revision-6 scope conflict:
approximate cached quotas include monetary budgets; unflushed usage may be
lost on crash/shutdown expiry; missing/unhealthy/full state returns the
existing retryable 503 until background recovery. No numerical global overrun
guarantee, crash-safe local ledger or strict replica coordination is required.
TDD is waived for TASK-006; focused regression/integration proof remains.

The maintainer requested the simple established pattern, citing LiteLLM.
Official primary sources inspected on 2026-11-11:

- [Life of a Request](https://docs.litellm.ai/docs/proxy/architecture): cached
  checks, database lookup on cache miss, and asynchronous usage updates after
  answering. Its documented cache-miss fallback differs from Wyrd's approved
  refusal/background-recovery behavior.
- [Production Best Practices](https://docs.litellm.ai/docs/proxy/prod#batch-spend-writes):
  recommends batched spend writes (its example interval is 60 seconds). This
  supports batching, not a requirement to copy its interval, Redis or other
  infrastructure into Wyrd. Private scheduling stays implementation-owned.

Remove repeated schema preparation, surface already-recorded Gateway
observations, and correct supported remaining Agent costs. New measurement
still requires the packet's explicit authorization and a quiet host. Neither
a historical baseline rerun nor a new coordination service is warranted.
