# Independent system-resilience review

**Result: PASS.** No material system-resilience finding was established in the complete cumulative change. This is a bounded contract/staging assessment, not approval of the deferred execution runtime.

## Immutable subject and authority

- Original base: `08b38f74255d344e1c5ec9a6d278935a26277622`.
- Candidate: `eb1681461897ee1eb9fe1fa4e00cb0b435658559`; HEAD matched this identity during review.
- Reviewed the complete 26-file base-to-candidate diff, using the prior candidate-to-candidate diff only to locate remediation.
- Read the subject, original task/spec snapshots, prior verdict, validated ledger and R1 remediation. Applied AGENTS.md, agent rules, spec-driven-development, maintainer-style and the relevant Verifier, accepted-authority, audit and execution ownership rules in Wyrd design/doctrine. CodeGraph was the first source navigation mechanism; source/caller searches expanded its results.
- Current task/owner instructions assign eventual judge removal to Task 005. No finding requires retaining or deleting it in this contract task.

## Deployed paths and boundaries

| Changed surface | Producer, callers and deployed effect | Failure ownership |
| --- | --- | --- |
| Staged authoring and execution messages | `card/blocks/mod.rs`, `verification/execution.rs`, and their public module exports are new pure values. `envelope.rs:229-241` still uses the existing `card::verifier::VerifierSpec`; `card/verifier.rs:261-265` still dispatches the existing implementations. `refs/mod.rs:328-354` retains that live visitor and adds a separately typed staged visitor at :356. Searches of the cumulative source found no production caller constructing the new execution messages or invoking staged block validation through server routes. | No new accepted work, DB write, outbox stage, background task, endpoint, restart state, health check or deployment setting. A malformed pure value cannot activate an alternate server execution path. |
| Injected Vala block seam | `vala-eval/src/blocks.rs:68-155` defines call values, deadline, result/error shapes and object-safe action/check traits. The only new executor implementation is test `Echo` at :174; no production caller invokes either new trait. Existing `executor.rs` continues to own live Eval stage execution and its JoinSet/error behavior. | Deadline/cancellation and release-on-drop are stated at the seam; there is no new worker/resource owner to crash or cancel here. Retryable/Terminal are values for the later engine. It would be incorrect to demand sandbox, queue, worker restart or retry execution in this task. |
| Shared permission handling | New `BuiltinTool` at `card/agent.rs:61`, `PermissionScope::Tool` at `auth/permission_scope.rs:55`, and `Resource::Tool` at `wyrd-runtime/src/permission.rs:154` enter the existing production permission codec. Stored roles decode through `wyrd-auth/src/issuance.rs:797-807`; delegation attenuates through :236 and existing `PermissionSet::intersection` at runtime permission.rs:989. Accepted work serializes the existing `AcceptedAuthority` at request_context.rs:78-108. | Invalid names/scoped combinations refuse during decoding through `PermissionWire` and `Permission::validate` (:341-376). Refusal is local to decoding/issuance; no panic, process termination or new shared-service failure is added. All/exact coverage and intersection use existing owners; no default tool grant or fallback grant is introduced. |
| Gateway batch discovery | The sole server diff adds Tool to the ignored non-gateway scopes in `gateway/batches.rs:188-192`. `invocable` at :169 already filters resources/actions before interpreting scope. `GatewayInvocation::decide` at invocation.rs:706-755 remains authoritative, including granted-deployment object scope and non-blocking audit staging. | Tool grants cannot make batch discovery or invocation reachable as gateway grants. Existing gateway/provider/DB outage, timeout, cancellation and accounting behavior is untouched. No new retry loop or health coupling is added. |
| Shared DAG planner | Existing `validate_dag` at `vala/eval/plan.rs:110-111` delegates to generalized `plan_stages` at :125. Existing `EvalSpec::{new,validate,execution_plan}` callers at spec.rs:99,127,158 retain their unique-ID BTreeMap input. New block validation rejects duplicate IDs before planning (`blocks/validate.rs:93-105,145-175,221`). | The existing missing-dependency/self-loop/cycle errors, stage ordering and cycle-witness recovery remain in the existing owner. The helper's unique-ID precondition is satisfied by both current callers; no new task runner or runtime dependency is added. |
| Shared comparison validation | `ComparisonOperator::validate` at operator.rs:593 moves the existing regex check behind its existing operator owner; `spec.rs:188-195` dispatches exactly the same four existing Eval task variants. New block checks use it at blocks/validate.rs:826-835. | Bad regex remains a validation error before live task execution, not a worker error, panic or failed subject verdict. Regex size/compilation bounds remain intact. |
| Media, IDs, errors and provenance | `MediaFormat` is additive; the `Prompt::media_kind` accessor at skald-spec/prompt.rs:467 only exposes retained metadata to staged validation. ParameterName ordering is additive. `message_details` only changes crate visibility. Tool provenance uses one shared record at execution.rs:314-338, consumed by both ActionOutput and StepResult. | No new file reader, fetch, provider request, durable writer or error side effect. Error-only calls can honestly omit a digest and report zero bytes. Full output retention/projector behavior remains deferred. |

## Credible failure and recovery assessment

1. **Cyclic authored schemas:** the original local-ref/combinator recursion is an actual pure API risk, although registration activation is staged. `schema_path.rs:15-16,41-69` now places one finite 1024-unit budget in the walker and spends it on recursion and ref hops. `admits` (:73-117) shares it across branches and returns unknown/admitted when analysis is exhausted; `fits` (:122-176) shares it and refuses compatibility when exhausted. `values` (:222-245) uses the same budget. Thus self/mutual combinator references cannot indefinitely consume the calling thread or grow the stack without a finite bound. `validate_reference` uses the same owner for input and Agent-output schemas. The focused public-validation regression covers both paths plus finite recursive known-missing fields. No aborting reproduction was attempted.
2. **Malformed/scoped tool authority after decode or restart:** the closed enum rejects arbitrary executable names. Permission decoding validates resource/action/scope combination before creating an effective grant. `PermissionScope::covers` at :155-162 treats different tool names as disjoint; a tool scope covers no sibling domain. Existing intersection retains the narrower scope and validates its operation axes. `AcceptedAuthority::new` strips credentials but preserves effective permissions and delegation history; serde round-trip preserves exact tool authority. Persisted authority does not become All or a SYSTEM privilege during rehydration. No separate permission store, retry mechanism or recovery service is introduced.
3. **Existing gateway request during dependency outage or shutdown:** the candidate does not alter `GatewayInvocation` or batch execution. `invocable` remains pure listing pruning; actual authorization and audit remain at `decide`. Existing open/granted behavior and conditional gateway intersection are unaffected by the added disjoint Tool scope. A tool-grant decode failure refuses that request rather than crashing the shared gateway/server. There is no new fault amplification or retry fan-out.
4. **Live Eval malformed graph or operator:** the existing planner/operator owners still return their existing typed errors; no mutation or external side effect precedes these synchronous checks. Existing unique-ID inputs preserve planner behavior. Recovery remains correction and resubmission of the authored spec, with unrelated server capabilities available. The candidate changes neither in-flight task ownership nor live Eval cancellation.
5. **Future block call cancellation, provider outage or worker crash:** contracts distinguish terminal versus retryable failures, resolved source slots, positions and engine-owned deadlines. There is no production new-block invocation or worker implementation to assess. An operational availability/cancellation proof here would require building another task's runtime and would exceed approved scope. No current system guarantee is inferred from the Echo happy-path test.

The candidate does not change migrations, persistent queues, claims/leases, DB transaction owners, result/audit outbox writers, worker pools, provider retries, process shutdown or health checks. Consequently process crash/outage/replacement recovery of those deployed owners has no new state transition to audit in this task. Additive enum acceptance does not establish mixed-version rollout compatibility; the approved packet supplies no compatibility protocol and explicitly stages activation later.

## Prior findings: source-backed closure within this role

| Prior ID | System-relevant closure evidence |
| --- | --- |
| FIND-TASK-001-1 | Exact/global tool scope/resource represented and strictly decoded; same coverage/intersection and accepted-authority owner retained. Fresh permission and accepted-authority tests passed. |
| FIND-TASK-001-2 | `BuiltinTool::ALL` has all ten approved names; `validate_agent` at validate.rs:536-568 resolves through the shared catalog, retaining check-position and workspace refusals. No permissive unknown fallback. |
| FIND-TASK-001-3 | Check-position guard at validate.rs:579-593 uses bounded structural `fits` against derived Verdict schema. It refuses unverifiable cyclic compatibility instead of promising a runtime-valid verdict; it does not constrain arbitrary action output. Complete schema/contract acceptance remains the implementation/domain reviewers' scope. |
| FIND-TASK-001-4 | One existing `ToolCallRecord` now carries typed tool, optional Sha256Hex and returned byte count; no new writer or raw output persistence. ActionOutput and StepResult share it. |
| FIND-TASK-001-5 | Complete shared schema traversal, not only one caller, is bounded; fresh cyclic input/output public validation test passed. |
| FIND-TASK-001-6 | Documentation of cancellation/error ownership is present in blocks.rs and of budget-exhaustion behavior in schema_path.rs. Full changed-item rustdoc compliance belongs to the independent repository/maintainer reviews; this role does not substitute for their check. |

## Proposed findings

None. No source-local finding IDs assigned. No optional hardening, future runtime implementation or speculative availability requirement is proposed.

## Verification notes

Fresh exact focused commands, all passed:

```sh
mise exec -- cargo nextest run --locked -p wyrd-spec -p wyrd-runtime --lib -E 'test(=card::blocks::validate::tests::verifier_spec_bounds_cyclic_schema_references) | test(=permission::tests::tool_invoke_is_exact_name_scoped_and_independent) | test(=request_context::tests::accepted_authority_keeps_exact_tool_scope)'
# 3 selected, 3 passed; 932 skipped.

mise exec -- cargo nextest run --locked -p wyrd-spec -p wyrd-runtime --lib -E 'test(=vala::eval::dag_cycle_tests::triangle_cycle_returns_witness) | test(=vala::eval::spec_tests::validate_catches_invalid_regex_pattern) | test(=permission::tests::gateway_grants_survive_merge_and_intersect_exactly) | test(=permission::tests::intersection_keeps_only_the_narrower_shared_authority)'
# 4 selected, 4 passed; 931 skipped.
```

The R1 evidence records 1139 owner library tests, Clippy, workspace all-features compile, fmt, dependency and codegen checks; those broad claims were not rerun by this reviewer. Focused tests establish bounded completion and authority regression behavior, not server restart or sandbox safety.

Skipped: live-provider, Postgres/server integration, SDK journeys and fault-injection runs, consistent with this task's staged boundary. Remaining risk: later activation must prove its own runtime cancellation, isolation and recovery; this PASS supplies no such operational proof.
