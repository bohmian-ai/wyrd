# System resilience review — TASK-005 and TASK-006

Subject: `/home/thorrester/Documents/GitHub/wyrd-worktrees/resource-scaling`, immutable `0c81715f2...180879328ac9dde94d7504855e821980e5db0f18`.
Role: independent system-rev. Result: **FAIL**.

## Scope and authority

Read the shared navigation/selection map, both original tasks (TASK-005 revision 6; TASK-006 revision 7), approved spec requirements/invariants, root rules, and operations authority. This review concentrates on the deployed Gateway owner, new memory-owned accounting, background configuration/usage refresh, cancellation, SQL interruption and shutdown. Other reviewers cover public detail/SDK parity, repository standards and general performance acceptance. No prior review evidence is carried forward.

Applicable authority: spec REQ-028–031 and AC-023/024/026, INV-001–003; TASK-006 “What Changes and What Must Stay True”, failure/recovery and background-accounting scenarios; `architecture/operations/deployment-and-release.md:272` (stop admission and settle accepted work under its owner); operations README subsystem readiness and accepted-work recovery. Approximate cross-server enforcement and loss after abrupt death/expired shutdown deadline are accepted. Neither strict global quotas nor crash-safe local storage is required.

CodeGraph was used first for owners/callers, followed by targeted source where exploration omitted bodies. Inspected current source and actual diff for `components/gateway/{owner,usage,ledger,snapshots,invocation,service}.rs`, SQL `queries/gateway_admission.rs`, app shutdown orchestration, related snapshot/usage/PG tests and Gateway resilience journeys. Read the notification migration to establish the producer of per-tenant invalidations. The reviewed source was not edited.

## Deployed and failure paths

| Path | Owner and concrete evidence | Assessment |
|---|---|---|
| Buffered call | `invocation.rs:543–642` checks draining, cached policy, authorization, route, capture slot, then local admission. `CallTask::settle_buffered:1300–1357` settles in memory before answer; capture follows answer. | No SQL accounting wait on answer; capture ownership remains tracked. |
| Stream/caller disappearance | `CallTask::settle_stream:1373–1457` sends headers, awaits stream end, then settles/captures. `ledger.rs:85–92,466–475` takes charge ownership once; drop releases concurrency while retaining bounded exposure. | Existing tracked completion remains; a second settlement/drop cannot release twice through Admission. |
| SQL failure/uncertain commit during live runtime | `usage.rs:553–602` serializes per tenant, keeps reservation entries until commit, applies only successful committed views, marks dirty on error. SQL writes absolute per-server windows/concurrency and fenced ledger entries. `run:625–634` periodically retries due tenants. | Existing mechanism supports retries without adding duplicate deltas. Shared usage older than 30 seconds causes covered-work 503 (`admit:324–337`). |
| Cold/restarted process | New process gets random server identity and no shared baseline. `admit:324–331` wakes background loading and refuses covered calls until first committed view. SQL views include other server contributions and unexpired concurrency. | Committed baseline loads asynchronously. Unflushed crash loss is explicitly accepted. |
| Configuration listener failure | `snapshots.rs:259–267` delegates to SQL listener health/reset; reset clears cached policy (`230–241`). | Loss of listener health denies cached admission. This does **not** prove successful refresh of each tenant after notifications; see SYS-001. |
| Configuration reload failure or invalidation race | Reload queue drained before IO; reload failure/fenced installation returns false; caller drops false. Old cached policy remains with no age or pending-health check. | FAIL, SYS-001. |
| Graceful shutdown | `app/server.rs:756–804` cancels/drains supervised workers then calls Gateway drain with shared deadline. `owner.rs:168–179` waits tracked calls then flushes usage. | Flush errors return before deadline without retry and do not affect drain result; FAIL, SYS-002. |

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence and limits | Result |
|---|---|---|---|
| REQ-028; AC-023 — answer before accounting SQL | Local `GatewayLedger::settle`; buffered/stream handoff above | Existing PG `warm_governance_uses_local_reservations_without_database_waits`, Gateway `gateway_cached_governance_answers_before_background_accounting` source; prior reported pass only, not rerun | PASS |
| REQ-029 — approximate global enforcement | Per-server usage contributions and background shared view; random process identity | PG two-server reconciliation source | PASS |
| REQ-030; AC-024 — cold/stale usage refuses and recovers in background | `usage.rs:324–337,553–634` | Existing `usage_cache_bounds_staleness_restarts_and_policy_changes`; source confirms baseline gating | PASS for usage health |
| REQ-030; INV-001 — policy refresh/recovery and authority remain usable and current under existing fencing | `snapshots.rs:125–166,199–225`; `owner.rs:141–145` | Current snapshot unit test checks stale insert rejection but does not consume A's queued reload before a different tenant invalidates it | FAIL, SYS-001 |
| REQ-031 — retain pending usage across failed/uncertain live writes | `usage.rs:451–602`; absolute rows/fenced entries in SQL | `background_usage_reconciles_retries_and_remote_updates:5645–5841` exercises refused persistence, retry/replay, two servers, timeout concurrency release | PASS outside shutdown |
| REQ-031; AC-024/026 — bounded accepted-work drain, loss only crash/expired deadline | `owner.rs:168–179`, `usage.rs:607–640`, app shutdown | PG helper `drain_gateway:3484–3490` directly calls a one-shot persist and does not exercise production `Gateway::drain` retry behavior | FAIL, SYS-002 |
| INV-002/003 — capture reservation, cancellation and tracked ownership preserved | `invocation.rs:614–640,1236–1457`; Admission drop/settle | Existing cancellation/stream/timeout journey and PG source, prior reported passing runs | PASS |
| AC-026 — bounded reuse of process owners, no crash-safe ledger/new service | Gateway extends existing refresh worker and task tracker; `SYNC_CONCURRENCY=8`, bounded retained tenant cache/pending entries | Source plus existing tests; exact buffer accounting is delegated to invariant specialist | PASS within reviewed system scope |

## Proposed material findings

### SYS-001 — A discarded configuration reload can leave superseded policy serving indefinitely

Classification: **INCORRECT / REGRESSION**, TASK-006. Violated obligation: REQ-030 background recovery and generation fencing; INV-001 preserves governance, credential and capture semantics except explicitly approximate accounting. Location: `crates/wyrd/wyrd-server/src/components/gateway/snapshots.rs:153–166,199–225`; consumer `owner.rs:141–145`.

Producer-to-consumer trace:

1. Server replica R already serves tenant A's old cached configuration. Another replica commits a normal A configuration change; SQL trigger `20261023000000_gateway_snapshot_notify.sql:9–37` emits A's tenant payload.
2. `invalidate(A)` increments the **global** generation and queues A but retains its old snapshot. `reloads()` drains A out of the queue before IO (`139–145`). The worker starts loading A using generation g.
3. A normal configuration commit for tenant B arrives before A installs. `invalidate(B)` advances the same global generation but queues **only B** (`212–217`). A's `insert` is rejected (`199–203`). `reload` returns false, which the worker ignores. A has no queued obligation left.
4. Subsequent A inference (`invocation.rs:571–579`), model-listing and batch snapshot reads (`service.rs:628–638`) return A's retained old snapshot. There is no policy timestamp or pending-refresh health check. Periodic usage refresh changes only usage, so it cannot repair policy. A listener that stays healthy never clears this snapshot. No further A change or eviction is required in supported use, so old policy can persist indefinitely.

A transient SQL failure at `reload:162–166` has the same lost-obligation cause: log and false, no requeue or cached-policy invalidation. This is distinct from usage staleness and listener health. A usage sync may continue successfully, and an old no-quota policy bypasses usage entirely (`ledger.rs:410–411`). Approximate visibility allows delay; it does not authorize permanently ignoring a committed quota, deployment/credential or capture-policy edit. Local admin `warm` retries help only the writing server, not R.

Consequence/materiality: a multi-tenant replica can permanently ignore a supported administrative policy update, continuing provider dispatch/capture behavior under superseded configuration even though listener and SQL usage refresh are healthy. This is a normal concurrent-admin/reload failure path, not unusual caller input.

Smallest testable correction boundary: fix `GatewaySnapshots`' background refresh ownership, not inference callers or SQL usage. Preserve a tenant's outstanding reload until successful current-generation installation; failed or fenced attempts must remain recoverable by bounded background work. Do not serve a known unusable policy indefinitely: reuse the existing cache-miss/503 path when a refresh cannot establish a usable snapshot. Existing invalidation eviction is a smaller safe alternative to adding a new stale-policy cache/TTL framework; retain async reload and existing listener fencing. No synchronous fallback, new service, strict quota coordination or new public error is needed. Both global-fence rejection and transient load failure must be covered as one cause.

Focused closure proof: in the existing snapshot tests consume A's queued reload, capture its generation, invalidate B, reject A's installation, then show A remains scheduled or becomes unavailable and recovers to its new snapshot without another A edit or request-triggered synchronous IO. Add a transient reload-error recovery case at the existing PG seam and verify old policy cannot be served indefinitely. Existing unit test's ordering (`snapshots.rs:315–322`) leaves A still queued, so it misses this path.

### SYS-002 — Graceful drain drops failed usage persistence before its deadline

Classification: **INCORRECT**, TASK-006. Violated obligation: REQ-031 permits loss on abrupt process death or **expired shutdown deadline**, requires accepted usage retained until committed or that approved boundary; AC-024/026 require accepted-work drain. Location: `crates/wyrd/wyrd-server/src/components/gateway/usage.rs:607–640` and `owner.rs:168–179`, reached by `app/server.rs:800–804`.

Producer-to-consumer trace:

1. A quota-covered call answers after memory settlement, leaving window/reservation data awaiting persistence. Shutdown cancels the refresh worker (`owner.rs:148–152`) and stops new call admission. Gateway drain waits for all call tasks.
2. `flush` calls `sync_where` **once**. For a quick transient SQL/commit failure, `sync` retains/marks the pending usage dirty (`596–600`), but `sync_where` logs it and returns unit (`615–619`). There is no background worker left to retry.
3. `timeout_at(deadline, flush)` is `Ok(())` immediately, so even the timeout warning is omitted. `drain` returns `drained` from call tasks alone. The server continues to exit and drops pending memory even if most of the drain budget remains and Postgres has recovered.

Consequence/materiality: ordinary rolling shutdown loses accepted usage without abrupt death or an expired deadline. Committed reservations may remain overreserved, or new window/cost contributions can disappear. This is not the explicitly accepted crash-loss case. The existing reconciliation test deliberately demonstrates the first failed one-shot persist retaining state (`pg_invocation_tests.rs:5675–5731`), but recovery there is an explicit later `persist`; production shutdown never makes that later attempt.

Smallest correction: keep final usage synchronization in the existing Gateway drain owner, retry only still-uncommitted contribution work while the existing deadline remains, and stop once pending usage/releases are committed. Reuse `GatewayUsage::sync` and its replay-safe entries/absolute rows. Do not require memory to become wholly empty (`clean` includes already committed active-window totals), add a durable spool, extend the deadline, or delay unrelated sibling lifecycles beyond their existing shared budget. Drain completion must accurately reflect pending accounting at deadline; accepted deadline loss remains permitted and observable. A bool-only reporting change without attempting recovery does not close the early-loss path.

Focused closure proof: exercise actual `Gateway::drain` with pending real-PG usage, cause its first flush to fail promptly, restore writes before the deadline, and assert drain persists each contribution once rather than returning after that first failure. A second case keeps failure through the deadline and establishes bounded return/loss reporting. Reuse the existing PG fixture/fault mechanism; no full journey suite/new harness is necessary.

## Verification limits

Static source/diff review only; no tests, release server, capacity workload, CPU/heap profiling or broad gate was run. Prior task/user test claims are recorded as prior evidence, not independently rerun. No stronger availability or strict cross-server quota promise is inferred. The two findings require owner-local recovery corrections under approved behavior, not a new spec decision.
