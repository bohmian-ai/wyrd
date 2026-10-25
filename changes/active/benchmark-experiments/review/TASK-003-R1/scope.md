# TASK-003 first implementation review

## Subject and authority

Root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-experiments`.
Base: `7f422c53ad54e370c24cb971a343ec5e06651109`.
Candidate: `45641bd5eadf8c86111d82fda6afd9605e218f84`.
Range: `git diff 7f422c53ad54e370c24cb971a343ec5e06651109...45641bd5eadf8c86111d82fda6afd9605e218f84`.
Task: `changes/active/benchmark-experiments/tasks/TASK-003-production-tracing.md` at candidate, including verification evidence.
Spec: working-tree `changes/active/benchmark-experiments/spec.md`, revision 8, SHA-256 `66a583cfaf0fb1310b55b3c92987c8bad859d5628ca06de575ca57208b5eea96`.

The maintainer explicitly confirmed in this review session: “the spec is ... approved.” This establishes approval of revision 8 and supersedes the earlier account of outstanding approval, stale draft frontmatter, and stale implementation evidence wording. Candidate Git contains revision 6; use the above explicitly approved revision-8 text as externally supplied governing authority. Do not edit review subject files to update metadata. The previous authority-check verdict performed no implementation review and carries no findings or acceptance evidence forward. This is the first complete implementation review, not remediation.

## Navigation map (starting point, not complete coverage)

- `crates/shared/wyrd-telemetry/src/lib.rs`: `TelemetryConfig`, `operator_sample_ratio`, `init`, `init_test_capture`, `subscriber`, `telemetry_sampler`, production-filter/default/export-loss tests. Callers include server `app/metrics.rs`, config validation, capture fixtures, and real SDK journeys.
- `crates/shared/wyrd-telemetry/src/export.rs`: `FailedBatches`, failed-batch metric; existing `ServerSampler`, `OperatorExport`, `TenantExport`, catalog and scrubbing own operator/tenant separation and processor lifecycle.
- `crates/wyrd/wyrd-server/src/config.rs`: shared ratio validation; environment/config producer and telemetry initialization consumers.
- `sdks/wyrd-sdk-rust/tests/integration/action_timeline.rs`: real SDK/Gateway/timeline readback, quiet production policy and tenant-negative paths; existing server `pg_verification_runtime` and outbox cap tests are regression seams.
- Manifests and lockfile: existing workspace metrics and telemetry dependencies.
- Architecture trace-context section, operations reliability/recovery, docs deploy settings table: normal policy, temporary capture, restoration and loss signals.
- Root AGENTS applies throughout; `docs/AGENTS.md` additionally applies to the changed deploy guide. Use CodeGraph before code navigation; its current source must be checked against immutable candidate where relevant.

## Roles and exact reports

- Fresh `behavior-rev`: comprehensive task acceptance, repository rules (including changed Rust-item rustdoc and nested docs rules), owner reuse/maintainability, filtering/sampling, caller-to-tenant output and credible failures. Report `task-review-behavior.md`.
- Fresh `system-rev`: operator export failure/stall, queue overflow and shutdown, deployed process availability, tenant-copy independence and operational documentation/proof. Report `system-review.md`.
- Fresh independent `ponytail-rev` after discovery: validate every proposal and acceptance coverage; report `findings-validation.md`.

System specialist selected for explicit export-loss and bounded-failure obligations. Separate invariant/domain specialists are unnecessary: established tenant routing, sampler, scrubbing, and stock processor remain owners; behavior reviewer must verify preservation and system reviewer covers lifecycle. No substantial ownership replacement or broad cross-tier contract implementation requires maintainer/repo specialists; those obligations remain with behavior review. Add a specialist if actual discovery reveals a material boundary change.

## Review constraints and verification

Reports only in this directory. No implementation edits, commits, resets, formatting, broad gates, or concurrent benchmarks. Preserve unrelated worktree edits. Independently inspect the full range, not the completion summary. Use task V1–V7 results as available evidence, challenge their sufficiency, and distinguish reported from rerun checks. Reviewers may run narrow read-only verification; coordinate runtime checks through the orchestrator. No findings quota or optional hardening. Approval is resolved by the explicit instruction above.
