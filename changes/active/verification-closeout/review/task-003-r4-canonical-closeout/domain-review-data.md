# Data, concurrency, and durability review — TASK-003 r4

**Subject:** `7f79fb341..f6c841d57` at `f6c841d57fb19517ddefe83c826b24085b853845`. **Result: FAIL.**

## Authority and source coverage

| Boundary | Authority | Source traced |
|---|---|---|
| In-memory evidence delivery, retries, and tenant slices | `AGENTS.md` §§2, 9, 11; `architecture/agent-rules.md`; `architecture/wyrd-design.md` audit and verification sections; `architecture/bifrost-design.md` consistency and audit sections; approved `spec.md` REQ-004, REQ-006, INV-002–008 | `wyrd_runtime::outbox`, `scribe_outbox.rs` frames, identity, route and peer submission, `boot/mod.rs` route selection, `app/server.rs` drain, Scribe journeys and task evidence |
| Membership and process lifetime | `architecture/bifrost-design.md` role fencing and shutdown; `architecture/operations/reliability-and-recovery.md` | `cluster/mod.rs` `ClusterTask`, heartbeat, poller; `boot/mod.rs`; `state.rs` role and process drain; owner inspection journey |
| Forge durable attempt recovery and publication safety | `architecture/bifrost-design.md` Forge scheduling/fencing; `architecture/references/domain/analytical-operations-reliability.md` maintenance protocols; `architecture/operations/reliability-and-recovery.md` | `forge_tasks.rs` reclaim, `forge/worker.rs` claim/runner/heartbeat/join, `app/supervise.rs` restart, `app/server.rs` worker supervision, `pg_forge_tasks.rs`, Forge restart journey |
| Oracle admission, graph settlement, and memory | `architecture/bifrost-design.md` Oracle cancellation/resource sections; `architecture/references/domain/analytical-operations-reliability.md` | `oracle/admission.rs` shutdown, `resources.rs` shrink notification, `oracle/analytical.rs` graph settlement and test pause, Oracle journeys |

The cumulative diff was the review subject; the r4 diff identified the changed recovery owners. SQL reclaim uses the existing `OperatorPool`; tenant-scoped cluster operations use `TenantConn`. The stable outbox batch identity includes tenant, destination, attribution, request and encoded rows; `OutboxWriter` retains a failed slice separately from later arrivals. The Oracle shutdown registers both notifications before checking active queries and shared bytes. The graph settlement and late admission check use the same attempt lock. These paths show no additional material issue in this domain.

## Material proposed finding

### DATA-1 — Immediate self-reclaim can fence a still-running plan after a worker panic (`REGRESSION`)

**Obligation:** Forge may reclaim its own unexpired attempt only when the preceding incarnation has ceased physical work. Publication and durable attempt ownership must stay fenced through failure and recovery (`architecture/bifrost-design.md` Forge scheduling/fences; task r4 restart recovery).

**Location and path:** `crates/vala/vala-sql/src/queries/forge_tasks.rs:890–900` now selects every `claimed_by = previous_owner` Claimed/Running row before lease expiry. `crates/vala/vala-bifrost-redux/src/forge/worker.rs:2188–2191` passes `Some(self.owner)` during startup recovery. This assumes the prior loop joined its tasks. A normal return does join in `run_event_loop` (`worker.rs:2427–2440`), but a panic bypasses that code. Plan runners are spawned without retained abort-on-drop handles (`worker.rs:6140–6176`); their `JoinHandle`s are discarded, so dropping the worker loop detaches them. The claim heartbeat is likewise an ordinary spawned handle (`worker.rs:8319–8350`) held in the dropped attempt pool. `app/supervise.rs:119–180` restarts after **any** joined worker outcome, including `JoinError`, with the same node/owner ID supplied by `boot/mod.rs:1034–1040`. The replacement then clears the old attempt ID while its detached plan and heartbeat can still execute.

**Observable consequence:** After a worker-task panic during a rewrite, the replacement can reclaim an unexpired attempt and become ready while the prior plan remains in flight. Its publication may race recovery or a successor attempt under a fence that the old plan has not cooperatively observed. This defeats the physical-work-drained premise used to justify reclaim before lease expiry. The existing `previous_owner_reclaims_its_unexpired_attempt` SQL test proves the predicate but does not exercise supervisor panic and child-task lifetime; `failed_worker_restarts_while_the_api_serves` uses an orderly error after the plan has drained.

**Testable correction:** The supervisor and worker lifetime boundary must establish that the previous incarnation's plan runners and claim heartbeats have stopped before passing its owner to unexpired reclaim. Preserve ordinary fast self-reclaim after a joined loop. If the worker exits abnormally, do not treat the owner ID alone as proof that physical work stopped; keep the prior lease/fence recovery semantics until its work is actually quiescent. Add a focused restart test that interrupts the worker with a plan in flight and proves the successor cannot reclaim that live attempt early or publish concurrently.

## Verification limits

The task records `mise run -c gate` exit 0 on 2026-10-09, Scribe 28/28, Forge 22/22, Oracle 50/50, codegen, format, lints and diff check. I inspected the relevant test bodies and their reach. I did not rerun lanes. The available Forge restart journey covers a returned worker error, not a panicked loop with detached child tasks. This review makes no finding about the spec-approved best-effort loss on hard process death or expired outbox shutdown.
