# UI benchmarking and profiling

Status: exploring — not approved for implementation.

## Problem

Nothing measures what the UI costs a user. The query audit and the planned UI
SQL L1 benchmark (`../../github-verifier-pipelines/design/ui-hot-read-latency.md`)
measure Bifrost execution. `src/lib/ui-library.integration.test.ts:474` times
an executor batch to its last result, not paint or interaction. No UI
benchmark, timing marks, browser profile, or bundle measurement exists, so a
slow page cannot be attributed to the query, the Node host, decoding,
hydration, or rendering.

## Desired outcome

The UI is benchmarked and profiled like `wyrd-server`: an opt-in experiment
asks one declared question of a production build, records a judged result in
`benchmarks/history/`, and a separate profiling run explains where the time
went.

## How `wyrd-server` is benchmarked today

`benchmarks/README.md`: prebuilt release binaries, a quiet host, fixed
fixtures, discarded warmup, repeated measurement windows, population
reconciliation. The `wyrd-benchmark` recorder captures boundary latency,
excluded waits and residual time, throughput, CPU, RSS, counters and
histograms. A run is VALID or INVALID; performance criteria report
met/missed/unsettled separately. Records are append-only JSON under
`benchmarks/history/<engine>/<experiment>/`, raw artifacts under
`target/benchmarks/`, rendered by `bench:results`. `--profile` runs samply and
the native heap sampler and stays out of baseline history. Benchmarks are
evidence, not CI.

## Industry practice for web UIs

- Measure production output, not dev mode (SvelteKit's performance guide).
- Fix the browser and version, viewport, CPU and network throttling, fixture
  size, and cache state; separate cold load, warm navigation, and
  interactions.
- Measure user-visible time: TTFB, FCP, LCP, CLS, interaction to visible
  result, long tasks, and transferred JavaScript.
- Drive realistic journeys with Playwright. Lighthouse audits page loads but
  cannot measure interactions (INP); do not judge on its aggregate score.
- Repeat runs, keep raw samples, compare against comparable baselines, and set
  numeric budgets only once repeatability is known.
- Profile browser JavaScript, layout and paint with a Chrome Performance
  trace; profile Node with V8 `--cpu-prof` and heap profiles. Playwright
  traces explain actions and network activity but are not CPU profiles.

## Reuse, adapt, or replace

| `wyrd-server` pattern | UI decision |
|---|---|
| Declared question, fixed fixtures, quiet host, warmup, repetitions | Reuse. Add browser version, viewport, throttling and cache state as recorded controls. |
| Boundary timings | Adapt. Browser `performance.mark`/`measure` at loader, executor and render boundaries; Node timings on the host. Browser and Node clocks stay separate. |
| `wyrd-benchmark` recorder and residuals | Replace for UI collection with browser and Node Performance APIs. Keep the Rust recorder for backend attribution. |
| L1 latency experiments | Adapt to navigation and interaction journeys. |
| L2 concurrency sweeps | Only for Node host capacity; concurrency says nothing about rendering responsiveness. |
| Verdicts, criteria, history, reports | Reuse the semantics. Extend the shared record and report contract for UI units. |
| samply and native heap profiling | Replace with Chrome Performance traces for the browser and V8 CPU/heap profiles for Node. |

## Proposed implementation

1. Extend the existing production-build Playwright journeys
   (`src/lib/ui-pages.integration.test.ts:39`) into an opt-in experiment:
   `mise run bench:l1:ui`, plus `--profile browser|cpu|heap`.
2. First experiment: evidence browsing against fixed seeded data. One cold
   load, then warm filtering, paging, and opening a detail. Each step asserts
   the correct visible result; a wrong result makes the run INVALID.
3. Record raw repetitions: loading and interaction timings, long tasks,
   request and transfer bytes, Node host CPU and RSS, correctness, build
   identity, and every control. Records go under
   `benchmarks/history/ui/<experiment>/`, profiles under
   `target/benchmarks/ui/`.
4. Compare compatible full runs on the same host. Set budgets after
   repeatability is measured.

## Skipped

Lighthouse CI, component microbenchmarks, and production real-user telemetry.
Add each when a concrete question needs it.

## Open questions

- Should the UI experiment live in the `capacity` driver beside the other
  engines, or as a Node script that writes the same record format?
- Which tasks own it: a TASK-004B follow-up or a new task in this packet?
