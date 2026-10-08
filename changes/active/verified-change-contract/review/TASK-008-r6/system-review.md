# TASK-008 round-six system-resilience review

## Immutable subject and scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `1d05642bf2c4d824de1aec27286048ec79b37e25`
- Candidate: `f8d6945041467d52020024d311ce8831a45ff4a1`
- Reviewed range: `1d05642bf2c4d824de1aec27286048ec79b37e25..f8d6945041467d52020024d311ce8831a45ff4a1`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 57
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior verdict and validation: `changes/active/verified-change-contract/review/TASK-008-r5/{verdict.md,findings-validation.md}`
- Remediation: `changes/active/verified-change-contract/review/TASK-008-r5/TASK-008-CLOSEOUT-R4-audit-handoff-and-owner-shape.md`

This is the caller-directed closure review. I reviewed only whether the range
closes `FIND-TASK-008-CLOSEOUT-16` and
`FIND-TASK-008-CLOSEOUT-17`, and whether the range itself introduces a
system-resilience regression. Earlier accepted code was not reopened. The
default `bench:capacity` execution (`FIND-TASK-008-CLOSEOUT-13`) remains
deferred to integration. The separately tracked intermittent
`verification_runtime::two_bindings_share_one_client_observation` failure is
outside this range and is not counted against it.

The candidate remained at the named commit during this review. This checkout
has no `.codegraph/` directory, so navigation used Git, `rg`, and direct source
inspection.

## Deployed paths and topology

### Audit decision to capacity-drain evidence

1. A public Oracle query constructs its read decision and calls the shared
   `OracleAudit` owner after admission
   (`crates/vala/vala-bifrost-redux/src/oracle/mod.rs:2723-2745`). Direct
   verification stages its allow or deny decision through the same
   `AppState::audit_outbox`
   (`crates/wyrd/wyrd-server/src/components/verification/service.rs:365-372,405-415,471-536`).
2. `AppState` shares the Oracle role's `OracleQueryAudit`, or creates exactly
   one process owner when no Oracle role is present
   (`crates/wyrd/wyrd-server/src/state.rs:2241-2266`).
3. `OracleQueryAudit::stage` raises both its atomic pending owner and the
   process-local `audit_outbox_pending` gauge before attempting the bounded
   enqueue. A full or closed queue immediately removes both and records the
   decision as lost (`query_audit.rs:101-116`). Thus a request cannot return
   with successfully queued work that was never represented by the gauge.
4. The writer groups the received batch by tenant, commits every group through
   the canonical `append_audit_batch` path into `vala.audit_staging`, and only
   after the complete received batch has committed or been counted lost lowers
   pending and the gauge (`query_audit.rs:155-214`). Serial processing can
   conservatively retain already committed rows in the gauge while another
   tenant waits; it cannot create a zero gap.
5. The one `AuditPublisher` freezes a per-tenant sequence range, durably
   appends it through local Scribe, and advances the publication watermark in
   settlement; failures leave staging and the frozen range for a later sweep
   (`crates/wyrd/wyrd-server/src/audit/publication.rs:179-209,259-301`).
6. During one capacity step, `Deployment::drain` scrapes every still-serving
   replica before reading Postgres (`capacity/step.rs:383-424`).
   `Backlog::with_replicas` sums `audit_outbox_pending` across replicas, and
   `Queue::backlog` counts every staging row above its tenant's
   `published_seq`, including a row committed after the load-stop timestamp
   (`capacity/evidence.rs:159-200,257-301`). The step accepts zero only when
   both the process-local and durable owners are empty.

This closes both handoff gaps from `FIND-TASK-008-CLOSEOUT-17`: pre-commit work
is visible at its actual process owner, and a post-stop commit remains visible
at the durable owner until publication advances its watermark.

### Benchmark cleanup and process ownership

`Benchmark` still owns the overall measure → client shutdown → replica
shutdown → report sequence. `Benchmark::clean_up` removes the `Deployment`,
shuts clients down under the existing reserve, then calls the inherent
`Deployment::stop_replicas` operation (`capacity/main.rs:379-424,581-605`).
`Deployment`, which owns the replica collection, consumes it newest-first,
executes each synchronous `LocalServer::stop` on Tokio's blocking pool, and
restores results to ordinal order (`capacity/step.rs:427-454`). Each
`LocalServer` retains TERM, grace, KILL, reap, and log-copy ownership
(`crates/wyrd/wyrd-testing/src/release_server.rs:365-405`), while `Drop` kills
and reaps any replica not reached normally (`release_server.rs:487-500`).

The module-level stateful shutdown workflow is deleted. The same blocking and
process behavior is now discoverable on the existing replica-set owner, so
`FIND-TASK-008-CLOSEOUT-16` is closed without introducing another owner or
changing the deployed server.

## Failure and recovery assessment

| Failure or interruption | What stops or remains available | Recovery and proof assessment |
|---|---|---|
| Audit queue full or closed | The request remains non-blocking; that decision is removed from pending and counted/logged as a commit failure. Other requests and server capabilities continue. | `stage` increments before enqueue and reverses both the atomic and gauge on refusal (`query_audit.rs:101-116`), so capacity does not wait forever on a decision already counted lost. |
| Tenant chain-head/commit delay | Requests have returned, but the writer and its process retain ownership; the pending gauge stays nonzero. A delayed tenant batch may conservatively keep other received decisions pending. | The drain scrapes the gauge and therefore cannot report an empty audit backlog. The held-chain-head integration proof drives two real Oracle reads and observes both pending (`capacity/evidence.rs:420-528`). |
| Commit races the drain poll | A scrape before commit sees pending; the subsequent SQL read may also see staging, causing only conservative double counting. A scrape after the decrement occurs only after commit returned, so the SQL read sees the durable row. | The required ordering is explicit in `Deployment::drain` and `OracleAuditWriter::run` (`capacity/step.rs:406-418`; `query_audit.rs:173-178`). The held-handoff proof asserts the cell never falls below the two owned decisions while the gauge drains (`capacity/evidence.rs:530-543`). |
| Decision commits after the captured stop timestamp | The process-local gauge eventually falls, but the row remains above `published_seq`. | Removing only the upper time cut makes the durable query retain the row until publication. The focused proof asserts at least one row has `created_at > stopped`, remains counted, and clears only after `publish_tenant` advances the watermark (`capacity/evidence.rs:544-563`). |
| Audit publication, Scribe, or settlement outage | Staged rows remain above the watermark and the capacity step remains nonzero; the publisher retries on later sweeps without changing the frozen identity. Unrelated serving does not crash. | Publisher cancellation and failure preserve committed staging/frozen range for replay (`publication.rs:179-209,259-301`); the benchmark fails the step after its existing 60-second edge instead of emitting a false PASS. |
| Replica `/metrics` scrape fails | The current capacity step returns an error; it does not silently substitute zero or pass. Other server replicas remain serving until benchmark cleanup. | `LocalServer::metrics` requires a successful HTTP response (`release_server.rs:325-334`) and `Deployment::drain` propagates any scrape error (`capacity/step.rs:388-418`). |
| Server shutdown with pending audit work | Request serving and Bifrost drain first; the shared audit outbox is then asked to finish until the shutdown deadline. A remaining count is warned, rather than silently described as drained. | `WyrdServer::shutdown` calls `audit_outbox.shutdown` after Bifrost shutdown (`crates/wyrd/wyrd-server/src/app/server.rs:853-883`). The new gauge follows the same pending owner and does not change shutdown semantics. |
| Benchmark cleanup while a replica stop blocks | Clients are already stopped. The active stop runs on Tokio's blocking pool, so timers and sibling async tasks remain schedulable. Stops proceed newest-first and results remain ordinal. | The inherent owner preserves the prior `spawn_blocking` boundary (`capacity/step.rs:427-454`); the focused cleanup test drives `Benchmark::clean_up`, checks heartbeat progress, process reaping, log retention, and the result slot (`capacity/main.rs:907-993`). |
| Cleanup future cancellation or a blocking-stop panic | The active blocking closure continues; remaining `LocalServer` values drop and kill/reap their children. A join panic is recorded as that replica's stop error. | The method's cancellation contract matches the ownership visible in the loop and `LocalServer::Drop`; no process is detached from both owners (`capacity/step.rs:438-453`; `release_server.rs:487-500`). The normal binary does not cancel `clean_up`. |

## Affected capabilities

- Production behavior changes only in observability: the shared audit outbox
  now exports its existing current pending count. Authorization remains
  blocking, audit commits remain non-blocking, and the canonical staging and
  publication path is unchanged.
- Oracle reads, tenant/security decisions, and direct verification all benefit
  from accurate process-local audit ownership visibility; their request
  success, refusal, latency, and durability semantics are unchanged.
- The capacity benchmark's audit saturation cell now spans queue/commit and
  staging/publication ownership. A scrape or database failure produces an
  execution error; a real undrained owner produces a 60-second saturation
  failure; neither can become a false PASS through this range.
- Benchmark teardown retains client-before-replica ordering, newest-first
  process shutdown, TERM-to-KILL escalation, reap/log behavior, and report
  alignment. Only the owner shape changed.

## Findings

No material system-resilience findings were found in the reviewed range.

- `FIND-TASK-008-CLOSEOUT-16`: **CLOSED**. Replica-set shutdown is an inherent
  method on `Deployment`; the accepted blocking boundary and process recovery
  behavior remain intact.
- `FIND-TASK-008-CLOSEOUT-17`: **CLOSED**. Process-local pending ownership and
  durable unpublished staging form a conservative, gap-free drain observation
  across the commit handoff.

## Verification limits

- This report is a static source and topology audit. Per orchestrator
  coordination, I did not run Cargo, mise, codegen, or integration commands;
  the review orchestrator owns sequential verification.
- The range includes focused proof for the pending → staging → publication
  path and for cleanup through the inherent owner. Runtime results recorded in
  the remediation artifact were treated as evidence to be independently
  rerun/validated by the orchestrator, not as a substitute for source review.
- The full unmodified default `mise run bench:capacity` remains expressly
  deferred as `FIND-TASK-008-CLOSEOUT-13`; this review makes no empirical
  AC-040/AC-041 capacity claim.
- `verification_runtime::two_bindings_share_one_client_observation` is a known
  intermittent failure in a path untouched by this range. It remains a
  separate integrated-branch blocker and does not change this range verdict.

## Overall result

**PASS**

The remediation closes both assigned findings and introduces no reachable
system-resilience regression in the audit handoff, publication/drain path, or
benchmark process lifecycle.
