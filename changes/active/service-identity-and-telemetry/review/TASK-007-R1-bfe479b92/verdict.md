# TASK-007 remediation-review verdict

**PASS**. `FIND-TASK-007-1` and `FIND-TASK-007-2` are closed. The independently validated open finding ledger is empty; no remediation task is required.

## Immutable subject and review scope

- Repository: `/Users/stevenforrester/Documents/GitHub/schema-change-task-7`.
- Original base: `76e095738d2cf89e10523a92bb9b4f585fa1dd3d`.
- Prior candidate: `dabe0e7dde58c6e26d584120ae0a48cfede79436`.
- Reviewed candidate: `bfe479b9248f4f71c43a719b61db1ce35bad69d9`.
- Delta reviewed: `git diff dabe0e7dd bfe479b92`; cumulative original task context: `git diff 76e095738...bfe479b92`. Ancestry is unchanged.
- Authority: approved revision-16 service-identity-and-telemetry spec, original TASK-007, applicable repository/architecture/testing rules, and [R1 remediation task](../TASK-007-dabe0e7dd/TASK-007-R1-complete-performance-evidence.md).
- Prior acceptance and findings: [prior verdict](../TASK-007-dabe0e7dd/verdict.md), [prior validated ledger](../TASK-007-dabe0e7dd/findings-validation.md), and its linked discovery/query-domain/verification reports.

This is the first remediation review. It checks the two original findings, the test/telemetry/document delta and affected consumers. No production Oracle, authority, persistence, dependency or lifecycle behavior changed. The source-backed prior acceptance of ordering, distributed planning, canonical results, windows, groups, traces, source visibility, limits, deadlines and non-goals carries forward. No unchanged area was reopened for comprehensive discovery, and no convergence assessment is required.

The candidate stayed fixed and tracked source remained clean. Only the new report directory was written; review did not swap source files, implement, commit, merge, push or deploy.

## Independent reviewer and reconciliation

One fresh `ponytail-rev` independently reviewed closure, actual source/callers, baseline correspondence, metrics and retained experiment evidence, expanded population/policy assertions, changed-item rustdoc and existing-owner reuse. Its [findings-validation.md](findings-validation.md) recommends PASS and contains the full closure matrix plus numerical evidence appendix.

No additional discovery/specialist reviewer was selected because the remediation changes no sensitive production boundary; prior domain coverage remains applicable. No follow-up reviewer was needed: no new blocker, conflicting material claim or unresolved failing path remained. The validator independently resolved the initial missing-table-cell concern from existing original measurement outputs and retained a numerical summary in its report. The orchestrator did not substitute its own source findings for independent validation. [scope.md](scope.md) records navigation, selection, scope and reconciliation.

## Reconciled acceptance and prior-finding closure

| Obligation | Independent source and verification evidence | Result |
|---|---|---|
| FIND-TASK-007-1: comparable final-contract observations | Baseline differs in precisely the two Oracle production files; common R1 harness and three repeated original runs per side match the approved final contracts. Explicit wrong-answer baseline handling prevents an invalid speed comparison. | CLOSED |
| FIND-TASK-007-1: elapsed, query/source/planning/scan/work/spread and representative write resources | Existing checkpoint/process/profile owners supply measurements; read-only resource rendering and exact unit test; committed R1 comparison plus independently extracted stage/range/CPU/RSS tables in findings-validation.md. | CLOSED |
| FIND-TASK-007-1: attribution and unavailable metrics | Whole-process CPU/RSS, endpoint memory, summed operator times, orchestration versus query duration, audit/background writes, single-node and unavailable page/decode detail are explicit. No writer-specific or causal speed claim is accepted. | PASS |
| FIND-TASK-007-2: populated gateway and exact shared identities | Ordinary authenticated gateway calls carry accepted release, subject and environment; all eight writer tables require positive results; UID/name-version equivalences and negatives are asserted. Expanded exploration passed. | CLOSED |
| FIND-TASK-007-2: audit as ninth table with truthful semantics | Settled cutoff, nonempty ground truth and four existing accountability keys; named null-aware columns avoid positional misreading without inventing observation attribution. | CLOSED |
| FIND-TASK-007-2: full/proper-subset/empty policy proof | Existing alter-table owner applies strict half-default subset and empty declarations; equal writer rounds, exact live/published answers and separate writer/audit filter-byte observations. | CLOSED |
| Existing result/window/group/custom/trace behavior, source and authority | Accepted production owners unchanged; current five focused journeys passed; prior negative/trace/storage/distributed acceptance and credible original regressions carried forward. | PASS |
| Rust structure/docs and smallest existing-owner correction | Existing telemetry renderer, public gateway, native query/alter and physical-inspection owners reused; substantive changed-item rustdoc; task citations removed. No duplicate workflow/profiler or dependency added. | PASS |
| Non-goals and unrelated changes | No cache/index/store/lookback/SLO/default/public/generated-contract change; committed prior reports are historical review context, not unrelated runtime behavior. | PASS |

Validated open ledger: **empty**. Stable prior IDs are preserved as closed; no new ID or correction work is created.

## Measurements and conclusion limits

The original detailed observations are summarized numerically in [findings-validation.md](findings-validation.md); original experiment locations are recorded there and in [verification.md](verification.md). The review preserves the needed evidence in a report rather than relying exclusively on temporary raw logs or modifying its committed subject.

The task's blanket statement that every timing difference is below run-to-run spread is not carried forward. For example, published full-default exploration planning medians are 676.0 ms at the base versus 689.3 ms at the candidate, with nonoverlapping three-sample ranges. This is a bounded observed difference, with no isolated cause or demonstrated material supported-use regression; no new performance budget applies. Accepted conclusions are exact result preservation, comparable observed work/timing/resource figures and their stated limits, not a causal no-cost or speedup guarantee. No further experiment or tuning is required for this bounded remediation.

## Verification

Review-time results in [verification.md](verification.md):

- Exact new telemetry unit: PASS 1/1.
- Server/Monty prerequisite build and isolated Postgres migration: PASS.
- Exact result, window, group, custom SQL and expanded exploration journeys: PASS 5/5, serial execution, 90.534s total.
- Remediation-range `git diff --check` and ancestry checks: PASS.

The fresh validator independently inspected all six original journey logs and the six storage and six trace logs. Candidate journey runs passed 5/5 three times; original-base custom query failed on ordering in two of three runs as disclosed. Storage and trace original runs passed on both sides. This review did not repeat the baseline source substitution or the full campaign. Format/lints/tenant isolation remain implementation-reported; full gateway payload, Oracle/distributed suites and cross-platform/capacity checks were not rerun. Existing macOS linker warnings did not fail builds/tests.

The remaining limitations are fixture scale, whole-process/endpoint resource attribution, small repeated samples and single-node execution. They are explicit accepted measurement limits, not open findings. PASS closes TASK-007's task-review gate; it does not finalize the entire change packet or authorize integration/deployment.
