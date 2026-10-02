# Structured Ponytail validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `28473e049705595306f2934cf4bc664168254086`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, revision 10
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Discovery inputs: both task reviews, standards, maintainer, system,
  concurrency, network-security, and focused follow-up reports in this
  directory

The candidate was exactly the stated commit before validation. CodeGraph is
absent. This pass read the complete cumulative diff, applicable authorities,
prior round ledger, cited source bodies, callers, sibling consumers, and
focused tests. Production source is byte-identical between round-one commit
`eb22b03f2bb766886d839bda23aafbd4ba130ab3` and this candidate; revision 10
changes only approved packet authority and adds the prior review record.

## Claim-by-claim validation

| Discovery claim | Validation | Final finding | Source-grounded resolution |
|---|---|---|---|
| BEH-R2-001 | **CONFIRMED** | FIND-TASK-001-1 | Text payloads are produced by `AttemptOutcome::from_agent`, charged by raw `String::len`, and admitted as though that were the serialized replacement delta. JSON escaping makes the retained JCS snapshot larger. |
| BEH-R2-002 | **CONFIRMED** | FIND-TASK-001-2 | The Responses loop explicitly filters every `Reasoning` item, the wire variant lacks the replay identity, and the focused test codifies the omission. |
| INVAR-R2-001 | **CONFIRMED, DUPLICATE** | FIND-TASK-001-1 | Same producer, budget owner, sink, and consequence as BEH-R2-001. |
| INVAR-R2-002 | **CONFIRMED, DUPLICATE** | FIND-TASK-001-2 | Same Responses producer-to-next-request path as BEH-R2-002. |
| INVAR-R2-003 | **CONFIRMED** | FIND-TASK-001-5 | Agent observation receives the complete provider response before Workflow applies its result-size ceiling. |
| INVAR-R2-004 | **REVISED, DEDUPLICATED** | FIND-TASK-001-6 | Panic and pending-callback failures share one missing best-effort invocation boundary with SYS-001 and CONC-001. |
| INVAR-R2-005 | **REVISED, SPLIT** | FIND-TASK-001-7, -8, -9, -12, -13 | The umbrella claim contains five independently closable repository obligations; no new aggregate finding is needed. |
| INVAR-R2-006 | **REVISED, SPLIT** | FIND-TASK-001-10, -11 | Python owner placement and public typing are distinct obligations with distinct proofs. |
| INVAR-R2-007 | **CONFIRMED, DUPLICATE** | FIND-TASK-001-13 | Same false guide statement and generated-ID producer as MNT-R2-005 and STD-007. |
| INVAR-R2-008 | **CONFIRMED** | FIND-TASK-001-14 | Parent state becomes running before spawn; the child records attempt one only when polled; abort can settle the entry as cancelled with zero attempts. |
| INVAR-R2-009 | **CONFIRMED** | FIND-TASK-001-15 | Public `u64`/`Duration` values reach unchecked `Instant + Duration` at run, step, and Agent deadline construction. |
| INVAR-R2-010 | **CONFIRMED** | FIND-TASK-001-16 | `u32::MAX` retries is accepted although the required `max_retries + 1` attempts cannot fit the public counter. |
| INVAR-R2-011 | **CONFIRMED** | FIND-TASK-001-17 | Binding insertion checks values but not names; merge forwards routing, framing, forwarding, proxy, and internal headers. |
| INVAR-R2-012 | **CONFIRMED** | FIND-TASK-001-18 | External-gateway refusal text is retained in `ProviderError::Status.body` and exposed by derived `Debug`. |
| STD-001 | **CONFIRMED** | FIND-TASK-001-7 | Five added test functions contain imports despite the mandatory test-module import owner. |
| STD-002 | **CONFIRMED** | FIND-TASK-001-8 | Changed docs and examples have no recorded required docs/example lane results. |
| STD-003 | **CONFIRMED** | FIND-TASK-001-9 | Four specifically named Rust tests have no exact recorded nextest selector. |
| STD-004 | **CONFIRMED, DUPLICATE** | FIND-TASK-001-10 | Same task-added PyO3 owner violation as MNT-R2-001. |
| STD-005 | **CONFIRMED, DUPLICATE** | FIND-TASK-001-11 | Same erased exact DTO declarations as MNT-R2-002. |
| STD-006 | **CONFIRMED, DUPLICATE** | FIND-TASK-001-12 | Same missing concrete Rust and public Python hook documentation as MNT-R2-003. |
| STD-007 | **CONFIRMED, DUPLICATE** | FIND-TASK-001-13 | Same false step-ID statement as MNT-R2-005. |
| STD-008 | **CONFIRMED, DUPLICATE** | FIND-TASK-001-17 | Same public binding-name admission path as NET-R2-001. |
| STD-009 | **CONFIRMED, DUPLICATE** | FIND-TASK-001-18 | Same external-only reflected-secret error path as NET-R2-002. |
| STD-010 | **CONFIRMED** | FIND-TASK-001-19 | The explicit base-to-candidate `git diff --check` exits 2 on the committed round-one validation report; working-tree-only checks do not inspect that range. |
| MNT-R2-001 | **CONFIRMED** | FIND-TASK-001-10 | Task-added conversion, authoring, result wrapper, and registration behavior lives in the retained migration crate while the SDK remains an aggregator. |
| MNT-R2-002 | **CONFIRMED** | FIND-TASK-001-11 | Exact Workflow step/run-error DTOs are declared as nested `Any` dictionaries. |
| MNT-R2-003 | **CONFIRMED** | FIND-TASK-001-12 | Six concrete Rust observer methods lack required item docs and three Python hooks lack signature-aligned argument contracts. |
| MNT-R2-004 | **CONFIRMED, DUPLICATE** | FIND-TASK-001-7 | Its sole retained issue is the same import-placement violation as STD-001; no test split or harness is warranted. |
| MNT-R2-005 | **CONFIRMED** | FIND-TASK-001-13 | `next_step_id` sanitizes, prefixes, synthesizes, and suffixes values that the guide calls Agent names. |
| SYS-001 | **CONFIRMED, DEDUPLICATED** | FIND-TASK-001-6 | Every Workflow callback is awaited directly; parent callbacks can unwind and step callbacks can become internal failures or delay cancellation/deadlines. |
| SYS-002 | **CONFIRMED, DUPLICATE** | FIND-TASK-001-15 | Same unchecked public deadline construction as INVAR-R2-009 and CONC-003. |
| SYS-003 | **CONFIRMED, DUPLICATE** | FIND-TASK-001-16 | Same retry-counter overflow path as INVAR-R2-010 and CONC-004. |
| SYS-004 | **CONFIRMED, DUPLICATE** | FIND-TASK-001-14 | Same spawn-before-first-poll cancellation race as INVAR-R2-008 and CONC-002. |
| CONC-001 | **CONFIRMED, DEDUPLICATED** | FIND-TASK-001-6 | Same observer failure/liveness source as SYS-001. |
| CONC-002 | **CONFIRMED, DUPLICATE** | FIND-TASK-001-14 | Same zero-attempt active-step race as SYS-004. |
| CONC-003 | **CONFIRMED, DUPLICATE** | FIND-TASK-001-15 | Same unrepresentable deadline path as SYS-002. |
| CONC-004 | **CONFIRMED, DUPLICATE** | FIND-TASK-001-16 | Same unrepresentable attempt count as SYS-003. |
| NET-R2-001 | **CONFIRMED** | FIND-TASK-001-17 | A programmatic binding may supply `Host` or another transport/routing/internal header and `merged_headers` forwards it unchanged. |
| NET-R2-002 | **CONFIRMED** | FIND-TASK-001-18 | A gateway may reflect the credential it received; shared status handling retains it in a publicly inspectable provider error. Current Workflow projection remains safe. |
| FOLLOWUP-R2-001 | **CONFIRMED — EVIDENCE-ONLY** | FIND-TASK-001-19 | The cumulative committed subject fails the task-selected check solely because the round-one report has an extra terminal blank line. This has no runtime consequence but directly falsifies required evidence. |

No discovery proposal is rejected. Duplicate and umbrella claims are collapsed
at their common producer or obligation; optional test splitting and any new
observer scheduler, serializer, error catalog, DTO runtime class, or check are
excluded.

## Prior-finding closure

| Prior finding | Round-two resolution | Evidence |
|---|---|---|
| FIND-TASK-001-1 | OPEN | Production source is unchanged; raw text length still stands in for serialized replacement cost. |
| FIND-TASK-001-2 | OPEN | Production source is unchanged; reasoning items are still explicitly filtered. |
| FIND-TASK-001-3 | **CLOSED — omit from active ledger** | Approved revision 10 fixes `ExternalGatewayBinding.secret_headers` as `HashMap<HeaderName, SecretString>` because order is unobservable and `HeaderName` has no `Ord`; `route.rs:100-112` exposes exactly that type. Direct constructors use it without an adapter. |
| FIND-TASK-001-4 | **CLOSED — omit from active ledger** | Approved revision 10 fixes `ProviderError::RemoteProblem(Box<RemoteProblem>)` over the public five-field payload; `error.rs:59-88` and all direct constructors/matches use exactly that shape. |
| FIND-TASK-001-5 through FIND-TASK-001-18 | OPEN | Each producer and consumer path is unchanged from the validated round-one candidate and was independently retraced above. |

Revision 10 resolves the only material public-contract choices. No remaining
correction requires a new product, public API, architecture, security,
compatibility, cross-service, concurrency-semantics, resource-ownership, or
persistent-data decision.

## Final deduplicated finding ledger

### FIND-TASK-001-1 — Complete-run accounting undercharges escaped text

- Discovery sources: BEH-R2-001, INVAR-R2-001
- Status: **CONFIRMED**
- Classification: `INCORRECT` (code defect)
- Violated obligation: REQ-017, REQ-045, INV-023, AC-019/020, and Scenario 6 require the complete terminal `WorkflowRun` JCS serialization to fit `max_run_bytes`.
- Exact location: `crates/skald/skald-workflow/src/attempt.rs:34-41`; `crates/skald/skald-workflow/src/run.rs:84-99,128-152,318-340`.
- Evidence and reachability: `AttemptOutcome::from_agent` produces every successful text payload; `charged_bytes` uses raw UTF-8 length; `RunLedger::step_succeeded` admits that charge; the sink serializes the value as a JSON string. Quotes, backslashes, and control characters expand, while the reserve contains `null`.
- Observable consequence: a successful returned run can have `jcs_len(run) > max_run_bytes`.
- Decision-complete minimum correction: keep admission on `RunLedger` and reuse `jcs_len` to charge the exact JCS replacement delta before committing the candidate payload. Do not add another serializer or allocator-size estimate.
- Preserved adjacent behavior: keep the existing payload-free terminal reserve, step-result ceiling, structured-output accounting, and exact 413 projection.
- Focused closure proof: extend `terminal_budget_reserve` with quote, backslash, and control-character text around the ceiling and assert bounded retention or `WYRD_WORKFLOW_413_RUN_TOO_LARGE` with the payload discarded.

### FIND-TASK-001-2 — Responses tool loops drop native reasoning continuation items

- Discovery sources: BEH-R2-002, INVAR-R2-002
- Status: **CONFIRMED**
- Classification: `INCORRECT` (code defect)
- Violated obligation: REQ-039 and Scenario 4 require the OpenAI Responses dialect to retain its native shape through the Agent tool loop.
- Exact location: `crates/skald/skald-agent/src/request_builder.rs:162-171`; `crates/skald/skald-spec/src/wire/openai_responses.rs:336-358`; `crates/skald/skald-agent/tests/loop_responses.rs:107-137`.
- Evidence and reachability: `assistant_message` filters every `Reasoning` output item before the next stateless request; the wire owner omits its replay identity; a Responses reasoning model returning reasoning before a function call reaches this path.
- Observable consequence: the next request is not the provider-native continuation and may be rejected or lose required reasoning context.
- Decision-complete minimum correction: extend the existing Responses wire item with only fields required for stateless replay and preserve reasoning items in provider order. Keep authored `previous_response_id` behavior and the single Agent loop unchanged.
- Preserved adjacent behavior: retain native message/function-call shapes for all dialects and do not introduce cross-dialect translation or another loop.
- Focused closure proof: update the existing Responses loop test so the next request contains the returned reasoning item, function call, and function-call output in native order.

### FIND-TASK-001-5 — Oversized provider output is observed before Workflow enforcement

- Discovery source: INVAR-R2-003
- Status: **CONFIRMED**
- Classification: `VIOLATION` (code defect)
- Violated obligation: REQ-017/022 and Scenario 6 prohibit oversized step data in observations as well as retained snapshots.
- Exact location: `crates/skald/skald-agent/src/loop_runtime.rs:720-746`; `crates/skald/skald-workflow/src/workflow.rs:471-480`; `crates/skald/skald-workflow/src/attempt.rs:74-123`.
- Evidence and reachability: every Workflow Agent attempt emits the complete `ProviderResponse` through `on_model_result`; only after `run_prompt` returns does Workflow normalize and enforce `max_step_result_bytes`.
- Observable consequence: a payload later rejected with `WYRD_WORKFLOW_413_STEP_RESULT_TOO_LARGE` has already crossed the configured observation boundary.
- Decision-complete minimum correction: reuse the existing task-local observer scope to enforce the Workflow ceiling before forwarding payload-bearing model-result events. Keep Agent's loop and observer API as the single pipeline; do not create parallel Workflow events.
- Preserved adjacent behavior: ordinary in-limit Agent observations, journals, provider-native execution, and the terminal 413 result remain unchanged.
- Focused closure proof: a recording observer receives no payload-bearing result for an over-limit final answer, while the step returns the exact 413 code and an in-limit answer remains observable.

### FIND-TASK-001-6 — Workflow callbacks own failure and liveness

- Discovery sources: INVAR-R2-004, SYS-001, CONC-001
- Status: **REVISED**
- Classification: `REGRESSION` (code defect)
- Violated obligation: REQ-016/019/047/048 and AC-020 require bounded terminalization; `Observer` promises best-effort callbacks whose panics do not enter run results.
- Exact location: `crates/skald/skald-workflow/src/workflow.rs:180-188,230-243,300-303,377-425`; `crates/skald/skald-observer/src/observer.rs:7-23`; `crates/skald/skald-observer/src/composite.rs:132-173`; `crates/skald/skald-observer/src/python.rs:248-339`.
- Evidence and reachability: start, binding-failure, attempt, result, backoff, and finish callbacks are directly awaited. Parent callback panics escape; step callback panics become internal step failures; pending callbacks precede or sit outside relevant selects. The Python bridge supplies a concrete blocking implementation.
- Observable consequence: adding an observer can panic, fail, or indefinitely stall an otherwise terminating Workflow, including beyond cancellation or deadline.
- Decision-complete minimum correction: route every new Workflow callback through one observer-owned best-effort invocation boundary. Isolate panics; race nonterminal callbacks against already-owned cancellation and absolute deadlines, computing the attempt deadline before attempt-start observation; terminal notification must not retain the completed return. Reuse Tokio and existing state; add no scheduler or timeout knob.
- Preserved adjacent behavior: callback ordering/data, authoritative run outcome, task ownership, deadline precedence, and ordinary observer delivery remain unchanged.
- Focused closure proof: one deterministic observer fixture panics and remains pending at start, binding failure, attempt, result/backoff, and finish; the underlying result remains authoritative, cancellation/deadline returns a complete snapshot, and no later attempt starts.

### FIND-TASK-001-7 — Added tests hide imports inside functions

- Discovery sources: STD-001, MNT-R2-004, INVAR-R2-005
- Status: **CONFIRMED**
- Classification: `VIOLATION` (structural source defect)
- Violated obligation: `architecture/agent-rules.md` requires all imports at module scope, including test-module dependencies.
- Exact location: `crates/skald/skald-workflow/src/workflow.rs:811-816,1246-1258,1473-1482,1692-1694`; `crates/skald/skald-workflow/src/workflow_surface.rs:848-850`.
- Evidence: five added tests introduce function-scoped imports.
- Observable consequence: the owning test modules no longer expose their dependency surface in one place.
- Decision-complete minimum correction: move and deduplicate only those imports in the two existing `#[cfg(test)] mod tests` import blocks. Do not split tests or add modules.
- Preserved adjacent behavior: scenario grouping and test behavior remain unchanged.
- Focused closure proof: source inspection plus `mise run fmt` and `mise run lints`.

### FIND-TASK-001-8 — Required docs and example lanes are absent

- Discovery sources: STD-002, INVAR-R2-005
- Status: **CONFIRMED**
- Classification: `VIOLATION` (evidence-only)
- Violated obligation: `AGENTS.md` requires `mise run docs:check` for docs changes and `mise run check:examples` or every touched example task for example changes.
- Exact location: cumulative changed docs/examples; completion evidence at `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md:447-466`.
- Evidence: neither required result is recorded.
- Observable consequence: the candidate does not prove that changed documentation and examples build and link.
- Decision-complete minimum correction: run the two existing lanes and record successful results; fix a red result rather than adding a check.
- Preserved adjacent behavior: retain the scoped verification strategy and add no aggregate gate or new harness.
- Focused closure proof: both existing commands exit zero on the remediation candidate.

### FIND-TASK-001-9 — Four named Rust tests lack exact recorded execution

- Discovery sources: STD-003, INVAR-R2-005
- Status: **CONFIRMED**
- Classification: `VIOLATION` (evidence-only)
- Violated obligation: `AGENTS.md` and the testing reference require an exact `mise exec -- cargo nextest run --locked` selector for every specifically named Rust test.
- Exact location: `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md:463`.
- Evidence: `agent_run_executes_openai_responses_tool_loop`, `responses_session_turns_seed_native_items`, `request::round_trip::messages_roundtrip`, and `request::untagged_dispatch::message_num_untagged_dispatch_per_provider` have only package/target shorthand.
- Observable consequence: the record can claim success without proving each named test was selected.
- Decision-complete minimum correction: use the repository-pinned toolchain and record one exact package/target/test selector for each name. Add no test.
- Preserved adjacent behavior: retain broader Skald/spec lanes and existing tests unchanged.
- Focused closure proof: each exact command selects its named test and exits zero.

### FIND-TASK-001-10 — New PyO3 Workflow behavior is outside the Python SDK owner

- Discovery sources: MNT-R2-001, STD-004, INVAR-R2-006
- Status: **CONFIRMED**
- Classification: `VIOLATION` (structural source defect)
- Violated obligation: `AGENTS.md` sections 7-8 and the PyO3 reference place new or materially relocated wrappers in `sdks/wyrd-sdk-python/src`; existing owner-crate wrappers may remain only as migration state.
- Exact location: `crates/skald/skald-workflow/src/python.rs:41-84,347-420,529-657`; aggregator at `sdks/wyrd-sdk-python/src/lib.rs:46-50`.
- Evidence and reachability: this task adds input/default/binding conversion, authoring and validation/run methods, `PyWorkflowRun`, projection, and registration to `skald-workflow`; the SDK delegates the whole surface.
- Observable consequence: the declared Python owner does not own the new public contract and the migration crate accumulates new Python-only behavior.
- Decision-complete minimum correction: leave untouched legacy wrappers in place, but move only this task's new conversions, authoring/validation/result projection, `PyWorkflowRun`, and registration into the existing SDK owner while calling Rust-native `Workflow`. Duplicate no validation or execution behavior.
- Preserved adjacent behavior: public `wyrd.agent` imports, Rust-native engine ownership, shared runtime bridge, and untouched migration wrappers remain unchanged.
- Focused closure proof: public Python tests/imports pass; retained-feature compilation, `check:pyo3-scope`, codegen, and typecheck pass.

### FIND-TASK-001-11 — Python declarations erase exact Workflow result DTOs

- Discovery sources: MNT-R2-002, STD-005, INVAR-R2-006
- Status: **CONFIRMED**
- Classification: `INCORRECT` (public declaration defect)
- Violated obligation: public Python types must describe exact domain results rather than broad `Any` when the shape is fixed.
- Exact location: `sdks/wyrd-sdk-python/python/wyrd/agent/__init__.pyi:401-439` and generated duplicate `python/wyrd/stubs/agent.pyi`; projection source at `crates/skald/skald-workflow/src/python.rs:574-645`.
- Evidence: `steps`, `error`, and `to_dict` erase `WorkflowStepResult`, `WorkflowRunError`, and the complete run shape to nested `Any` dictionaries.
- Observable consequence: field drift does not produce Python type failures and callers must infer the contract from prose.
- Decision-complete minimum correction: from the SDK-owned annotation/generator source, emit `TypedDict` projections for step result, run error, and complete run dictionary; keep only genuinely JSON-valued outputs/details broad. Add no runtime DTO classes and do not hand-edit generated stubs.
- Preserved adjacent behavior: runtime values remain ordinary wire-shaped dictionaries and `status` remains a literal union.
- Focused closure proof: regenerate cleanly and run `mise run codegen:check` plus `mise run py:typecheck` with focused exact-field usage.

### FIND-TASK-001-12 — New observer hooks lack required contract documentation

- Discovery sources: MNT-R2-003, STD-006, INVAR-R2-005
- Status: **CONFIRMED**
- Classification: `VIOLATION` (documentation source defect)
- Violated obligation: repository rules require substantive rustdoc on every new or materially modified Rust item and signature-aligned public Python docstrings.
- Exact location: `crates/skald/skald-observer/src/composite.rs:132-174`; `crates/skald/skald-observer/src/python.rs:248-339`; `sdks/wyrd-sdk-python/python/wyrd/observer.py:160-188`.
- Evidence: six concrete Rust forwarding/bridge methods have no item docs; three public Python hooks omit argument meanings and types.
- Observable consequence: fan-out, swallowed Python callback errors, payload omission, stable error-code meaning, and duration units are absent at the concrete owners.
- Decision-complete minimum correction: document the existing methods in place and add aligned Python `Args` sections. Add no helper or abstraction.
- Preserved adjacent behavior: callback signatures and behavior remain unchanged.
- Focused closure proof: source inspection, `mise run fmt`, `mise run lints`, and `mise run docs:check`.

### FIND-TASK-001-13 — Workflow guide misstates generated step IDs

- Discovery sources: MNT-R2-005, STD-007, INVAR-R2-007, INVAR-R2-005
- Status: **CONFIRMED**
- Classification: `INCORRECT` (documentation defect)
- Violated obligation: public authoring documentation must match the binding contract.
- Exact location: `docs/src/content/docs/how-to/build-a-workflow.svx:19`; producer at `crates/skald/skald-workflow/src/workflow_surface.rs:611-632`.
- Evidence: the guide says step IDs are Agent names; the producer sanitizes invalid characters, prefixes leading digits, synthesizes unnamed IDs, and suffixes collisions.
- Observable consequence: users can bind or declare dependencies with the documented name and receive unknown-step errors.
- Decision-complete minimum correction: describe IDs as deterministic values derived from names, state the transformations, and direct callers to the resulting `Workflow.steps` value when a name is not already a unique identifier.
- Preserved adjacent behavior: keep the existing ID algorithm and authoring API unchanged.
- Focused closure proof: `mise run docs:check` plus the existing builder contract test.

### FIND-TASK-001-14 — Cancellation can produce an active step with zero attempts

- Discovery sources: INVAR-R2-008, SYS-004, CONC-002
- Status: **CONFIRMED**
- Classification: `INCORRECT` (code defect)
- Violated obligation: exact snapshot invariants require interrupted active work to have at least one attempt; never-begun work is unstarted with no timestamps.
- Exact location: `crates/skald/skald-workflow/src/workflow.rs:205-228,249-285,365-381`; `crates/skald/skald-workflow/src/run.rs:120-125,166-172,211-220`.
- Evidence and reachability: the parent records running before spawn; the child increments the shared counter only on first poll; cancellation may abort first and the parent then writes cancelled with zero.
- Observable consequence: equivalent cancellation yields contract-distinct snapshots based only on Tokio poll order.
- Decision-complete minimum correction: at executor settlement, leave an aborted spawned task whose counter is zero for `RunLedger::finish` to make unstarted; call `step_cancelled` only after an attempt began. This owner sees both the join outcome and counter, so no downstream guard is needed.
- Preserved adjacent behavior: already-begun attempts remain cancelled with timestamps; pending work and ordinary failure drain remain unchanged.
- Focused closure proof: deterministically exercise cancellation after spawn but before first poll and contrast it with cancellation after attempt start.

### FIND-TASK-001-15 — Unrepresentable deadlines panic on public input

- Discovery sources: INVAR-R2-009, SYS-002, CONC-003
- Status: **CONFIRMED**
- Classification: `INCORRECT` (code defect)
- Violated obligation: public timeout input must produce stable pre-dispatch or terminal behavior, never unwind execution.
- Exact location: `crates/wyrd-spec/src/card/workflow.rs:349-368`; `crates/skald/skald-workflow/src/plan.rs:270-288`; `crates/skald/skald-workflow/src/workflow.rs:183-188,382,434-444`.
- Evidence and reachability: the contract accepts full-range values and planning converts them to `Duration`; run, step, and Agent paths use unchecked `Instant + Duration`.
- Observable consequence: validly decoded YAML or a public local option can panic rather than return a stable error or run.
- Decision-complete minimum correction: use checked deadline construction at existing validation/preparation owners. Reject unrepresentable authored timeouts through the field-specific Workflow validation path and unrepresentable local deadlines through the existing pre-dispatch result; do not clamp, saturate, or drop the bound.
- Preserved adjacent behavior: normal deadline precedence, cancellation, and representable duration semantics remain unchanged.
- Focused closure proof: maximum authored timeout and local deadline cases never dispatch or panic and return the selected stable errors; normal precedence stays green.

### FIND-TASK-001-16 — Accepted retry count cannot fit the public attempt counter

- Discovery sources: INVAR-R2-010, SYS-003, CONC-004
- Status: **CONFIRMED**
- Classification: `INCORRECT` (code defect)
- Violated obligation: REQ-016 and the exact snapshot contract define attempts as `max_retries + 1` in `u32`.
- Exact location: `crates/wyrd-spec/src/card/workflow.rs:360-368`; `crates/skald/skald-workflow/src/workflow.rs:365-427`; `crates/skald/skald-workflow/src/run.rs:331-340`.
- Evidence and reachability: `u32::MAX` is valid input, but the required final increment panics when checked or wraps when unchecked; reserve saturation cannot repair execution semantics.
- Observable consequence: accepted input can panic or fail to terminate and cannot report the promised count.
- Decision-complete minimum correction: pure `WorkflowSpec` validation rejects only `max_retries == u32::MAX` through the existing field-specific validation error. Do not add downstream saturation or widen an internal counter that still cannot project publicly.
- Preserved adjacent behavior: all other retry values, default retries, backoff, and exact attempt accounting remain unchanged.
- Focused closure proof: accept `u32::MAX - 1`, reject `u32::MAX` before dispatch with the retry field named, and retain ordinary exhaustion tests.

### FIND-TASK-001-17 — Secret bindings may override routing and transport headers

- Discovery sources: NET-R2-001, INVAR-R2-011, STD-008
- Status: **CONFIRMED**
- Classification: `INCORRECT` (security code defect)
- Violated obligation: REQ-042/049, INV-010/017, AC-016, and Scenario 5 require exact-origin binding and pre-dispatch reserved-header refusal.
- Exact location: `crates/skald/skald-workflow/src/route.rs:144-185,452-491`; Card classifier at `crates/wyrd-spec/src/card/workflow.rs:585-607`.
- Evidence and reachability: public programmatic insertion checks secret values only; merge forwards any `HeaderName`, including `Host`, framing/hop-by-hop, forwarding/proxy, and Wyrd-internal names.
- Observable consequence: a bound `Host` can select another virtual backend behind the screened TLS origin and carry credentials there; sibling names can alter framing, proxying, or correlation.
- Decision-complete minimum correction: reuse the existing route-header classification by separating its transport/routing/internal subset from the Card-only credential subset, and apply the former at binding insertion. Continue allowing binding-owned `authorization`, API-key, and token names. Do not add a second unrelated denylist.
- Preserved adjacent behavior: authored credential-header refusal, valid secret credential headers, origin/protocol checks, collision handling, and exact egress screening remain unchanged.
- Focused closure proof: insertion rejects case-insensitive Host, framing/hop-by-hop, forwarding/proxy, and internal names before dispatch, accepts ordinary credential headers, and keeps authored/collision tests green.

### FIND-TASK-001-18 — External-gateway refusal bodies retain reflected credentials

- Discovery sources: NET-R2-002, INVAR-R2-012, STD-009
- Status: **CONFIRMED**
- Classification: `INCORRECT` (security code defect)
- Violated obligation: REQ-042, INV-012, and AC-016 prohibit bound secret values in errors or logs.
- Exact location: `crates/skald/skald-providers/src/clients/external.rs:130-149`; `crates/skald/skald-providers/src/clients/mod.rs:40-75`; `crates/skald/skald-providers/src/error.rs:9-30`.
- Evidence and reachability: the external client uses shared status handling, which stores the complete bounded refusal body in `ProviderError::Status`; derived `Debug` prints it. A gateway that received the credential can reflect it. Current Workflow projection uses safe code/display data and is not the defect source.
- Observable consequence: public external-client error inspection or debug logging can disclose the credential.
- Decision-complete minimum correction: sanitize only external-gateway non-success errors at `ExternalGatewayClient`: preserve status and retry metadata, replace the refusal body with a fixed safe diagnostic, and leave native-provider behavior unchanged.
- Preserved adjacent behavior: response bounds, retry classification, successful decoding, native provider diagnostics, and safe Workflow projection remain unchanged.
- Focused closure proof: a gateway reflects a canary credential; the complete returned error's `Display` and `Debug` and Workflow projection omit it while status-based retry classification remains unchanged.

### FIND-TASK-001-19 — The cumulative candidate fails its required diff check

- Discovery sources: STD-010, FOLLOWUP-R2-001
- Status: **CONFIRMED**
- Classification: `VIOLATION` (evidence-only)
- Violated obligation: the original task selects `git diff --check`, records it as passing, and immutable task review evaluates the complete base-to-candidate range.
- Exact location: `changes/active/skald-workflow-runtime/review/TASK-001-r1/findings-validation.md:269`; stale evidence at `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md:466`.
- Evidence and reachability: `git diff --check a51af030b6039eea4b2914f3ebf2c31925d08721..28473e049705595306f2934cf4bc664168254086` exits 2 with `new blank line at EOF`; the no-argument check inspects only working-tree changes and cannot substantiate the committed range.
- Observable consequence: no runtime behavior changes, but the immutable candidate directly falsifies required clean-diff evidence.
- Decision-complete minimum correction: remove only the extra terminal blank line from the existing round-one report and record the explicit base-to-remediation-candidate check at exit zero. Add no code, test, formatter sweep, or new check.
- Preserved adjacent behavior: keep the round-one report content and all production source unchanged.
- Focused closure proof: the explicit base-to-new-candidate `git diff --check` exits zero.

## Validation result

**FIX_REQUIRED**

The active ledger contains bounded implementation, source-structure,
documentation, and evidence corrections. FIND-TASK-001-3 and
FIND-TASK-001-4 are closed exactly by approved revision 10 and are omitted from
the active ledger. The remaining corrections reuse existing owners,
mechanisms, dependencies, and checks; none requires renewed specification
authority.
