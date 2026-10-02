# TASK-009 r5 invariant review

## Immutable subject

- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `11eb8ed2b64b70903f171335723dcc549b463352`
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 46
- Original task: `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md`
- Remediation inputs: TASK-009-R1, TASK-009-R2, TASK-009-R3, and TASK-009-R4E. R4E supersedes R4.

The candidate remained at the stated commit during this review. I reviewed the
complete cumulative range and treated prior `FIND-TASK-009-1` through
`FIND-TASK-009-8` as hypotheses rather than accepted conclusions.

## Producer-to-sink trace

1. Shared Rust mints one `RunId` only after the requested initial alias has
   resolved and stores the exact hydrated `CardRef` on the immutable `Run` view
   (`crates/shared/wyrd-client/src/state.rs:489-510`,
   `crates/shared/wyrd-client/src/observe/mod.rs:42-113`). Sibling views clone
   the invocation identity while selecting their own subject; every observation
   builds per-row `Correlation` from that view.
2. Python, Rust, and TypeScript initial-Card projections delegate to this shared
   owner. Unknown aliases therefore fail before Run construction, context entry,
   or network work; language boundaries do not mint an independent identity.
3. Python `Run.__enter__` and `Run.__exit__` pass the same view-local CardRef and
   Run ID into `wyrd.otel` (`sdks/wyrd-sdk-python/src/observe/mod.rs:195-248`).
   Neither stores mutable scope state on `PyRun`.
4. `wyrd.otel` creates one import-time private OTel key and stores a tuple stack
   of `(card_ref, run_id)` pairs in that context value
   (`sdks/wyrd-sdk-python/python/wyrd/otel.py:192-203,272-313`). Entry performs
   one attach of the pushed tuple. Exit performs one attach of the popped tuple
   only when the current top equals the exiting view's pair. This preserves
   nesting, isolates copied asyncio contexts, permits the identical immutable
   Run in concurrent tasks, and makes a mismatched or failed exit a no-op.
5. The stateless processor reads the innermost pair from `parent_context`
   (`otel.py:206-236`), which covers ordinary child spans and spans started with
   a captured OTel context. The entry path separately stamps an already-active
   recording span only when it does not already carry `wyrd.card_ref`
   (`otel.py:272-297`). Provider registration uses one weak, locked terminal
   outcome per provider, marked false before the foreign call and changed to
   true only after normal return (`otel.py:239-269`). Thus success is
   idempotently true, unsupported/failing/ambiguous registration is terminal
   false, and an accept-then-raise provider receives no second processor.
6. The unchanged Python journey uses a private stock SDK provider plus the
   public install hook, exports through authenticated OTLP/HTTP, separately
   flushes the provider and Wyrd writer, then queries persisted trace, custom,
   and Eval rows (`sdks/wyrd-sdk-python/tests/integration/state/test_observe_journey.py:365-475,477-563`).
   It proves the client pair reaches server-authorized Card UID and publisher
   identity, custom rows join by Run ID, and Eval joins by exact trace/span IDs.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-123: root and initial-Card runs share one Rust-owned identity model; sibling selection is immutable | `WyrdState::run` / `run_for_card`, `Run::new` / `for_card`; thin Python and TypeScript projections | Shared Rust selection tests; Python `test_run_card_*`; Rust and TypeScript journey assertions | PASS |
| Unknown initial or sibling aliases fail locally before a Run/context/network action | Shared alias lookup precedes `Run::new`; SDK projections propagate the same stable error | Shared, Python, and TypeScript negative assertions | PASS |
| REQ-151: one import-time OTel context key; stack value contains only ordered CardRef/Run-ID pairs | `_SCOPE_KEY` and tuple-stack operations at `otel.py:195-203,272-313`; no lazy key or separate scope store | Source inspection; R4E focused-file evidence | PASS |
| Entry pushes once, exit pops once only for its exact top pair; no detach/token state exists | `_enter_run` and `_exit_run`; PyO3 passes the pair to both | `test_nested_card_scopes_share_the_run_and_restore_the_outer_card`, `test_mismatched_exit_changes_nothing`; source search in the changed owner | PASS |
| Await, copied task, distinct concurrent views, and concurrent tasks using the identical immutable Run remain isolated | Scope is entirely in OTel's context value; no state exists on `Run` or `PyRun` | `test_scope_survives_await_and_isolates_concurrent_tasks`, `test_concurrent_tasks_entering_the_same_run_exit_independently` | PASS |
| Processor reads the innermost parent-context pair and in-scope values replace conflicting initial child-span values | `_RunCorrelationProcessor.on_start` at `otel.py:213-221` | `test_entering_a_run_returns_it_and_correlates_active_and_child_spans` | PASS |
| Nested entry does not overwrite an already-correlated active span | `_carries_card_ref` gates the active-span writes at `otel.py:284-297` | `test_nested_entry_never_overwrites_an_active_span_correlation` | PASS |
| Registration is thread-safe, weak, idempotent, and attempted at most once per provider, including accept-then-raise | Locked `WeakKeyDictionary` terminal outcome cache at `otel.py:200-203,239-269` | `test_global_and_private_providers_receive_one_processor_each`, `test_a_provider_that_raises_after_accepting_is_never_asked_again`, `test_unsupported_providers_are_refused_without_raising` | PASS |
| Missing/API-only OTel, registration, attach, enrichment, and exit-update failures do not block Run use, explicit observations, or user exceptions | Optional import bindings and exception containment at every OTel boundary; strict observation and alias errors remain outside it | Focused failure cases at `test_observe_surface.py:475-588`; recorded 37-pass R4E focused file | PASS |
| Python `__exit__` signature/defaults match the declared public API and never suppress | PyO3 conventional names/defaults and unconditional false at `src/observe/mod.rs:220-248`; owning/generated stubs agree | `test_run_exit_accepts_conventional_keywords_and_omitted_arguments`; recorded typecheck/codegen results | PASS |
| AC-032: authenticated stock OTLP export persists exact Run/Card identity and joins custom and Eval evidence | Existing journey's private provider, framework spans, separate lifecycle barriers, and persisted queries | Recorded exact journey pass; `assert_scope_joins` checks publisher, resolved Card UID, Run ID, and trace/span join | PASS |
| Run exit is not a span, exporter, Bifrost, network, or durability lifecycle boundary | `__exit__` delegates only `_exit_run`; journey explicitly flushes provider, shuts down state, then flushes server publication | Journey ordering at `test_observe_journey.py:550-563` | PASS |
| INV-007: client injects no tenant, principal, Card UID, or request identity | Correlation contains only CardRef and Run ID; server journey reads authenticated/server-resolved fields | Persisted journey publisher/Card UID assertions | PASS |
| INV-012 and task non-goals: no second Run resource, queue, exporter, wrapper span, required OTel dependency, or log/metric promise | Cumulative diff retains shared observation/Bifrost path and optional `otel` dependency; processor is installed on caller-owned providers | Complete diff and manifest inspection | PASS |

## Prior-finding closure

| Finding | Closure evidence | Result |
|---|---|---|
| `FIND-TASK-009-1` | Every named optional failure reaches unchanged explicit Drift behavior; actual raising registration is exercised through Run entry. | CLOSED |
| `FIND-TASK-009-2` | `Run::subject` documents root, initially selected, and sibling-selected views accurately. | CLOSED |
| `FIND-TASK-009-3` | Revision 46 supersedes token/detach recovery with the approved context-value stack and attach-only exit. No detach failure path remains. | CLOSED BY APPROVED SPEC REVISION |
| `FIND-TASK-009-4` | The private key is created once at module import, eliminating concurrent lazy publication. | CLOSED |
| `FIND-TASK-009-5` | Runtime names/defaults and owning/generated stubs agree. | CLOSED |
| `FIND-TASK-009-6` | Both new shared Rust tests contain the required `# Panics` documentation. | CLOSED |
| `FIND-TASK-009-7` | Two coordinated tasks enter the identical Run and prove independent exit/correlation state. | CLOSED |
| `FIND-TASK-009-8` | A false outcome is stored before processor handoff and never discarded; accept-then-raise receives exactly one handoff and remains terminal false. | CLOSED |

## Proposed findings

None. I found no reachable missing, incorrect, drifting, violating, or
regressing invariant in the cumulative candidate.

The documented attach-without-detach residual in `run_api.md`—a framework that
attaches an inner context during the block and detaches it only after the Wyrd
scope exits can restore a captured in-scope context—is an explicitly approved
revision-46 decision, not a task-review finding. Raw thread propagation is also
an explicit non-goal.

## Verification notes

- Per discovery coordination, I ran no Cargo, pytest, or mise lane in parallel.
- I inspected the R4E evidence recording exact focused cases, the complete
  37-test Python surface, the Postgres-backed persisted journey, Python unit and
  integration suites, typecheck, codegen, boundary, formatting, and lint lanes
  as passing. Those recorded runs support but do not replace the source trace
  above.
- I independently ran `git diff --check` for the immutable range; it passed.
- Candidate stability was rechecked at report time.

## Overall result

**PASS**
