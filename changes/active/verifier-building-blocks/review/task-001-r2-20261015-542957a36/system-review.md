# Independent system-resilience review — TASK-001 cumulative R2

**PASS. No material system-resilience findings.** This is a task-scoped assessment of the complete `08b38f74255d344e1c5ec9a6d278935a26277622` → `542957a3622461e259aee5f8108d978e18adad1c` change. It does not certify the future hosted verification runtime, sandbox or integrated replacement.

## Subject, authority and method

Reviewed the immutable subject, approved r6 specification and original task, initial and R1 verdicts/validated ledgers, R1 and R2 remediation instructions, cumulative 26-file change and latest four-file correction. Applied AGENTS.md, agent rules, the task-review skill, relevant design/doctrine ownership and verification/authority/recovery rules, and the reference router's architecture-constraints, evaluation, Rust, spec-driven-development and maintainer guidance. The approved task explicitly stages contracts while retaining the existing live format until Task 005. Current owner direction assigns complete legacy LLM-judge removal, including offline consumers, to that cutover; temporary compilation-preserving retention is not a regression here.

CodeGraph exploration preceded source discovery. Expanded its initial map into the live envelope, reference visitor, existing Eval planner/spec/store, generic permission producers and authority carrier, shared Prompt owner, gateway batch projection, and staged executor/result consumers. Confirmed HEAD is the candidate and `git diff --exit-code 542957a36 -- crates` is empty. No production source, specification, task or earlier report was changed. This report is the only assigned write.

## Deployment and caller evidence

The applicable design supports self-hosted, single-tenant and multi-tenant deployments behind one logical `wyrd-server` surface, potentially replicated or targeted by subsystem. No process, worker pool, target, dependency, startup setting, persistent table, queue, writer, health check or deployment manifest changes in this task.

| Changed surface | Actual owner/callers and system effect | Assessment |
|---|---|---|
| `card/blocks/{mod,reference,validate,schema_path,extract}.rs`, `card/mod.rs` | Public pure construction/validation/serialization. `validate.rs:64,87` lead to the local Validator; required reference analysis is consumed at `:692` and `:738`, and check schema fit at `:569`. The live envelope still imports `card::verifier::VerifierSpec` (`envelope.rs:29`), not `card::blocks::VerifierSpec`. | The pure API is required now, but does not mount a second live authoring/registration/execution route. Errors are returned to its caller. No shared server shutdown or runtime retry loop is introduced. |
| `verification/execution.rs`, `verification.rs`, `ids.rs`, `error.rs` | New request/result/status/provenance and execution identity contracts; existing Wyrd errors/statuses reused. Source search finds the new result types in the staged validator and Vala seam/tests, not mounted handlers or background writers. `error.rs` only widens an existing helper to crate visibility. | No synchronous reservation, durability guarantee, result publisher or recovery state is added. Completed-without-visible-result and delayed conflict are representable; future persistence remains later work. |
| `vala-eval/blocks.rs`, its module export | `ActionExecutor` and `CheckExecutor` declarations at `:138,149`; only Echo's test implementation exists. `BlockCall` carries resolved inputs/source slots, position, workspace and deadline. ActionOutput and StepResult share ToolCallRecord. | No production model/tool/sandbox work is launched. Deadline racing and cancellation remain engine-owned by contract; no claim that drop cancellation has already been implemented is made. The narrow seam retains no queue or hidden resource owner. |
| `vala/eval/{plan,spec,operator,mod}.rs` | Existing `validate_dag` delegates to `plan_stages` (`plan.rs:110-112`); old EvalSpec and Vala TaskRegistry paths remain consumers. Shared regex validation moves behind `ComparisonOperator::validate`, and `validate_task_operator` delegates to it. | Actual shared live/offline regression boundary. The planner retains missing-dependency/self-loop/cycle errors and sorted stages; the old map guarantees unique IDs, and new block validation checks its global ID namespace before planning. Regex compilation/length refusal is preserved, not duplicated. |
| `refs/mod.rs` | New `Visit for blocks::VerifierSpec` at `:356` uses existing SlotEntry/Agent/code/drift traversal. Existing `visit_paths` still dispatches the old `Spec::Verifier`. | No new Card loader, resolver, cache or registry path. No change to IO, credentials or recovery behavior of existing reference resolution. |
| `card/agent.rs`, `auth/permission_scope.rs`, runtime `permission.rs`, `request_context.rs` | One ten-name BuiltinTool catalog. Resource/Scope Tool extends existing validated three-axis permissions. Existing role-row decoder (`wyrd-auth/issuance.rs:797`) reads Vec<Permission>; existing coverage/intersection and AcceptedAuthority serde carry the same values. request_context production code is unchanged; its addition is a test. | No default grants or new checker. Existing grants retain their scope and general wildcard meaning; Cards/Bifrost/Verifier grants do not imply tool invocation, and tool invocation does not imply them. Exact authority can survive the existing accepted-work representation without a second persisted format. |
| Server `components/gateway/batches.rs:169-195` | `invocable` first selects Gateway/Invoke grants. Its only change adds Tool to the ignored disjoint-scope arm at `:190-193`. Listing pruning remains subordinate to the existing audited invocation decision. | A tool grant cannot become gateway reach. No new provider call, retry, cancellation, listing query or batch continuation behavior is added. |
| Skald `media.rs`, `lib.rs`, `prompt.rs`; Prompt ParameterName ordering | Pure MediaFormat vocabulary/codecs/schema; existing Prompt gets read-only media-kind access at `prompt.rs:467`, called only by the new validator. ParameterName adds ordering. | No replacement MIME IO/sniffer, provider path or changed live Prompt binding. New declaration checking rejects a portable media-kind mismatch before later runtime use. |

The complete new scope reference parser and extraction/result shapes remain data/pure transformations. No network/file access or new hidden durable authority was found in those modules. Existing DAG/operator/visitor/Prompt/permission owners are reused; no second graph executor, permission store or workflow is introduced.

## Credible failure and recovery paths

| Failure or interruption | Producer → owner → affected capability | Containment, recovery and proof |
|---|---|---|
| Finite authored schema with combinator/local-reference cycle | JSON slot schema or resolved action Prompt schema → validate_reference/output_schema → shared Walk::admits. Check declarations also enter Walk::fits/values. | `schema_path.rs:14,49-65,68-81,109-140,234-251`: one 1024-visit/hop budget is retained across recursive branches and refs. Exhausted selector analysis returns unknown/admitted; unproved Verdict fit returns false. Calls finish without a persistent poisoned validator/cache; a corrected declaration can be retried independently. This establishes bounded recursion/ref work, not a blanket bound on all input width or allocations. Public cyclic input/output and finite-recursive known-missing tests pass in fresh logs. No deliberate process abort was attempted. |
| Incompatible check schema or invalid graph/operator | Authored declaration → pure position validator/shared planner/operator owner → Result refusal | Refusal is local to validation; no panic, `process::exit`, server startup failure or poisoned shared state was added. Latest allOf sibling, finite const/enum and literal extra-name cases are handled by the same existing bounded guard. Old graph cycle/missing-dependency semantics and regex refusal remain intact. Healthy-path tests are supplemented by negative/cyclic checks. |
| Provider unavailable, attempt timeout or cancellation during a block | Future engine → staged ActionExecutor/CheckExecutor → BlockError/result seam | This candidate adds no production executor or engine caller. Retryable versus terminal errors, engine deadline and drop ownership are explicit in `blocks.rs:1-15,67-80,115-154`. There is no changed outage/cancellation implementation to fault-inject now. Requiring a worker, retry controller or process-level shutdown would implement later tasks, not close a Task 001 defect. |
| Delegated accepted work is serialized then reloaded after restart | Existing subject/actor effective-permission intersection → Principal → AcceptedAuthority → existing queued-work consumer | New exact Tool scope is a typed serde enum with strict name/resource/action validation. `permission.rs:989-1033` reuses narrower scope/axes and retains gateway-only conditional semantics. `request_context.rs:78-106` keeps the accepted subject and delegation and drops credentials. Prior four-library proof includes exact-tool authority round-trip and gateway merge/intersection tests; those production bodies are unchanged since R1. No DB migration or new replay representation is introduced. This is encoding/attenuation proof, not a claim of a new tool dispatcher surviving a real restart. |
| A caller has tool-only permission and lists/continues gateway batches | Existing Caller → gateway invocable filtering → existing listing and invocation owners | Resource coverage rejects Tool as Gateway; the new exhaustive arm ignores it. Existing Gateway/All/model/deployment projection is unchanged. No failure amplification or unrelated capability loss is introduced. |
| Server process crashes, Scribe/DB is unavailable, lease expires, or a replica is replaced | Existing live runner/outbox/lease paths | The cumulative diff does not change claim/renew/settle/outbox code or start another worker/writer. Existing design's loss, lease cancellation and retry behavior remains the governing contract; neither improvement nor a new guarantee is inferred from staged wire types. There is no changed lifecycle path here requiring new crash/outage proof. |

## Prior-finding closure within this review's scope

| Stable ID | Source-backed assessment |
|---|---|
| FIND-TASK-001-1 | Preserved closed: Tool resource/scope, validation, coverage, intersection and accepted-authority encoding are present at the existing owners. No default/implicit domain grant or alternate durable carrier was added. |
| FIND-TASK-001-2 | Preserved closed: all ten BuiltinTool names feed `validate_agent` membership; filesystem/workspace and tool-less check guards remain. |
| FIND-TASK-001-3 | Latest specified bounded closure is present: `fits` falls through from non-proving allOf parts to same-node facts (`:130-136`), infers types from finite values (`:153-158`), and compares anchored literal-name pattern additions against closed typed properties (`:202-218`). Existing-field refinements remain permitted. Source and fresh focused/full-library proof cover the R2 cases. No arbitrary schema/regex inclusion or numeric/string-strength requirement is added. |
| FIND-TASK-001-4 | Preserved closed: ToolCallRecord carries optional typed output digest and returned_bytes; both ActionOutput and StepResult use it. No-output semantics and pure handoff are explicit; a production projector is not part of this candidate. |
| FIND-TASK-001-5 | Preserved closed: full recursive walk/ref hops share the finite budget across both schema consumers, retaining unknown-versus-known-missing behavior. |
| FIND-TASK-001-6 | The two R2 formatter Errors sections are now present (`reference.rs:235-240`, Skald `media.rs:303-308`) and bodies remain canonical write operations. The standards/maintainer reviewers own complete cumulative documentation certification; no system failure is inferred from wording. |

## Review Findings

### Critical

None.

### Important

None. No changed reachable system failure, availability regression or unsafe recovery path was established.

### Suggestions

None; no optional hardening or deferred engine/sandbox work is requested.

## Open Questions

None affecting this task-scoped system-resilience verdict.

## Verification Notes

Reviewed fresh candidate `library-check.log`: 869 wyrd-spec tests passed, zero skipped, including latest Verdict declaration, cyclic input/output schemas, and existing DAG diamond/cycle tests. `focused-check.log` records the exact Verdict regression selection passing. Reviewed prior R1 `library-check.log`: 1139 tests passed, including unchanged runtime tool-scope/accepted-authority and gateway intersection coverage. R2 changes only the walker, its existing test and two formatter docs; no rationale exists to duplicate the unchanged runtime lane in this reviewer. Clippy/codegen logs are available to the orchestrator; this report does not treat them as outage or cancellation proof.

No tests or builds were run independently by this reviewer, and no source edits were made. Proof is source inspection plus the identified immutable-candidate and unchanged-owner logs, not a fresh live deployment experiment.

Skipped: live provider/server/SQL/sandbox crash, cancellation and restart runs, plus SDK journeys, as those runtime implementations remain outside this staged task. Risk: this PASS does not establish future execution isolation, cancellation cleanup, durable replay or final integrated journey behavior; those require their owning tasks' proof.
