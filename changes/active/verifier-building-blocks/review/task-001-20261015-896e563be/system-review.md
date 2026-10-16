# System-resilience review — TASK-001

## Review Findings

No material system-resilience findings. **PASS** for this lens.

The candidate stages pure contracts and injected interfaces. It deploys no new verification engine, sandbox, queue, upload handler, or worker. Two shared live helpers change implementation: DAG planning and regex-operator validation. Existing callers retain their graph and rejection behavior. This conclusion does not accept a future runtime implementation or replace the other task-review lenses.

## Subject and authority

- Base: `08b38f74255d344e1c5ec9a6d278935a26277622`.
- Candidate: `896e563bee69ab6b5f47b31977d7de1a0ca88d57`.
- Inspected the complete cumulative source change and inline tests. HEAD remained the candidate; `git diff --quiet <candidate> -- crates/skald/skald-spec crates/vala/vala-eval crates/wyrd-spec` passed. Uncommitted docsite/planning edits were excluded.
- Authority: supplied AGENTS.md, agent rules, task-review skill, Wyrd design/doctrine, evaluation reference, spec-driven-development and maintainer references, original task and approved revision 6 snapshots. The subject expressly approves BlockError instead of JudgeError on the new interfaces while preserving offline scenarios.
- Scope: pure contracts and minimal injected handoff now; real engine/hosted implementations in Tasks 002/003; live protocol cutover in Task 005. Approved coordination defers full integration/journey execution until all tasks merge.

## Independently expanded coverage

CodeGraph preceded source discovery. Searches expanded its map; actual source and pinned diff grounded the conclusions. No other reviewer's report was read.

| Changed surface | Callers/owners traced and consequence |
|---|---|
| `card/blocks/{mod,validate,reference,schema_path,extract}.rs`, `card/mod.rs` | New declarative vocabulary, synchronous registration/schedule validation and locked extraction schema. `envelope.rs:28,241` still imports and stores the old `card::verifier::VerifierSpec`. New authoring is not admitted by live Card registration. |
| `verification/{execution.rs}`, `verification.rs`, `ids.rs`, `error.rs` | Staged request, evidence-upload, result/status, identity and error conversion contracts. Current `components/verification/service.rs:425` still accepts ExecuteVerificationRequest and returns Judgment. The visibility-only change to message_details preserves existing error rendering. |
| `refs/mod.rs` | New Visit implementation reuses existing Agent/Prompt and durable slots. Canonical ReferenceSlotVisitor still dispatches the old Spec::Verifier (`:152`); existing Verifier visitor remains intact (`:329–353`). No live loader normalization or authorization path changes. |
| `vala/eval/{plan,operator,spec,mod}.rs` | Reachable shared changes: EvalSpec::{new,validate,execution_plan} → validate_dag → plan_stages; ScenarioScoring::new → TaskRegistry → execute_scenario/execute_plan. Existing BTreeMap IDs are unique. Layer sorting, missing/self/cycle refusal and existing cycle witness remain. ComparisonOperator::validate moves the same two regex branches unchanged. |
| Skald `media.rs`, `prompt.rs`, `lib.rs`; `card/prompt/parameter.rs` | New pure MediaFormat vocabulary, getter and Ord support. Existing MediaRef and Prompt binding behavior are unchanged. The new media_kind getter is called only by staged block validation. |
| `vala-eval/src/{blocks,lib}.rs` | New ActionExecutor/CheckExecutor seams have no production implementation/caller. The only implementation found is test Echo (`blocks.rs:172`). Existing runtime consumers remain on TaskExecutor, Executors and JudgeInvoker. |

The map was expanded through `envelope.rs`, `card/verifier.rs`, `refs::ReferenceSlotVisitor`, server Card resolution, `ScenarioScoring::{new,score_scenario,score_record,score_record_batch}`, `execute_scenario`, `TaskRegistry::from_plan`, `execute_plan`, current direct execution, VerificationRuntime and VerifierRunner.

## Deployed topology and failure paths

The server hosts verification alongside other request capabilities. Current VerificationRuntime composes supervised capability tasks over AppState, the SQL run queue and process Scribe outbox. A capability exit marks its own health slot down and triggers backoff/restart (`verification/mod.rs:260–298`). The runner resolves the existing implementation (`runner.rs:304–310`), applies execution timeout (`:333–353`) and maps completed/retry/trace-wait/defer/terminal outcomes (`:355–364`). These owners are unchanged.

Offline Vala reaches the changed planner through EvalSpec::execution_plan and ScenarioScoring::new (`orchestrator/scoring.rs:40–60`). A planner error remains a construction-local Result. Task execution still runs through the existing stage loop (`executor.rs:201–214`), propagating errors rather than fabricating failed assertions. The planner adds collections of dependency references, with no IO, spawn, cache, lease, retry or retained state. Existing EvalSpec task bounds remain (`spec.rs:89–99,117–127`).

| Credible failure | Actual boundary, survival and recovery |
|---|---|
| Missing dependency/self-loop/cycle | `plan.rs:110–208` retains the structured refusal before executor dispatch. Old map inputs cannot duplicate IDs; new block validation rejects duplicate IDs before planning. No work/state is created. Corrected input can be reconstructed. |
| Invalid/overlong regex | `spec.rs:182–189` now calls the inherent validator containing the same regex checks. Refusal remains local to validation. |
| Provider/trace outage during scoring | Existing executors and propagation are unchanged. BlockError's retry/terminal distinction is a staged contract, not an active new retry loop. |
| Direct timeout/cancellation | `components/verification/service.rs:425–492` still drives old DirectExecutor under timeout. Future drop cancels this invocation. No new block executor runs under this path. |
| Runner outage, process restart or replacement | Existing lease claim, renewal, cancellation and fenced settlement remain (`runner.rs:529–684`). Capability restart remains local. Process interruption leaves durable rows to expiry/reclaim. Pure helper errors acquire no process-shutdown authority. |
| SQL/Scribe outage or interrupted publication | No transaction, acknowledgment callback, outbox writer or durability configuration changes. No new state survives or is lost. New completed-without-visible-result/delayed-conflict wire states represent future outcomes; they do not implement them. |
| New executor cancellation | Not deployed here. `blocks.rs:9–15,65–82,136–155` assigns deadline/drop ownership to the future engine and executor resource release to its implementations. The interface acquires no resource itself. Echo's happy path does not prove descendant cancellation or worker containment. |

No pod/process isolation, startup, health, retry policy, migration, persistent format or durability boundary changes. Shared graph/operator owners were extended without adding a runnable orchestrator. Visitor extension does not replace the live loader workflow.

## Verification Notes

Reviewed tests for new graphs/references, schedule mappings, schemas, position/media validation, extraction output shapes, wire results and object-safe executor calls. Existing `vala/eval/mod.rs` covers DAG missing/self/cycle cases and regex validation; `vala-eval/src/executor.rs` covers error-versus-false-result propagation and mixed task execution. The implementer's 1069 library-test pass, Clippy, workspace test compilation, fmt and dependency-check claims are recorded evidence, not independent reruns.

Review was static: no tests, builds, integration environment or fault injection were run. Source evidence supports preservation of current failure propagation, not future engine resilience. `codegen:check` was not reported run; this report does not claim generated-contract validation.

## Open Questions

None affecting the system-resilience conclusion within the staged boundary.

## Result

**PASS**. No proposed finding IDs.

Skipped fresh execution and future runtime fault proof. Activation still requires the owning tasks to prove timeouts/cancellation, crash containment, accepted-authority handoff and replay/recovery against real implementations.
