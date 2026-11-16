# Independent findings validation — TASK-001

Result: **FIX_REQUIRED**. Five deduplicated findings remain. No specification
revision or additional comprehensive discovery is required.

## Subject, independence and limits

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/resource-scaling`.
- Base: `8d25b13a1ededa23b9eb91973a2b489fe9224557`.
- Candidate: `bcbe88527eccd7be4768cc39e1ba55b855300512`.
- First cumulative review; no previous acceptance or closure is carried forward.
- Authority: approved specification revision 2; original TASK-001 at
  `8387f3a05fa018012d049104ffd7865074b23534`, read separately from candidate
  implementation claims; root rules, task-review/implementation skills,
  spec-driven workflow, Rust and maintainer guides, applicable Bifrost telemetry
  and operational-indicator/configuration authority.

This fresh validator read all six complete discovery reports, scope and claims
comparison. It independently inspected the proposed defects, surrounding source,
complete correction-target function bodies, sibling callers and existing owners.
Agreement between reports was not treated as proof. No new blocker was discovered.
No `.codegraph/` directory exists. Only this report was written.

All validation was static. No tests, builds, benchmarks, services, installations
or synthetic host load ran. A check was unnecessary to decide these source-proven
paths and explicit evidence gaps. Candidate command results are recorded claims,
not independently reproduced results. This report establishes neither performance
nor actual exhaustion of a recording bound.

## Specialist applicability and completeness

| Contract or invariant | Required coverage and validation |
|---|---|
| Complete acceptance and non-goals | Fresh behavior report covers the original scenarios, all six engine/level consumers, producer deduplication, capture/ACK meaning, history and disposition. Complete. |
| Shared state and conservation | Invariant report traces standard/explicit producers through cumulative snapshots, interval reconstruction, Population and consumers. Complex shared state makes this role applicable. Complete; its raw-projection PASS is reconciled with the separately established setup/drain failure below. |
| Repository rules across tiers | Standards report inventories all 47 changed paths, dependency direction, test placement, rustdoc, diagnostics and evidence obligations. Broad shared/server/tonic/testing changes justify this role. Complete. |
| Ownership and abstraction | Maintainer report compares the existing `wyrd-bench` recorder, moved HDR owner, server lifecycle, conversion and consumer aggregation. Substantial ownership changes justify this role. Complete; no material DRIFT finding. |
| Failure propagation and recovery | System report follows deployed startup, listener supervision, snapshot errors, replacement, drain and shutdown. New backend/projection failure semantics make this role applicable. Complete. |
| Concurrency | Domain report covers handle lifetime, registration/update serialization, cumulative snapshot cuts, atomic request parts, installation publication and lock ordering. Applicable and complete; no material concurrency finding. |
| Security/RBAC/tenancy | Separate domain review is not required: existing loopback/enabled validation is retained, closed classification carries no authority, and the range changes no token, permission, identity or tenant-selection behavior. Behavior/standards coverage includes local exposure and bounded non-secret labels. |
| Persistent data/SQL | Separate data-layer review is not required: recording and projection do not change transactions, RLS, schemas, durable writers or accepted-work ownership. Behavior/invariant/system coverage traces capture/ACK consumers and unchanged durable owners. |

All selected reports are available and cover their assigned boundaries. No missing
specialist or unresolved producer-to-consumer gap requires another discovery pass.
Unique Y-001 and S-* proposals were validated directly; uniqueness is not a reason
for follow-up. A later focused diagnostician required by finding 5 is correction
of missing execution evidence, not a vote on this ledger.

## Proposal disposition

| Discovery IDs | Decision | Stable finding |
|---|---|---|
| B-001, I-1 | REVISED: functional mismatch confirmed; choose the shared representation correction below, without claiming the tooling recorder already supports non-time fractions | FIND-TASK-001-1 |
| B-002, I-2 | REVISED: retain the explicit missing bound proof; restrict coverage to reachable declared workloads within their actual process lifetimes | FIND-TASK-001-2 |
| Y-001 | CONFIRMED: lossy raw projection bypasses refusal evidence on real setup/drain consumers | FIND-TASK-001-3 |
| S-1 | CONFIRMED: mandatory changed-item documentation gate | FIND-TASK-001-4 |
| S-2 | CONFIRMED: mandatory independent diagnosis record is missing; no assertion weakening established | FIND-TASK-001-5 |

No proposed finding is rejected outright. The standards report's function-scoped
import observation has no demonstrated material consequence and is not retained
as a finding or remediation work. Alternative locking/layout preferences,
unmeasured contention, arbitrary label products and speculative saturation
hardening likewise create no work.

## Validated ledger

### FIND-TASK-001-1 — Valid fractional diagnostics are refused

**Status/classification:** REVISED / INCORRECT. Sources: B-001, I-1.

**Obligation:** REQ-001/004, AC-001 and original scenario 2 require real diagnostic
histograms to preserve units and populations; non-time does not mean integer-only.

**Location:** `crates/shared/wyrd-benchmark/src/recorder.rs:99–112`
(`Metric::observe`), registration at 179–190, and the corresponding
`HistogramUnit`/snapshot/interval representation in that owner.

**Producer → consumer:** `parquet/variant_residual.rs:49–70` divides residual
uncompressed bytes by total Variant bytes. `VariantWriter::record` at 33–36
emits this legitimate ratio through the standard histogram interface. Final
Scribe sealing calls it at `scribe/parquet_writer.rs:926–927`; Forge output
measurement calls the same owner at `forge/managed/executor.rs:475`. The declared
OTLP fixture deliberately mixes shredded and residual attributes
(`capacity/query/telemetry.rs:1–10`), while WritesParquet and competing ingest
permit publication during measured work (`query/mod.rs:136–160`). Partial
shredding therefore reaches this path in supported Bifrost workloads.

Registration chooses `HistogramUnit::Value` for this non-`_seconds` family.
`observe` leaves 0.25 unchanged, fails `integer.fract() == 0.0`, increments
`rejected`, and records no sample. `IntervalMetric::between` preserves that
rejection delta; `Population::from_interval` at `capacity.rs:521–527` makes it
invalid evidence, and `Population::validate` at 669–680 transfers that invalidity
to the repetition. The sibling `wyrd_gateway_call_cost` producer at
`gateway/invocation.rs:1183–1187` can emit decimal costs and shares the same
assumption. Current instant-upstream pricing is not needed to establish the
Bifrost failure.

**Consequence/materiality:** ordinary valid publication diagnostics are lost and
can invalidate otherwise correct benchmark work. Explicitly reporting rejection
does not satisfy preservation of valid observations. The shared-observation test
at `lib.rs:1190–1310` currently codifies the mismatch by expecting 0.5 rejection.

**Smallest safe correction:** correct fractional non-time representation in the
sole `wyrd-benchmark` owner, using bounded fixed-point HDR encoding with an
explicit scale/unit declaration that snapshot and interval consumers can interpret
in original metric units. Keep duration nanoseconds and integer count-family
semantics intact; select and document precision/range against actual producer
values. Preserve genuine non-finite/out-of-range refusal. Do not delete the ratio
or cost producers, round them to whole units, suppress Population invalidity, or
introduce another collector/dependency. Read/update the corresponding snapshot,
interval reconstruction/selection and precision documentation together so scaled
buckets cannot masquerade as whole values.

The existing tooling recorder at `wyrd-bench/src/recorder.rs:422–430` illustrates
scaling for **durations only**; its non-time truncation is not a ready-made correct
solution for these fractions. Reuse HDR and the existing snapshot workflow, not
that truncation behavior or the tooling dependency. Deletion and a downstream
guard cannot preserve the required diagnostic. The burden is a local numeric
representation/conversion correction, not a lifecycle or product decision.

**Focused closure:** through real existing metric interfaces, record a
representative residual ratio (Scribe/Forge's shared producer), a decimal cost,
an integer count histogram and a seconds histogram. Difference snapshots and
verify counts, declared units/scale and bounds containing the original values,
zero rejection and no Population invalidity. Keep invalid-value and duration
regressions. An in-process owner/consumer test suffices; no live benchmark is needed.

### FIND-TASK-001-2 — Complete declared-workload bound proof is missing

**Status/classification:** REVISED / MISSING. Sources: B-002, I-2.

**Obligation:** AC-002 and original scenario 3 explicitly require the complete
declared engine/level workload to fit bounded recording capacity and focused proof
that detects insufficient bounds. This is not an inferred performance target.

**Location:** `wyrd-benchmark/src/lib.rs:75–78,1340–1364`,
`recorder.rs:28,140–203`, and the scenario-3 proof set, including
`capacity/main.rs:1689`'s small consumer example.

**Evidence/reachability:** `Registry::{record,scan,metric}` retain series for the
entire process and refuse new series at 256/64/1024 respectively. Snapshot reads
do not reset those owners. Workload drivers emit multiple admitted cases, phases,
outcomes and timing parts across setup, warmup and measurements; standard
instrumentation adds separate series. The existing overflow test fills only the
private timing map with arbitrary keys and proves the next key is refused. The
consumer test records a selected small set in one pair of snapshots. The Gateway
journey emits two calls in `steady`. No MAX_* reference, focused test or supplied
source-derived footprint calculation establishes the complete reachable footprint
against all three caps. Candidate evidence at task line 373 explicitly limits
its qualification to the Gateway journey/bounded tests.

**Consequence/materiality:** an expressly required acceptance proof is absent.
Neither current source nor this review establishes that any cap actually
overflows; do not turn the evidence gap into that assertion. Repetitions which
reuse keys do not multiply cardinality, and separate server lifetimes must not
be combined into an impossible global workload.

**Smallest safe correction:** extend the existing in-process coverage proof with
a source-backed finite series-footprint derivation for each actual declared
engine/level process lifetime. Tie it to the existing declaration and producer
owners: reachable cases/phases/outcomes/parts, scan series and standard families,
including incidental setup/drain observations. Exercise the distinct reachable
series through existing recorder/producer interfaces and assert retained counts
and zero capacity refusals against all three caps. A documented finite bound can
cover unchanged standard families where replaying real server work would require
an unnecessary harness. Preserve the separate overflow-negative check. Increase
a bound only if the derived supported footprint requires it.

Deletion cannot discharge an explicit acceptance requirement. Reuse declarations
and existing tests rather than a new framework, release server, synthetic load or
arbitrary Cartesian product. This bounded proof is materially cheaper than live
qualification and does not authorize a benchmark run.

**Focused closure:** a reviewer can map every counted distinct series to its
producer and actual process lifetime, compare the resulting three upper bounds
with the declared caps, and see focused retained-count/no-drop assertions. The
proof must fail if a cap is reduced below that reachable footprint. No requirement
to enumerate arbitrary external labels or every conceivable failure combination.

### FIND-TASK-001-3 — Raw metric projection erases known incomplete evidence

**Status/classification:** CONFIRMED / INCORRECT. Source: Y-001.

**Obligation:** REQ-003/004 and original scenarios 3–4 require explicit refusal
evidence through setup, polling and drain; missing evidence must not silently
become successful zero backlog or healthy residence.

**Location:** `wyrd-testing/src/release_server.rs:358–359,568–594`
(`LocalServer::metrics`, `Metrics::of_snapshot`).

**Producer → consumer:** `Registry::metric` increments snapshot `dropped` and
returns no slot when the standard-series cap is reached; registration supplies a
no-op handle. `Metric::gauge` increments `rejected` and preserves the previous
value on an unrepresentable update. `Registry::snapshot` emits both failure fields
in successful JSON. `Metrics::of_snapshot` discards them and retains numeric
values only. `Metrics::sum` at 617–632 returns zero for absent series.

Gateway `settle` at `gateway/run.rs:259–269` accepts zero active calls plus pending
items. Verification `drain`/`backlog` at `verification/run.rs:933–956` accepts zero
queue/outbox backlog. Query `Residence::{read,all_published}` at
`query/mod.rs:1619–1640` treats missing live gauges as zero if any published file
exists. These are real driver paths, including before/after measured intervals.
The generic registration/refusal mechanism can omit any newly registered gauge;
the defect requires no claim that healthy declared workloads exceed 1024 series.

**Consequence/materiality:** a snapshot already known to be incomplete can yield
successful setup or settled evidence. This is specifically required failure
handling, not speculative demand for additional health instrumentation. Population
validation does catch refusal deltas within measured intervals, but it does not
protect these raw reads or reconstruct a gauge refused before a later baseline.
No whole-experiment false PASS or actual accepted-work loss is inferred.

**Smallest safe correction:** make the existing snapshot-to-Metrics boundary
fallible and reject cumulative `dropped > 0` or any metric `rejected > 0` before
discarding metadata. Propagate through the existing `LocalServer::metrics` Result
and update every direct conversion caller: the capacity consumer test, ignored
calibration `pg_tests.rs::scrape`, and benchmark Gateway journey. Keep valid
never-emitted-family semantics and production `Metrics::parse` unchanged.

This guard belongs at the projection because it owns the loss of validity
metadata and no longer has enough evidence downstream to recover it. One
fallible conversion is smaller and safer than per-engine guards, required-family
lists, retries or a new lifecycle service. It aborts invalid benchmark evidence;
it does not change server drain/accepted-work ownership or shared-service policy.

**Focused closure:** in-process snapshot fixtures prove a nonzero drop with absent
backlog gauges and a nonzero rejection with a stale-zero gauge both fail
conversion. A healthy snapshot still preserves counters/gauges and histogram
observation counts, including zero for a valid unused family. Keep the existing
six-consumer proof; no service or broad journey rerun is needed for this gap.

### FIND-TASK-001-4 — Changed-item rustdoc gate is incomplete

**Status/classification:** CONFIRMED / VIOLATION. Source: S-1.

**Obligation:** root Code rules and `agent_docs/rust.md` Rustdoc explicitly require
documentation for every new/materially changed item, including private fields,
and `# Errors` for fallible functions. Missing rustdoc blocks merge.

**Locations/evidence:**

- `verification/run.rs:1113–1122`: new `acknowledgements` returns
  `Result<Distribution, String>` but has no `# Errors`. The full body can return
  histogram-selection/refusal errors when no append distribution is available,
  or forward a merge error; caller `Bench::competing` turns the result into
  evidence readings, and diagnostic/consumer tests also call it.
- `app/server.rs:95,393`: renamed/retyped `metrics: Option<MetricsBackend>` fields
  have no rustdoc. `new_with_metrics` retains the preinstalled backend, `bind`
  transfers it, and `BoundServer::run` consumes it or installs the configured
  backend when a metrics listener is present. These are materially changed
  ownership fields, not unrelated pre-existing documentation debt.
- `wyrd-benchmark/src/lib.rs:200,245,271,1139`: moved/changed owner docs reference
  removed `enable`; current activation is `install`, observed by `enabled`.

**Consequence/materiality:** an explicit mandatory gate remains unmet. This is
not an allegation of runtime failure or grounds for broader style cleanup.

**Smallest safe correction:** add accurate error behavior to the acknowledgement
helper, field ownership/absence semantics to those two fields, and replace the
removed activation links with the current contract. Keep it comment-only and
batch with functional remediation. No new helper, behavior or test is justified.

**Focused closure:** inspect the corrected docs against these signatures and
complete ownership/error paths; check changed benchmark-owner links with an
existing narrow documentation check if available. Formatting/diff checks suffice
for comment-only changes; do not require a broad test suite for them.

### FIND-TASK-001-5 — Fixture repair lacks required independent diagnosis

**Status/classification:** CONFIRMED / VIOLATION. Source: S-2.

**Obligation:** implementation skill, “Diagnose unexpected failures,” requires a
fresh read-only diagnostician before a test/assertion or ordering fix, and its
report beside the diagnosis. Task-review explicitly treats a test change to clear
a failure without both diagnosis and diagnostician report as a violation.

**Location/evidence:** task evidence at line 368 reports that
`diagnostics_use_benchmark_distributions` failed a component-order assertion,
attributes it to equal 100 µs fixtures, and records changing the outbox fixture
to 50 µs. Current `verification/run.rs:1248` contains that fixture;
`component_totals` builds a BTreeMap, `components` at 1186–1207 sorts by descending
bucketed total, and assertions at 1291–1292 require engine then outbox. Full
producer/test and aggregation bodies support the stated tie explanation. No
independent diagnostician report was supplied or found in this packet. The
orchestrator explicitly reconfirmed this absence for validation; its earlier
Gateway diagnosis concerned a different assertion and does not discharge this
obligation.

**Consequence/materiality:** the mandatory independent test-repair process record
is missing. Source inspection does not establish a weakened assertion or a
production bug, and green checks cannot retrospectively prove the required
independence/order occurred.

**Smallest safe correction:** obtain and preserve one focused fresh read-only
diagnosis of the original fixture, exact failing selector and available
trace/diff. It must independently explain the cause, fix site and affected
callers and assess whether unequal fixture values test the intended ranking.
Disclose that this validates an already-applied repair; do not invent historical
trace or claim the report preceded the fix. Keep the existing fixture/assertions
if justified. No product change or broad diagnosis is needed.

**Focused closure:** the separately recorded diagnosis source-traces the equal
total ordering and corrected ranking, preserves the original failure limitation,
and supports the existing exact
`verification::run::tests::diagnostics_use_benchmark_distributions` selector.
Rerun that in-process test only as needed to close concrete uncertainty. This
validator report is not a substitute for that separately required diagnostician.

## Reconciled acceptance

| Obligation | Reconciled implementation/proof evidence | Result |
|---|---|---|
| Sole backend, exclusive deployed startup, opt-in/loopback and ordinary production behavior | Backend enum/runtime install before boot; exclusive/production startup and router/config tests inspected; standards/system/concurrency reports agree on their separately traced boundaries | PASS by source/test inspection |
| One semantic timing observation and correct request-local residuals | Gate/Scribe/Gateway/Verification branches and corresponding producer/interval tests; no change to response or accepted-work policy | PASS by source/test inspection |
| All real diagnostic units/populations | Actual fractional publication and decimal-cost producers reach integer-only representation | FAIL — finding 1 |
| Interval conservation, cohorts, replacement/reset and measured invalidity | Snapshot/interval methods, Population validation and retained interval/history tests | PASS for implemented measured path; capacity proof remains finding 2 |
| Maximum complete declared workload fits bounds | Existing overflow/small consumer/two-call journey evidence does not prove this explicit requirement | FAIL — finding 2 |
| Six workflows use snapshots throughout correctness bookkeeping | Source conversion/callers and consumer matrix establish transport closure and histogram-count meanings; raw refusal metadata is lost before setup/drain | FAIL — finding 3; diagnostic semantics also finding 1 |
| Capture/ACK, security/tenancy, cancellation/settlement and production durable owners unchanged | Instrumentation-only producer changes; existing owners retained; new functional capture journey and retained production regressions inspected | PASS for regression boundary by static evidence; no independent execution claim |
| Immutable readable history and declared instrumentation settings | Historical files unchanged, new metric/method fields defaulted, retained record/report tests and README declaration | PASS by source/test inspection |
| Candidate disposition and verification evidence | Candidate inventory and commands separate interrupted/failed setup from claimed final passes; required fixture-diagnosis record is absent | FAIL — finding 5; no fabrication of command results inferred |
| Mandatory changed-item documentation | Specific error/ownership/activation docs incomplete | FAIL — finding 4 |
| Non-goals and owner reuse | No optimization, scheduler, admission, durable service or parallel collection workflow introduced; tooling-recorder comparison justifies narrow shared owner | PASS |

REQ-005/006/008 and AC-003/004 belong to TASK-002 and are excluded. No report's
limited PASS substitutes for the failures above. No correction requires changing
approved behavior, public product contracts, security, persistence, concurrency
semantics or resource ownership.

Skipped: runtime checks, broad suites and performance qualification under the
review's static boundary. Risk: valid fractional observations currently fail,
known incomplete raw evidence can read as healthy, and full declared capacity
plus two mandatory documentation/process gates remain unclosed.
