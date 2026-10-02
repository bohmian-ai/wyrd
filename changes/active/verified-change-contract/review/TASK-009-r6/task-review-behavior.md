# TASK-009 round-six behavior review

## Proposed findings

None. No `MISSING`, `INCORRECT`, `DRIFT`, `VIOLATION`, or `REGRESSION`
finding survived caller-to-result tracing against specification revision 46.

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `1a4bbff5a26a1462d0f509c4595d52d08fbd25ae`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 46
- Original task: `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md`
- Remediations: TASK-009-R1, TASK-009-R2, TASK-009-R3, human-approved
  TASK-009-R4E (superseding TASK-009-R4), and TASK-009-R5
- Review scope: complete cumulative `base..candidate` range

`HEAD` was the candidate before source inspection, after focused verification,
and before this report was written. The repository has no `.codegraph/`
directory, so direct repository search and source inspection were used.

## Navigation map and caller paths

| Producer / entry | Owner and path | Consumers / observable result |
|---|---|---|
| Root or initially selected Run | `crates/shared/wyrd-client/src/state.rs` `WyrdState::run` / `run_for_card` | Resolves the exact hydrated Card before `Run::new` mints one invocation ID |
| Immutable Card view | `crates/shared/wyrd-client/src/observe/mod.rs` `Run::new`, `for_card`, `correlation` | Every Drift, Eval, and generic row receives that view's exact CardRef and the shared RunId |
| Python public Run | `sdks/wyrd-sdk-python/src/state/mod.rs`, `src/observe/mod.rs` | `state.run(card=...)`; synchronous entry/exit delegates only optional OTel work and returns/propagates as specified |
| Execution-local scope | `sdks/wyrd-sdk-python/python/wyrd/otel.py` `_enter_run` / `_exit_run` | Import-time private OTel key holds the tuple stack; matching exit restores the preceding pair |
| Provider installation | `sdks/wyrd-sdk-python/python/wyrd/otel.py` `install_run_correlation` / `_RunCorrelationProcessor` | Weak identity lookup gives each provider one terminal outcome; only normally accepted per-attempt processors become active |
| Rust and TypeScript projections | `sdks/wyrd-sdk-rust/tests/observe_run.rs`; `sdks/wyrd-sdk-ts/native/src/cards.rs`; `sdks/wyrd-sdk-ts/wyrd/src/index.ts` | Root default, initial Card selection, same-run sibling views, and local unknown-alias refusal |
| Focused Python proof | `sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py` | Active/child spans, nesting, async isolation, same-Run concurrency, fail-open cases, provider identity, and failed retained processor behavior |
| Persisted Python journey | `sdks/wyrd-sdk-python/tests/integration/state/test_observe_journey.py` | Authenticated stock OTLP export and persisted trace/custom/Eval joins after explicit durability barriers |

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-123: a root-default Run is local and mints one UUIDv7 invocation | `WyrdState::run` supplies `root_ref` to `Run::new`; only `Run::new` creates `RunId` | Shared Run tests and Rust/Python/TypeScript journey assertions recorded green | PASS |
| REQ-123: Python `run(card=...)`, TypeScript `run(card?)`, and Rust `run_for_card` select an exact hydrated Card before minting the invocation | Shared `WyrdState::run_for_card` resolves `card_ref(alias)?` before `Run::new`; both foreign boundaries delegate | Shared initial-selection/refusal tests plus language journey cases | PASS |
| REQ-123: later Card views are immutable siblings sharing one RunId and cannot retarget their parent | `Run::for_card` clones state and RunId and resolves a new subject without mutation | Shared, Python, Rust, and TypeScript subject/identity assertions | PASS |
| REQ-123: unknown or out-of-graph aliases fail locally without transport, authentication, or server Run creation | Alias lookup precedes `Run::new` and all observation work | Shared and language negative cases assert `WYRD_SDK_404_UNKNOWN_ALIAS` | PASS |
| REQ-123: the approved Rust, Python, and TypeScript public Run surfaces and declarations agree | Shared owner plus thin PyO3/N-API projections; Python and TypeScript declarations match the runtime signatures | Recorded typecheck/codegen/N-API evidence and Python public signature test | PASS |
| REQ-151: Python Run is a synchronous context manager; entry returns the same Run and exit returns `False` without masking application exceptions | `PyRun::__enter__` returns `slf`; `__exit__` accepts the conventional optional triple, delegates cleanup, and always returns false | Entry/active-span case, signature/default case, and user-exception cases | PASS |
| REQ-151 revision 46: one import-time private OTel context key stores a tuple stack of `(card_ref, run_id)` pairs, with no detach token or Run-owned scope state | `_SCOPE_KEY` is created at import; `_enter_run`/`_exit_run` use OTel context get/set/attach; `PyRun` stores only native `Run` | Nested, mismatched-exit, awaited, copied-task, and concurrent same-Run cases | PASS |
| REQ-151: entry pushes the exact pair, a matching exit pops it, and empty, mismatched, or failed exit changes nothing and never raises | `_enter_run` appends once; `_exit_run` compares the top pair before attaching the shortened tuple; optional failures are contained | Nested restoration, mismatched-exit, context-update-failure, and exception propagation tests | PASS |
| REQ-151: the already-active recording span is stamped only when it does not already carry `wyrd.card_ref` | `_enter_run` gates both writes through `_carries_card_ref` | Active-span and nested-no-overwrite cases | PASS |
| REQ-151: child spans receive the innermost pair and scoped values replace conflicting initial Wyrd values | Active `_RunCorrelationProcessor::on_start` reads the supplied parent context and writes both exact attributes | Child/grandchild, nested Card, private-provider, and persisted framework-span cases | PASS |
| REQ-151 / AC-032: correlation survives `await` and copied task context and remains isolated across concurrent tasks, including two tasks entering the same immutable Run | Scope state exists only in OTel's execution-local context; no mutable scope slot exists on `Run` | `test_scope_survives_await_and_isolates_concurrent_tasks` and coordinated `test_concurrent_tasks_entering_the_same_run_exit_independently` | PASS |
| REQ-151: registration is thread-safe, weak-lifetime, idempotent, and attempted at most once per provider identity | `_outcomes_lock`; `_outcome_entry` prunes dead weakrefs and compares live referents with `is`; terminal `False` is recorded before the foreign call | Healthy global/private, same-provider failure, accept-then-raise, and distinct-equal-provider cases | PASS |
| REQ-151 revision 46: failed registration, including accept-then-raise, gives no enrichment and is never retried | Each attempt owns an inactive processor; it becomes active only after `add_span_processor` returns normally; cached failure remains false | `test_a_provider_that_raises_after_accepting_is_never_asked_again` and `test_a_processor_retained_by_a_failed_registration_never_enriches` | PASS |
| REQ-151 / AC-032: missing OTel, API-only provider, registration failure, entry/update failure, processor failure, and exit-update failure never escape or block explicit Wyrd observations | Optional import and installation, entry, processor, and exit paths contain telemetry exceptions; Card/observation errors remain outside that containment | Every named failure class reaches the representative strict Drift boundary; user exceptions propagate | PASS |
| INV-007 / REQ-151: client injects only CardRef and RunId; tenant, principal, Card UID, request identity, authorization, and write validation remain server-owned and strict | Correlation code writes only `_CARD_REF` and `_RUN_ID`; shared observation and server paths are reused | Persisted journey asserts authenticated publisher and server-resolved Card UID; unknown alias and observation refusals remain visible | PASS |
| INV-012: existing observation and Bifrost semantics are reused rather than replaced | Shared Run correlation feeds the existing Drift/Eval/generic projections and state-owned Bifrost facade | Existing scoped-observation journey is extended rather than replaced; no parallel pipeline appears | PASS |
| AC-032: stock Python OTel SDK and OTLP/HTTP spans exported to authenticated `/v1/traces` join custom and Eval evidence by the required identities | `otlp_provider`, `emit_framework_scope`, and `assert_scope_joins` use a registered Service, private-provider hook, active tool span, and existing public SDK/server path | Recorded Postgres-backed journey proves exact run/Card, publisher, Card UID, custom-row RunId join, and Eval trace/span join | PASS |
| AC-032: context exit is not a flush, shutdown, exporter lifecycle operation, network call, server Run, or durability acknowledgement | `PyRun::__exit__` delegates only local context update; the journey performs tracer flush, state shutdown, and server publication separately after exit | Exit source inspection and persisted journey ordering | PASS |
| Non-goals: no mandatory OTel dependency, second pipeline/exporter/queue, wrapper span, process-global Card scope, ambient log/metric promise, client-authored managed identity, or server Run | OTel remains optional and duck-typed; the existing provider and Bifrost owners are reused; processor creates no span | Dependency and cumulative diff inspection | PASS |
| Approved R4E residual is not expanded into an unapproved token/detach recovery design | Candidate implements the revision-46 attach-only tuple stack and preserves the explicitly accepted identical-pair entry-failure ceiling | R4E non-goal and revision-46 authority | PASS |
| R5 proof/artifact closure remains present | Run authority states revision 46; TASK-009-R4 is `superseded`; original task records three exact Rust commands | Current artifacts and immutable implementation evidence | PASS |

## Prior-finding closure hypotheses

| Stable finding | Behavior review result |
|---|---|
| `FIND-TASK-009-1` | CLOSED — each specified optional OTel failure class leaves explicit observation behavior unchanged. |
| `FIND-TASK-009-2` | CLOSED — Run subject documentation covers root, initial, and sibling-selected views. |
| `FIND-TASK-009-3` | SUPERSEDED/CLOSED — revision 46 replaced token/detach recovery with the approved tuple-stack contract. |
| `FIND-TASK-009-4` | SUPERSEDED/CLOSED — the private context key is created once at import. |
| `FIND-TASK-009-5` | CLOSED — PyO3 exit names/defaults agree with public stubs and conventional calls. |
| `FIND-TASK-009-6` | CLOSED — both named shared Rust tests document their panic conditions. |
| `FIND-TASK-009-7` | CLOSED — coordinated tasks entering the identical Run isolate and exit independently. |
| `FIND-TASK-009-8` | SUPERSEDED/CLOSED under revision 46 — one terminal outcome is recorded before registration, preventing an ambiguous retry. |
| `FIND-TASK-009-9` | CLOSED — outcomes are weakly keyed by live referent identity, so distinct equal providers do not alias. |
| `FIND-TASK-009-10` | CLOSED — a processor retained by failed registration stays inactive and cannot enrich. |
| `FIND-TASK-009-11` | CLOSED — the Run authority identifies revision 46. |
| `FIND-TASK-009-12` | CLOSED — replaced TASK-009-R4 is marked `superseded`. |
| `FIND-TASK-009-13` | CLOSED — the original task records separate exact shared tests and the fully wrapped exact Rust SDK journey command, all with zero-exit evidence. |

## Verification notes

- Independently ran the complete focused Python surface after `mise run py:setup`:
  `39 passed in 1.03s`.
- The first focused invocation overlapped a missing editable-wheel state and
  failed only because its fresh subprocess could not import `wyrd`.
  `mise run py:setup` restored the repository-owned environment, after which
  the complete file passed. This matches the diagnosed setup race already
  recorded in R4E evidence; it did not reproduce as an implementation failure.
- `git diff --check 7d96c30066425e0cde2290842d5801307843283d..1a4bbff5a26a1462d0f509c4595d52d08fbd25ae`
  passed.
- The task and remediation records contain zero-exit evidence for the exact
  shared Rust tests, the Postgres-wrapped Rust SDK journey, the authenticated
  Python journey, broader Rust/Python/TypeScript lanes, typing, codegen,
  boundary checks, formatting, and lints. This reviewer inspected those
  commands and their mapped assertions rather than rerunning every broad lane.

## Overall result

**PASS**

The cumulative candidate satisfies TASK-009 exactly under approved
specification revision 46. All prior behavior findings are closed or
superseded by the approved token-free design, the independently exercised
focused behavior is green, and no unrelated executable behavior entered the
task's implementation scope.
