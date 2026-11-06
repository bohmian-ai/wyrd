---
id: SPEC-resource-scaling
revision: 3
status: approved
---

# One startup resource configuration for Bifrost

Every Bifrost consumer in a process should size itself from one resource
configuration resolved at startup, so an operator's CPU cap or explicit worker
setting means the same thing everywhere. The process Scribe outbox keeps main's
fixed concurrency, and its delivery limit holds even when a waiter is cancelled.

Revision 2 also scaled outbox writers and ordered lanes with CPU. Measured on
the Gateway L1 benchmark, that change reduced throughput while raising CPU and
memory. This revision withdraws it: the outbox keeps main's geometry, and any
future outbox concurrency change must first show the outbox is the bottleneck.

## Problem

On main `5234961d51798581a026834cfffd5efaadc226b4`, the existing Bifrost
resource owner resolves CPU, memory and scratch once and composes Scribe,
Oracle and Forge from one root. Two gaps remain:

- **Divergent CPU sizing.** Omitted Scribe coordination, ingress and
  persistence worker counts are derived by probing the host's CPU count
  separately from the resource owner. An effective CPU cap therefore sizes
  Oracle from the capped value while Scribe workers still follow the host.
- **Uncounted delivery work.** Each outbox writer allows up to eight concurrent
  frame deliveries (encode, submit to Scribe, wait for acknowledgement), four
  writers in all, so up to 32 per process. When a delivery's waiter is
  cancelled, blocking encoding that already started keeps running but no
  longer counts against that limit, so retries can accumulate encoding work
  beyond it.

## Why revision 2's scaling is withdrawn

Revision 2 set writers and lanes per tenant to `W = max(4, C)` with `8 × W`
deliveries. On a 32-CPU host that is 32 writers and 32 lanes instead of 4 and
4. Gateway L1, same host, same workload, median (min..max) of three
repetitions:

| Step | Main | Revision 2 |
|---|---|---|
| buffered 1 KiB metadata, 64 clients | 53,368 req/s (51,131..53,408) | 49,372 req/s (48,750..50,705) |
| streamed 1 KiB metadata, 64 clients, equal paced rate | 1.49 cores, 1,446 MiB peak RSS | 2.67 cores, 3,367 MiB |
| buffered 64 KiB metadata, 64 clients | 18,269 req/s | 17,018 req/s |
| buffered 64 KiB payload, 64 clients | 4,783 req/s, 11.1 cores, 8,882 MiB | 5,110 req/s, 16.8 cores, 14,053 MiB |

Each lane batches independently behind its own 50 ms linger. More lanes split
the same writes into more, smaller slices, and every slice pays fixed encode,
Scribe ingress, WAL and memtable-batch costs. This is the batching trade-off
Kafka documents for `linger.ms` and `batch.size`. The evidence never showed
lane concurrency limiting throughput. The large-payload step improved, but the
benchmark did not record why its slices were flushed, so the gain is not
attributed to any batching mode, and it cost 1.5× CPU and 1.6× memory.

Coalescing several idle lanes into one slice could recover batch sizes, but it
would make several lanes one frozen retry unit, block their successors
together, and require new fairness and frame-identity rules. That adds
scheduling and failure behavior only to regain efficiency main already has,
so it is rejected for this change.

## Proposed behavior

Resolve one immutable resource configuration per process at startup, before
any consumer starts, from that process's own observations. Let C be the
existing effective CPU (the observed CPU, which `WYRD_BIFROST_EFFECTIVE_CPU`
may only lower):

| Setting | Policy |
|---|---|
| Scribe coordination, ingress and persistence workers, when omitted | Existing formulas and floors, using C from the shared configuration: with `B = max(C − 2, 2)`, ingress `max(B / 3, 1)`, persistence `max(B − ingress, 1)`, coordination `max(C, 2)` (integer division) |
| Scribe workers, when set explicitly | The configured value and its existing validation |
| Oracle, Forge, storage/cache and transport sizing | Existing policies, using the shared configuration |
| Bifrost managed memory and scratch | Existing headroom, cap and free-space-floor policies |
| Outbox writers | 4, fixed, not derived from C and not configurable |
| Ordered outbox lanes per tenant | 4, fixed |
| Concurrent frame deliveries per writer | 8 |
| Concurrent frame deliveries per process | 32 |
| Outbox item/byte capacity | Existing configured limits and defaults |

Example: a 32-CPU pod with `WYRD_BIFROST_EFFECTIVE_CPU=8` today sizes Oracle
from 8 CPUs but derives Scribe workers from 32. After this change Scribe
derives 8 coordination, 2 ingress and 4 persistence workers from the same 8,
unless an operator set one explicitly. The outbox runs 4 writers, 4 lanes and
32 deliveries on every host, as on main.

## Requirements

- **REQ-001 (fixed outbox geometry):** the process Scribe outbox runs 4
  writers and 4 ordered lanes per tenant on every host and CPU setting, with
  two delivery limits: 8 concurrent deliveries within one writer's slice, and
  32 across the process. The process limit is enforced on its own, not only as
  4 × 8, because cancelled work can outlive the writer slot that started it
  (REQ-002). No CPU-derived outbox geometry and no lane-count setting exist.
- **REQ-002 (counted deliveries):** a delivery counts against the process
  limit from encoding through submission and the wait for its Scribe
  acknowledgement, including blocking encoding that continues after its waiter
  is cancelled. A cancelled delivery is not submitted; its count is released
  when its started encoding finishes. Retries cannot start uncounted encoding
  beyond the limit. The count bounds concurrency, not bytes. This applies to
  the Scribe outbox; the verifier submission outbox is unchanged.
- **REQ-003 (preserved outbox semantics):** one outbox, tenant/request
  ordering, identical retry identities, detail-before-summary,
  ACK-before-handoff, admission/loss policy and graceful shutdown are
  unchanged.
- **REQ-004 (diagnostics):** existing startup diagnostics report the resolved
  CPU and its source, memory, headroom, scratch, Scribe worker counts,
  explicit worker overrides, outbox counts, configured queue limits and
  existing dependency caps, without a new public product API. These are
  structured operator log fields, not a stable machine contract.
- **REQ-005 (one configuration):** the existing Bifrost resource owner
  resolves one immutable configuration at server startup. Scribe, Oracle,
  Forge, storage/cache, transport and the outbox use it or capabilities derived
  from it. No consumer constructs a sibling resource root or probes the host
  for its own sizing.
- **REQ-006 (shared CPU):** automatic CPU-derived defaults throughout Bifrost
  use the shared C; explicit worker overrides remain effective. A configured
  effective CPU cap never leaves a consumer sized from an independent probe.

## Invariants, scope and non-goals

- **INV-001:** detection failure, invalid settings and unrepresentable derived
  counts refuse startup before serving; no consumer starts with a fallback
  plan.
- **INV-002:** each process sizes itself from its own resources. A pod that
  sends its outbox to a peer Scribe uses its own configuration; the receiving
  peer uses its own.
- **INV-003:** usage counters, reservations and pressure monitoring stay live;
  only the startup configuration is immutable.
- **INV-004 (future concurrency changes):** a proposal to change outbox
  writers, lanes, delivery limits or batching must first show measured outbox
  saturation and a profile identifying its limiting stage. A CPU count alone
  is not evidence of an outbox bottleneck.

Production and production-equivalent test startup use the same resolution
path, with deterministic observations injectable for tests. Tenants keep
sharing the existing global governors with tenant attribution; every outbox
slice stays tenant-scoped. No permission, audit or durability boundary
changes. The verifier submission outbox keeps its existing four-lane geometry.

Excluded: CPU-scaled outbox geometry, cross-lane slice coalescing, linger or
batch-size tuning, queue-capacity, database-pool, WAL IO or shard scaling,
gateway admission/latency/capture changes, reservation fairness, memory
partitions, per-frame byte allowances, preallocation, new schedulers or pools,
live reconfiguration, new public settings, dependencies, SDK or wire changes.

## Acceptance

- **AC-001 (startup resolution):** injected observations at C=1, 2, 4, 8, 16
  and 32 all resolve the fixed 4/4/8/32 outbox counts. Omitted Scribe workers
  follow the existing formulas from C; explicit settings keep their values; a
  cap lowers C for every consumer. Invalid settings, zero and overflow refuse
  startup through existing errors.
- **AC-002 (delivery limit and correctness):** gated-sink tests show the
  per-writer and process limits hold while acknowledgements are withheld, that
  cancelled blocking encoding stays counted until it finishes, and that a
  retry cannot start encoding past the limit. Existing same-lane order, retry
  identity, detail-before-summary, ACK-before-handoff, tenant separation,
  admission/loss and shutdown evidence remains green.
- **AC-003 (non-regression):** compare main and this change on the same host
  with every existing `bench:l1:gateway` step, three repetitions each as the
  driver already runs. For each step, compare the candidate's median sustained
  rate, server CPU, sampled peak RSS and server p99 against main's minimum and
  maximum repetition. A candidate median outside that range in the worse
  direction is re-run as a fresh, back-to-back main/candidate pair of that
  step; it is a regression only if it falls outside again. Errors, lost, shed
  and undrained captures stay zero. A larger-host scaling demonstration is not
  required.
- **AC-004 (one configuration):** combined and split-role targets, including a
  producer without Scribe, each started from injected observations and an
  effective CPU cap, resolve one configuration whose worker and outbox counts
  match what their consumers run and what the startup diagnostic reports.
  Consumer startup performs no additional host probe.
- **AC-005 (memory and scratch):** existing resource-owner tests continue to
  show unchanged headroom, caps, floors and shared accounting.

All existing correctness, reference and applicable real client/server journey
gates remain. A real SDK verification journey on combined and split targets
continues to read back exact, attributed evidence after drain.

## Decisions and approval

No open material decisions. Requested approval: withdraw revision 2's CPU-scaled
outbox geometry in favor of main's fixed 4/4/8/32, keep the shared startup
configuration, worker sizing, diagnostics and cancellation-safe delivery
accounting, replace the scaling criterion with L1 non-regression, and adopt
INV-004 as the gate for future outbox concurrency proposals.

Consequence: the revision 2 implementation on `change/resource-scaling`
reverts its CPU-scaled outbox counts and configurable lanes, keeping the
shared configuration, worker sizing, diagnostics and delivery accounting.
TASK-001 is re-scoped through `$wyrd-plan` after approval.

Approved 2026-11-06 by the maintainer.

## Authority and baseline

Baseline: main `5234961d51798581a026834cfffd5efaadc226b4`. Owners:
`crates/wyrd/wyrd-server/src/config.rs`, `boot/`, `scribe_outbox.rs`,
`crates/shared/wyrd-runtime/src/outbox.rs` and
`crates/vala/vala-bifrost-redux/src/resources.rs`. Evidence: the Execution
Evidence section of `tasks/TASK-001-shared-resources-and-scaled-outbox.md`.
Follow [Bifrost memory](../../../architecture/bifrost-design.md#admission-and-memory),
[resource invariants](../../../architecture/bifrost-design.md#resource-and-failure-invariants)
and [overload](../../../architecture/operations/reliability-and-recovery.md#capacity-and-overload).
The [Wyrd doctrine](../../../architecture/wyrd-design.md#doctrine) owns the server
boundary and journey proof; [security posture](../../../architecture/wyrd-security-posture.md#security-principles)
owns unchanged tenant and audit authority.

## Revision history

- Revision 1, 2026-11-06: fresh-start draft. Its process-wide 2C delivery limit was
  rejected because it reduced available baseline concurrency.
- Revision 2, 2026-11-06: preserved eight deliveries per writer, scaled writers
  and lanes with CPU above a four-writer floor, and required one shared startup
  resource configuration. Approved 2026-11-06.
- Revision 3, 2026-11-06, approved: Gateway L1 showed revision 2's CPU-scaled
  lanes reduced throughput and raised CPU and memory through batching dilution,
  with no evidence that lane concurrency limited throughput. Restores fixed
  4/4/8/32 outbox geometry, keeps the shared configuration and correctness
  improvements, replaces the scaling criterion with L1 non-regression, and adds
  INV-004.
