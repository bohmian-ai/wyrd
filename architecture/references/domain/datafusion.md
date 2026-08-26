# Apache DataFusion

Load for DataFusion planning, TableProvider behavior, pruning, execution
parallelism, query diagnostics, or memory and spill decisions.

## Use the planner as a boundary

DataFusion should receive a typed, tenant-qualified logical plan from the Wyrd
query service. The provider owns schema, statistics, scan construction, and
honest pushdown claims. A query-admission layer validates table identity,
projection, predicate windows, and sensitive columns before planning. Do not
use a privileged `SessionContext` as an authorization system.

Filter and projection pushdown are correctness and cost concerns. A provider
may advertise exact filter enforcement only when the scan truly applies the
predicate; file or manifest pruning is often inexact. Keep tenant predicates at
the plan root and add a tripwire that fails if a returned row carries the wrong
tenant. Select only requested columns, preserve nullability, and map fields by
name.

## Performance guidance

Accurate statistics enable join selection, repartitioning, and row-group
pruning. Parquet min/max statistics and partition predicates reduce IO, but
they do not replace a bounded time window. Tune batch size, concurrency,
repartitioning, memory limits, and spill directories from measured workloads.
Stream `RecordBatch` results and expose planning/execution metrics rather than
guessing from wall time alone.

Cache a `SessionContext` or provider only with explicit schema/catalog epochs.
Refresh before a new snapshot or schema fingerprint is visible. Keep blocking
object-store work off async request executors and bound parallel file reads.

## Failures and trade-offs

Aggressive pushdown can be fast but unsafe if the provider lies about semantics;
prefer a conservative plan over silently broad reads. More parallelism improves
throughput until object-store, memory, or CPU contention dominates. Spilling
protects process health but increases latency and requires capacity and cleanup
telemetry. Cancellation must stop downstream scans and release reservations.

Fail closed on unknown columns, stale schema, tenant mismatch, unbounded input,
unsupported expression, memory admission failure, or a provider error that
could leave a partial result looking complete.

## Anti-patterns

Reject arbitrary SQL as the only control, `collect()` of user-sized results,
post-collection filtering, positional Arrow mapping, false `Exact` pushdown
claims, global mutable contexts, and a custom executor when DataFusion's
provider and execution abstractions are sufficient.

## Stable Wyrd anchors

- Query contract and tenant tripwire: `architecture/wyrd-design.md` §Bifrost.
- Wyrd query service: `crates/wyrd/wyrd-server/`.
- Providers and execution: `crates/vala/vala-bifrost-redux/src/`.
- Pure query types: `crates/wyrd-spec/src/vala/api/`.

## Primary grounding

- [DataFusion features](https://datafusion.apache.org/user-guide/features.html)
- [DataFusion configuration](https://datafusion.apache.org/user-guide/configs.html)
- [DataFusion TableProvider API](https://docs.rs/datafusion/latest/datafusion/catalog/trait.TableProvider.html)
- [Apache Arrow columnar format](https://arrow.apache.org/docs/format/Columnar.html)
- Wyrd anchors: `architecture/wyrd-design.md` §Bifrost;
  `crates/wyrd/wyrd-server/`; `crates/vala/vala-bifrost-redux/src/`.
