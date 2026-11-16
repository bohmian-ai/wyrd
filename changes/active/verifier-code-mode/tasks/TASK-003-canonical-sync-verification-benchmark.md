---
id: TASK-003
kind: implementation
status: proposed
spec: SPEC-verifier-code-mode
spec_revision: 7
requirements: [REQ-019, REQ-020, REQ-021, REQ-022, REQ-023, REQ-024, REQ-025, INV-007, INV-008, AC-019, AC-020, AC-021, AC-022, AC-023, AC-024, AC-025, AC-026, REQ-026, INV-009, AC-027]
depends_on: [TASK-001, TASK-001-R1, TASK-002B]
parent_task:
remediates: []
---

# Benchmark public verification across Rust, Python and TypeScript

## Outcome and Value

Run the same immediate public verification benchmark in Rust, Python and
TypeScript, each at 1 and 64 concurrent clients against the same release Rust
server and settings. Its Verifier has three independently eligible checks:
one Assert and two Agents, each with two governed mock-model exchanges and
one real code.run. Include all elapsed client time.

## What Changes and What Must Stay True

This task is partly implemented, not passed. The canonical workload and all
three callers already exist. Do not rebuild them. The former integrated
profiling Scenario 4 is withdrawn by spec revision 7, including mandatory
synthetic coverage probes. The remaining cleanup and closeout work is owned by
[TASK-003-R1](TASK-003-R1-simplify-benchmark-and-closeout.md), which is the
execution entry point. There is no separate profiling implementation task.

Keep the identical fixture, original citations/custom data, passing/failing
arrivals, authorization, code execution and exact exchange/code counts. Keep
public call-return/result-ready boundaries, release SDK artifact identities,
server elapsed/phases and separate historical server-overhead criterion.
Retire client subtraction/500 µs target; introduce no replacement client target.
L2, prior code-mode correctness, compact decisions and explicit details remain.

Remove duplicate client capture machinery and compile benchmark timing
machinery out of production in the same cleanup. Keep useful server diagnostics and require
client/server phase latency and CPU/native/managed allocation breakdowns for
every SDK. Reuse the existing shared recorder behind bench; minimal diagnostic
feature forwarding is permitted, but no duplicated collector or public SDK
benchmark API. Production artifacts must compile out the machinery. Missing
required SDK phases or allocation domains blocks diagnostic closeout.
Do not equate RSS with allocation attribution, or subtract separate percentiles to infer SDK overhead. Rust controller/provider CPU and memory
must remain labelled as mixed process costs.

## Approach

1. Preserve the existing common workload and public SDK callers.
2. Execute TASK-003-R1's cleanup, production fix and focused regression proof.
3. Record the full unprofiled six-population benchmark and useful diagnostics.
4. Submit the integrated result for task review; only a passing review closes
   TASK-003. A planning edit or historical smoke run does not close it.

## Proof Strategy

This parent adds no new executable behavior. Its already implemented workload,
SDK boundaries and reporting require regression/real-run evidence, not invented
RED results. TASK-003-R1 owns TDD for the disabled-instrumentation fix and all
remaining exact verification commands. Prior evidence below is retained as a
historical execution record, not current acceptance of withdrawn profiling.

## Acceptance Criteria

| Obligations | Proof owner and expected result |
| --- | --- |
| REQ-019/022, AC-019/023 | R1 V2/V3: same public SDK workload at c1/c64; all three checks, four model exchanges and two code operations reconcile with correct evidence. |
| REQ-020/023, INV-007, AC-020/024 | R1 V1/V2: complete call-return/result-ready timing, overlap-safe server observations and disabled envelope records absent. |
| REQ-021, AC-021 | R1 V3: full unprofiled release record, common integrity/sample rules and unchanged separate server criterion; no client subtraction/target. |
| REQ-024/026, INV-008/009, AC-025/027 | R1 V1/V4/V5: remove duplicated profiling, compile all benchmark machinery out of production, demonstrate phase/CPU/native-and-managed allocation breakdowns on client and server in bench builds, and inspect actual production packages. |
| REQ-025, AC-022/026 | R1 V2: preserve L2, compact/details ownership, code-mode records/citations and persisted/legacy report meanings; TASK-002B retains its independent composition proof. |

## Implementation Reference

Use the current owner/reuse map in TASK-003-R1. Implemented benchmark commits
85ec2bdba, 45ac7d325, 1c1c58f53, ad4c67564 and 8759adf9a are the preservation
baseline. The three profiling commits are removal provenance, not a blanket
revert instruction. Preserve unrelated work and the evidence below.

## Verification and Evidence

Run the exact commands and closeout requirements once from TASK-003-R1.
No new full SDK/MCP journey campaign belongs here; integrated change review
owns the broad suites. Full L1 benchmark evidence remains required separately
from regression tests and diagnostic captures.

## Material Stop Conditions

Use R1's material stop conditions. No production benchmark machinery,
duplicated collectors, public SDK benchmark API or runtime/pool retuning is an acceptable workaround for a
missing diagnostic or performance miss.

## Authority Links

- [Approved spec revision 7](../spec.md)
- [Cleanup and closeout](TASK-003-R1-simplify-benchmark-and-closeout.md)
- [Implementation execution](../../../implementation-execution.md)
- [Testing](../../../../TESTING.md)

## Implementation Evidence

Commits on `change/verifier-code-mode`: `85ec2bdba` (S1 workload and
fixture), `45ac7d325` (valid code Role, recorded verification limits),
`1c1c58f53` (S2 whole-call client totals and direct Agent envelopes),
`ad4c67564` (S3 Python/TypeScript drivers and six populations), `8759adf9a`
(Model artifact for Python hydration, release SDK build, SDK boundary
stories). All smokes are dirty diagnostic runs (`--smoke --allow-dirty`),
never qualification.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-019/AC-019 canonical workload | `verification/workload.rs` (`CANONICAL`, `Conversations`, `Ledger`, `bench_code` Role, Model declared alone with one artifact); `run.rs::canonical_expected`, `recorded` | V1 four selectors; V2 `canonical_workload_requires_all_three_checks`; smokes reconcile 0 unreconciled, 0 wrong at both counts | PASS |
| REQ-020/INV-007/AC-020 total client boundary and attribution | `run.rs::{Sample, materialize, Measured::integrity}`; server `verification/telemetry.rs::{Envelope, Envelopes}` around workspace, tool, sandbox, model | V2 `canonical_client_latency_includes_all_work`, `canonical_phases_explain_total_latency` (`--features bench`); V3 two server selectors | PASS |
| REQ-021/AC-021 declaration and judgment | `verification/mod.rs::direct` (client totals, no client criterion, no adjusted id, server overhead criterion kept on `direct-verify-c64`) | V1 `level_one_declares_direct_sweep_and_strict_objective`; `tests::level_one_declares_complete_engine_coverage` | PASS |
| REQ-022–023/INV-008/AC-023–024 native public callers | `verification/driver.rs::{Language, ForeignDriver, unsequenced}`; `run.rs::{Measured::reported, set_client_process, unrecorded_client}`; `sdks/wyrd-sdk-python/tests/benchmark_driver.py`; `sdks/wyrd-sdk-ts/wyrd/tests/profile/verification-driver.mjs`; `mise.toml` `bench:build:sdks` | V2 `language_drivers_preserve_public_boundaries_and_counts`, `verification::driver::tests::{steps_select_their_language, drivers_report_identity_then_every_call}`; Python `test_benchmark_driver_preserves_public_call_boundary`; TS `benchmark driver preserves public call boundary`; V4 smokes below | PASS (smoke) |
| REQ-024/AC-025 language/native profile coverage (S4) | `experiment.rs::{Role::{Python,TypeScript}, Workload::{clients,capture_clients}}`; `profile.rs::{MANAGED_PROBES, names}`; `driver.rs::{Language::role, ForeignDriver::heap}`; `run.rs::Bench::{prepare, clients, capture_clients}`; driver `Profiles` (Python `tracemalloc`, V8 snapshot); README "Python and TypeScript clients" (commit 3cb0c9647) | `profile::tests::managed_captures_must_name_their_probe_frame`; `verification::run::tests::client_heap_captures_name_their_gaps`; `verification::driver::tests::cpu_profiled_drivers_expose_their_runtime_frames`; Python/TS `benchmark_profiles_cover_language_and_native_work`; manual samply probe of `python -X perf` and `node --perf-basic-prof` named both probe frames | PASS (tests); V4 profiled run blocked by host.busy |
| AC-022 reporting, README, help | `benchmarks/README.md` Verification; `mise.toml` `bench:l1:verification` help | static review; standard renderer prints every new reading | PASS |

Commands (all passed unless noted):

```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity            # 71 passed
mise exec -- cargo nextest run --locked -p wyrd-server --lib --features bench -E 'test(=verification::telemetry::tests::canonical_phases_explain_total_latency) | test(=verification::telemetry::tests::hosted_setup_is_a_direct_part_outside_the_engine)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::telemetry::tests::overhead_is_the_execution_less_its_blocks)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::hosted::blocks::tests::histogram_overhead_excludes_real_overlapping_blocks)'
mise exec -- cargo clippy --locked -p wyrd-testing --all-targets -- -D warnings
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run --no-sync python -m pytest -q tests/integration/test_verifier_building_blocks.py::test_benchmark_driver_preserves_public_call_boundary'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/verifier-building-blocks.test.ts -t "^benchmark driver preserves public call boundary$"'
mise run fmt:check; mise run lints; mise run check:deps
mise run py:format:check; mise run py:lints; mise run py:typecheck
mise run ts:format:check; mise run ts:lints; mise run ts:typecheck
mise exec -- git diff --check
```

The TS test follows its file's sentence-name convention, so its selector
differs from the planned underscore name.

V4 smokes (`target/benchmarks/smoke/verification/direct-verify/`):

| Record | Populations | Result |
|---|---|---|
| `20261112T160836Z-1c1c58f53.json` | Rust c1/c64 | VALID. c1 client p50 4.871 ms (ready equal); envelopes cover every completed execution |
| `20261112T163238Z-ad4c67564.json` | all six | INVALID: Python driver failed hydration (diagnosis below). Rust c1 client p50 4.917 ms; Rust c64 client 32.039 ms; TS c1 client 5.205 ms, ready 5.209 ms; TS c64 client 25.763 ms |
| `20261112T163716Z-ad4c67564.json` | Rust c1, Python c1/c64 | INVALID only for `host.busy` (outside `qemu-system-x86` used 11 of 32 CPUs). Python c1 client 6.569 ms, ready 6.6 ms, client CPU 0.19 s, RSS 74.5 MB; Python c64 server p50 22.2 ms, client 27.361 ms, client CPU 7.4 s, RSS 124.6 MB |

Every Python/TS repetition records its runtime (`CPython 3.12.3`;
`node`/V8), SDK version, native module path and file identity, pid and
worker count. Their client phase roots are recorded inapplicable with an
"attribution unfinished" observation: the public SDKs record no benchmark
phases, and adding a recorder there would need a new public control.

**Diagnosis — Python driver exits during hydration.**
- **Symptom:** `direct-verify-python-c*` failed with "the driver exited
  before answering"; driver stderr `WyrdError: runtime hydration failed at
  artifact_load`.
- **Evidence:** the error's details are `reason: verified artifact directory
  is missing`, alias `model`, from
  `sdks/wyrd-sdk-python/src/state/mod.rs::missing_runtime_artifacts`; the
  pulled `cards/model/artifacts.yaml` was `[]`.
- **Cause:** Python eagerly hydrates every Model from its verified artifact
  directory, and the benchmark Model declared no artifact. The Rust and TS
  SDKs never load it, so earlier smokes passed.
- **Fix site:** the shared workload fixture. It declares the Model with one
  artifact, on its own (`WYRD_REGISTRY_400_HEAVY_ARTIFACT_NOT_SOLE_SUBMISSION`
  otherwise), and every Service references it by exact identity. No SDK or
  server change. The Rust and TS populations use the same fixture.

**Stop condition — c64 refusals (needs a decision).** Every c64 population
answers most calls `WYRD_SERVER_503_SERVICE_UNAVAILABLE`: Rust 222,722 and
TS 89,540 in one 3 s smoke window, Python 31,164. The server's default
verification limits (recorded in the `limits` control: 16 workspaces, 2..8
Python workers, 16 waiters) admit fewer than the 128 concurrent Agent
workspaces 64 canonical clients need. Workspace open and sandbox admission
refuse without waiting. A qualified c64 run therefore fails `no-errors`.
Changing pool limits or the workload geometry is outside this task (Material
Stop Conditions). The evidence is retained for a separately scoped decision.

**S4 coverage claimed and its limits.** CPU: samply captures each language
process under its own role with managed frames (`-X perf`,
`--perf-basic-prof`); a capture naming no `wyrd_profile_probe` /
`wyrdProfileProbe` frame is noted native-only and the profile incomplete.
Managed heap: `tracemalloc` and V8 snapshots per step, each checked for a
known probe allocation. Native heap inside the Python and Node processes is
**not covered**: the SDK native modules install no allocator hooks, so every
heap-profiled run with drivers is noted incomplete. Covering it needs
allocator hooks in the SDK cdylibs, a feature decision outside this task.
**V4 diagnostic run.** `mise run bench:l1:verification -- --profile
--allow-dirty --smoke` was refused before admission five times (`host.busy`,
a Lima `qemu-system-x86` VM and another session's builds) and ran on the
sixth: record `target/benchmarks/smoke/verification/direct-verify/20261112T181616Z-16a6b9e4a.json`,
INVALID only for `host.busy` after the run (5.40 of 32 CPUs). It saved 48
captures (server, driver, python, typescript: 12 each) across six steps and
was correctly reported incomplete. Its smoke readings, diagnostic only:
Rust c1 server p50 5.76 ms / client 6.11 ms; Python c1 7.25 / 7.73 ms;
TypeScript c1 5.83 ms; c64 server p50 56.5 (Rust), 48.3 (Python), 27.0
(TypeScript) ms.

The run exposed two defects. One is fixed: the report filed a capture's role
by any `-<role>-` in its path, so a capture under `direct-verify-python-c1/`
read as python. Roles now come from the file name's `cpu-<role>-` /
`heap-<role>-` prefix (`render_profile`, render test extended). The other is
open: **every Python and TypeScript CPU capture named no probe frame.**
Diagnosis: both drivers ran with perf maps
(`/tmp/perf-<pid>.map` held `wyrd_profile_probe` 2x and `wyrdProfileProbe`
9x), and the probe ran inside the language's own steps, but samply 0.13.1
embeds perf-map names only for a process it launches. Attaching with `--pid`
saves no perf-map names, with or without `--unstable-presymbolicate`; a
manual attach to a `python -X perf` loop reproduced it. The probe check
reported this correctly as native-only coverage. Covering managed CPU means
either launching each driver under samply for the whole run (one capture per
driver instead of one per step, a change to the per-step profile contract) or
an in-runtime CPU profiler (V8 inspector `Profiler`; Python has no stdlib
sampling profiler for all threads). Both need a decision.

Non-goals stayed excluded: no client target, no typed-model case, no L2
language sweep, no submit/wait or cold-start benchmark, no public SDK
benchmark API, no pool resize.
