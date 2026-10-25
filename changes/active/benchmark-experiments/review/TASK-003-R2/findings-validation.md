# Independent remediation validation — TASK-003

Recommendation: **PASS**. All three prior findings are closed. The independently validated outstanding ledger is empty.

## Subject and scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-experiments`.
- Original base: `7f422c53ad54e370c24cb971a343ec5e06651109`.
- Prior candidate/remediation base: `45641bd5eadf8c86111d82fda6afd9605e218f84`.
- Cumulative candidate: `71d2c240abb0544e07155a93cf8b6d5b5f95a228`.
- Reviewed the complete `git diff 45641bd5eadf8c86111d82fda6afd9605e218f84 71d2c240abb0544e07155a93cf8b6d5b5f95a228`: four paths, with no ancestry change. Current changed source matches candidate Git; HEAD remains the candidate.
- Governing authority is the explicitly approved working revision 8 of `../../spec.md`, SHA-256 `66a583cfaf0fb1310b55b3c92987c8bad859d5628ca06de575ca57208b5eea96`, checked independently. Approval is resolved; stale metadata is not a finding.
- Acceptance context: original `../../tasks/TASK-003-production-tracing.md`, prior `../TASK-003-R1/findings-validation.md`, verdict, behavior/system reports, claim comparison and verification, and `../TASK-003-R1/TASK-003-R1-production-shutdown-and-proof.md`, including its available uncommitted evidence.

This is the first bounded remediation review. Read root rules, task-review skill, specification workflow, maintainer/Rust guidance and the relevant approved tracing obligations/architecture. No nested AGENTS applies to the changed source directories. Used CodeGraph before code navigation. No broad task rediscovery, new inventory requirement or new specification decision was introduced.

The affected scope is guard disposal, explicit shutdown, initialization, retained provider ownership, production shutdown ordering, capture/test lifetimes, changed-item documentation, and quiet-story selection. Inspected complete changed runtime/test bodies and the affected production and test consumers rather than relying on implementation completion claims. Unchanged source-backed acceptance below is carried forward from the prior independent ledger. Only the shutdown-dependent rows and delivered proof/documentation gates required a new conclusion.

## Closure decisions

| Stable finding | Decision | Independently checked closure |
|---|---|---|
| FIND-TASK-003-1 — changed-item rustdoc | CLOSED | `export.rs:255–270` documents the retained exporter field and the exact delegated export failure. `lib.rs:673–676,700–706,763–772,805–808,819–838,853–860,898–901,961–974,1083–1093,1170–1177` supplies the missing storage/error/panic contracts for Staged, serve, production filters, Logs, Gated, FailedBatchCount and the telemetry tests. New guard disposal and retained-provider regression have intent/invariant/panic documentation. The correction is confined to the previously changed items and new lifecycle correction; no behavior was weakened to satisfy documentation. |
| FIND-TASK-003-2 — quiet journey ignored/omitted from V5 | CLOSED | The only SDK source edit removes the new ignore on `action_timeline.rs:191`; the production initializer and `act_and_read` assertions/setup are unchanged. Original task V5 now selects connected-tree, quiet-policy and other-tenant stories. Existing Postgres lifecycle and nextest process isolation support this test. Orchestrator confirms default list selection without an ignored marker and reruns all three successfully. |
| FIND-TASK-003-3 — deployed disposal omits SDK shutdown | CLOSED | `lib.rs:301–324`: consuming `shutdown(self)` drops the guard; Drop takes its optional provider and explicitly calls the installed stock provider's shutdown, logging an error. `lib.rs:1095–1169` tests actual guard disposal with a retained tracer, controlled queued/dropped spans, exporter drain/shutdown and exact SDK total, then explicit shutdown and no-provider disposal. Production caller ordering and stock bounded lifecycle are independently traced below. |

These decisions rest on source and observable proof, not agreement with the appended implementation evidence. No finding is deferred or relabeled as nonblocking.

## Lifecycle validation and correction burden

`TelemetryGuard` remains the existing lifecycle owner. `init` (`lib.rs:445–483`) still builds one provider with the stock batch processor and installs its tracer globally. `init_test_capture` (`lib.rs:505–549`) uses that same guard/provider composition with its existing simple capture processor; `init_test_only_no_global` (`lib.rs:424–429`) returns no provider. The correction adds no public configuration, timeout policy, worker, queue, dependency or alternate initialization path. A downstream cleanup wrapper or per-serving-branch fix is unnecessary.

The installed OpenTelemetry SDK 0.31.0 `trace/tracer.rs:26–29` retains an SDK provider in each tracer. `trace/provider.rs:246–274` explicitly shuts down through an atomic false-to-true transition, defaulting to five seconds; `:137–147` skips final-reference shutdown once that transition occurred. Thus the guard's explicit invocation works while the global tracer remains alive, and later provider/tracer destruction does not invoke processors again. Taking the Option also makes ownership disposal unambiguous; the consuming API cannot retain a second provider for its subsequent drop.

The provider's actual processors are unchanged: `export.rs:67–81` installs TenantExport and optional OperatorExport. `TenantExport::shutdown_with_timeout` (`:550–552`) is immediate because its sink owns delivery. `OperatorExport::shutdown_with_timeout` (`:232–233`) delegates the bound to the stock batch processor. Installed SDK `trace/span_processor.rs:633–694` logs the lifetime total and waits with `recv_timeout`; worker shutdown (`:367–386`) exports buffered spans, shuts down the exporter, then acknowledges. The bound is on the shutdown caller's wait: a stalled exporter may finish later, and errors remain logged. Existing V3 proves that distinction with a controlled 100 ms timeout and eventual exporter shutdown. No new timeout injection or live restart harness is needed.

The new regression retains `tracer` until after guard disposal. Its first gated export has started before one span is queued and one dropped; two released export results allow the stock worker to complete the pending/drain work. Assertions require the queued export, exporter shutdown, `dropped_span_count=1`, and no second shutdown after releasing the retained tracer. A separate retained-tracer instance exercises explicit guard shutdown once, and a no-global guard exercises the no-provider path. The implementation reports the expected RED (queued-span assertion sees Empty without disposal shutdown); this review did not mutate immutable source to reproduce RED. The orchestrator independently reran GREEN.

Production `WyrdTelemetryRuntime` (`wyrd-server/src/app/metrics.rs:239–290`) retains its Arc guard until the enclosing lifecycle ends; recorder-install failure now also disposes the already-created local guard. `app/mod.rs:43–105` retains runtime and local guard while either API serving or standalone Forge supervision runs, then releases compaction/coordination runtimes before the telemetry references. API `BoundServer::run` (`app/server.rs:740–895`) settles transport/worker and Gateway/MCP/hosted work, drains the Scribe outbox before Bifrost closes, and shuts down submission delivery before returning. The standalone path (`app/mod.rs:121–190`) awaits its existing bounded supervisor before return. No drain ordering was edited; cleanup now occurs at the previously promised shared boundary, including early-return disposal.

Sibling test consumers remain sound: `wyrd-testing/src/bifrost/cluster.rs:687–730` retains its capture guard in process-wide OnceLock ownership, so dropping one server/node Arc cannot prematurely disable other capture users. `wyrd-testing/src/server.rs:4676–4686,4747–4753` borrows that process ownership or uses the existing no-provider guard when requested. `shutdown_and_inspect` (`:1162–1205`) joins serving and settles owners before dropping its server. Direct telemetry capture tests end their spans, flush/read capture, and explicitly shut down (`lib.rs:1200–1215,1231–1264`), so the new Drop does not discard observations they still need. The quiet SDK journey retains its local guard until its deployment shutdown finishes (`action_timeline.rs:191–202`).

## Reconciled acceptance matrix

| Approved obligation / constraint | Implementation evidence | Verification evidence / carry-forward basis | Result |
|---|---|---|---|
| REQ-017 independent log/operator policies and tenant copies at quiet settings | Shared subscriber/filter composition unchanged | Prior independent acceptance; current V1 and quiet V5 rerun | PASS |
| REQ-017 default 0.01, explicit ratios and validation | Existing operator_sample_ratio/telemetry_sampler/server validation unchanged | Prior V2 and source-backed acceptance carried forward | PASS |
| REQ-017/INV-010 caller isolation, W3C IDs/parents/links | Existing ServerSampler and OperatorExport decisions unchanged | Prior V4/V5/V6 acceptance; current three-story V5 | PASS |
| INV-001/003/010, AC-010 authorization, catalog, safe fields and nonblocking tenant delivery | Existing tenant routes, binding, TenantExport and Scribe outbox unchanged; shutdown remains after existing drain | Prior V4/V6 cap/rejection and tenant-negative acceptance; current V5 | PASS |
| REQ-018 accurate failed-batch metric and unchanged result/delegation | FailedBatches body unchanged; only field/error docs changed | Prior acceptance plus current V3 rerun | PASS |
| REQ-018 stock bounded finite queue and saturation/request independence | Same stock BatchSpanProcessor; no queue, worker or policy replacement | Prior SDK source/V3/V6 acceptance; current V3 | PASS |
| REQ-018/AC-011 deployed orderly disposal reaches exact lifetime total and final drain | TelemetryGuard Drop now explicitly invokes retained provider shutdown; production owner/order inspected | New guard regression and current V3; SDK five-second default and bounded receive independently inspected | PASS |
| REQ-018 scrubbed correlated unsampled error logs | Existing formatting/safe-field policy unchanged | Prior acceptance plus current V1 | PASS |
| REQ-017/019 operation-boundary inventory and optimization workflow | Engine instrumentation/inventory unchanged | Prior source-backed inventory and representative trace acceptance carried forward | PASS |
| REQ-019/INV-006 normal, temporary-capture and restore docs with truthful loss limits | Existing authority/operations/config text unchanged; promised orderly SDK invocation now supplied | Prior docs acceptance; lifecycle source/guard/V3 proof updates the former failed portion | PASS |
| AC-011 delivered real SDK timeline and tenant-negative proof | Quiet story no longer ignored; maintained V5 includes all three | Default selection confirmed by orchestrator; corrected V5 3/3 | PASS |
| Mandatory changed-item Rust documentation | Missing field/error/panic contracts supplied; new Drop/test documented | Manual source validation, scoped fmt/clippy | PASS |
| Non-goals, minimal ownership and dependency boundaries | Four-path remediation diff reuses existing guard/provider/fixtures; no new abstraction or dependency | Full remediation diff inspected; prior unchanged boundaries carried forward | PASS |

## Evidence and final ledger

Read `verification.md` in this directory. Orchestrator reruns against unchanged candidate passed: exact new guard regression, V1 and V3 (one each); corrected V5 (three stories); repository Postgres migration prerequisites; scoped formatting; telemetry all-targets/test-support clippy and SDK integration clippy with warnings denied; prior-to-candidate and worktree whitespace checks. The default nextest list selects the quiet story without an ignore. These are orchestrator-executed results, not runtime runs by this validator. Implementation-reported full telemetry 28/28 and RED evidence remain distinguished from independently rerun focused proof.

Outstanding findings: **none**. Independently validated ledger: **[]**. No new material blocker, fix-induced regression, unresolved correction boundary, missing selected report or specification conflict was found. Separate focused source validation is unnecessary because no new blocker was proposed. The bounded shared-owner correction closes the original failure rather than adding another lifecycle owner; no additional remediation is recommended.

Recommended task verdict: **PASS**.

Skipped: duplicate builds, broad gates, benchmarks, live process restart and full language-surface suites; integrated change review retains those applicable obligations. Risk: abrupt termination cannot guarantee a final drain/total, and stock timeout return does not promise a stalled exporter has already finished.
