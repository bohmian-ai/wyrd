# TASK-007 independent repository standards review

Overall result: **FAIL**.

Subject: base `a7582db587c6170a290760f1741673125612b797`, candidate `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`; cumulative `git diff HEAD~1`. The candidate remained detached and unchanged during this review. Scope follows the user's TASK-007 ownership restriction: Oracle/Scribe live scan, partitioning, local transport, `staged_tail.rs` deletion, and the three diagnosed Oracle journey corrections. Bundled benchmark/harness migration, lifecycle timer and unrelated lint work were excluded except supporting consumers of the three corrections.

This is a standards audit. It does not independently decide task acceptance, propose refactors, or reproduce runtime qualification. No builds, tests, Postgres wrappers, full mise lanes, source edits or commits were performed.

## Authority coverage

All applicable reference slices were selected through `architecture/references/README.md`. CodeGraph is absent, so repository navigation used `rg`, source reads and Git diffs.

| Changed surface | Governing authorities read | Surrounding source/consumers inspected |
|---|---|---|
| `oracle/follower.rs`, `oracle/exec.rs`: staged scan, predicates, source lifetime | `AGENTS.md` §§3–6, 9–12, 15–16; `architecture/agent-rules.md`; `wyrd-design.md`; `wyrd-doctrine.mdx`; `bifrost-design.md`; references `languages/rust-core.md`, `languages/errors.md`, `languages/maintainer-style.md`, `architecture/patterns.md`, `doctrine/architecture-constraints.md`, `domain/datafusion.md`, `domain/iceberg.md`, `domain/olap-serving.md`, `domain/arrow-analytical-interop.md`, `domain/analytical-operations-reliability.md` | Full added resolver leaf, resolution/decode callers, `HotParquetExec` lease wiring and staged scan tests; catalog/storage ownership |
| `oracle/live.rs`, `resources.rs`: session partitions and streaming | Same Rust/engine authorities; Bifrost query, admission, memory, terminal and cancellation sections | `LiveScribeExec`, `LiveFragmentRead`, `LiveFrameDecoder`, `follower_execution`, partition contract test, live frame test |
| `oracle/dispatcher.rs`, `oracle/mod.rs`, server `oracle/peer_service.rs`: local Arrow transport and pending reservation shutdown | Same Rust/engine authorities; Bifrost system boundary, distributed execution, resource/failure invariants | Transport directory routing, `ScribeFragmentExecutor`, gRPC encoder, reservation registry and shutdown ownership |
| `scribe/tail_rpc.rs`, removal of `scribe/staged_tail.rs` and module edge; `storage/cache.rs` | Same Rust/engine authorities; Bifrost live-tail authority and staged-reader lifetime | `LiveTailBatches::into_parts`, lease/source fields, moved production-observation wrapper, staged fixture and lease test, new `ObjectPin::Staged` identity |
| Workspace/redux manifests and lockfile | `AGENTS.md` §§2–4, 15; reference `architecture/patterns.md`; `domain/datafusion.md` dependency boundary; `languages/rust-core.md` nonblocking IO | Direct dependency edge and pinned filesystem backends; current lockfile and its diff |
| Supervisor attempt observation and three corrected journeys | `AGENTS.md` §§11–12, 16; `agent-rules.md`; references `languages/testing-workflows.md`, `languages/spec-driven-development.md`, `languages/implementation-execution.md`; Bifrost telemetry and analytical execution | Counter increment/settlement sites, existing production attempt telemetry, `PeerCluster::attempt_counts`, one-worker graph-lease assertion, table destination producer and route/task-count consumers |
| TASK-007 verification evidence | `AGENTS.md` §11; references `languages/spec-driven-development.md` and `languages/implementation-execution.md` | Task Evidence table and Diagnosis, named tests in owning modules, declared commands |

No Python, TypeScript, PyO3, schema, public wire/error catalog, SQL migration, CLI, MCP catalog, UI or documentation-site surface is changed by TASK-007. Their separate typing, generated-artifact, serving and test rules are therefore not invoked by this bounded diff. Auth/fence validation remains in the server-owned existing executor; the change adds no tenant URL input or permission decision.

## Rule assessment

| Applicable rule | Result | Source evidence |
|---|---|---|
| Rust-owned server behavior; Vala engines never own listeners; narrow dependency owner | PASS | Staged planning remains `ScribeTailResolver`; network framing remains `wyrd-server::oracle::peer_service`; storage dependency is on redux, not client/foundation crates. |
| Struct-centered workflow and meaningful dependency ownership (`AGENTS.md` §5) | PASS | Resolver owns tail/catalog/storage/FileIO; live read owns leaf/routes/bindings; executor owns Scribe. New predicate conjunction and filesystem construction are narrow deterministic helpers. |
| Narrow async IO boundary and no blocking request IO (§6) | PASS | `live_leaf` awaits `tokio::fs::metadata`; staged bytes go through the pinned OpenDAL-backed FileIO. The alternative pinned `LocalFsStorage` directly calls `std::fs` from async methods. No new runtime is created. |
| Single analytical native dependency universe | PASS | Added dependency is pinned to the same Iceberg revision. `Cargo.lock` adds only redux's direct edge; OpenDAL 0.58.2 was already resolved alongside 0.57.0. No second Arrow/DataFusion/Parquet universe is introduced. |
| Tenant-qualified source/cache identity and local staged ownership | PASS (static) | New staged metadata key carries tenant, table, path, writer epoch and size. Source paths come from opened local Scribe authority; no remote pod path is exposed to the leader. |
| Existing lease, cancellation and terminal mechanisms preserved | PASS (standards shape) | `HotParquetExec` and streams retain the staged lease; local completion is explicit; remote `AttemptEncoder` remains only on gRPC. Domain/runtime correctness is delegated to the appropriate independent reviews. |
| Library errors use typed propagation, structured diagnostics; no input unwrap introduced | PASS | New staged construction maps to redacted `FollowerResolutionError::Fault`; executor errors retain `DispatchError`; no new public error code/parallel mapper. |
| New/materially modified item rustdoc (§16) | PASS | Added source owners, fields/variants, helpers and tests explain operation/lifetime; fallible additions describe errors; focused fixture/scan tests document panic conditions. |
| Top-of-module imports and bare signature/field types (`agent-rules.md`) | **FAIL** | REPO-007-1 below. |
| Tests in owning Rust runtime and justified integration binary; no production metric labels with node identity | PASS | New scan/frame tests are module tests; existing real server journeys stay in their justified integration target. Attempt observation is feature-gated; production metric labels remain bounded. |
| Required tests must not be weakened to hide a violation | PASS for diagnosed one-worker correction | One-table destination is selected in `Oracle::register_cut_providers`; `OracleRouteTasks` routes that stage to its frozen peer; live-stage task count is capped at one in `AnalyticalCutTaskCount`. The revised assertion still demands remote graph activation, actual peer body polls, exact rows and one fragment per Scribe. |
| Exact focused command for every named test in task evidence (§11) | **FAIL** | REPO-007-2 below. |
| Recorded format/lint/targeted verification and no blind diagnosis | PASS as recorded; execution not reproduced | Task records format/lint/diff checks and tracing-enabled reruns of the three journeys; the current user expressly forbids reproducing Postgres/full lanes. Missing precise recipes are a separate finding, not an invented failed run. |
| Generated artifacts, permanent task names in code, wildcard versions, lint suppression and test skipping | PASS within TASK-007 | No generated artifact edit, new task-ID code comment, wildcard dependency or new suppression/skip in the owned changes. Existing gated journey ignores are preserved. |

## Material findings

### REPO-007-1 — new imports and signatures violate the repository's explicit Rust dependency declaration rules

Classification: **VIOLATION**.

Rule: `architecture/agent-rules.md` requires “All `use` statements live at the top of the module” and “Bring types in with `use` and use bare names in signatures”, explicitly including struct fields and return types. The test exception permits imports in a `#[cfg(test)] mod tests` scope; it does not permit imports inside individual test functions.

Exact changed evidence:

- `crates/vala/vala-bifrost-redux/src/oracle/follower.rs:3178–3179`: new pruning test imports `Int64Array`, `ScanLiteral` and `ScanPredicate` inside the function. Its test module already owns a dependency-import block.
- `oracle/follower.rs:693`, `:695`, `:718`, `:745–746`: added storage/FileIO fields and added/materially changed signatures use qualified type paths.
- `oracle/exec.rs:2779`, `:2844`: new staged lease field and method parameter name the type with a qualified module path.
- `scribe/tail_rpc.rs:351–353`, `:375–376`: new split and observation functions expose qualified Arrow/path/lease/stream types in signatures.
- `crates/wyrd/wyrd-server/src/oracle/peer_service.rs:235`: materially changed executor return type uses `arrow::datatypes::SchemaRef` despite requiring module-level type imports.
- `oracle/analytical_supervisor.rs:393`: new attempt-count field repeats fully qualified atomic type paths.

Consequence: the added dependency surface is split between module imports, signatures, fields and function bodies, contrary to the mandatory discoverable dependency manifest. This is an explicit repository requirement, not a preference for an alternative layout. It creates no claimed runtime failure.

Smallest correction: move only the new function-local imports into the owning test module's existing import block, and import the types used by the new/materially changed fields/signatures into each owning module. Keep qualified value/construction expressions where allowed. Do not refactor unrelated old qualified signatures, add helper abstractions or change behavior.

Closure proof: inspect the TASK-007 diff for the cited rule violations, then formatting and the permitted scoped Clippy check. Existing focused scan/frame checks suffice; no new runtime test is needed for import placement.

### REPO-007-2 — named verification claims lack the required reproducible exact commands

Classification: **VIOLATION**.

Rule: `AGENTS.md` §11 requires every specifically named test in a task artifact/report to “include and run its exact focused command via `mise exec --`”. `architecture/references/languages/spec-driven-development.md` “Test command precision” requires explicit package, target, features and exact expression, with environment wrappers for Postgres.

Location: `changes/active/bifrost-scribe-live-reads/tasks/TASK-007-one-parquet-scan-for-live-reads.md:71–78`, `:82–95`.

Evidence: the Evidence table names the staged pruning, dropped lease, publication lease, projection/filter, follower partition and live-frame tests. It records only their names and pass assertions, a module regex fragment, an aggregate lane and a wildcard server test group. Diagnosis names three journey reruns but replaces the actual expression with literal `-E 'test(=…)';` neither their complete package/target/features/wrapper recipes nor the actual traced rerun recipes are recorded. The full task has no exact focused Cargo command.

Consequence: a maintainer cannot reproduce the exact claimed proof from the task packet without rediscovering package/target/feature/environment details, and the packet cannot establish that each named check was selected and run as required. This does not assert that the named tests failed.

Smallest correction: add the actual exact focused command and outcome for each already named test, using current owning targets and minimal features. Recover recorded command evidence where available; otherwise run the allowed non-Postgres focused commands and explicitly defer prohibited Postgres execution to the authorized integrated caller. Do not run a Postgres wrapper during this review, invent prior run evidence, replace a journey with a unit check or rerun a full lane.

Closure proof: each named test in the packet has an exact reproducible recipe and truthful outcome; any unavailable rerun is distinguished from previously recorded execution. User restrictions remain controlling. The current user's allowed raw-Cargo template overrides repository invocation defaults for this review; this finding concerns the original packet's missing historical recipes, not the permitted current invocation or the absence of a prohibited Postgres/benchmark rerun.

## Requested concern assessments

The new test-support attempt counts do not replace missing production observability. `AnalyticalAttemptTelemetry::start` and `finish` remain at the same admit/settle sites. TASK-007's “production metric, not a test hook” clause governs staged row-group pruning proof, which uses existing production `OracleQueryScanStats` (`files_scanned`, `row_groups_scanned`, `row_groups_pruned`). The new counts solve attribution within a multi-node, single-process test fixture. Turning node identity into a metric label would conflict with Bifrost's closed bounded-label policy. No standards finding requests that change.

The one-worker assertion is supported by the existing table-cut destination and route/task-count owners described above; it is not justified merely because it makes the test green. The recorded independent diagnosis is in the task. No standards finding demands two workers for one frozen table cut.

`iceberg-storage-opendal` is a direct edge to an already installed pinned backend, not a new OpenDAL version introduced by TASK-007. The native `LocalFsStorage` at the pinned revision uses blocking standard filesystem calls; existing `tokio::fs` call sites in staging do not implement the Iceberg FileIO backend required by `HotParquetExec`. The direct backend dependency stays in the narrow owning crate, so no dependency standards violation was established.

## Verification limits

This reviewer used static inspection and packet evidence. The orchestrator subsequently recorded 18 passing focused unit tests and passing `git diff --check` in `verification.md`, including pruning, lease drop/publication protection, nonzero-ordinal projection, follower shape and decoder completion. Those results corroborate the supporting unit proof without supplying the omitted exact historical recipes. The prohibited Postgres/full-lane tests were not run. This report finds two bounded standards violations; its PASS rows do not independently certify acceptance behavior or runtime recovery. Both findings require independent validation before inclusion in the final verdict.
