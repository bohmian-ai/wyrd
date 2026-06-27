# Production Architecture Rubric

Use this reference when a Wyrd proposal claims production readiness, touches a
service boundary, handles durable writes, introduces background workers, changes
security posture, or affects high-volume ingest/query paths.

## Review Threshold

Report only findings that affect correctness, security, reliability,
performance, cost, operability, public contracts, or implementation feasibility.
Do not report style, wording, or preference issues unless they can cause a wrong
implementation or a user/agent-facing contract problem.

## First Questions

- What is the expected scale: requests/sec, records/day, bytes/day, tenants,
  concurrent users, query volume, object-store calls, and retention?
- What are the latency and reliability targets: p50/p95/p99, RPO/RTO,
  visibility lag, retry window, and failure tolerance?
- What is the security boundary: tenant/space/profile, actor identity, scopes,
  secrets, policy checks, audit trail, and redaction?
- Which service owns each side effect: registry, storage, lineage, policy,
  audit, Vala observability, or Skald runtime?
- Which operations are idempotent, retryable, cancellable, and observable?
- What verification proves the claim: unit, integration, load, migration,
  property, security, codegen, or demo gate?

## Security And Governance

Production Wyrd designs should specify:

- Actor identity propagation across HTTP, Python, CLI, MCP, UI, workers, and
  service-to-service calls.
- Authn/authz and policy checks at every management-plane write, install,
  execution, artifact use, trigger firing, and MCP write.
- Tenant/space isolation in storage keys, SQL predicates, cache keys,
  object-store paths, metrics, traces, and background jobs.
- Durable audit records with request ID, trace context, actor, decision,
  affected card/run/observation/artifact, redacted payload summary, and result.
- Secret handling with redacted `Debug`, no trace/log leakage, no generated
  schema examples containing live secrets, and no public errors exposing tokens.
- Supply-chain/dependency risk when new crates, native libraries, object-store
  clients, provider SDKs, or code generation tools are introduced.

Blocking patterns:

- Cross-tenant cache keys or queries without tenant/space inputs.
- MCP or CLI writes without scopes, policy checks, idempotency, and audit.
- Background workers that bypass the same policy/audit requirements as request
  handlers.
- Secrets in specs, generated schemas, logs, traces, status, or test fixtures.

## Reliability And Distributed Operation

Review whether the plan covers:

- Idempotency keys for retryable writes and background jobs.
- Bounded queues, admission control, timeouts, cancellation, and backpressure.
- Retry policy with retryable vs terminal errors.
- Lease/fencing semantics for singleton work such as compaction, checkpointing,
  vacuum, backfill, pollers, or scheduled eval/drift jobs.
- Graceful shutdown behavior and bounded flush/drain deadlines.
- Cross-store consistency when an operation spans Postgres, Iceberg/object
  store, registry records, audit records, or cache invalidation.
- Reconciliation jobs and metrics for pending, failed, orphaned, duplicated, or
  partially visible work.

Blocking patterns:

- Unbounded `tokio::spawn`, queues, in-memory maps, or query concurrency.
- Sleep/poll coordination where scale or correctness requires leases, queues, or
  explicit scheduling state.
- Claims of horizontal scaling without ownership of ordering, deduplication,
  coordination, and shutdown.
- Atomicity assumptions across Postgres, Iceberg, object stores, and caches.

## Performance And Cost

Require explicit budgets when the proposal makes performance claims:

- Ingest throughput, batch size, flush age, write latency, and backpressure
  behavior.
- Query p95/p99, max rows/bytes scanned, concurrency, cache behavior, and broad
  scan fallback.
- Storage growth, retention, compaction/checkpoint cadence, object-store
  request rates, and metadata growth.
- CPU, memory, connection pool, async runtime, and worker-pool limits.

Review for:

- Hot-path data kept in typed Rust/Arrow structures, not JSON/string roundtrips.
- Batching where one request/record should not become one durable commit/file.
- Query isolation so broad scans, dashboards, or compaction cannot starve
  ingestion and management-plane writes.
- Capacity/load testing that matches production row width, cardinality, object
  store, concurrency, and long-running maintenance pressure.

## Agent And Developer First

Production quality includes agent and developer usability:

- Declarative specs remain serializable and inspectable.
- Errors carry stable Wyrd codes and actionable remediation.
- Python, HTTP, CLI, MCP, UI, generated schemas, and docs expose the same
  contract with surface-specific ergonomics only.
- MCP tools and generated docs describe required scopes, idempotency, examples,
  errors, and side effects clearly enough for agents to call safely.
- Defaults are safe for local development and explicit about production knobs.

## Verdict Guidance

- **Critical**: data loss, cross-tenant leak, governance/audit bypass, invalid
  durable contract, or architecture cannot meet stated goals.
- **Major**: meaningful reliability, security, performance, cost, migration,
  or implementation-plan gap that must change before build.
- **Minor**: localized ambiguity or missing guardrail that matters but will not
  change the core architecture.
- **Question**: missing information blocks a confident judgment; ask only what
  materially affects feasibility, security, correctness, or scale.
