# TASK-001 behavior acceptance review

Overall: **FAIL**.

Subject: base `8d25b13a1ededa23b9eb91973a2b489fe9224557`, candidate
`bcbe88527eccd7be4768cc39e1ba55b855300512`. First review of the complete
base-to-candidate range. Authority is approved specification revision 2 and
the original TASK-001 contract at `8387f3a05`; appended implementation evidence
is a verification claim, not acceptance authority. No prior coverage carried
forward. No other reviewer reports were read.

## Coverage and limits

Read the task-review skill and scope first, then the actual diff and surrounding
producers, recorder, startup, snapshot conversion, population validation, six
driver workflows, historical snapshot shape, production metric branches and
Gateway journey. No `.codegraph/` exists. Relevant authorities inspected include
the supplied root AGENTS instructions, `agent_docs/index.md`, Rust and maintainer
guides, `TESTING.md`, spec-driven development, the principal capture contract
and Bifrost acknowledgement definition. The diff touches no directory with a
more specific AGENTS file. Separate standards, maintainability, concurrency,
state-conservation and resilience reviewers cover their assigned specialist
boundaries; their results were not used to infer acceptance here.

All inspection was static. No tests, builds, benchmarks, installation or service
startup ran. The candidate records focused passing commands and owner-suite,
format, lint and dependency checks; I checked their source targets and intended
assertions, but did not independently reproduce those results. This report does
not establish performance or use implementation summaries as proof.

Navigation expanded beyond the starting map to the real fractional histogram
producers in `parquet/variant_residual.rs`, Scribe `parquet_writer.rs`, Forge
`managed/executor.rs`, Gateway `invocation.rs`, and the actual OTLP span fixture.
That expansion establishes B-001 through ordinary supported workload data.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence inspected | Result |
|---|---|---|---|
| REQ-001,002: one startup-selected metrics backend, before observations, no mixed fallback | `app::run` installs `WyrdTelemetryRuntime` before boot; `MetricsBackend::install` chooses one recorder; benchmark router serves snapshot without `/metrics`; explicit timing and standard metrics share `Registry` | Exclusive-startup test rejects a second recorder, checks initialized Gate families; ordinary-startup test verifies production rendering and empty benchmark observations | PASS |
| REQ-002: opt-in, loopback-only, ordinary production behavior | Config validation requires enabled metrics and loopback for benchmark mode; producer branches retain production histograms with benchmark mode off | Local-access test and config loopback test; retained production Gateway metric journeys | PASS |
| REQ-001,003: timing units, per-request residuals, no duplicate semantic duration | Gate write, Scribe ordinary append, direct Verification and Gateway handoff switch between precise timing and production histogram; elapsed/excluded/residual share one recording operation | Real producer tests assert one timing and absent duplicate histogram; interval tests exercise negative residual and out-of-range input | PASS |
| REQ-001,003,004: preserve diagnostic histogram units and populations | Standard histogram conversion assumes all non-`_seconds` observations are integers, contrary to existing fractional producers | Shared-observation test deliberately rejects fractions without covering the existing fractional families | **FAIL — B-001** |
| REQ-003: cohorts, late completions, resets, replacement and missing parts | Existing label classification and cumulative HDR interval machinery retained; `Population::collect`, `expect`, `only`, and `validate` preserve classified request populations; snapshots carry process identity and method | Concurrent interval test, population validation and persisted-evidence/cohort tests retain their assertions | PASS |
| AC-002 / scenario 3: maximum declared engine/level workload fits bounded recording capacity | Three independent bounds exist; complete declared workload footprint is not established | Overflow test pre-fills a private timing map; consumer test records a small fixed example; Gateway journey is two calls in one phase | **FAIL — B-002** |
| REQ-004: all six workflows read benchmark snapshots, including setup, polling, stop/drain and integrity | `LocalServer::metrics` converts `timings` JSON; Bifrost L1/L2 use it for insertion, residence, publication, slots and resources; Verification L1/L2 use snapshots for components, submit/queue means, acknowledgements, backlog and drain; Gateway L1/L2 use snapshot-derived capture, outbox, accounting and active-call metrics | Consumer matrix exercises real conversion and selected consumer helpers; four focused consumer regressions; source inspection follows remaining call sites | PASS for transport/consumer migration; diagnostic correctness fails under B-001 |
| REQ-004: acknowledgement/capture meaning retained | Gateway retains `outbox_write_seconds_count` as writes, distinct from items; admitted calls remain accounting histogram count. Verification merges timed successful appends and deduplicated-retry ack histogram | Gateway functional journey checks two tenant-visible captures, staged counts, accounting, outbox write and loss/pending readings; diagnostic test checks merged acknowledgement sample count | PASS |
| INV-001–003: security, capture reservation, accepted work, response and durability unchanged | Changes are instrumentation/backend/consumer paths; capture reservation, dispatch, outbox accepted-work and durable append mechanisms are unchanged; classification only chooses series | Existing production capture/post-response journeys retained, new benchmark journey observes returned results and retained captures | PASS |
| INV-004 / REQ-007: historical records immutable/readable; settings declared | History files unchanged; snapshot metrics and method maximum use serde defaults for earlier data; `timing_declaration` and README declare recorder and precision change | Existing history/cohort/report tests updated to moved types; no production optimization or controlled speedup claim | PASS |
| REQ-009 / AC-005: candidate disposition and required verification evidence | Candidate task inventory distinguishes retained/reworked/removed routes and untracked pre-existing packet; final claimed command list separates earlier failed setup and remaining limits | Named test source exists and checks relevant behavior; no interrupted run claimed as passing | PASS for disposition and recorded verification, subject to failures above |
| Non-goals: no TASK-002 diagnosis/optimization implementation, no public workflow, scheduler, admission or durability change | Full range contains recorder migration and evidence/docs; provider/capture execution policies stay with existing owners | Static diff inspection | PASS |

## Findings

### B-001 — INCORRECT: legitimate fractional diagnostics are rejected and invalidate benchmark windows

**Obligation:** REQ-001 and REQ-004 require every diagnostic histogram to flow
through the benchmark owner without lost units/populations or erased classes;
scenario 2 specifically requires non-time histograms not be scaled as seconds.

**Changed location:** `crates/shared/wyrd-benchmark/src/recorder.rs:100–112`
(`Metric::observe`), together with its non-time `HistogramUnit::Value`
selection at registration. The exact rejection condition is line 107.

**Producer → consumer proof:**

- `parquet/variant_residual.rs::variant_residual_share` computes residual
  uncompressed bytes divided by total Variant bytes, a legitimate ratio between
  zero and one. `VariantWriter::record` records that value in
  `bifrost_variant_residual_share` at lines 33–36.
- Scribe final artifact sealing calls it at `scribe/parquet_writer.rs:927`;
  Forge output processing also calls the same producer at
  `forge/managed/executor.rs:475`. These are ordinary writer paths, not dormant
  test-only instrumentation. The existing Bifrost benchmark OTLP fixture
  deliberately mixes shredded and residual attributes
  (`capacity/query/telemetry.rs:1–10`); L1 `writes-parquet` and L2 competing
  ingest use production publication settings (`query/mod.rs::PARQUET_ENV`).
  Publishing partially shredded span files therefore emits a fractional ratio.
- The new recorder leaves a non-time value unchanged, then requires
  `integer.fract() == 0.0`. A valid ratio such as 0.25 increments `rejected`
  without recording any histogram observation.
- `Population::from_interval` (`capacity.rs:521–527`) converts any such rejected
  observation into invalid evidence. Bifrost L1 invokes `population.validate`
  in `query/isolated.rs` after collection; L2 does so in
  `query/competing.rs:420`. Thus normal publication during a measured window
  loses a useful diagnostic and invalidates otherwise valid benchmark work.

The sibling producer `GatewayCall` accounting in
`components/gateway/invocation.rs:1183–1187` records decimal
`wyrd_gateway_call_cost` for priced calls and hits the same assumption. The
finding does not depend on the current instant upstream having prices; the
Bifrost fixture alone establishes supported benchmark reachability.

**Consequence/materiality:** This contradicts complete consumer closure for the
actual declared Bifrost workloads. Reporting the refusal explicitly does not
preserve the original metric: valid application observations have been
misclassified as invalid input. The shared-observation test's expectation that
every fractional non-time value be refused embeds the bad assumption rather
than proving compatibility with existing instrumentation.

**Smallest testable correction:** Correct non-time histogram representation in
the one benchmark recorder/snapshot owner so existing finite fractional metric
values retain declared units, counts and bounded precision. Keep integer count
histograms and nanosecond duration handling intact; do not multiply ratios by
seconds, round them to zero, omit these families or relax population validity.
No new collection pipeline or dependency is necessary. Extend the existing
shared-observation proof with `VariantWriter::Scribe.record(Some(0.25))` (or an
actual footer-derived partial ratio), snapshot differencing and population
validation: one observation, bounds containing 0.25 in ratio units, zero
rejected updates and no invalid reason. Include a representative decimal cost
through its existing metric interface to cover the sibling family. This is a
local representation correction, not an optimization or a production contract
change.

### B-002 — MISSING: required complete bounded-workload coverage proof is absent

**Obligation:** AC-002 explicitly requires maximum declared workload coverage
to fit bounded recording capacity. Original task scenario 3 requires a focused
case proving the complete engine/level workload fits without dropped
observations; it explicitly does not require an unauthorized benchmark run.

**Location:** `wyrd-benchmark/src/lib.rs::tests::series_beyond_the_bound_are_dropped`
at line 1340, `wyrd-testing/src/bin/capacity/main.rs:1689` consumer matrix, and
the scenario-3 proof set. The incomplete proof is the defect; this finding
does **not** assert that 256/64/1024 are necessarily too small.

**Evidence/scenario:** The timing-bound test inserts random private keys until
the map is full and proves refusal of one more key. It does not produce declared
cases, outcomes, phase parts, scan series or standard metric families. The
consumer matrix uses one snapshot pair with a small selection of observations;
it does not iterate workload declarations, warmup/repetitions, L2 stages or
setup/drain diagnostic series. The new real-server journey exercises two
Gateway calls in `steady`. These checks can all pass when a declared process
lifetime's complete series set cannot fit. No other added test establishes
that missing property. The candidate's evidence limit also explicitly reports
only Gateway journey coverage, but that admission is corroboration, not proof.

**Consequence/materiality:** Collection correctness is the delivered outcome.
The original mandatory acceptance proof remains incomplete across the six
supported workflows, including new standard histogram storage. Finding B-001
illustrates why a hand-picked snapshot cannot establish complete producer
coverage. No actual overflow is inferred from the missing check.

**Smallest testable correction:** Extend a focused existing in-process test
or provide a source-backed finite footprint calculation tied to current
workload declarations and real producers. Cover each process lifetime's
declared cases, phases, outcomes and phase boundaries plus required standard
metrics and setup/drain observations; assert retained counts and zero drops
within the three declared bounds. Reuse recorder/HDR and existing declaration
owners, without a new harness, release server, synthetic host load or live
benchmark. Change a bound only if that proof demonstrates necessity. This is
a bounded acceptance check, not an instruction to enumerate arbitrary
user-generated labels or every imaginable failure combination.

## Reuse and excluded proposals

The new owner appropriately extends the standard `metrics::Recorder` approach
already present in `crates/shared/wyrd-bench/src/recorder.rs`, while retaining
cumulative precise interval evidence absent from that tooling recorder. It
does not pull the test harness into server dependencies. Startup stays with
the telemetry/server lifecycle; driver preparation and draining stay with
existing workloads; snapshot conversion stays with `LocalServer`/`Population`.
No source-backed duplicate whole-workflow owner was found.

I did not turn hypothetical counter saturation, arbitrary key permutations,
or stylistic layout preferences into findings. Negative observations, missing
distributions and process replacement are already explicitly represented.
Scope does not authorize optimizations or new experiments to resolve the two
findings.

Final stability check: HEAD remains the candidate; tracked diff is empty.
Only the review directory and pre-existing gateway-latency-capture packet are
untracked. This reviewer wrote only this report.

Skipped: execution and performance qualification, as required by review scope.
Risk: accepted normal fractional diagnostics currently invalidate supported
benchmark windows; bounded full-workload capacity remains unproven.
