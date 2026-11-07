---
id: SPEC-benchmark-heap-profiling
revision: 1
status: approved
---

# Heap profiles for benchmark runs

A benchmark record says how much resident memory the server peaked at, but not
what holds it or how much a step added. On Gateway L1 the 64 KiB metadata step
at one client peaked at 2.3 GiB against an idle server of about 130 MiB, and
the memory that remained after each step carried into the next. Nothing in
the record or the CPU profile tells a maintainer which code owns that memory.

This change adds two pieces of evidence:

- every repetition records the server's resident memory when its window
  opens, beside the peak, so peak minus opening is the memory that step added;
- `--profile` also captures a sampled heap profile of the server at the end
  of every step, in pprof format, naming the call stacks that hold live
  allocations.

The allocator stays mimalloc. Changing the allocator would profile a different
server than the one benchmarked.

## Proposed behavior

**Opening memory.** Each repetition records `opening_memory_bytes`, the first
resident-memory reading of its window, beside `peak_memory_bytes`. It comes
from the same sampler and interval as the peak. It is evidence only: no
criterion or verdict reads it. A record without the field still reads.

**Heap sampling in the server.** A `bench` build of `wyrd-server` can sample
its own heap. The production build contains none of this code. In a `bench`
build, sampling stays off until the benchmark harness starts it, and while it
is off each allocation and free costs one relaxed atomic load.

Once sampling starts, the allocator samples about one allocation per 512 KiB
allocated, with the exponentially distributed intervals that Go, tcmalloc and
jemalloc use. It records each sampled allocation's size and call stack until
the allocation is freed. Each sample is weighted by the inverse of its
sampling probability, so totals estimate all live heap bytes allocated since
sampling started. Allocations made by the sampler itself are never sampled.

**Endpoints.** These are served on the loopback metrics listener, only in a
`bench` build with benchmark snapshots enabled:

| Request | Effect |
|---|---|
| `POST /benchmark/heap` | Starts sampling. Repeating it is a no-op. Answers 204. |
| `GET /benchmark/heap` | Answers a gzipped pprof `Profile` with sample types `inuse_objects` (count) and `inuse_space` (bytes), symbolized function names, files and lines, and period type `space`/bytes with period 524288. Before sampling starts, the profile has no samples. |

Allocations made before sampling starts are not tracked, which matches how
the samply CPU profile attaches only after fixtures load. A profile therefore
shows memory the measured steps allocated and still hold, not idle server
memory.

**Harness.** `--profile` already runs samply. It now also:

1. starts heap sampling once the workload is prepared, at the point where
   samply starts;
2. saves the profile after each measured step's last repetition, as
   `profiles/heap-<step>.pb.gz` under the run's artifacts.

Saved heap profiles are listed in the record's `artifacts.profile.captures`
beside the samply captures. A failed start or capture becomes a note. A
workload whose server serves no heap profile captures none and adds no note.
As with CPU profiles, a heap profile never changes a measurement, criterion
or verdict. Runs with `--profile` already write to `target/benchmarks/`,
never to history.

Example:

```bash
pprof -top profiles/heap-buffered-64kib-capture-metadata.pb.gz
pprof -top -base profiles/heap-buffered-1kib-capture-metadata.pb.gz \
  profiles/heap-buffered-64kib-capture-metadata.pb.gz
```

The first command shows what the server still holds after that step. The
second shows only what changed since the earlier step.

## Requirements

- **REQ-001:** each repetition records its window's opening resident memory
  next to its peak.
- **REQ-002:** a `bench` build samples live heap allocations only after
  `POST /benchmark/heap`, through the existing `CountingMiMalloc` wrapper,
  which still delegates every allocation to mimalloc.
- **REQ-003:** `GET /benchmark/heap` returns a self-contained, symbolized,
  gzipped pprof profile of the live sampled allocations. Reading it needs
  neither the server binary nor a running server.
- **REQ-004:** `--profile` captures one heap profile per measured step and
  records each one's path or the reason it is missing.

## Invariants and non-goals

- **INV-001:** production builds and unprofiled benchmark runs behave as
  before; records from them differ only by the opening-memory field.
- **INV-002:** heap profiling is diagnostic only and never changes a verdict.
- **INV-003:** the endpoints exist only on the loopback benchmark listener of
  a `bench` build. They expose no tenant, request identity or payload, only
  aggregate allocation sizes and code locations.
- **INV-004:** every supported platform uses the same code path. Stacks come
  from the `backtrace` crate, without `/proc` or platform gating.

Out of scope: changing the allocator, allocation counts (`alloc_*` sample
types), heap profiles of driver or worker processes, memory criteria or
targets, history or results-page changes, and production heap profiling.

## Expensive-to-reverse decisions

- The new optional dependencies `backtrace` (stack capture and symbolization)
  and `prost` (pprof encoding, already a workspace dependency) are enabled
  only by the `bench` feature.
- The opening-memory field is added to the schema-3 record with a default.
  Old records stay readable, so the schema version does not change.

## Acceptance

- **AC-001:** a unit test shows that a repetition window records an opening
  reading and a peak no lower than it, and that a schema-3 record without
  the opening field still reads.
- **AC-002:** server unit tests show that, before the start request, the
  profile has no samples. After it, a live allocation of a few MiB is
  attributed to the allocating function with roughly its size, and freeing
  it removes it. Sampler allocations are never sampled. Non-GET/POST methods
  are refused, and a build or listener without benchmark snapshots serves no
  heap route.
- **AC-003:** profiler unit tests show that captures and failures become
  notes and captures, and that a workload without a heap endpoint adds
  neither.
- **AC-004:** a Gateway L1 `--profile` run saves one heap profile per step,
  and `pprof -top` lists symbolized Wyrd frames.
- **AC-005:** an unprofiled Gateway L1 run from a `bench` build stays within
  the last benchmark-records entry's repetition range, showing that disabled
  sampling costs nothing measurable.

## Decisions and approval

The maintainer approved putting sampling inside `CountingMiMalloc` under the
`bench` feature, keeping mimalloc, and using pprof as the standard heap
profile format. Approved 2026-11-06 by the maintainer ("ok do it").

## Authority

[Benchmarks README](../../../benchmarks/README.md#profiling) owns profiling
and records. `crates/wyrd/wyrd-server/src/main.rs` owns the allocator.
`crates/wyrd/wyrd-server/src/app/metrics.rs` owns the loopback benchmark
listener, and `crates/wyrd/wyrd-testing/src/profile.rs` owns diagnostic
captures.

## Revision history

- Revision 1, 2026-11-06: first draft.
