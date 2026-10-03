# Durability and persistent-data domain review

## Immutable subject

- Base: `ce5c09ef3b559c35e65d6a3e73beebe0a69ae5bd`
- Candidate: `f451d52be01acba66bae003b8509c95086fc8558`
- Approved specification: `changes/active/audit-outbox/spec.md`, revision 1
- Tasks: `AUDIT-OUTBOX-T01`, `AUDIT-OUTBOX-T02`, and `AUDIT-OUTBOX-T03`
- Reviewed range: base..candidate only

## Review Findings

### Important

- **DOMAIN-DUR-001 — INCORRECT** — [`crates/vala/vala-sql/src/audit_outbox.rs:292`](../../../../../crates/vala/vala-sql/src/audit_outbox.rs) and [`crates/vala/vala-sql/src/audit_outbox.rs:271`](../../../../../crates/vala/vala-sql/src/audit_outbox.rs): a failed tenant batch is permanently discarded instead of retained for retry. `commit_tenant` logs and counts every event after any acquire, append, or commit error, then returns `()`; `AuditOutboxWriter::settle` consequently removes the batch from `committing_tenants` and decrements `pending` exactly as it does for a successful commit. This includes ordinary dependency slowness and transient Postgres failures, so a short connection-pool timeout, failover, or database interruption creates a permanent audit gap even while the process remains alive and later recovers. That violates the user-approved repository principle supplied for this review: derived audit work must remain in the batched server-owned outbox and retry on dependency slowness; only unflushed work lost to a hard process kill is an accepted window. The current proof covers chain contention that waits and later succeeds, but does not exercise a commit attempt that fails and then becomes healthy. Preserve the failed batch at the front of that tenant's ordered work, retry it with bounded/backed-off scheduling while keeping its events counted as pending, and do not allow later events for the tenant to pass it. A graceful shutdown should continue those retries until its existing deadline and report any remainder; a hard process kill may still lose the in-memory remainder and be counted. Add a Postgres-backed test that makes the first append/commit attempt fail transiently, restores the dependency without restarting the process, and proves the exact events eventually form one gap-free hash-chain prefix without duplication or reordering. The existing permanent-failure surface tests may continue proving that requests do not wait or fail, but they do not close this recovery obligation.

## Reviewed boundary

| Boundary | Source evidence reviewed | Result |
|---|---|---|
| Migration, backfill, constraints, RLS, and grants | `20261003000000_audit_publication_progress.sql`; predecessor `20260802000000_vala_audit_staging.sql` | PASS. Existing progress is copied before the old columns are dropped; the new row remains tenant-keyed, forced-RLS, and available to `wyrd_app`. |
| Per-tenant sequence and hash-chain atomicity | `append_audit_events` in `queries/audit_staging.rs`; `pg_audit_staging.rs`; `pg_audit_outbox.rs` | PASS. One tenant transaction locks the chain head once, derives every sequence/hash in order, inserts the batch, advances the head, and commits as one unit. Two outboxes are covered against the same durable chain. |
| Batched outbox concurrency and overload accounting | `audit_outbox.rs`; `pg_audit_outbox.rs` | FAIL only for DOMAIN-DUR-001. The pending counter bounds accepted in-memory work and tenant commits are connection-bounded; transient commit failures are nevertheless settled as lost instead of retried. Queue-full and accepted hard-kill loss were not treated as defects under the supplied principle. |
| Frozen publication range and stable batch identity | `freeze_publication_range`, `list_publication_range`, audit projection `derive_batch_id`, Scribe durable batch fence | PASS. The persisted upper bound is reused, the projection identity is derived from tenant plus exact inclusive range, and the global tenant/table/batch fence suppresses the same logical batch even when another replica reaches a different local Scribe. |
| Watermark monotonicity, settlement, and garbage collection | `settle_publication`; `AuditPublisher::{publish_tenant,publish_range,settle}`; SQL and server publication tests | PASS. Settlement uses `GREATEST`, clears only the matching bound, and deletes staging through the committed watermark in the same tenant transaction. A stale completion cannot clear a newer bound. |
| Restart and competing publishers | publication SQL, `AuditPublisher`, `frozen_range_survives_tail_growth_competition_and_stale_settlement`, server publication journeys | PASS. A crash after Scribe acceptance but before settlement leaves the durable bound for identical replay; a competing publisher either reuses it or receives immediate progress-row contention. |
| Durable error-code decoding | `AuditErrorCode` in `wyrd-spec/src/vala/audit_detail.rs`; schemas; reserved gRPC enum number/name | PASS. Request-facing audit-unavailable errors are removed while the retained-history enum value remains decodable and the deleted proto value is reserved. |

## Authority and source coverage

Reviewed the applicable repository authorities and guidance: `AGENTS.md`, `architecture/agent-rules.md`, `architecture/bifrost-design.md` (audit projection, resource/failure invariants, telemetry), `architecture/wyrd-security-posture.md` (tenant/data isolation and audit integrity), `architecture/operations/{README,deployment-and-release,reliability-and-recovery,runbooks}.md`, `architecture/references/domain/{vala-architecture,olap-serving,analytical-operations-reliability}.md`, and the task-review workflow's spec-driven-development and maintainer-style references. The explicit user-approved eventual-consistency/outbox-retry principle was treated as governing review authority for this candidate.

Source tracing covered the migration, all audit-staging append/freeze/read/settle queries, the full outbox writer, publication orchestration, audit projection and stable batch derivation, the Scribe durable batch fence and reconciliation path, audit error decoding, and the relevant SQL/server tests. The repository has no `.codegraph/` index, so normal source navigation was used.

## Verification limits

- I reviewed the implementation evidence and named green commands recorded in the task packets; I did not rerun the broad test lanes in this read-only domain pass.
- No test in `pg_audit_outbox.rs` injects a transient acquire/append/commit failure and proves in-process recovery. Existing failure-injection journeys prove request non-blocking behavior and failure counting, not retry.
- I found no dedicated upgrade test that seeds nonzero `published_seq`/`publishing_seq_hi` under the prior schema and then applies this migration. The SQL backfill order is direct and no defect was established from that verification gap.
- Accepted eventual-consistency windows, receipt-before-durability semantics, graceful asynchronous flushing, and counted loss on hard process kill were deliberately not reported as defects.

## Overall result

**FAIL**

`DOMAIN-DUR-001` is a material durability failure for a reachable dependency-recovery path. Publication progress, retained-history replay, tenant isolation, and durable error decoding otherwise satisfy the reviewed boundary.
