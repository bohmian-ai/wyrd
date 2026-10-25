# TASK-003 remediation review scope

## Immutable subject and authority

- Root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-experiments`.
- Original base: `7f422c53ad54e370c24cb971a343ec5e06651109`.
- Prior candidate / remediation base: `45641bd5eadf8c86111d82fda6afd9605e218f84`.
- New cumulative candidate: `71d2c240abb0544e07155a93cf8b6d5b5f95a228`.
- Review diff: `git diff 45641bd5eadf8c86111d82fda6afd9605e218f84 71d2c240abb0544e07155a93cf8b6d5b5f95a228`.
- Cumulative original context: `git diff 7f422c53ad54e370c24cb971a343ec5e06651109...71d2c240abb0544e07155a93cf8b6d5b5f95a228`.
- Approved authority: working-tree revision 8 of `changes/active/benchmark-experiments/spec.md`, SHA-256 `66a583cfaf0fb1310b55b3c92987c8bad859d5628ca06de575ca57208b5eea96`. The maintainer explicitly approved this text in the prior review session; approval persists. Stale frontmatter and implementation evidence do not reopen that resolved question. Candidate Git contains the older spec; this is externally supplied approved review authority.
- Original task: `changes/active/benchmark-experiments/tasks/TASK-003-production-tracing.md`, original at prior candidate and updated V5 at cumulative candidate.
- Prior verdict and independently validated findings: `../TASK-003-R1/verdict.md`, `findings-validation.md`, discovery reports, and `verification.md`.
- Remediation: `../TASK-003-R1/TASK-003-R1-production-shutdown-and-proof.md`, including externally available uncommitted implementation evidence. Review artifacts are untracked but present; no commit of them is required to inspect the pinned source subject.

This is the first remediation review after the first complete implementation review, not a new first review. No rebase or ancestry change occurred. Preserve prior reports and stable finding IDs.

## Scope and carried-forward acceptance

Independently check closure of FIND-TASK-003-1 (changed-item rustdoc), FIND-TASK-003-2 (new quiet story ignore and V5 selection), and FIND-TASK-003-3 (guard disposal reaches bounded stock provider shutdown despite retained tracer references). Review the entire remediation diff and affected guard/caller/test paths for fix-induced regression.

Carry forward the source-backed passing rows in prior `findings-validation.md`: filters/ratio policy, sampler/W3C identity, tenant catalog/auth/scrubbing and nonblocking outbox, failed-batch accounting, stock queue mechanics, engine inventory, configuration/docs and non-goals. Reopen only the portions affected by guard lifecycle or new material evidence; record why. Guard disposal behavior affects both production and test initialization, explicit shutdown, Arc ownership, production role/outbox drain, and capture fixture lifetimes, so inspect these callers. No whole-task rediscovery or new span inventory is warranted.

## Navigation map (starting point, not complete coverage)

- `crates/shared/wyrd-telemetry/src/lib.rs`: `TelemetryGuard`, consuming `shutdown`, new `Drop::drop`; provider disposal via `Option::take`; new `tests::guard_drop_shuts_down_a_retained_provider`; changed docs on existing test helpers/tests and storage fields. Initializers: `init`, `init_test_capture`, `init_test_only_no_global`.
- `crates/shared/wyrd-telemetry/src/export.rs`: field/error docs on existing `FailedBatches`; no accounting or processor algorithm change.
- Production consumers: `wyrd-server/src/app/metrics.rs::WyrdTelemetryRuntime`, `wyrd-server/src/app/mod.rs::run`; test consumers include `wyrd-testing/src/server.rs`, cluster telemetry handles and capture/timing tests. Trace actual lifetime/order, not just calls to the explicit shutdown method.
- Installed OpenTelemetry SDK provider/processor: retained tracer/provider ownership, explicit shutdown, timeout and exactly-once behavior. Retain installed stock mechanisms.
- `sdks/wyrd-sdk-rust/tests/integration/action_timeline.rs`: quiet production story ignore removed, existing assertions/setup unchanged; original task V5 now includes all three stories. Shared-family lane owns Postgres; nextest owns per-test process isolation.
- Root AGENTS and applicable Rust/maintainer guides apply; no nested AGENTS in changed source directories. Spec workflow and trace-context/reliability architecture remain authority. CodeGraph exists and must precede code search; current on-disk source must correspond to candidate.

## Reviewer selection and reports

Select one fresh independent `ponytail-rev` for bounded remediation closure and validation, as the skill's remediation default requires. Exact report: `changes/active/benchmark-experiments/review/TASK-003-R2/findings-validation.md`.

Do not recreate discovery specialists: the correction fulfills the previously reviewed guard owner contract, with no new processor/queue/concurrency policy, public setting, security/persistent-data boundary or orchestration. The fresh validator must inspect the affected lifecycle and callers. If it discovers a new material blocker, obtain separate focused source validation before retaining it; add a specialist only for material unresolved scope.

## Execution constraints

Review is read-only except reports in this new directory. No source edits, formatting, commits, pushes or implementation. Preserve all unrelated worktree changes. Orchestrator runs focused Cargo checks sequentially; reviewer must not compete for builds. Report independently inspected source separately from implementation-reported and orchestrator-rerun evidence. No broad gates, full language suites, live restart harness or benchmarks. No intended verdict is supplied.
