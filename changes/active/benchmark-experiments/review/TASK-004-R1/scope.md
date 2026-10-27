# TASK-004 first implementation review

## Immutable subject and authority

Root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-experiments`.
Base: commit `0b500d677` (the packet-only parent of first implementation commit `b321c388c`).
Candidate: `a020500e8ea0c39311d508d6b5bfa85ebe90368c`.
Range: `git diff 0b500d677...a020500e8ea0c39311d508d6b5bfa85ebe90368c`.
Original task: `changes/active/benchmark-experiments/tasks/TASK-004-server-benchmark-timing.md` at base; candidate adds execution evidence.
Spec: `changes/active/benchmark-experiments/spec.md`, revision 8, SHA-256 `66a583cfaf0fb1310b55b3c92987c8bad859d5628ca06de575ca57208b5eea96`, unchanged since prior session's explicit maintainer approval. Approval persists; stale draft metadata/evidence is not a review blocker. The packet commit records this same spec text. TASK-003 dependency passed review at `71d2c240a` (prior TASK-003-R2 verdict).

This is the first review of TASK-004, covering the complete implementation range, not TASK-003 remediation. No TASK-004 acceptance or finding closure is carried forward. Available evidence is the original/candidate task and runtime artifacts, distinguished from independent review reruns. Inspect evidence diagnoses against source; do not accept the implementation summary as proof.

## Navigation map (starting point, not complete coverage)

- New `wyrd-telemetry/src/timing.rs`: bounded HDR registry, closed case/phase classification, cumulative snapshots, interval arithmetic, quantization and count validity. `lib.rs::record_timing` now feeds this owner rather than sampled timing spans. Existing `wyrd-bench::BenchmarkRecorder` owns tooling HDR summaries/global recorder; compare complete workflow and production dependency boundaries before accepting new machinery.
- Server `app/metrics.rs`: existing Prometheus recorder/router plus opt-in `/benchmark/timings`; config/env and app/server listener ownership; `wyrd-testing::release_server::LocalServer` readout. Preserve normal recorder/listener and refuse unsafe exposure.
- Request classification: Rust client `transport/trace.rs`, HTTP router and tonic middleware propagate benchmark metadata to authenticated contexts. Bound labels, ignore normally, prohibit identities/payloads and unknown-label evidence passing. Follow actual consumers, not only parser acceptance.
- Timing producers: Verification telemetry and hosted blocks; Gateway telemetry; Bifrost public query result lifecycle, Gate durable write, Oracle scan facts, gRPC OTLP acknowledgement. Existing monotonic timers/wait unions remain owners. Trace observations through snapshots into judgments.
- `wyrd-testing/src/capacity.rs`: snapshot reader/population, resources, smoke and expected reconciliation; `experiment.rs`: histogram accounting, v2 schema/read/report, legacy-v1 behavior, CPU/work/measurement/drain windows and validity.
- Capacity binary engine owners: query isolated/competing/read/write phases, Gateway run/capacity/recovery, Verification run; levels/main/workload dispatch and mandatory release smoke. Interleaved case/outcome/phase populations must not satisfy unrelated offered work. Inspect the original task/spec's population and strict-threshold obligations.
- `benchmarks/README.md`, manifests/lockfile and task evidence: record semantics, compatibility, dependencies, smoke proof. No nested AGENTS applies to these changed paths (root rules govern); docs site AGENTS is outside the write set.
- Relevant tests: task V1–V7 exact selectors and V2a actual boundary-owner additions (Verification addition lives in `verification::hosted::blocks::tests`); mandatory `bench:smoke` V8 and `wyrd-testing --lib --bins` under repository Postgres. Tests naming/failure diagnoses are claims to check.

## Selected independent roles and exact reports

All reviewers receive this same map and immutable subject. They must expand against actual source and applicable authority, without reading other discovery reports or the chat completion summary.

1. Fresh `behavior-rev`: full original task/spec acceptance and realistic engine-to-record/report paths; supported regressions, outcomes, windows, smoke, negative/recovery behavior and non-goals. `task-review-behavior.md`.
2. Fresh `invariant-rev`: materially complex bounded shared histogram state, concurrency/snapshot consistency, interval/quantile arithmetic, clocks, count/class/outcome/cohort reconciliation and resource denominators. `task-review-invariants.md`.
3. Fresh `repo-rev`: broad cross-tier mandatory rules across client/telemetry/runtime/server/tooling, new Rust-item documentation, dependencies, test layout/commands and architecture authority. `standards-review.md`.
4. Fresh `maintainer-rev`: substantial new timing owner and replacement collection/accounting, existing-owner reuse and duplicate-workflow audit, maintainability/minimality. `maintainer-review.md`.
5. Fresh security `domain-rev`: new opt-in local snapshot/readout and benchmark metadata trust/privacy boundary, authenticated-context propagation, loopback refusal, bounded labels/output, normal behavior and no tenant/payload leakage. `domain-review-security.md`.
6. After discovery/follow-up, fresh independent `ponytail-rev`: validate proposals and overall acceptance coverage from source, corrections/materiality, consolidated ledger. `findings-validation.md`.

No separate system specialist selected initially: stock server lifecycle remains owner, and behavior covers credible smoke/collection/persistence/drain failures while invariant and security specialists cover shared state/exposure. Add a specialist only if actual discovery establishes a material unresolved failure boundary. No role is simulated by the orchestrator.

## Execution and reporting constraints

Reports only in this new directory, no source/diff archives. Source remains immutable; no implementation, formatting, commits or pushes. Preserve untracked pre-existing benchmark records. All Cargo and measurement commands are sequential; orchestrator owns runtime checks, reviewers request focused checks rather than competing builds. No concurrent benchmark/build work. Distinguish available implementation evidence, artifact validation, source proof and independently rerun checks. No intended verdict or findings quota. Use CodeGraph before code search and run shell commands through mise. Required correction must follow explicit approved obligations and material supported scenarios, not optional hardening. Prior accepted unrelated behavior is not a new task acceptance basis.
