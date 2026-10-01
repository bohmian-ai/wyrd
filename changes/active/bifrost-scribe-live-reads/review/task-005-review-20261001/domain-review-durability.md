# Durability and persistent-state domain review

**Subject:** `05d7d741304af3b0b4e667e7e18f93dec16b897b..885d16c11ecc7a3eda73b5f1b27dd40c0a2cece2` (excluding `1f1cbcf5f`). **Result: PASS.**

## Boundary and authority

Reviewed durable Scribe staging registration, ready/claimed ownership, restoration, publication and settlement, plus Forge claim release, committed settlement, task-result observation and recovery. Governing obligations: approved `changes/active/bifrost-scribe-live-reads/spec.md` REQ-012, INV-001 and INV-009, AC-014; original `TASK-005-simplify-bifrost-telemetry.md` Scenario 1 and Scenario 4, including the prohibitions on changing ACK, WAL retirement, Iceberg publication and Forge settlement; `AGENTS.md` §§2, 9, 11–12; `architecture/agent-rules.md`; and `architecture/bifrost-design.md` staging, Forge and telemetry sections. The candidate adds no durable schema or public contract.

## Source and recovery paths

| Path | Evidence and assessment |
| --- | --- |
| Scribe durable stage to gauge | `ScribeStagingRuntime::register_member` publishes the ready record before `StagingAssembler::register_ready`, then publishes `StagingBacklog` from the same ready/claim owner. `StagingAssembler::backlog` includes both ready and outstanding members' encoded bytes and persisted ready times. Claim take moves ownership between those sets without reducing live backlog; `settle` publishes the post-retirement owner snapshot. No telemetry ledger determines claim membership. |
| Scribe restart | `ScribeStagingRuntime::restore` recovers and validates staged records, filters terminal publication claims, rebuilds ready and outstanding ownership, then publishes the complete snapshot. `Scribe::replay_wal_async` sets recovery readiness false before `restore_staging`, publication reconciliation and claim resumption, and sets it true only after successful WAL replay. An error leaves Scribe unready. `restored_stage_republishes_backlog` installs a fresh recorder before the actual async restore and checks all four recovered gauges; the abrupt-restart journey checks exposition, readback and eventual settlement. |
| Scribe publication | `ScribeStagingRuntime::publish` awaits the fenced publisher, then records committed output and settles the claim. The existing publication/retirement machinery stays in its original owner. A failure before confirmed publication leaves claim evidence for retry/recovery. |
| Forge normal settlement | `ForgeSettledAttempt` carries the result committed by each settling transition through `begin_claim_episode`/`finish_admitted_attempt` to `close_claim_episode`. `record_task_execution_telemetry` uses that committed result for the existing task-attempt family, avoiding the prior telemetry-only Postgres read. `ForgeExecutionEvidenceState::Settled(result)` covers self-settled dispatches; other successful `finish_claim_execution` paths return `Succeeded` after their transaction. |
| Forge uncertain settlement and release | `release_cancelled_claim` distinguishes a committed retry from a no-match release. A settlement error, post-effect retention or no-match supplies no committed result, so `record_task_execution_telemetry` reads `durable_task_observation` and maps the actual state/failure class. `settle_claim_execution` preserves a fatal settlement or lease-release error and closes the worker owner. This preserves recovery authority rather than inferring success from Rust `Ok`. |

The task's Forge journey compares task/file state with attempt metrics and task/catalog spans, including release and recovered settlement. The Scribe journeys cover staged restoration, client readback and zero backlog after publication. The final-tree exact Scenario 1–4 commands and touched-module unit tests are reported passing in the task evidence; I inspected their assertions and source but did not rerun them for this read-only review. The task explicitly assigns the standard benchmark and broad gate to its caller, so those remain verification limits rather than durability findings.

## Proposed findings

None. No reachable change in the reviewed diff makes a telemetry value the durable authority, skips required recovery, or infers a Forge task result from an uncommitted return.
