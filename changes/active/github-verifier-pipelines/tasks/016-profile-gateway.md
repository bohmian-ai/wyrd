---
id: TASK-016
status: proposed
approved_spec_revision: 16
depends_on: []
---

# Profile Gateway overhead and capture memory

Identify which waits and retained allocations matter for Gateway callers on the
integrated branch. Use the saved `gateway/overhead/20261117T195559Z-cfa3bee12.json`
record and the existing L1 Gateway driver. Findings support
[revision 16 qualification](../spec-revisions/16-shared-workload-management.md#acceptance).

```sh
mise run bench:l1:gateway -- --steps buffered-1kib-capture-metadata.c64 --profile cpu
mise run bench:l1:gateway -- --steps buffered-64kib-capture-metadata.c1 --profile heap
mise run bench:l1:gateway -- --steps buffered-64kib-capture-payload.c64
```

1. **1 KiB metadata c64.** Overhead p99 crossed the 0.50 ms target at
   0.527 ms while full server p99 stayed near 1.37 ms. Inspect dispatch and
   HTTP task resumption with the CPU profile. If scheduling is a candidate,
   use the driver's `--pace` option at a lower offered rate on the same revision
   and record missed arrivals alongside p99.
2. **64 KiB metadata c1.** The latest run opened above the historical peak.
   In the fresh single-step heap run, report server opening, peak, and post-drain
   RSS alongside live heap and Scribe-owned bytes. Identify what stays live
   across repetitions and what RSS remains unexplained.
3. **64 KiB payload c64.** Admission p99 reached 55.35 ms inside 56.03 ms
   overhead. Correlate admission wall time with process pressure, capture-slot
   occupancy, and Scribe write and drain time. If the current metrics leave
   both causes open, identify the missing observation at the Gateway invocation
   wait boundary.

Record the revision, commands, profile paths, capture reconciliation,
and a focused next change or the evidence still needed to select one.
A production fix has its own unprofiled qualification under REQ-024.
