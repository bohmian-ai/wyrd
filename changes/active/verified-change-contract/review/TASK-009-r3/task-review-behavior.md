# TASK-009 behavior review — Round 3

## Immutable subject and coverage

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `d01307b8c37b47488115743c94b624a29d66e4be`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 45
- Original task: `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md`
- Remediations reviewed cumulatively: `TASK-009-R1-restore-otel-correlation.md` and `TASK-009-R2-close-proof-and-boundary-parity.md`

The complete base-to-candidate range was reviewed. Prior findings and verdicts
were treated as hypotheses and traced against the cumulative source. There is
no `.codegraph/` index, so repository search and direct source inspection were
used. The candidate matched `HEAD` before and after review.

The caller-to-result paths inspected were:

1. hydrated alias lookup through `WyrdState::run` / `run_for_card`, `Run::new`,
   immutable `Run::for_card`, correlation creation, and the Rust, Python, and
   TypeScript public projections;
2. `PyRun.__enter__` / `__exit__` through `wyrd.otel` key creation, provider
   registration, execution-local attach/restore, active-span stamping, and
   child-span processing;
3. framework span creation through authenticated OTLP/HTTP ingest, explicit
   custom and Eval observation emission, lifecycle barriers, and persisted
   Bifrost joins; and
4. all changed focused tests, the existing extended Python journey, generated
   public declarations, and the prior R1/R2 correction paths.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-123: root-default Runs mint one UUIDv7 invocation and target the root Service | `crates/shared/wyrd-client/src/state.rs:489-496`; `observe/mod.rs:59-69` | Existing root and distinct-invocation tests; Rust/Python/TS journey assertions | PASS |
| REQ-123: initial Card selection resolves locally before Run construction | `state.rs:498-510` resolves `card_ref(alias)?` before `Run::new`; Python and TS delegate to this shared owner | Shared `run_for_card_*`, Python `test_run_card_*`, Rust and TS public-surface assertions | PASS |
| REQ-123: later Card views are immutable siblings sharing the same invocation | `observe/mod.rs:84-100` clones state and `run_id` while resolving a new subject | Shared/Python/Rust/TS selection and sibling-identity assertions | PASS |
| REQ-123: unknown or out-of-graph aliases fail strictly and locally | Shared lookup returns `WYRD_SDK_404_UNKNOWN_ALIAS`; language boundaries preserve it | Shared, Python, Rust, and TS negative cases | PASS |
| Public API parity across Rust, Python, and TypeScript | Shared `run_for_card`; Python keyword-only `run(*, card=None)`; TS `run(card?)`; generated `.pyi`/`.d.ts` match runtime boundaries | Recorded Rust/TS/type/codegen lanes; source/declaration inspection | PASS |
| REQ-151: Python Run is a synchronous context manager returning itself and never suppressing application exceptions | `sdks/wyrd-sdk-python/src/observe/mod.rs:195-239` | Active/child test, user-exception checks, and public `__exit__` signature/default test | PASS |
| REQ-151: entry attaches exact `wyrd.card_ref` and `wyrd.run_id`, stamps an active recording span, and overwrites conflicting values on child spans | `python/wyrd/otel.py:215-231,279-297` | `test_entering_a_run_returns_it_and_correlates_active_and_child_spans` covers active, child, grandchild, and conflicting initial CardRef | PASS |
| REQ-151: global provider installation is automatic; explicit private-provider installation is idempotent | `otel.py:246-276,288`; one locked weak provider registry | `test_global_and_private_providers_receive_one_processor_each`; real journey uses the private-provider hook | PASS |
| REQ-151: concurrent first entries use one private OTel context key | `_key` double-checks under the existing lock at `otel.py:201-212` | Deterministically overlapped `test_concurrent_first_entries_share_one_scope_key` | PASS |
| REQ-151 / AC-032: nested scopes restore the outer Card and normal exit removes correlation | Exact token/prior stack and restoration at `otel.py:279-325` | Nested scope and post-exit span assertions | PASS |
| REQ-151 / AC-032: correlation survives `await`, isolates concurrent tasks, and is copied into a task created inside the scope | OTel execution-local context plus `ContextVar` token stack; no token on `Run` | `test_scope_survives_await_and_isolates_concurrent_tasks` | PASS |
| AC-032: missing OTel, API-only provider, actual registration exception, attach failure, enrichment failure, and detach failure do not escape or block explicit observation emission | Optional imports and every telemetry operation are contained in `otel.py`; PyO3 discards only optional telemetry errors | Focused tests pair every named failure with the ordinary explicit Drift boundary; actual registration attempts are asserted | PASS |
| REQ-151: detach/reset failure restores the prior pair, including nested outer restoration and absent outer scope | `_exit_run` attempts exact detach then compares and reattaches recorded prior at `otel.py:300-325` | `test_detach_failure_restores_the_prior_correlation` covers swallowed inner reset and raising outer detach in the same context | PASS |
| REQ-151: strict Card identity, validation, authorization, and Wyrd write failures are not swallowed | Alias resolution and observation calls remain outside optional telemetry containment; server paths are unchanged | Unknown-alias and ordinary explicit-observation errors remain visible; persisted journey uses signed scope | PASS |
| R2 public-exit parity: conventional keyword and omitted-argument calls agree with the public stub | PyO3 names/defaults at `src/observe/mod.rs:225-230`; owning and generated stubs declare the same optional triple | `test_run_exit_accepts_conventional_keywords_and_omitted_arguments`; focused Python file passes | PASS |
| AC-032: stock OTLP/HTTP spans are exported through authenticated `/v1/traces` from `with state.run(card="agent")` | `test_observe_journey.py:353-402` exchanges the credential, installs the processor on a stock provider, creates framework spans, and emits custom/Eval evidence | Recorded exact Postgres-backed journey passed | PASS |
| AC-032: persisted spans carry exact Run/Card correlation, authenticated publisher, and server-resolved Card UID | Persisted query and lossless attribute decoding at `test_observe_journey.py:413-440` | Journey asserts both spans' exact CardRef, Run ID, managed Card UID, and common non-null publisher | PASS |
| AC-032: custom evidence joins traces by Run and Eval joins the active tool span by exact trace/span IDs with the same Run/Card identity | Journey SQL/assertions at `test_observe_journey.py:442-474`; Eval call supplies no explicit trace/span IDs | Persisted custom and Eval join assertions | PASS |
| Context exit is correlation cleanup, not flush, shutdown, span lifecycle, server IO, or durability acknowledgement | `PyRun.__exit__` only delegates `_exit_run`; journey separately flushes provider, shuts down state, and waits for server publication at `test_observe_journey.py:550-562` | Focused post-exit assertions and recorded persisted journey | PASS |
| INV-007: client injects no tenant, principal, Card UID, or request identity | Processor writes only `_CARD_REF` and `_RUN_ID`; server ingest/auth code is unchanged | Authenticated persisted publisher and server-resolved Card UID assertions | PASS |
| INV-012: existing observation, Eval, Bifrost, and ingest contracts are reused | Run supplies existing row correlation; journey extends the existing Service fixture and existing writer/query surfaces | Existing multi-language observation journeys and the extended Python journey | PASS |
| Non-goals: no server Run resource, second pipeline/queue, wrapper span, process-global Card value, required OTel dependency, implicit lifecycle ownership, or log/metric enrichment | No server/Vala contract change; execution-local context value; OTLP/HTTP exporter is dev-only; caller provider and state writer remain owners | Cumulative diff, dependency inspection, and unchanged production surfaces | PASS |
| R1/R2 documentation corrections describe the actual behavior without executable drift | `Run::subject` documents root/initial/sibling views; the two new shared tests include accurate `# Panics` sections | Static source inspection plus recorded formatting/lint evidence | PASS |
| Scope control and Ponytail: smallest existing owners and mechanisms are reused | Shared lookup/Run identity, existing module lock, native OTel context, current generator, existing focused test home, and existing journey fixture are reused | Complete cumulative diff and caller inspection found no unrelated behavior or compatibility path | PASS |

## Prior-finding closure

| Finding | Closure evidence | Result |
|---|---|---|
| `FIND-TASK-009-1` | The R2 case selects a provider whose `add_span_processor` records and raises during Run entry, then reaches the representative Drift error; the remaining named failure classes likewise reach explicit observation emission. | CLOSED |
| `FIND-TASK-009-2` | `Run::subject` now names root, initially selected, and sibling-selected views accurately. | CLOSED |
| `FIND-TASK-009-3` | Exact token plus recorded prior restoration is implemented and same-context nested/outer failure recovery is exercised. | CLOSED |
| `FIND-TASK-009-4` | Lazy key creation is serialized with the existing lock and deterministic concurrent-first-use proof establishes one key. | CLOSED |
| `FIND-TASK-009-5` | Runtime PyO3 parameter names/defaults and owning/generated stubs agree; public calls are exercised. | CLOSED |
| `FIND-TASK-009-6` | Both new shared Rust tests document their panic conditions without changing their executable behavior. | CLOSED |

## Proposed findings

No material behavioral findings.

## Verification assessment

I independently ran:

- `mise exec -- uv run python -m pytest -q tests/unit/state/test_observe_surface.py`: **34 passed**;
- `git diff --check 7d96c30066425e0cde2290842d5801307843283d..d01307b8c37b47488115743c94b624a29d66e4be`: passed.

The task and remediation evidence record successful focused Rust tests, the
real Postgres-backed Python journey, shared/Rust/Python/TypeScript suites,
typing, codegen, boundary, format, and lint lanes. Those broader results were
inspected against the named source and assertions but not all independently
rerun in this discovery pass. The persisted journey directly exercises the
required client-to-server-to-Bifrost outcome and is not replaced by the unit
evidence.

## Overall result

**PASS**
