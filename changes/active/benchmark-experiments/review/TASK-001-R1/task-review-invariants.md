# TASK-001 invariant review

**FAIL** — first complete review; no prior acceptance evidence.

Subject: base `0739bbf38af7cc05bbfefb09850490c55432ec2d`, candidate `aaa152dd4243d57752b73d3a4e9c140bccc7d171`, full three-dot range. Approved revision-5 spec and original TASK-001 govern. Scope: exact timing producer/export/collector/phase/judgment and CPU/work/window invariants across engines. The two pre-existing uncommitted repairs are excluded from acceptance.

Read root rules, task-review skill, specification, relevant task metrics/collection/proof sections, spec-driven-development, maintainer style, and Bifrost Telemetry/Measurement meanings. CodeGraph was first navigation. No nested rules apply per supplied directory inventory. Read actual Git diff and surrounding source; only the two identified repair files differ in current source. No source changes, Cargo or benchmarks run by this reviewer; orchestration owns sequential verification. Parent reports existing i64 export regression and nine experiment tests passed, and lints passed; those results do not change candidate source identity.

## Coverage and existing owners

`record_timing` and installed tracing-opentelemetry `SpanAttributeVisitor`; query QueryTiming; Gate native write timing; OTLP timed services; Verification ExecutionTelemetry; Gateway CallTelemetry and retry wait union; capacity Timing/decode_timings/nanos/Intake/Collector/Population; isolated and mixed Bifrost, closed/competing Verification and both Gateway measurement paths; Window/ResourceWindow/sysinfo resource owners; Repetition::derive, nearest-rank readings and validity. Existing mechanism comparison: extend these owners, retain the operator OTLP path and request trace context. No second exporter, transport, query scan, public endpoint or generic measurement store is earned. Baseline transcription, complete workload coverage, history, and process lifecycle are sibling review scopes.

## Scoped acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-007 exact integers reach benchmark through production exporter | u64 producer versus integer-only decoder | Candidate V4 constructs IntValue, telemetry capture compares stringified values; pending real-export regression outside candidate | FAIL, INV-01 |
| REQ-007 nearest-rank/per-request residuals, overlapping waits counted once, negative invalid | Timing::residual_ns; Population::percentiles; block/provider interval unions | Candidate arithmetic/union tests; parent experiment checks passed | PASS for inspected arithmetic |
| REQ-006 separate collector/helper accounting | Collector Tokio service inside driver; own CPU charged to driver | Direct source contradiction | FAIL, INV-02 |
| Bounded collection; overflow INVALID | unbounded Received Vecs, usize::MAX message limit | No overflow path exists | FAIL, INV-03 |
| REQ-007 missing timings cannot be masked by other traffic | mixed lower-bound write counts share boundaries with queue lanes | Existing exact-window tests do not cover substitution | FAIL, INV-04 |
| REQ-006 W/C/T share window | Bifrost deadline resource close, totals include drained completions | Reachable final in-flight request | FAIL, INV-05 |
| REQ-007 request-specific scan rows/bytes beside server time | aggregate Oracle counters, isolated averages, no L2 scan data | Scan-stats summation test does not test association | FAIL, INV-06 |
| REQ-006 portable CPU and normalization arithmetic | sysinfo, ResourceWindow, Repetition::derive; P before/after | Candidate missing/reset/zero/P/unit tests; no hardware runs by reviewer | PASS for static arithmetic, execution qualification absent |
| REQ-002/003 invalid repetitions cannot become valid successful subset | Population.validate and shared validity | Supported for exact windows; mixed loss can be masked | FAIL through INV-04 |

## Proposed findings

### INV-01 — INCORRECT — real exported durations are rejected

Obligation: REQ-007 integer nanoseconds and complete standard OTLP collection; task section 4/V4.

Location: candidate `crates/shared/wyrd-telemetry/src/lib.rs:332–344`; `crates/wyrd/wyrd-testing/src/capacity.rs:528–598`; synthetic test `experiment.rs:3938–3970`.

Evidence/path: record_timing converts Duration to u64. Installed tracing-opentelemetry SpanAttributeVisitor implements record_i64 but no record_u64; its record_debug emits Value::String. The tracing visitor default therefore turns u64 into text. Decoder nanos accepts only IntValue and emits “a non-integer duration” otherwise. Intake keeps malformed reasons; collect waits for never-decodable timings and invalidates. All native/query/OTLP/Verification/Gateway producers call this shared owner. Candidate telemetry test stringifies attributes; V4 directly manufactures IntValue and bypasses production encoding.

Consequence/materiality: ordinary requests yield no usable server timing on every engine. This source path explains the human's error without reconstructing deleted artifacts.

Smallest correction: emit signed OTLP integers at record_timing, documenting saturation, and preserve strict decoder validation. Accepting debug strings in the consumer preserves the broken producer contract and is unnecessary. Pending i64 repair sits at the correct root; pending real-export regression traverses init → exporter → collector and asserts exact values >u32::MAX. Both are excluded from candidate; parent now reports that repair test passed. Closure: retain/run that regression and exercise each real release-server path in the human-requested mandatory smoke before long runs. No new exporter/dependency/endpoint.

### INV-02 — VIOLATION — collector work is charged to driver CPU

Obligation: REQ-006 explicitly makes collector a helper separate from server and driver CPU; spec timing paragraph requires separate collector cost; task section 4 requires helper PID/resource/health evidence.

Location: capacity.rs:647–654,667–696,734–740; driver_cpu_seconds at 449; Window open/close at 327–360; experiment.rs:2087–2109.

Evidence/path: Collector::start uses tokio::spawn inside capacity. Its rustdoc and declaration expressly say collector CPU is driver CPU. driver_cpu_seconds measures accumulated CPU for that same process; all engines copy it into Repetition and shared validity judges saturation. No independent helper CPU/PID evidence exists. Quiet-host descendant exclusion cannot split the CPU total.

Consequence/materiality: collector decoding/export volume can cause driver.saturated while traffic generation itself remains eligible; normal records cannot distinguish their costs as explicitly required.

Correction boundary: keep Collector and existing portable process resource owners; provide independently attributable helper accounting, declared helper identity and collection/resource-health evidence while leaving server tracing/export in server CPU. Reuse an existing mechanism if it can supply accurate separate attribution; do not prescribe a second collector/transport or estimate/subtract CPU. Merely relabeling the same process reading does not close it. If no portable existing mechanism can separate same-process work, running the existing collector as a separately measured helper is consistent with the already-approved helper boundary, but private process mechanics are implementation-owned. Focused proof must show independently reported collection cost and traffic-driver cost plus missing helper health handled honestly. This is required accounting, not speculative isolation or a new product decision.

### INV-03 — MISSING — no bound or overflow invalidation for collection

Obligation: spec opening requires bounded collection and overflow invalidates affected measurement; REQ-007 repeats it.

Location: capacity.rs:611–640,679,776–889.

Evidence/path: Received has unbounded Vec<Timing> and Vec<String>, extended on every export. Service sets max_decoding_message_size(usize::MAX). Collection happens after workload windows and, for Bifrost preparation, after fixture loading/settling. There is no retained count/byte bound or overflow state; raw append happens after collecting the entire population. BSP queue length bounds server pending batches, not collector accumulation over repeatedly drained batches.

Consequence/materiality: required full-recording six-minute windows and setup traffic accumulate with request volume; exhaustion can kill the driver before partial INVALID evidence is saved. Independently, the explicit overflow-to-INVALID requirement has no implementation path. This does not depend on a malicious remote caller or require a synthetic host-load test.

Smallest correction: bound existing collector retention/incoming data with a declared limit, retain a specific loss/overflow reason for the affected window, and stop retaining excess. Bound malformed diagnostics too; preserve in-limit raw evidence. Reuse this collector, no generic store or new dependency. Proof: intentionally exceed a small test bound through real OTLP and assert bounded retention plus specific INVALID; in-limit sample remains exact.

### INV-04 — INCORRECT — mixed traffic can replace a missing required timing

Obligation: REQ-007 incomplete populations INVALID and request/handoff reconciliation; task section 4 forbids judging a successful subset after dropped required observations.

Location: query/competing.rs:324–359; capacity.rs:763–889; mixed write percentiles query/competing.rs:579–593. Sibling producers: client_queue lanes and visibility probes started at 252–267.

Evidence/path: timings derives write minima from acknowledged offered ingest only. Collector::traced rejects counts below those minima but permits extra observations; trace requirements cover successful explicit queries only. Native client_queue writes concurrently emit the same native-write boundary. Dropping one offered native-write timing and receiving one lane timing still satisfies the count; no offered-write identity association detects substitution. Subsequent p99 judges successful timings of that boundary by phase. Span-ID duplication detection does not detect one missing distinct request.

Consequence/materiality: dropped or slow offered-write observation disappears from a judged population without INVALID. This is supported mixed-load behavior, not invented input hardening; successful lane traffic makes the current completeness test unable to prove its claim.

Smallest correction: reconcile each required producer's observations using existing request tracing/correlation, or exact totals over all relevant writers if existing evidence can supply them. Reuse request_trace/trace propagation and Collector, preserve background observations with their population identity; lower-bound counts alone are insufficient. No need to redesign traffic scheduling. Proof: satisfy present minimum using unrelated same-boundary spans while omitting one required offered write; it must remain INVALID; complete matched mixed population succeeds.

### INV-05 — INCORRECT — Bifrost credits post-window work against frozen CPU/T

Obligation: REQ-006 C and W share repetition window; REQ-007 metric meanings.

Location: query/competing.rs:241–305,450–475,544–550,618–624; query/mod.rs:910–919; experiment.rs:1258–1310.

Evidence/path: closer closes Window at end, freezing CPU and wall duration. tokio::join still waits for pending writes and closed-loop queries sent before end. read_queries counts every successful Asked without at < phases.end; read_ingest sums every successful Acked including after end. These totals feed W/T and W/C. Example: final query begins just before end and completes one second afterward; it contributes to W though its post-end CPU does not contribute to C. Bounded drain is required supported L2 behavior, so this is realistic.

Consequence/materiality: rate/efficiency inflate and CPU/operation understates cost; extra drain work can improve apparent efficiency despite added server cost.

Smallest correction: use existing completion timestamps/phase boundaries to restrict measured work/byte numerators to the same interval as C/T; keep late completion/error/loss evidence in drain/integrity diagnostics. Preserve all failures; do not extend fixed repetitions arbitrarily or remove server background CPU. Proof: one before-end and one after-end completion count only the former in measured W, preserve both in total completion/drain reconciliation, and recompute derivatives. Inspect analogous Verification totals while correcting same invariant (observer freezes CPU before joined arrivals/drain); this fully established finding is Bifrost's path.

### INV-06 — MISSING — per-query scan facts cannot join their server timing

Obligation: REQ-007 requires executed-plan scan rows and bytes per query alongside that request's server time; AC-005/task section 4 repeat it.

Location: vala-bifrost-redux/src/oracle/mod.rs:558–615; query/isolated.rs:36–41,164–175; capacity.rs:482–503; query/competing.rs:330–359,512–552; query/mod.rs:316–324.

Evidence/path: Oracle already owns per-query scan_stats but added row facts are emitted only as a process counter. Isolated divides scrape-delta sums by successful query count. Timing retains duration/identity/outcome only, with no rows/bytes. L2 mixed reading consumes neither scan counters nor correlated scan facts. Two queries with different scan volume/time cannot be reconstructed as distinct triples. Metric summation can also make an absent counter appear as zero instead of unavailable.

Consequence/materiality: required request-level scan/time comparison is absent, so engineer cannot tell whether a slow request scanned more data or independently recompute the prescribed evidence. This is an explicit metric, not an optional observability feature.

Smallest correction: attach already-owned final Oracle scan facts to existing query tracing/OTLP context, then join them to timing by trace/process/window through shared collection. Keep unavailable facts explicit. Reuse OracleQueryScanStats/QueryTelemetryGuard, no additional query scan, endpoint or identity-bearing metric label. Aggregate counters can remain diagnostic. Proof: two queries with known distinct rows/bytes/durations, exported out of order, retain correct triples; missing facts stay unavailable/INVALID as applicable; both isolated/mixed consumers retain the evidence.

## Recommendation and limits

These proposals require fresh independent source validation; agreement is not acceptance. Candidate arithmetic-level evidence does not establish working production collection. The i64 repair alone cannot close the other required invariants. No runtime reruns, deleted-artifact reconstruction, new profiling/OS mechanism, or engine optimization was attempted. No material spec change is needed for the stated outcomes; implementation mechanics remain with existing owners.
