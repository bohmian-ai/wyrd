# Security and tenancy domain review

## Overall result

**FAIL**

Revision 2 closes the prior security/tenancy findings for transient write
failure and tenant-admin ordering. Failed batches remain at the front of their
tenant queue with bounded backoff, other tenants remain dispatchable, and all
six affected admin handlers stage their allowance immediately after the
permission verdict. Tenant-bound writes still use forced-RLS `TenantConn`
transactions, and the reviewed surfaces derive audit tenancy and attribution
from verified or receiver-trusted context.

The candidate does not, however, satisfy REQ-009 across the complete audit
lifecycle. The event-ID uniqueness fence exists only in transient
`vala.audit_staging`. Publication deletes that fence while an outbox retry may
still own the same event, allowing one authorization decision to enter retained
history twice under different chain sequence numbers.

## Reviewed boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-aad682fbca5074900`
- Immutable base: `6714ae35d732814240fdcc42fc226c079b14d3f0`
- Immutable candidate: `5a5542cbb965af99e92a3983a2ddf586412cea73`
- Approved authority: `changes/active/audit-outbox/spec.md`, revision 2; the
  original task packet; the r1 review ledger and
  `TASK-AUDIT-OUTBOX-R1-remediation.md`; `AGENTS.md`;
  `architecture/agent-rules.md`; `architecture/wyrd-design.md`;
  `architecture/bifrost-design.md`; and
  `architecture/wyrd-security-posture.md`.
- Scope: r1 closure, new revision-2 obligations, and regressions affecting
  audit security, tenancy, confidentiality, or attribution. Earlier code was
  inspected only where needed to trace RLS, publication retirement, and the
  complete producer-to-retained-history lifecycle.
- Candidate identity was rechecked after inspection and remained unchanged.
  The repository has no `.codegraph/` directory, so ordinary source navigation
  was used.

## Authority and source coverage

| Boundary | Source and authority coverage | Result |
|---|---|---|
| Generic queue and sink tenant binding | `wyrd-runtime/src/outbox.rs`; `vala-sql/src/audit_outbox.rs`; spec REQ-002, REQ-003, REQ-003a, REQ-007, REQ-008; callers of `AuditOutbox::stage` | PASS. Each tenant has at most one in-flight write; failure restores the batch ahead of later same-tenant events; the audit sink opens a connection bound to the queue key. |
| RLS and canonical append | Original audit-staging migration; event-ID migration; `queries/audit_staging.rs`; spec INV-002, INV-003, INV-004 | PASS for tenant isolation. `audit_chain_head`, `audit_staging`, and publication state remain forced-RLS, and append selection/update rely on the connection tenant. |
| Event-ID identity through retry and retirement | `StagedAuditEvent`; `(data_tenant_id, event_id)` constraint; append duplicate lookup; `settle_publication`; `AuditPublisher`; `pg_audit_outbox.rs` | FAIL: `SEC-TEN-R2-001`. The uniqueness authority is deleted before the retry owner necessarily settles. |
| Surface attribution | Server audit builders and shared authorization helpers; `wyrd-auth` grant/authz/session callers; Gate; Oracle; peer audit; gateway; principals; verification | PASS. Tenant-plane events use the verified caller or authorized query tenant. Platform-plane events use `SYSTEM_OWNER`. Unverified peer input cannot choose a tenant; verified peer violations use the receiver-verified tenant. Principal kind, credential ID, delegation attribution, and request ID remain derived from trusted state. |
| Tenant-admin ordering closure | Six handlers in `components/admin/routes.rs` and their focused tests; prior `FIND-AUDIT-OUTBOX-2` | PASS. Allowed decisions are staged before discovery, projection/sealing, tenant-connection acquisition, or mutation; denials remain staged by the shared authorization owner. |
| Audit confidentiality | Audit event builders; generic writer logging; Gate/Oracle/peer detail projections; security posture | PASS within the approved no-event-content-change boundary. Retry logs contain the bounded outbox name, tenant ID, SQL error, item count, and delay, but not event bodies, bearer material, client secrets, prompts, or request payloads. Metrics use only the closed `outbox` label. |
| Security and operator documentation | Security posture, Bifrost design, reliability guidance, runbook, public Bifrost architecture docs | FAIL as a consequence of `SEC-TEN-R2-001`: each states that an unknown-outcome retry cannot duplicate a decision, which the retained-row lifecycle does not enforce. Other retry, loss, and operator-metric descriptions match the candidate. |
| Dependency/supply-chain change | `wyrd-runtime` and `vala-sql` manifests; `Cargo.lock` | PASS. The change moves already workspace-pinned `metrics`, `tokio-util`, and `tracing` into the shared runtime dependency cone and adds the existing `wyrd-runtime` crate to `vala-sql`; it adds no new registry source or unpinned dependency. |

## Verification limits

- This was a review-only source audit. No production or test source was
  modified, and no test lane was rerun.
- Recorded remediation evidence covers the generic outbox tests, Vala SQL
  integration tests, server/principal/Bifrost journeys, TypeScript lanes,
  codegen, docs, formatting, lints, client-tier checks, and unwrap audit. The
  focused REQ-009 test writes the same batch twice while its first rows still
  exist in staging; it does not interleave publication and deletion between
  the first commit and the retry.
- No fault-injection mechanism currently proves the exact unknown-commit
  sequence across both owners: commit succeeds but reports failure, publisher
  retains and retires the row, then the outbox retries the same event ID.
- The migration applies a volatile UUID default to existing staging rows and
  creates a non-concurrent unique constraint. The review found no
  production-cardinality migration timing or lock-impact evidence. This is a
  rollout verification limit, not a finding: migrations run under the
  repository's explicit migration owner, and no approved lock-time or
  zero-downtime criterion was supplied.
- No SQL/command/template injection, path traversal, unsafe deserialization,
  secret exposure, authentication bypass, cross-tenant read/write, CORS,
  redirect, or cryptographic regression was found in the reviewed boundary.

## Material finding

### SEC-TEN-R2-001 — INCORRECT — staging retirement removes the retry idempotency fence

- **Violated obligation:** spec REQ-009 and AC-009 require a batch retried
  after an unknown commit outcome to produce no duplicate staged or retained
  decision. INV-002 requires an accountable, ordered tenant audit chain, and
  the security posture promises that a retry never stages a decision twice.
- **Exact locations:**
  `crates/vala/vala-sql/src/queries/audit_staging.rs:111-129` checks event IDs
  only in `vala.audit_staging`;
  `crates/vala/vala-sql/src/queries/audit_staging.rs:489-520` deletes every
  staged row through the publication watermark;
  `crates/vala/vala-sql/migrations/20261003000001_audit_staging_event_id.sql:12-15`
  makes the unique key local to that transient table; and
  `crates/vala/vala-sql/tests/pg_audit_outbox.rs:185-215` exercises retry only
  while the original rows remain staged.
- **Evidence and reachable path:** `AuditSink::write` can receive an error from
  `TenantConn::commit` after Postgres committed but before the client learned
  the outcome. The generic writer therefore restores the exact
  `StagedAuditEvent` to the tenant queue and backs off for 50 ms, doubling to
  five seconds. During that ownership window, the independent publisher can
  freeze the newly committed sequence, append it durably to
  `vala.system.audit_log`, advance the watermark, and delete the staging row.
  The publisher also sweeps every five seconds, so this is a normal race after
  a prolonged fault reaches maximum backoff. When the outbox retries, its
  `already_staged` query finds no event ID and the unique constraint has no
  surviving row to conflict with. The append assigns the same decision a new
  sequence and hash; a later publisher range has a different batch identity,
  so Scribe's range-level dedup fence does not absorb it.
- **Observable consequence:** retained audit history can contain two immutable
  rows for one authorization decision. Counts, investigations, compliance
  exports, and action-to-decision attribution can therefore overstate activity
  and cannot distinguish the duplicate from a second real permission
  evaluation. The chain remains syntactically gap-free, which makes the
  semantic corruption difficult to detect after staging is gone.
- **Required testable correction:** make the event-ID idempotency authority
  survive staging retirement for the full lifetime of any retry owner, while
  preserving the single canonical append, one publisher, tenant RLS, chain
  ordering, and transient-staging requirements. If that requires a new durable
  dedup authority or changes what retained history stores, obtain the required
  persistent-data/security specification decision instead of relying on a
  timing delay. Add a Postgres/Scribe integration test that forces a successful
  commit to report an unknown outcome, publishes and retires that exact row
  before releasing the outbox retry, then proves one retained decision, no
  second sequence allocation for that event, and an intact chain.

## Security Audit

### Critical

- None.

### High

- None.

### Medium

- `[crates/vala/vala-sql/src/queries/audit_staging.rs:111]` Event-ID deduplication
  is scoped to transient staging, but publication deletes the key while a retry
  can still own the event. An unknown commit outcome followed by publication
  and retirement lets the retry create a second retained audit record under a
  new sequence. Preserve an enforceable tenant-scoped event-ID fence through
  the retry lifetime and prove the retire-before-retry ordering end to end.

### Low / Defense In Depth

- None within the approved task boundary.

### Positive Controls

- The generic writer preserves failed batches, same-tenant order, bounded
  cross-tenant write concurrency, and capped backoff without holding a database
  connection during the delay.
- Audit append and publication operate through forced-RLS tenant connections;
  the candidate did not add an operator-pool bypass or a second audit sink.
- Allowed and denied decisions remain derived from the same permission verdict
  that governs the request, and the repaired admin handlers stage before
  fallible post-verdict work.
- Platform and pre-verification peer events use the reserved system tenant and
  principal; verified tenant events use receiver-trusted tenant context.
- Audit logs and metrics remain redacted and bounded-cardinality, and no new
  external dependency or secret-bearing configuration was introduced.
