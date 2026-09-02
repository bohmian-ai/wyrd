# Bifrost distributed analytics engine v1 — intent

Build a dual-path Oracle behind one public raw-SQL contract. Callers submit
`BifrostQueryRequest.sql`; Oracle authenticates, authorizes, pins a cut, parses
and optimizes the SQL, chooses `Interactive` or `Analytical`, constructs any
distributed stages, and streams one terminal-safe result. Callers never submit
a physical plan, stage graph, query class, or execution-path override.

The existing native scatter-gather engine remains the default Interactive path.
The additive distributed engine executes only optimized, exchange-requiring
plans whose supported operators and exact immutable facts make Analytical
execution safe. Acceptance requires post-drain evidence that non-serving
followers actually executed the claimed joins, partial/final aggregates,
partitioned windows, supported decorrelated-subquery operators, deduplicating
sets, streamed exchanges, pushdown, and qualified spill. A route label,
`EXPLAIN`, stage count, topology, remote scan, or exact result alone is not
execution evidence.

Analytical remains an Oracle-owned capability, not a second service. It
inherits Oracle's aggregate resource root and protected Interactive floor,
governed `BifrostStorage`, catalog, audit WAL/relay, peer trust, cancellation,
readiness, and shutdown hierarchy. The public query remains an incremental,
bounded asynchronous stream; v1 adds no durable query-job API.

`OracleTelemetry` is the single production instrumentation owner for both
Interactive and Analytical execution. Every Oracle hot path emits production
traces, structured logs, and bounded-cardinality metrics through that owner.
Tests install and inspect the same production recorder and capture used for
operations; they do not substitute test-only counters, events, or harnesses for
production instrumentation. This makes the behavioral journey harness the same
diagnostic surface operators use in production.

Private stage work uses mutually authenticated transport and a signed,
purpose-bound stage ticket. Peer identity and ticket authority are verified
from transport metadata before protobuf or physical-plan decoding. Bounded raw
message hashing may bind a plan body before decode; a first application message
may admit/fence work but is not authentication.

Stage authority carries distinct typed identities for the client-visible
Oracle query and its one private DataFusion physical plan. Oracle allocates the
private identity only after an exchange-bearing plan is validated and
Analytical selection is irreversible. Set-plan, execute-task, caches,
exchanges, attempts, frames, evidence, cancellation, and cleanup bind both
identities; the private identity is never a client field.

Wyrd first proves the pinned upstream library can satisfy admission, lifecycle,
and cancellation through supported extension points. A narrow public fork is a
contingency only when a focused spike proves one required invariant cannot be
implemented externally and cannot land upstream on the required timeline. The
fork may expose only the missing supervised-lifecycle or exact-attempt seam.

Bifrost has never shipped. Implementation changes the unshipped schema,
contracts, queries, fixtures, and generated artifacts directly. It adds no
forward migration, backfill, dual-read/write path, compatibility alias, or
historical execution-path conversion.

Non-goals include replacing Interactive, materialized shuffle, an independent
scheduler or shuffle service, a durable job API, mid-query recovery, DML/CTAS,
cross-region execution, result caching, materialized views, pre-aggregation,
autoscaling, distributed UDFs, caller-selected execution, or compatibility
surfaces. Verification is behavioral only: user-journey, integration, and unit
tests. Performance measurement, comparison, thresholds, historical capture,
and publication are deferred to separate future work.
