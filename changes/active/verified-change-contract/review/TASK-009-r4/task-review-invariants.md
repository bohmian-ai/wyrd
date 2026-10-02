# TASK-009 invariant review — round 4

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `017a54d5390eb488e820f890bbb953a5f1ca3a53`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 45
- Original task: `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md`
- Remediation tasks: TASK-009-R1, TASK-009-R2, and TASK-009-R3 in their respective prior review directories

The candidate remained at the stated commit during this review. The cumulative
base-to-candidate diff, not only the R3 test change, was reviewed.

## Navigation and invariant trace

| Producer or owner | Value/state produced | Consumers and sinks inspected |
|---|---|---|
| `crates/shared/wyrd-client/src/state.rs::WyrdState::{run,run_for_card}` | Root or locally resolved initial `CardRef`; one new UUIDv7 `RunId` | `Run::new`; Rust, Python, and TypeScript SDK projections |
| `crates/shared/wyrd-client/src/observe/mod.rs::Run` | Immutable `(run_id, subject)` view; sibling views retain `run_id` and replace only `subject` | Drift, Eval, and generic-record `Correlation`; SDK getters and language journeys |
| `sdks/wyrd-sdk-python/src/{state,observe}/mod.rs` | Python `run(card=...)`, immutable views, context-manager entry/exit, active-span Eval IDs | Public generated stubs, Python unit tests, persisted Python journey |
| `sdks/wyrd-sdk-python/python/wyrd/otel.py` | Private OTel context key, execution-local token/prior stack, per-provider processor registration, span attributes | Global and private providers, active span, child spans, nested/async scopes, OTLP export |
| Rust/Python/TypeScript SDK tests | Public projection and correlation proof | Shared writer correlation and real Bifrost persisted rows |

The main value trace is: hydrated alias -> exact `CardRef` -> `Run::subject`;
`RunId::new()` -> immutable sibling `Run` clones -> per-row `Correlation`; and,
for Python spans, `(card_ref, run_id)` -> OTel execution-local context -> span
processor -> record-level OTLP attributes -> Gate/Scribe authorization and
server-derived `card_uid` -> persisted Bifrost joins.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-123: root-default and initial Card selection are shared, local, and immutable | `WyrdState::run`, `run_for_card`, `Run::new`, and `Run::for_card` in shared `wyrd-client` | Shared selection/refusal tests; Python, Rust SDK, and TypeScript selection assertions recorded in TASK-009 evidence | PASS |
| REQ-123: one UUIDv7 invocation is shared by sibling views while separate runs get separate IDs | `Run::new` mints once; `Run::for_card` clones `run_id` and resolves only `subject` | Shared and three-language tests assert shared sibling identity and distinct new invocation identity | PASS |
| Unknown and out-of-graph aliases fail locally without opening a Run or doing network IO | `run_for_card` resolves through `card_ref(alias)` before `Run::new`; `for_card` uses the same index | Shared, Python, and TypeScript unknown-alias cases | PASS |
| Rust, Python, and TypeScript expose the approved initial-Card and sibling-view surfaces | Thin bindings delegate to shared state/Run; generated Python and N-API declarations agree with runtime | Recorded typecheck/codegen/N-API evidence and language tests | PASS |
| REQ-151: Python entry returns the same Run, attaches exact CardRef/Run ID, stamps an active recording span, and enriches child spans | `PyRun::__enter__`, `_enter_run`, `_RunCorrelationProcessor::on_start` | `test_entering_a_run_returns_it_and_correlates_active_and_child_spans`; persisted OTLP journey | PASS |
| REQ-151/AC-032: nested scopes, `await`, copied task context, sibling-task isolation, and identical-Run concurrent tasks restore/isolate correlation | `_scope_tokens` is a `ContextVar` stack of exact token/prior entries; no token is stored on `PyRun` | Nested/async test plus R3 `test_concurrent_tasks_entering_the_same_run_exit_independently`; focused rerun passed | PASS |
| Concurrent first entries use one private OTel context key | `_key` double-checks `_scope_key` under the existing lock | Deterministic two-thread first-entry case | PASS |
| Exit restores prior/absent correlation even when public detach raises or silently fails, and never suppresses an application exception | `_exit_run` removes the matching execution-local entry, attempts exact-token detach, and restores recorded prior value when needed; `PyRun::__exit__` returns false | Detach recovery, nested restoration, exit-signature, and user-exception cases; focused detach case rerun passed | PASS |
| Missing/API-only/registration/attach/enrichment/detach failures do not escape or block explicit observations | Optional imports and OTel operations are contained at the Python boundary; explicit observation paths remain shared Rust paths | Focused failure cases reach the ordinary Drift boundary and preserve user exceptions; focused registration/attach case rerun passed | PASS |
| Provider registration is idempotent and each global or explicit private provider receives at most one Wyrd processor, including an ambiguous registration failure | Success is tracked in `_registered`, but the provider is removed when `add_span_processor` raises, permitting another processor to be handed to the same provider on every retry | Healthy-provider test proves one processor only on success. Direct reproduction with a provider that appends the processor and then raises returned `False` twice while its processor count grew from one to two | **FAIL — `INV-REV-R4-001`** |
| The public `Run.__exit__` runtime names/defaults and generated typing agree | PyO3 signature uses `exc_type`, `exc_value`, `traceback`, each defaulting to `None`; owning and generated stubs match | `test_run_exit_accepts_conventional_keywords_and_omitted_arguments`; recorded typecheck/codegen evidence | PASS |
| AC-032: stock OTLP/HTTP export persists exact Run/Card correlation and joins trace, custom, and Eval evidence under server-derived identity | Existing Python journey installs the processor on a private provider, exports through authenticated `/v1/traces`, drains explicitly, and queries persisted tables | `test_scoped_run_emits_drift_eval_and_generic_rows` recorded passing; assertions cover exact attributes, publisher, resolved `card_uid`, run join, and trace/span join | PASS |
| Context exit is not a flush, shutdown, exporter lifecycle action, network call, or durability acknowledgement | Exit performs only optional context cleanup; journey separately calls provider flush, state shutdown, and server publication | Source inspection and journey ordering | PASS |
| Non-goals remain excluded: no server Run, second client/queue/exporter, wrapper span, mandatory OTel production dependency, process-global Card scope, ambient log/metric promise, or client-authored managed identity | Shared Run remains local; Python uses the caller's provider and optional import; production dependencies and server contracts are unchanged | Cumulative diff and manifests | PASS |
| INV-007/INV-012: existing signed Card-scope authorization, server-derived identity, observation projection, and Bifrost contracts remain the sinks | Client supplies only exact `card_ref` and opaque `run_id`; no Vala/server contract was changed | Persisted authenticated journey and unchanged server owners | PASS |

## Prior-finding closure

| Finding | Closure evidence | Result |
|---|---|---|
| `FIND-TASK-009-1` | Actual raising registration, API-only provider, attach, enrichment, and detach failures reach the ordinary explicit Drift path; application exceptions remain unchanged | CLOSED |
| `FIND-TASK-009-2` | `Run::subject` now documents root, initially selected, and sibling-selected views | CLOSED |
| `FIND-TASK-009-3` | Token/prior entries and fallback restoration prevent stale Wyrd correlation after failed or swallowed detach | CLOSED |
| `FIND-TASK-009-4` | Lazy key creation is serialized and proven with concurrent first entry | CLOSED |
| `FIND-TASK-009-5` | PyO3 and stub `__exit__` names/defaults agree and public calls are exercised | CLOSED |
| `FIND-TASK-009-6` | Both new shared Rust tests contain accurate `# Panics` sections | CLOSED |
| `FIND-TASK-009-7` | Two coordinated asyncio tasks enter the exact same `Run`, interleave exits, preserve the still-active task's pair, and leave neither exited task correlated | CLOSED |

The prior findings do not share a remaining Run/token source defect. The new
finding is at the separate provider-registration state transition: a raised
registration call has an ambiguous side effect, but the owner currently treats
it as definitely unregistered and retries.

## Proposed finding

### `INV-REV-R4-001` — `INCORRECT`: ambiguous registration failure can install duplicate processors

- **Violated obligation:** REQ-151 and TASK-009 Scenario 3 require idempotent
  provider registration; the task acceptance criteria require global and
  explicitly supplied private providers to receive at most one Wyrd processor.
- **Exact location:** `sdks/wyrd-sdk-python/python/wyrd/otel.py:263-273`, with
  the retry made reachable from every Run entry at `otel.py:279-288`.
- **Producer-to-consumer evidence:** `install_run_correlation` adds the provider
  to `_registered`, calls the foreign provider's `add_span_processor`, and on
  any exception discards the provider. That exception does not establish that
  the foreign call had no side effect. A provider that appends the supplied
  processor and then raises receives another `_RunCorrelationProcessor` on the
  next explicit installation or Run entry. The existing failure test at
  `tests/unit/state/test_observe_surface.py:417-427` raises before retaining the
  processor, while the Run-entry failure case at lines 478-493 explicitly
  expects a second registration attempt, so neither falsifies the ambiguous
  side-effect path.
- **Independent reproduction:** from the Python SDK environment, a minimal
  provider whose `add_span_processor` appends its argument and then raises
  produced `(False, 1)` on the first call and `(False, 2)` on the second. No
  repository source was changed for this check.
- **Observable consequence:** one provider holds multiple Wyrd processors and
  runs duplicate correlation hooks for every started span, directly violating
  the at-most-one and idempotency contract. Repeated Run entries can continue
  increasing that count.
- **Required testable correction:** keep registration outcome in the existing
  locked, weak per-provider owner, but distinguish successful installation
  from failed/ambiguous attempted installation. A successful provider remains
  idempotently `True`; a provider whose registration raised remains
  fail-open `False` and is not called again, because the foreign side effect
  cannot safely be retried. Reuse the existing lock and weak provider tracking;
  add no retry, warning, wrapper provider, dependency, or second registry
  mechanism. Replace the current assertions that require repeated failing
  attempts.
- **Focused closure proof:** use the existing provider-registration test home
  with one provider that retains the supplied processor and then raises. Two
  calls, including at least one through Run entry, must not raise or block the
  representative explicit observation, must both report/behave unavailable,
  and must leave exactly one supplied Wyrd processor. Retain the healthy global
  and private provider assertions that successful installation returns `True`
  and adds exactly one processor.

## Verification assessed

- Focused rerun: same-Run async isolation, healthy provider idempotency,
  registration/attach fail-open behavior, and detach restoration — `4 passed`.
- Direct ambiguous-registration reproduction — first call retained one
  processor and returned `False`; second retained a second processor and
  returned `False`.
- Broader Rust/Python/TypeScript, generated-declaration, boundary, lint, and
  persisted-journey results were available in the task and remediation evidence.
  They do not exercise the retained-then-raised provider behavior above.

## Overall result

**FAIL**

One bounded implementation defect remains: `INV-REV-R4-001`.
