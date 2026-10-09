# TASK-003 r4 invariant review

Subject: repository root `/Users/stevenforrester/Documents/GitHub/wyrd-verification-closeout-task3`, base `7f79fb341`, candidate `f6c841d57` (HEAD at review). This review considered the cumulative diff, approved `spec.md` revision 2, original TASK-003, `AGENTS.md`, agent rules, Wyrd/Bifrost design, and the recorded gate results. There is no `.codegraph/` index. The evidence claims `mise run -c gate` exited zero on 2026-10-09; I did not rerun that aggregate.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-006: optional activation, direct and queued canonical results, application Run identity | verification binding and result changes; `VerificationControl::execute` and `stage_result` in `components/verification/service.rs`; Eval judgment projection | server verification journeys and gate recorded in task evidence | PASS |
| REQ-007: declared Service table validation/ensure | `ServiceSpec.tables`; `components/cards/service.rs`; registration changes | cards integration and three support-desk journeys recorded | PASS |
| REQ-007: paired Run/Card gateway correlation authorized before dispatch and captured | `ingress::requested_subject`, `GatewayInvocation::attribute`, `CallCapture::calls_batch`, `PublicWyrdGatewayCaller::with_subject` | ingress unit, gateway Postgres, transport and support-desk tests recorded | PASS |
| REQ-008: Agent and judge use gateway and initiating authority | `Run::invoke`, `PublicWyrdGatewayCaller`, `vala-eval::orchestrator::judge` | gateway and verification journeys recorded | PASS |
| REQ-009: telemetry setup and native Run scope for each SDK | Rust `otel.rs`; Python `otel.py`; TypeScript `otel.ts` and SDK projections | language journeys, codegen and gate recorded | PASS |
| REQ-010 / AC-005: shared support-desk story and correlated MCP explanation | checked-in example and three SDK integration tests | three language journey results recorded | PASS |
| INV-001: tenant and Card attribution isolation | `GatewayInvocation::attribute` checks signed scope UIDs or tenant-local registry using `TenantConn`; Scribe frame carries attributed UID | `gateway_correlation_is_authorized_before_dispatch_and_captured` | PASS |
| INV-002 to INV-008: one nonblocking outbox, stable retries, separate observation queue | `scribe_outbox.rs`, queue and Scribe ingest changes | Scribe 28/28 and gate recorded | PASS |
| AC-004 / scenario 5: late peer route and owned membership tasks | boot selects `ScribeRoute` without Oracle; `ClusterTask` owns heartbeat and poller | late-Scribe and owner-inspection journeys recorded | PASS |
| Scenario 5 recovery: Forge restart reclaims own old unexpired attempts | `ForgeWorker::drain_recoverable_work` passes owner; `ForgeTasks::reclaim_expired_attempts` fences by owner and clears attempt | `previous_owner_reclaims_its_unexpired_attempt` and Forge 22/22 recorded | PASS |
| Scenario 5 recovery: Oracle refuses late plan and drains memory | `GraphLease::settle` and `admit_attempt` share attempt lock; `OracleAdmission::shutdown` waits on admission and pool release | Oracle 50/50 recorded | FAIL: INV-1 below |
| Scenario 6: container `psql` bootstrap | Postgres wrapper and contract test changes | contract, roles and gate recorded | PASS |
| AC-006 and stated non-goals | generated contracts, docs, examples and mise changes; no new gateway alias or task-local publisher in cumulative diff | codegen, formatting, lint and gate recorded | PASS |

## Proposed finding

### INV-1 — INCORRECT: Oracle shutdown now depends on unrelated shared memory

**Violated obligation:** Scenario 5 requires bounded cleanup of the selected roles, and repository process ownership requires each role's final shutdown to complete without making unrelated colocated capabilities' reservations its own residual state. This is a recovery regression in mixed-role deployments.

**Location and source path:** `crates/vala/vala-bifrost-redux/src/oracle/admission.rs:739-767` waits for `self.shared.resources.shared_memory_reserved() == 0` and reports that count as Oracle residual. The method's owner, `OracleEngine::shutdown` (`oracle/mod.rs:2946-2980`), passes that report to `Oracle::shutdown_owner` (`crates/wyrd/wyrd-server/src/state.rs:684-700`), which returns an error if the bytes are nonzero. `BifrostResources::shared_memory_reserved` (`resources.rs:1740-1750`) explicitly counts **Oracle, Forge, and Scribe follower** reservations in one process pool. `Bifrost::drain_selected_owners` (`state.rs:1969-2008`) shuts Oracle before Scribe, using one deadline; on error `Bifrost::shutdown` aborts the remaining selected owners (`state.rs:1948-1958`).

**Producer to sink:** A colocated Scribe follower or Forge consumer can legitimately hold shared pool bytes while Oracle's own query and child tasks are fully drained. The shared counter stays nonzero; Oracle waits until the process deadline and calls that a failed Oracle shutdown. The server then aborts the Scribe owner before its ordered drain, risking accepted Scribe work and availability. The existing Oracle admission wakeup fixes the lost-notification race, but the chosen zero predicate is broader than Oracle's ownership. A healthy Oracle-only journey cannot establish the mixed-role boundary.

**Testable correction:** Keep the two pre-enabled wakeups, but make Oracle shutdown's completion and residual report use Oracle-owned reservations, or otherwise complete the colocated owners in an order that makes the shared-zero predicate valid without consuming the process deadline. Reuse the existing per-query/holder resource accounting in `BifrostResources`; do not create a second memory governor or change the process-wide capacity limit. Prove normal mixed-role shutdown while a non-Oracle reservation is live and Oracle-owned reservations have returned, plus the delayed Oracle-child release that motivated this fix. This review does not prescribe a new public or persistent contract.

## Diagnosis checks and limits

The r4 gateway diagnosis matches the source: correlation moves from a CardRef lookup to the signed UID or tenant registry before dispatch, then the same authorized UID reaches the capture batch. Forge self-reclaim is confined to startup recovery; ordinary expiry scans pass `None`, so it does not reclaim active same-owner attempts during normal operation. Oracle plan settlement and late admission share a lock, closing the described race. The `verify:rust-sdk` filter excludes both identity-owned modules, and `test:identity:journey` remains a dependency. The test-only Oracle cleanup pause now re-arms after a cancelled hold; I found no production effect. The claim that Oracle shutdown waits for its own memory alone is not supported by the shared pool's documented scope.

Overall result: **FAIL** (one proposed material recovery finding). I did not rerun the broad gate; the mixed-role shutdown path needs independent validation.
