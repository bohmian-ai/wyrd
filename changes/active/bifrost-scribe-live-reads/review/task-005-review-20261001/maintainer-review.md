# TASK-005 maintainer review

**Subject:** `05d7d741304af3b0b4e667e7e18f93dec16b897b..885d16c11ecc7a3eda73b5f1b27dd40c0a2cece2` (the later dashboard-spec commit excluded). **Result: FAIL.** This is a source-layout and maintainability review, not a task-acceptance verdict. I read the complete changed-file list and diff, changed owners and their nearby callers/tests, `AGENTS.md` §§5, 11, 16, `architecture/agent-rules.md`, `architecture/references/languages/maintainer-style.md`, the approved task and spec, and the Bifrost design authority. `.codegraph/` is absent.

## Changed-surface coverage

| Surface | Owner/caller/test path inspected | Maintainer assessment |
| --- | --- | --- |
| Gate and transport errors | `gate/{mod,error}.rs` → HTTP/gRPC OTLP edges; query lifecycle in `oracle/query_stream.rs` | Edge helper and stream parentage have clear owners; the changed error and result methods carry useful intent. |
| Scribe staging and metrics | `scribe/{assembly,staging_runtime,persistence,mod,shards,ingress,execution_lanes,memory,telemetry}.rs` → server test inspection → Scribe telemetry and write/read journeys | Backlog reads existing assembler ownership; test inspection follows the owner. Three new signatures hide that domain type's import; finding M-1. |
| Oracle query and telemetry | `oracle/{mod,telemetry,exec,query_stream,dispatcher,pruning,planner,live,follower}.rs` → published, capacity, and peer-network journeys | Query guard and stream wrapper are discoverable at their owners; producer-to-sample paths are documented. The new published journey hides a domain type in its signature; finding M-1. |
| Shared storage | `storage/{mod,cache,telemetry}.rs` → Oracle storage journey and cache/storage unit tests | Inspection reads real cache/request owners; recorder helpers stay test-only. No separate production telemetry authority remains. |
| Forge | `forge/worker.rs` → catalog settlement methods and live-rewrite journey | `ForgeSettledAttempt` carries committed result through the existing worker episode; caller paths and docs identify cases requiring a durable-state read. |
| Capture, docs, benchmark | `wyrd-telemetry`, `wyrd-testing` capture/server/workload, bench support, architecture and operator docs | Capture fields and metric consumers follow the changed producers. No generated Python/TypeScript declaration surface changed. |

## Material finding

**M-1 — Changed modules conceal dependencies outside their import blocks (`AGENTS.md` §16 and `architecture/agent-rules.md`, top-level imports/bare signature names; maintainer-style “document the typed contract”).** The new backlog return signatures at `scribe/staging_runtime.rs:175`, `scribe/persistence.rs:1007`, and `scribe/mod.rs:2226` each spell `crate::scribe::assembly::StagingBacklog` inline. The new published journey helper at `wyrd-testing/tests/bifrost/oracle/published.rs:602` spells `wyrd_spec::DataTenantId` inline. The new `QuerySpanJoinSetTracer::trace_future` method at `oracle/telemetry.rs:20` imports `tracing::Instrument` inside the function, although the agent-rule exception covers only a generic function needing a locally scoped trait. Their callers use these types as concrete domain facts: `ScribeImpl` forwards the assembler backlog through persistence to the server inspection; the published helper receives the journey tenant; Oracle installs the tracer at owner creation. A maintainer reading the module imports cannot see these dependencies, and the repeated long return type obscures the small forwarding contract. **Smallest correction:** import `StagingBacklog`, `DataTenantId`, and `tracing::Instrument as _` at the respective module tops and use bare names in the changed signatures. This changes no behavior or test assertion.

## Calibration, not findings

- `storage/telemetry.rs` now contains a test-only recorder-reading module. It is several helpers, but cache and request-owner tests share them, and it emits no production signal. No concrete maintenance defect warrants moving it.
- The stream polling wrapper and DataFusion span tracer add tracing machinery, but each addresses a distinct async lifetime. Their correctness belongs to the task and resilience reviews; no additional maintainer abstraction is justified here.

Verification was reported for the focused scenarios, module units, Scribe journey, formatting, lints, and docs. I did not rerun tests for this read-only maintainer pass. The benchmark, broad gate, and whole journey lanes were not run.
