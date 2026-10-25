---
id: TASK-004-R1
kind: remediation
status: ready
spec: SPEC-benchmark-experiments
spec_revision: 8
requirements: [REQ-002, REQ-003, REQ-006, REQ-007, REQ-013, REQ-014, REQ-015, INV-001, INV-004, INV-005, INV-006, INV-008, INV-010, AC-001, AC-004, AC-005, AC-006, AC-008, AC-012]
depends_on: [TASK-004]
parent_task: TASK-004
remediates: [FIND-TASK-004-1, FIND-TASK-004-2, FIND-TASK-004-3, FIND-TASK-004-4, FIND-TASK-004-5, FIND-TASK-004-6]
---

# Complete cohort timing, accounting and retained evidence

## Subject and authority

Route this bounded task directly to `$wyrd-implement`. It corrects missing approved outcomes; it does not authorize another timing engine, collector, transport or record framework.

- Root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-experiments`.
- Approved spec: `changes/active/benchmark-experiments/spec.md`, revision 8, SHA-256 `66a583cfaf0fb1310b55b3c92987c8bad859d5628ca06de575ca57208b5eea96`. The maintainer explicitly approved this text in the preceding review session; approval persists despite stale draft wording.
- Original task: `changes/active/benchmark-experiments/tasks/TASK-004-server-benchmark-timing.md`, original at base and execution evidence at candidate.
- Original base: `0b500d677531ba501924b1c6d412fe14ff9aa5cd`.
- Reviewed candidate / remediation base: `a020500e8ea0c39311d508d6b5bfa85ebe90368c`.
- Independent acceptance context: this directory's `verdict.md`, `findings-validation.md`, all discovery reports and `verification.md`. Preserve these reports and stable finding IDs; they are the diagnosis for this task.

The current local HDR arithmetic, engine boundary timers, bounded snapshot, legacy reading and real smoke are accepted for the behavior they exercise. All named focused checks and the Postgres-backed tooling lane passed, as did eight real smoke invocations. Those proofs do not exercise actual admission-stop transitions, complete producer classification or interval artifact retention. Correct those seams without replacing already sufficient owners.

## Diagnosis and selected correction boundaries

### FIND-TASK-004-1 — Freeze measurement before request drain

**Why the current implementation fails.** REQ-006/007 and original Scenario 5 require completed work and CPU to stop at the declared admission-stop instant, with late work contributing to final cohort latency and separate drain accounting. All L1 engine paths and Gateway L2 instead await outstanding requests before closing their resource window. A request admitted at 9.9 seconds and answered at 12 seconds can extend a ten-second measurement to approximately twelve seconds and count its work/CPU as measured. Shared `Window::close` also reads CPU after awaiting the closing metrics scrape, although wall elapsed is recorded earlier.

**Evidence and affected consumers.** Shared `capacity.rs:348`; binary query `isolated.rs:123` and `run.rs:244,373`; Verification `run.rs:279,942`; Gateway `run.rs:553,656` and `capacity.rs:313,773,947`, all under `crates/wyrd/wyrd-testing/`. Their result conversions directly supply Repetition's W/T, W/C, C/W and provisioned throughput. Verification's closed driver records `late=false`; Gateway's final semaphore wait drains calls before closing. Capture/outbox settlement afterward does not recover a request-drain split. This is a routine slow-tail scenario and an explicit accounting failure, not clock-resolution hardening.

**Selected outcome.** Keep existing schedules, task/cancellation/cleanup owners and `Window`/`ResourceWindow`. Freeze server/driver CPU, measured elapsed and completed work at the existing declared stop before closing scrapes or waiting for in-flight results. Complete the already admitted cohort under its existing bounded lifecycle, retaining its latency and recording measured versus late work/counts/duration separately. Carry the distinction through affected driver results and Repetition conversion; adjusting a denominator after information is discarded is insufficient. Preserve Bifrost and Verification L2's existing late-work separation, and keep their practical freeze/scrape endpoints consistent. No synchronized clock service, new scheduler, weakened deadline or changed durability/capture ownership is needed.

### FIND-TASK-004-2 — Preserve offered cohort identity through actual producers

**Why the current implementation fails.** REQ-007 and original Scenario 1 require finite case/repetition/admission-phase/outcome populations, final cohort latency, independent incidental/probe traffic and matching scans. Actual outbound metadata carries only case; many producers do not even set that. Completion-time phase slices can exclude the slowest capacity requests when they finish in overload, or omit recovery requests finishing after stop. Unlabelled native/queued writes plus lower-bound reconciliation let unrelated observations cover missing offered evidence. Oracle scan facts can cross a snapshot before public query handoff, and the current mean calculation does not reconcile the matching final latency population.

**Evidence and affected consumers.** Client `transport/trace.rs:34,66,124`; tooling query `run.rs:191,590`, `competing.rs:328,428,529,555,576,668`; Verification `run.rs:733,752,777,838`; Gateway `capacity.rs:966,1030`; shared `capacity.rs:534,584,664,780`. A slow capacity-admitted call completing in overload is no longer judged in capacity p99, while later total validation can still succeed. This is the required supported interleaving scenario.

**Selected outcome.** Complete the existing benchmark-only scoped context with the approved closed case and repetition/admission-phase values at actual admission. Carry them through existing authenticated receiving context into the current timing/scan owners; select latency from the final drained admitted cohort, not completion-time slices. Reconcile attributable attempts, terminal outcomes and applicable scans for the same finite population; retain measurement-only work from finding 1. Keep incidental/probe writers in their own closed populations. Account for existing retries and pre-boundary refusals explicitly rather than demanding unconditional equality between logical calls and physical attempts or accepting unrelated lower bounds. Expand finite bounds only for the actual declared matrix, never arbitrary caller values.

**Native transport calibration.** Offered native writes use unary `InsertBatch`, not an intrinsically unlabelled long-lived stream. Existing path: `Sender::send` → tooling `BifrostWriter::write_batch` → `WriterPool::write_batch` → `SealedBatchSender` → `BifrostIngestSink` → `BifrostGrpcTransport::send_once/add_auth_metadata` → `trace::headers`. Scoped metadata can reach each awaited direct attempt and retry. Queued/background sends need a distinct closed incidental context at their actual producer/send execution point; they do not inherit a caller task-local automatically. Reuse these owners. Do not redesign stream/queue payloads, retry identity, allocation guards, resource ownership or durable acknowledgement. A stronger downstream count guard alone cannot recreate missing upstream identity.

### FIND-TASK-004-3 — Retain the actual interval distributions and structured evidence

**Why the current implementation fails.** REQ-007/013 and AC-001 require full target-only interval distributions, maxima, percentile ranks/value bounds, numeric reconciliation and window metadata. `Population` reduces snapshots to in-memory series, selected readings and health prose; `Reading::quantile` drops rank. No actual tooling caller writes full buckets or maxima. A successful v2 result therefore loses the only complete timing/scan evidence when the server stops. Reprinting a stored scalar does not regenerate its percentile from retained evidence.

**Evidence.** Shared `capacity.rs:519,555,600,727,736,747,780`; `experiment.rs:1117,1247,1541,1835`; binary `levels.rs:191` and all Population-to-Repetition conversions. The old `Collector::keep` retained observations; its transport was correctly retired, but that required outcome has no replacement. This affects every successful measured repetition, not only crash paths.

**Selected outcome.** Preserve actual selected interval evidence before summary reduction at the existing shared Population/artifact handoff. Use the current target run directory, bucket representation and experiment publication/error owners. Retain elapsed/excluded/residual and scan distributions for all terminal outcomes, finite cohort keys, method/process identity and snapshot/window boundaries. Keep structured admitted/measured/late/completed, expected/recorded and outcome reconciliation plus percentile ranks, exact bounds and maximum in machine-readable evidence/summary as required. Findings 1/2 produce those values; this correction retains them rather than reimplementing accounting.

Full buckets stay out of committed summary history. Target references remain optional, legacy v1 reading stays supported, and stored-summary reporting must work after target artifacts disappear. Failed required evidence writes must result in explicit incomplete/INVALID evidence, never a completed successful timing claim. Reuse the existing writer; neither more scalar percentiles nor health prose substitutes for full distributions. No new collector, report renderer, history writer, dependency or baseline migration is warranted.

### FIND-TASK-004-4 — Activate classification only after successful authentication

**Why the current implementation fails.** The approved classification contract uses authenticated receiving context. Outer HTTP/tonic request-span construction calls `label_request` before authentication, and unknown metadata immediately increments the authoritative global invalid counter. An unauthenticated `GET /healthz` with an unknown case, or a protected request subsequently refused, can therefore make an otherwise valid experiment INVALID. This is evidence integrity, not a tenant leak or permission bypass.

**Evidence.** Telemetry `timing.rs:173`; HTTP `router.rs:161,206`; tonic `server/mod.rs:228`; tooling consumer `capacity.rs:608`. OTLP timing surrounds authentication too, so moving the counter into generic timing recording alone is insufficient.

**Selected outcome.** Reuse existing successful authentication/context owners to activate closed benchmark metadata and its accounting. Cover HTTP bearer/browser/API-key, Gateway ingress and relevant gRPC verified-context paths consistently. Pre-auth/public/refused requests must not activate benchmark labels or change this invalidity counter. Authenticated unknown metadata must remain counted-invalid with raw values discarded. Preserve existing authentication results, audits, principal/tenant selection, permission checks and ordinary refused-request timing. Do not special-case health, suppress downstream invalidity or add another permission, endpoint, policy or registry. This correction is the authenticated activation seam of finding 2, not new authorization machinery.

### FIND-TASK-004-5 — Complete mandatory changed-item rustdoc

Root AGENTS and `agent_docs/rust.md` make new/changed-item rustdoc mandatory, including private fields, errors and possible test/helper panics. Complete the exact listed contracts in `findings-validation.md`: timing tuple fields, interval Result helpers and two tests; new client header-scope test; Gate `SlowScribe::is_ready`; server metrics `fetch`/`timings` helpers; loopback config test; Gateway histogram/phase/observed fixtures. Add required documentation to the remediation's own new/changed items as well. Preserve behavior/assertions and document actual failures/invariants; no unrelated old-code sweep, wording tests, wrapper or separate style round.

### FIND-TASK-004-6 — Use the required checked rank conversion

`timing.rs:690–696` explicitly suppresses truncation/sign-loss on `ceil(...) as u64`, contrary to `agent_docs/rust.md`'s “Never as-and-suppress” rule. Replace that one conversion with the existing checked conversion convention, preserving supported nearest-rank/clamp/bucket behavior and removing the suppression. The installed workspace `ToPrimitive` convention in `capacity::nearest_rank` is sufficient where needed; no new package, numeric abstraction, public API or percentile algorithm. This is a mandatory-rule correction; no normal-load rank bug or huge-count test is alleged or required.

## Preserved behavior and non-goals

Keep integer monotonic timer ownership and exact per-request excluded-wait unions, consistent bounded HDR snapshots, zero/overflow/reset handling, whole-bound threshold decisions, disabled-by-default local readout, loopback refusal and the operational Prometheus recorder. Keep real auth/tenancy/audit, request results, retries/refusals and durable boundaries. Preserve legacy report reading, collision-safe publication, normal tracing and the absence of a required operator exporter.

No new public SDK API, request/tenant identities in snapshots, arbitrary labels, second provider/recorder/collector/launcher, queue or concurrency/resource redesign, generic test harness, tail sampler or historical rewrite. Pooled-tenant metrics are not a separately validated finding; do not add tenant labels or expand into TASK-005 calibration/qualification work. Approved revision 8 already requires the selected corrections; no new material product decision is needed. A proposed alternative that changes such a boundary must return to specification authority rather than being bundled here.

## Acceptance and focused proof

| Finding | Required observable closure |
|---|---|
| FIND-TASK-004-1 | At an actual driver-to-Repetition seam, controlled final work crossing stop causes no later admission; measured work/CPU stop at the declared boundary before closing scrape/drain; final latency and late work count the completion exactly once. Cover affected closed-loop/read-write and streamed/open-loop callers, retaining correct L2 siblings. |
| FIND-TASK-004-2 | Actual outbound metadata and authenticated receiving context preserve finite case/repetition/admission phase through final outcomes. Slow capacity work finishing in overload and recovery work finishing after stop stay in their admitted cohort. Extra incidental native/queued work cannot cover missing offered evidence. Final applicable scan/timing counts reconcile, including handoff delay and bounded retry/refusal semantics. |
| FIND-TASK-004-3 | Actual shared collection/artifact handoff retains multi-outcome/scan data including zero and late/failure work. After in-memory/source state is gone, saved buckets reproduce p50/p95/p99/max, ranks, bounds and reconciled counts/windows. Evidence-write failure cannot complete successfully; removing target artifacts still allows unchanged stored-summary report rendering and legacy reading. |
| FIND-TASK-004-4 | With recording enabled, public/refused HTTP and gRPC requests with unknown metadata do not change classification invalidity; successful authenticated unknown metadata does. Gateway/HTTP/gRPC activation uses verified context while preserving refusal statuses, ordinary timing and existing authority. |
| FIND-TASK-004-5 | Source audit confirms required field/error/panic contracts on changed items; assertions remain intact. |
| FIND-TASK-004-6 | Prohibited conversion suppressions are absent; existing bounded-histogram rank/bound behavior and telemetry lints pass through the checked mechanism. |

Use existing owner fixtures and installed dependencies. Follow one-scenario RED/GREEN at the actual missing seam, then focused regression checks; do not substitute manually labelled snapshots for producer propagation or fabricated scalar records for persistence. Document each new regression's actual exact selector and expected RED cause. Do not select nonexistent names or pass after selecting zero tests. No new documentation-only runtime tests are required.

Focused existing commands, plus the exact new-regression selectors recorded by the implementer:

```bash
mise exec -- cargo nextest run --locked -p wyrd-telemetry --features test-support --lib -E 'test(=tests::benchmark_histograms_preserve_bounded_intervals)'
mise exec -- cargo nextest run --locked -p wyrd-client --features test-support --lib -E 'test(/^transport::trace::tests::/)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/^app::metrics::tests::/)'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --lib --bin capacity -E 'test(/^capacity::tests::/) | test(/^experiment::tests::/) | test(/^query::run::tests::/) | test(/^query::competing::tests::/) | test(/^gateway::run::tests::/) | test(/^gateway::capacity::tests::/) | test(/^verification::run::tests::/)'"
mise exec -- cargo fmt -p wyrd-telemetry -p wyrd-client -p wyrd-tonic -p wyrd-server -p wyrd-testing -p vala-bifrost-redux -- --check
mise exec -- cargo clippy --locked -p wyrd-telemetry -p wyrd-client -p wyrd-tonic -p wyrd-server -p wyrd-testing -p vala-bifrost-redux --features wyrd-telemetry/test-support,wyrd-client/test-support --all-targets -- -D warnings
mise exec -- git diff --check
```

Confirm actual newly affected auth/metadata test targets and exact selectors from source/nextest list, and use repository Postgres setup where their real dependency requires it. Run no Cargo work concurrently. If manifests/dependency edges change for the installed conversion mechanism, also run `mise run check:deps`. Scope format/lints to the actual correction write set if it differs from the listed owners. Do not run full L1/L2 suites, broad gates or full language journeys for remediation; final integrated verification retains broader release smoke/qualification obligations.

## Handoff

Record implementation evidence and the new cumulative commit identity without altering prior verdicts or historical claims. Commit/push only if separately asked. The next review checks these six closures, the prior-to-new candidate diff and affected producer/consumer paths, carrying forward unchanged acceptance; it must not restart whole-task discovery.

## Implementation evidence

Candidate commits on `a020500e8`: `b675505d0` (F6), `8900c01f9` (F5), `098bedd87` (F4), `0ffbac5c9` (F1/F2/F3 producers and engines), `e26acdf4d` (regressions, F5 Gateway fixtures), `46505b53f` (L2 future boxing). Cumulative candidate: `46505b53f`.

Reuse map: the existing owners are extended and nothing new is added beside them. `Window` gains `measure`, with `close` delegating to it; the freeze ordering lives in the private `until_stop`. `Population` gains exact `Expected` cohorts and keeps its snapshots. `Repetition` gains `reconciliation` and the skipped `timings`. `Reading` gains `rank`. The runner's existing artifact directory receives `keep_timings`. The task-local in `wyrd_client::transport::trace` now carries case plus phase; `benchmark_headers` exposes it to the Gateway's raw reqwest driver. The existing authentication owners call `activate_request`. There is no new dependency except workspace `num-traits` in `wyrd-telemetry`, which is approved and installed; `check:deps` passed.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| F1 freeze before drain, late work separate | `capacity.rs` `Window::measure` / `until_stop`. L1 query `isolated.rs` `closed` + `run.rs` `Admission`/`Outcome::late`. Verification `run.rs` `closed`/`drive` (late at stop). Verification L2 `observe` now freezes server and driver CPU before the closing scrape. Gateway L1 `run.rs` `step`/`drive`/`Samples::late`. Gateway L2 `capacity.rs` `step` (`measure` at schedule end, `late`). Bifrost L2 `competing.rs` (`measure` at window end). Readings: `late_requests`, `late_rows`, `verifications_after_window`, `drain_seconds` | `capacity::tests::the_measurement_freezes_at_the_stop_before_work_drains` (RED: freeze after drain never releases the work, so the test hangs; early-ending work must freeze before stop). `query::run::tests::completions_after_the_stop_are_late_and_kept` (RED: late completion counted as measured). `query::competing::tests::only_answers_before_the_window_end_are_work` | PASS |
| F2 cohort identity, exact reconciliation, incidental separation, scans | `trace.rs` `with_benchmark_labels(case, phase)` + `benchmark_headers`. Producers: query `Read::case`, `WriteOp::name`, `Phase::label`. Bifrost L2 `Admitting` (capacity / overload until grace end / recovery), offered writes, mixed queries, `probe` labelled. Verification `admission` (grace joins overload), `Op::case`. Gateway `Case::label`, `Period::label`, `mode`, `cohort`; `workload.rs` `exchange` sends headers. Exact `Population::expect` per (boundary, outcome, case, phase); wrong answers owed as success; refusals/errors not owed and counted beside. `scanned_per_query` requires scans = matching timings | `query::competing::tests::cohorts_follow_admission_not_completion` (RED: capacity call completing in overload leaves capacity p99, or a probe / another phase covers a missing offered observation). `verification::run::tests::recovery_grace_arrivals_join_overload`. `capacity::tests::snapshot_windows_keep_classes_and_scans_apart` (RED: scans averaged over another cohort's timings). `transport::trace::tests::scoped_benchmark_labels_are_sent` | PASS |
| F3 retained distributions and structured evidence | `Population::between` keeps snapshots; `validate(repetition, window)` pushes `TimingEvidence` + `Reconciled`. `percentiles` adds the `max` reading; `Reading::quantile` records rank. `experiment.rs` `keep_timings` writes `timings/<step>/<phase>.json` under target artifacts; failure appends INVALID reason. `timings` is `serde(skip)` so buckets stay out of the summary | `experiment::tests::kept_timing_evidence_rebuilds_judged_readings` (RED: missing file, a rebuilt p50/p95/p99/max/rank/bound differing, or an unwritable directory not invalidating). Existing `reports_regenerate_from_persisted_records` (artifacts removed) and `histogram_records_preserve_method_and_legacy_reports` (v1) | PASS |
| F4 activation after authentication | `timing.rs` `Pending` / `activate_request`. HTTP `authenticate.rs` success paths, Gateway `ingress.rs`, gRPC `principal_extractor.rs`, Gate `AuthContext::from_verified` | `wyrd-telemetry` `tests::benchmark_histograms_preserve_bounded_intervals`. `wyrd-server` `http::middleware::authenticate::pg_tests::only_authenticated_requests_activate_benchmark_labels`. `vala-bifrost-redux` `gate::tests::only_verified_writes_activate_benchmark_labels` (RED: refused/public request with unknown labels changes `invalid_labels`) | PASS |
| F5 rustdoc | `8900c01f9`, plus Gateway `timing`/`phase`/`observed` fixture `# Panics` in `e26acdf4d`, and docs on every new item | source audit; clippy `-D warnings` | PASS |
| F6 checked rank | `timing.rs` nearest rank via `ToPrimitive` with clamp; suppression removed | `timing::tests::*`, clippy | PASS |

Commands (all passed, run sequentially):

```bash
mise exec -- cargo nextest run --locked -p wyrd-telemetry --features test-support --lib -E 'test(=tests::benchmark_histograms_preserve_bounded_intervals) | test(/^timing::tests::/)'   # 3 passed
mise exec -- cargo nextest run --locked -p wyrd-client --features test-support --lib -E 'test(/^transport::trace::tests::/)'   # 6 passed
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/^app::metrics::tests::/)'   # 9 passed
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=gate::tests::only_verified_writes_activate_benchmark_labels)'   # 1 passed
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --lib --bin capacity -E 'test(/^capacity::tests::/) | test(/^experiment::tests::/) | test(/^query::run::tests::/) | test(/^query::competing::tests::/) | test(/^gateway::run::tests::/) | test(/^gateway::capacity::tests::/) | test(/^verification::run::tests::/)' && cargo nextest run --locked -p wyrd-server --lib -E 'test(=http::middleware::authenticate::pg_tests::only_authenticated_requests_activate_benchmark_labels)'"   # 34 passed; 1 passed
mise exec -- cargo fmt -p wyrd-telemetry -p wyrd-client -p wyrd-tonic -p wyrd-server -p wyrd-testing -p vala-bifrost-redux -- --check
mise exec -- cargo clippy --locked -p wyrd-telemetry -p wyrd-client -p wyrd-tonic -p wyrd-server -p wyrd-testing -p vala-bifrost-redux --features wyrd-telemetry/test-support,wyrd-client/test-support --all-targets -- -D warnings
mise run check:deps
mise exec -- git diff --check
```

Material limits:

- Queued/background lane writes and the overload filler queries stay `none/none`. That is their own closed population, so they cannot cover an offered cohort, but they are not individually reconciled.
- Visibility probes are labelled `probe` and are not reconciled. Their client poll count is not kept.
- The Verification per-second recovery curve stays completion-time diagnostic.
- `MAX_SERIES` is 256 and `MAX_SCAN_SERIES` is 64, so they cover the declared label matrix.
- The Gateway ingress activation has no dedicated fixture. Its path is the same `activate_request` call, exercised by the real smoke's labelled Gateway cohorts.
- No real L1/L2 run or release smoke was executed for this remediation.

Non-goals stayed excluded: no new collector, transport, SDK API, tenant label or dependency beyond the installed `num-traits`. No unrelated files changed. The untracked `benchmarks/` result directories predate this task.
