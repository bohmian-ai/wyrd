# Bounded two-group commit pipeline

Status: exploring — a read-only Codex design recommendation recorded on
2026-11-22 at the maintainer's request. Not approved. It conflicts with the
eager two-group exclusion in [spec.md](../spec.md) (line 220) and the selected
serial design in [notes.md](notes.md), so it needs a spec revision before any
implementation. Nothing here was built, tested or benchmarked by the reviewer.

## Problem

Each Scribe shard runs one group at a time: WAL SLICE/COMMIT write, one
fdatasync, the PostgreSQL batch-control fence, memtable insert, then ACK, and
only then the next group. At otlp-logs-c64 p50 a request spent about 10 ms
waiting for its shard, 8 ms in WAL append, 12 ms in fsync and 5 ms in
visibility. The mandates are 80 input MiB per server CPU-second on
writes-parquet otlp-logs-c64 and "write as much as possible".

## What the earlier pipeline did and why it failed

Evidence (dirty runs at `09e81560c`, records `20261120T155012Z-09e81560c.json`
and `20261120T164611Z-09e81560c.json`; the prototype was never committed and its
removal is recorded in the archived breakdown):

- It drained every ready request into the next group immediately, with no
  collection deadline.
- It spawned N's fence and then awaited `join!(write_and_sync(N+1), fence(N))`,
  inserting and acknowledging N only after both finished, so a completed fence
  for N waited on N+1's unrelated WAL work.
- Tenant fences stayed serial (it was not the held four-transaction proposal),
  and its rotation projection omitted the preceding group's pending memtable
  bytes.

| Reading (otlp-logs-c64) | Before overlap | Eager overlap |
|---|---:|---:|
| Requests/s | 1,180.251 | 1,064.546 |
| Input MiB/server CPU-second | 58.745 | 54.817 |
| CPU µs/row | 20.335 | 21.792 |
| Client p50 | 44.780 ms | 50.102 ms |
| Mean requests/group, repetition 1 | 31.87 | 18.43 |
| Queue-exhausted groups, repetition 1 | 194/199 | 342/347 |
| Mean sync, repetition 1 | 9.87 ms | 4.82 ms |
| Mean fence, repetition 1 | 5.54 ms | 7.96 ms |

Established: groups shrank in all three repetitions; groups per request rose
about 73% in repetition 1; CPU efficiency fell 6.7%, throughput fell 9.8% and
latency rose although individual syncs got faster.

Inference (not isolated): serial execution had given an accidental collection
interval, and eager collection removed it, raising fixed group work per byte.
The join barrier also delayed ACKs and so the closed-loop clients' next
requests. Why fence latency rose was not determined; PostgreSQL contention is
not proven. The earlier result does not show pipelining is ineffective: it
combined overlap with smaller groups and an unnecessary ACK dependency.

## Recommended design

One shard owner, one ordered WAL producer, at most two dispatched groups:

```text
N:    WAL write → sync → fence → insert/mirror/snapshot → ACK
N+1:                    WAL write → sync → fence → insert → ACK
```

- **Independent completion.** N completes and acknowledges as soon as its own
  work finishes, never waiting on N+1's write or sync. N+1's fence starts only
  after N settles successfully, keeping per-shard fence and visibility order.
- **Split at the durable-WAL boundary.** The owner loop (`shards.rs` `run`)
  today awaits all of `process_group` before collecting another group. Split
  that responsibility where the WAL is durable, keep the existing WAL and SQL
  owners, and service completion events while later IO is outstanding, with no
  cancellation of in-flight IO and no `join!` barrier ahead of earlier
  visibility.
- **Group formation.** Keep tenant/table fairness, existing byte and admission
  bounds and the 64-request cap. For the overlapping slot, drain ready requests;
  if the queue empties below a soft target, wait for arrivals up to a bounded
  deadline. Initial experiment: a 32-request soft target and a 1 ms maximum
  collection interval from the first request collected into that slot (proposed
  values, not proven optima). Keep draining ready requests above 32 up to the
  existing limits. Dispatch immediately when idle with no predecessor, so c1 is
  unchanged. Identity barriers override collection. If occupancy still
  collapses, the experiment has failed its mechanism check; do not add depth.
- **Fsync and fences unchanged.** One sync per group and the existing batched
  per-tenant SQL fence (already multirow). No four concurrent tenant
  transactions, held connections or cross-group fence aggregation. The WAL
  already skips a sync whose frontier a completed sync covers; keep that, but N+1
  is durable only for writes covered by a captured frontier. Cross-group sync
  coalescing and append-during-sync are separate experiments.

### Invariants and failure handling

- **Durability and recovery.** Fence only after every required SLICE/COMMIT
  segment is synced; insert and ACK only after accepted fencing. WAL COMMIT
  alone stays provisional; recovery restores only the canonical accepted attempt
  and fails closed without acceptance authority
  (`architecture/bifrost-design.md`, Scribe durability and recovery).
- **Identity and order.** A repeated `(tenant, table, batch_id)` blocks behind
  its earlier in-flight owner; the within-group duplicate guard does not cover
  two groups. Keep tenant-scoped transactions and conflict reconciliation.
- **Partial or ambiguous failure.** A group's tenant transactions can partially
  succeed. Keep accepted and uncertain ownership and reconcile through existing
  authority. Never fence N+1 past an unresolved N; a failed N+1 must not undo an
  acknowledged N.
- **Backpressure.** With both slots occupied, stop dispatching WAL work. Slow
  fencing fills the bounded slots and raises existing admission pressure rather
  than accumulating synced-but-unfenced groups.
- **Memory.** Two slots bound the count, not the bytes. Charge both groups'
  prepared buffers, conversion overlap, WAL and fence state and pending memtable
  ownership to the existing global/tenant/table ledgers until side effects
  settle.
- **Rotation and shutdown.** Rotation projection includes both groups' pending
  rows and bytes; dispatched groups drain before their generation freezes.
  Dropping an awaiting task does not cancel running WAL work. `FlushAll` is not
  an "all earlier queued appends" barrier; do not invent that contract.

## Expected effect

Same-drive A/B before this proposal: 59.909 → 59.714 input MiB/CPU-second and
1,009.736 → 1,000.378 requests/s; groups averaged about 32 requests, mostly
closing on queue exhaustion, one physical sync per group.

- **Throughput.** In repetition 1, fence plus insert averaged about 5.30 ms of a
  27.74 ms group cycle. Hiding that tail at unchanged group sizes suggests about
  24% more group throughput under ideal scheduling. This is a stage model, not
  a forecast.
- **CPU efficiency.** 59.714 → 80 needs 25.36% less CPU per input MiB.
  Overlapping waits removes no work. Efficiency can only improve through larger
  groups and fewer syncs, fences, dispatches and wakeups per byte; preserving
  group sizes mainly prevents the earlier regression, and extra scheduling could
  cost CPU. A throughput-only gain is an intermediate result.
- Request p50 stage values are overlapping wall-time distributions; do not add
  them into a latency or CPU budget (`benchmarks/README.md`).

## Decisions the maintainer must make

1. Approve two dispatched groups with independently completed ACKs, ordered
   fencing and visibility, and existing acceptance authority.
2. Approve the bounded collection policy and its latency allowance, initially a
   32-request soft target and 1 ms interval with immediate idle dispatch.
3. Keep 80 whole-server input MiB/CPU-second as acceptance, with throughput, c1,
   memory and sustained/read protections.

Keep the exclusions on extra shards, PL/pgSQL or held connections, WAL-only
acceptance and independent receipt quotas. The held four-transaction proposal
need not be revived.

## Measurement plan

Existing tasks and readers only, clean committed baseline and candidate on the
same quiet host, drive, configuration and workload:

```bash
mise run bench:l1:bifrost:writes -- --steps otlp-logs-c1,otlp-logs-c64
mise run bench:l1:bifrost:writes -- --profile cpu --steps otlp-logs-c64
mise run bench:l1:bifrost:writes
mise run bench:l2:bifrost
mise exec -- target/profiling/release/capacity report <record.json>
mise exec -- target/profiling/release/capacity compare <before.json> <after.json>
```

Selected runs diagnose; full unprofiled writes decide acceptance; L2
overload/recovery and mixed ingest/query/Forge follow.

| Question | Readings |
|---|---|
| Did collection avoid the earlier failure? | Group requests/rows/logical bytes/WAL bytes; `group_close_total{reason=exhausted,limit}`; remaining queued requests and bytes |
| Did fixed work per byte fall? | Fsyncs, fence stages, PostgreSQL acquisitions, WAL lane jobs and waits per completed input byte |
| Were both mandates met? | `input_mib_per_cpu_s`, `cpu_us_per_row`, requests/s, input MiB/s/vCPU, server cores |
| Did the user journey improve? | Shard-wait and request p50/p99, group cycle and stage timings, c1 latency |
| Is it sustainable? | Governed and peak memory, memtable and staged backlog, published bytes and files, refusals and errors, L2 recovery |

Group occupancy collapse, with more groups, syncs and fences per byte, would
have caught the earlier failure at once even though sync latency improved. The
measured short windows published no Parquet files, so they show ingest, not
sustained storage throughput; L2 and backlog recovery are needed before
claiming "write as much as possible". `VALID` alone is not enough: the OTLP
validity floor is below the mandated 80.
