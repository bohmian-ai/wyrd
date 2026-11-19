# Gateway payload c64 bottleneck diagnosis

Status: exploring

## Problem

The `buffered-64kib-capture-payload.c64` Gateway L1 step has low throughput
and a long latency tail. The maintainer wants evidence identifying latency
and memory bottlenecks before selecting an optimization. Earlier attribution
to Scribe preparation was premature: wall-time histograms do not identify
the CPU owner or distinguish computation from scheduling and dependency waits.

## Desired outcome

Explain where caller time goes, what limits completed requests per second,
which owners allocate and retain memory, and what remains live after drain.
Select a focused optimization only after connecting these observations.

## Constraints and non-goals

- Design and read-only evidence inspection now; no production changes or new
  benchmark runs are authorized by this design discussion.
- Preserve existing working-tree edits, spawned Gateway execution, tenancy,
  capture reconciliation, durability and bounded admission.
- Treat sync performance as the drive's floor; do not propose sync tuning.
- Another worktree is running code. Coordinate a quiet interval before future
  experiments; do not stop other processes or weaken the host-busy guard.
- No preferred bottleneck, batch size, buffer increase or implementation fix.

## Proposed implementation

Proposed diagnostic sequence, awaiting discussion:

1. Inspect existing records and profile artifacts before collecting more.
   Record case, revision, binary build identity, profile completeness and host
   validity. Symbolicate only against the matching executable or symbols.
   Existing other-case profiles are controls, not payload-c64 evidence.
2. During an agreed quiet interval, collect an unprofiled payload-c64 baseline
   and CPU plus heap captures of that exact case using the existing harness.
   Keep profiled results separate from throughput qualification.
3. Analyze CPU stacks across the whole server and the separate driver. Trace
   caller admission, execution and response handoff alongside background work.
   Separate CPU service, scheduler/CPU-lane waiting and dependency waiting.
   Retain alternative explanations until evidence distinguishes them.
4. Analyze allocation volume and retained heap by stack/owner. Compare opening,
   peak and post-drain RSS, live heap, outbox backlog and governed memory.
   Distinguish useful retained data, queued work and allocator retention;
   rising RSS alone is not a leak. Check whether current captures provide
   these lifecycle points before proposing extra snapshots.
5. Add only missing benchmark observations needed to distinguish surviving
   hypotheses. Candidates are capture-slot versus process-pressure waits,
   reserved-byte occupancy, CPU-lane waits and pre-authentication elapsed time.
   Specify the exact unanswered question before adding each observation.
6. Produce a compact causal account and ranked optimization recommendation,
   naming supporting stacks/timings/bytes, contrary evidence and uncertainty.
   A later implementation changes one thing and measures it against comparable
   unprofiled records, preserving correctness and retention.

## Evidence and rationale

- Current-tree valid two-step record:
  `target/benchmarks/gateway/overhead/20261118T024519Z-0f8ca4f23.json`.
  Payload repetition overhead p99 is 50.758–58.720 ms; rate is
  8,963–9,258 requests/s. Admission p99 is 50.1–58.2 ms. This locates caller
  waiting but does not establish the throughput bottleneck.
- The subsequent `024645Z` and `024823Z` records are INVALID for host activity.
  They do not settle regression. Earlier 56–65 ms results overlap the valid
  current-tree result; three comparable quiet runs remain unavailable.
- Saved Gateway CPU artifacts found in `20261117T211926Z` cover 1 KiB metadata
  c64. Saved heap artifacts in `20261117T212018Z` cover 64 KiB metadata c1.
  Neither is the requested payload-c64 workload. CPU stack analysis is not
  complete; the old capture's server build identity differs from the current
  profiling executable.
- Existing owners: Gateway invocation and telemetry; process resource/lane
  management; `scribe_outbox.rs`; Scribe ingress, shard and WAL owners.
  Existing harness: `benchmarks/README.md`, capacity Gateway driver and
  `wyrd-testing/src/profile.rs`. Reuse these before introducing machinery.

## Decisions and alternatives

User decisions: restart diagnosis without a preferred cause; investigate both
latency and memory; sync is already hardware-limited; retain spawned execution.
The diagnostic sequence above is proposed, not an agreed production solution.

## Standards conflicts

None identified. Relevant authorities are root `AGENTS.md`, the existing
benchmark measurement contract, and the Gateway/Scribe/resource sections of
`architecture/wyrd-design.md` and `architecture/bifrost-design.md`.

## Open questions

- When is a quiet interval available for the missing payload-c64 captures?
- Do existing heap captures expose allocation and retained memory at the
  lifecycle points needed, or is a small benchmark-only extension necessary?
- Which resource limits sustainable throughput, and which allocations explain
  the memory peak and post-drain footprint? These remain research questions.

## Handoff

Exploring. Next work is inspection and agreement on diagnostic collection,
not a production optimization or an implementation-ready fix packet.
