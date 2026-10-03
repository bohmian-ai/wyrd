# System-resilience review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-aad682fbca5074900`
- Base: `ce5c09ef3b559c35e65d6a3e73beebe0a69ae5bd`
- Candidate: `f451d52be01acba66bae003b8509c95086fc8558`
- Approved specification: `changes/active/audit-outbox/spec.md`, revision 1
- Tasks: `01-publication-progress.md`, `02-one-outbox.md`, and `03-remove-audit-unavailable.md`
- Additional approved repository principle supplied for this review: high-throughput acknowledgements mean receipt rather than guaranteed durability; audit uses a batched server-owned outbox that retries dependency slowness, flushes on graceful shutdown, and may lose unflushed work on a hard process kill. The accepted acknowledgement and hard-kill windows are not findings below.

`HEAD` was the candidate commit when this report was written. The complete base-to-candidate name/status diff and the runtime owners and callers named below were inspected.

## Deployed path evidence

The candidate composes one `AuditOutbox` in `compose_bifrost` and stores the same `Arc` in `Bifrost` and `AppState` (`crates/wyrd/wyrd-server/src/boot/mod.rs:1021-1131`, `crates/wyrd/wyrd-server/src/state.rs:1600-1639,2174-2215`). Gate, Oracle, peer security, `wyrd-auth`, HTTP/MCP/gRPC routes, gateway, verification, catalog, and platform decisions enqueue on that owner. A production-source search found the canonical `append_audit_events` caller only in `crates/vala/vala-sql/src/audit_outbox.rs:296-301`; the other public append helpers are test-support only.

The writer accepts synchronously into a process-local bounded count, drains the channel into per-tenant waiting vectors, and starts at most four tenant commits with no more than one commit per tenant (`crates/vala/vala-sql/src/audit_outbox.rs:40-54,111-126,195-289`). Each tenant commit opens its own `TenantConn`, takes the tenant chain-head lock through the canonical append, and commits once (`crates/vala/vala-sql/src/audit_outbox.rs:292-309`; `crates/vala/vala-sql/src/queries/audit_staging.rs:70-180`). This preserves RLS tenant binding and committed-chain order across replicas.

The retained-history publisher runs only in a process with a local Scribe and an operator directory. It lists active tenants, freezes or reuses a per-tenant range, sends the derived stable batch through Scribe, and settles only after durable acceptance (`crates/wyrd/wyrd-server/src/audit/publication.rs:113-139,179-286,288-438`). The new `vala.audit_publication` row is separate from the chain head (`crates/vala/vala-sql/migrations/20261003000000_audit_publication_progress.sql:14-37`); freeze uses `FOR UPDATE NOWAIT`, while settlement advances the watermark, conditionally clears only the matching bound, and deletes through the watermark in one tenant transaction (`crates/vala/vala-sql/src/queries/audit_staging.rs:317-488`).

On graceful process shutdown, transport and accepted request work drain first, then Bifrost owners shut down, and the process outbox is closed and awaited against the same absolute deadline (`crates/wyrd/wyrd-server/src/app/server.rs:744-884`). Rows committed after the publisher has stopped remain in Postgres for a publisher after restart or on another Scribe replica. Abrupt process loss can lose only process-local unflushed events; that is the explicitly accepted window and is not reported as a defect.

## Failure and recovery assessment

| Scenario | Observed behavior | Recovery/proof assessment |
|---|---|---|
| Postgres chain-head contention for one tenant | One commit for that tenant blocks while up to three other tenant commits can proceed. Later events for the blocked tenant remain queued. | The low-volume isolation case is proved by `pg_audit_outbox::a_contended_tenant_does_not_delay_another_tenants_audit`. Saturation isolation is not preserved; see `SYS-002`. |
| Transient Postgres acquire, append, or commit failure | The entire in-flight tenant batch is logged and counted once per event, then removed from `pending`; no retry state survives. Requests and unrelated server capabilities remain available. | This is intentional in the candidate and is directly asserted by the Gate/start-run journey, but conflicts with the newer approved retry-on-dependency-slowness principle; see `SYS-001`. |
| Queue saturation | `stage` remains non-blocking and rejects the new event after the one global 16,384-event pending count is reached. | The request continues and the loss is counted. A noisy blocked tenant can consume this whole shared allowance and cause a quiet tenant's audit to be rejected; see `SYS-002`. |
| Publisher Postgres or Scribe outage | Staged rows and the frozen bound remain durable; a later sweep retries. | Recovery is sound. The range identity is stable, and Scribe's batch fence absorbs an uncertain replay. |
| Competing publishers | A progress-row contender fails immediately or reuses the committed frozen bound. Both may append the same derived batch, then settle idempotently. | Covered by the progress-row contention/stale-settlement integration tests and the server publication journey. No defect found. |
| Cancellation between freeze, Scribe append, and settlement | A committed bound survives; an append without settlement is replayed with the same batch identity. | Safe. `TenantCycles` abort on publisher shutdown, but durable progress plus Scribe dedup is the recovery owner. |
| Graceful shutdown | New transport admission is stopped, accepted MCP/gateway/Bifrost work drains, then the outbox closes intake and uses the remaining process deadline. A nonzero remainder is logged. | Correct for the approved bounded shutdown contract. The outbox integration test proves a healthy dependency drains, but not transient-failure recovery because the candidate discards failures. |
| Hard process kill / rolling replacement | In-memory unflushed decisions may be lost; committed staging and publication progress survive. | This is the explicitly accepted window. A rolling replacement resumes retained-history publication from shared Postgres. |
| Tenant isolation | Each writer and publisher operation opens a `TenantConn` for the selected `DataTenantId`; progress and staging are RLS protected. | The changed SQL and tests preserve tenant scoping. No cross-tenant widening was found. |

## Material findings

### SYS-001 — Transient audit-store failures are discarded instead of retried

- **Violated obligation:** the user-approved repository principle supplied for this review requires batched server-owned audit outboxes to retry rather than drop on dependency slowness. This is distinct from the accepted hard-process-kill loss window.
- **Location:** `crates/vala/vala-sql/src/audit_outbox.rs:292-309`, with completion accounting at `crates/vala/vala-sql/src/audit_outbox.rs:271-288`; the contrary task proof is encoded in `crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs:871-1018`.
- **Evidence:** `commit_tenant` converts every acquire, append, or commit error into per-event failure records and returns `()`. `settle` then decrements the whole batch from `pending`, so the events leave the only process outbox and can never be retried after Postgres recovers. The journey deliberately installs a rejecting trigger, waits for the outbox to settle, drops the trigger, and asserts that neither lost decision reached staging.
- **Observable system consequence:** a brief database connection interruption, transaction abort, or temporary staging rejection can permanently remove every authorization decision in an in-flight batch while the protected Gate, auth, admin, or verification operation succeeds. This is not the accepted acknowledgement delay or hard-kill window: the process remains alive and the dependency can recover immediately after the batch is discarded.
- **Required testable correction:** revise the approved spec's explicit failed-commit-loss clauses (`REQ-003` and the expensive-to-reverse decision at `changes/active/audit-outbox/spec.md:78-83,126-128`) to match the newer approved repository principle, then keep a failed batch under its tenant owner for bounded retry/backoff without blocking request paths or sibling tenants. Prove that a temporary commit failure followed by dependency recovery commits the original events once, preserves per-tenant order, and still lets graceful shutdown drain to its deadline. Permanent pressure behavior must remain bounded and decision-complete in the revised authority.

### SYS-002 — One blocked noisy tenant can exhaust the process-wide queue and drop quiet-tenant audit

- **Violated obligation:** `REQ-002` requires tenant commits to be bounded and concurrent so one contended tenant does not delay another tenant's audit (`changes/active/audit-outbox/spec.md:71-76`).
- **Location:** the single global pending limit in `crates/vala/vala-sql/src/audit_outbox.rs:40-44,60-66,111-125`, combined with per-tenant waiting and one in-flight commit at `crates/vala/vala-sql/src/audit_outbox.rs:175-192,232-268`.
- **Evidence:** events remain charged to the global `pending` count while queued, waiting behind their tenant, or blocked in commit. Holding one tenant's chain-head lock and enqueuing 16,384 decisions for that tenant consumes the complete process allowance. The next event for an otherwise unconstrained tenant takes the full-queue branch before dispatch can apply tenant concurrency. The existing contention test stages only two events per tenant (`crates/vala/vala-sql/tests/pg_audit_outbox.rs:131-170`) and cannot expose this path.
- **Observable system consequence:** a single noisy or deliberately contended tenant can make audit disappear for unrelated tenants on the same replica even though their Postgres rows and writer connections are healthy. Protected operations still succeed, so the effect is silent except for the shared failure metric and breaks the advertised sibling-tenant isolation under the saturation condition that motivated this change.
- **Required testable correction:** keep the process bound and non-blocking request path, but make outbox admission/dispatch preserve capacity or fair progress for another tenant when one tenant is blocked. Add a Postgres integration test that holds tenant A's chain head, fills A's allowed backlog, stages tenant B, and proves B commits without releasing A; also prove total memory/work remains bounded and shutdown reports any remainder.

### SYS-003 — The required deployed capacity proof is absent

- **Violated obligation:** `AC-005` requires the saturation SLO and two-replica scale-out step to pass in the named capacity benchmark (`changes/active/audit-outbox/spec.md:146-149`).
- **Location:** the completion evidence in `changes/active/audit-outbox/tasks/02-one-outbox.md:113-183` records unit, integration, journey, formatting, lint, and codegen lanes but no capacity run.
- **Evidence:** none of the three task reports records `mise run bench:capacity` results or equivalent artifacts. In the reviewed candidate, `mise.toml` defines `bench:verification:capacity` at lines 511-528 but no `bench:capacity` task, so the exact approved command is not runnable from this candidate without first reconciling the authority/task name.
- **Observable system consequence:** the change's stated objective is recovery of two-replica throughput under audit-chain contention, but the only required production-shaped proof of that outcome is missing. Green correctness suites cannot establish saturation SLO or scale-out behavior.
- **Required testable correction:** reconcile the approved benchmark command with the repository-owned capacity lane, run the required production-shaped one- and two-replica judged steps, and preserve the result artifact showing every audit-related saturation assertion and the scale-out step passed. This is a completion-proof gap; it does not justify weakening the benchmark or substituting a local microbenchmark.

## Verification notes

The task packet reports green format, lint, codegen, SQL integration, Redux integration, server/MCP/SDK journeys, gateway journeys, principals integration, Rust/Python unit and typing lanes. Static review confirmed the named outbox, publication, system-owner publication, failure-injection, competing-publisher, stale-settlement, and shutdown tests exist and exercise the stated healthy/failure paths. `git diff --check base..candidate` passed during this review.

The reported failure-injection proof establishes non-refusal and counter behavior, but it proves permanent loss rather than recovery from dependency slowness. The low-volume contention test does not exercise global-queue saturation. No required capacity evidence was supplied. I did not rerun the expensive Postgres or capacity lanes; this report assesses their recorded claims and source coverage.

## Overall result

**FAIL**

The publisher's durable freeze/replay/settle recovery is sound, tenant-scoped SQL remains isolated, and the accepted eventual-consistency and hard-kill windows are not defects. The live process outbox nevertheless discards transient commit failures contrary to the newer approved recovery principle, its global admission bound lets one contended tenant cause unrelated tenant audit loss, and the required capacity acceptance proof is absent. `SYS-001` also conflicts with explicit revision-1 spec language, so the orchestrator must reconcile that newer approved authority before prescribing implementation remediation.
