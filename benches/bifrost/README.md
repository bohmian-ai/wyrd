# Bifrost benchmark standards

The public benchmark surface has four commands:

```text
bench:bifrost:capacity
bench:bifrost:qualify
bench:bifrost:compare
bench:bifrost:components
```

Capacity is a bounded diagnostic sweep. It runs the absolute offered-rate
sequence `25,50,75,100,200,300,500,750,1000,1500,2000` and may continue at
`2500,3000,4000` while every stage remains healthy. A failed stage is retained,
the next stage confirms the stop predicate, and the highest healthy rate is
replayed for recovery. The four-rate shortened smoke (`100,300,500,1000`) is
non-promotable and makes no wall-clock or SLO claim.

Qualification is separately invoked and replays each scenario's own reviewed
healthy, target-operating, and near-saturation rates for three trials. The
compact `qualification-profile-v2` binds every scenario entry to its exact
source and qualification workload plus selected stage IDs and the SHA-256 of
the complete capacity artifact; it does not duplicate tenant row ledgers.
Matrix qualification requires exactly the six canonical entries, while a
selected run resolves only its exact scenario before cluster startup. Candidate
profiles remain non-authoritative under `target/bifrost-benchmarks` until human
review. Qualification enforces durable-write p99 ≤100 ms, flush-to-strict-visibility
p99 ≤5 s, bounded-query time-to-first-frame p99 ≤500 ms, and at least 200
samples per required operation. A report is versioned as
`wyrd.bifrost.cluster-report/v2`; failed, partial, unsupported, dirty, or
undersampled runs still write a non-promotable diagnostic report.

The declared profile records Rust major/minor, locked Arrow/DataFusion/Iceberg
versions, OS/kernel, architecture, normalized CPU vendor/model and logical
cores, host memory, exact container CPU/memory limits, Postgres image/version/
configuration, and the real local-filesystem object-store root/device class.
Tenant/table identity remains in traces and audit evidence, never metric
labels. Public strict queries project only `row_id, wyrd_event_time`; the
server-managed `data_tenant_id` tripwire is never requested or returned.

`bench:bifrost:compare` is the only comparator. It refuses incompatible report
versions, topology/workload/seed/rate shape, environment identity, missing
telemetry, censored knees, or incomplete trials. Compatible candidates must
run the exact persisted absolute rates. Median-of-three comparisons fail on
throughput −10%, p95/p99 +15%, retry/backpressure +2 percentage points,
fairness below 0.95, or balanced scaling efficiency below 75%.

Reports and diagnostics remain under `target/bifrost-benchmarks`. There is no
checked-in `reference-v2.json` until a compatible full qualification capture
passes and a human reviewer explicitly approves the baseline diff. Promotion
is manual and records before/after manifests, reason, hardware identity, and
review approval.
