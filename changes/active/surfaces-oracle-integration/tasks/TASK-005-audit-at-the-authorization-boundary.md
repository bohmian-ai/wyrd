---
id: TASK-005
kind: implementation
status: proposed
spec: SPEC-surfaces-oracle-integration
spec_revision: 5
requirements: []
depends_on: [TASK-001]
parent_task:
remediates: []
blocked_by: SPEC_REVISION_REQUIRED
---

## Objective

Audit records authorization decisions at trust boundaries and nothing else.
The completed outcome has `append_audit` reachable only from a site that
evaluated a principal's permission, `vala.system.audit_log` carrying only
columns that answer an AU-3 question, and the self-referential publication
loop structurally impossible rather than suppressed.

Executed during TASK-001 closeout, against the same integration branch.

## Blocking precondition

This task changes decisions fixed in `architecture/bifrost-design.md:454-463`
and MUST NOT begin until that revision is human-approved. The revision covers:

- Audit fires at authorization boundaries only; engine-internal transitions
  (Scribe writes, Forge maintenance) are lineage, not audit.
- `vala.audit_outbox` becomes `vala.audit_staging` with watermark semantics;
  retirement becomes garbage collection.
- `vala.system.audit_log` deviates from the house hourly partitioning to daily.

It also supersedes the audit portions of TASK-001's status amendment: the
interim `CorrelationPolicy::Observation` change and the `audit_principal_id`
rename are withdrawn.

## Context

`ScribeIngressFrame` carries a required `audit_event`, so "every ingest is
audited" is baked into the frame type. Every caller must therefore invent an
audit event — including `gate::publish_audit_projection`, whose ingest *is*
the audit. `gate/mod.rs:772-789` hand-authors an event whose entire content is
"I published audit sequences N..M", Scribe appends it at batch commit, and
`is_publication_tail` suppresses the resulting spiral by string-comparing row
content against a table name. The outbox therefore never drains to empty.

Gate already builds all three audit events itself (`gate/mod.rs:694`, `:772`,
`:868`). The decision is already made at the boundary; only the *append* is
misplaced downstream.

## Constraints

- No suppression flag, no table-identity exemption, no row-content check. The
  loop must be impossible because no decision occurs, not because a branch
  skips one.
- Reuse the existing Scribe and Forge path. Do not add a write mode, a direct
  Iceberg writer, an audit-specific sizing mechanism, or a second scheduler.
- Preserve tenant isolation, the hash chain, gapless `seq`, and fail-closed
  refusal when audit cannot be appended.
- Do not weaken a gate to pass. All failures are handled in the current
  execution context.

## Change 1 — Audit only at authorization boundaries

Move the append from Scribe's batch commit to the Gate admission that made
the decision.

| Site | Action |
|---|---|
| `gate/mod.rs:694`, `:868` | Keep the event; append at Gate on admission |
| `gate/mod.rs:772-789` | **Delete.** The self-referential publication event |
| `ScribeIngressFrame.audit_event` | **Delete the field** |
| `scribe_batch_commits::record(..., audit_event)` | **Drop the parameter** |
| `vala-sql/src/queries/scribe_batch_commits.rs:296` | **Delete the append** |
| `audit/publication.rs:161,244` `is_publication_tail` | **Delete** with its branch |
| `scribe/audit_envelope.rs` | **Delete** if the frame field was its only consumer |
| `forge_operations.rs:138,301`, `forge_tasks.rs:1727` | **Delete.** No principal, no decision — same rule as Scribe. `vala.forge_operations` already owns this lineage |

Unchanged (real boundaries, each with a principal and a decision):
`oracle/query_audit.rs:620`, `oracle/mod.rs:1205`,
`oracle/reader_pins.rs:1153,1211`, `components/auth/audit_writer.rs:60`,
`catalog/bifrost_catalog.rs:1016`, `wyrd-storage/src/audit.rs:104`,
`wyrd-server/src/audit/mod.rs:135,152,178`.

`publish_audit_projection` then builds no event — not because it is exempt,
but because moving already-audited records evaluates no new permission.

## Change 2 — `audit_log` schema

```rust
const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::None;

fn arrow_fields() -> Vec<Field> {
    vec![
        Field::new("seq", DataType::Int64, false),
        utf8("entry_hash", false),
        utf8("principal_id", false),
        utf8("principal_card_ref", true),
        utf8("operation", false),
        utf8("resource", false),
        utf8("outcome", false),
        utf8("request_id", false),
        utf8("trace_id", true),
        utf8("detail", true),
    ]
}

fn physical_layout() -> PhysicalLayoutWire {
    daily_layout(
        vec![sort_desc(WYRD_EVENT_TIME), sort_asc("principal_id")],
        &["principal_id", "resource", "operation"],
    )
}
```

26 physical columns become 15 (10 content plus `wyrd_event_time`,
`wyrd_ingested_at`, `wyrd_batch_id`, `wyrd_row_ordinal`, `data_tenant_id`).

Deletions, each because no AU-3 question needs it:

| Column | Reason |
|---|---|
| `prev_hash` | Byte-identical to row `seq-1`'s `entry_hash` |
| `created_at_us` | Becomes `wyrd_event_time`; one event, one timestamp |
| `principal_kind` | Property of the principal; agent case already discriminated by `principal_card_ref IS NOT NULL` |
| `auth_method` | Two values, one derivable from `principal_id` being a platform principal. Earns back when a third auth method ships |
| `permission` | A static function of `operation`; denormalizes a code constant into every row |
| `payload_summary` | `NOT NULL` free text nothing parses, beside typed redacted `detail` |
| `decision` + `result` | Three reachable states, two booleans. Merge to `outcome` in `{allowed_success, allowed_failure, denied}` |
| `run_id`, `card_uid`, managed `principal_id`, `wyrd_request_id` | Publication metadata describing the exporter, removed by `CorrelationPolicy::None` |

`card_ref` becomes `principal_card_ref`: it is the actor's writer-identity
card (`wyrd-spec/src/vala/api.rs:2736-2738`), not the card acted upon — that is
`resource`. Under `None` the reserved name is free, so the `audit_` prefix
workaround goes away.

Two behavior changes travel with the schema:

1. **Ingest honours `CorrelationPolicy::None`.** `DecodeContext` already carries
   `definition.correlation_policy`, so the unconditional correlation append in
   `scribe/execution_lanes.rs` becomes conditional at one site. Dynamic tables
   (`definition: None`) keep today's envelope. Restore the `None` variant in
   `tables/mod.rs`, `tables/managed_columns.rs`, and the docs line.
2. **Skip card-scope resolution under `None`.** `resolve_card_uids`
   (`execution_lanes.rs:1109-1144`) matches a non-null `card_ref` against the
   *publisher's* signed scope and fails the whole batch on a miss; the audit
   row's card belongs to the original actor. Under `None`,
   `principal_card_ref` is plain content and resolution never runs.

## Change 3 — Event time

`project_audit_rows` (`tables/audit/projection.rs:210-212`) already reads
`row.created_at`, stamped by Postgres `DEFAULT now()` inside the audited
operation's own transaction (`20260802000000_vala_audit_outbox.sql:46`) —
server clock, never client-supplied. Today it lands in `created_at_us` while
`wyrd_event_time` receives Scribe's *receipt* time, so the table is partitioned
by publication time: a three-day publisher outage puts three days of audit in
one partition stamped "now".

Route the same value into `wyrd_event_time` and delete `created_at_us`.

`event_time_past_window_secs` defaults to `None`
(`wyrd-server/src/config.rs:1547`), so a backlog replays unbounded by default.
A deployment that configures a past window must exempt the audit path; record
this in the design doc.

## Change 4 — Staging, not outbox

Rename `vala.audit_outbox` to `vala.audit_staging`. An outbox delivers to an
external consumer and there is none; it is a write-ahead staging buffer for a
table this system owns. The wrong name is why it grew delivery-protocol
machinery.

Replace claim/retire with a watermark:

```
1. SELECT … WHERE seq > watermark ORDER BY seq LIMIT N
2. verify chain continuity across the batch
3. gate.publish_audit_projection → scribe.ingest_frame
4. advance watermark
5. DELETE seq <= watermark - grace          -- garbage collection
```

One monotonic `last_exported_seq` per tenant is the only progress state. A
crash between 3 and 4 replays a range that Scribe's existing batch-id dedup
fence absorbs. Deletes `retire_published` (`audit_outbox.rs:255`) and the
"may retire only after durable publication" invariant.

Keep the polling publisher on a fixed interval. Do not convert it to cron or
to a per-event trigger: Iceberg commit rate is the scarce resource, and
conflict probability rises non-linearly with commit frequency while
`commit.retry.num-retries` defaults to 0.

## Change 5 — Daily partitioning

Add `daily_layout` mirroring `hourly_layout` (`tables/mod.rs:120`).
`TimeGranularityWire::Day` already exists (`wyrd-spec/src/vala/api.rs:315`) and
is unused, so this is a helper addition, not a contract change.

This deviates from all seven built-in tables. Justification, decisive reason
first:

1. **Compaction ceiling.** Forge cannot bin-pack across partition boundaries,
   so hourly partitions permanently cap every audit file at one hour of that
   tenant's traffic. A tenant emitting 1k rows/day gets ~42 rows per file
   forever, regardless of Forge tuning. A layout-imposed hard ceiling, not a
   tuning preference.
2. Volume is two orders of magnitude below the telemetry tables: one row per
   authorized request versus continuous high-rate ingest.
3. Query shape is date-range ("what did P do on the 14th"), not recent narrow
   window, so hourly pruning buys nothing at 24x the partition count.
4. Retention is years, not days. Years x 8760 partitions is metadata bloat.

No new sizing machinery: shards rotate at 512 MiB or 10 minutes
(`scribe/geometry.rs:38,48`), staging assembles across generations toward
512 MiB independent of rotation (`wyrd-server/src/config.rs:346-348`), and
Forge bin-packs from there (`forge/managed/policy.rs:43-47`).
