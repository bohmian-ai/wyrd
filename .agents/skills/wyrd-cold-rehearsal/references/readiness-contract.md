# Executability and allocation contract

Load this reference for memory-bounded, fail-safe, concurrent, durable, or
cross-owner tasks.

## Required proof tables

### Interface construction

| Production value | Constructed by | Inputs available then | Authoritative owner | Consumer | Terminal disposition |
|---|---|---|---|---|---|

Every row must correspond to an executable material construction path or an
existing repository API. The packet fixes which owner reserves, which facts
are available, which consumer receives ownership, and terminal disposition; it
does not need to freeze a private generic, helper signature, guard argument, or
constructor when multiple in-owner shapes enforce the same contract. A type
name without a material construction path fails.
For a serial downstream task, a call shape defined by an earlier task packet
with current PASS evidence is acceptable as `DEPENDENCY-PROVIDED`; absence of
that future symbol at the shared pre-integration base is not itself a failure.
The packet-to-packet semantic handoff, owner transfer, dependency edge, and
release contract must still be complete, and the implementation controller
must re-resolve private symbols after integrating the predecessor.

### Allocation provenance

| Allocation/site | Pre-allocation facts | Checked bytes/cardinality | Reserving authority | Enforcement invariant | Live owner/transfers | All release paths | Proof |
|---|---|---|---|---|---|---|---|---|

Include temporary metadata/index vectors, parser state, output encoders,
normalization scratch, retry bodies, queue nodes, channel capacity, task sets,
error-building storage, and library-internal workspaces. Logical length is not
physical capacity. Account simultaneous lifetimes as a peak, not as unrelated
phase totals.

The sole root authority cannot recursively guard the allocation that creates
that authority. This is the only bootstrap exception. A packet using it must
name one exact allocation site and checked `Layout`, prove there is exactly one
per configured root, classify its materialized capacity separately from the
root's governed bytes, include it in the process/container reconciliation, and
forbid it from admitting any other allocation before construction completes.
An `Arc`, pool, or budget below the sole root is not a bootstrap authority and
still requires a real parent guard.

## Allocation invariants

1. Derive the byte/cardinality formula from facts available before allocation.
2. Refuse arithmetic overflow and cap+1 before allocation or mutation.
3. Atomically reserve the full simultaneous peak from the owning authority.
4. Split the reservation only through explicit move-only children whose sum
   equals the grant.
5. Pass the enforcing child into the concrete allocator/constructor.
6. Prevent growth beyond the child; a later length check is insufficient.
7. Retain the child for at least as long as every byte, view, task, or retry.
8. Transfer bytes and their owner together. Do not expose an ownerless escape.
9. Release exactly once on every terminal path; retain deliberately across
   durable unknown outcomes.
10. Observe requested, materialized, peak, transferred, and released values
    without creating a second capacity authority.

Allocator fragmentation and runtime residual may be measured separately, but
they cannot replace exact requested-capacity accounting or justify padding.

## Compile-shaped rehearsal

For each new cross-owner interface, write a minimal call sketch and answer:

- Which crate owns the type and which crates may name it?
- Is the dependency edge permitted and cycle-free?
- Can the caller construct every argument at that point in the workflow?
- Does the method consume, borrow, or clone the owner correctly?
- Does the returned type retain or transfer the guard?
- Can any public or production-callable path allocate or return bytes without
  the guard?
- Are feature-gated test-support types reachable from the real test crate?

If a helper crosses an owner, public/persisted contract, production allocation
authority/bound, dependency, or acceptance boundary, the task must define that
material seam or fail. A local private helper, adapter, fixture, or ownership
plumbing addition inside the approved owner is implementation work and is not
a rehearsal failure.

## Failure-path rehearsal

Trace at least:

- cap+1 and arithmetic overflow;
- malformed or unsupported input before allocation;
- allocation or construction failure after reservation;
- cancellation during work and during output transfer;
- panic/accounting poison and supervisor response;
- retry and unknown durable commit;
- shutdown/drain with every owner released or intentionally retained.

## Verification rehearsal

For every command confirm the task/script exists, target and feature union
compile, filter selects the intended test, external setup is owned by the
command, and the lane is focused. Planned new tests may be absent at the base,
but their owning target and setup path must exist and the task must define the
test seam precisely.

## Regression case

A packet that permits a workload-scaled production metadata index to grow
without a known cardinality/byte ceiling, authoritative admission owner,
terminal release semantics, and proving observation MUST produce `FAIL`.
Private slot representation, helper, guard type, and constructor call are not
the verdict driver when those material invariants are fixed. A rehearsal that
passes the unbounded production path—or fails only on the private mechanics—is
not suitable for task readiness.
