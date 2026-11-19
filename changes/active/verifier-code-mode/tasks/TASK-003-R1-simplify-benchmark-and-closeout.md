---
id: TASK-003-R1
kind: remediation
status: proposed
spec: SPEC-verifier-code-mode
spec_revision: 7
requirements: [REQ-019, REQ-020, REQ-021, REQ-022, REQ-023, REQ-024, REQ-025, REQ-026, INV-007, INV-008, INV-009, AC-019, AC-020, AC-021, AC-022, AC-023, AC-024, AC-025, AC-026, AC-027]
depends_on: [TASK-001, TASK-001-R1, TASK-002B]
parent_task: TASK-003
remediates: [TASK-003]
---

# Finish the common SDK benchmark with usable breakdowns and production isolation

## Outcome and Value

Run the same verification benchmark through Rust, Python and TypeScript, and
explain where elapsed time, CPU and allocations come from on both client and
server. Remove duplicated profiling machinery without removing that diagnostic
capability. Compile all benchmark machinery out of production server and SDK
builds. Fix the production envelope overhead in this cleanup, not later.

The common workload and three language callers are already implemented. Keep
them. TASK-003 remains open until this task's cleanup, actual breakdowns, full
benchmark evidence and independent task review pass. This plan is not a PASS.

## What Changes and What Must Stay True

### One benchmark, three public SDKs

Keep the canonical Verifier's one Assert and two independently eligible Agent
checks, four governed mock exchanges and two real code operations per completed
verification. Preserve real permissions, workspaces, sandbox/pool handling,
original citations/data, deterministic passing/failing arrivals, exact counts,
compact decisions and explicit details reads. Keep public Rust/Python/TS callers
at c1/c64, one shared fixture/runner/recorder/reporting method, existing limits,
L2 and prior runtime/integration correctness. No raw HTTP substitute.

Client means the entire public-call round trip, including provider/sandbox
waits and language conversion. Preserve call-return and result-ready timing.
Server means server-induced latency. Keep server elapsed/phases and its separate
historical overhead criterion. No client subtraction/500 µs target, phase sums,
independent percentile subtraction or pool resizing to improve results.

### Required diagnosis, not optional gaps

The following are observable boundaries, not prescribed metric keys/modules.
Reuse existing phase owners and add only the observations genuinely missing.

| Side | Required elapsed-time breakdown | CPU and memory evidence |
| --- | --- | --- |
| Rust/Python/TS client | Language input/native-boundary work; shared-client preparation/serialization and authentication; HTTP attempt/body receipt/decoding; language return and compact-result materialization. Include retries/waits in totals and identify them where reached. | Real SDK stacks and allocation sites, including shared native Rust and Python/V8 domains, allocation volume and retained memory; Python managed memory needs retained memory and peak only (see below). Attribute findings to phases and the measured window/process. |
| Rust server for every language run | Admission/authorization and input preparation; orchestration/workspace setup; Gateway/provider work; sandbox admission/execution/IPC/host waits; result assembly/response. | Actual server CPU and native allocation/retention stacks tied to reached phases/window; preserve existing standard server profiles. |

Nested phases are inclusive, concurrent work overlaps, and shared runtime/GC
costs may not belong exclusively to one request. Label these limits; do not
invent exclusive CPU/allocation numbers or call a sandbox envelope pure Python
CPU. Reached phases must have readouts and correct populations; inapplicable
means not reached in that workload, not instrumentation unavailable. Missing
required Python/TS phases or native/managed allocation coverage blocks diagnostic
closeout. Totals, peak RSS, opaque calls into native code, synthetic probes and
server-only captures are not substitutes. A separately valid total-latency
record remains valid while diagnosis is unfinished.

Python managed allocation volume is not required. The Python layer is thin,
and its conversion cost is already measured by the `sdk.verify` output
component and the per-call materialization interval, with CPU frames from
the standard profiler. The standard managed-heap tool (`tracemalloc`)
records retained memory and peak, not freed volume; that is sufficient,
and the volume limit is recorded rather than closed with a new tool or
dependency. Native volume and retention, and V8 volume and retention,
remain required.

Rust's controller also hosts the mock provider. Keep aggregate scope labels,
but distinguish SDK costs from fixture/controller costs in diagnostic stacks
or existing role observations. A mixed process total alone is insufficient.
Separate execution only if existing attribution genuinely cannot distinguish
those costs; no new process architecture is prescribed here.

### Compile-time isolation and reuse

Production server, Rust client, Python wheel and TypeScript packages must
exclude benchmark measurement code/state, registries, collector lifecycle,
allocator hooks and controls behind bench. An uninstalled recorder or an empty
runtime record is not sufficient. Preserve ordinary operational metrics/OTEL.
Shared call sites may have a stateless non-bench facade with no measurement
logic; production must not retain dormant envelope/collector state or timers.

This revision supersedes the earlier blanket prohibition on SDK bench feature
wiring. Minimum forwarding of the existing bench capability and necessary
harness wiring in diagnostic-only release builds are authorized. Reuse the
single wyrd-benchmark owner across languages; no second recorder, new public
SDK benchmark API, separately maintained SDK product, new shipped dependency,
production activation/readout interface or general profiling framework.
Record exact native artifact/feature identity. Bench artifacts stay local and
must not replace published wheel/npm artifacts. The public SDK call path and
contract remain the same; instrumentation overhead stays measured and disclosed.
Unprofiled means CPU/heap profilers off; shared timing recording remains enabled
in the bench build. Do not claim an instrumented artifact is a production build.

Use established runtime/process tools for CPU and managed-heap diagnosis and
reuse the existing native heap owner where appropriate. A small launcher or
harness-only bridge is acceptable where the existing lifecycle needs it;
remove duplicated collectors, synthetic busy-loop/held-allocation probes,
probe parsers/validators and redundant capture protocol/hooks. Do not delete a
working useful capture solely because its commit was labelled S4, then build a
replacement system. Keep only the minimum collection needed for the required
breakdown; benchmark Python/JS scripts stay outside production packages and
must not be imported by ordinary SDK calls.

The implementor reported benchmark machinery in production wheels. That claim
must be checked against actual built packages. Inspection of the cited S4
commits finds collector additions under tests/harness paths, not new production
SDK exports; current normal/build feature graphs show no bench feature for
Python/TS. However, shared benchmark recorder/timing definitions and server
envelope state are not all compile-time gated. This is insufficient production
isolation regardless of which commit introduced it. Record the actual package
and feature evidence; do not assert a specific wheel leak without proving it.

Preserve unrelated dirty work, review folders and spec edits. Recheck and remove
only confirmed profiling/debug hunks from the pending Python diffs; no whole-file
restore, reset, blanket revert, history rewrite, commit or push is authorized.

## Approach

1. Recheck the integrated candidate and package/feature paths. Preserve the
   implemented workload/callers and isolate duplicate machinery by consumers.
2. Compile benchmark state/collectors out of non-bench builds and remove
   throwaway envelope allocation/propagation. Preserve enabled observations.
3. Reuse the shared recorder for complete client/server phase readouts across
   SDKs, with minimal bench-only feature/lifecycle wiring and language timings.
4. Simplify profiling to standard tools/shared native owners on real SDK work;
   remove artificial coverage systems and redundant protocol/runner paths.
5. Verify both artifact modes, SDK boundaries and persisted breakdown reports.
6. Record a full unprofiled six-population L1 plus short real-workload diagnostic
   captures; submit the integrated TASK-003/R1 evidence for independent review.

## Ordered Implementation Scenarios

### Scenario 1 — Production builds contain no benchmark machinery

**Behavior.** A build without bench has no benchmark recorder/registry,
measurement state, envelope allocation, clock/lock path, allocator hook or
activation/readout surface. It retains normal SDK operations and operational
telemetry. The bench counterpart still records the useful workspace, tool,
sandbox and model intervals without changing results, cancellation semantics,
concurrent overlap or full elapsed totals. Cover execution setup and Gateway /
hosted-tool constructors, including default records later replaced.

**RED.** Add the focused disabled-envelope proof in V1 and record its current
failure from unconditional record creation. Strengthen the existing production
package/build checks in V5 for compile-time isolation: demonstrate a bench-
contaminated build is rejected, then restore the intended non-bench build.
The runtime-disabled test alone cannot prove compile-time exclusion. Use the
smallest compiler/configuration/package proof; no allocator test framework or
production introspection API. State which state/implementation is excluded,
not merely that its entry point was never called.

**GREEN.** Gate the actual machinery and consumers at their existing owners;
keep non-bench compilation free of measurement state/logic. Reuse bench-only
native allocation hooks rather than creating duplicate samplers. Run V1 and
V5's normal-build isolation proof, plus retained enabled-overlap regression.

**REFACTOR.** Remove throwaway setup, dead wrappers and duplicate ownership.
No generic collector lifecycle, new production control or blanket removal of
normal telemetry. No-op source compatibility must remain stateless.

### Scenario 2 — Every SDK run explains client and server elapsed time

**Behavior.** The real public Rust/Python/TS verify flow returns full call and
result-ready samples and the required reached client/server phase readouts.
Both Agents' work is represented. Language conversion remains measured outside
the shared Rust core; native phases are nested observations, not extra requests.
Retries/errors/late responses preserve population accounting. A dropped required
phase or missing language process readout cannot render as zero or complete.

**RED.** Add the planned phase-completeness/report test in V2 and extend the
existing public Python/TS boundary journeys there. Show current Python/TS HTTP
phase evidence is unavailable and rejected as incomplete. Exercise real public
SDK/native transport for positive proof and a removed phase/readout for the
negative case; no canned distributions standing in for native integration.

**GREEN.** Connect the existing shared recorder/observations to each language
bench process and report, adding only bounded missing language-boundary work.
Retain existing transport, authorization and result ownership. Run V2's focused
proof and V1's regressions; keep production artifacts clean through V5.

**REFACTOR.** Remove language-specific duplicate collectors, unrecorded-client
fallbacks that falsely satisfy acceptance and redundant report paths. Keep one
shared collector and renderer. Preserve historical records: an older record's
missing phase remains a historical gap, not a newly fabricated measurement.

### Scenario 3 — Profiles explain actual client and server CPU/allocation work

**Behavior.** A separate diagnostic run of the common verification workload
produces inspectable CPU and native/managed allocation/retention evidence on
both sides for each SDK. Findings identify real verification work and relate
it to reached phases and process/window, while fixture/runtime/GC sharing stays
explicit. Missing required capture domains make diagnostic coverage incomplete;
synthetic work and RSS cannot make it complete. Standard tools may supply
artifacts without a custom collector or automated sampled-frame validator.

**RED.** Extend the same planned completeness test in V2 to reject complete
coverage claims with missing client native or Python/V8 allocation domains
(Python managed retention only, as above).
Keep this a deterministic report/coverage assertion, not a profiler validation
framework. Record the current real SDK capture gap in a short V4 run before
changing its wiring; do not manufacture probe functions or statistical frame
thresholds to make a test red.

**GREEN.** Reuse standard runtime/process tools and the existing native profile
owner, retaining only the minimum bench-only launcher/lifecycle/collection
needed to inspect real SDK and server costs. Run the V2 completeness proof and
inspect V4's actual artifacts. Explicit metadata cannot substitute for actual
coverage; the implementation evidence must explain what those captures show.
Recheck V5 production exclusion for any changed native wiring.

**REFACTOR.** Remove duplicate runtime collectors, parsers, synthetic allocations
and redundant capture controls. Keep one report path and existing native heap
owner; no public SDK API or second recorder/profiling subsystem. An external
tool's necessary own collection is not a reason to build a parallel Wyrd one.

## Acceptance Criteria

| Obligations | Proof and failure meaning |
| --- | --- |
| REQ-019/022, AC-019/023 | V2/V3 run the common public SDK flow at both counts with all three checks, exact exchanges/code operations and correct original evidence/details. Rust-only or reduced work is incomplete. |
| REQ-020/023, INV-007, AC-020/024 | V1/V2 retain enabled server overlap/totals and provide complete client/server phase populations through real native SDK calls. Missing SDK phases or subtracted time fails acceptance. |
| REQ-021, AC-021 | V3 provides full unprofiled release bench results for all six populations, common integrity rules and >=1,000 p99 samples per repetition; preserve the separate server criterion and report actual feature mode. |
| REQ-024, INV-008, AC-025 | V4 inspects actual client/server CPU and native plus Python/V8 allocation/retention evidence (Python managed: retention and peak only), maps findings to phase/window and distinguishes fixture costs. Missing required domains blocks closeout; RSS/probes cannot substitute. |
| REQ-026, INV-009, AC-027 | V1/V5 establish compile-time non-bench isolation and actual production wheel/npm boundaries; bench counterparts still produce diagnostics. Runtime-disabled or feature-unified test builds alone fail proof. |
| REQ-025, AC-022/026 | V2 retains L2, compact/details/code-mode correctness and persisted/legacy report meanings. TASK-002B's composition proof remains; no duplicate full integration campaign. |

## Implementation Reference

Inspected candidate: 12a545542 plus pending planning/Python profiling changes.
Revalidate if HEAD advances. Paths describe owners, not a file allowlist.

| Inspected owner/callers/tests | Reuse and precise gap |
| --- | --- |
| verification/workload.rs CANONICAL/Conversations; run.rs canonical_expected/Measured; mod.rs L1/L2 declaration tests | Workload and counts already exist. Preserve, do not rebuild. |
| Python Driver/timed/materialize and TS run/timed/materialize; both building-block boundary tests | Public callers and return/readiness boundaries exist. Keep them; add missing language phase evidence and remove artificial probes/duplicate Profiles machinery. |
| shared wyrd-client observe/verify and transport/http.rs Observation; wyrd-benchmark Observation/install/snapshot/BenchmarkRecorder | The existing shared timing/backend owner. Current Python/TS production artifacts do not enable/read it; minimum bench-only propagation/lifecycle/reporting is needed, not another recorder. |
| wyrd-benchmark lib.rs recorder module/REGISTRY/Active and recorder.rs; server telemetry.rs Envelopes/ExecutionTelemetry and hosted tools/Gateway constructors | install/heap have some feature guards, but other measurement definitions/state remain outside them; Envelopes allocates unconditionally. Gate the machinery and consumer setup, preserving ordinary telemetry. |
| profile.rs Profiler/HeapProfiler; capacity main.rs and server main.rs allocators; experiment.rs profile/report lifecycle | Existing CPU/native allocation lifecycle, sample format and role renderer. Reuse; standard runtime tools must also cover Python/V8, which native Rust allocation hooks do not. |
| run.rs set_client_process/unrecorded_client; driver.rs ForeignDriver run/capture; Bench prepare/capture_clients | Phase readouts are missing; generic capture/probe plumbing also includes idle drivers. Replace only missing/inadequate wiring and remove redundancy; do not preserve missing-phase acceptance or create a second orchestration system. |
| mise.toml bench:build/bench:build:sdks, check:py-wheel-no-testing, ts:pack:check; check_ts_package.mjs | Existing build/package owners. Add minimal explicit bench mode for local diagnostics and isolated production boundary assertions; never let packaging reuse a local bench addon/wheel. |

Keep benchmark commits 85ec2bdba, 45ac7d325, 1c1c58f53, ad4c67564 and
8759adf9a except required fixes. config.rs in 45ac7d325 exposes the existing
limits loader; it does not rename a production Role. Inspect S4 commits
3cb0c9647/b357f536e/12a545542 by responsibility; preserve useful filename role
reporting and any minimal real capture already satisfying this task. Historical
evidence from 16a6b9e4a stays, explicitly superseded where acceptance changed.
The temporary flate2 addition was already removed; do not repeat that churn.

## Verification and Evidence

Cargo-backed operations run sequentially. Existing selectors below were checked
from source. New names are planned: confirm actual module/selector with nextest
or the owning runner after adding them, before RED. No full journey suite/gate.

### V1 Server disabled/enabled observations

```bash
# Planned; bench compiled but no recorder installed: catches unconditional state.
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::telemetry::tests::disabled_envelopes_do_not_create_records)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib --features bench -E 'test(=verification::telemetry::tests::canonical_phases_explain_total_latency)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::telemetry::tests::overhead_is_the_execution_less_its_blocks)'
```

Server tests self-enable bench through dev-dependencies. The first test needs
an uninstalled recorder in its own nextest process and does not prove non-bench
compilation. V5 provides the separate production artifact proof.

### V2 Phase, workflow, SDK and persisted-report regressions

```bash
# The final selector is planned; other selectors exist.
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=verification::tests::level_one_declares_direct_sweep_and_strict_objective) | test(=verification::tests::level_two_declares_phases_and_records_rate_overrides) | test(=verification::run::tests::canonical_workload_requires_all_three_checks) | test(=verification::run::tests::canonical_client_latency_includes_all_work) | test(=verification::run::tests::language_drivers_preserve_public_boundaries_and_counts) | test(=verification::run::tests::language_phase_breakdown_requires_complete_evidence)'
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=experiment::tests::reports_keep_repetition_diagnostics_after_artifacts_are_removed) | test(=experiment::tests::reports_regenerate_from_persisted_records)'
mise run py:setup
mise run ts:build
mise run ts:build:testing
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run --no-sync python -m pytest -q tests/integration/test_verifier_building_blocks.py::test_benchmark_driver_preserves_public_call_boundary'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/verifier-building-blocks.test.ts -t "^benchmark driver preserves public call boundary$"'
```

Extend the existing two public-boundary journeys for real phase evidence under
an explicitly identified bench-enabled native artifact. Use the existing build
owners once forwarding is implemented; record the exact bench build commands
and artifact paths in evidence, not an accidental testing feature union. Test
core/error/conversion boundaries without new public benchmark API. Retain report
regressions when role/coverage rendering changes; record any added exact selector.

### V3 Full common benchmark

```bash
mise run bench:l1:verification
mise run bench:results
mise exec -- target/profiling/release/capacity report "PATH_TO_RECORD.json"
```

All six populations use normal full windows/repetitions, identical workload /
limits/method and explicitly identified release bench artifacts. Record request
counts/errors, throughput, client call-return/result-ready and server p50/p95/p99,
phase populations, CPU/RSS scopes and the existing criterion's actual judgment.
Unprofiled means no CPU/heap profiler, not missing phase observations. Preserve
VALID/INVALID versus met/missed/unsettled semantics. Re-render the new record and
an actual legacy record using their real paths, independently of raw profiles.
Historical profiled smoke results cannot substitute. No extra full L2 run.

A clean full qualification record requires a clean final candidate; no Git action
is authorized here. A full --allow-dirty run is diagnostic and cannot be relabelled
as clean qualification. If source or host state blocks that run, retain evidence
and mark it pending; do not close TASK-003, spin through repeated waits, generate
synthetic load or weaken sample/host gates. A real performance miss stays a miss;
this task authorizes no runtime tuning.

### V4 Real client/server diagnostic captures

```bash
mise run bench:l1:verification -- --smoke --profile --allow-dirty
```

Use the existing orchestration with minimal standard-tool/native-owner wiring;
a short diagnostic run is sufficient if its actual workload has useful samples.
Record exact working tool invocations/versions, feature/native identities,
process/window scopes and artifact paths. Inspect real verify/conversion /
transport CPU frames and native plus Python/V8 allocation sites, allocation
volume and retained memory (Python managed: retained memory and peak; its
volume is a recorded limit, not a missing domain). Explain important reached client/server phases and
waits using the measured observations; function samples alone do not measure
blocking. Distinguish fixture stacks from SDK stacks, and shared runtime/GC
costs from exclusively attributable work. Do not run synthetic probes or add a
coverage-validation framework. If a longer capture is necessary for usable
samples, run only that targeted diagnostic, not another qualification campaign.
Every mandatory domain must be demonstrated; a labelled missing domain prevents
diagnostic closeout. This run does not qualify latency performance.

### V5 Production build/package exclusion and changed-owner checks

```bash
mise exec -- cargo check --locked -p wyrd-server --lib
mise exec -- cargo check --locked -p wyrd-ai --lib
mise exec -- cargo check --locked -p wyrd-sdk-python --features python --lib
mise exec -- cargo check --locked -p wyrd-sdk-ts --lib
mise exec -- cargo tree --locked -p wyrd-server -e normal,build,features -i wyrd-benchmark
mise exec -- cargo tree --locked -p wyrd-ai -e normal,build,features -i wyrd-benchmark
mise exec -- cargo tree --locked -p wyrd-sdk-python --features python -e normal,build,features -i wyrd-benchmark
mise exec -- cargo tree --locked -p wyrd-sdk-ts -e normal,build,features -i wyrd-benchmark
mise run check:py-wheel-no-testing
mise run ts:pack:check
mise run fmt:check
mise run lints
mise run py:format:check
mise run py:lints
mise run py:typecheck
mise run ts:format:check
mise run ts:lints
mise run ts:typecheck
mise exec -- git diff --check
```

Run normal artifact builds separately from bench/test feature unions and inspect
selected implementations to establish that measurement state/logic is excluded;
a feature graph alone is insufficient. If the benchmark dependency becomes
absent from a normal graph, record the equivalent full graph listing as absence
proof rather than reintroducing it to satisfy the inverse query. Extend the
existing wheel/package checks
to reject bench contamination, inspect/import actual production archives in
isolated environments and retain generated API parity. Neither existing package
check currently establishes benchmark exclusion by itself. Do not create a new
standalone gate when the package/build owner can prove it. Rebuild normal TS
artifacts before packing if a preceding bench run replaced a local addon; never
publish or install a diagnostic build as the normal package. Check the production
server binary's allocator/benchmark control gates as well as its library.

Run V1/V2/V5 before V3/V4; keep separate build directories or record intentional
artifact replacement so a testing/normal addon cannot silently stand in for a
bench one or vice versa. Run napi/stub parity checks through their existing
owners if actual binding declarations change; no new public declaration is
intended. Correct README/task help for both modes and actual coverage. The
full SDK/MCP journey campaign remains final integrated change-review work.

## Material Stop Conditions

- A required phase, native/managed allocation domain or SDK/fixture boundary
  cannot be demonstrated: identify the precise gap before closeout, rather
  than weakening acceptance or treating totals/RSS as diagnosis.
- Reuse would require a new public SDK API, second recorder/profiler framework,
  new shipped dependency or production benchmark control: return to authority.
  Minimal existing-bench forwarding/harness wiring is already authorized.
- Production packaging still includes benchmark state/controls or its feature
  isolation cannot be proven: fix it now; no dormant-code exception.
- Clean qualification is blocked by source/host state: leave it pending, with
  diagnostic evidence retained. No commit/reset/host-gate bypass is authorized.
- Actual limits/criteria miss or prerequisite correctness fails: retain the
  evidence and route runtime correction to its owner; no pool/workload tuning.

## Authority Links

- [Approved spec revision 7](../spec.md), REQ-019–026, INV-007–009, AC-019–027
- [Parent TASK-003 and historical evidence](TASK-003-canonical-sync-verification-benchmark.md)
- [Repository rules](../../../../AGENTS.md)
- [Implementation execution](../../../implementation-execution.md)
- [Testing](../../../../TESTING.md)
- [Benchmark method](../../../../benchmarks/README.md)

## Obligations

Recorded after Scenarios 1–3 were implemented (process exception: the
ledger postdates the code; the contract is unchanged).

| ID | Promise and required cases | Contract source | Planned proof |
| --- | --- | --- | --- |
| AC-019/023 | Common public Rust/Python/TS flow at c1/c64: one Assert and both Agent checks, four exchanges, two code operations, correct evidence/details | REQ-019/022; One benchmark | V2 regressions; V3 |
| AC-020/024 | Every language run yields reached client phases (language boundary, preparation/auth, HTTP attempt/body/decode, return/materialization) and server phases; retries, errors and late calls keep population accounting; a dropped phase or missing readout is invalid, never zero; bench envelopes keep results, cancellation, overlap and totals; older records' missing phases stay historical gaps; no client subtraction | REQ-020/023, INV-007; Required diagnosis; Scenarios 1–2 | V1; V2 incl. bench-branch journeys; V3 for per-language server phases |
| AC-021 | Full unprofiled six-population release bench record, integrity rules, >=1,000 p99 samples per repetition, separate server criterion, actual feature mode and disclosed instrumentation | REQ-021; V3 | V3 |
| AC-025 | Real client/server CPU and native plus Python/V8 allocation/retention captures mapped to phases and window; SDK costs distinguished from fixture/controller costs; a missing domain marks coverage incomplete; Python managed: retention and peak only | REQ-024, INV-008; Scenario 3 | Capture unit/journey tests; V4 |
| AC-027 | Non-bench server binary and library, Rust client, wheel and npm packages carry no benchmark state, registry, allocator hooks, timers or controls (stateless facade only); ordinary metrics/OTEL kept; package checks reject bench artifacts and import production archives in isolation; bench artifacts never replace published ones; generated API parity kept | REQ-026, INV-009; Compile-time isolation; Scenario 1; V5 | V1; V5 |
| AC-022/026 | L2, compact/details/code-mode correctness, persisted and legacy report meanings | REQ-025, AC-022/026 row | V2; V3 legacy re-render |
| R1-DUP | One recorder, one renderer, one native heap owner; duplicate collectors, synthetic probes, probe parsers and redundant capture paths removed; no public SDK benchmark API, second recorder or new shipped dependency; benchmark scripts outside production packages and unimported by SDK calls | Compile-time isolation and reuse; Scenario 2–3 REFACTOR | Source inspection |
| R1-DOC | README and task help describe both build modes and actual coverage; exact bench build commands and artifact identity recorded | V2, V4, V5 text | Content review; evidence |

## Implementation Evidence

Commits: `e004b5907` (Scenario 1), `39ef5f69e` and `bc166b950`
(Scenarios 2–3), `51ee896c1` (V5 package rejection, bench journeys),
`6863e6a7e` (napi type-definition cache fix, diagnosis below).

RED: Scenario 1 `disabled_envelopes_do_not_create_records` failed to
compile against the unconditional `Arc<Mutex<..>>` record (`no method named
is_none`). Scenario 2 `language_phase_breakdown_requires_complete_evidence`
failed to compile (`unresolved imports super::client_roots,
super::set_language_client`): no language breakdown existed. Scenario 3's
gap capture is the earlier V4 record
`target/benchmarks/smoke/verification/direct-verify/20261112T181616Z-16a6b9e4a.json`
(native heap uncovered in Python/Node); no new short V4 run was made before
rewiring (process exception; benchmark runs await authorization).

Bench builds used for the bench-branch journeys (local only, then replaced
by `py:setup`, `ts:build`, `ts:build:testing`):
`cd sdks/wyrd-sdk-python && uv run maturin develop --features testing,bench`
(module `wyrd._wyrd._benchmark` importable); `mise run ts:build:testing`;
the TS line of `bench:build:sdks` (release, `--features bench`,
`wyrd.linux-x64-gnu.node` exporting `benchmarkInstall`, `benchmarkSnapshot`,
`benchmarkHeapStart`, `benchmarkHeapProfile`; declarations to
`target/benchmark-sdk/index.d.ts`).

| Obligation and required cases | Implementation evidence | Proof location | Verification evidence | Result |
|---|---|---|---|---|
| AC-019/023 | Workload and callers kept (`verification/workload.rs`, `run.rs::canonical_expected`) | `run.rs` tests; building-block journeys | V2 `canonical_workload_requires_all_three_checks`; Python file 39 passed, TS file 34 passed. V3 not yet authorized | FAIL (V3 pending) |
| AC-020/024: client phases | Python `observe/mod.rs` and TS `native/src/cards.rs` `sdk.verify` boundary; `run.rs::{client_roots, set_language_client}` | `verifier-building-blocks.test.ts:509-533`; `test_verifier_building_blocks.py:535-560`; `run.rs:3392` | Bench-branch journeys (2 Python, 2 TS passed): one `sdk.verify` and `client.verify` success, two `client.http`/`client.http.attempt`, the refused call's own `error` outcome, components `dispatch,input,core,output` and `serialize,auth,send,body,decode`. Unit: complete window reads every root; a dropped `core` component and fewer roots than calls are invalid; no readout leaves readings missing | PASS |
| AC-020/024: populations | `run.rs::Measured` reported calls | `run.rs` `language_drivers_preserve_public_boundaries_and_counts` | V2: refusals counted by code and never sampled, late calls sampled and counted late, unacknowledged reports refused | PASS |
| AC-020/024: server overlap, totals, cancellation | `telemetry.rs::Envelopes` (record only when enabled and direct) | `telemetry.rs` tests | V1 `canonical_phases_explain_total_latency` (`--features bench`), `overhead_is_the_execution_less_its_blocks` | PASS |
| AC-020/024: per-language server phases, both Agents in a live run, historical gaps | — | — | V3 run and legacy re-render not yet authorized | FAIL (V3 pending) |
| AC-021 | — | — | V3 not yet authorized | FAIL (pending) |
| AC-025: capture domains and gaps | `run.rs::capture_clients` notes a missing native domain; any note clears `ProfileNote::complete` (`profile.rs:127`); drivers always write `.tracemalloc` or `.heapprofile`+`.heapsnapshot`, plus `.pb.gz` from a bench module, and a failed capture is a note | `run.rs:2336`; `verifier-building-blocks.test.ts:557`; Python `test_benchmark_heap_capture_holds_real_conversion_work` | `client_captures_name_their_gaps`; `profile::tests::*` 3/3; bench heap journeys: native `.pb.gz` present, V8 sampled allocations, snapshot holds the converted result | PASS |
| AC-025: real profiled run, phase/window mapping, SDK vs fixture | — | — | V4 not yet authorized | FAIL (V4 pending) |
| AC-027: compile-time exclusion | `wyrd-benchmark` `bench` gates `heap`, `recorder`, `registry`, `ENABLED`, `Observation` state; every non-facade dependency optional; server `main.rs` allocator heap hooks `cfg(feature = "bench")`; benchmark routes bench-only (`app/metrics.rs`) | `wyrd-benchmark/src/lib.rs:44-67,398`; `wyrd-benchmark/Cargo.toml`; `wyrd-server/src/main.rs:61-103` | V5 `cargo check` server/wyrd-ai/python/ts 4/4; normal trees reach only `wyrd-benchmark feature "default"` (empty); `check:deps` (`deps.sh:122` rejects `bench` in production graphs) | PASS |
| AC-027: package boundaries | Wheel check imports the default wheel in an isolated env and refuses `wyrd._wyrd._benchmark`; `check_ts_package.mjs` packs, imports the compiled facade and refuses the registry marker | `mise.toml` `check:py-wheel-no-testing`; `scripts/check_ts_package.mjs:114` | `check:py-wheel-no-testing`, `ts:pack:check` pass on normal builds; bench addon rejected ("holds the benchmark registry"); bench wheel exposes `wyrd._wyrd._benchmark`, which the wheel check fails on | PASS |
| AC-027: ordinary telemetry, API parity | Operational metrics unchanged | `telemetry.rs` `abandoned_direct_execution_finishes_cancelled` | Server telemetry tests pass in V1's build; `ts:napi:check` passes after a bench build (fix `6863e6a7e`); no `.pyi` change | PASS |
| AC-022/026 | Owners unchanged | `verification/mod.rs`, `experiment.rs` tests; building-block journeys | V2 `level_two_declares_phases_and_records_rate_overrides`, `reports_regenerate_from_persisted_records`, `reports_keep_repetition_diagnostics_after_artifacts_are_removed`; compact/details/code-mode journeys in both files pass. Legacy record re-render is V3 | FAIL (legacy re-render pending) |
| R1-DUP | `MANAGED_PROBES`, probe frames, `unrecorded_client`, driver `Profiles` removed in `bc166b950`; heap sampler moved into `wyrd-benchmark::heap` and reused by capacity and both SDK cdylibs; `wyrd-benchmark` is a new SDK crate dependency but without `bench` adds only `http`, `tracing`, `tracing-subscriber`, which the SDKs already build | `rg` for `MANAGED_PROBES`, `wyrd_profile_probe`, `wyrdProfileProbe`, `unrecorded_client`, `class Profiles`, `managed_captures_must_name` over `crates sdks scripts`: no matches; `rg _benchmark sdks/wyrd-sdk-python/python`: no matches; drivers live under `tests/` | Source inspection | PASS |
| R1-DOC | `benchmarks/README.md` lines 211, 416-441 (both modes, capture table, `tracemalloc` limit); `bench:build:sdks` help | `mise.toml` `bench:build:sdks` | Content review | PASS |

V5 also passed: `fmt:check`, `lints`, `py:format:check`, `py:lints`,
`py:typecheck`, `ts:format:check`, `ts:lints`, `ts:typecheck`,
`git diff --check`. No pool, limit or workload change was made.

**Diagnosis — bench exports in the committed `index.d.ts`.**
- **Symptom:** after a bench build, `mise run ts:build` left
  `benchmarkInstall`, `benchmarkSnapshot`, `benchmarkHeapStart` and
  `benchmarkHeapProfile` in `sdks/wyrd-sdk-ts/wyrd/index.d.{ts,cts}`,
  although the built addon exported none of them.
- **Evidence:** `target/napi-rs/wyrd-sdk-ts-cd3c5e39/wyrd-sdk-ts` was
  written at the bench build's time and held the bench entries; napi
  CLI 3.4.1 `generateIntermediateTypeDefFolder` reuses that folder when
  `--dts-cache` (default true) and forces a rebuild only when the file is
  absent.
- **Cause:** the folder is keyed by crate and manifest, not profile or
  features, so a cached dev build emitted the release bench build's
  definitions.
- **Fix site:** `bench:build:sdks` passes `--no-dts-cache`, which deletes
  the shared folder and writes to a throwaway one; the next `ts:build`
  recompiles `wyrd-sdk-ts` and regenerates its own definitions. Verified:
  after `bench:build:sdks` then `ts:build`, no diff and no `benchmark`
  entry in `index.d.ts`. `ts:build:testing` uses a different crate folder.

Status: BLOCKED only on the user-deferred V3 and V4 runs (AC-019/023,
AC-020/024 live-run cases, AC-021, AC-025 live captures, AC-022/026 legacy
re-render). Local SDK modules are the normal builds.
