# TASK-009 r6 invariant review

## Review findings

### Important

- **`INV-R6-001` — INCORRECT** —
  [`sdks/wyrd-sdk-python/python/wyrd/otel.py:302`](../../../../../sdks/wyrd-sdk-python/python/wyrd/otel.py)
  ignores the terminal `False` returned by `install_run_correlation()`, then
  lines 305-308 stamp the already-active span unconditionally. A global
  provider whose `add_span_processor` retains the processor and raises is
  therefore recorded as failed but still receives `wyrd.card_ref` and
  `wyrd.run_id` on an active span. This violates revision-46 REQ-151's specific
  failed-registration outcome: a provider that raises after accepting the
  processor gets no enrichment (`spec.md:335-342`). It also leaves
  `install_run_correlation() == False` describing a different outcome for an
  active span than for child spans.

  I reproduced the reachable path with a real `TracerProvider` subclass whose
  `add_span_processor` delegates to the SDK and then raises: with one of that
  provider's spans active, `with state.run(card="model")` returned normally and
  the active span held both Wyrd attributes. The R5 regression only invokes the
  retained processor (`test_observe_surface.py:439-468`), so it cannot catch
  this sibling enrichment path.

  **Observable consequence:** a caller receives the documented terminal
  `False` registration outcome and no child-span correlation, while its
  already-active span is nevertheless correlated and may be exported and
  persisted. **Required testable correction:** make the failed global-provider
  path leave its already-active span unchanged while preserving scope-context
  attachment and accepted global/private-provider correlation; add a focused
  case using the failed provider's own active span and assert that neither Wyrd
  attribute is written. The approved requirement's failed-registration clause
  is the narrower exception to its ordinary active-span stamping rule.

## Immutable subject

- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `1a4bbff5a26a1462d0f509c4595d52d08fbd25ae`
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 46
- Original task: `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md`
- Remediations: TASK-009-R1, TASK-009-R2, TASK-009-R3, TASK-009-R4E, and TASK-009-R5; R4E supersedes TASK-009-R4

The candidate remained at the stated commit throughout this review. No
`.codegraph/` directory exists, so direct source and caller inspection was used.

## Producer-to-sink trace

1. `WyrdState::run` and `run_for_card` resolve the exact hydrated `CardRef`
   before `Run::new` mints one `RunId`; `Run::for_card` clones that identity into
   an immutable sibling and every observation derives its correlation from the
   view (`crates/shared/wyrd-client/src/state.rs:489-510`,
   `crates/shared/wyrd-client/src/observe/mod.rs:42-113`).
2. Python and TypeScript delegate initial and sibling selection to that shared
   owner. No language projection mints a second identity, mutates its parent,
   or performs network work during selection.
3. Python `Run.__enter__` and `__exit__` send the view's exact pair to
   `wyrd.otel`; no mutable scope token is stored on `PyRun`
   (`sdks/wyrd-sdk-python/src/observe/mod.rs:195-248`).
4. `_enter_run` installs the global processor, attaches one pushed tuple to the
   import-time OTel context key, and stamps an eligible active span. The
   processor reads the innermost pair from `parent_context`. `_exit_run` attaches
   the popped tuple only for an exact top match, so normal OTel context copying
   supplies await/task isolation (`otel.py:192-334`).
5. R5 replaced equality-keyed weak outcomes with locked weak identity entries
   and gives each attempt an inactive processor activated only after normal
   registration return (`otel.py:200-290`). That closes the equal-provider and
   retained-processor defects, but the active-span sibling path bypasses the
   recorded failure as described in `INV-R6-001`.
6. The real Python journey uses a successfully installed private stock provider,
   authenticated OTLP/HTTP export, explicit provider flush, state shutdown, and
   server publication before querying trace/custom/Eval joins
   (`tests/integration/state/test_observe_journey.py:365-475,477-563`).

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-123: root/default and initially selected runs use one Rust-owned invocation model | `WyrdState::run` / `run_for_card`; `Run::new` | Shared Rust, Python, Rust SDK, and TypeScript selection assertions | PASS |
| Initial and sibling Card selection is local, exact, immutable, and unknown aliases fail before Run construction or network work | Alias lookup precedes `Run::new`; `Run::for_card` preserves `run_id` and replaces only `subject` | Shared and language-surface positive/negative cases | PASS |
| Rust, Python, and TypeScript expose the approved Run surfaces without a second durable owner | Thin projections over `wyrd_client::Run`; generated declarations agree | Recorded Rust/TS journeys, Python signature tests, typecheck/codegen evidence | PASS |
| REQ-151: one import-time context key stores only an ordered tuple stack of exact CardRef/Run-ID pairs | `_SCOPE_KEY`, `_enter_run`, `_exit_run` at `otel.py:192-334` | Nested, mismatch, await, task-copy, sibling-task, and same-Run concurrent cases | PASS |
| Entry/exit each attach once; exit pops only its exact top and never detaches | `otel.py:293-334` contains no token or detach path | Nested restoration and mismatched-exit cases | PASS |
| Active and child spans of an accepted provider receive the exact pair; nested entry does not overwrite an already-correlated active span | `_enter_run`, `_carries_card_ref`, active per-attempt processor | Active/child/grandchild, nested no-overwrite, private-provider, and persisted journey proof | PASS |
| A registration failure, including accept-then-raise, is terminal and gets no enrichment | Retained per-attempt processor stays inactive, but `_enter_run` stamps the failed global provider's active span after installation returns `False` | Retained-processor test covers only `processor.on_start`; direct real-provider reproduction shows the active-span leak | **FAIL (`INV-R6-001`)** |
| Registration outcomes are weak, identity-keyed, thread-safe, and attempted once per provider object | Locked list of weak refs; `_outcome_entry` compares referents with `is`; entry is recorded before the foreign call | Equal-provider and healthy/private idempotency cases; 39-test focused file | PASS |
| Missing/API-only OTel and registration/context/processor failures never escape or block Run use, explicit observations, or user exceptions | Optional import and containment at every OTel/PyO3 edge | Focused absence, attach, lookup, registration, explicit-observation, and user-exception cases | PASS |
| Context propagation remains isolated across await, task creation, distinct views, and two tasks entering the identical immutable Run | Scope state exists only in OTel context; `Run` and `PyRun` retain no tokens | Both async isolation tests | PASS |
| `Run.__exit__` has public name/default parity and never suppresses | PyO3 signature and unconditional `false`; owning/generated stubs agree | Public signature/default test and codegen/typecheck evidence | PASS |
| AC-032 persists authenticated trace/custom/Eval joins with server-derived publisher/Card UID | Stock private provider, OTLP endpoint, explicit lifecycle barriers, public Bifrost queries | Recorded exact Postgres-backed journey | PASS |
| INV-007: the client injects only CardRef and Run ID | Correlation pair and processor writes contain only the two approved keys | Persisted publisher/Card-UID assertions | PASS |
| INV-012/non-goals: no server Run, second pipeline, wrapper span, required OTel dependency, implicit lifecycle barrier, or log/metric promise | Cumulative diff and optional dependency boundary | Source/manifests and journey ordering | PASS |
| R5 artifact closure: revision 46 authority, superseded R4, and exact named Rust proof | `run_api.md:3`; R4 frontmatter; original-task exact commands | Static inspection and recorded zero-exit commands | PASS |

## Prior-finding closure

| Finding | Result |
|---|---|
| `FIND-TASK-009-1` | CLOSED — optional failures remain contained and explicit observations retain their ordinary strict errors. |
| `FIND-TASK-009-2` | CLOSED — Run subject documentation covers root, initial, and sibling views. |
| `FIND-TASK-009-3` | SUPERSEDED/CLOSED — revision 46 replaced token/detach recovery with the context-value tuple stack. |
| `FIND-TASK-009-4` | CLOSED — the context key is created once at import. |
| `FIND-TASK-009-5` | CLOSED — runtime and generated exit names/defaults agree. |
| `FIND-TASK-009-6` | CLOSED — both named shared Rust tests document panic conditions. |
| `FIND-TASK-009-7` | CLOSED — two tasks entering the identical Run prove independent scope exit. |
| `FIND-TASK-009-8` | CLOSED — each provider gets at most one registration handoff. |
| `FIND-TASK-009-9` | CLOSED — outcomes are weakly keyed by referent identity, so distinct equal providers register independently. |
| `FIND-TASK-009-10` | PARTIAL — the retained failed processor is inert, but the same failed global provider's already-active span remains enrichable (`INV-R6-001`). |
| `FIND-TASK-009-11` | CLOSED — the Run authority identifies revision 46. |
| `FIND-TASK-009-12` | CLOSED — TASK-009-R4 is marked `superseded`. |
| `FIND-TASK-009-13` | CLOSED — all named Rust proofs now have exact repository-native commands, including the Postgres wrapper. |

## Verification notes

- `mise run py:setup` and the complete focused Python surface passed: `39 passed`.
- `git diff --check 7d96c300..1a4bbff5` passed.
- I reproduced `INV-R6-001` in the installed candidate using a real SDK
  `TracerProvider` that accepts the processor then raises; its active span
  contained both Wyrd attributes after Run entry.
- Broader Rust, Python, TypeScript, persisted-journey, typecheck, codegen,
  boundary, formatting, and lint evidence was inspected in the cumulative task
  and remediation records rather than rerun.

## Overall result

**FAIL**

The cumulative candidate preserves Run identity, execution-local isolation,
provider identity bookkeeping, and persisted correlation, but it does not yet
satisfy the explicit terminal no-enrichment invariant for a failed global
provider's already-active span.
