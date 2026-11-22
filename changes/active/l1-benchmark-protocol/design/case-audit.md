# L1 benchmark case audit

The current inventory is **65 named timed cases**, with 48 in the default suite
and 17 available by explicit selection. Gateway's arbitrary offered rates
are additional parameterized runs. Peak/scaling rows, setup checks, and
report breakdowns do not increase the timed-case count.

The agreed routine selection is **28 cases**, with targeted cases available through the
same lifecycle and report. The routine suite answers which major paths are
healthy under a representative workload. Targeted runs answer questions about
a specific SDK, concurrency curve, payload, layout or query. Routine status
does not change a case's validity or satisfy another change's evidence requirements.

## Complete inventory and proposed disposition

Braces expand to one case per value in each brace. IDs are scoped to their family.

| Category and family | Current measured IDs | Default / explicit | Routine cases and rationale |
|---|---|---:|---|
| Verification, canonical closed loop | `direct-verify-c{1,64}`, `direct-verify-python-c{1,64}`, `direct-verify-typescript-c{1,64}` | 6 / 0 | **2.** Keep the Rust pair for the low-load and concurrent server control. Make the four Python/TypeScript closed-loop cases targeted for SDK-specific saturation and latency judgments. |
| Verification, canonical paced | `direct-verify-r3000`, `direct-verify-python-r3000`, `direct-verify-typescript-r3000` | 3 / 0 | **3.** Keep the matched offered-rate trio. Each SDK can limit offered work differently in closed loop. |
| Verification, Rust concurrency sweep | `direct-verify-c4`, `direct-verify-c16`, `direct-verify-c32` | 0 / 3 | **0.** Remove dedicated sweep registrations if the canonical Rust workload can accept the client count explicitly and preserve its controls and evidence. The sweep measures intermediate points on the same workload. |
| Verification, Agent-only model path | `direct-verify-model-c{1,64}` | 0 / 2 | **0.** Keep both targeted. They exercise the local Gateway-backed Agent path required by benchmark latency diagnosis REQ-020. |
| Gateway, overhead | `buffered-1kib-capture-metadata.c{1,64}`, `streamed-1kib-capture-metadata.c{1,64}`, `buffered-64kib-capture-metadata.c{1,64}`, `buffered-64kib-capture-payload.c{1,64}` | 8 / 0 | **5.** Keep all four c64 shapes and the small buffered c1 control. The c64 shapes isolate streaming, body size, and payload capture. Keep the other three c1 results targeted for low-load diagnosis. |
| Bifrost, writes-parquet | `otlp-spans-c{1,64}`, `otlp-spans-1k-c{1,64}`, `otlp-logs-c{1,64}`, `native-write-c{1,64}` | 8 / 0 | **8.** Keep the full matrix while write-throughput REQ-013 requires it. L2 calibration currently reads the highest measured median per path for standard spans, logs and native. Wide spans are outside that calculation and inside the full write matrix. |
| Bifrost, hot | `point-memtable-c{1,64}`, `point-staged-c1`, `small-aggregate-{memtable,staged}-c1`, `span-shredded-{memtable,staged}-c1`, `span-long-tail-{memtable,staged}-c1` | 9 / 0 | **3.** Keep the memtable point pair and the staged point. Keep aggregate and Variant shapes targeted in both residences. Variant-file-reads AC-004 requires the matched query matrix. |
| Bifrost, Parquet | `point-c{1,64}`, `small-aggregate-c1`, `full-scan-c1`, `span-trace-c{1,64}`, `span-shredded-c1`, `span-long-tail-c1` | 8 / 0 | **4.** Keep the point pair, full scan and concurrent trace. Keep the aggregate, single-client trace and two Variant shapes targeted. Trace scaling requires explicitly selecting both trace counts. |
| Bifrost, UI SQL | `{memtable,staged,published}-ui-c{1,8}` | 6 / 0 | **3.** Keep c8 in each residence for the mixed-page workload. Keep c1 targeted. |
| Bifrost, UI SQL 10× volume | `{memtable,staged,published}-ui-c{1,8}` in `ui-sql-10x` | 0 / 6 | **0.** Retain the larger fixture as a targeted volume selection through the same UI workload. |
| Bifrost, UI SQL learned layout | `{memtable,staged,published}-ui-c{1,8}` in `ui-sql-learned` | 0 / 6 | **0.** Retain the learned fixture as a targeted layout selection through the same UI workload. Keep its learning and layout checks. |
| **Total** | **Bifrost 43, Verification 14, Gateway 8** | **48 / 17** | **28 routine** (Bifrost 18, Verification 5, Gateway 5). Twenty current defaults move to targeted selection. Of the 37 cases outside the proposed routine suite, 34 retain their existing workload coverage and three sweep points could be supplied as explicit parameters. |

## Routine cases in detail

### Verification

Every case calls the canonical direct Verifier through the public SDK. The
fixture has one Assert and two Agent checks, each Agent making two local model
exchanges and one `code.run`. Report completed verifications/s, server CPU per
verification, server execution and overhead latency, client result and
materialization latency, and server concurrency. Overhead excludes the union of
per-execution Verifier waits. Paced cases additionally report offered, started,
missed and late calls. [Verification declaration](../../../../crates/wyrd/wyrd-testing/src/bin/capacity/verification/mod.rs#L454)

| Case | Workload and measurement purpose |
|---|---|
| `direct-verify-c1` | One Rust client calls again after each result. Measures serial call latency and CPU cost. |
| `direct-verify-c64` | 64 Rust clients call back to back. Measures concurrent throughput, CPU efficiency and the existing server overhead p99 target below 500 µs. |
| `direct-verify-r3000` | 64 Rust clients offer 3,000 verifications/s. Measures server and Rust client timings at a fixed offered rate and whether the driver keeps schedule. |
| `direct-verify-python-r3000` | 64 Python SDK clients offer the same 3,000/s. Measures Python call and result materialization timings, server timings and arrival misses at that load. |
| `direct-verify-typescript-r3000` | 64 TypeScript SDK clients offer the same 3,000/s. Measures Node client call and result materialization timings, server timings and arrival misses at that load. |

### Gateway

All five use a local upstream. Report completed requests/s, server CPU per
request, added Gateway overhead, end-to-end latency and capture reconciliation.
The c64 cases check added overhead p99 below 500 µs. The streaming case also
separates first token and end-of-stream timing. [Gateway cases](../../../../crates/wyrd/wyrd-testing/src/bin/capacity/gateway/mod.rs#L94)

| Case | Workload and measurement purpose |
|---|---|
| `buffered-1kib-capture-metadata.c1` | One client sends small buffered chats with metadata capture. Measures serial request latency and CPU cost. |
| `buffered-1kib-capture-metadata.c64` | 64 clients send the same small buffered chats. Measures concurrent buffering and metadata-capture cost. |
| `streamed-1kib-capture-metadata.c64` | 64 clients consume the 32-event stream with metadata capture. Measures first token, full stream, and final handoff overhead. |
| `buffered-64kib-capture-metadata.c64` | 64 clients send large buffered chats with metadata capture. Measures the larger body path and provides the same-size control for payload capture. |
| `buffered-64kib-capture-payload.c64` | 64 clients send the same large buffered chats with request and response payload capture. Measures the added capture path, retained payload reconciliation and backlog. |

### Bifrost writes

Each case drives one ingestion path through durable acknowledgement. Report
acknowledged rows/s, rows/server CPU-second, ACK latency, and publication and
backlog diagnostics. OTLP cases also measure uncompressed request MiB/s and
input MiB/server CPU-second. Native cases report row-based efficiency. The
windows measure ACK throughput and CPU spent during those windows. Publication
completion and outstanding work have separate readings. [Write registrations](../../../../crates/wyrd/wyrd-testing/src/bin/capacity/query/mod.rs#L1047)

| Case | Workload and measurement purpose |
|---|---|
| `otlp-spans-c1` | One sender ingests standard OTLP spans, about 340 B per span. Measures serial span ingest and ACK latency. |
| `otlp-spans-c64` | 64 senders use the same standard spans. Measures concurrent span ACK throughput and CPU efficiency. |
| `otlp-spans-1k-c1` | One sender ingests wider OTLP spans, about 1 KiB per span. Measures serial write cost for the wider payload. |
| `otlp-spans-1k-c64` | 64 senders use the same wider spans. Measures concurrent throughput and CPU efficiency for wide spans. |
| `otlp-logs-c1` | One sender ingests OTLP log records. Measures serial log ACK latency and input efficiency. |
| `otlp-logs-c64` | 64 senders ingest the same log population. Measures concurrent log throughput and input efficiency. This is the population named by REQ-013's 80 input MiB/server CPU-second target. |
| `native-write-c1` | One sender writes native Arrow batches. Measures serial native ACK latency and row cost. |
| `native-write-c64` | 64 senders write the same native batches. Measures concurrent native row throughput, CPU efficiency and outstanding staging/publication work. |

### Bifrost reads

The hot fixture holds 10M events and 1M spans, split between memtable and staged
residences after its layout-learning sample. Parquet cases use settled files.
Report exact-answer checks, queries/s, CPU/query and latency. For Parquet,
report executed scan rows and bytes where applicable. UI cases use the same
500-request fixture and cycle the original traces, verification, logs and
metrics pages. Report server query cost, client page and statement latency,
and exact statement answers for each residence. [Query declarations](../../../../crates/wyrd/wyrd-testing/src/bin/capacity/query/mod.rs#L306) · [UI declarations](../../../../crates/wyrd/wyrd-testing/src/bin/capacity/query/ui/mod.rs)

| Case | Workload and measurement purpose |
|---|---|
| `point-memtable-c1` | One client selects an event by ID from memtable rows. Measures serial live lookup cost. |
| `point-memtable-c64` | 64 clients make the same live lookups. Measures concurrent lookup throughput and tail latency, with existing p99 below 5 ms. |
| `point-staged-c1` | One client selects an event by ID from staged rows. Measures lookup latency and CPU before publication in the staged residence. |
| `point-c1` | One client selects an event by ID from published Parquet. Measures serial lookup latency, CPU and scan work. |
| `point-c64` | 64 clients make the same published lookups. Measures concurrent throughput and tail latency, with existing p99 below 5 ms. |
| `full-scan-c1` | One client runs the whole-table event count, duration sum and payload-length sum over Parquet. Measures scan p95, CPU and physical scan volume. |
| `span-trace-c64` | 64 clients look up spans by trace ID in Parquet. Measures concurrent lookup latency, CPU and the physical scan work after pruning. |
| `memtable-ui-c8` | Eight clients cycle the original UI pages over memtable rows. Measures live page and statement latency and server query cost. |
| `staged-ui-c8` | Eight clients cycle the same pages over staged rows. Measures page, statement and query cost in the staged residence. |
| `published-ui-c8` | Eight clients cycle the same pages over published rows. Measures published-data page latency, statement cost and physical scans. |

## Dependencies and simplification

- The approved Bifrost write-throughput requirement needs the full write
  matrix and preserved hot/Parquet read criteria. Its qualification must
  explicitly select all required cases if the routine suite omits any. The
  current L2 consumer obtains spans/logs/native rates from a compatible
  writes-parquet record. Preserve its required input coverage through the
  refactor. [REQ-013](../../bifrost-write-throughput-and-retention/spec.md#required-behavior) · [L2 calibration](../../../../crates/wyrd/wyrd-testing/src/bin/capacity/query/mod.rs#L1209)
- Variant-file-reads needs matched leaves and query shapes across memory,
  staged and published sources, plus canonical and learned layout evidence.
  Moving a case out of the routine suite leaves that evidence requirement in
  place. [AC-004](../../variant-file-reads/spec.md#acceptance)
- The Agent-only diagnostics and the large-payload Gateway cases have concrete
  investigation uses. Keep the former selectable and keep the latter in the
  routine c64 matrix. [REQ-020](../../benchmark-latency-diagnosis/spec.md#required-behavior) · [Gateway payload diagnosis](../../gateway-payload-diagnosis/design/notes.md)
- The UI volume and learned-layout treatments already use the same query
  workload. Keep their fixture identities and use one declaration path.
  Preserve exact-answer and durable-custody checks when reducing routine
  frequency. [UI case declarations](../../../../crates/wyrd/wyrd-testing/src/bin/capacity/query/ui/mod.rs)
- Use `mise run benchmark:l1:<category>` for the three routine sweeps, with
  explicit selection for targeted cases. See the [sweep and summary design](notes.md#5-category-sweeps-and-deterministic-summaries).

The suite choices are based on source registrations and current consumers. No measured redundancy or runtime savings have been established.
