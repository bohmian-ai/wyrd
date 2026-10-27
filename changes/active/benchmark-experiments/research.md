# Benchmark experiment research

Supporting evidence for spec revision 5. Historical sources were inspected
2026-10-23; measurement and platform decisions were revised 2026-10-24.
This file records current behavior and provenance. REQ-011 incorporates only
the baseline mapping's selected values, source identities and eligibility
statuses as normative data; the remaining research adds no requirements.
No benchmarks, tests, builds, or profilers were run. The original investigation did not use CodeGraph. This checkout now has
.codegraph/; revision 4 used CodeGraph before inspecting current timing owners. No nested `AGENTS.md`
was found in the two inspected benchmark owner trees.

## Original owners and complete workflows

This inventory describes the pre-implementation investigation. It includes
paths since changed or removed in the rejected candidate; it does not claim
those files still exist. Current revision-4 timing owners are identified below.

| Inspected owner / consumers | Current behavior and gap |
|---|---|
| `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs`, `query/mod.rs`, `query/run.rs`, `query/report.rs` | Public-client fixture loading and query/write sweeps against a release server, exact-result checking, resource/telemetry windows, local samples/profiles and report. Preserve workload orchestration. Query still owns `Measured::percentile_ms` (line 175) and `Usage` (line 211), with its own report and judgment. No common experiment declaration or historical recorder. |
| `capacity/verification/{mod,run,report,workload}.rs` | Direct verify and background submit through public client observation flows. Direct overhead subtracts Verifier blocks; background case is documented as reported only. Own report; no shared per-experiment persisted declaration. |
| `capacity/gateway/{mod,capacity,envelope,report,run,workload}.rs` | Public Gateway requests with local upstream, captures, open-loop arrivals, capacity envelope and recovery. `Report`/`Row` already handle general rows and richer judgments, making them a useful base; today rows and samples still depend on engine-specific formatting. `Slo` has optional fields and reads required `--max-*-ms` flags. Release-run inputs are 200/350/50/1100 ms for answer/first-token/chunk-lag/stream; D-2 originally adopted these constants; revision 5 re-expresses completion/token/stream at server boundaries and makes chunk lag diagnostic-only. |
| `capacity/client_queue/{mod,step}.rs` | Provisions release server and tenant, emits observations, collects queue/drain/row integrity. `Record` carries a gating distinction; non-gating failures render as `report:`. `Readings::quantile` at `step.rs:147` duplicates bucket-delta percentile work. |
| `bin/bifrost_forge_capacity/{host,report}.rs` | Separate Host and load sampler; `HostLoad::qualifies` requires eight free CPUs after sampling CPU and load average. Separate Forge report. Existing scoped/systemd workflow must not be copied into the portable experiment contract. |
| `src/capacity.rs` | Shared Percentiles, Histogram, Usage, Window, Resources/ResourceWindow, release binary identity and telemetry helpers exist. ResourceWindow currently accesses scoped CPU/peak-memory operations; reuse meanings and portable owners, not cgroup mechanics. |
| `src/release_server.rs`, `src/release_server/os.rs` | Existing complete release-server launch/provision/drain workflow and Host/Placement. Host placement has shared `sysinfo` process CPU and sampled RSS readings, with a 10 ms peak sampler. Scoped placement and envelopes also exist and conflict with the new platform constraint; they are not a permissible new benchmark requirement. Missing CPU currently may return zero: required experiment validity must detect this. |
| `src/profile.rs` | Existing PID-attached perf capture, artifact metadata and explicit incomplete capture reporting. Replace direct perf commands with optional samply diagnostics in target; preserve attribution and incomplete-capture notes, never sampling as CPU-seconds. |
| `capacity/{query,verification,gateway}/report.rs` | Three `MEMORY_HEADROOM_BYTES` constants, each 1 GiB, and three comparable Report implementations. Client-queue and Forge have additional report paths; “three reports” is not a complete inventory of every writer. |
| `capacity/flags.rs` and workload parsing | Shared value/list/client/known flag validation already exists and Gateway/Verification consume it; query has separate parsing and client-queue uses a clap Cli. The supplied “four flag parsers” observation is not four wholly isolated implementations at HEAD. Extend/reuse the existing shared owner rather than introduce a fifth parser. |
| `mise.toml:536–620` | Seven old entry points: Bifrost query-capacity, write-capacity, client-queue, forge-capacity; Verification capacity; Gateway overhead, capacity. Descriptions say opt-in and no gate; Forge still describes Linux/systemd envelopes. Migration replaces these with the level commands, without aliases. |
| `benches/thresholds.toml`, `crates/shared/wyrd-bench/src/slo.rs`, `src/lib.rs` | Ten threshold keys; references found only in SloGate implementation/path lookup and its tests (plus export). None names a live capacity experiment. Delete the dead owner, not unrelated recorder/measurement primitives still exported by wyrd-bench. |
| `crates/wyrd/wyrd-testing/src/load/`, `src/lib.rs`, `mise.toml:348` | Exported legacy harness with inline self-tests; no external `wyrd_testing::load` consumer found. A test-lane expression still selects `load::`; retirement must remove stale references while preserving the rest of that lane, not widen its selection. |

The reuse boundary is the whole benchmark workflow, not only percentile leaf
calls. Existing engine workload setup remains useful; declaration, validity,
repetitions, shared accounting, criteria, record and rendering are the missing
common workflow. No new production runtime or replacement client is justified.

## OS-agnostic benchmarks (maintainer portability decision)

Original source checked 2026-10-23. These are pre-implementation paths; some
are now removed or changed in the rejected uncommitted implementation. The
portability decision removes prohibited mechanisms rather than treating them
as authority. No acceptance runs have
been performed. Paths below identify the original inspected source, not approved implementation.

| Verified current mechanism | Required replacement |
|---|---|
| `profile.rs:108–145` spawns `perf record` for CPU/context-switch captures; `185–215` runs `perf report`; `243–277` runs `perf script`. | Remove direct perf commands and their processing. Optional `--profile` uses external samply with identical invocation on every platform. Missing/unusable capture records a diagnostic note only; profiles never enter a verdict or make a run INVALID or FAIL. |
| `release_server.rs:426–444` invokes `systemd-run --user --scope` with CPUQuota, MemoryMax and MemorySwapMax; `564–606` reads cgroup CPU/memory; `691–738` reads `/proc/<pid>/cgroup`, resolves `/sys/fs/cgroup`, and checks `cpu.max`/`memory.max`. | Remove scoped placement, cgroup discovery, limit enforcement and accounting. Plain child servers have no OS resource limits; only Wyrd configuration supplies server-side limits. Report provisioned-vCPU throughput alongside efficiency normalized by measured server CPU-seconds. |
| `bifrost_forge_capacity/main.rs:72–78` sets leader quota 100% and worker quota 233% (documented as 7/3 CPU, rounded to 2.33); `120–125` defaults both memory envelopes to 4 GiB; `179–185` builds envelopes and `240` selects scoped leader placement. `deployment.rs:309–319` passes the worker envelope to `start_forge_worker`; `release_server.rs:378–395` selects scoped worker placement. | Remove leader/worker systemd user-scope envelopes and quota accounting. Preserve Forge workload coverage with plain child processes and both throughput normalizations across declared server processes. |
| `release_server/os.rs:55–70` uses sysinfo accumulated CPU time; `85–104` samples resident memory (10 ms interval declared at line 20). `bifrost_forge_capacity/host.rs:72–96` samples host/process CPU through sysinfo. `Cargo.toml:233` declares sysinfo 0.39.6; `wyrd-testing/Cargo.toml:121` consumes it. | Reuse sysinfo for CPU time, sampled peak memory, quiet-host and driver-saturation checks on all supported platforms. Zero-on-missing in the current helper must not admit missing accounting. A sampled RSS peak is not an exact OS high-water mark. |

The samply upstream README,
checked 2026-10-23, documents macOS/Linux support, `samply record`, and Linux
perf-events sampling; macOS uses its native backend. On/off-CPU diagnostic
detail can differ by OS. Linux event access and macOS attach setup may need
external preparation; the runner does not modify OS permissions or read
`/proc` to arrange it. Unavailable profiling is recorded and skipped.

Both levels and every experiment must run unchanged on macOS arm64, Linux
x86_64 and Linux arm64 using identical commands and record schema. Host records
name OS and architecture. Cross-OS comparison follows the advisory cross-host
rule with both fingerprints visible, never same-host regression proof. AC-005
requires real CPU accounting and every L1/L2 experiment only on Linux x86_64.
Real macOS arm64 and Linux arm64 runs are optional. Source/command inspection
must still confirm unchanged portability for every experiment on both platforms;
it does not substitute for required Linux x86_64 runs.

## Latency measurement revision

The rejected candidate derives internal latency tails from production /metrics
bucket deltas and treats a bucket crossing a strict SLO as unresolved. This
cannot recover individual latency values. Changing bucket bounds is not a
benchmark measurement remedy. Raw client request samples already exist and
remain reusable only as diagnostics in revision 5; judged server durations must
be collected before aggregation.

Revision 4 selects raw integer-nanosecond duration samples with empirical
nearest-rank percentiles. No histogram quantization is needed. Collection uses
monotonic request timing and, for server-internal observations, standard
OpenTelemetry request tracing with 100% recording and local OTLP collection.
The OpenTelemetry Tracing API defines individual spans and time values up to
nanosecond precision; the Tracing SDK defines always-on recording and export
processing. Those are collection primitives, not proof of clock accuracy or
zero overhead. Wyrd still needs completeness reconciliation and the declared
same-process boundaries. Source references: OpenTelemetry specification,
Tracing API (Time, Span) and Tracing SDK (Sampling, Span Processor), checked
2026-10-24. Production metrics histograms remain diagnostic for latency.

There is no standard universal Verification or Gateway overhead metric. The
spec retains Wyrd's existing measurable boundaries: arrival-to-result-staging
less Verifier-block interval union, and arrival-to-Gateway handoff less provider
wait to that handoff. Compute residuals per request before percentiles. No
cross-process timestamp subtraction or subtraction of aggregate p99s is valid.

Inspected existing owners: components/gateway/telemetry.rs Profile::point
computes monotonic elapsed and upstream durations per request before recording
metrics. components/verification/service.rs VerificationControl::execute hands arrival
into verification/telemetry.rs ExecutionTelemetry::emit, which computes
block union and the residual before metrics recording.
wyrd-telemetry/src/lib.rs and export.rs own SDK sampling, request tracing and
operator OTLP export. These are extension points, not evidence that all required
raw observations are already emitted. Missing instrumentation is future task
work at those owners, preserving server behavior and public metrics contracts.
Collection loss, incomplete matching or inconsistent components is INVALID;
if standard tracing cannot expose the boundaries exactly, a new measurable
boundary needs a spec revision. No custom timing endpoint/protocol is proposed.

## Revision 5 measurement decisions

The maintainer fixes handler entry after receipt through result ready, write
acknowledgment or response handoff as the judged server latency boundary,
using OpenObserve per-query `took` as the analogy. This is a maintainer boundary
decision, not a newly verified upstream implementation claim. Client request
samples, visibility polling and inter-chunk lag cannot supply that boundary and
are diagnostic-only. Keep scheduled sends/send-lateness only for driver validity;
remove scheduled-time-to-completion judgment. Server elapsed minus that request's
own Verifier/provider wait union remains the traced overhead definition.

Report both (W/T)/P (per provisioned vCPU) and W/C (per CPU-second actually
used), recording P as logical CPUs available to the server process scope and
counting shared-host CPUs once. Historical “per-core” evidence below is W/C;
regression remains W/C and C/W. Historical decimal MB/CPU-s values must be
converted to MiB/CPU-s by ×10^6/2^20, preserving the original operands and
eligibility; they do not establish provisioned-vCPU baselines. Write reports
require rows and uncompressed MiB in both normalizations. Queries additionally
need executed-plan scanned rows/bytes per query alongside server query time.

At a 5% scan share, 1,000 heavy scans require at least 20,000 total queries in
each six-minute repetition. Select p95 with ≥200 samples instead: at least
4,000 total queries, or 11.12 queries/s overall (0.556 scans/s), with actual
completed samples checked. This preserves the original mixture/window and
removes the infeasible p99 obligation; it does not waive insufficient samples.
No benchmark was run to assert achieved rate or instrumentation feasibility.

## Historical baseline provenance (D-3)

Revision 2 corrects revision 1's claim that local reports were not found.
Five requested originals exist and have been copied byte-for-byte below. Only
`report.md` files were copied; no samples, logs, profiles or fabricated records.
The requested Gateway `target/verification-capacity/report.md` does not exist.
All reports identify AMD Ryzen 9 9950X, 32 logical CPUs, 91 GiB, Linux. The host
count does not supply the CPU-used baseline denominator C. Revision 5 separately
requires P, available provisioned vCPUs, for reported throughput (W/T)/P; do not
reinterpret these historical W/C values as provisioned throughput. Times below are EDT (UTC−04:00).

| Preserved report | Original mtime, 2026-10-21 | SHA-256 |
|---|---|---|
| bifrost-source-consistency/bifrost-query-capacity/standard/hot/report.md | 15:13:50 | `d09e2b8e4401ad87c08fa5dde17fd50affef80b6850cc9158590ba6f872f3027` |
| bifrost-source-consistency/bifrost-query-capacity/standard/parquet/report.md | 15:21:08 | `f757e4a9a572b723113036256c02e95bcc7d4b1174a68f27be06e61fa50807d6` |
| bifrost-source-consistency/bifrost-query-capacity/writes/parquet/report.md | 15:02:40 | `f3f337ceb8bc6c63fd33e2d0d2d5d4faca53ac68570f6f090f21b4b8e8480200` |
| bifrost-source-consistency/verification-capacity/report.md | 15:07:37 | `37ae97b069c1e94fe8e01a1447346ef8bde7f5f49cc5011a4342e597be2caf29` |
| gateway-capacity/gateway-overhead/report.md | 16:05:04 | `7e70fb359234e9cfc22a7ba952190b3295fa94cb8fe790920f7e586fb2b29882` |

Read-only provenance checks: report titles and Gateway binary identity;
`git -C <worktree> status --short`; worktree-local reflogs and git log around
15:00–16:10; and the committed capacity evidence at `051be28a3`. Both worktrees
have empty current status output. This proves current cleanliness, not a clean
measurement-time tree or correspondence of an old binary to that tree.

- **Bifrost worktree:** HEAD became
  `05186c1e443096e6c33d6b5f08831b613fa2caa2` at 14:57:56, then
  `051be28a35d4871f54329dfdf531243aa5c1445c` at 15:21:46 (evidence-only), then
  `0d3223696` at 15:40:36 (verification evidence). The four report mtimes fall
  before the evidence commit. Its TASK-001 AC-006 explicitly names a quiet-host
  sequence at `05186c1e4` and matching hot peak 8,178 qps, Parquet peak 7,411 qps
  and table aggregate 56.9 qps. Thus source attribution is established by
  contemporaneous committed evidence, rather than inferred solely from mtime.
  Titles contain fixture/host identity but no binary SHA or clean-tree stamp.
  No contemporaneous clean-status/diff attestation was found. All four are
  preserved but excluded from clean baselines pending that evidence.
- **Gateway worktree:** reflog has merge `25bf69d05` at 15:41:48,
  `993bd7d52dc7c3765ad5f59349b469a74b6e852a` at 15:49:36 (refresh-journey test),
  then `0cf04b55d` at 16:06:06 (revision-5 approval). Report mtime is 16:05:04.
  Its title records `target/profiling/release/wyrd-server`, 1,202,857,136 bytes,
  binary modified `2026-10-21T19:53:37.330016786Z` (15:53:37 EDT), 2-second
  warmup and 10-second windows, profiling at 999 Hz. `993bd7d52` is the
  contemporaneous HEAD candidate, not a proven binary source SHA; the title
  has no commit/dirty identity. Source commit and historical cleanliness remain
  unestablished, so exclude it. Its 64-client rows also lose accepted captures;
  none can be presented as proof of today's zero-loss requirement.

The open evidence question is whether contemporaneous clean-source/build
attestations can be recovered. No human decision has been invented to waive
clean-source eligibility. If recovered, select primary values per metric;
until then the mappings below govern fallback eligibility. Verification and
Gateway have no eligible historical fallback in PR 150 and no invented entry.

The clean fallback is PR 150 pre-merge `cfe21dc90`, preserved in Git object
`888fc673ed9598a7dc218b27be67640e85e6242d` under
`changes/active/bifrost-variant/benchmarks/pre-merge/`. Read its README, all
four read reports, all three write reports, and client-queue JSON/text using:

```text
git show 888fc673ed9598a7dc218b27be67640e85e6242d:changes/active/bifrost-variant/benchmarks/pre-merge/<file>
```

The README explicitly identifies clean `cfe21dc90` for reads, native/spans
writes and client queue; the maintainer confirms this complete fallback is
clean, including logs. Exclude merged `5c775110f` telemetry: its uncommitted
DataFusion fork patch cannot be attributed to the commit. Its standard reads
are superseded by the primary-era reports and are not selected as fallback.

## Eligible and excluded baseline mapping

These tables specify future `baselines.toml` entries; no TOML is written here.
Each eligible entry retains engine, experiment/step, client count, signal/tier,
fixture, source file and commit, host, metric/unit/direction and operands.
Use only matching controls; differences in row shape, limits, build, profiling,
publication or CPU scope must be explicit. Unknown settings stay unknown and
can prevent a particular comparison. Historical reports lack the new three
repetitions and validity checks: eligibility does not retrofit a new-format
PASS or assert compliance with new SLOs. A valid historical FAIL can supply
measured efficiency, but never proof that its failed correctness criterion met.

For query rows, compute qps/core = printed qps / measured server cores and
CPU ms/query = 1,000 × measured cores / qps. Write rows analogously use rows/s
and CPU µs/row = 1,000,000 × cores / rows/s. Prefer explicitly printed per-core
byte/row values over ratios of rounded display columns. Approximate derived
values below retain the original printed operands; do not claim extra precision.
Query CPU entries below use µs/query; divide by 1,000 for the required ms/query metric.
Only these CPU-used W/C and C/W quantities are baseline criteria. Latency,
absolute throughput, loss counts, backlog and drain are source context.

### Primary candidates, excluded pending provenance

All following references are the preserved reports identified above. Values are
candidates only; status is **excluded** for the report-level reasons above.

| Metric / step | Source | Per-core / per-operation value | Status/reason |
|---|---|---|---|
| write events, clients 4 | `bifrost-source-consistency/bifrost-query-capacity/standard/hot/report.md` | ≈2093162.069 rows/CPU-s; ≈0.478 CPU µs/row (rate 1821051, cores 0.87); 734.2 MB/CPU-s | Excluded: historical clean state unestablished |
| selective sweep, clients 1 | `bifrost-source-consistency/bifrost-query-capacity/standard/hot/report.md` | ≈582.137 queries/CPU-s; ≈1717.809 CPU µs/query (rate 681.1, cores 1.17) | Excluded: historical clean state unestablished |
| selective sweep, clients 4 | `bifrost-source-consistency/bifrost-query-capacity/standard/hot/report.md` | ≈559.310 queries/CPU-s; ≈1787.919 CPU µs/query (rate 2349.1, cores 4.2) | Excluded: historical clean state unestablished |
| selective sweep, clients 8 | `bifrost-source-consistency/bifrost-query-capacity/standard/hot/report.md` | ≈477.275 queries/CPU-s; ≈2095.228 CPU µs/query (rate 3923.2, cores 8.22) | Excluded: historical clean state unestablished |
| selective sweep, clients 16 | `bifrost-source-consistency/bifrost-query-capacity/standard/hot/report.md` | ≈373.487 queries/CPU-s; ≈2677.471 CPU µs/query (rate 6084.1, cores 16.29) | Excluded: historical clean state unestablished |
| selective sweep, clients 32 | `bifrost-source-consistency/bifrost-query-capacity/standard/hot/report.md` | ≈296.327 queries/CPU-s; ≈3374.645 CPU µs/query (rate 7746.0, cores 26.14) | Excluded: historical clean state unestablished |
| selective sweep, clients 64 | `bifrost-source-consistency/bifrost-query-capacity/standard/hot/report.md` | ≈280.845 queries/CPU-s; ≈3560.686 CPU µs/query (rate 8178.2, cores 29.12) | Excluded: historical clean state unestablished |
| small-aggregate sweep, clients 1 | `bifrost-source-consistency/bifrost-query-capacity/standard/hot/report.md` | ≈302.269 queries/CPU-s; ≈3308.312 CPU µs/query (rate 359.7, cores 1.19) | Excluded: historical clean state unestablished |
| small-aggregate sweep, clients 4 | `bifrost-source-consistency/bifrost-query-capacity/standard/hot/report.md` | ≈314.125 queries/CPU-s; ≈3183.449 CPU µs/query (rate 1309.9, cores 4.17) | Excluded: historical clean state unestablished |
| small-aggregate sweep, clients 8 | `bifrost-source-consistency/bifrost-query-capacity/standard/hot/report.md` | ≈274.085 queries/CPU-s; ≈3648.498 CPU µs/query (rate 2247.5, cores 8.2) | Excluded: historical clean state unestablished |
| small-aggregate sweep, clients 16 | `bifrost-source-consistency/bifrost-query-capacity/standard/hot/report.md` | ≈204.771 queries/CPU-s; ≈4883.513 CPU µs/query (rate 3348.0, cores 16.35) | Excluded: historical clean state unestablished |
| small-aggregate sweep, clients 32 | `bifrost-source-consistency/bifrost-query-capacity/standard/hot/report.md` | ≈137.972 queries/CPU-s; ≈7247.866 CPU µs/query (rate 3795.6, cores 27.51) | Excluded: historical clean state unestablished |
| small-aggregate sweep, clients 64 | `bifrost-source-consistency/bifrost-query-capacity/standard/hot/report.md` | ≈133.092 queries/CPU-s; ≈7513.615 CPU µs/query (rate 3874.3, cores 29.11) | Excluded: historical clean state unestablished |
| 1m-aggregate, clients 8 | `bifrost-source-consistency/bifrost-query-capacity/standard/hot/report.md` | ≈110.558 queries/CPU-s; ≈9045.057 CPU µs/query (rate 892.2, cores 8.07) | Excluded: historical clean state unestablished |
| table-aggregate, clients 1 | `bifrost-source-consistency/bifrost-query-capacity/standard/hot/report.md` | ≈16.382 queries/CPU-s; ≈61044.177 CPU µs/query (rate 24.9, cores 1.52) | Excluded: historical clean state unestablished |
| broad-window, clients 1 | `bifrost-source-consistency/bifrost-query-capacity/standard/hot/report.md` | ≈14.058 queries/CPU-s; ≈71134.021 CPU µs/query (rate 19.4, cores 1.38) | Excluded: historical clean state unestablished |
| full-scan, clients 1 | `bifrost-source-consistency/bifrost-query-capacity/standard/hot/report.md` | ≈27.310 queries/CPU-s; ≈36616.162 CPU µs/query (rate 39.6, cores 1.45) | Excluded: historical clean state unestablished |
| small-aggregate while writing, clients 4 | `bifrost-source-consistency/bifrost-query-capacity/standard/hot/report.md` | ≈171.066 queries/CPU-s; ≈5845.691 CPU µs/query (rate 930.6, cores 5.44) | Excluded: historical clean state unestablished |
| write during small-aggregate, clients 4 | `bifrost-source-consistency/bifrost-query-capacity/standard/hot/report.md` | ≈317925.463 rows/CPU-s; ≈3.145 CPU µs/row (rate 1373438, cores 4.32); 111.3 MB/CPU-s | Excluded: historical clean state unestablished |
| 1m-aggregate while writing, clients 4 | `bifrost-source-consistency/bifrost-query-capacity/standard/hot/report.md` | ≈70.578 queries/CPU-s; ≈14168.734 CPU µs/query (rate 403.0, cores 5.71) | Excluded: historical clean state unestablished |
| write during 1m-aggregate, clients 4 | `bifrost-source-consistency/bifrost-query-capacity/standard/hot/report.md` | ≈271997.036 rows/CPU-s; ≈3.677 CPU µs/row (rate 1376305, cores 5.06); 95.1 MB/CPU-s | Excluded: historical clean state unestablished |
| write events, clients 4 | `bifrost-source-consistency/bifrost-query-capacity/standard/parquet/report.md` | ≈922808.537 rows/CPU-s; ≈1.084 CPU µs/row (rate 1513406, cores 1.64); 323.3 MB/CPU-s | Excluded: historical clean state unestablished |
| selective sweep, clients 1 | `bifrost-source-consistency/bifrost-query-capacity/standard/parquet/report.md` | ≈441.983 queries/CPU-s; ≈2262.532 CPU µs/query (rate 512.7, cores 1.16) | Excluded: historical clean state unestablished |
| selective sweep, clients 4 | `bifrost-source-consistency/bifrost-query-capacity/standard/parquet/report.md` | ≈469.587 queries/CPU-s; ≈2129.529 CPU µs/query (rate 1934.7, cores 4.12) | Excluded: historical clean state unestablished |
| selective sweep, clients 8 | `bifrost-source-consistency/bifrost-query-capacity/standard/parquet/report.md` | ≈436.858 queries/CPU-s; ≈2289.074 CPU µs/query (rate 3503.6, cores 8.02) | Excluded: historical clean state unestablished |
| selective sweep, clients 16 | `bifrost-source-consistency/bifrost-query-capacity/standard/parquet/report.md` | ≈378.632 queries/CPU-s; ≈2641.087 CPU µs/query (rate 5895.3, cores 15.57) | Excluded: historical clean state unestablished |
| selective sweep, clients 32 | `bifrost-source-consistency/bifrost-query-capacity/standard/parquet/report.md` | ≈305.489 queries/CPU-s; ≈3273.444 CPU µs/query (rate 7252.3, cores 23.74) | Excluded: historical clean state unestablished |
| selective sweep, clients 64 | `bifrost-source-consistency/bifrost-query-capacity/standard/parquet/report.md` | ≈285.027 queries/CPU-s; ≈3508.440 CPU µs/query (rate 7410.7, cores 26) | Excluded: historical clean state unestablished |
| small-aggregate sweep, clients 1 | `bifrost-source-consistency/bifrost-query-capacity/standard/parquet/report.md` | ≈176.471 queries/CPU-s; ≈5666.667 CPU µs/query (rate 210.0, cores 1.19) | Excluded: historical clean state unestablished |
| small-aggregate sweep, clients 4 | `bifrost-source-consistency/bifrost-query-capacity/standard/parquet/report.md` | ≈200.583 queries/CPU-s; ≈4985.479 CPU µs/query (rate 826.4, cores 4.12) | Excluded: historical clean state unestablished |
| small-aggregate sweep, clients 8 | `bifrost-source-consistency/bifrost-query-capacity/standard/parquet/report.md` | ≈192.905 queries/CPU-s; ≈5183.892 CPU µs/query (rate 1547.1, cores 8.02) | Excluded: historical clean state unestablished |
| small-aggregate sweep, clients 16 | `bifrost-source-consistency/bifrost-query-capacity/standard/parquet/report.md` | ≈169.854 queries/CPU-s; ≈5887.394 CPU µs/query (rate 2683.7, cores 15.8) | Excluded: historical clean state unestablished |
| small-aggregate sweep, clients 32 | `bifrost-source-consistency/bifrost-query-capacity/standard/parquet/report.md` | ≈139.745 queries/CPU-s; ≈7155.881 CPU µs/query (rate 3564.9, cores 25.51) | Excluded: historical clean state unestablished |
| small-aggregate sweep, clients 64 | `bifrost-source-consistency/bifrost-query-capacity/standard/parquet/report.md` | ≈132.557 queries/CPU-s; ≈7543.928 CPU µs/query (rate 3841.5, cores 28.98) | Excluded: historical clean state unestablished |
| 1m-aggregate, clients 8 | `bifrost-source-consistency/bifrost-query-capacity/standard/parquet/report.md` | ≈49.677 queries/CPU-s; ≈20130.195 CPU µs/query (rate 399.4, cores 8.04) | Excluded: historical clean state unestablished |
| table-aggregate, clients 1 | `bifrost-source-consistency/bifrost-query-capacity/standard/parquet/report.md` | ≈18.779 queries/CPU-s; ≈53251.318 CPU µs/query (rate 56.9, cores 3.03) | Excluded: historical clean state unestablished |
| broad-window, clients 1 | `bifrost-source-consistency/bifrost-query-capacity/standard/parquet/report.md` | ≈8.673 queries/CPU-s; ≈115298.507 CPU µs/query (rate 26.8, cores 3.09) | Excluded: historical clean state unestablished |
| full-scan, clients 1 | `bifrost-source-consistency/bifrost-query-capacity/standard/parquet/report.md` | ≈1.538 queries/CPU-s; ≈650000.000 CPU µs/query (rate 14.4, cores 9.36) | Excluded: historical clean state unestablished |
| small-aggregate while writing, clients 4 | `bifrost-source-consistency/bifrost-query-capacity/standard/parquet/report.md` | ≈106.778 queries/CPU-s; ≈9365.198 CPU µs/query (rate 669.5, cores 6.27) | Excluded: historical clean state unestablished |
| write during small-aggregate, clients 4 | `bifrost-source-consistency/bifrost-query-capacity/standard/parquet/report.md` | ≈314741.829 rows/CPU-s; ≈3.177 CPU µs/row (rate 1617773, cores 5.14); 110.0 MB/CPU-s | Excluded: historical clean state unestablished |
| 1m-aggregate while writing, clients 4 | `bifrost-source-consistency/bifrost-query-capacity/standard/parquet/report.md` | ≈29.243 queries/CPU-s; ≈34196.332 CPU µs/query (rate 185.4, cores 6.34) | Excluded: historical clean state unestablished |
| write during 1m-aggregate, clients 4 | `bifrost-source-consistency/bifrost-query-capacity/standard/parquet/report.md` | ≈280032.943 rows/CPU-s; ≈3.571 CPU µs/row (rate 1436569, cores 5.13); 97.9 MB/CPU-s | Excluded: historical clean state unestablished |
| write x1, clients 1 | `bifrost-source-consistency/bifrost-query-capacity/writes/parquet/report.md` | ≈854827.419 rows/CPU-s; ≈1.170 CPU µs/row (rate 529993, cores 0.62); 299.1 MB/CPU-s | Excluded: historical clean state unestablished |
| write x4, clients 4 | `bifrost-source-consistency/bifrost-query-capacity/writes/parquet/report.md` | ≈785429.348 rows/CPU-s; ≈1.273 CPU µs/row (rate 1445190, cores 1.84); 274.7 MB/CPU-s | Excluded: historical clean state unestablished |
| write x16, clients 16 | `bifrost-source-consistency/bifrost-query-capacity/writes/parquet/report.md` | ≈855301.759 rows/CPU-s; ≈1.169 CPU µs/row (rate 3404101, cores 3.98); 298.8 MB/CPU-s | Excluded: historical clean state unestablished |
| write x64, clients 64 | `bifrost-source-consistency/bifrost-query-capacity/writes/parquet/report.md` | ≈774668.127 rows/CPU-s; ≈1.291 CPU µs/row (rate 3183886, cores 4.11); 270.8 MB/CPU-s | Excluded: historical clean state unestablished |
| 1 direct verify 1, clients 1 | `bifrost-source-consistency/verification-capacity/report.md` | ≈4122.034 verifications/CPU-s; ≈242.599 CPU µs/verification (rate 4864, cores 1.18) | Excluded: historical clean state unestablished |
| 1 direct verify 4, clients 4 | `bifrost-source-consistency/verification-capacity/report.md` | ≈3520.343 verifications/CPU-s; ≈284.063 CPU µs/verification (rate 14363, cores 4.08) | Excluded: historical clean state unestablished |
| 1 direct verify 16, clients 16 | `bifrost-source-consistency/verification-capacity/report.md` | ≈2504.737 verifications/CPU-s; ≈399.244 CPU µs/verification (rate 33313, cores 13.3) | Excluded: historical clean state unestablished |
| 1 direct verify 64, clients 64 | `bifrost-source-consistency/verification-capacity/report.md` | ≈2398.402 verifications/CPU-s; ≈416.944 CPU µs/verification (rate 43531, cores 18.15) | Excluded: historical clean state unestablished |
| 2 background submit 1, clients 1 | `bifrost-source-consistency/verification-capacity/report.md` | ≈1773.246 verifications/CPU-s; ≈563.938 CPU µs/verification (rate 4043, cores 2.28) | Excluded: historical clean state unestablished |
| 2 background submit 4, clients 4 | `bifrost-source-consistency/verification-capacity/report.md` | ≈6126.721 verifications/CPU-s; ≈163.219 CPU µs/verification (rate 15133, cores 2.47) | Excluded: historical clean state unestablished |
| 2 background submit 16, clients 16 | `bifrost-source-consistency/verification-capacity/report.md` | ≈8011.111 verifications/CPU-s; ≈124.827 CPU µs/verification (rate 17304, cores 2.16) | Excluded: historical clean state unestablished |
| 2 background submit 64, clients 64 | `bifrost-source-consistency/verification-capacity/report.md` | ≈6006.573 verifications/CPU-s; ≈166.484 CPU µs/verification (rate 12794, cores 2.13) | Excluded: historical clean state unestablished |
| 1 chat 1, clients 1 | `gateway-capacity/gateway-overhead/report.md` | ≈3505.263 requests/CPU-s; ≈285.285 CPU µs/request (rate 4662, cores 1.33) | Excluded: source SHA and historical clean state unestablished |
| 1 chat 4, clients 4 | `gateway-capacity/gateway-overhead/report.md` | ≈3721.951 requests/CPU-s; ≈268.676 CPU µs/request (rate 15260, cores 4.1) | Excluded: source SHA and historical clean state unestablished |
| 1 chat 16, clients 16 | `gateway-capacity/gateway-overhead/report.md` | ≈2702.785 requests/CPU-s; ≈369.989 CPU µs/request (rate 33001, cores 12.21) | Excluded: source SHA and historical clean state unestablished |
| 1 chat 64, clients 64 | `gateway-capacity/gateway-overhead/report.md` | ≈1832.453 requests/CPU-s; ≈545.717 CPU µs/request (rate 38848, cores 21.2) | Excluded: source SHA and historical clean state unestablished |
| 2 chat-stream 1, clients 1 | `gateway-capacity/gateway-overhead/report.md` | ≈2556.250 requests/CPU-s; ≈391.198 CPU µs/request (rate 3681, cores 1.44) | Excluded: source SHA and historical clean state unestablished |
| 2 chat-stream 4, clients 4 | `gateway-capacity/gateway-overhead/report.md` | ≈2632.127 requests/CPU-s; ≈379.921 CPU µs/request (rate 11634, cores 4.42) | Excluded: source SHA and historical clean state unestablished |
| 2 chat-stream 16, clients 16 | `gateway-capacity/gateway-overhead/report.md` | ≈2063.941 requests/CPU-s; ≈484.510 CPU µs/request (rate 27760, cores 13.45) | Excluded: source SHA and historical clean state unestablished |
| 2 chat-stream 64, clients 64 | `gateway-capacity/gateway-overhead/report.md` | ≈1498.135 requests/CPU-s; ≈667.496 CPU µs/request (rate 30532, cores 20.38) | Excluded: source SHA and historical clean state unestablished |
| 3 chat-capture 16, clients 16 | `gateway-capacity/gateway-overhead/report.md` | ≈1004.740 requests/CPU-s; ≈995.283 CPU µs/request (rate 10811, cores 10.76) | Excluded: source SHA and historical clean state unestablished |
| 3 chat-capture 64, clients 64 | `gateway-capacity/gateway-overhead/report.md` | ≈849.146 requests/CPU-s; ≈1177.654 CPU µs/request (rate 8950, cores 10.54) | Excluded: source SHA and historical clean state unestablished |

### Selected fallback entries: Bifrost reads and writes

Source for every row: clean `cfe21dc90`, via the Git object/path above. Standard
read fixtures are 10M rows; telemetry read fixture details remain as reported
in each file. Write sweeps use 10M native rows or 2M signal rows per step.
Higher efficiency is better; lower CPU/operation is better. Query rows do not
supply scanned GB/s/core without measured scan bytes, nor do write rows supply
storage ratios or memory intensity without matching byte/window evidence.

| Metric / step / clients | Source file | Entry values | Status/reason |
|---|---|---|---|
| write events / 4 | `read-standard-hot.md` | ≈1574733.673 rows/CPU-s; ≈0.635 CPU µs/row (rate 1543239, cores 0.98); 442.3 MB/CPU-s | Eligible fallback; primary unavailable or excluded |
| selective sweep / 1 | `read-standard-hot.md` | ≈629.612 queries/CPU-s; ≈1588.281 CPU µs/query (rate 648.5, cores 1.03) | Eligible fallback; primary unavailable or excluded |
| selective sweep / 4 | `read-standard-hot.md` | ≈562.997 queries/CPU-s; ≈1776.207 CPU µs/query (rate 2178.8, cores 3.87) | Eligible fallback; primary unavailable or excluded |
| selective sweep / 8 | `read-standard-hot.md` | ≈458.201 queries/CPU-s; ≈2182.449 CPU µs/query (rate 3514.4, cores 7.67) | Eligible fallback; primary unavailable or excluded |
| selective sweep / 16 | `read-standard-hot.md` | ≈355.453 queries/CPU-s; ≈2813.313 CPU µs/query (rate 5417.1, cores 15.24) | Eligible fallback; primary unavailable or excluded |
| selective sweep / 32 | `read-standard-hot.md` | ≈272.831 queries/CPU-s; ≈3665.270 CPU µs/query (rate 6466.1, cores 23.7) | Eligible fallback; primary unavailable or excluded |
| selective sweep / 64 | `read-standard-hot.md` | ≈270.921 queries/CPU-s; ≈3691.109 CPU µs/query (rate 6851.6, cores 25.29) | Eligible fallback; primary unavailable or excluded |
| small-aggregate sweep / 1 | `read-standard-hot.md` | ≈350.096 queries/CPU-s; ≈2856.358 CPU µs/query (rate 364.1, cores 1.04) | Eligible fallback; primary unavailable or excluded |
| small-aggregate sweep / 4 | `read-standard-hot.md` | ≈322.949 queries/CPU-s; ≈3096.467 CPU µs/query (rate 1259.5, cores 3.9) | Eligible fallback; primary unavailable or excluded |
| small-aggregate sweep / 8 | `read-standard-hot.md` | ≈282.529 queries/CPU-s; ≈3539.461 CPU µs/query (rate 2200.9, cores 7.79) | Eligible fallback; primary unavailable or excluded |
| small-aggregate sweep / 16 | `read-standard-hot.md` | ≈215.987 queries/CPU-s; ≈4629.905 CPU µs/query (rate 3358.6, cores 15.55) | Eligible fallback; primary unavailable or excluded |
| small-aggregate sweep / 32 | `read-standard-hot.md` | ≈144.303 queries/CPU-s; ≈6929.864 CPU µs/query (rate 3819.7, cores 26.47) | Eligible fallback; primary unavailable or excluded |
| small-aggregate sweep / 64 | `read-standard-hot.md` | ≈140.030 queries/CPU-s; ≈7141.346 CPU µs/query (rate 3782.2, cores 27.01) | Eligible fallback; primary unavailable or excluded |
| 1m-aggregate / 8 | `read-standard-hot.md` | ≈116.937 queries/CPU-s; ≈8551.635 CPU µs/query (rate 923.8, cores 7.9) | Eligible fallback; primary unavailable or excluded |
| table-aggregate / 1 | `read-standard-hot.md` | ≈17.310 queries/CPU-s; ≈57768.924 CPU µs/query (rate 25.1, cores 1.45) | Eligible fallback; primary unavailable or excluded |
| broad-window / 1 | `read-standard-hot.md` | ≈14.627 queries/CPU-s; ≈68367.347 CPU µs/query (rate 19.6, cores 1.34) | Eligible fallback; primary unavailable or excluded |
| full-scan / 1 | `read-standard-hot.md` | ≈29.333 queries/CPU-s; ≈34090.909 CPU µs/query (rate 39.6, cores 1.35) | Eligible fallback; primary unavailable or excluded |
| small-aggregate while writing / 4 | `read-standard-hot.md` | ≈195.988 queries/CPU-s; ≈5102.356 CPU µs/query (rate 972.1, cores 4.96) | Eligible fallback; primary unavailable or excluded |
| write during small-aggregate / 4 | `read-standard-hot.md` | ≈346000.249 rows/CPU-s; ≈2.890 CPU µs/row (rate 1387461, cores 4.01); 97.4 MB/CPU-s | Eligible fallback; primary unavailable or excluded |
| 1m-aggregate while writing / 4 | `read-standard-hot.md` | ≈81.818 queries/CPU-s; ≈12222.222 CPU µs/query (rate 414.0, cores 5.06) | Eligible fallback; primary unavailable or excluded |
| write during 1m-aggregate / 4 | `read-standard-hot.md` | ≈341443.641 rows/CPU-s; ≈2.929 CPU µs/row (rate 1369189, cores 4.01); 96.1 MB/CPU-s | Eligible fallback; primary unavailable or excluded |
| write events / 4 | `read-standard-parquet.md` | ≈1068394.521 rows/CPU-s; ≈0.936 CPU µs/row (rate 1559856, cores 1.46); 299.6 MB/CPU-s | Eligible fallback; primary unavailable or excluded |
| selective sweep / 1 | `read-standard-parquet.md` | ≈460.935 queries/CPU-s; ≈2169.505 CPU µs/query (rate 493.2, cores 1.07) | Eligible fallback; primary unavailable or excluded |
| selective sweep / 4 | `read-standard-parquet.md` | ≈440.447 queries/CPU-s; ≈2270.423 CPU µs/query (rate 1775.0, cores 4.03) | Eligible fallback; primary unavailable or excluded |
| selective sweep / 8 | `read-standard-parquet.md` | ≈419.937 queries/CPU-s; ≈2381.311 CPU µs/query (rate 3321.7, cores 7.91) | Eligible fallback; primary unavailable or excluded |
| selective sweep / 16 | `read-standard-parquet.md` | ≈360.159 queries/CPU-s; ≈2776.554 CPU µs/query (rate 5449.2, cores 15.13) | Eligible fallback; primary unavailable or excluded |
| selective sweep / 32 | `read-standard-parquet.md` | ≈296.141 queries/CPU-s; ≈3376.775 CPU µs/query (rate 6660.2, cores 22.49) | Eligible fallback; primary unavailable or excluded |
| selective sweep / 64 | `read-standard-parquet.md` | ≈292.723 queries/CPU-s; ≈3416.199 CPU µs/query (rate 6170.6, cores 21.08) | Eligible fallback; primary unavailable or excluded |
| small-aggregate sweep / 1 | `read-standard-parquet.md` | ≈205.481 queries/CPU-s; ≈4866.635 CPU µs/query (rate 213.7, cores 1.04) | Eligible fallback; primary unavailable or excluded |
| small-aggregate sweep / 4 | `read-standard-parquet.md` | ≈195.757 queries/CPU-s; ≈5108.379 CPU µs/query (rate 788.9, cores 4.03) | Eligible fallback; primary unavailable or excluded |
| small-aggregate sweep / 8 | `read-standard-parquet.md` | ≈184.944 queries/CPU-s; ≈5407.056 CPU µs/query (rate 1474.0, cores 7.97) | Eligible fallback; primary unavailable or excluded |
| small-aggregate sweep / 16 | `read-standard-parquet.md` | ≈162.794 queries/CPU-s; ≈6142.739 CPU µs/query (rate 2534.7, cores 15.57) | Eligible fallback; primary unavailable or excluded |
| small-aggregate sweep / 32 | `read-standard-parquet.md` | ≈139.017 queries/CPU-s; ≈7193.353 CPU µs/query (rate 3550.5, cores 25.54) | Eligible fallback; primary unavailable or excluded |
| small-aggregate sweep / 64 | `read-standard-parquet.md` | ≈134.287 queries/CPU-s; ≈7446.748 CPU µs/query (rate 3492.8, cores 26.01) | Eligible fallback; primary unavailable or excluded |
| 1m-aggregate / 8 | `read-standard-parquet.md` | ≈50.135 queries/CPU-s; ≈19946.026 CPU µs/query (rate 407.6, cores 8.13) | Eligible fallback; primary unavailable or excluded |
| table-aggregate / 1 | `read-standard-parquet.md` | ≈18.735 queries/CPU-s; ≈53377.265 CPU µs/query (rate 60.7, cores 3.24) | Eligible fallback; primary unavailable or excluded |
| broad-window / 1 | `read-standard-parquet.md` | ≈8.704 queries/CPU-s; ≈114885.496 CPU µs/query (rate 26.2, cores 3.01) | Eligible fallback; primary unavailable or excluded |
| full-scan / 1 | `read-standard-parquet.md` | ≈1.456 queries/CPU-s; ≈686956.522 CPU µs/query (rate 13.8, cores 9.48) | Eligible fallback; primary unavailable or excluded |
| small-aggregate while writing / 4 | `read-standard-parquet.md` | ≈111.782 queries/CPU-s; ≈8946.023 CPU µs/query (rate 665.1, cores 5.95) | Eligible fallback; primary unavailable or excluded |
| write during small-aggregate / 4 | `read-standard-parquet.md` | ≈299119.326 rows/CPU-s; ≈3.343 CPU µs/row (rate 1331081, cores 4.45); 84.2 MB/CPU-s | Eligible fallback; primary unavailable or excluded |
| 1m-aggregate while writing / 4 | `read-standard-parquet.md` | ≈30.947 queries/CPU-s; ≈32313.473 CPU µs/query (rate 186.3, cores 6.02) | Eligible fallback; primary unavailable or excluded |
| write during 1m-aggregate / 4 | `read-standard-parquet.md` | ≈295666.951 rows/CPU-s; ≈3.382 CPU µs/row (rate 1386678, cores 4.69); 83.2 MB/CPU-s | Eligible fallback; primary unavailable or excluded |
| export spans / 4 | `read-telemetry-hot.md` | ≈73851.807 rows/CPU-s; ≈13.541 CPU µs/row (rate 61297, cores 0.83) | Eligible fallback; primary unavailable or excluded |
| export spans / 4 | `read-telemetry-parquet.md` | ≈49467.769 rows/CPU-s; ≈20.215 CPU µs/row (rate 59856, cores 1.21) | Eligible fallback; primary unavailable or excluded |
| write x1 / 1 | `write-native.md` | ≈815661.765 rows/CPU-s; ≈1.226 CPU µs/row (rate 554650, cores 0.68); 227.8 MB/CPU-s | Eligible fallback; primary unavailable or excluded |
| write x4 / 4 | `write-native.md` | ≈800230.070 rows/CPU-s; ≈1.250 CPU µs/row (rate 1144329, cores 1.43); 224.7 MB/CPU-s | Eligible fallback; primary unavailable or excluded |
| write x16 / 16 | `write-native.md` | ≈1353995.000 rows/CPU-s; ≈0.739 CPU µs/row (rate 2437191, cores 1.8); 380.6 MB/CPU-s | Eligible fallback; primary unavailable or excluded |
| write x64 / 64 | `write-native.md` | ≈1107400.000 rows/CPU-s; ≈0.903 CPU µs/row (rate 3311126, cores 2.99); 311.5 MB/CPU-s | Eligible fallback; primary unavailable or excluded |
| otlp spans x1 / 1 | `write-telemetry.md` | ≈49554.000 rows/CPU-s; ≈20.180 CPU µs/row (rate 22720, cores 0.46); 42.0 MB/CPU-s; 16.0 OTLP wire MiB/CPU-s | Eligible fallback; primary unavailable or excluded |
| otlp spans x4 / 4 | `write-telemetry.md` | ≈52247.000 rows/CPU-s; ≈19.140 CPU µs/row (rate 59741, cores 1.14); 44.3 MB/CPU-s; 16.9 OTLP wire MiB/CPU-s | Eligible fallback; primary unavailable or excluded |
| otlp spans x16 / 16 | `write-telemetry.md` | ≈48344.000 rows/CPU-s; ≈20.685 CPU µs/row (rate 191337, cores 3.96); 41.0 MB/CPU-s; 15.6 OTLP wire MiB/CPU-s | Eligible fallback; primary unavailable or excluded |
| otlp spans x64 / 64 | `write-telemetry.md` | ≈51177.000 rows/CPU-s; ≈19.540 CPU µs/row (rate 425901, cores 8.32); 43.4 MB/CPU-s; 16.5 OTLP wire MiB/CPU-s | Eligible fallback; primary unavailable or excluded |
| otlp logs x1 / 1 | `write-logs-run-37.md` | ≈33996.000 rows/CPU-s; ≈29.415 CPU µs/row (rate 11668, cores 0.34); 93.9 MB/CPU-s; 40.6 OTLP wire MiB/CPU-s | Eligible fallback; primary unavailable or excluded |
| otlp logs x4 / 4 | `write-logs-run-37.md` | ≈38110.000 rows/CPU-s; ≈26.240 CPU µs/row (rate 41673, cores 1.09); 105.3 MB/CPU-s; 45.5 OTLP wire MiB/CPU-s | Eligible fallback; primary unavailable or excluded |
| otlp logs x16 / 16 | `write-logs-run-37.md` | ≈39223.000 rows/CPU-s; ≈25.495 CPU µs/row (rate 137713, cores 3.51); 108.3 MB/CPU-s; 46.9 OTLP wire MiB/CPU-s | Eligible fallback; primary unavailable or excluded |
| otlp logs x64 / 64 | `write-logs-run-37.md` | ≈35670.000 rows/CPU-s; ≈28.035 CPU µs/row (rate 260475, cores 7.3); 98.5 MB/CPU-s; 42.6 OTLP wire MiB/CPU-s | Eligible fallback; primary unavailable or excluded |

### Selected fallback entries: Bifrost client queue

Source `client-queue.json` at clean `cfe21dc90`; `client-queue.txt` rounds the
same values. 60-second windows, 100 rows/observation, rates and in-flight limits
as recorded. Use server CPU/row, not client CPU or the envelope's 32 host CPUs.

| Metric / step | Source | CPU µs/row entry | Status/reason |
|---|---|---|---|
| sustained | `client-queue.json`, `server_cpu_us_per_row` | 1.89 | Eligible fallback; no primary queue report |
| headroom | `client-queue.json`, `server_cpu_us_per_row` | 1.48166667 | Eligible fallback; no primary queue report |
| ack-delay | `client-queue.json`, `server_cpu_us_per_row` | 1.48 | Eligible fallback; no primary queue report |
| max-in-flight-1 | `client-queue.json`, `server_cpu_us_per_row` | 1.90333333 | Eligible fallback; no primary queue report |
| max-in-flight-2 | `client-queue.json`, `server_cpu_us_per_row` | 4.15 | Eligible fallback; no primary queue report |
| max-in-flight-4 | `client-queue.json`, `server_cpu_us_per_row` | 3.89 | Eligible fallback; no primary queue report |
| max-in-flight-8 | `client-queue.json`, `server_cpu_us_per_row` | 3.88666667 | Eligible fallback; no primary queue report |

The telemetry read reports contain exact-answer checks and pruning evidence,
but no timed query sweeps with completed-query counts. Their exported-span
write rows above are eligible efficiency context; no telemetry query efficiency
entry is invented from latency percentiles alone.

### Unavailable and superseded entries

| Metric | Source | Value | Status/reason |
|---|---|---|---|
| Verification per-core / CPU per verification | Gateway `target/verification-capacity/report.md` | Absent | No file; not an entry. Bifrost-worktree candidate excluded above; no PR 150 fallback. |
| Gateway request/capture efficiency | PR 150 pre-merge | Absent | No Gateway report; primary excluded. No entry; a declared comparison without a usable baseline is INVALID. |
| Telemetry read/write efficiency | Merged `5c775110f` | Not selected | Excluded: uncommitted fork patch; never silently attributed to clean commit. |
| Standard read efficiency | Merged `5c775110f` | Not selected | Superseded by 2026-10-21 report generation; fallback is clean pre-merge, not this merged source. |
| Historical raw latency / total rate / drain / loss | All reports | Retained in sources | Context only, not per-core/per-operation baseline criteria. |
| Scan GB/s/core, capture bytes/core, storage/memory ratios without matching evidence | All selected sources | Unavailable | No invented bytes/window denominator; CPU-used baseline only, not provisioned-vCPU throughput. |

Historical methodology uses production paths, no compilation during execution,
no synthetic host load, matching-host comparisons for causal attribution and
repeated cases. Historical ~25% write and ~2.5× fsync swings motivate preserving
spread. Normalized cross-host comparisons remain qualified evidence, not proof
of hardware-independent code regression.

## Criteria history inspected

| Source commit | Relevant change and consequence for the new spec |
|---|---|
| `97c120cc0` | Deletes former benchmark harnesses, qualification artifacts, docs and task wiring. Deleted documented write p99 ≤100 ms, visibility p99 ≤5 s, first-frame p99 ≤500 ms, and compare −10% throughput/+15% p95/p99 are historical context supplied by the maintainer; they are not the current live threshold contract. The dead thresholds/SloGate remains. |
| `3d6b53ed0` | Selective target loosened to p50 7 ms. |
| `47f0c66ee` | Reads-while-writing changed to two clients; selective p95 target to 10 ms. |
| `364e5d8dd` | Selective latency judged at one client and 600 qps at the busiest point. |
| `32e39d333` | Inclusive verification 500 µs and selective 1,000 µs judgments. New intent deliberately uses strict <500 µs overhead and <5 ms point lookup at 64 clients; equality must not accidentally inherit inclusive behavior. |
| `e0629de83` | Shortens Gateway windows and lowers MIN_CALLS from 100 to 30. This is not a validation of 30 observations for general p99 judgment. |
| `58baeee97` | Bounds Scribe outbox and makes shedding an explicitly accounted saturation state. The experiment must judge declared capture expectations rather than treat every shed row as a failed product request or ignore the loss. |

Current design authority remains the owning architecture. Historical change
specifications, handoff recommendations and report verdicts are evidence to
reconcile, not alternate product contracts. The older optimization draft also
contains prohibited `/proc` cache-dropping and architecture-waiver prose;
the new packet explicitly excludes inheriting those provisions.

## Revision history

- Revision 1 — 2026-10-23: Initial workflow and historical-source research.
- Revision 2 — 2026-10-23: Corrected the missing-report claim; preserved five
  originals with hashes; checked source history, reflogs, report identity and
  current cleanliness; recorded primary provenance exclusions and explicit
  eligible fallback values. D-1/D-2/D-4 decisions are reflected in spec revision
  2; D-3's remaining clean-source/build evidence question is explicit.

- Revision 3 — 2026-10-23: Rechecked and cited perf commands, scoped server/
  cgroup paths, Forge envelopes and existing sysinfo owners/dependency; recorded
  approved portable replacements and upstream samply diagnostic limitations.

- Revision 4 — 2026-10-24: Replaced bucket-based benchmark methodology with raw
  per-request samples and standard OpenTelemetry tracing of internal duration
  observations; distinguished standard collection from Wyrd overhead meanings.
  Required real evidence is Linux x86_64 only, with unchanged macOS arm64 and
  Linux arm64 portability. Marked original source inventories as historical
  where the rejected uncommitted implementation has changed them.

- Revision 5 — 2026-10-24: Recorded maintainer server-only latency boundaries,
  diagnostic client timings, both CPU normalizations, historical byte-unit
  conversion and heavy-scan p95 sample arithmetic. Historical reports and
  eligibility are unchanged; no new upstream claim or benchmark evidence.
