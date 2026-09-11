# OLAP serving

Load for Bifrost table design, ingest and query paths, admission, consistency,
tenant-safe analytical APIs, or high-throughput warehouse behavior.

## One analytical substrate

Bifrost is Wyrd's internal distributed OLAP warehouse. Every logical table
resolves to one tenant-qualified physical Iceberg table. Postgres owns
catalog and control state; object storage owns analytical bytes. Built-in and
user-defined tables use the same managed envelope, physical binding, admission,
and tenant rules.

`wyrd-server` owns the public API. Vala owns Scribe, Oracle, and Forge behind
that boundary. Public requests are typed, authenticated, tenant-bound, and
budgeted before analytical work begins.

## Write and visibility lifecycle

```text
validated append
  -> global / tenant / table admission
  -> one of sixteen deterministic pod-local Scribe shards
  -> shard WAL append + fsync + durable batch fence
  -> active memtable authority
  -> immutable member
  -> fsynced, checksummed, query-registered staged run authority
  -> approximately 512 MiB published Scribe hot object
  -> unchanged-object Iceberg promotion snapshot
  -> managed Forge rewrite toward approximately 1 GiB files
  -> fenced Iceberg rewrite snapshot
```

Acknowledgement occurs after WAL durability, batch fencing, and authoritative
active insertion. It does not wait for staging, object publication, Iceberg
promotion, or compaction. WAL retirement occurs only after every member in the
rotation cohort has a validated staged replacement registered for live-tail
reads. Staged authority switches to the published hot object only after the
fenced `file_list` and audit transaction commits or reconciles as identical.

Scribe writes 32 MiB logical or 131,072-row Parquet row groups and combines
compatible same-node staged runs into approximately 512 MiB immutable hot
objects. Those values are independent: a row group is a bounded write unit,
not a whole-file ceiling, and indivisible groups or residues may cross or miss
the approximate object target. Forge first appends eligible hot objects to
Iceberg unchanged, then uses the managed compaction core to rewrite eligible
live files toward an independent approximately 1 GiB target with an independent
128 MiB encoded-row-group target. The managed writer rolls only between
completed writes after its encoded-size estimate exceeds the target; per-
partition residue and compression variance make physical sizes approximate.

Scribe owns writes within one pod. Routing and replay do not coordinate one
logical append across pods. Oracle may distribute reads across authenticated
peers; distributed reads do not imply distributed ingest or catalog writes.

## Oracle read paths

Oracle pins one consistent cut across the selected Iceberg snapshot, published
hot objects, and Scribe live-tail authority. It never opens another node's
local staged files and does not query WAL in normal operation.

Oracle runs the pinned `datafusion-distributed` planner once. A normal
DataFusion physical root selects the interactive admission and terminal path;
a `DistributedExec` root selects the analytical path and its streamed stage
graph. Wyrd adds no operator allowlist, candidate heuristic, second physical
build, or pre-selection fallback. It has no materialized shuffle service,
independent scheduler, or public plan-explanation surface.

Both paths:

1. authenticate the principal and resolve `(data_tenant_id, TableRef)`;
2. authorize the query class, projection, time window, and sensitive columns;
3. acquire path-specific slots from one atomic capacity check and a query-owned
   memory/spill grant;
4. bind the optimized plan to the pinned snapshot and post-pruning statistics;
5. preserve the plan-root tenant predicate and `TenantTripwireExec`;
6. stream bounded Arrow batches with one explicit terminal result.

Distributed stages bind tenant, snapshot digest, fragment digest, and fence
before plan decoding or IO. Every worker receives the admitted query-owned
runtime, memory pool, spill allocation, deadline, and cancellation tree, all
drawn from one aggregate query memory pool shared by operators and exchanges.
Head cancellation joins all descendants. A selected analytical query owns one
execution attempt; peer or transport loss after selection fails the stream
terminally with no successor attempt and no interactive rerun. Success can
never contain partial or duplicate rows.

Interactive and analytical work use separate queues and counters. Analytical
work cannot borrow the protected interactive slot floor. Both remain beneath
one total Oracle capacity check and one shared elastic resource root.

## Admission and failure semantics

Derive the path only from the physical root returned by the pinned planner:
normal root is interactive and `DistributedExec` root is analytical. Planning,
codec, or worker incompatibility is a structured failure, not a reason to build
or run a second plan. Path selection never bypasses authorization, admission,
deadline, audit, or result budgets.

Fail closed on tenant mismatch, schema fingerprint conflict, unknown columns,
unregistered tables, replay identity conflict, unsupported expressions,
oversized input or result, sensitive-column denial, memory or exchange-budget
refusal, invalid stage authority, missing catalog evidence, or ambiguous
publication. Never represent a truncated or failed stream as success.

## Rejected shapes

Reject caller-controlled tenant predicates, one object or catalog commit per
event, cross-pod reads of local Scribe files, a global table lock, unbounded
`collect()`, positional schema mapping, SQL text as the authorization boundary,
materialized shuffle infrastructure, distributed writes, and network listeners
owned by Vala crates.

## Stable Wyrd anchors

- Bifrost authority: `architecture/bifrost-design.md`.
- Cross-system doctrine: `architecture/wyrd-design.md` §Bifrost.
- Wire contracts: `crates/wyrd-spec/src/vala/api.rs`.
- Engine: `crates/vala/vala-bifrost-redux/`.
- Public serving: `crates/wyrd/wyrd-server/`.

## Primary grounding

- [Apache Iceberg specification](https://iceberg.apache.org/spec/)
- [Apache DataFusion features](https://datafusion.apache.org/user-guide/features.html)
- [Apache DataFusion configuration](https://datafusion.apache.org/user-guide/configs.html)
- [Apache Arrow columnar format](https://arrow.apache.org/docs/format/Columnar.html)
