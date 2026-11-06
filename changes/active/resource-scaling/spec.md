---
id: SPEC-resource-scaling
revision: 2
status: approved
---

# Resource-scaled Bifrost execution

A larger host should drain evidence faster when CPU is the bottleneck. The main
baseline has four outbox writers and four ordered lanes per tenant. Each writer
processes one lane's batch, called a slice, with up to eight concurrent frame
deliveries: encoding chunks of that batch, submitting them to Scribe and waiting
for acknowledgements. With enough independent work, that permits up to 32
deliveries across the process. The overlap lets writers make progress while
durable acknowledgements wait and lets WAL commits cover several submissions.

Preserve that overlap and increase writers and lanes with CPU. At eight effective
CPUs, the default becomes eight writers, eight lanes per tenant and up to 64
deliveries. One busy ordering key still runs serially; more lanes enable
independent keys, not reordering of one key.

Resolve one core Bifrost resource configuration during server startup and share
it across every Bifrost consumer that needs resource sizing. Extend the existing
resource owner: CPU, memory, scratch and their resolved settings must not diverge
between Scribe, Oracle, Forge, transport or the process outbox. Existing
role-specific capabilities remain views of that same owner.

More concurrency can increase CPU use and temporary encoding memory. It does not
guarantee higher throughput when WAL, storage, Postgres or a receiving Scribe is
the bottleneck. Preserve the existing memory governor and admission policies;
raising queue capacity alone is not a throughput improvement.

## Startup policy

Let C be the existing effective CPU, M the detected process memory limit, H the
existing configured server-memory minimum, and W = max(4, C). Resolve these
settings once before activating their consumers:

| Setting | Policy |
|---|---|
| Outbox writers | W |
| Ordered lanes per tenant | W, fixed for the process lifetime |
| Concurrent frame deliveries per writer | 8, preserving existing ACK overlap |
| Aggregate concurrent frame deliveries across the process | 8 × W, derived from the writer count |
| Bifrost managed memory | Existing M − H policy, reduced by an existing explicit Bifrost cap |
| Bifrost scratch | Existing detected-capacity, configured-cap and free-space-floor policy |
| Automatic Scribe coordination, ingress and persistence worker counts | Existing formulas, using C from the shared configuration |
| Oracle and Forge resource sizing | Existing policies, using the shared configuration |
| Outbox item/byte capacity | Existing configured admission limits and defaults |

At C=1, 2 or 4, writers and lanes remain four and the aggregate delivery ceiling
remains 32. At C=8 they become eight and 64; at C=16, sixteen and 128. These are
concurrency ceilings, not reserved cores, reserved memory or preallocated buffers.
The aggregate limit must never be applied independently to each writer.

Explicit existing worker settings retain their values and validation; omitted
settings derive from the shared configuration. Preserve the existing worker
formulas and floors rather than introducing another tuning policy. The resource
owner resolves these settings before dependent executors and engines start.
Consumers may derive workload-specific execution sizing from that configuration,
as Oracle already does from scan size and locality; they may not independently
probe the host or choose fallback CPU or memory values.

Use checked sizing arithmetic and existing validation/error behavior. Detection
failure, invalid settings and unrepresentable derived counts refuse startup
before serving; they do not start consumers with different fallback plans.
Do not increase database pool maxima, queue capacities, WAL IO defaults or durable
Scribe shard counts with C. Producer lanes and durable shards are independent;
more shards also add WAL synchronization work.

## Required behavior

- **REQ-001:** the process outbox receives the immutable writer, lane and delivery
  counts from the startup policy. Preserve eight concurrent deliveries per writer
  and the derived process ceiling, without live lane migration or multiplying
  that process ceiling again per writer.
- **REQ-002:** a delivery remains counted through encoding, submission and the
  wait for its Scribe acknowledgement, including started blocking work after
  cancellation. Retries cannot accumulate uncounted encoding work beyond the
  process ceiling. A delivery count does not establish a byte bound.
- **REQ-003:** preserve one outbox, tenant/request ordering, identical retry
  identities, detail-before-summary, ACK-before-handoff, admission/loss policy
  and graceful shutdown. Queue growth alone is not a scaling improvement.
- **REQ-004:** existing startup diagnostics report the resolved CPU, memory,
  headroom, scratch, worker and outbox settings, explicit overrides, configured
  queue limits and existing dependency caps without a new public product API.
- **REQ-005:** one immutable core resource configuration is resolved by the
  existing Bifrost resource owner at server startup. Scribe, Oracle, Forge,
  storage/cache and transport resource sizing, and the process outbox use that
  same configuration or capabilities derived from it. No consumer constructs a
  sibling resource root or independently detects resources for its sizing.
- **REQ-006:** automatic CPU-based defaults throughout Bifrost use the shared C;
  existing explicit worker overrides remain effective. A configured effective
  CPU cap must not leave some consumers sized from an independent host probe.

## Scope and non-goals

Use the current startup resource owner, runtime outbox and Scribe sink. Sharing
the configuration applies on every server target, including pods that send their
outbox to a peer Scribe. Each process sizes itself from its own resources; a
receiving peer uses its own configuration, never the producer's CPU or memory.
Production and production-equivalent test startup use the same resolution path,
with deterministic observations injectable for tests.

Preserve main's existing memory/headroom, scratch, Oracle sizing and Forge
admission policies, queue settings, request sizes, batching, retry and timeout
policies. Runtime reservations, resource usage and existing pressure monitoring
remain live; startup configuration immutability does not freeze those counters.
Tenants continue to share the existing global governors with tenant attribution,
and every outbox slice remains tenant-scoped. This changes no permission, audit
acceptance or durability boundary.

Added RAM can increase governed capacity under the existing policy; it does not
automatically enlarge queues or promise faster writes. Existing queue counters
and batch targets do not account for all temporary encoding memory or exact RSS.

Exclude gateway admission/latency/capture changes, new reservation fairness,
process-memory partitions, per-frame byte allowances, buffer tracking,
preallocation tuning, new schedulers/pools, live resource reconfiguration,
SQL/file-reader work and cache-fill coalescing. Do not import code or requirements
from the discarded branch. Do not replace the existing resource owner with a
parallel configuration or accounting system. Changing other accepted ceilings or
resource ownership boundaries requires its own specification decision.

## Acceptance

- **AC-001 (startup counts):** injected C=1, 2 and 4 each resolve four writers,
  four lanes and 32 aggregate deliveries; C=8 resolves eight/eight/64 and C=16
  resolves sixteen/sixteen/128, with eight deliveries per writer in every case.
  Invalid settings and overflow refuse startup through existing errors.
- **AC-002 (concurrency and correctness):** gated-sink tests demonstrate the
  per-writer and aggregate limits, useful progress across independent lanes,
  same-lane order, retry identity, counting through acknowledgement waits and
  counting of blocking encoding after cancellation. Existing detail-before-summary,
  ACK-before-handoff, tenant separation, admission/loss and shutdown regression
  evidence remains required.
- **AC-003 (throughput):** using an existing Gateway or Verification driver,
  compare unchanged main with this change on the same resources, then compare
  smaller/larger actual CPU allocations with enough independent keys and offered
  work to exercise added concurrency. Record ACK-completed evidence writes,
  latency, backlog, loss, drain/reconciliation, CPU and sampled RSS, alongside
  actual resources and resolved settings. Baseline sustained completed throughput
  must not regress beyond observed run variation. The larger CPU case must show
  higher sustained completed throughput when the write path is CPU-bound. Flat
  throughput with an evidenced dependency bottleneck explains a limit; it does
  not demonstrate throughput scaling. A same-host effective-C sweep proves
  configuration effects, not physically provisioned CPU scaling. No linear
  throughput promise, arbitrary gain threshold or new benchmark framework.
- **AC-004 (one configuration):** injected observations and an effective CPU cap
  produce one consistent configuration across enabled roles and the outbox on
  combined and split-role targets. Omitted Scribe worker settings use that C;
  explicit settings retain their values. Startup diagnostics match the resolved
  configuration. Consumer startup requires no additional host probe. Existing
  Oracle/Forge, storage/cache and transport derivations remain tied to the same
  owner.
- **AC-005 (memory and scratch):** injected smaller/larger memory observations
  change the shared governed cap by the existing headroom/cap policy, without
  multiplying roots or queue capacity. Explicit caps, memory floors and scratch
  limits retain their validation and admission behavior.

Proof mapping: startup counts cover REQ-001; concurrency/correctness covers
REQ-001 through REQ-003; throughput covers REQ-003 and REQ-004; one-configuration
covers REQ-004 through REQ-006; memory/scratch covers REQ-005. All existing
correctness, reference and applicable real client/server journey gates remain.
Scaling is independent of gateway latency/capture or separate memory-policy
changes.

## Decisions and approval

No open material decisions. The startup configuration is process-local and
immutable, existing role capabilities share its owner, producer concurrency
preserves the four-writer baseline and scales above it, and existing durable
shard and resource-admission policies remain authoritative. Private interfaces,
module placement and permit plumbing are implementation choices.

Approved 2026-11-06 by the maintainer's agreement to the revision proposal and
requirement for one shared startup resource configuration. Approval records
these behavioral decisions; implementation readiness is a separate review.

## Authority and baseline

Fresh baseline: main at `5234961d51798581a026834cfffd5efaadc226b4`. Relevant owners are
`crates/wyrd/wyrd-server/src/config.rs`, `boot/`, `scribe_outbox.rs`,
`crates/shared/wyrd-runtime/src/outbox.rs` and
`crates/vala/vala-bifrost-redux/src/resources.rs`. The existing resource bootstrap
already composes Scribe, Oracle and Forge from one root; the missing behavior is
consistent startup sizing of all consumers, including Scribe defaults and the
outbox. Required architecture documentation must express one core resource
configuration and distinguish the one logical outbox dispatcher from its
concurrent slice writers.
Follow [Bifrost memory](../../../architecture/bifrost-design.md#admission-and-memory),
[resource invariants](../../../architecture/bifrost-design.md#resource-and-failure-invariants)
and [overload](../../../architecture/operations/reliability-and-recovery.md#capacity-and-overload).
The [Wyrd doctrine](../../../architecture/wyrd-design.md#doctrine) owns the server
boundary and journey proof; [security posture](../../../architecture/wyrd-security-posture.md#security-principles)
owns unchanged tenant and audit authority.

## Revision history

- Revision 1, 2026-11-06: fresh-start draft. Its process-wide 2C delivery limit was
  rejected because it reduced available baseline concurrency.
- Revision 2, 2026-11-06: preserves eight deliveries per writer and a four-writer
  floor, scales writers/lanes with CPU, and requires one shared startup resource
  configuration. Records the maintainer's agreement to the revision proposal and
  explicit requirement that all Bifrost resource consumers share that
  configuration. This specification does not authorize implementation tasks,
  implementation edits or benchmark execution in this turn.
