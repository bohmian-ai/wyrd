---
id: TASK-005
kind: implementation
status: ready
spec: SPEC-surfaces-oracle-integration
spec_revision: 7
requirements: [REQ-026, REQ-026A, REQ-026B, REQ-027, REQ-027A, REQ-027B, REQ-028, REQ-029, REQ-030, REQ-030A, INV-008, INV-008A, INV-008B, INV-008C, AC-005, AC-017]
depends_on: []
parent_task: TASK-001
remediates: []
---

## Objective

Record authorization decisions at their trust boundaries and nothing else.
The completed flow audits both allowed and denied permission checks before the
operation proceeds or refuses, publishes a minimal verifiable retained ledger,
and makes self-referential audit publication structurally impossible.

Implement this task against TASK-001's completed integration result while its
closeout is paused. TASK-005 must finish before TASK-006 and TASK-001 closeout;
do not restart or reimplement TASK-001.

## Constraints

- Preserve tenant isolation, gapless per-tenant sequence allocation, the hash
  chain, and fail-closed refusal when audit cannot be accepted.
- Gate's existing Bifrost RBAC checks remain the ingest authorization boundary.
  Audit outcomes are `allowed | denied`; they do not claim the later Scribe
  operation succeeded or failed.
- Preserve the effective dynamic `permission` in retained audit content.
- Oracle query admission remains the sole WAL-first exception. No other engine
  transition becomes an audit event.
- Reuse the existing Scribe and Forge publication path. Do not add a write mode,
  direct Iceberg writer, audit-specific sizing mechanism, scheduler, or
  historical table. Audit state is unshipped: edit its existing schema
  definition in place without a migration, compatibility path, or backfill.
- Do not add test cases or test files. Update existing fixtures or assertions
  only where the changed contracts require it, then use existing verification.

## Relevant Surface

- Gate authorization and ingest dispatch in
  `crates/vala/vala-bifrost-redux`
- Scribe ingress, durable batch fencing, correlation stamping, audit projection,
  built-in table definitions, and Forge lineage in the same engine
- Canonical audit contracts and greenfield SQL state in `crates/wyrd-spec` and
  `crates/vala/vala-sql`
- Retained publication and Oracle audit relay in
  `crates/wyrd/wyrd-server`
- Existing audit SQL, projection, publication, and Bifrost journey coverage
- Audit guidance in `architecture/agent-rules.md`,
  `architecture/references/architecture/patterns.md`, and
  `architecture/references/languages/agent-harness.md`

Paths are ownership guidance, not a private implementation allowlist.

## Approach

1. Trace every canonical audit append and retain only sites that evaluate a
   principal's permission. Record Gate allow and deny decisions before further
   admission or refusal; keep Oracle's WAL-first query-read path.
2. Remove audit payload propagation from Scribe and remove internal Scribe,
   Forge, Oracle reader-protection, and retained-publication audit appends while
   preserving their existing lineage authorities. Delete the publication-loop
   suppression made unnecessary by this boundary.
3. Define the greenfield staging and retained schemas around the approved audit
   content, including dynamic permission and `allowed | denied`; canonicalize
   `entry_hash` from retained fields plus the predecessor hash.
4. Preserve the original server-stamped decision time as `wyrd_event_time`,
   exempt audit backlog publication from the ordinary past-event-time window,
   use no observation correlation envelope, and partition retained audit daily.
5. Publish contiguous tenant ranges through Scribe, then atomically advance the
   tenant watermark and delete every staged row through it. Preserve replay via
   the existing deterministic batch identity and durable Scribe dedup fence.
6. Align the remaining audit architecture references with the approved spec and
   current Bifrost and security authorities.

## Acceptance Criteria

- Every permission evaluation produces exactly one tenant-owned audit decision:
  `allowed` before the operation proceeds or `denied` before refusal. Audit
  failure refuses the operation without an unauthorized effect.
- Bifrost ingest continues to use Gate's existing RBAC checks. Retained rows
  preserve the exact dynamic permission that Gate or the owning authorization
  boundary evaluated.
- No Scribe commit, Forge maintenance transition, Oracle reader-protection
  transition, or retained-publication operation appends audit. Their durable
  operational tables remain the lineage and recovery authorities.
- Audit publication carries no synthetic audit event and needs no suppression
  flag, table exemption, or row-content tail check.
- The retained schema contains sequence, reproducible entry hash, principal and
  optional principal Card identity, operation, resource, permission,
  `allowed | denied` outcome, request and optional trace identity, redacted
  detail, and the five managed physical columns required by
  `CorrelationPolicy::None`. No deleted field remains in the hash preimage.
- The original Postgres decision timestamp becomes `wyrd_event_time`; a future
  audit backlog remains publishable beyond the ordinary past-event-time window;
  and retained audit data partitions daily.
- After durable publication, watermark advancement and deletion through the
  watermark commit together. Crash replay does not duplicate retained rows, and
  an idle tenant's staging table drains to zero with no grace tail.
- No compatibility migration, old-file handling, second historical authority,
  or new test is introduced.

## Verification

Use the existing repository coverage only; do not add tests:

```bash
mise run fmt
mise run lints
mise run codegen:check
mise run test:sql
```

Completion evidence records the existing audit, SQL, projection, publication,
and Bifrost journey results that cover the changed flow. Any required behavior
not credibly exercised by existing coverage is a completion blocker; it does
not authorize writing an additional test under this task.

## Completion Evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Every permission evaluation produces exactly one tenant-owned decision; audit failure refuses without effect | `crates/vala/vala-bifrost-redux/src/gate/mod.rs` (`GateAudit::append_write_decision` before admission or refusal); `crates/vala/vala-sql/src/queries/audit_staging.rs::append_audit` | `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=gate::tests::gate_enforces_bifrost_record_write)'` — a sinkless Gate returns `AuditUnavailable` with `scribe_calls == 0`; a sinked Gate records exactly one `Denied` and still never reaches Scribe | PASS |
| Gate's existing RBAC checks stay the ingest boundary; retained rows preserve the exact dynamic permission | `gate/mod.rs` passes the evaluated permission string through unchanged; `vala-sql` staging/retained columns carry `permission` | `mise run test:sql` (`pg_audit_staging`, 218/218) | PASS |
| No Scribe commit, Forge transition, Oracle reader-protection transition, or retained publication appends audit | Audit appends deleted from Scribe batch commit, Forge operation/task transitions, Oracle reader protection and admission recovery, and retained publication; `vala.forge_operation_state`, `vala.forge_tasks.evidence`, `vala.oracle_reader_epochs`, `vala.oracle_table_protections` remain the lineage authorities | `mise run test:sql` — `pg_audit_staging` asserts a batch commit appends 0 audit rows; `pg_forge_tasks` / `pg_forge_operations` assert 0 across every transition and that `wyrd_platform_admin` is denied INSERT on `vala.audit_staging`. `mise run test:bifrost:integration:server` 67/67 — Oracle epoch loss and protection expansion now gate on their own durable rows | PASS |
| Audit publication carries no synthetic event and needs no suppression flag, table exemption, or tail check | Publication-loop suppression removed; `crates/vala/vala-bifrost-redux/src/tables/audit/projection.rs` projects staged rows only | `mise run test:bifrost:journey:server` 7/7 (`replayed_audit_publication_retains_each_event_once`, `audited_transitions_retire_only_into_retained_history`) | PASS |
| Retained schema carries seq, reproducible entry hash, principal identity, operation, resource, permission, outcome, request/trace identity, redacted detail, and the five `CorrelationPolicy::None` managed columns; no deleted field in the preimage | `crates/vala/vala-sql/migrations/*_audit*.sql`; canonical preimage in `vala-sql`; `crates/wyrd/wyrd-sql/src/queries/cards/audit.rs` migrated onto the same columns and preimage | `mise run test:sql` 218/218; `mise run codegen:check` clean | PASS |
| Postgres decision timestamp becomes `wyrd_event_time`; audit backlog publishable past the ordinary window; retained audit partitions daily | `AuditTable` declares `CORRELATION_POLICY = None` and `PAST_EVENT_TIME_EXEMPT`; `scribe/ingress.rs::builtin_definition` resolves every built-in so both declarations take effect, and `execution_lanes.rs` stamps the correlation envelope only when the policy asks for it | `mise run test:bifrost:integration:redux` 973/973 (built-in layout fixed-point assertions require `Day` granularity for a `None`-policy table); `mise run test:bifrost:journey:forge` 13/13 (promotion invariant no longer sees a column-count disagreement on `vala.system.audit_log`) | PASS |
| Watermark advancement and deletion through the watermark commit together; replay does not duplicate; an idle tenant drains to zero | `drain_through_watermark` updates the chain head and deletes rows `<= published_seq` in the caller's transaction; replay is absorbed by Scribe's durable batch-id dedup fence | `mise run test:sql` (`pg_audit_staging`); `mise run test:bifrost:journey:server` 7/7 | PASS |
| No compatibility migration, second historical authority, or new test introduced | Migration 14 deleted rather than superseded; `vala.system.audit_log` is the only retained authority; the diff removes test cases and rewrites existing assertions onto lineage, adding no test file | `git diff` review: no new test file; `mise run check:unwrap-audit`, `mise run lints` clean | PASS |

Non-goals held: no write mode, direct Iceberg writer, audit-specific sizing
mechanism, scheduler, historical table, or compatibility migration was added.
Oracle query reads remain the sole WAL-first exception.

### Commands

```
mise run fmt                              # clean
mise run lints                            # exit 0
mise run codegen:check                    # All checks passed!
mise run test:sql                         # 218/218
mise run test:bifrost:integration:redux   # 973/973
mise run test:bifrost:integration:server  # 67/67
mise run test:bifrost:journey:server      # 7/7
mise run test:bifrost:journey:forge       # 13/13
mise run test:bifrost:journey:oracle      # 28/28
mise run test:bifrost:journey:otlp        # 10/10
```

### Material limits

- `Oracle audit WAL recovery failed: QueryAuditUnavailable` was observed at
  server start in one `test:bifrost:integration:server` run and in neither the
  run before nor the run after. `AuditWal::recover` collapses every filesystem,
  advisory-lock, and framing failure into one opaque `BifrostError`, so the
  message carries no diagnosis. It is unrelated to the audit boundary — the
  recovery path was not touched by this task — and is recorded here rather than
  chased under it.
- `mise run verify:bifrost` was not run: it is withheld by explicit instruction
  until the change's merge work completes.
