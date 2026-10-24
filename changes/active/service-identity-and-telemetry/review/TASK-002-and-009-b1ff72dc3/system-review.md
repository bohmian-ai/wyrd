# System resilience review

Result: **FAIL**. Two context omissions remain at the accepted-work boundary. Neither correction requires a new queue, auth cache, telemetry pipeline, or wider failure boundary.

## Subject and authority

Base `036fc48fe26fb87648a277b10099bc475d293b92`; candidate `b1ff72dc34c344b596c85cf408e6a9217685be35`. HEAD remained the candidate at the final source check. Reviewed the cumulative TASK-002/TASK-009 range against approved spec revision 15, both tasks, auth/storage supplements, root and applicable SQL/Python/TypeScript rules, spec-driven development, maintainer guidance, and owning security/deployment/recovery authority. CodeGraph was the initial code-navigation step, followed by cumulative diff and targeted source/caller reads. No other reviewer conclusions were read.

TASK-003 warehouse columns, physical projection and canonical row readback are deferred; their absence is not a finding. No build, generator, test, source edit, commit or deployment was performed.

## Deployed paths and failure boundaries

The topology is one logical `wyrd-server`, potentially replicas/role-targeted pods, backed by durable Postgres/object storage and persistent Scribe volumes (`architecture/operations/deployment-and-release.md:7–19,128–156`). Cache loss during replacement is expected. Dependency uncertainty denies the affected operation (`reliability-and-recovery.md:224–235`), without granting permission to terminate unrelated capabilities.

| Runtime path | Source and failure/recovery evidence |
| --- | --- |
| State, root and auth cache | `wyrd-client/src/state.rs:763–788` pins one state's immutable root/environment. `auth.rs:841–913` derives middleware with an independent memory cache and no rooted disk-cache reuse. Exchange retains the root (`:1170–1228`), including actor-root delegation. SQL/signing failures are returned by `wyrd-auth/src/issuance.rs:616–670`; store failure maps to auth unavailable (`:518–524`). The operation fails; the shared server remains running. Recovery permits another exchange. |
| Signed bearer and optional selector | `components/auth/token_extract.rs:169–180` verifies synchronously. `wyrd-auth/src/issuance.rs:397–426` resolves only a rootless selector under signed access. Equal/different signed selectors are decided by `wyrd-runtime/src/request_context.rs:107–118`. SQL outage therefore refuses a selector that needs SQL, rather than broadening authority; already signed context uses bounded expiry semantics. |
| Gateway cache and shared invocation | `components/gateway/api_keys.rs:95–161` keys reuse by full credential plus root, verifies before caching, never caches a failed exchange, and bounds reuse from exchange start. Cold/replaced replicas exchange again. `components/gateway/invocation.rs` retains the existing validation/admission/execution owner. A request refusal cannot relabel another release or crash unrelated serving. |
| Submission, Scribe and queue | `components/verification/service.rs:665–768` reconciles environment before staging, snapshots accepted authority, and reserves both outboxes. `verification/submissions.rs:184–233` resolves readiness and commits accepted submissions in one tenant transaction. Its existing outbox retries SQL failures; submission identity/idempotency handles uncertain commits. In-memory loss before durable Scribe acknowledgement remains the established limit. Once queued, Postgres and lease fencing own retry/reclaim. |
| Scheduling and account inactivity | `queries/verification.rs:60–84,106–135` records activity per exact root/account and reads the current associated family account. `verification/scheduler.rs:325–334,483–494` and `authority.rs:252–291` resolve exact-root fresh authority and refuse missing/inactive/unavailable authority. SQL failure rolls back. Another release's activity does not keep this one alive; SYSTEM adds no grants. Existing cursor/lease owners resume work without environment fanout. |
| Worker failure, restart and shutdown | `verification/mod.rs:342–393` supervises individual capability tasks, degrades the affected slot, and restarts with backoff. Aborting it leaves claims reclaimable after lease expiry; graceful drain releases unfinished work. `runner.rs:453` restores `Caller::accepted`; `:861–864` uses the accepted owning root. This preserves authority/root without a live account lookup, subject to the omissions below. |
| Provider SDK timeout/cancellation/retry | Python `_http_auth.py:82–146` and TS `index.ts:3228–3263` snapshot state-specific Run headers before token await and refuse conflicts before dispatch. Request headers are copied. Rust HTTP/gRPC attach immutable middleware context on each attempt (`transport/http.rs:877–888`, `bifrost/grpc.rs:292–314`). Token failure stops that request. Provider SDK retries within the active Run re-enter the same adapter context. Python `to_thread` cancellation can leave the shared token operation finishing in its thread, but does not dispatch the cancelled HTTP request. |
| Global telemetry ownership | Rust SDK `otel.rs:108–155`, Python `otel.py:311–343`, TS `otel.ts:115–135` refuse an already installed provider and release a losing new provider. A second state's automatic startup fails locally, rather than replacing application telemetry or exporting silently as another Service. Exporters reuse client auth renewal; existing batching/flush/drain bounds govern delivery failures. |

TASK-002 explicitly selects a coordinated breaking fresh baseline. This report does not demand old/new auth-schema compatibility during overlapping migration. Production rollout still follows the drain/contract boundary in `deployment-and-release.md:193–198`.

## Proposed findings

### SYS-001 — Exact bound occurrence is discarded before acceptance

Classification: **MISSING**. Violates spec REQ-015/REQ-052, INV-004, and TASK-009 approach step 4/Scenario 3: the selected binding occurrence must survive direct/submitted work, retries and restart.

Locations: `crates/shared/wyrd-client/src/state.rs:410–417,886–939`; `crates/wyrd-spec/src/verification/execution.rs:40–68`; `crates/wyrd/wyrd-server/src/verification/cache.rs:168–203`; `crates/wyrd/wyrd-server/src/components/verification/service.rs:707–721,739,800`; `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs:104–132`.

`bound_verifier` gathers bindings, finds the first matching Verifier, and returns only Verifier UID, subject UID and spec. Observe's direct/submitted target (`observe/mod.rs:505–520`) carries no occurrence. No occurrence selector crosses the wire. The server instead collects every owner/component/Agent binding matching subject and Verifier into a set of environments. Both explicit staging and the manual queue insert retain no binding ID. Scheduled/group paths already have the existing distinct binding ID; the explicit paths cannot restore information discarded upstream.

Consequence: two component occurrences referencing the same Agent and Verifier collapse into one request. Different occurrence environments are rejected collectively by `effective_environment`, even when the caller selected a legitimate occurrence. Equal environments still leave the accepted execution unable to distinguish the occurrences. Retry/restart preserves that absence. This is not only a deferred warehouse field.

Smallest correction: preserve the binding selected by existing state/Run resolution through the typed invocation and server validation into accepted input/queue context. Reuse existing server binding projection/identity, and validate against authenticated owner, subject and Verifier before acceptance. Preserve permitted truly unbound explicit work. Prove two same-subject/same-Verifier occurrences remain distinguishable, including different environment selectors and claim after restart; keep scheduled/group lifecycle unchanged.

### SYS-002 — Frozen environment is read back as status but not restored to execution

Classification: **MISSING**. Violates TASK-009's accepted environment/origin handoff through gateway calls, queues and restart; spec REQ-052/REQ-053 and the storage supplement's environment-of-evaluated-evidence rule.

Locations: `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs:411–426,1324–1372,2934–3072`; `crates/wyrd/wyrd-server/src/verification/runner.rs:418–472`; `crates/wyrd/wyrd-server/src/verification/hosted/blocks.rs:62–79`. Direct sibling: `components/verification/service.rs:513–549`.

Admission computes an effective environment and manual INSERT persists it (`verifier_executions.rs:111,121,1805`). But `CLAIM_SQL` does not return it; `ClaimedRow`, `ClaimedExecution` and `ExecutionInput` have no field for it. Runner builds its engine without it. `GatewayCallSubject` contains only Run and Card (`components/gateway/invocation.rs:83–91`). Direct execution likewise computes environment for its result but does not pass it in `Execution`. Scheduled/group enqueue owners also do not carry the binding selector into runnable context. The omission occurs before TASK-003's table projection.

Consequence: work accepted with `prod` resumes with correct account/root/subject, but its actual executing checks/gateway calls have no originating environment available. Status can nevertheless return `prod`, because status SELECT independently reads the column. That is not restoration of execution context. Server-read population processing cannot receive the accepted selector through this boundary.

Smallest correction: carry the computed selector through existing accepted execution/claim/engine/origin context for direct, submitted, scheduled and grouped paths. Never substitute worker environment, change auth, filter whole-trace fetches, add schedule fanout, or introduce another queue. TASK-003 can retain ownership of final table projection. Prove a real engine/gateway-backed execution observes its environment after lease retry/restart and that the binding selector reaches the applicable population reader before evaluation, retaining unscoped/whole-trace rules.

## Proof assessment

Recorded TASK-002 evidence covers root-specific renewal/refusal, family-root activity, delegated snapshot/restart and federation/browser regressions. TASK-009 reports named server/SDK/MCP/provider journeys and static/language gates. Source supports the healthy failure-isolation boundaries above, but green counts do not close these omissions.

`pg_verification_runtime::configured_service_context_survives_acceptance_and_restart` (`:4546–4750`) uses `EngineScript`, which returns before actual dispatch (`runner.rs:418–423`). It checks persisted environment/subject and API result; it does not check occurrence identity or real worker-to-engine/gateway environment. This proves queue/restart/status behavior, not the execution-origin handoff. Application provider journeys exercise a different caller path.

No additional outage-hardening findings are proposed. Existing bounded caches, auth refusal, transaction rollback, outbox retry, supervisor restart and lease fencing are reused. No reviewed operation-level refusal justifies crashing the shared server.

Skipped execution of tests, outage injection and rolling replacement; this is a source audit plus assessment of recorded evidence. Risk: scripted restart proof leaves the actual accepted-context handoff unproven and the two source-backed omissions open.
