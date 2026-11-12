# TASK-005-R1 — Close measurement qualification and detail-availability proof

Route directly to `$wyrd-implement`. Status: ready for bounded correction; new workload execution retains the original separate authorization prerequisite. This review request does not authorize benchmarking.

## Subject and authority

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/resource-scaling`.
Approved spec: `changes/active/benchmark-latency-diagnosis/spec.md`, TASK-005 revision-6 obligations as retained in revision 7.
Original task: `changes/active/benchmark-latency-diagnosis/tasks/TASK-005-full-path-benchmark-parity-and-optimization.md`.
Review base: `0c81715f2ac52efd2b105e454ee309724df1655f`; reviewed candidate: `180879328ac9dde94d7504855e821980e5db0f18`.
Acceptance context: original task, spec, [verdict](verdict.md), and independent [validated ledger](findings-validation.md). First remediation, no prior closure to assume. Candidate source was not changed by review.

The result must qualify the existing measurement path, demonstrate missing-detail behavior with real server proof, and close its bounded mandatory documentation gaps. Existing compact/detail implementation is source-supported; this task does not redesign it.

## Diagnosis and selected corrections

### FIND-005-1 — Qualification remains unresolved

REQ-022/AC-018 and original task lines 626–632 require observer qualification before attributing improvements. Its lines 706–725 explicitly retain U→M envelope misses and an INVALID M Verification run; lines 778–783 retain two uncompleted independent-reference proofs. Later compact-response comparisons are useful but cannot retroactively qualify a different representation/milestone. Consequently reported gains do not close the original measurement-quality gate.

Use the existing canonical reports, qualification and calibration owners. Inspect retained evidence first and resolve source/build/workload/method correspondence before comparison. Complete the existing `tests::pg_tests::engine_measurements_keep_boundaries_processes_and_windows` and `verification::run::tests::calibration::the_measurement_path_agrees_with_independent_references` proofs sequentially with the repository Postgres lifecycle on an idle host, preserving host guards and actual populations. Obtain any workload authorization required by the original task before execution. If retained comparable records cannot resolve observer cost, prepare the smallest current-code observation-cost experiment for separate authorization. Keep every standard repetition and original validation/observer controls. Do not rebuild historical baselines or relabel INVALID/confounded evidence. Correct a measurement implementation only if this investigation demonstrates its defect; unresolved qualification remains unresolved until the existing acceptance condition is met.

This requires existing proof/report machinery, not another recorder, framework or SLO. Pure evidence inspection cannot substitute for the two expressly required observations, and new speculative runtime machinery is unjustified.

### FIND-005-2 — Incomplete history and recording failure are unproved

REQ-026/AC-021 and original scenario 12 require known incomplete/missing detail plus a useful direct result when recording fails. `crates/wyrd/wyrd-server/tests/integration/pg_verification_routes.rs:2595–2697` first waits for published detail and checks an unknown random ID, authority and foreign tenancy. None exercises a known execution whose required record is unavailable or a direct execution whose result recording fails. The implementation appears correct statically, but a named public-contract journey obligation is missing.

Extend the existing server route/runtime fixtures at the shared `VerificationControl` HTTP/service boundary. Demonstrate a known queued execution with incomplete retained data and a completed execution with a required retained record unavailable: explicit details returns `WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND`, while existing queued status/optional result semantics remain intact. Separately induce direct result-recording failure and prove the caller receives the useful compact judgment without waiting for publication; explicit detail reads must report absence rather than fabricate successful empty evidence. Reuse existing publication/fault controls and authenticated Bifrost read paths. Change runtime only if these focused proofs expose a defect. Update original evidence to identify the actual observations.

These bounded fixture/evidence additions are sufficient; a new availability tracker, retention contract, parallel reader, or duplicated SDK/MCP failure suite would increase scope without closing another cause.

### FIND-005-3 — TASK-005 subset of mandatory changed-item rustdoc

The root rustdoc gate applies to private and test items; green lints do not waive it. Add concise accurate missing contracts to this exact TASK-005 subset (candidate locations):

| File | Items / missing contract |
|---|---|
| `crates/wyrd/wyrd-gateway/src/request.rs:257,322,414` | `Canonical::borrowed` Errors; `Part::visit_borrowed_str` intent/non-root invariant/Errors; `canonical_borrowed_strings_match_compact_serialization` Panics |
| `crates/skald/skald-providers/src/auth/google_oauth.rs:452` | `access_token_source_needs_no_network_client` Panics |
| `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:1783,1790` | `answered`, `serving` Panics |
| `crates/wyrd/wyrd-testing/src/profile.rs:396,422,438` | `heap_endpoint`, `saving_profiler`, `ready` Panics |
| `crates/wyrd/wyrd-testing/src/experiment.rs:5739` | `saving_profiler` Panics |
| `crates/wyrd/wyrd-server/tests/integration/pg_verification_routes.rs:2449` | `compacted` Panics |

Document actual mismatch errors, fixture setup/shape expectations and deadline failures. Preserve behavior; deletion of an item legitimately removed by a functional correction closes that row. TASK-006-R1 owns the other subset under this same finding ID; do not duplicate its edits or create a documentation-only round.

## Constraints and preserved behavior

Retain compact default results, explicit descriptive details, exact authored configuration and unchanged stored evidence. Preserve authorization/audit/tenant boundaries, no hidden detail IO, and no Prompt compilation or judgment on retained reads. No new SDK/API, cache/availability service, retention promise, dependency or benchmark mechanism. Preserve host guards, workload identity, deadline/cancellation and measurement truthfulness. Do not fix Azurite or infer c64 fixture causality without separate supported diagnosis. No broad gate or full journey suite is required by this remediation.

## Acceptance and focused proof

| Finding | Closure observation |
|---|---|
| FIND-005-1 | Both exact existing reference proofs complete sequentially; retained/canonical evidence resolves the original observer qualification with every repetition and honest validity/comparability. Host refusal or missing authorization does not count as a pass. |
| FIND-005-2 | Focused real-server tests establish known incomplete/unavailable history 404, unchanged queued/status semantics, and useful direct compact result despite recording failure. |
| FIND-005-3 (005 subset) | Exact listed changed items have accurate required rustdoc sections; behavior unchanged. |

Use existing owning mise setup, repository Postgres wrapper and test target definitions. Run only the new/extended exact server integration selectors and the two named existing reference selectors (sequential, retaining their release-server prerequisites and workload authorization). Record actual new selectors and verify discovery before execution. Use targeted affected format/lint tasks, `git diff --check`, and only relevant boundary checks if runtime write set expands. Documentation alone needs source inspection/format; do not rerun broad journeys. Preserve command results and canonical report identities in the task evidence, including any remaining refusal. Later review retains this original subject and checks only remediation closure, changed code and affected paths.

## Implementation evidence (R1)

Benchmark and reference-proof runs were authorized by the maintainer for the
current code only; no code was reverted to measure old baselines. Runs were
serial with nothing else of this session building. Commits: `ab2e78944`
(FIND-005-2), `f48af0892` (FIND-005-3), `e96164a54`, `fe0ac1505`,
`660160c04` (measurement-proof fixes below), evidence `bfd1748c9`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-005-2: known incomplete/unavailable history 404, queued/status semantics unchanged, useful direct compact result despite recording failure | `wyrd-server/tests/integration/pg_verification_routes.rs` `details_report_absence_when_retained_records_are_missing`; runtime unchanged | queued execution: status `pending`, no `result`, details `WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND`; completed with an unretained `result_id`: status `completed`, no `result`, details 404; WAL disk-full: direct execute 200 with compact `{requirements, verdict}` = `passed`, details and status 404; after recovery and flush, compacted details equal the direct outcome. With `details_preserve_visibility_and_read_authority` and `details_preserve_retained_execution_evidence`: 3/3 PASS | PASS |
| FIND-005-3 (005 subset) | `# Errors`/`# Panics`/intent on every listed item in `request.rs`, `google_oauth.rs`, `capacity/main.rs`, `profile.rs`, `experiment.rs`, `pg_verification_routes.rs::compacted` | source inspection; `mise run fmt`; clippy `-D warnings` | PASS |
| FIND-005-1: calibration reference proof completes | three measurement-proof defects fixed (below) | `verification::run::tests::calibration::the_measurement_path_agrees_with_independent_references` PASS (53.7 s): wall per verification 0.0935 ms collected vs 0.0936 ms control, observation cost −0.0001 ms (allowance ±0.0100); server CPU 0.1024 vs 0.1021 ms, cost +0.0003 ms (allowance ±0.0102); windows 5.0010/5.0008 s; spin CPU 0.98 s in [0.80, 1.33]; server p50 (0.0388, 0.0388] ms under client p50 0.075 ms; every server and driver boundary reconciled every call | PASS |
| FIND-005-1: engine reference proof completes | recorder fix `e96164a54` | `tests::pg_tests::engine_measurements_keep_boundaries_processes_and_windows`: every Bifrost hot/parquet step, all writes-parquet steps but one, Verification `direct-verify-c1/c64`, and all eight Gateway `overhead` steps hold; fails on `native-write-c64` (see unresolved) | FAIL |
| FIND-005-1: observer qualification resolved with every repetition | calibration observation cost above; current-code L1 Verification records in TASK-006-R1 FIND-006-2 | direct-Verify observer cost is within its allowance on current code; Gateway observer cost has no calibration proof of its own; the original U→M envelope misses were measured on other source under host load and are not relabelled | UNRESOLVED for Gateway |

### Measurement-proof defects found and fixed

1. **Driver recorder never started** (`e96164a54`).
   - **Symptom:** the Verification smoke record was INVALID: 0 `client.verify`, `client.http` and `client.http.attempt` successes for 32576 completed calls, and every driver component was missing. Calibration also counted no successful `client.verify`.
   - **Evidence:** `wyrd-benchmark/src/lib.rs` `enabled()` is false until `install()`, so `Observation::start` returns a no-op. `capacity/main.rs` `level()` calls `start_benchmark()`. `pg_tests.rs::measured` and the calibration test call `run(..)` directly and never call it.
   - **Cause:** both proofs skipped the step that `l1`/`l2` take to start the recorder. Production `bench:l1:*` is unaffected.
   - **Fix site:** start the recorder in `measured()` and in the calibration test. An independent read-only diagnostician produced the same cause.
2. **Calibration summed boundary counts** (`fe0ac1505`).
   - **Symptom:** recorded total 217276 against a population of 54319.
   - **Cause:** once the driver records, four boundaries each reconcile all 54319 calls. The assertion summed them, which only works when the server is the sole recorder.
   - **Fix:** each cohort must equal the population, which is stricter than the old sum.
3. **Control window scraped a server with collection off** (`660160c04`).
   - **Symptom:** `/benchmark/snapshot` returned 404 after the restart with `WYRD_METRICS_BENCHMARK_SNAPSHOT=0`.
   - **Cause:** `Window::open`/`measure` always scrape.
   - **Fix:** the control window uses `ResourceWindow` directly, freezing CPU and wall time at the stop before in-flight calls drain, the same semantics as `Window::measure`.

### Unresolved: engine proof `native-write-c64` audit-row reference

- **Symptom:** `writes-parquet: native-write-c64 repetition N: server rows inserted read R+1, outside its reference [R, R]`. This was repetition 1 at `e96164a54` (8045036 vs 8045035) and repetition 2 on the rerun (6898086 vs 6898085). Every other step holds.
- **Evidence:** `rows_match_the_server` counts one audit row per request the driver acknowledged. The memtable counter `bifrost_scribe_memtable_rows_inserted_total` has no labels, so it includes audit rows. `GrpcSink::send_owned_bytes` (`wyrd-client/src/bifrost/grpc.rs`) silently retries busy or uncertain attempts with the same batch id, up to `max_frame_retries = 3`. `Gate::dispatch_native_frame` authorizes, and so stages an audit row, before Scribe admits.
- **Leading hypothesis:** under 64 writers, a native write is retried after an attempt the Gate had already authorized. That stages one audit row the driver never sees.
- **What did not confirm it:** I tried taking the audit term from the server's `bifrost_gate_requests_total{operation="write"}` delta. The benchmark server's snapshot does not export that family, so it read 0 in every window. I reverted both attempts (`a78bb1cfd`, `5e533cfe3`) and the check is back to its original form.
- **Needed:** a server-side count of authorized write attempts that the benchmark snapshot exports, or a driver-visible retry count from the native client. Either is a new observation surface, and the remediation excludes new benchmark mechanisms, so this is left for a decision.
- **Separate refusals during this work, none counted as passes:** setup got `WYRD_AUTH_401` because another process held the harness's fixed `127.0.0.1:8080` (readiness answered before our server booted), and one rerun was refused `host.busy` (12.8%, from node and qemu processes outside this session).

Commands:
```
RUSTFLAGS='-C force-frame-pointers=yes' CARGO_PROFILE_RELEASE_DEBUG=line-tables-only mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked --release --target-dir target/profiling -p wyrd-testing --bin capacity --run-ignored=only --no-capture -E 'test(=verification::run::tests::calibration::the_measurement_path_agrees_with_independent_references)'
RUSTFLAGS='-C force-frame-pointers=yes' CARGO_PROFILE_RELEASE_DEBUG=line-tables-only mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked --release --target-dir target/profiling -p wyrd-testing --bin capacity --run-ignored=only --no-capture -E 'test(=tests::pg_tests::engine_measurements_keep_boundaries_processes_and_windows)'
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration -E 'test(/pg_verification_routes::details_/)'"
```
Also `mise run fmt`, clippy `-p wyrd-testing --bin capacity --tests -D warnings`, `git diff --check`: PASS.

Non-goals stayed excluded: no new SDK/API, cache, availability service, retention promise, dependency or benchmark mechanism; host guards, workload identity and recorded populations unchanged; no historical baseline rebuilt; no INVALID or confounded evidence relabelled.

Status: FIND-005-2 and FIND-005-3 IMPLEMENTED. FIND-005-1 is BLOCKED on the `native-write-c64` audit-row reference above; the calibration half passes.
