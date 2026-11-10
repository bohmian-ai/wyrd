# Gateway c64 capture latency: diagnosis and next work

Research output of [TASK-002](../tasks/TASK-002-gateway-latency-diagnosis.md)
under [spec revision 2](../spec.md). No benchmark, build, profiler or
instrumentation ran for this report. Every number below comes from an
existing record, timing snapshot or profile. Every mechanism comes from source
read at the commit named next to it.

[TASK-003](../tasks/TASK-003-recorder-overhead-and-standard-reporting.md)
later ran three bounded experiments, answered M1, and implemented four
reductions. Its results are in
[Measurement qualification and implemented reductions](#measurement-qualification-and-implemented-reductions).
[TASK-004](../tasks/TASK-004-client-and-json-hot-path.md) added fresh current
allocation evidence and stage comparisons in
[TASK-004 current allocation and stage comparisons](#task-004-current-allocation-and-stage-comparisons).
The sections before them are unchanged.

## Summary

| Case at c64 | Client p50 (reps) | Objective < 1 ms | Gateway-added p50 (reps) | Objective < 500 µs |
|---|---|---|---|---|
| 1 KiB metadata | 1.016 / 1.015 / 1.046 ms | **missed** (marginally) | 176 / 176 / 178 µs | met |
| 64 KiB metadata | 2.319 / 2.374 / 2.363 ms | **missed** (2.3×) | 406 / 409 / 409 µs | met (≈ 90 µs margin) |
| 64 KiB payload | 4.616 / 4.385 / 4.348 ms | **missed** (4.4×) | 2.888 / 2.488 / 2.020 ms | **missed** |

Source: unprofiled candidate record `20261109T153318Z-b1cf95f2e`, per
repetition. "Gateway-added" is the record's `overhead_p50_ms`: the per-call
residual, handler arrival to answer handoff less that call's own provider
waits. The two objectives are judged independently. Meeting the server
objective says nothing about the client objective.

Ranked explanation (detail in [Ranked explanation](#ranked-explanation)):

1. **Most of client time at c64 lies outside the Gateway residual.** For the
   two metadata cases, the residual is 17% (1 KiB) and 18% (64 KiB) of the mean
   client cycle. Provider exchange time and time outside the handler make up
   the rest. Both grow about 12× from c1 to c64, while the residual grows
   about 2–3×. (Measured.)
2. **Exchange time is local, not remote.** The stub upstream runs inside the
   driver process, on the same 8-worker runtime as the 64 clients. Exchange
   time therefore contains driver scheduling and loopback transport. The
   streaming control shows a stub-side transport effect of about 40 ms.
   (Source-proven mechanism. How much of the c64 growth comes from the driver
   is not measured.)
3. **At c64 the closed loop is bounded by CPU throughput.** Server and driver
   together use about 24–26 of 32 logical CPUs. Server CPU per call is
   1.6–2.5× its c1 value. By Little's law a 1 ms mean cycle at 64 clients
   needs about 64k calls/s. At the measured CPU cost per call, 64 KiB calls
   would need about 64 cores, twice this host. (Measured inputs,
   arithmetic bound.)
4. **The 64 KiB path allocates about 11× the payload per call.** That is about
   702 KiB per metadata call, on the provider exchange, ingress and adapter
   paths. memmove is 7.9% (64 KiB metadata) and 15.2% (payload) of on-CPU
   samples, mostly on Tokio workers. (Measured cost. Its share of latency is
   not established.)
5. **The payload residual is bimodal.** 5.6–7.0% of calls take more than 10 ms,
   almost all of them 20–100 ms, and these calls carry 56–59% of all residual
   time. The cause is **unknown**. The capture-slot wait and post-answer
   capture work on shared workers are both plausible. No historical
   observation separates them or rules either out.

The next work is two bounded measurements on the current code with existing
tools. Neither one is authorized by this report. One measurement reads the
phase and outbox metrics that TASK-001 now keeps in benchmark snapshots. The
other is a matched-load `--pace` sweep. A copy-reduction correction on the
64 KiB path is supported as a cost. Its latency benefit depends on the second
measurement. See [Proposals](#proposals).

## Authority and dependency status

- TASK-001 is at candidate `67ee01bc5`. Its second review returned
  `FIX_REQUIRED` for FIND-TASK-001-2, the proof that standard metric series
  fit the capacity cap
  (`review/TASK-001-review-002-67ee01bc5/verdict.md`, not yet committed).
- The contract this report relies on is implemented at that candidate:
  - timing series semantics;
  - the per-call residual;
  - benchmark snapshots that carry every standard metric
    (`wyrd-benchmark/src/lib.rs::BenchmarkSnapshot::metrics`) and are kept per
    window by `experiment.rs::keep_timings`.
- The open finding does not change these semantics. It does affect whether a
  new HEAD snapshot can be trusted to have dropped no series. Proposal M1
  therefore waits for TASK-001 to close.
- Historical records predate the benchmark-owned standard metrics: their
  snapshots carry no `metrics`. TASK-001 adds no historical stage
  observation.

## Evidence inventory

Records are relative to the repository root. "Profile" means `--profile`
(samply CPU plus per-step heap). All records were built with
`release`, `-C force-frame-pointers=yes`, `debug = line-tables-only` and
rustc 1.99.0. They ran on an AMD Ryzen 9 9950X (16 cores, 32 logical CPUs),
used a `file://` store, took 5 s warmup plus 3 × 5 s per step, and recorded
`dirty: false`, `diff_sha256: null`, verdict `VALID`.

| Id | Record | Source | Mode | Server binary (bytes, mtime) | Status for this report |
|---|---|---|---|---|---|
| B | `benchmarks/history/gateway/overhead/20261108T234942Z-5508cf29f.json` | `5508cf29f` | full, unprofiled | 1234346656, 2026-11-08T15:27:58Z | valid; different source than C, so it is a separate population and **not a controlled comparison** |
| BP | `target/benchmarks/gateway/overhead/20261109T002859Z-5508cf29f.json` | `5508cf29f` | full, profile | same as B | timings and heap valid. **CPU capture failed**: `profiles/samply-931681.log` reports `perf_event_paranoid` = 2. `profile.complete = false` |
| C | `benchmarks/history/gateway/overhead/20261109T153318Z-b1cf95f2e.json` | `b1cf95f2e` | full, unprofiled | 1237307872, 2026-11-09T15:29:15Z | **primary evidence** |
| CP | `target/benchmarks/gateway/overhead/20261109T153854Z-b1cf95f2e.json` | `b1cf95f2e` | full, profile | same as C | timings and heap valid; CPU profile only partly usable (below) |
| CR | `target/benchmarks/gateway/overhead/20261109T154508Z-b1cf95f2e.json` | `b1cf95f2e` | `--steps` 1 KiB c1, 64 KiB metadata c64, payload c64; unprofiled | same as C | valid corroborating repetition set; not history |

Each record's artifact directory has the same stem without the commit suffix.
It holds `timings/<step>/repetition-{1,2,3}.json`, the before and after
benchmark snapshots of each measured window. `warmup.json` is an empty array,
so warmup windows have no snapshots.

**CPU profile identity.** The checks were:

- CP's raw `profiles/profile-2483944.json.gz` names
  `target/profiling/release/wyrd-server` with code id
  `a361cdcab45f6876352bd1427e107e909fddcf05`.
- The binary now at that path has GNU build id
  `84ce3fd42b2458d212c2a237f9906d01e22b944d`, 1237425072 bytes, modified
  2026-11-09T16:33:10Z. It was rebuilt after the run.
- No file under `target/`, `~/.debug` or `~/.cache/samply` carries the
  recorded id.

So **every `wyrd-server` frame in CP is unsymbolized** and stays an evidence
gap. Two derived files exist:

- `20261109T153854Z/cpu-payload-c64-analysis.json`
- `20261109T153854Z/symbolized-payload-c64-stacks.json.gz`

Both were written at 16:36Z, after the rebuild. Their `wyrd-server` frames are
bare addresses, so they were not symbolized with the recorded binary. Their
12 s `region_ms` matches no recorded window. They are **not used** for any
`wyrd-server` attribution.

Usable parts of CP:

- Thread names.
- Sample timestamps.
- `libc.so.6` frames. The recorded code id
  `a4a7992a8e66555c8141ab2a08a8465ff6e0ea65` equals the installed library's
  build id. The leaf `0x1a1640` (size `0x7c0`) disassembles to the AVX-512
  `memmove` (vector loads from `%rsi`, stores to `%rdi`, `rep movsb`).

**Profile intervals.** The profile spans the whole run: one capture per server
PID, 168.4 s from 15:39:10.24Z (`profile.rs`). No window timestamps are
recorded. Snapshot `taken_ns` counts from the server's own registry epoch,
not samply's clock. Steps were placed by the profile's own on-CPU sample rate,
about 1 kHz per busy thread, which reproduces each step's recorded server
cores:

| Step | Profile seconds from first sample | Samples/s (recorded cores) |
|---|---|---|
| 1 KiB c1 | 1–19 | 1194 (1.27–1.28) |
| 1 KiB c64 | 21–40 | 21816 (21.2–21.4) |
| 64 KiB metadata c64 | 105–123 | 21963 (21.0–21.5) |
| payload c1 | 126–144 | 1849 (1.80–1.83) |
| payload c64 | 146–166 | 14125 (14.4–14.8) |

These are **inferred** step intervals, accurate to about ±1 s. Each one covers
warmup, all three repetitions and the windows' edges, and none is a single
repetition. Profiled results (CP) inflate c64 client p50 by about 13% against
C (1.155–1.179 ms against 1.015–1.046 ms at 1 KiB). They are never mixed with
unprofiled results.

**Heap profiles.** CP and BP each saved `profiles/heap-<step>.pb.gz` after each
step:

- mimalloc samples about one allocation per 512 KiB allocated.
- Each profile was symbolized in-process at capture, so its function names and
  files do not depend on the rebuilt binary.
- `alloc_*` values are cumulative. A step's allocation is this step's profile
  minus the previous step's, keyed by the frame name, file and line stack.
  Location ids differ between files.
- A step's interval also contains Scribe background work and the pause between
  steps.

**Server log.** `target/benchmarks/gateway/overhead/server.log` was written at
15:46:27Z. It belongs to CR, the last run, not to CP. `gateway/mod.rs:194-196`
copies the log to the experiment directory, where each run overwrites it. That
contradicts `benchmarks/README.md:266-267`, which says the log sits beside the
profiles. This is a documentation drift, recorded here and not fixed.

**Source identity.**

- `b1cf95f2e..67ee01bc5` changes no Gateway path logic. The Gateway's
  `telemetry.rs` only switches its recording backend from `wyrd_telemetry` to
  `wyrd_benchmark` and skips the production duplicates while benchmarking.
- `routes.rs`, `ingress.rs`, `invocation.rs`, `capture.rs`,
  `scribe_outbox.rs`, `wyrd-runtime/src/outbox.rs`, `wyrd-gateway/src/engine.rs`
  and `skald-providers/src/exchange.rs` are identical.
- Source citations below are therefore valid for C, CP and CR at HEAD line
  numbers.
- `5508cf29f..b1cf95f2e` changes 21 Gateway route, adapter, provider and
  wire files (+2979/−1452, the TASK-005 copy-removal commits). B and BP results are a different
  implementation.

## How the numbers were computed

This is enough for a second reader to reproduce every table. No analysis
program is committed. The steps are plain arithmetic over the JSON:

- **Window distributions.** A snapshot series is cumulative over the server
  process's lifetime, keyed by `(boundary, outcome, case, phase)`. Steps c1
  and c64 of one case share one series, because labels carry the case but not
  the client count. For a window:
  1. take `after` minus `before`, bucket by bucket on the bucket value, for
     `elapsed`, `excluded` (union of provider waits) and `residual`;
  2. take the nearest rank `ceil(q·N)` over the bucket values (each value is
     the bucket's highest equivalent at 3 significant digits).

  The resulting p50s equal the record's `p50_ms` and `overhead_p50_ms` for
  every C step, which validates the method.
- **Means** use bucket highest-equivalent values (bias ≤ 0.1%). Means are
  additive per call (`elapsed = excluded + residual` for each call). Medians
  are never subtracted or summed.
- **Mean client cycle.** By Little's law it is `clients / rate`, with `rate`
  being answered calls per measured second. It is the mean time per closed-loop
  cycle: send, through validation, to the next send. "Outside handler" is the
  mean cycle minus the mean server `elapsed`. It is a difference of means over
  the same window population, with up to 64 late completions in about 130k
  calls.
- **Calls per profiled step** (heap denominators) is the sum over phases of the
  step's case series in the `after` of `repetition-3`, minus the same sum after
  the case's c1 step. This includes warmup and late calls.
- **Profile segments.** Samples are binned by `time` relative to the first
  sample, using the profile's sample weights. Thread names come from
  `threads[].name`. A leaf is `frameTable.address` of `stackTable.frame[stack]`
  with its library from `resourceTable`.

## Critical path

Source at `b1cf95f2e`, which equals HEAD for these files.
`G = crates/wyrd/wyrd-server/src/components/gateway`. Columns:

- **Clock:** which recorded boundary contains the segment.
- **R:** in the residual.
- **X:** in the provider exchange.
- **O:** outside the server `elapsed`. Client completion still contains it.

### Buffered call

| # | Segment | Owner | Clock | Historical measurement |
|---|---|---|---|---|
| 1 | Client builds the request and sends it on its own keep-alive reqwest HTTP/1.1 connection | `wyrd-testing/src/bin/capacity/gateway/workload.rs:253` (`sent`) | O | none per call |
| 2 | Loopback transfer; hyper parses the request head; outer `TraceLayer` and trace context run | hyper, `http/router.rs:160-166` | O | none |
| 3 | **Arrival**: `track_metrics` records `RequestArrival` | `http/middleware/metrics.rs:39-41` | start of `elapsed` | — |
| 4 | Edge layers (request id, panic, capacity, timeout, body limit) | `http/router.rs:305-346` | R | none |
| 5 | Authentication; an API-key exchange deserializes and clones permissions | `G/ingress.rs:130-175`, `api_keys().authenticate` | R | phase `authenticate` (production metric only) |
| 6 | Body read and full parse to `Json<Value>`; borrowed contract validation | `G/routes.rs:916-931, 1922-2002` | R | phase `parse` |
| 7 | Tenant snapshot, authorize/attribute, route; invoke audits staged, never awaited | `G/invocation.rs:565-598` | R | phases `snapshot`, `authorize`, `route` |
| 8 | **Capture-slot reservation**: `scribe_outbox().reserve_until(1, deadline)` reserves one item. FIFO waiters; deadline `now + request.timeout`; refusal `GatewayCaptureStagingFull` with `retry_after_seconds` | `G/invocation.rs:601-607, 645-663`; `wyrd-runtime/src/outbox.rs:475-519` | R | inside phase `admit`; **no observation of its own** |
| 9 | Ledger `admit`: one Postgres statement only when a limit or budget covers the call | `G/invocation.rs:521-523, 629` | R | inside phase `admit` |
| 10 | Spawn a `CallTask` on `gateway.tasks()` (same Tokio runtime); the handler awaits a `oneshot` | `G/invocation.rs:473-495`; `G/owner.rs:36,110` | R | `wyrd_gateway_queue_delay_seconds` (`:1240-1244`) |
| 11 | Credential resolution and provider request construction (`to_json`) | `wyrd-gateway/src/engine.rs:720-778`; `skald-providers/src/clients/openai.rs` | R | phases `credential`, `translate` |
| 12 | **Provider exchange**: `ExchangeTimer` starts right before `client().execute` and stops after the full body is collected. It covers connection pool checkout, request write, the stub's driver-side scheduling and handling, loopback and body receipt | `skald-providers/src/clients/mod.rs:66-78`; `exchange.rs:59-100` | X | `excluded` histogram |
| 13 | Response decode; the response capture projection parses the raw body to a `Value` and scrubs credentials, **before the answer** | `wyrd-gateway/src/adapter/http.rs:480-488, 751`; `engine.rs:297-303` | R | phase `translate` |
| 14 | Accounting: `observe`, pricing, `facts()` clones; a Postgres `settle` only when charged | `G/invocation.rs:1297-1340, 1014-1075` | R | phase `account` |
| 15 | **Answer handoff**: `head.send(Answer)`; the handler wakes and `telemetry.answered` records `gateway.buffered` | `G/invocation.rs:1344, 507`; `telemetry.rs:163-211` | end of `elapsed` | `elapsed`, `excluded`, `residual` |
| 16 | Relay `Box<str>` → `String`, `into_response`, hyper writes about 1 KiB or 64 KiB | `G/routes.rs:1492-1500` | O | none |
| 17 | Client receives the full body, parses it as JSON and checks the content length and `usage` | `workload.rs:303, 337-349, 269` (`done`) | O | client histogram only |

The residual therefore contains every server segment from 4 to 15 except
segment 12, including waits: segment 8 and the spawn and wake of segments
10 and 15. It does **not** contain segments 2 and 16, which are server-side
work. A residual is not "every server-induced delay".

### Post-answer work

This work happens after handoff, on the same `CallTask`, which runs on a
shared Tokio worker:

| Segment | Owner | Shares the response's resources? | Measurement |
|---|---|---|---|
| `capture()`: request body is moved (not copied); `CallCapture::from_facts` → `redacted_payload` for request and response (redaction walk, base64 extraction, `EncodedVariant::from_serialize` to a one-row Arrow column), plus any object `persist` | `G/invocation.rs:1350-1354, 1089-1120`; `G/capture.rs:220-306, 646-660, 789-812, 856-866` | yes: main-runtime workers, so it competes with other calls' segments 4–15 and with the outbox writer | phase `capture`, no case label |
| `slot.stage()`: non-blocking enqueue; the reserved slot becomes a staged item | `outbox.rs:699-710` | yes, briefly | none |
| Outbox writer: lingers up to 50 ms; batches up to 4096 items or 32 MiB; 4 concurrent writers × 4 lanes | `scribe_outbox.rs:110-152`; `outbox.rs:300-327, 1065` | yes: writer and lane tasks are on the main runtime | `outbox_write_seconds` |
| Delivery encode on `spawn_blocking`; up to 8 frames per slice; 32 process-wide delivery permits | `scribe_outbox.rs:165-181, 1695` | blocking pool; shares CPU, not workers | none |
| Scribe ingress, persistence, WAL and coordination: rayon pools `wyrd-scribe-ingress/persistence-cpu-N`, `wyrd-scribe-wal-io-N`, a separate coordination runtime | `vala-bifrost-redux/src/scribe/execution_lanes.rs:194, 1846, 2131`; `boot/mod.rs:614-639` | separate threads; share CPU only | `bifrost.scribe.write.*` timing series (historical) |
| **Release**: the item keeps its reserved share until the Scribe write is acknowledged, then `release` wakes waiters | `outbox.rs:1213-1220` | — | none |

Reserved capacity is 3/4 of the outbox limits: 786,432 items and 768 MiB of
the default 1,048,576 items and 1 GiB (`outbox.rs:92, 526-536`;
`config.rs:340-350`). A new capturing call waits at segment 8 only in two
cases: when staged or in-flight captures not yet acknowledged exhaust a share,
or when another waiter is already queued (`outbox.rs:435-457, 481-489`). A
slow Scribe write is on the response path only through this release. Scribe
memory (`scribe_used`) is not outbox occupancy. The `outbox_bytes` doc at
`config.rs:346-347` calls gateway captures unreserved and says they are shed.
That contradicts `capture_slot`, a documentation drift recorded here and not
fixed.

### Streams

`settle_stream` sends the `Answer` immediately, with upstream equal to
send-to-headers. The head point therefore excludes event relay. The end point
(`gateway.stream.end`) adds relay decode, translation and framing, and is
recorded after accounting and capture (`G/invocation.rs:1370-1463`).

## Measurements

### c64 cases with c1 controls (record C)

Values are per repetition. Server distributions are reconstructed from
snapshots (p50 / p99 in µs; client values are in the record):

| Step | Client p50 ms | Client p99 ms | Server elapsed p50 | Exchange p50 / p99 | Residual p50 / p99 | Rate calls/s | Server CPU µs/call | Server / driver cores |
|---|---|---|---|---|---|---|---|---|
| 1 KiB c1 | .164 .158 .161 | .383 .368 .373 | 119 115 117 | 34/90 32/87 33/87 | 84/191 82/187 83/190 | 5639 5804 5724 | 221 214 220 | 1.25 / 0.20 |
| 1 KiB c64 | 1.016 1.015 1.046 | 2.110 2.171 2.155 | 576 576 593 | 394/1101 394/1135 408/1143 | 176/457 176/475 178/481 | 59091 58923 57378 | 341 338 354 | 20.2 / 3.5 |
| 64 KiB meta c1 | .281 .284 .284 | .518 .554 .558 | 204 207 207 | 59/125 60/132 60/131 | 144/271 146/280 145/285 | 3454 3392 3394 | 339 345 345 | 1.17 / 0.26 |
| 64 KiB meta c64 | 2.319 2.374 2.363 | 4.466 4.600 4.492 | 1342 1372 1367 | 916/2218 940/2292 935/2236 | 406/1438 409/1499 409/1412 | 26162 25622 25769 | 784 804 801 | 20.6 / 5.1 |
| payload c1 | .356 .338 .341 | .711 .676 .697 | 267 256 258 | 68/153 64/146 65/151 | 197/398 190/373 191/368 | 2658 2819 2788 | 691 628 623 | 1.78 / 0.24 |
| payload c64 | 4.616 4.385 4.348 | 63.6 66.0 66.6 | 3680 3367 3246 | 795/3338 899/3412 937/3480 | 2888/61538 2488/64586 2020/65044 | 8664 8907 8952 | 1710 1621 1626 | 14.6 / 2.5 |

The other runs bracket these values:

- **CR** (same source and binary): 64 KiB metadata c64 client p50
  2.288/2.321/2.404 ms and residual p50 403/403/406 µs; payload c64 client
  p50 4.914/4.425/4.445 ms and residual p50 2.722/2.382/2.494 ms.
- **B** (different source): 1 KiB c64 client p50 1.010/0.992/1.039 ms;
  64 KiB metadata c64 2.450/2.427/2.452 ms; payload c64 4.672/4.591/4.490 ms.
  B is lower than C at 1 KiB and higher at 64 KiB. These are not controlled
  effects (INV-004).

### Mean cycle decomposition (record C, µs)

| Step | Mean cycle (Little) | Server elapsed mean | Exchange mean | Residual mean | Outside handler |
|---|---|---|---|---|---|
| 1 KiB c1 | 172–177 | 125–128 | 36–37 | 88–90 | 48–49 |
| 1 KiB c64 | 1083–1115 | 618–637 | 433–449 | 185–188 | 465–478 |
| 64 KiB meta c1 | 290–295 | 211–214 | 62–63 | 148–151 | 79–81 |
| 64 KiB meta c64 | 2446–2498 | 1414–1444 | 972–996 | 443–448 | 1032–1053 |
| payload c1 | 355–376 | 267–283 | 68–72 | 199–211 | 88–93 |
| payload c64 | 7149–7387 | 5932–6352 | 997–1094 | 4838–5355 | 1035–1217 |

From c1 to c64 for the metadata cases:

- the residual mean grows 2.1× (1 KiB) and 3.0× (64 KiB);
- the exchange mean grows 12× and 16×;
- the outside-handler mean grows 9.6× and 13×.

The residual is 17% of the 1 KiB c64 cycle and 18% of the 64 KiB metadata
c64 cycle. For payload c64 it becomes the dominant term, at 67–72%.

### Payload residual tail (record C)

| Repetition | > 1 ms | > 10 ms | > 20 ms | > 50 ms | Max | Share of residual time in > 10 ms calls | Exchange > 10 ms |
|---|---|---|---|---|---|---|---|
| 1 | 69.2% | 6.97% | 5.48% | 2.08% | 180 ms | 56% | 0 |
| 2 | 60.7% | 5.83% | 5.60% | 2.15% | 123 ms | 59% | 1 (37 ms) |
| 3 | 57.5% | 5.62% | 5.43% | 2.76% | 93 ms | 59% | 0 |

The payload c1 control has 0.0–0.02% of calls above 10 ms. The 64 KiB
metadata c64 control has under 0.01% above 10 ms in every repetition (max
31–106 ms) and 2.3–2.6% above 1 ms. Exchange exceeds 5 ms in only 0.10–0.12%
of payload c64 calls. The slow mode needs **both** payload capture **and** concurrency.

Scribe write timings in the same windows (`bifrost.scribe.write`, about 1900
writes per window) have p50 45–51 ms and p99 169–186 ms at payload c64,
against 8.5–9.6 ms and 26–32 ms at payload c1. Accounted capture bytes are
about 113.7 KB per call, about 1.0 GB/s at c64. These numbers only describe
the Scribe side. Nothing in the record measures outbox reserved occupancy or
capture-slot waiting, so they do **not** show that requests waited. Rough
arithmetic shows why. A mean capture residence of 0.1–0.2 s from staging to
acknowledgement implies 100–200 MB staged at 1 GB/s. That is under the 768 MiB
reserved share unless residence exceeds about 0.75 s or bursts concentrate.
This is a plausibility bound, not an observation.

### Streaming control (record C)

| | Head residual p50 | End residual p50 | End elapsed p50 | Client p50 |
|---|---|---|---|---|
| c1 | 182 / 213 / 221 µs | 171 / 199 / 205 µs | 40.9 ms | 41.04–41.08 ms |
| c64 | 116 / 120 / 115 µs | 108 / 113 / 109 µs | 41.0 ms | 41.06–41.08 ms |

Stream elapsed is about 41 ms, almost all of it in the exchange (40.6–40.9 ms
mean) for a 32-event stub with zero event interval. Two facts are source-proven:

- the stub's `PacedUpstream` uses plain `axum::serve` without disabling Nagle
  (`wyrd-testing/src/gateway_capture.rs:460`);
- `wyrd-server` documents and disables exactly this roughly 40 ms
  Nagle/delayed-ACK effect on its own listener (`app/serve.rs:12-27`).

That the 40 ms is this effect is a **hypothesis**. It is the most direct
evidence that "provider exchange" time includes local transport behavior.
Stream residuals do not grow at c64, because streams offer only about 1.5k
calls/s at 1.5 server cores. The handoff and relay machinery is not
intrinsically slow under concurrency. The buffered c64 growth tracks load
(calls/s and cores), not concurrency alone.

## CPU and allocation attribution

### On-CPU samples (CP, inferred step intervals, symbol-free)

| Step | Tokio workers | Scribe threads (ing / per / coo / wal) | memmove leaf | writev / recv / syscall leaf |
|---|---|---|---|---|
| 1 KiB c64 | 95.4% | 1.7 / 2.5 / 0.4 / 0.1% | 4.8% | 3.8 / 0.7 / 2.0% |
| 64 KiB meta c64 | 94.7% | 1.7 / 2.7 / 0.7 / 0.1% | 7.9% | 6.6 / 5.6 / 2.6% |
| payload c1 | 58.6% | 21.6 / 14.8 / 3.7 / 1.4% | 9.6% | 2.2 / 1.7 / 2.3% |
| payload c64 | 84.4% | 11.1 / 3.6 / 0.7 / 0.2% | 15.2% | 4.9 / 4.1 / 4.5% |

memmove samples are 90–94% on Tokio workers at c64. Tokio workers run every
request segment, post-answer capture and the outbox writer, so the profile
cannot divide memmove among them. Samples are on-CPU only and say nothing
about waiting. They are CPU proportions, not wall-time proportions. At c1,
server CPU per call (214–221 µs) exceeds the server elapsed mean (125–128 µs).
At least that difference is CPU spent off the measured response interval or
in parallel with it: post-answer capture, outbox and Scribe work, and runtime
overhead.

### Heap allocation per call (CP, step deltas)

| Step | Allocated per call | Objects per call | BP (`5508cf29f`) for comparison |
|---|---|---|---|
| 1 KiB c64 | 203 KiB | 1182 | 205 KiB, 1184 |
| 64 KiB meta c1 | 713 KiB | 1433 | 859 KiB, 1533 |
| 64 KiB meta c64 | 702 KiB | 1183 | 762 KiB, 1206 |
| payload c1 | 2153 KiB | 2013 | 2178 KiB, 2009 |
| payload c64 | 1796 KiB | 1701 | 1856 KiB, 1693 |

Leading CP owners by allocated bytes (innermost repository frame, step delta):

- **64 KiB metadata c64:** the provider exchange path
  (`skald-providers/src/exchange.rs`, `wyrd-gateway/src/adapter/http.rs`,
  `engine.rs`) is 31.6% inclusive, about 222 KiB per call. Within it,
  `skald-providers/src/clients/mod.rs` is 10.7% and `clients/openai.rs::to_json`
  9.2%. Gateway `ingress.rs` is 16.7% inclusive and
  `wyrd-gateway/src/adapter/mod.rs::complete` 9.6%. That is about eleven
  payload-sized allocations on segments 5–13, all **before** the handoff.
- **1 KiB c64:** by objects, capture projection dominates:
  `capture.rs::otlp_span` 8.8% and `wyrd-spec/src/gateway/record.rs::validate_canonical_json`
  8.6%, with `capture.rs` 27.8% inclusive. This work is post-answer.
  Permission deserialize and clone (`wyrd-runtime/src/permission.rs`) is 6.3%
  of bytes in authentication, on the response path. Outbox and Scribe sink
  together are 22%.
- **payload c64:** Scribe execution lanes are 32.5% inclusive, on Scribe
  threads. Gateway `capture.rs` is 30.8% inclusive, post-answer on Tokio
  workers: `variant/serialize.rs` scalar and end builders about 21.5%, and the
  capture closure 7.2%. Bifrost `preprocess.rs::encode_ipc` is 14.9% and
  `layout_cache.rs::shred` 14.8%.

Allocation bytes are not CPU time. These figures establish copy and
allocation volume per owner, not latency shares.

## Waiting mechanisms and blind spots

| Mechanism | Source-proven? | Historical observation | Blind spot |
|---|---|---|---|
| Capture-slot wait (segment 8) | yes: FIFO wait until a reserved share frees, released only on Scribe ack | none. The `admit` phase histogram existed only in production metrics, which these records did not keep | waiting time, waiter count and reserved items/bytes over time |
| Spawn queueing and handler wake (10, 15) | yes | none (`queue_delay` not kept) | per-window distribution |
| Post-answer capture on shared workers delaying other calls | mechanism yes (same runtime) | CPU samples show that workers do the work, not that others wait for it | the phase `capture` distribution and worker queueing; the contribution is unmeasured |
| Outbox writer and Scribe delivery | yes | Scribe write stage series only | outbox write latency and reserved occupancy |
| Driver-runtime scheduling inside exchange and outside-handler time | yes: stub and clients share 8 driver workers | driver cores 3.5–5.1 of 8 at c64 | no per-call send, arrival or receipt join |
| Host CPU saturation / SMT | arithmetic | about 24–26 of 32 logical CPUs busy (server + driver) at metadata c64; Postgres not counted | per-thread run-queue delay |
| Pre-arrival and post-handoff server work (2, 16) | yes | none | not in any server boundary |

## Ranked explanation

**Measured facts.**

- F1: All three c64 cases miss client p50 < 1 ms; 64 KiB payload also misses
  the residual p50 < 500 µs objective.
- F2: Metadata c64 client time is mostly exchange and outside-handler time
  (82–83% of the mean cycle).
- F3: Server CPU per call grows 1.6× (1 KiB), 2.3× (64 KiB) and 2.5× (payload)
  from c1 to c64.
- F4: About 702 KiB is allocated per 64 KiB metadata call, mostly before the
  handoff; memmove is 7.9–15.2% of c64 on-CPU samples.
- F5: The payload c64 residual has a 20–100 ms mode in about 5.5% of calls,
  absent from both controls.
- F6: Profiling perturbs c64 latency by about 13%.

**Source-proven mechanisms.**

- M-a: Exchange time contains the in-driver stub's scheduling and transport.
- M-b: Capture, the outbox writer and request handling share one Tokio
  runtime.
- M-c: A capturing call can wait at `capture_slot` until a Scribe ack releases
  reserved share.
- M-d: The response capture projection (`Value` parse and scrub) runs before
  the handoff.
- M-e: The residual excludes request-head parsing and response writes.

**Hypotheses, each falsifiable.**

1. **H1 — load queueing.** At c64 the closed loop is CPU-throughput-bound on
   this host. The latency growth outside the residual, and most growth inside
   it, is queueing (server workers, driver workers, SMT sharing), not per-call
   fixed work. The utilization law gives the bound. For a 1 ms mean cycle,
   64 clients need about 64k calls/s. At 64 KiB the measured combined CPU is
   about 800 µs server plus about 195 µs driver per call, which would need
   about 64 cores. *Contradicted if:* at a matched open-loop rate of about 25%
   of the closed-loop c64 rate, with the same 64-call bound, exchange and
   residual p50s and CPU per call stay at least 2× their c1 values (M2).
2. **H2 — slot wait.** The payload tail is capture-slot waiting: reserved share
   exhausted while about 1 GB/s of captures await Scribe acks. *Contradicted
   if:* in a payload c64 window, the `admit` phase p99 stays under 1 ms while
   the residual p99 stays at or above 20 ms (M1). *Supported if:* `admit`
   reproduces the 20–100 ms mode at about the same frequency.
3. **H3 — worker stalls.** The payload tail is post-answer capture work
   (redaction plus Variant encoding of about 128 KiB per call) occupying shared
   workers and delaying other calls' handoff and wake. *Contradicted if:*
   `queue_delay` and the `translate`/`account` phases stay small while the tail
   sits in `admit`. *Supported if:* the tail appears in `queue_delay` or in
   handoff wake time (residual minus the sum of phases), and the `capture`
   phase p99 reaches the same scale.

H2 and H3 are not exclusive, and neither is preselected.

**Unknowns.**

- U1: How outside-handler time divides among driver, kernel and the server's
  segments 2 and 16.
- U2: Why CPU per call inflates with load: SMT, memory bandwidth from the copy
  volume, lock contention, or added work.
- U3: The CP CPU attribution of `wyrd-server` functions, which is lost with the
  binary.
- U4: Whether the benchmark fixture enables a limit or budget, which would make
  segment 9 or the charged `settle` a Postgres round trip. Settle this by
  reading the fixture setup before interpreting the `admit` and `account`
  phases.

## Proposals

None is executed or authorized here. Every proposal preserves the following,
and none selects a scheduler, cache, allocator, transport or
memory-accounting change:

- INV-001: authentication, tenancy, scrubbing, audit and response contents;
- INV-002: reservation before charge and dispatch, the existing deadline and
  refusal, and no shedding of reserved captures;
- INV-003: settlement and durable acknowledgement.

### M1 — Phase and outbox evidence on current code (first)

| | |
|---|---|
| Hypotheses | H2 against H3; also places the residual's c64 growth among phases |
| Missing observation | per-window `wyrd_gateway_phase_duration_seconds{operation,phase}` (admit, translate, account, capture), `wyrd_gateway_queue_delay_seconds`, `outbox_write_seconds` and `outbox_*_total`. Since TASK-001 these are in every kept `timings/<step>/<phase>.json` snapshot, so no instrumentation change is needed |
| Prerequisite | TASK-001 passes review, so snapshot series are known not to drop |
| Driver and cases | existing `bench:l1:gateway`, `--steps buffered-1kib-capture-metadata.c64,buffered-64kib-capture-metadata.c64,buffered-64kib-capture-payload.c64`, closed loop, 64 clients, unprofiled, default 3 × 5 s plus warmup, one invocation |
| Validity | verdict VALID; reconciliation expected = recorded; snapshot `dropped = 0`, no rejected metric; capture staged = arrivals, none lost or shed; payload c64 client and residual p50 within the C/CR range (4.35–4.91 ms, 2.0–2.9 ms), otherwise the run is not comparable |
| Interpretation | phase families lack case labels, but each window runs one case and is differenced before/after. Supported, or contradicted, if the admit p99 and the frequency of admit above 10 ms match the residual's 20–100 ms mode (H2), or the mode appears in queue delay or unaccounted wake time with a large `capture` phase (H3). A tail in none of them names a further blind spot (segments 4–6 or allocator), not a cause |
| Observer effects | the phase histograms already ran in the historical builds through the production recorder, so instrumentation cost is about equal; recorder differences are declared by the run |
| Stopping rule | one invocation. If the three repetitions disagree on which phase holds the tail, report it as unresolved; do not repeat without a new hypothesis |
| Retained artifacts | the record under `target/benchmarks/gateway/overhead/`, its `timings/`, and `server.log` copied beside it before any later run |
| Why existing evidence cannot answer it | historical snapshots have no `metrics`; samples cannot see waits |

Candidate command for separate authorization, after TASK-001 closes:
`mise run bench:l1:gateway -- --steps buffered-1kib-capture-metadata.c64,buffered-64kib-capture-metadata.c64,buffered-64kib-capture-payload.c64`
(add `--allow-dirty` only if the tree is dirty, which records the diff hash).

### M2 — Matched-load sweep (second)

| | |
|---|---|
| Hypothesis | H1 against per-call concurrency cost; also addresses U2 |
| Driver and cases | existing `bench:l1:gateway -- --steps buffered-64kib-capture-metadata.c64 --pace <r>`: open loop at most 64 in flight, unprofiled, at r ≈ 6500, 13000 and 19500 calls/s (25/50/75% of the C closed-loop rate); three invocations; never enters history |
| Expected counts | about r × 5 per repetition window; missed or capped arrivals counted as errors invalidate that rate |
| Result that decides | H1 holds if exchange p50, residual p50, outside-handler time and server CPU per call fall monotonically toward the c1 values (60 µs, 145 µs, about 80 µs, about 340 µs) as r falls. H1 fails if at 25% they stay at least 2× the c1 values |
| Observer effects | open-loop pacing changes arrival burstiness against closed loop; compare across rates, not against C, except for CPU per call |
| Stopping rule | three rates, once each; add no rate unless the 50% point is non-monotonic |

### C1 — Reduce payload-sized copies on the 64 KiB response path (correction, conditional)

- **Supported cause:** F4. Eleven payload-sized allocations per call before
  the handoff, plus memmove share.
- **Existing owners:**
  - `skald-providers/src/clients/openai.rs::to_json` and the body read in
    `clients/mod.rs`;
  - the `wyrd-gateway/src/adapter/http.rs` response decode and capture `Value`
    parse (`captured`, `:480-488`);
  - `adapter/mod.rs::complete`;
  - `G/routes.rs` ingress `Json<Value>` and relay `Box<str>` → `String`.

  This continues the TASK-005 copy-removal approach that already moved these
  owners between B and C.
- **Expected effect:** lower server CPU per call (784–804 µs) and memory
  bandwidth. Under H1 that raises closed-loop throughput and lowers c64 client
  latency. Under not-H1 it lowers only CPU.
- **Gate:** take it as a latency correction only if M2 supports H1.
- **Limits:** cannot by itself meet client < 1 ms at 64 KiB (see the
  utilization bound). Does not touch the payload tail.
- **Risks:** response bytes and credential scrubbing in the response capture
  (INV-001); body limit and contract validation; the capture projection must
  still see the scrubbed response before handoff.
- **Regression proof:** existing Gateway capture, redaction and response
  journey tests unchanged and passing; byte-identical responses.
- **Performance proof:** heap allocation per call and unprofiled `--steps`
  CPU per call and residual p50 at the same commit lineage, declared as a
  changed implementation, not a controlled improvement over C.
- **Rejected if:** allocation per call drops but CPU per call and residual p50
  stay inside the C/CR repetition range.

### C2 — Payload tail (correction, selected only by M1)

- **If H2 is supported:** the owner is the Scribe outbox, its reserved share
  and its delivery throughput (`scribe_outbox.rs`, `wyrd-runtime/src/outbox.rs`).
  Any change to reservation release or capacity touches INV-002 and INV-003,
  so it needs its own approved scope. This packet selects none. The proof
  would be an `admit` tail that disappears with captures still never shed.
- **If H3 is supported:** the owner is `GatewayInvocation::capture` and
  `CallCapture::from_facts`. The candidate is to reduce the redaction and
  Variant encode work per payload: allocation volume in
  `variant/serialize.rs` and the capture closure. Moving that work to another
  executor would be a scheduling change outside this packet.
- **Rejected if:** the tail persists with the selected phase reduced.

### Lower-ranked supported costs

These are not proposed for action yet:

- Per-call permission deserialize and clone in API-key authentication
  (`wyrd-runtime/src/permission.rs`, 6.3% of 1 KiB c64 bytes, response path).
  First confirm with source whether an existing owner already holds the parsed
  form. No new cache is proposed.
- Post-answer metadata projection objects (`otlp_span`,
  `validate_canonical_json`, about 17% of 1 KiB objects). These matter for
  the 1 KiB client objective only under H1.

U1 (the driver/server split of outside-handler time) has no existing
observation. Closing it needs a driver-side per-call join or upstream send
timestamps (`PacedUpstream` holds at most `bound` send times,
`gateway_capture.rs:511-516`), and
that is instrumentation. It needs planning authority and is not proposed as a
measurement here.

## Measurement qualification and implemented reductions

Source:
[TASK-003 execution evidence](../tasks/TASK-003-recorder-overhead-and-standard-reporting.md#execution-evidence).
Every figure there is pasted from the canonical report, `capacity report
<record>`, which shows each repetition's diagnostics. `benchmarks/README.md`
("Records and reports") makes that report the standard reference for this
work.

**Qualification.** Corrected recorder, full phase instrumentation:
`target/benchmarks/gateway/overhead/20261110T114927Z-b0033a70a.json`.
- It meets the pre-task recovery envelope (worst repetition of
  `20261109T153318Z-b1cf95f2e`) on 13 of 15 figures. The two misses are
  64 KiB metadata CPU per request (+0.2%) and payload client p50 (+1.2%). The
  maintainer accepted both as run-to-run swing.
- A paired run without the phase and queue-delay histograms
  (`…/20261110T115959Z-b0033a70a.json`) was not faster: 1–8% slower on
  client p50 and throughput. So the added instrumentation has no cost this
  protocol resolves. This is not a zero-overhead claim.

**M1 answered.** In every repetition of the payload case, the `admit` phase
holds the residual tail:

| | Repetition 1 | Repetition 2 | Repetition 3 |
|---|---|---|---|
| `admit` p99 | 67.6 ms | 53.1 ms | 51.3 ms |
| residual p99 | 68.4 ms | 53.8 ms | 52.2 ms |
| `capture` p99 | 1.17 ms | 0.74 ms | 0.70 ms |

- The `admit` p95 (12–26 ms) also tracks the residual p95 (16–26 ms).
- `queue_delay` overlaps `admit` by declaration and adds nothing separate.
- **H2, the capture-slot wait, is supported.** H3, post-answer capture work
  stalling workers, is contradicted as the tail's location.
- The frequency of `admit` above 10 ms was not read separately.
- The final run agrees.
- C2's H2 branch, the Scribe outbox reserved share and delivery throughput,
  is therefore the owner of the payload tail. Any change there still needs
  its own approved scope (INV-002, INV-003).

**Implemented reductions** (final record
`…/20261110T123522Z-57e143636.json`, VALID):

| Reduction | Work removed | Measured effect against the qualification run |
|---|---|---|
| Capture drain (`scribe_outbox.rs`, `staged.rs`) | one `Part` clone per row per submission attempt; one `StagedRow` clone per dense projection | background `outbox_write` −15% / +5% / +2% p50 by case: below resolution, not response latency |
| Metadata usage (`adapter/mod.rs::complete`) | the owned `Value` tree of every metadata-only native answer | `translate` p50 −9% (1 KiB) and −5% (64 KiB); p99 −12% / −11% |
| Payload redaction (`capture.rs::redact_object`) | one normalized `String` per object key; one media-type copy per media object | payload `capture` p50 unchanged, p99 −6%: below resolution |
| Audit projection (`projection.rs::stage_audit_event`) | copies of trace, operation, resource, permission and request text | `authorize` p50 −4% to −6%: near resolution |

**Final comparison.**
- Client p50: 1.003 / 2.351 / 4.621 ms.
- Gateway-added p50: 175 / 414 / 3075 µs.
- CPU per request: 345.7 / 801.7 / 1636.3 µs.
- Client < 1 ms is still missed in every case (1 KiB marginally).
- Gateway-added < 500 µs is met for both metadata cases and missed for
  payload.
- **In the first final run the payload residual p50 was 6.5% above the
  recovery envelope.** That increase lay entirely in `admit` (p50 2.19 →
  2.39 ms), which no change touches. Capture work and outbox write time did
  not grow. On this metric the pre-task record spans 2.02–2.89 ms and the
  qualification run 1.27–3.01 ms.
- A repeat run of the same code (`…/20261110T124026Z-7d7c73b9d.json`,
  VALID) measured 2.842 ms, inside the envelope, with client p50
  0.991 / 2.342 / 4.526 ms. Its one miss is the 64 KiB metadata residual p50
  (+0.2%). The first final run's miss was run-to-run variation, not a
  regression.
- None of the four reductions moves the payload tail, as C1 and C2 predicted.

## TASK-004 current allocation and stage comparisons

Source: [TASK-004 execution evidence](../tasks/TASK-004-client-and-json-hot-path.md#execution-evidence),
which holds every repetition, record, executable hash and method.

**Current server allocation** comes from fresh profiled runs of the
post-TASK-003 server (A, `…/20261110T144306Z-d4ea94988.json`) and the
production candidate (B, `…/20261110T162345Z-a31d121c4.json`). Figures are
step deltas over the reconciled calls, warmup included. The 702 KiB above is
historical record CP.

| Step | A bytes/call | B bytes/call | A objects | B objects |
|---|---|---|---|---|
| 1 KiB meta c64 | 193.9 KiB | 193.4 KiB | 1117 | 1086 |
| 64 KiB meta c64 | 646.7 KiB | 584.3 KiB | 1155 | 1081 |
| payload c64 | 2065.7 KiB | 2037.8 KiB | 1591 | 1596 |

- **64 KiB metadata:** B removes two payload-sized copies and adds one, in
  `JsonAnswer::validated` (the reader buffer shrunk by `into_boxed_str`).
  `3ae65fe4a` removes that one too, but no run measured it.
- **Payload:** capture still decodes the request tree. The net −28 KiB is in
  Arrow/Parquet capture encoding and is not attributed to this work.

**A → B, production, original fixture.**
- Client p50 is 1.04 / 2.28 / 4.65 ms.
- `translate` falls from 92 to 64 µs at 64 KiB.
- The only envelope miss is 64 KiB driver CPU (+1.5%), in an unchanged driver.
- Verify SDK changes are below resolution.

**B → C, fixture, identical server.**
- Driver CPU falls 9–16%. Client p50 is 1.005 / 2.179 / 4.532 ms.
- Residual p50 rises 0.6–9.9% with no server change. The faster closed-loop
  driver keeps more calls in the server at once.

**Objectives at C:**
- Client < 1 ms is missed in every case; the 1 KiB case misses by 5 µs.
- Gateway-added < 500 µs is met for both metadata cases (175, 362 µs) and
  missed for payload (2.83 ms).

## Limits

- No improvement or target attainment is claimed from B, C and CR: they are
  observations, not a controlled comparison. TASK-003's effects are
  same-host, same-protocol comparisons, with the resolution limits stated in
  its section.
- CP's `wyrd-server` CPU attribution is unavailable. Step intervals in the
  profile are inferred from its own CPU signal (±1 s) and include warmup.
- Heap deltas include background and between-step work. Heap sampling is
  statistical at about 1 per 512 KiB.
- Means are bucket-upper-bound approximations. Little's law uses rate over
  measured seconds with ≤ 64 late completions.
- Thread-name shares are on-CPU samples at about 1 kHz. The streamed c64
  segment shows fewer samples per second than its recorded cores, so
  low-utilization segments are not used.
- Every historical record and raw profile was left unchanged. The baseline was
  not rerun.
