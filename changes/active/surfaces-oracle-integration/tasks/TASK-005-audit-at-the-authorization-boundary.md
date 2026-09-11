---
id: TASK-005
kind: implementation
status: ready
spec: SPEC-surfaces-oracle-integration
spec_revision: 6
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

Implement this task when the paused, in-progress TASK-001 resumes, against the
same integration branch and before TASK-001 closeout. Do not schedule or execute
TASK-005 independently.

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
  direct Iceberg writer, audit-specific sizing mechanism, scheduler, historical
  table, or compatibility migration for unshipped audit state.
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
