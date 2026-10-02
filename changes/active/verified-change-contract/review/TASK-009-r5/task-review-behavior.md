# TASK-009 round-five behavior review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `11eb8ed2b64b70903f171335723dcc549b463352`
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 46
- Original task: `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md`
- Remediations: TASK-009-R1, TASK-009-R2, TASK-009-R3, and human-approved TASK-009-R4E; R4E supersedes TASK-009-R4
- Review scope: the complete cumulative base-to-candidate range

The candidate was `HEAD` before and after source inspection. No `.codegraph/`
directory exists, so direct repository search and source inspection were used.
This discovery pass did not execute Cargo, pytest, or mise commands.

## Navigation map and caller paths

| Producer / entry | Owner and path | Consumers / result |
|---|---|---|
| Root or initially selected Run | `crates/shared/wyrd-client/src/state.rs:489-510` | `Run::new` mints one invocation ID after local alias resolution |
| Immutable Card view | `crates/shared/wyrd-client/src/observe/mod.rs:42-114` | `Run::for_card` preserves `run_id`; `correlation` supplies exact CardRef and run ID to every emit |
| Python public Run | `sdks/wyrd-sdk-python/src/state/mod.rs:177-194`, `src/observe/mod.rs:156-258` | Thin PyO3 projection; entry and exit pass the view pair to `wyrd.otel` |
| Run scope stack | `sdks/wyrd-sdk-python/python/wyrd/otel.py:192-313` | Import-time private OTel key; tuple stack; processor stamps child spans; matching exit pops by attach |
| Provider registration | `sdks/wyrd-sdk-python/python/wyrd/otel.py:200-269` | One weak cached terminal outcome per supported provider; one shared stateless processor |
| Rust and TypeScript projections | `sdks/wyrd-sdk-rust/tests/observe_run.rs`; `sdks/wyrd-sdk-ts/native/src/cards.rs:299-311`; `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1855-1866` | Root default, selected initial Card, same-run sibling views, local unknown-alias refusal |
| Focused Python proof | `sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py:215-613` | Initial selection, active/child spans, nesting, asyncio isolation, failures, provider outcomes, exit parity |
| Persisted Python journey | `sdks/wyrd-sdk-python/tests/integration/state/test_observe_journey.py:365-563` | Authenticated OTLP trace export and persisted trace/custom/Eval joins after explicit barriers |

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Root-default Run creation is local and mints one UUIDv7 invocation | `WyrdState::run` resolves the existing root and calls `Run::new`; `Run::new` alone creates the `RunId` | Shared Run unit coverage plus Python/Rust/TypeScript journey assertions recorded green | PASS |
| Python `run(card=...)`, TypeScript `run(card?)`, and Rust `run_for_card` select an initial hydrated Card before minting the invocation | Shared `WyrdState::run_for_card` performs the alias lookup before `Run::new`; both foreign projections delegate to it | Shared selection/refusal tests, Python focused tests, and Rust/TypeScript journey cases recorded green | PASS |
| Initial and later Card views are immutable siblings sharing one run ID; the parent remains unchanged | `Run::for_card` clones state and run ID and resolves a new subject without mutation | Shared, Python, Rust, and TypeScript subject/identity assertions | PASS |
| Unknown or out-of-graph aliases fail locally without network IO | `card_ref(alias)?` precedes Run construction and all observation/transport work | Shared and all three SDK negative cases assert `WYRD_SDK_404_UNKNOWN_ALIAS` | PASS |
| Rust, Python, and TypeScript expose the approved Run surfaces with declaration parity | Shared owner plus thin PyO3/N-API projections; Python owning/generated stubs and TypeScript wrappers/declarations agree | Recorded Rust/TS typing and journey lanes, Python signature/default test, typecheck and codegen evidence | PASS |
| Python `Run` is a synchronous context manager; entry returns the same Run and exit returns `False` without masking user exceptions | `PyRun::__enter__` returns `slf`; `__exit__` keeps conventional names/defaults and always returns false | `test_entering_a_run_returns_it_and_correlates_active_and_child_spans`, exit-signature test, and failure cases | PASS |
| One import-time private OTel context key stores only a tuple stack of `(card_ref, run_id)` pairs; no token, detach, or Run-owned scope state remains | `_SCOPE_KEY` is created at module import; `_enter_run` and `_exit_run` use only OTel `get_value`/`set_value`/`attach`; `PyRun` holds only native `Run` | R4E source assertions and focused nesting/concurrency cases; cumulative source contains no Run-correlation detach/token path | PASS |
| Entry pushes the exact pair; matching exit pops it; empty, mismatched, or failed exit changes nothing and never raises | `_enter_run` appends one pair; `_exit_run` compares the top before attaching the shortened tuple; both contain optional telemetry exceptions | Nested restoration, mismatched-exit, exit-update-failure, and user-exception cases recorded green | PASS |
| Active recording span is stamped only when it does not already carry `wyrd.card_ref` | `_enter_run` gates both attributes through `_carries_card_ref`; unreadable attributes retain the specified stamp-as-before fallback | Active-span and nested-no-overwrite focused cases recorded green | PASS |
| Child spans use the innermost pair, replacing conflicting initial values | `_RunCorrelationProcessor.on_start` reads `parent_context` (or current) and sets both exact attributes | Child/grandchild, nested Card, private-provider, and persisted framework-span cases | PASS |
| Correlation survives `await` and copied task context and stays isolated across concurrent tasks, including two tasks entering the identical immutable Run | Scope state is held only in OTel execution-local context; no token is stored on `PyRun` | Await/distinct-view/spawned-task case and `test_concurrent_tasks_entering_the_same_run_exit_independently` recorded green | PASS |
| Global and explicit private provider registration is thread-safe, idempotent, weak-lifetime, and attempted at most once, including accept-then-raise | Locked `WeakKeyDictionary` stores `False` before the foreign call and changes to `True` only after normal return; the mark is never discarded | Healthy global/private case, raises-before-retention case, and accept-then-raise direct/global-entry case recorded green | PASS |
| Missing OTel, API-only/unsupported provider, registration failure, context attach/update failure, and enrichment failure are fail-open and do not block explicit observations | Optional import bindings and all correlation entry/processor/exit/registration calls contain failures locally; Card and observation paths remain outside that containment | Each AC-032 failure class reaches the representative Drift boundary; user exceptions remain unchanged | PASS |
| Explicit Card lookup, validation, authorization, and observation failures remain strict | Alias lookup occurs before entry; explicit observation methods and server paths were not swallowed or redirected by the OTel integration | Unknown-alias tests, representative ordinary observation errors, and authenticated journey negative flows | PASS |
| Real stock OTLP/HTTP spans, custom data, and Eval evidence join by exact run/Card and active span identity with server-derived Card UID and authenticated publisher | Private-provider hook installs the same processor; journey emits framework spans without Wyrd attributes, a generic row, and an Eval row using the active span IDs | `test_scoped_run_emits_drift_eval_and_generic_rows` recorded passing against `WyrdTestServer` and repository Postgres; persisted queries assert every required join and identity | PASS |
| Scope exit is not a flush, shutdown, exporter lifecycle operation, network call, server Run, or durability acknowledgement | `PyRun::__exit__` delegates only to local `_exit_run`; journey orders provider force-flush, state shutdown, and server publication after exit | Exit source inspection and persisted journey barrier ordering | PASS |
| Only `wyrd.card_ref` and `wyrd.run_id` are injected; tenant, principal, Card UID, and request identity remain server-derived | Processor and active-span path set exactly the two constants; no server ingest or schema change was added for correlation | Persisted journey separately asserts asserted CardRef, authenticated publisher, and server-resolved Card UID | PASS |
| OTel remains optional and no second pipeline, wrapper span, global Card scope, log/metric promise, new queue, or new server Run enters the change | Optional Python extra only; shared Bifrost owner and existing OTLP endpoint are reused; processor creates no spans and owns no exporter | Dependency/diff inspection and unchanged broader journey ownership | PASS |
| R4E explicitly accepted residual for an entry attach failure under an identical outer pair is not expanded into an unapproved correction | Candidate implements the approved token-free attach-only design exactly | Revision-46 authority and R4E stop/non-goal text | PASS |

## Prior-finding closure hypotheses

| Stable finding | Behavior review result |
|---|---|
| `FIND-TASK-009-1` | CLOSED — every named optional failure class reaches unchanged explicit-observation behavior, including actual registration failure. |
| `FIND-TASK-009-2` | CLOSED — the Run subject documentation covers root, initially selected, and later sibling views. |
| `FIND-TASK-009-3` | SUPERSEDED/CLOSED — revision 46 deletes token/detach recovery and specifies a matching attach-only pop; exit-update failure is contained and proven. |
| `FIND-TASK-009-4` | SUPERSEDED/CLOSED — revision 46 creates the one private key at import, eliminating lazy-key publication. |
| `FIND-TASK-009-5` | CLOSED — PyO3 parameter names/defaults, owning/generated stubs, and public calls agree. |
| `FIND-TASK-009-6` | CLOSED — the two shared Rust tests contain their required `# Panics` sections. |
| `FIND-TASK-009-7` | CLOSED — the focused case coordinates two tasks entering the identical Run and proves independent exit. |
| `FIND-TASK-009-8` | SUPERSEDED/CLOSED under revision 46 — one terminal cached outcome is written before provider registration, so accept-then-raise is never retried; the exact direct-and-later-entry path is proven. |

## Proposed findings

None. No `MISSING`, `INCORRECT`, `DRIFT`, `VIOLATION`, or `REGRESSION` finding
survived source and caller-path validation in the behavior scope.

## Verification assessment

The immutable R4E implementation evidence records exact execution of every new
or changed AC-032 focused case, the complete Python focused file (37 passed),
the authenticated Postgres-backed journey, Python unit/integration/type/codegen/
PyO3/format/lint lanes, Rust format/lints, and `git diff --check`, all exiting
zero. Earlier cumulative task/remediation evidence records the shared Rust and
Rust/TypeScript SDK journey and typing lanes. This reviewer inspected those
commands, their mapped assertions, the current source, and the cumulative diff;
it did not rerun them during parallel discovery. `git diff --check` was inspected
successfully for the immutable range.

## Overall result

**PASS**

The cumulative candidate satisfies the original TASK-009 behavior as revised
by approved specification revision 46 and R4E. Required behavior is present,
the prior findings are closed or explicitly superseded by the approved design,
and the behavior review found no unrelated executable scope.
