# Immutable review subject and coverage plan

Repository: /home/thorrester/Documents/GitHub/wyrd-worktrees/resource-scaling
Base: 0c81715f2; candidate: 180879328ac9dde94d7504855e821980e5db0f18.
First joint acceptance review of TASK-005 (revision 6) and TASK-006 (revision 7), using the complete `git diff 0c81715f2...180879328` and committed task/spec authorities. No prior acceptance is carried forward: historical reviews concern TASK-001; no reliance on pre-rebase finding closure. Implementation evidence's old commits are navigation claims only, verified against candidate source. Exclude untracked directories, including TASK-001-review-002 and gateway-latency-capture. Source stays immutable; reports only here. No benchmark or profile run is authorized by this review.

## Shared navigation map (starting points, not coverage limits)

- Benchmark: wyrd-benchmark lib/heap; client transport http/trace; skald-providers exchange/clients; wyrd-gateway engine/request; server gateway telemetry and verification telemetry; wyrd-testing capacity/{gateway,verification}, gateway_capture, profile, experiment; manifests/mise. Consumers: canonical JSON reports, driver/server profiling and tests.
- Compact contract: wyrd-spec verification/execution; server verification/results, components/verification/{routes,service}, mcp/verification; shared client verify/execution; all SDK projections/stubs/generated declarations; Rust/Python/TS authoring/verifier/real-time journeys, MCP verification tests, docs.
- Local governance: server components/gateway/{usage,ledger,invocation,snapshots,owner,service}; SQL queries/gateway_admission and migration 20261111000000; runtime drain and call tasks; pg_invocation_tests and gateway/resilience journey. Existing owners: Gateway runtime/snapshot lifecycle, TenantConn SQL capability, durable usage ledger.
- Agent reductions: skald-agent agent/structured/loop_runtime, skald-spec prompt/authoring, vala-verify blocks/agent and blocks/engine. Consumers: public Agent.prompt mutation, prompt_ref/to_spec, effective-schema validation, evidence/citation/repair. Tests: Skald constructor/Card/structured output, Vala blocks and journeys.

## Required independent roles

| Contract/invariant | Owners/producers/consumers | Selection and source-based reason |
|---|---|---|
| Complete acceptance, measurement integrity, compact/details behavior, Agent mutation and validation | Tasks/spec; map above | behavior-rev: entire range and both task acceptance matrices |
| Tenant-local atomic limits, reservation accounting, refresh races and replay | GatewayUsage, GatewayLedger, SQL sync and admitted calls | invariant-rev: new shared mutable memory and background acknowledgements |
| Cross-tier mandatory rules and public declaration parity | spec/server/client/SDK/docs/tooling, nested AGENTS | repo-rev: broad changed Rust/Python/TS/SQL/docs surfaces |
| Ownership and reuse of state/cache/schema/recorder machinery | Gateway, snapshot/usage, Prompt/Agent and benchmarks | maintainer-rev: substantial ownership and abstraction changes; compare existing owners |
| Shutdown, SQL outage, cancellation, stale/cold/full recovery and shared-server availability | Gateway owner/drain, refresh_worker, usage run, calls and app lifecycle | system-rev: async accounting changes failure propagation and recovery |
| Persistent data integrity and RLS tenancy | contribution migration, sync_gateway_usage, TenantConn, ledger consumers | domain-rev-data: durable idempotency, pruning, isolation and concurrency leases |
| Permission/audit parity and trust boundaries for details reads | HTTP/MCP details/status, service authorizers, SDKs and audit | domain-rev-security: new public evidence read requires independent authority/tenant/audit coverage |

Concurrency expertise is supplied by invariant-rev plus domain-rev-data; no additional separate concurrency role needed. No other sensitive domain is materially affected. All selected reviewers receive this map and independently inspect source. A fresh ponytail-rev validates every discovery report and applicability; focused follow-up only for material unresolved claims or validator-origin new findings.

## Verification policy

Reported prior checks are evidence claims in task files and the user request, not checks rerun by this review. Static review may use narrow existing tests if necessary; no broad gates, load, profiling, or capacity runs. Bench-only production reachability requires manifest/cfg/caller inspection. Task-006 explicitly records missing post-reduction unprofiled measurements; evaluate that obligation without inventing performance evidence. A spec revision is not grounds to omit remediations for independently validated implementation defects within approved behavior.
