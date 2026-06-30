# Rust Service Architecture Review

Use this reference when a Wyrd proposal specifies Rust crates, service
boundaries, async work, background workers, storage/query engines, PyO3
boundaries, generated contracts, or implementation plans.

## Crate And Boundary Checks

Review whether:

- Durable shared contracts live in `wyrd-spec`.
- `wyrd-spec` remains PyO3-free, IO-free, async-free, tokio-free, SQL-free,
  cloud-SDK-free, object-store-free, Arrow-free, DataFusion-free,
  Iceberg-free, and Delta-free.
- Runtime behavior lives in the owning crate: `wyrd` control plane, `vala`
  observability, `skald` provider/runtime, `shared` utilities.
- `python/py-wyrd` remains a thin aggregator with no business logic and no Rust
  tests.
- Dependencies point inward toward contracts/shared utilities, not across Vala
  and Skald directly.
- Feature gates are per capability/type, not broad compatibility switches.

Blocking patterns:

- Service-private object models that duplicate Wyrd foundations.
- Logic in `python/py-wyrd`.
- PyO3, tokio, SQL, object-store, Arrow, DataFusion, Iceberg, Delta, or
  telemetry SDK implementation dependencies in `wyrd-spec`.
- Compatibility aliases or predecessor package names in public APIs.

## API And Type Design

Review whether:

- Public identifiers use domain types, not raw strings.
- Public contracts are exhaustive by default; extension points are justified.
- Traits are small and capability-focused, with multiple real implementations
  or a clear testing/runtime boundary.
- Enums are used for closed sets where callers benefit from exhaustiveness.
- Builders or typed constructors validate required fields without hiding
  durable side effects.
- Runtime-only handles, clients, pools, callbacks, or threads stay out of
  durable specs.
- Serialization, schema generation, and stable error codes are specified for
  every cross-surface contract.

## Async, Concurrency, And Backpressure

Review whether:

- Async is used at IO boundaries, not to wrap pure computation unnecessarily.
- Libraries do not create ad hoc Tokio runtimes.
- Blocking filesystem, CPU, or compression work is moved off async request
  paths or explicitly bounded.
- External calls have timeouts, cancellation, retry limits, and structured
  errors.
- Workers use bounded channels, semaphores, or queues with metrics.
- Shared mutable state is avoided or narrowly scoped; message passing or
  immutable `Arc` state is preferred for hot paths.
- Shutdown drains or flushes with a bounded deadline and records failures.

Blocking patterns:

- Unbounded `tokio::spawn` without lifecycle ownership.
- `Arc<Mutex<T>>` around broad service state on hot paths without evidence it
  will not create contention.
- Holding locks while awaiting network, object-store, SQL, or DataFusion work.
- Ignoring cancellation when clients disconnect or jobs are superseded.

## Error Handling And Observability

Review whether:

- Library crates use `thiserror`; binaries may use `anyhow`.
- Public cross-boundary errors use the Wyrd error catalog and stable codes.
- Error variants preserve caller action: validation, auth/policy, conflict,
  retryable storage, unavailable dependency, timeout, internal bug.
- `unwrap()` is absent from non-test environment, filesystem, network, parsing,
  user input, database, storage, and external-service paths.
- `expect()` names a true invariant.
- `tracing` spans include request ID, trace context, tenant/space, actor,
  card/run/observation refs, operation, version, and redacted summaries where
  relevant.

## PyO3 And Python Boundary

Review whether:

- PyO3 is feature-gated into the crate that owns the Python-visible type.
- Python inputs are converted at the boundary, then Rust-native APIs do the
  work.
- `Bound<'py, T>` is not held across `.await`.
- Python objects are converted to `Py<T>` before crossing threads, awaits, or
  long-lived state.
- Long Rust compute or IO does not hold the GIL.
- Python-visible behavior is tested from public `wyrd` imports and stubs are
  generated, not hand-edited.

## Verification

Implementation plans should name narrow and completion gates:

- Rust unit/property tests for Python-free logic.
- Integration tests for storage, registry, lineage, policy, audit, workers, and
  cross-store recovery.
- Python tests for Python-visible behavior and PyO3 lifetimes.
- Codegen/schema/stub checks for public contracts.
- Load or benchmark gates for hot ingest/query paths.
- Failure tests for retry, timeout, cancellation, lease loss, stale snapshots,
  duplicate batches, and shutdown.

Missing verification is a major issue when it weakens tenant isolation, public
contracts, PyO3 boundaries, storage correctness, policy/audit behavior, or
production performance claims.
