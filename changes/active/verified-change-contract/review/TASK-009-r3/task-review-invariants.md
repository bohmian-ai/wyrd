# TASK-009 invariant review — Round 3

## Immutable subject and scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `d01307b8c37b47488115743c94b624a29d66e4be`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 45
- Original task: `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md`
- Remediations: `TASK-009-R1-restore-otel-correlation.md` and
  `TASK-009-R2-close-proof-and-boundary-parity.md`
- Reviewed range: the complete cumulative `base..candidate` diff, with prior
  findings used only as hypotheses

`HEAD` matched the candidate before and after this review. The reviewed source
had no uncommitted changes; only the new round-three review directory was
untracked. No `.codegraph/` index exists, so repository search and direct source
inspection were used.

## Producer-to-sink navigation and invariant trace

The shared identity producer is `WyrdState`. `run()` clones the validated state,
selects its exact root `CardRef`, and calls `Run::new`; `run_for_card(alias)`
first resolves the alias through the same hydrated index, then calls `Run::new`.
`Run::new` is the single invocation-ID mint and produces one UUIDv7 `RunId`.
`Run::for_card` clones that ID while resolving a new immutable subject. The
correlation sink for explicit Drift, Eval, and generic observations is
`Run::correlation`, which always uses the current immutable view's exact
`CardRef` and the shared invocation ID. Unknown initial or sibling aliases fail
before a Run is returned and before network IO.

The Python and TypeScript initial-selection surfaces are thin consumers of that
owner. Python `WyrdState.run(*, card=None)` and TypeScript `WyrdState.run(card?)`
delegate to shared `run_for_card` when an alias is present and shared `run`
otherwise. Rust consumes the shared methods directly. None mints another Run
identity, re-resolves a Card independently, or mutates a sibling view.

Python ambient span correlation is a separate foreign-runtime path. `PyRun`
passes only the shared native `card_ref` and `run_id` to `wyrd.otel._enter_run`.
The module owns one lazily created private OTel context key, serialized by its
existing lock; one execution-local `ContextVar` stack records the exact attach
token and prior correlation for each successful entry. `_RunCorrelationProcessor`
reads that same key from the parent context and copies only
`wyrd.card_ref`/`wyrd.run_id`. `_exit_run` consumes the innermost local entry,
attempts its exact-token detach, and restores the recorded prior value if OTel
raises or silently fails to reset. Nested scopes therefore restore the outer
pair; separate asyncio contexts and directly entered threads retain their own
stack values while sharing the one process-private key.

Provider registration is serialized and weakly tracked per provider. Failed
registration removes the provisional registry entry and returns `False`; entry
continues to attach best-effort context. R2's focused case now reaches the actual
raising `add_span_processor` branch during Run entry, then proves an explicit
Drift call reaches its ordinary offline writer error. API-only, missing-package,
attach, processor lookup/enrichment, and detach failures have the same
representative explicit-observation proof. These failures do not alter the
native Run or its explicit observation path.

The persisted Python journey uses a private stock SDK provider plus the
explicit installation hook, exports unannotated framework spans through the
authenticated OTLP/HTTP endpoint, and joins persisted trace, custom, and Eval
rows. Bifrost remains the sink that validates the exact record attributes,
authorizes the asserted CardRef against signed scope, resolves `card_uid`, and
derives tenant and publisher identity. Client code does not author those managed
identities. Provider flush, state shutdown, and publication remain explicit
barriers after context exit.

R2 also restores public boundary parity: the PyO3 `__exit__` parameter names and
`None` defaults now match the owning and generated stubs, and the boundary
always returns `False`. The two new shared Rust selection tests now document
their panic conditions without changing their executable proof.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-123: root default and local initial Card selection use one shared Run owner | `state.rs`: `run`, `run_for_card`; `observe/mod.rs`: `Run::new`, `Run::for_card`, `Run::correlation` | Shared selection/refusal tests; Rust, Python, and TypeScript journey assertions | PASS |
| One UUIDv7 invocation is shared by immutable sibling views, while separate opens mint separate IDs | `Run::new` is the only changed mint; `for_card` clones `run_id` and resolves a distinct subject | Shared `run_for_card_selects_the_initial_view_and_shares_its_invocation`; all SDK initial-selection assertions | PASS |
| Unknown and out-of-graph aliases fail locally without opening or retargeting a Run | `WyrdState::run_for_card` resolves before `Run::new`; `Run::for_card` returns the shared index error | Shared/Python/TypeScript unknown-initial-alias checks and existing sibling refusal checks | PASS |
| Rust, Python, and TypeScript project the same Card/Run identities without duplicating ownership | Python state wrapper and TypeScript native wrapper delegate to shared state; Rust reuses shared types | Public Python and TypeScript tests plus Rust SDK journey | PASS |
| REQ-151: entry attaches exact CardRef/Run ID, stamps an active span, and child spans inherit through one processor | `PyRun::__enter__`; `_enter_run`; `_RunCorrelationProcessor.on_start` | Active/child/grandchild focused test and persisted OTLP journey | PASS |
| Nested, `await`, copied-task, concurrent-task, and concurrent-first-entry contexts isolate and restore values | execution-local `_scope_tokens`; locked double-checked `_key`; exact token/prior entry | Nested, asyncio/task-copy, and deterministic two-thread first-entry tests | PASS |
| Detach/reset failure cannot leave the exited pair active; nested failure restores the outer pair | `_exit_run` compares post-detach value with recorded prior and re-attaches prior on mismatch | `test_detach_failure_restores_the_prior_correlation` proves outer restoration and post-exit absence | PASS |
| Missing/API-only/registration/attach/enrichment/detach failures remain fail-open and cannot block explicit observations | all optional integration is contained in `wyrd.otel`; native explicit observe path is independent | R2 registration case plus missing-package, attach, enrichment, and detach representative Drift checks; user exception check | PASS |
| Global and private providers receive idempotent registration; OTel remains optional | `_registered` under `_registered_lock`; production dependency remains in optional `otel` extra | Global/private provider count test; unsupported/failing-provider checks | PASS |
| Public Python context-manager declaration matches runtime and never suppresses a user exception | PyO3 `__exit__(exc_type=None, exc_value=None, traceback=None)`; owning/generated stubs match | `test_run_exit_accepts_conventional_keywords_and_omitted_arguments`; user-exception checks | PASS |
| AC-032: persisted spans, custom rows, and Eval rows join by approved identities while managed identities remain server-derived | existing OTLP/Gate/Scribe projection unchanged; journey uses shared Run and authenticated endpoint | Recorded exact Python journey passes and persisted assertions for Run/Card/trace/span/publisher joins | PASS |
| Context exit has no flush, shutdown, network, span-lifecycle, or durability effect | `__exit__` delegates only `_exit_run`; journey separately flushes provider, shuts state, and publishes | Source ordering and persisted journey | PASS |
| INV-007/INV-012 and non-goals: no new server Run, queue, schema, wrapper span, log/metric enrichment, client-managed identity, or altered observation algorithm | cumulative production diff is limited to shared Run selection, thin SDK projections, and Python span context; Bifrost/server owners are unchanged | Complete diff inspection; boundary/codegen and language lanes recorded in task/remediation evidence | PASS |
| Prior repository-rule remediation remains closed | `Run::subject` docs cover every constructor; new Rust tests have `# Panics` sections | Static source inspection plus recorded format/lint results | PASS |

## Prior-finding closure

| Finding | Invariant assessment |
|---|---|
| `FIND-TASK-009-1` | CLOSED. The actual raising registration branch is now exercised during Run entry and followed by an explicit Drift call; all other specified optional-failure classes retain equivalent proof. |
| `FIND-TASK-009-2` | CLOSED. `Run::subject` documents root, initially selected, and sibling-selected views consistently with both construction paths. |
| `FIND-TASK-009-3` | CLOSED. Each successful entry retains its exact token and prior value; exit restores the prior pair even after a raising or swallowed reset failure. |
| `FIND-TASK-009-4` | CLOSED. Lazy key creation is double-checked under the existing lock, and deterministic concurrent first use proves one key. |
| `FIND-TASK-009-5` | CLOSED. PyO3 names/defaults, owning stub, generated stub, and public calls agree. |
| `FIND-TASK-009-6` | CLOSED. Both new Rust tests document their panic conditions without changing proof behavior. |

## Proposed findings

None. No required state, identity, lifecycle, failure, or sibling-consumer
invariant remains unsatisfied in the reviewed cumulative candidate.

## Verification assessment

The candidate records successful focused shared Run checks, the 34-test Python
surface file, the exact authenticated persisted Python journey, Rust and
TypeScript SDK journeys, shared and SDK family tests, Python/TypeScript typing
and tests, code generation, client/PyO3 boundary checks, formatting, lints, and
`git diff --check`. I inspected those claims against the final source and task
recipes. I independently ran cumulative `git diff --check`; no Cargo or
Postgres lane was rerun in this review, so the recorded immutable evidence is
the execution proof for those lanes.

## Overall result

**PASS**
