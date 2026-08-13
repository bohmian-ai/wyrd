# Production Risk Review

Load this reference only when the artifact touches security, tenancy, durable
writes, storage, background work, service boundaries, high-volume paths, or an
explicit production claim.

## Proportional Standard

Judge the plan against its stated workflow, scale, deployment topology, and
readiness claim. Do not require mature multi-region, HA, capacity, or provider
machinery merely because Wyrd is an enterprise platform.

Raise a finding when the current design needs a safeguard now, claims a
property it cannot provide, or creates a boundary that will be materially hard
or unsafe to correct later.

## Critical Risks

- Cross-tenant access or cache/object/SQL identity without the tenant boundary.
- Authn/authz, policy, audit, redaction, or secret handling that can be bypassed.
- Durable writes that can corrupt, lose, or irreversibly misattribute data.
- False atomicity across Postgres, object storage, Iceberg, caches, or external
  systems.
- Irreversible migration without a safe transition or recovery path.

## Major Risks

Report only when supported by the actual design or stated targets:

- Retryable writes without idempotency or deduplication.
- Distributed singleton work without ownership, lease, or fencing semantics.
- Unbounded queues, tasks, memory, scans, concurrency, retries, or shutdown.
- No defined ordering or visibility semantics where correctness depends on it.
- A hot path whose per-record commit, serialization, scan, or object-store
  behavior cannot plausibly meet the stated scale.
- Cross-store partial failure with no explicit durable state or reconciliation.
- A production claim without the specific verification needed to prove its
  novel or risky mechanism.

## Do Not Require By Default

- Exact throughput, latency, RPO, or RTO numbers when the design does not make
  a related claim or choice.
- Multi-region, active-active, disaster recovery, autoscaling, or every cloud
  provider for an earlier bounded phase.
- New queues, workers, caches, leases, abstractions, or configuration solely to
  prepare for hypothetical scale.
- Exhaustive operational metrics in an architecture spec when local patterns
  already determine baseline instrumentation.

Prefer a bounded simple design that preserves tenant isolation, durable
correctness, auditability, and observability. Enterprise quality means the
load-bearing boundaries are correct; it does not mean every future operational
capability ships in the current plan.
