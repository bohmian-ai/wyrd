# Concurrency and audit-handoff domain review

## Immutable subject and reviewed boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `1d05642bf2c4d824de1aec27286048ec79b37e25`
- Candidate: `f8d6945041467d52020024d311ce8831a45ff4a1`
- Range: `1d05642bf2c4d824de1aec27286048ec79b37e25..f8d6945041467d52020024d311ce8831a45ff4a1`
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 57, especially REQ-168 and REQ-171
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior finding and remediation authority: `review/TASK-008-r5/findings-validation.md` and `review/TASK-008-r5/TASK-008-CLOSEOUT-R4-audit-handoff-and-owner-shape.md`

This domain pass is intentionally limited to the concurrency and durability handoff in `FIND-TASK-008-CLOSEOUT-17` and concurrency regressions introduced by the immutable range. Earlier accepted code was not reopened. `FIND-TASK-008-CLOSEOUT-13` remains deferred, and the separately tracked intermittent `verification_runtime::two_bindings_share_one_client_observation` failure is outside this range and is not charged to it.

The candidate remained at the stated commit during this review. The checkout has no `.codegraph/` directory, so source navigation used Git, `rg`, and direct source inspection.

## Authority and source coverage

| Boundary | Authority | Source and consumer evidence | Result |
|---|---|---|---|
| Non-blocking pending ownership | AGENTS.md audit rules; `architecture/agent-rules.md`; REQ-168; remediation AC-R4-2 | `crates/wyrd/wyrd-server/src/oracle/query_audit.rs:53-178`; shared construction in `crates/wyrd/wyrd-server/src/state.rs:2241-2266`; shutdown consumer in `crates/wyrd/wyrd-server/src/app/server.rs:876-883` | PASS |
| Pending-to-staging handoff | REQ-171; remediation diagnosis and recommendation at `TASK-008-CLOSEOUT-R4...md:54-123`; AC-R4-2/3 | `crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:182-299`; `crates/wyrd/wyrd-testing/src/bin/capacity/step.rs:383-423` | PASS |
| Gauge recorder and scrape semantics | Remediation AC-R4-2; installed metrics transport | `crates/wyrd/wyrd-server/src/app/metrics.rs:220-257`; `crates/wyrd/wyrd-testing/src/release_server.rs:522-555`; locked `metrics` 0.24.6 and `metrics-exporter-prometheus` 0.16.2 in `Cargo.lock:5534-5552` | PASS |
| Durable publication ownership | AGENTS.md one-audit-path rule; `architecture/bifrost-design.md:587-598`; remediation AC-R4-3 | `Queue::backlog` at `capacity/evidence.rs:257-301`; the existing `vala.audit_staging` / `audit_chain_head.published_seq` ownership remains unchanged | PASS |
| Focused reachable proof | Remediation focused-proof requirement at `TASK-008-CLOSEOUT-R4...md:199-213` | `capacity/evidence.rs:354-418,420-588` | PASS, subject to execution limits below |

## Concurrency trace

### Producer and metric ordering

`OracleQueryAudit::stage` increments both the existing atomic pending owner and the process-local gauge before `try_send` (`query_audit.rs:107-116`). The receiver cannot observe a successfully queued event before the gauge increment, because enqueue occurs afterward. A full or closed queue reverses both increments synchronously and records the loss before `stage` returns, so no request-completed decision is left uncommitted and uncounted while the gauge reads zero.

The writer drains at most 1,024 events, commits or counts each tenant group lost, and only after the complete batch finishes decrements the atomic and gauge by the received count (`query_audit.rs:162-178,182-216`). Earlier tenant groups can therefore remain over-counted while later groups commit, but they cannot be under-counted. Tokio cancellation cannot interleave between the adjacent atomic and gauge operations because neither operation awaits. A commit failure is counted inside `commit_batch` before the writer releases the batch from pending ownership.

The Prometheus recorder stores gauge updates atomically, renders the current process snapshot, and `Metrics::sum` reads the exact family or all matching labeled series. The gauge is intentionally unlabeled and process-local; every release replica has its own recorder and `/metrics` endpoint, so summing the family across replica scrapes produces the deployment total without collapsing replica-local writers inside one process.

### Scrape-to-SQL handoff

`Deployment::drain` scrapes every replica before it reads the durable queue (`capacity/step.rs:406-418`). The transition cases are safe:

1. If a decision is still queued or committing when its replica is scraped, the scrape counts it. A later commit or even publication before the SQL read can only make that poll over-count.
2. If its gauge has already decremented when scraped, `conn.commit().await` has completed first, so the later SQL snapshot can see the staged row unless publication has already advanced past it. In that latter case the work is genuinely drained.
3. If commit occurs between the scrape and SQL read, both owners may be counted for that poll. This is conservative and cannot produce a false zero.

Sequential scrapes do not open a multi-replica omission: all workload request futures are joined before drain begins, and each request stages its decision synchronously before returning. No step-owned decision can first appear after its own replica was scraped. Each replica is independently sampled, then the shared durable state is read once.

### Durable side of the handoff

`Queue::backlog` now counts every `vala.audit_staging` row above its tenant publication watermark and no longer applies `created_at <= stopped` (`capacity/evidence.rs:257-299`). A decision committed after the captured stop timestamp therefore remains visible until `published_seq` advances past it. This keeps the existing staging table and watermark as the only durable owners and does not change publication, batching, tenant serialization, or loss semantics.

The broader query still uses `stopped` only for the unchanged Forge demand boundary and `since` only for the unchanged run boundary. Scribe remains sourced from the replica metrics. The candidate therefore changes only the missing audit handoff and introduces no concurrency regression in adjacent backlog cells.

### Cancellation and shutdown

The new gauge does not create a task, lock, queue, wait, or shutdown dependency. If the audit writer is still draining at server shutdown, the existing `OracleQueryAudit::shutdown` continues to wait on the same `TaskTracker` and report the atomic pending owner. A pending or failed writer remains visible to capacity drain; a dead serving replica makes its `/metrics` scrape fail rather than falsely returning zero. The range does not alter request latency or make authorization audit commits blocking.

## Focused proof assessment

`the_audit_backlog_holds_from_a_pending_decision_until_its_publication` exercises the real public Oracle request, shared audit writer, tenant chain-head lock, Prometheus recorder, Postgres staging rows, and `AuditPublisher` (`capacity/evidence.rs:420-565`). It establishes the reachable pre-commit state after two requests return, checks that pending evidence holds the audit cell at two, releases the lock, requires every observed transition poll to remain at least two, proves at least one staged row has `created_at > stopped`, and reaches zero only after publication. `pending_decisions_add_to_staged_audit_rows` separately proves summation across two replica-shaped scrapes (`capacity/evidence.rs:568-588`).

The held-lock test's `pg_stat_activity` probe is broad, but it is not the proof's only reachability evidence: the subsequent exact `pending == 2`, audit-cell value, post-stop row, and publication-to-zero assertions bind the observed behavior to this writer and tenant. No material false-PASS path was found.

## Review findings

No Critical, Important, or Suggestion findings are proposed within the user-directed closure scope. Static source tracing supports closure of `FIND-TASK-008-CLOSEOUT-17`; the range introduces no concurrency or audit-handoff regression.

## Verification limits

- Per orchestrator direction, this reviewer ran no Cargo, mise, codegen, or integration lane. Runtime verification is left to the orchestrator's sequential pass.
- Static `git diff --check` for the immutable range passed.
- The focused held-chain-head test is present and reaches the required real boundaries, but this reviewer did not execute it.
- The default `bench:capacity` run and empirical AC-040/AC-041 qualification remain deferred under `FIND-TASK-008-CLOSEOUT-13`.
- The known intermittent `verification_runtime::two_bindings_share_one_client_observation` failure remains a separate integrated-branch blocker; this range does not touch its Oracle scan, live-route, or Drift-fold path.

## Overall result

**PASS**

`FIND-TASK-008-CLOSEOUT-17` is closed for both the process-local pre-commit interval and the durable post-commit interval. The atomic/gauge ordering, batch decrement point, scrape-before-SQL ordering, multi-replica aggregation, full/closed queue behavior, publication watermark, cancellation, and shutdown paths do not expose a reachable false-zero drain result in the reviewed range.
