# Independent TASK-001 findings validation

Recommendation: **FIX_REQUIRED**. This is the first cumulative task review, not a remediation round. No prior acceptance evidence or finding IDs are carried forward.

## Subject and method

Base: `5234961d51798581a026834cfffd5efaadc226b4`. Candidate: `0e51af2a2332f362ab44b7c5b88b51c3cb4f8011`. I inspected the committed cumulative diff and the source/callers at that candidate. HEAD and the two governing input hashes match `scope.md`: approved revision-2 spec `cc1b3c37d28201f3207769901776cc34b6a73c2810e9c42c53b50c2033cd4055`; original task including appended evidence `494bca6fa15325ace56ffe116e45d7d4f745361c56567e5b8351af6cb9fb1521`. The untracked change packet is accepted governing input. No tracked source was modified.

I read the complete task-review skill, scope/comparison/verification, all eight selected discovery reports, the focused follow-up, specification and original task. Governing source includes root and applicable directory rules, spec-driven development, maintainer/Rust/errors/agent-surface guidance, Verifier and accepted-authority sections, and relevant testing guidance. CodeGraph is absent. No Cargo, source mutation, subagents, synthetic load, or new test execution was performed by this validator. Independent runs in `verification.md` are distinguished below from static evidence and implementation-reported checks.

The traced map is declaration → ToolSession::resolve → one shared Dispatch → accepted hosted Invocation → existing capability owner; Sandbox drives the existing Monty checkout and child futures. Engine-owned ToolCalls supplies produced step/check results across attempts; StoredOutput and InputResolver preserve those records. Correction bodies inspected include complete Dispatch/ToolBridge/Halt/RecordedTool, ToolCalls, AgentExecutor::invoke and action/check handling, engine attempt/check settlement, hosted Invocation::call/code/Reservation, Sandbox::exec/drive/request/answer/conversion, Workspace::extract/read/writer/mounts/room, provider FIFO/result-loop behavior, and the cited fixture/helper bodies.

## Specialist applicability and coverage

All selected reports exist. The selection is justified independently by the actual behavior, not simply changed paths.

| Required role | Material boundary and coverage assessment |
| --- | --- |
| behavior-rev | Complete orchestration, four task scenarios, public result behavior and non-goals. Full acceptance coverage is present; failed rows below supersede its overly broad passing subclaims. |
| invariant-rev | Retry-spanning call/time state, records, permits, cancellation and cleanup. Required because these owners cross attempts and future lifetimes; report traces producers through engine settlement. |
| repo-rev | Pure contract, Vala, server, Rust SDK, generated TS/docs and changed test helpers. Broad cross-tier rules and mandatory rustdoc coverage are present. No Python/UI/SQL migration authoring is changed. |
| maintainer-rev | New shared dispatch, bridge injection and lifecycle/record owners. Existing whole workflows, not just leaf calls, are compared. The CodeTool finding was withdrawn after source reassessment; see the amended maintainer report. |
| system-rev | Shared server versus worker failure boundaries, Busy, shutdown, queue deferral and recovery. Required; it traces actual runner/engine consumers and does not demand process-wide failure. |
| domain-security-rev | Accepted caller, tenancy/delegation, per-call permission/audit, declared callable and parent/worker trust. Required and adequately scoped; no independently supported security bypass was proposed in that report. |
| domain-concurrency-rev | Shared cap, permit waiters, request spending, workspace writers and blocking lifetime. Required separately from healthy Agent behavior; unique CON-002 was inspected directly, not discarded by vote. |
| domain-records-rev | Admitted/completed/cancelled record integrity, recipients, historical/custom-data decoding and result container/readback. Required; it traces both production and consumption. |
| followup-rev | Selected for the specific missing independent diagnoses behind REP-002. Its four diagnoses resolve that uncertainty; FOLLOW-001 receives independent source validation below. |
| focused validation | `integer-validation.md` independently validates the new wire-range proposal raised by this validator and establishes the residual unpublished-future delivery path. `proof-validation.md` independently assesses the bounded missing-proof questions; the fresh `capacity-validation.md` separately confirms its newly discovered downstream-capacity claim. No validator retained its own new blocker without that separate validation. |

No applicable specialist was omitted. No external standard/provider protocol was changed; installed-library source is the relevant mechanism evidence. TASK-002 authored Python/TypeScript/MCP journeys are excluded here; mechanical contract parity/buildability remains included. The tested real Rust journey supplies the required shared public-client/worker boundary, not a substitute for TASK-002 completion.

## Discovery dispositions

| Every proposed source ID | Independent disposition | Final boundary |
| --- | --- | --- |
| BEH-001, INVREV-001, SYS-001 | CONFIRMED; one code-execution failure propagation cause | FIND-TASK-001-1 |
| BEH-002, INVREV-002, SYS-002, CON-001 | REVISED to preserve explicitly allowed bounded extraction completion/partial scratch during awaited cleanup | FIND-TASK-001-2 |
| INVREV-004, SYS-003, CON-003 | CONFIRMED; initial workspace coordination is outside deadline enforcement | FIND-TASK-001-3 |
| INVREV-003, DATA-002, CON-004 | CONFIRMED; admission is earlier than record allocation | FIND-TASK-001-4 |
| BEH-003, DATA-001 | REVISED; source failure confirmed, plus separately validated supported unpublished-future path establishes delivery correction remains necessary after range repair | FIND-TASK-001-5 |
| CON-002 | CONFIRMED; already-spent waiter can invoke after a synchronously raised halt | FIND-TASK-001-6 |
| MNT-001 | REJECTED after source reassessment: production-only implementation counting was not stated by the rule, and callback replacement had no demonstrated benefit | FIND-TASK-001-7 withdrawn; ID reserved, not reassigned |
| REP-001 | CONFIRMED mandatory changed-item rustdoc gate, including private/test helpers | FIND-TASK-001-8 |
| REP-002 | REJECTED as remaining remediation work: the actual present-day focused diagnostic report now supplies the missing independent evidence | Closed by followup-review.md; no FIND ID |
| FOLLOW-001 | CONFIRMED bounded required-proof defect; final recovery invocation consumes the wrong FIFO response | FIND-TASK-001-9 |
| New wire-range proposal, independently validated in integer-validation.md | CONFIRMED; preserving the signed/unsigned JSON wire range is independently required | FIND-TASK-001-10 |
| New downstream QueueFull refinement from proof-validation.md, independently confirmed by capacity-validation.md | CONFIRMED; exact process-capacity refusal misses existing Busy observer | FIND-TASK-001-11 |

REP-002 closure does not assert an implementation-time report existed. The skill requires a recorded diagnosis and diagnostician report; it does not require a new implementation round merely to attach one. Follow-up independently traces immediate Busy versus an actually pending sibling, Tokio pin reference versus underlying owner, ordered MockProvider responses, and fs.read's multi-piece result. The yield still leaves the drop-flag assertion intact; the scope actually drops the future; consuming the earlier leftover response preserves the earlier refusal assertion; starts 0/1 still distinguish each returned future. Historical pre-fix logs remain unavailable. FOLLOW-001 is the concrete remaining FIFO proof failure, not a second paperwork finding.

## Final deduplicated ledger

### FIND-TASK-001-1 — Generated execution failure loses invocation-failure semantics

**Sources/status/classification:** BEH-001, INVREV-001, SYS-001; CONFIRMED; INCORRECT. **Obligations:** REQ-009/012, AC-005/009/018; TASK-001 ordinary uncaught failure and host-stop contracts.

**Exact locations and trace:** hosted `tools.rs:302–334` maps Sandbox failures other than Busy into cause-less tool errors; a spent Reservation at `:304–305` does likewise. `sandbox.rs:389–395` produces terminal timeout/program failure. Vala `blocks/tools.rs:409–420` observes only a typed Busy; Skald `loop_runtime.rs:632–711` appends remaining errors and permits another model turn. AgentExecutor `blocks/agent.rs:410–432` therefore accepts the later successful model answer. Engine action/check settlement then correctly consumes the wrong successful executor result. The same mapping covers an uncaught Python exception from a denied child and ordinary uncaught generated-program errors. Caught ordinary child errors remain inside Python and are a separate permitted path.

**Reachability/consequence/materiality:** the real Rust failure journey at `verifier_building_blocks.rs:1953–1956` asserts Succeeded while its outer record contains a time-limit error; the independent journey run passes this counterexample. Hosted `pg_hosted_artifacts.rs:2379` explicitly expects model continuation. Execution can manufacture a successful step/judgment after the approved host stop. An ordinary uncaught child error also fails only the model's tool exchange instead of failing the invocation. These are explicit verification-integrity obligations, not a preference for less repairable general tools.

**Smallest correction:** preserve the existing code-operation failure category from Invocation/Sandbox through the existing session Halt and AgentExecutor error path, including operation/cumulative time exhaustion and uncaught generated execution failure. Stop only the affected invocation; leave ordinary errors caught within Python repairable, Busy separately deferrable, and general Skald tool repair unchanged. Preserve completed and cancelled records and engine failure/inconclusive semantics. Do not route every direct capability error into a host stop or crash the server. This is a bounded adapter/stop propagation correction; downstream result rewriting or a new runtime cannot fix its producer.

**Focused closure:** C1. Correct the existing timeout-success assertions; prove an uncaught denial after an earlier completed child, timeout while a real query is held, action failure/check inconclusive, no next model/tool work, no execution-error subject failure, records retained, and fresh success. Reuse the existing worker/public fault fixtures.

### FIND-TASK-001-2 — Started blocking extraction escapes cleanup and its writer

**Sources/status/classification:** BEH-002, INVREV-002, SYS-002, CON-001; REVISED; INCORRECT/MISSING required interruption proof. **Obligations:** REQ-011–013, AC-008–009, TASK-001 blocking extraction interruption.

**Exact locations and trace:** hosted `tools.rs:263–277` holds `_writer` in the cancellable async caller and awaits a spawn_blocking closure that holds only Workspace/arguments. Vala Dispatch's permit is likewise caller-owned (`blocks/tools.rs:502–535`). Sandbox timeout/return drops its Calls and JoinHandle await. Started blocking work continues through `Workspace::extract`, `workspace.rs:675–770`, including work_room and persist_noclobber. The Arc retains directory/workspace admission, not the writer or joined settlement. Python writes and sibling extractions use the same Workspace writer; after caller drop they can overlap the still-running extraction's room-check/publication.

**Consequence/materiality:** supported interruption can report invocation cleanup while extraction still runs and holds workspace capacity; serialization and the active-operation bound can end earlier than the actual mutation. The task's Arc-only equivalence is false. Fixing FIND-1 does not close cancellation/normal return with unresolved extraction, so this is independently necessary.

**Smallest correction:** keep ownership of writer and active-operation admission attached to the actual bounded extraction until settlement; integrate joining started work into the existing hosted attempt/workspace cleanup before that cleanup is reported complete or releases its workspace. Preserve the extractor, no-overwrite, shared budget and allowed partial scratch semantics. The spec permits already-started bounded extraction to finish during cleanup; it does not require rollback or forbidding its normal publication before cleanup completes. It requires no late tool value/mutation after cleanup. Merely moving the guard or aborting a started blocking JoinHandle is insufficient. No new extraction service, durable journal, cancellation result guarantee or runtime-thread blocking is justified.

**Focused closure:** C2. Deterministically interrupt after extraction starts; observe cleanup/admission/writer retained until settlement, release work, assert no work/result after cleanup and subsequent admission. Extend the existing hosted ownership proof rather than create host load or sleeps. Include all exit causes through the same cleanup owner; do not duplicate one lifecycle mechanism per caller.

### FIND-TASK-001-3 — Initial workspace coordination is charged but not deadline-bounded

**Sources/status/classification:** INVREV-004, SYS-003, CON-003; CONFIRMED; INCORRECT. **Obligations:** REQ-012, AC-009 and TASK-001 time spent waiting.

**Exact locations and trace:** hosted `tools.rs:304–327` reserves time and computes the operation deadline before awaiting writer/mount setup, but enforcement first starts inside Sandbox::exec. A direct extraction legitimately holding the same writer can outlast a shorter code timeout while the outer invocation deadline remains later. Accounting elapsed time on Reservation drop cannot terminate this unstarted waiter. Entering checkout after expiration can even misclassify the elapsed operation as worker-readiness Busy.

**Consequence/materiality:** a supported concurrent code/extraction workload violates the explicitly included workspace-wait operation bound and occupies resources until the unrelated writer releases. Existing query-wait proof begins inside exec and does not test this path.

**Smallest correction:** enforce the already-computed operation/invocation deadline over initial workspace coordination/setup at Invocation::code, returning its host-time stop through FIND-1. Unstarted work must expire without acquiring a worker; started blocking setup must follow FIND-2's bounded settlement ownership rather than become detached. Bounded cleanup may settle started work; do not claim the work may continue ordinarily after its deadline. Keep actual capacity refusal Busy, existing reservation accounting and no new timeout setting/scheduler.

**Focused closure:** C3. Hold the existing writer deterministically, use a shorter operation timeout/later invocation deadline, prove timeout without releasing the held writer, no worker/child/model starts, time is not replenished on retry, and fresh recovery.

### FIND-TASK-001-4 — Admitted permit waiters have no cancellation record

**Sources/status/classification:** INVREV-003, DATA-002, CON-004; CONFIRMED; MISSING. **Obligations:** admitted-call definition, REQ-015–017, AC-012.

**Exact locations and trace:** ToolBridge resolves/spends at `blocks/tools.rs:588–591`; Dispatch validates object/encoded bounds at `:491–501`, then awaits capacity at `:502–511`, and only creates a slot at `:512–519`. Direct RecordedTool shares the same path. At cap 1 a second valid gathered request waits after admission; timeout/halt drops it before a slot exists. ToolCalls::settled and engine-owned result handoff preserve existing slots but cannot reconstruct that missing request.

**Consequence/materiality:** a produced failed step or inconclusive check loses a bounded, admitted, budget-spending request and its intended recipient/cancellation. This is ordinary supported concurrency under the explicitly supported small cap.

**Smallest correction:** begin the existing bounded ToolCalls record after target/argument admission and before the semaphore wait. Reuse unfinished-slot cancellation settlement. Preserve pre-admission rejection without a leaf record, spent-request accounting, admission-start order, and the active-operation permit boundary. No queue journal/new record type or per-result patch is needed. Coordinate the no-dispatch condition with FIND-6 without charging another request.

**Focused closure:** C4. Cap 1, one held operation and another validated waiter, then halt/timeout into a produced result; both records appear once in admission order, waiter has no value/digest/bytes and a safe cancellation, underlying waiter capability never invokes, earlier records survive retries. Existing failed/inconclusive projections must keep this state.

### FIND-TASK-001-5 — Nested success is recorded before value publication to Python

**Sources/status/classification:** BEH-003, DATA-001, supported residual path independently validated in `integer-validation.md` addendum; REVISED; INCORRECT. **Obligations:** REQ-015–016, AC-011–012: no delivered value means no digest, zero bytes and a safe error/cancellation.

**Exact locations and trace:** Vala `blocks/tools.rs:537–565` encodes the bounded result and marks its slot successfully finished before returning JSON. Only afterward do Sandbox `answer:489–510` and `resume_futures:361–385` convert and publish it. A supported Experiment `ParameterValue::Json` containing u64::MAX reaches cards.get through GetCardArgs::read and ordinary hydration/to_value; it currently fails conversion while retaining successful bytes/digest. FIND-10 fixes that numeric refusal but cannot alone close this finding.

The independently checked residual path uses an ordinary authorized fs.list, not a hypothetical deeply nested Card. Python obtains a host future and performs independent local/mounted work before its intended later await. `drive:435–447` polls the child while the worker continuation is pending; the child can complete successfully into parent `answered`. If Python's independent work raises or completes before ResolveFutures, `exec:252`/its error return drop `answered` without ever publishing the value. The already-finished slot survives ToolCalls::settled unchanged. Monty's supported future-then-later-await behavior and REQ-011/012's unresolved-future contract make this a supported scheduling path. No proof of a >128-depth production result was established; that speculative alternative is excluded.

**Consequence/materiality:** retained evidence claims Python received a successful bounded value when it received only an unresolved awaitable or a conversion error. This contradicts the central explicit recipient/digest contract during supported later program failure/return. Earlier successfully delivered values must still survive a later error; merely clearing every record on program failure would be equally incorrect.

**Smallest correction:** retain nested successful settlement as pending until the existing driver publishes the bounded converted value through its resumption boundary; use existing ToolCalls completion/unfinished cancellation semantics to represent conversion failure or program exit before publication. Direct records and ordinary completed child failures retain their existing path. Keep the shared result bound/digest, safe error projection and flat record owner. No worker acknowledgement protocol, durable journal, raw-result retention or requirement that Python inspect a resumed value is needed. This is the actual existing recipient-publication boundary, so finalizing there is an earned correction, not repeated capability guards.

**Focused closure:** extend C5's real bridge proof with a deterministically settled ordinary child held in the parent before ResolveFutures, then terminate the program; assert no digest/zero bytes/safe no-value failure for that child. Pair with successful await and delivered-child-before-later-error cases, whose successful digest must survive. The new assertion may use the existing driver/worker seam; it does not require another harness. C7 separately closes numeric conversion. The known u64 Card case must succeed losslessly after FIND-10 rather than be used to bless a forbidden supported-range rejection.

### FIND-TASK-001-6 — An already-spent waiter can invoke after exhaustion is refused

**Sources/status/classification:** CON-002; CONFIRMED; INCORRECT. **Obligations:** REQ-009–010, AC-006 and TASK-001 no later tool/model work after refusal.

**Exact locations and trace:** `blocks/tools.rs:453–464` checks Halt only at spending, before the permit await at `:502–511`. Execution at `:520–540` never checks it again. `sandbox.rs:456–479` evaluates malformed `bridge.refuse` synchronously while building the ready future, raising exhaustion. `drive:435–447` can then poll outstanding children repeatedly within the same outer run poll. At cap 1/budget 3, outer code spends 1, A spends 2/holds permit, B spends 3/waits, a malformed fourth request raises Halt, then ready A releases capacity and drive polls B. B invokes even though refusal already occurred. AgentExecutor's outer select cannot preempt that current inner poll. This is a supported gather/malformed-request ordering, not a numerical budget overrun.

**Consequence/materiality:** an additional protected query/read/extraction and its permission audit may start after the explicit terminal refusal. Agreement is not the evidence: the synchronous refusal, retained spent waiter and cooperative driver establish reachability even with one task thread.

**Smallest correction:** enforce existing Halt at the shared actual execution boundary after acquiring capacity and before invoking the capability; stop waiting on a raised halt as needed. This consumer guard belongs here because this owner grants active execution after a suspension. Preserve final-allowance success, first stop identity, Busy and admitted cancellation records; no second spend, outer select bias or per-capability guards.

**Focused closure:** C4. Establish held A/validated B, raise exhaustion, make A release before yielding back to outer selection; assert B never invokes, spending remains capped, B cancellation retained and no model turn follows. This complements rather than duplicates ordinary cap/final-allowance tests.

### FIND-TASK-001-8 — Changed fallible/panicking Rust items omit required rustdoc

**Sources/status/classification:** REP-001; CONFIRMED; VIOLATION. **Obligation:** mandatory root changed-item rustdoc gate and `agent_docs/rust.md:233–242`, explicitly including private/test helpers.

**Exact examples validated:** `sandbox.rs:791` to_python lacks Errors despite range/depth refusal; `blocks/tools.rs:172` ToolCalls::finish directly indexes a slot without its Panics contract. Vala test run_engine `blocks_agent.rs:1347`, bound `:1378`, query_call `:1492`, code_call `:1771`; hosted CodeAgent::call `pg_hosted_artifacts.rs:1769`; SDK leaves/tools_for `verifier_building_blocks.rs:1660/1677` contain expect/index/unreachable panic paths without required sections. Their complete bodies substantiate the omissions; no runtime bug is claimed for valid invariants.

**Smallest correction/burden:** complete the actual error/panic and invariant rustdoc in place, including the same omission in remaining newly/materially changed items in this bounded write set. Preserve valid code; no defensive runtime guards, unchanged-item documentation expansion or prose-only test. Batch with behavioral remediation so this does not become a style-only round. Static changed-item audit and formatting close it.

### FIND-TASK-001-9 — Capacity recovery test does not execute its recovery program

**Sources/status/classification:** FOLLOW-001; CONFIRMED; MISSING required verification. **Obligations:** AC-010 and TASK-001 Scenario 4 capacity recovery; maintainer tests prove the claimed observation.

**Exact locations/trace:** `pg_hosted_artifacts.rs:1176` queues final `{}` after the two-program Busy response. `:1181` correctly proves that response is still unread. `:1182` appends the catching program behind it, and `:1183–1185` invokes once expecting that program to run. MockProvider::send (`skald-runtime/src/mock.rs:137`) pops FIFO; Skald's no-tool final-answer branch accepts the front `{}`. No code/child runs and the appended program remains unread. The earlier cleanup at `:1162` fixes the same FIFO cause only for the previous phase. Existing authored-code recovery and separate fresh query recovery do not make this later assertion truthful.

**Consequence/materiality:** the specifically promised worker-capacity recovery of that Agent program is unobserved; the test passes even if it cannot run. This is a tiny correction to a required failure/recovery proof, not a request for another broad journey or runtime feature.

**Smallest correction:** align the existing provider phases so the recovery step actually consumes the catching program followed by a final answer, and assert its observed tool result/records or provider consumption. Reuse the same fixture/provider/server and existing script. No production changes, retry/sleep/timeout enlargement or new harness is needed. C6 closes it.

### FIND-TASK-001-10 — Valid unsigned JSON integers cannot cross the bridge

**Source/status/classification:** new validator proposal independently confirmed by the fresh reviewer in `integer-validation.md`; CONFIRMED; INCORRECT. **Obligations:** REQ-002/004/005 and TASK-001 pure conversion: preserve integers in the existing wire range in both directions.

**Exact locations/trace:** Sandbox Estimate::json `sandbox.rs:722–724` accepts only signed ObjectRef::as_int; to_python `:799–803` rejects JSON unsigned integers above i64::MAX. Its unit test `:900–902` incorrectly calls u64::MAX unsupported. `ParameterValue::Json` (`wyrd-spec/src/card/common.rs:13–24`) is distinct from its signed Int variant; Experiment default_parameters accepts ordinary serde_json::Value. The locked serde_json wire representation supports negative i64 and positive u64. A real supported cards.get of a structured Experiment seed/parameter u64::MAX passes authorization/hydration/JSON result bounds, then fails only when delivered to Python. Reverse Python input also rejects a representable u64 before typed dispatch. All supported platforms are 64-bit; existing fs.read start accepts usize, corroborating the reverse-direction contract without making a huge offset the material-use rationale.

**Consequence/materiality:** the same authorized bounded Card read succeeds directly but code cannot inspect its valid structured data. This is an explicit wire-preservation requirement, so safe rejection is not an acceptable substitute. The approved range is i64::MIN through u64::MAX; this is not a new arbitrary-precision JSON promise.

**Smallest correction:** correct only the two integer arms in the existing Sandbox converter, using installed Monty BigInt/object transport for the unsigned portion and exact checked extraction for Python BigInt. Retain signed integers, finite numbers, unsupported-type/cycle/key/depth/byte refusals and interpreter limits. Reject below i64::MIN/above u64::MAX; do not round or stringify numeric leaves. Existing MontyObject::bigint, unstable BigInt node, checked numeric conversion and wire encoding already supply the mechanism; no dependency/new codec/Card schema or worker protocol change is required. Update the touched docs and wrong test expectation. This low-burden correction is independent of FIND-5's premature finalization.

**Focused closure:** C7 extends the existing pure converter proof with i64::MAX+1/u64::MAX, nested objects and exact-byte boundaries, exact both-way round trips, signed endpoints and out-of-range BigInt refusals. C5 can reuse a real Card read to assert public bridge delivery/records; no full public journey matrix is required for a numeric arm correction.

### FIND-TASK-001-11 — Oracle queue-full refusal stays catchable instead of Busy

**Source/status/classification:** new concrete refinement in `proof-validation.md`, independently confirmed by a separate fresh reviewer in `capacity-validation.md`; CONFIRMED; INCORRECT. **Obligations:** REQ-006/009, AC-010/018, TASK-001 downstream hosted capacity and queued deferral.

**Exact locations and producer→consumer:** Oracle `admission.rs:913–925` returns typed BifrostError::QueryQueueFull when every bounded pod waiting place is occupied, before granting a place/framing. `wyrd-spec/src/vala/error.rs:82–94` and Bifrost authority distinguish this retryable process overload from admitted QueryResourcesExhausted. Real Oracle SQL admission reaches it; query service and BoundedQuery stream-open (`query/collect.rs:232–266`) propagate the owner error. Hosted `tools.rs:241–253` projects it with tool_error into StructuredInvocation; Vala `blocks/tools.rs:409–420` ignores that variant because it only recognizes a typed BlockError::Busy cause. Sandbox therefore resumes a catchable RuntimeError, and a try/except program/model can complete. The required engine Busy→Deferred→claim release/refunded attempt sink exists (`engine.rs:687/780`, `verification/runner.rs:1049–1052/694–708`); it is never selected by this producer.

**Consequence/materiality:** real downstream saturation can become a judgment and consume a queued attempt instead of immediate unavailability/deferred work. This is not a grant bypass, and it is not a demand to reclassify all query 429/503/domain errors. Existing worker/workspace Busy tests exercise a different ToolError variant, so their unchanged queue composition does not close this producer-to-stop gap.

**Smallest correction:** extend existing shared Halt::observe to recognize the exact owned structured catalog code projected from BifrostError::QueryQueueFull as the existing Busy stop, alongside the existing typed Busy cause. Preserve the StructuredInvocation problem/owner metadata and no-value record; use the concrete catalog variant's projected code, not prose or generic status. Do not reconstruct delegated Vala errors with from_code (that mapper expressly does not cover them), change Oracle fairness/admission, classify intrinsic resource ceilings/permission/query errors, add a scheduler or alter queue attempt accounting. This consumer owns the common continuation boundary for both origins, so one exact classification there is sufficient.

**Focused closure:** C8. Exercise the real catalog QueueFull projection once direct and beneath actual Python try/except, verify Busy/no further model/child work and ordinary record/settlement behavior, then fresh success. Keep an adjacent QueryResourcesExhausted or permission-denial case catchable with its original safe catalog problem. Existing real Oracle queue-full producer tests plus the exact hosted adapter fault and existing queue refund/recovery proof compose without synthetic load or a new harness.

## Focused closure commands

These are remediation proof, not evidence that missing cases already pass. New selectors are explicitly **planned** and must be confirmed/recorded when implemented. Existing targets and repository Postgres setup are known. Run only the named affected proof and the narrow formatting/lint lanes for the eventual write set; do not rerun whole journey suites as task remediation.

**C1 — existing host/public fault proof, extended for FIND-1:**

```sh
mise run build:server
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_hosted_artifacts::code_host_waits_obey_deadlines_and_cancellation)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=verifier_building_blocks::code_mode_failure_releases_session)'
```

**C2 — planned original-task extraction interruption proof, FIND-2:**

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_hosted_artifacts::code_extraction_settles_before_workspace_release)'
```

**C3 — planned initial-workspace deadline proof, FIND-3:**

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_hosted_artifacts::code_workspace_wait_obeys_operation_deadline)'
```

**C4 — planned shared admission/halt proof, FIND-4/6; existing settlement regression:**

```sh
mise exec -- cargo nextest run --locked -p vala-verify --test integration -E 'test(=blocks_agent::admitted_waiters_survive_halt_without_dispatch)'
mise exec -- cargo nextest run --locked -p vala-verify --test integration -E 'test(=blocks_agent::agent_records_survive_failed_steps_and_inconclusive_checks) | test(=blocks_agent::agent_tool_budget_spans_attempts_without_refund)'
```

**C5 — existing shared bridge/publication proof, FIND-5/10:**

```sh
mise exec -- cargo nextest run --locked -p vala-verify --test integration -E 'test(=blocks_agent::code_bridge_shares_the_recorded_path)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_hosted_artifacts::code_bridge_preserves_declared_tool_boundaries)'
```

**C6 — existing capacity fixture, FIND-9:**

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_hosted_artifacts::capacity_refusal_is_busy_at_every_hosted_consumer)'
```

**C7 — existing conversion owner proof, FIND-10, plus FIND-5/10 bridge assertions under C5:**

```sh
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::hosted::sandbox::tests::code_bridge_json_conversion_is_lossless_and_bounded)'
```

**C8 — existing hosted/queue fixtures extended for exact downstream capacity, FIND-11:**

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_hosted_artifacts::capacity_refusal_is_busy_at_every_hosted_consumer) | test(=pg_hosted_artifacts::busy_queued_execution_is_deferred_until_capacity_frees)'
```

## Reconciled acceptance and evidence limits

No full task/journey suite, broad gate, platform matrix or new fault injection has been run by this validator. Green independent config/conversion/Vala/wire/public checks in `verification.md` prove their present assertions; the public timeout assertions positively contradict FIND-1. Implementation-reported fmt/lint/codegen/boundary/docs lanes are reported evidence, not new executions by this reviewer. There is no convergence-round assessment to apply on this first review.

| Requirement / AC / constraint | Reconciled implementation evidence | Verification evidence | Recommendation |
| --- | --- | --- | --- |
| REQ-001–004, AC-001–003; step/check orchestration, direct catalog, only declared dotted targets, no recursion | One resolved Dispatch, hosted protected Invocation and Monty callable; authored entrypoint passes no bridge | Independent Rust reduction journey; Vala bridge; inspected actual-worker boundary | PASS except supported conversion failure under FIND-10 |
| REQ-005, AC-003/011; exact supported JSON, bounded pre-dispatch/result conversion | Existing capped encoding/converter retains ordinary values/refusals, but unsigned range is lost | Independent converter test passes wrong u64 refusal; supported Card path and focused independent report contradict it | FAIL FIND-10; delivery consequence FIND-5 |
| REQ-006–007, INV-001/002/004; AC-004/017; accepted authority, grants, tenant, audit and Oracle | Both origins use same Invocation authorization/capability owners and statement-specific caller; no worker identity selector/new audit owner | Independent public denial/recovery; inspected recovered/delegated and existing queue authority tests; unchanged permission/audit owner source | PASS authority reuse; exact QueueFull control failure is FIND-11, not an authorization finding |
| REQ-008; caught half AC-005 | Structured safe owner problem retained, catchable repair inside Python | Existing real worker/domain denial repair and public permission journey | PASS; preserve during host-stop correction |
| REQ-009, AC-005/009/010/018; uncaught versus host stop semantics | Sandbox timeout/program error loses category; exact downstream QueueFull misses Halt; Busy/budget typed paths otherwise work | Independent passing public timeout is a source-proven counterexample; capacity-validation traces real overload producer | FAIL FIND-1/11; misleading recovery proof FIND-9 |
| REQ-010, AC-006/014; atomic retry-spanning ceiling/final allowance/no refund | Engine ToolCalls outside retry loop and atomic bounded spend are sound; post-wait stop boundary absent | Independent Vala budget tests and public budget phase; concrete cooperative-poll ordering | PASS ceiling/final allowance; FAIL no-later-work FIND-6 |
| REQ-011, AC-007; shared cap, cap1, association | All direct and nested operations hold the identical Dispatch semaphore; code holds none; call_id drives futures | Runtime peak/cap1/2 and hosted gather; static one-owner proof, not a claimed simultaneous mixed-origin execution | PASS ordinary shared cap; FAIL detached active extraction under FIND-2 |
| REQ-012, AC-009/018; wall-clock setup/waits/retries and all-exit cleanup | Reservation accounting and async query cancellation/drop are sound; initial writer/setup is unbounded and started extraction detaches | Held-query cancellation/source and worker lifecycle proof; missing extraction barrier is a concrete failing guarantee, not just absent test name | FAIL FIND-1/2/3/6; accounting itself PASS |
| REQ-013, AC-008/011; scratch, awaited visibility, no-overwrite | Same Workspace writer/room/persist_noclobber owner; normal awaits work; interrupted extraction releases guard early | Inspected real-worker mutation proof and unchanged mounted-write regression | PASS normal behavior; FAIL interruption FIND-2 |
| REQ-014, AC-001/012/016; intermediate visibility/reduction | Children bypass model conversation; outer bounded value alone is installed in loop | Independent 20-note real-client/worker comparison, matching leaf evidence, cited judgment/data, model byte assertions | PASS; no raw-result store introduced |
| REQ-015–017, INV-006, AC-005/009/012; once-only honest admitted records, failure/retry/check retention | Engine settlement preserves existing completed/unfinished slots and custom-data calls, but waiter has no slot and child finalizes before publication | Independent failed/inconclusive/retry tests; supported missing-slot and unpublished-future paths | FAIL FIND-4/5; existing result handoff otherwise PASS |
| REQ-016–017/AC-013 historical/additive/default/custom-data contracts | Closed recipient/default model, optional-empty requirement calls, decode_data transfers list; Rust export/generated TS agree | Independent wire round-trip plus real immediate/retained equality and server build | PASS task's mechanical contract scope; authored other-language/MCP delivery remains TASK-002 |
| REQ-017 privacy/no alternate argument surface | Records remain in existing outcome/StoredOutput, summary remains payload-free; no new record-bearing summary/debug surface | Records/proof reviewer source traces. Sensitive metadata is a declaration, not universal proof of elevated enforcement | PASS preservation/no-new-surface within task; no invented outcome-disable mode or unrelated access-control audit claimed |
| REQ-018, AC-001/011/016; original evidence/Verdict integrity | Existing verdict/citation/reachable pinned-evidence owners retained; scratch/live reads gain no pinning authority | Independent reduction judgment/citation/data/leaf equivalence and retained readback; existing malformed/citation seams | PASS except execution failure must not reach judgment under FIND-1/11 |
| AC-014 startup settings | Existing checked scaled parser with named setting refusal; positive defaults 128/64KiB, no Card override | Independent valid/invalid/default config test | PASS |
| INV-003/005, AC-015, non-goals and portable isolation | Existing pool/interpreter/allocator/mounts; authored code has no bridge; no dependency/feature/migration/OS isolation/new runtime | Actual-worker negative tests, source/diff and build; no platform matrix rerun claimed | PASS |
| Required governing authority correction and no unrelated changes | Changed Verifier/security paragraphs reflect approved portable mediated execution; mechanical constructors/generated outputs fit new fields | Cumulative diff and specialist coverage | PASS |
| Repository ownership/rustdoc gates | CodeTool is accepted after reassessment; listed changed items still lack required sections | Amended maintainer report and changed-item rustdoc audit | FAIL FIND-8 only |

### Proof equivalence and task-private correction

`proof-validation.md` rejects the additional capture-disabled and mixed-origin test-only blocker proposals; I agree from the actual owners. ContextCapture controls requirement actual values, not whether execution outcomes exist. There is no reachable outcome-disable selector to exercise. Do not invent one or use Redact to remove required tool records. Correct that task-private proof assumption through wyrd-plan under the same approved spec, expressing the actual existing-container/no-alternate-surface and applicable read-boundary proof. The sensitive declaration by itself is not evidence of elevated enforcement; this validation claims preservation of existing owners/containers, not a new universal authorization guarantee or an unrelated Oracle audit finding.

The literal simultaneous direct+nested runtime test is absent. The non-weaker evidence is that recipient selection occurs after exactly the same semaphore acquisition, and every direct RecordedTool and nested ToolBridge references exactly the same Arc<Dispatch>. The cap1/2 runtime proof exercises this owner; no second scheduling owner exists to multiply capacity. Do not label the existing test as a mixed-origin run, but no extra test-only blocker is justified.

Existing typed Busy plus unchanged queue deferral/refund proof composes for the worker/workspace failure type. It does not cover QueueFull's different structured projection, which is why FIND-11 is a concrete behavior finding rather than an absent-name complaint. FIND-9 remains independently necessary because its specific recovery assertion executes no recovery program.

All ten retained findings have explicit approved obligations or mandatory repository gates, concrete supported consequences and bounded existing-owner corrections. None needs a new public API, runtime, concurrency policy, persistence decision, rollback or spec revision. Rejected proposals add no implementation work. Consolidate the retained corrections into one bounded remediation task; preserve the focus commands above and batch rustdoc. Recommendation remains **FIX_REQUIRED**, not BLOCKED or SPEC_REVISION_REQUIRED.

Skipped broad suites, cross-platform execution and implementation. The material remaining risks are false post-failure judgments, detached extraction, missed/incorrect records, valid-value refusal and downstream capacity misclassification until the retained findings are corrected.

## Finding withdrawal amendment

At the maintainer's explicit request, FIND-TASK-001-7 (MNT-001) is removed
from the active ledger and remediation. The original production-only reading
of “real implementations” was stronger than the written rule, the test
implementation is substantive, and no correctness or simplification case for
replacing this cross-tier trait was established. The amended
[maintainer report](maintainer-review.md) records the source reassessment.
Keep `CodeTool`; no callback replacement is required. Its old ID remains
reserved. The ten other findings and FIX_REQUIRED verdict remain unchanged.
