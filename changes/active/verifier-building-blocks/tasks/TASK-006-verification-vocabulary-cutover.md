---
id: TASK-006
kind: implementation
status: proposed
spec: SPEC-verifier-building-blocks
spec_revision: 10
requirements: [REQ-017, REQ-018, REQ-019, REQ-020, REQ-021, REQ-022, INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008, INV-009, AC-009, AC-010, AC-011, AC-012, AC-013, AC-014, AC-015, AC-016]
depends_on: [TASK-005]
---

# Delete obsolete Eval machinery and finish the verification cutover

## Outcome and Value

Wyrd has one verification model and one vocabulary. Delete the unused Eval
product rather than renaming its workflows; move the generic Verifier and
its live dependencies under verification names. Old commands, permission
resources and tables receive ordinary refusals. Existing Verifier behavior
and the external GenAI OTel semantics remain intact.

This is the revision 10 closeout requested by the owner. Nothing has shipped:
there is no compatibility, historical-reader or data-conversion requirement.
The obsolete subsystem is removed directly, with a forward development-schema
upgrade. This task owns the remaining stale-reference cleanup identified in
the handoff. It does not retroactively modify TASK-005 review evidence.

TASK-005 must pass independent `$wyrd-task-review` and be integrated before
implementation begins. TASK-006 must then pass its own `$wyrd-task-review`
before the final `$wyrd-change-review`. Execute with `$wyrd-implement`;
UI edits additionally require `$wyrd-ui`.

## Owners, Scope, Consumers, and Prohibited Changes

Rename `vala-eval` to `vala-verify`. Move Verifier-used shared contracts from
`wyrd_spec::vala::eval` into existing `wyrd_spec::verification`, and rename
`EvalContextCapture` to `ContextCapture`. Keep Vala's engine ownership, pure
`wyrd-spec` contracts, the shared `wyrd-client` SDK boundary and server-owned
hosted behavior. Do not introduce another engine, scheduler, result store,
transport, validator or historical-data service.

Source-backed reuse map (paths describe the inspected pre-cutover tree):

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Generic Verifier execution | `vala-eval/src/blocks/engine.rs::VerificationEngine`; `blocks.rs::ActionExecutor` / `CheckExecutor` | Server `verification/hosted/blocks.rs::HostedBlocks::engine`; engine tests; SDK real-time verification stories | Live Verifier shares modules with obsolete scenario execution | Rename owner; retain live helper closure and remove obsolete workflow | None: move existing behavior |
| Graph, assertion, trace and capture primitives | Engine imports `context::extract_jsonpath_from`, `executor::{Prepared, RunLedger, walk}`, `results::capture_actual`, `tasks::assertion::{evaluate_item_mode, evaluate_single}`, `tasks::trace::project_spans`, `trace_source::TraceSource`, and `operators` | `blocks/engine.rs`, `blocks/scope.rs`, engine tests and `HostedBlocks` | Deleting legacy directories wholesale would delete live behavior | Preserve the called capabilities under the retained engine; delete unused scenario/task orchestration | None: shared helpers already implement required semantics |
| Authoring and result contracts | `wyrd-spec/src/vala/eval/mod.rs`; `verification.rs`; `card/verifier/mod.rs::VerifierSpec` | Engine imports, `verification/execution.rs`, schema generator, server hosted composition | Removed model remains authorable; live types keep old paths | Delete unused Eval specs/protocols and move live IDs/operators/plans/results; regenerate consumers | None: existing verification module is the selected owner |
| CLI | `wyrd-cli/src/cli.rs::{Cli, Command}` dispatches `Command::Eval` into `eval/run.rs` | `tests/cli.rs` includes `eval_support` and `eval_local_records` | Old workflow remains discoverable and executable | Delete command, implementation, obsolete fixtures and registrations; retain ordinary clap refusal | None: parser already rejects unknown commands |
| Permissions | `wyrd-runtime/src/permission.rs::{Resource, Permission}` includes `Evals` / `eval_run` | Permission tests; Rust and TS real-time refusal stories construct an `evals:run` credential; workload-role docs | Old resource still parses and fixtures treat it as valid | Delete resource/helper/grants; retain generic unknown-resource handling and exact Verifier coverage | None: existing permission parser/authorization owns refusal |
| Analytical retirement | `tables/eval/mod.rs::ResultItemsTable`, Gate registration, `vala-sql` Forge identities and migrations | `pg_migration` / `pg_forge_tasks`; Rust/Python/TS SQL stories; MCP `query::pg_tests` | Old namespace can still resolve and is valid maintenance state | Remove registrations/identity support; forward migration removes obsolete task/demand rows and tightens constraints | None: existing catalog, query and migration owners suffice |
| External GenAI projection | `vala/trace/gen_ai_eval_result.rs::GenAiEvalResult`; `gen_ai_span.rs::GenAiSpanRecord.eval_results` | Trace serde/validation tests and canonical GenAI MCP SQL journey | Wyrd abbreviations differ from the preserved external vocabulary | Rename owned projections to `GenAiEvaluationResult` / `evaluation_results`; preserve `gen_ai.evaluation.*` events/attributes and stored OTel content | None: existing projection and serde owners |
| Current consumers and docs | Observe `core/types.ts::{EvalRow, EvalTask}` and mock fixtures; state stub; queue batch-builder rustdoc; architecture and generators | UI fixtures import both types; Python startup prose and queue rustdoc still name removed observations | Stale product/observation instructions imply supported workflows | Remove obsolete mocks/consumers, update surviving contracts, docs and sources | None: consumer closure, no UI redesign |
| Vocabulary boundary | `scripts/checks/deps.sh`, invoked by `mise run check:deps` | Existing dependency rules and spec REQ-022 | Compilation cannot reject a new cross-language permission/table/documentation name | Extend the existing owning check with the approved semantic exclusions | Only the owner-required live vocabulary rule; no checker framework |

Revalidate callers at implementation time. These paths are guidance, not a
private module or symbol allowlist. Retain all live Verifier helpers regardless
of their current directory. Remove the CLI's engine dependency if it becomes
unused; do not retain a dependency merely because the handoff listed it as a
consumer. The same rule applies to every manifest and test-support owner.

No aliases, deprecated exports, renamed copies of obsolete workflows, table
views, permission aliases, old error-code aliases or compatibility routes.
No new Card kind, command replacing scenario simulation, drift algorithm,
runtime feature, dependency, or object-store cleanup workflow. Preserve all
unrelated/current verification evidence and applied migration files.

TASK-005 already removed the LLM judge, `observe.eval`, `observations_ready`,
writer-Card Eval fan-out, `vala.eval.observations`, and old verification-run
contracts/tools. Audit their references; do not recreate or repeat that work.
The separate Oracle root-query memory defect is outside this task. A required
check failing because of it remains an explicit blocker, never a waived gate.

## Approach

1. Reconfirm the live Verifier dependency closure and the obsolete CLI/scenario
   callers. Establish focused refusal tests before changing their owners.
2. Remove obsolete CLI/workflows/contracts/errors and move retained engine,
   pure contracts and faithful GenAI projections to the approved names.
3. Close permissions and analytical retirement through their existing owners;
   add the forward migration and development-schema upgrade proof.
4. Update all necessary consumers, manifests/lockfiles, generators, authored
   Python stubs, UI mocks, examples and architecture/documentation references.
5. Extend the existing vocabulary check using REQ-022's narrow exclusions;
   prove prohibited names fail and legitimate external/ordinary words pass.
6. Run focused behavior, migration, generation, typing and boundary proof;
   record the acceptance matrix and inspect the complete diff.

## Ordered Implementation Scenarios

Commands P1–P10 below are the exact focused proof. New selectors are explicitly
marked planned; their existing packages, targets and parent modules were
inspected. Confirm collection after adding them, and record equivalent updated
selectors if implementation-owned placement changes. Never accept zero tests.

### Scenario 1 — The obsolete command and permissions are unknown

**Behavior.** `wyrd eval` and `wyrd eval run` receive clap's ordinary unknown
command refusal, are absent from help, and perform no work. Both string and
structured `evals` permissions are unknown, including `run` and `invoke`.
Valid exact/global `verifier:run`, unrelated permissions, delegation coverage,
audit and tenant rules retain their semantics (REQ-018/020, INV-003/004/009,
AC-011/012).

**RED.** Add P1 and P2; they fail because the command exists and the resource
still parses. Add the permission portion of P4–P7: exercise the actual public
permission/credential boundary, not only a test fixture's scoped-key parser.
A valid permission for another resource remains a distinct authorization-denial
case. Existing tests that successfully mint `evals:run` are not the new proof.

**GREEN.** Remove the command/workflow and unused permission resource/grants;
adapt affected refusal tests to valid unrelated permissions, and keep removed
names only as explicit unknown-input cases. Preserve ordinary error mapping.
Retire scenario-only contracts and catalog entries as their callers disappear.
Rerun P1/P2 and each newly passing permission journey.

**REFACTOR.** Delete unused dispatch, tests, fixtures, helpers and dependencies.
Reuse the existing parser and authorization owners; add no legacy-specific
error or second verification command.

### Scenario 2 — Retired analytical tables stay unknown across development upgrades

**Behavior.** `vala.eval.result_items` and the already-removed observations
path cannot be listed/described/read/written as supported tables. SQL receives
the ordinary unknown-table error with no rows, including with stale development
catalog state. A fresh migration and an upgrade containing old Forge task and
demand rows succeed; removed namespace rows disappear before constraint
validation, and remaining namespaces and current verification data survive
(REQ-020/021, INV-008/009, AC-012/015).

**RED.** Add P3's upgrade test with existing retired-namespace state and extend
its fresh migration assertion. Add the SQL portions of P4–P7 using real SDKs
and MCP; the current result-items registration admits the removed identity.
Include the public CLI SQL path within P1's compiled-binary coverage when
driving a server query; its no-server command refusal remains independently
executable. Prove current tables still work beside the refusal.

**GREEN.** Retire table/namespace definitions, Gate registration, catalog
resolution support and correlation attributes. Apply a new forward migration
that removes the obsolete Forge task/demand rows and tightens only the owning
constraints. Reject stale registrations through the ordinary current owners.
Remove misleading reserved-table expectations for the deleted observation
path; keep it only as a genuine unknown-table negative input. Run P3–P7.

**REFACTOR.** Delete obsolete table code and maintenance identity variants.
Use the existing migration/catalog/query seams. Introduce no historical-data
reader, archive, quarantine, conversion, object deletion or GC protocol.

### Scenario 3 — The retained Verifier and GenAI projections use their new names

**Behavior.** Every retained Verifier consumer uses `vala-verify`, shared
`wyrd_spec::verification` contracts and `ContextCapture`, with unchanged
execution, capture, verdict, drift and lifecycle semantics. GenAI projections
use `GenAiEvaluationResult` / `evaluation_results` without aliases while
external `gen_ai.evaluation.*` remains unchanged (REQ-019, INV-001/002/005–009,
AC-013/014).

**RED.** The move itself is mechanical and uses regression/static proof, not
a manufactured compile-failure RED. For the changed GenAI serialized field,
add the new-name/old-name assertions to P8b: the current serializer emits
`eval_results`, so the new field assertion fails. Keep the canonical event
assertion in P8c green throughout.

**GREEN.** Move the engine and only its live primitive closure; delete obsolete
scenario execution. Update consumers and generated sources, and change the
owned GenAI projection names. P8a–P8c pass; run P9/P10 and the retained engine
tests. Preserve their assertions with only naming/import updates. If a test
was coupled to an obsolete fixture, preserve its behavioral assertions in the
existing surviving owner rather than deleting the Verifier coverage.

**REFACTOR.** Remove dead exports/modules/dependencies and obsolete rustdoc.
Do not rewrite the engine, change algorithms, create zero-state utility owners,
or change hosted admission, queueing, cancellation, sandboxing or authority.

### Scenario 4 — Current surfaces cannot reintroduce the parallel Eval model

**Behavior.** The one existing owning check rejects prohibited Wyrd vocabulary
in active code, owned identifiers/paths, contract strings, generated surfaces
and current documentation. It permits only REQ-022's semantic exclusions:
external GenAI semantics and faithful expanded projections, ordinary compute
words, external syntax/APIs, unrelated lexical matches, applied migrations,
completed records, and explicit negative/check/historical decision literals.
No broad directory exclusion hides current APIs or positive fixtures
(REQ-017/022, AC-009/011/014/016).

**RED.** Before cleanup, run the added rule through `mise run check:deps` and
record representative real prohibited hits. Use the existing check's smallest
self-check or reversible temporary probes to show that a prohibited name in
each covered surface fails; do not add a script just to test this script.

**GREEN.** Remove the prohibited current names and stale positive prose,
including SDK startup claims and queue rustdoc. Regenerate from source,
update authored stubs and current architecture/reference paths and links,
and remove obsolete UI mocks and their consumers. The check passes and the
same probes still fail; legitimate exception examples pass. Run codegen,
served OpenAPI/MCP catalog proof and touched documentation/type checks.

**REFACTOR.** Keep one rule in the existing boundary workflow, with clear
diagnostics and narrow exceptions. The owner expressly requires this check
despite AGENTS.md §12's default against completed name bans; do not silently
weaken that policy globally or invent a checker framework.

## Acceptance Criteria

| Criterion | Spec obligations | Owner and proof |
|---|---|---|
| Obsolete command/scenario/DAG authoring and execution are gone; no second workflow remains | REQ-018/020, INV-008, AC-011/012 | CLI/contracts/engine; P1, caller/dependency inventory and vocabulary check |
| Old permission strings/resources are unknown; unrelated and exact Verifier permissions still behave normally | REQ-018/020, INV-003/004/009, AC-012 | Runtime permission owner and real public credential boundary; P2, P4–P7 |
| Old table identities are unknown, including stale development registrations, across SDK/MCP and CLI SQL consumers | REQ-020/021, AC-012/015 | Catalog/Gate/query; P1's server SQL case and P4–P7 |
| Fresh and development-upgrade migrations remove only obsolete Eval task/demand state and validate remaining namespaces | REQ-021, INV-008/009, AC-015 | Vala SQL migrations; P3 plus migration module proof |
| All live Verifier dependencies move and retain their behavioral assertions; no engine rewrite or new durable owner | REQ-019, INV-001–009, AC-013 | Engine/contracts/server/shared client; P9/P10, engine regression and source diff |
| External OTel strings/content stay intact; owned GenAI field/type names change without aliases | REQ-019, INV-009, AC-014 | Trace contracts/projection; P8a–P8c and existing canonical GenAI read proof when touched |
| Generated/served surfaces, SDK exports, UI mocks, examples and current docs close the same vocabulary cutover | REQ-017/019/022, AC-009/010/014/016 | Generators and existing consumer owners; codegen, OpenAPI, MCP catalog, type/docs checks |
| One permanent check catches reintroduction without false positives outside the stated boundary | REQ-022, AC-011/016 | Existing boundary lane; failing/passing probes and final check |

REQ-001–016 remain regression constraints, not reimplementation scope.
INV-001–009 apply throughout; their preservation is established by source
comparison, unchanged retained assertions, focused proof and integrated review.
The original TASK-001–005 evidence remains attached to its reviewed candidates.

## Expected Write Set and Consumer Closure

- Workspace manifests/lockfiles and narrow owning package manifests; retained
  `crates/vala/vala-verify` engine after the directory/package rename.
- `wyrd-spec` Eval/Card contracts, verification contracts, error metadata,
  trace projections, schema generators and affected tests.
- CLI command tree/dispatch, old Eval implementation, tests/fixture includes
  and Eval-only errors/dependencies.
- Runtime permission resource/helpers, built-in grant sources, auth consumers
  and tests; replace stale successful `evals:run` fixtures in all languages.
- Bifrost built-in tables, Gate/catalog/namespace resolution, correlation
  attributes, Vala SQL identities and a new forward migration.
- Server verification, MCP, shared client/queue, testing helpers and Rust,
  Python and TypeScript SDK consumers/exports/tests. Generated TS declarations,
  card/error types and other outputs come from their owning generators.
  Python `.pyi` files remain authored sources.
- UI observe feature/mock consumers; docs, architecture, doctrine, applicable
  reference names and incoming links, examples, and `scripts/checks/deps.sh`.

Paths are guidance, not an allowlist. Preserve other active work and immutable
review reports. The separate `remove-eval-naming` draft has already been
absorbed and deleted; do not create another change packet. TASK-005's old
offline-retention sentence is superseded by revision 10 and this task; do not
rewrite its historical evidence to claim it performed TASK-006.

## Verification and Evidence

Run every named test below with its exact selector. P1–P7 add focused cases in
inspected existing targets; the names below are planned, not claims that tests
already exist. Use repository-managed Postgres and real language runtimes.
P8a–P10 name existing tests, with P8a/P8b's projection module/test spelling updated
by the approved rename. Build the Monty worker with
`mise exec -- cargo build --locked -p wyrd-monty-worker` before hosted execution
proof; run `mise run py:setup`, `mise run ts:build` and
`mise run ts:build:testing` before focused owning-language journeys.

| Proof | Exact focused command |
|---|---|
| P1a — planned compiled-binary command/help refusal, without a server | `mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=retired_command_is_unknown)'` |
| P1b — planned compiled-binary authenticated SQL refusal | `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=retired_tables_are_unknown)'` |
| P2 — planned permission string/structured refusal and retained grant coverage | `mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(=permission::tests::retired_permission_is_unknown)'` |
| P3 — planned exact forward upgrade with old state | `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-sql --test integration -E 'test(=pg_migration::pg_tests::retired_verification_namespace_upgrades)'` |
| P4 — planned Rust SDK public-boundary negative journey | `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=verify_in_real_time::retired_verification_contracts_are_refused)'` |
| P5 — planned Python public-boundary negative journey | `mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/test_verify_in_real_time.py::test_retired_verification_contracts_are_refused` |
| P6 — planned TypeScript public-boundary negative journey | `mise exec -- scripts/postgres/with-test-postgres.sh -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/verify-in-real-time.test.ts -t '^retired verification contracts are refused$'` |
| P7 — planned MCP public-boundary negative journey and catalog assertion | `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(=query::pg_tests::retired_verification_contracts_are_refused)'` |
| P8a — renamed existing GenAI result roundtrip | `mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=vala::trace::gen_ai_evaluation_result_tests::gen_ai_evaluation_result_round_trip_full)'` |
| P8b — existing span projection roundtrip with renamed field and assertions | `mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=vala::trace::gen_ai_span_record_tests::gen_ai_span_record_evaluation_results_round_trip_with_two_entries)'` |
| P8c — unchanged canonical external event | `mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=vala::trace::attributes_tests::gen_ai_evaluation_event_name_is_canonical)'` |
| P9 — existing no-step assertion/trace engine regression | `mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::engine::tests::no_step_verifier_judges_inputs_and_traces)'` |
| P10 — existing background SDK execution/reconnect regression | `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=verify_in_real_time::submitted_answer_is_judged_in_the_background_and_resumes_by_id)'` |

P1a must demonstrate no connection attempt; P1b covers CLI queries through
an authenticated real server. P1 refers to both focused cases above.
For malformed permissions, use the real exposed parser/credential boundary;
do not widen a typed SDK API or add a production raw transport merely to send
invalid values. Public runtime rejection may happen before IO. Independently
prove server authentication/decoding refusal with the existing authenticated
wire test support; a harness scoped-key parser failure alone proves neither
the server nor the journey. Retain valid SDK → server → SDK calls beside the
refusals so local validation does not replace cross-boundary proof.

Additional task-scoped proof:

- Retained engine regression: `mise exec -- cargo nextest run --locked -p
  vala-verify --lib`. Contract/permission modules: `mise exec -- cargo nextest
  run --locked -p wyrd-spec --lib` and `mise exec -- cargo nextest run --locked
  -p wyrd-runtime --lib -E 'test(/^permission::tests::/)'`.
- Migration module (fresh/idempotent plus upgrade): `mise exec --
  scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p
  vala-sql --test integration -E 'test(/^pg_migration::pg_tests::/)'`.
- Served verification OpenAPI assertion, extended to reject prohibited names:
  `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run
  --locked -p wyrd-server --test integration -E
  'test(=pg_openapi_contract::verification_contract_publishes_five_typed_operations_and_the_local_upload)'`.
  P7 inspects the real MCP catalog; generation is not catalog proof.
- Static/generated/documentation work needs no manufactured RED:
  `mise run codegen:regen`, `mise run codegen:check`, `mise run check:deps`,
  `mise run check:tenant-isolation`, `mise run docs:check` and
  `mise run check:examples` for the touched example closure.
- Touched-language verification: `mise run fmt`, `mise run lints`,
  `mise run py:format`, `mise run py:lints`, `mise run py:typecheck`,
  `mise run ts:format:check`, `mise run ts:lints`, `mise run ts:typecheck`.
  Run `mise run ts:napi:check` after any N-API declaration change.
  The UI has no dedicated mise check task: use `mise exec -- pnpm --dir
  crates/wyrd/wyrd-server/wyrd-ui run check`, following `$wyrd-ui` for any
  further focused consumer proof required by the actual edit.
- Record removed exported symbols/catalog entries and the absence of callers,
  generated old names, stale positive docs and unused dependencies. For changed
  authored exports, run their owning runtime's focused export/typing proof.
  Follow AGENTS.md's Python unit lane when Python executable code changes.
- `git diff --check` and a final tracked/untracked diff audit. Evidence must
  distinguish checks actually run from planned commands and known blockers.

Full journey suites and integrated aggregate verification belong to change
review; do not run them as task verification or claim this focused set replaces
their spec acceptance obligations. Diagnose unexpected failures with tracing
before editing; follow `$wyrd-implement`'s independent diagnosis rule where
triggered. Keep the separate Oracle diagnosis outside this write set.

Append the implementation acceptance/evidence matrix and record non-goals,
exact command results, material limits, and the immutable candidate identity
for task review. Planning has run no implementation tests.

## Material Stop Conditions

- Retaining the Verifier requires changing behavior, durable payload semantics
  beyond approved naming, resource ownership, auth/tenancy, sandboxing,
  queue/replay or cancellation rather than moving current behavior.
- Deletion would remove a live non-obsolete caller's capability not covered
  by the approved cutover; enumerate that caller and required outcome.
- Upgrade requires deleting current verification/unrelated evidence, changing
  applied migrations, or introducing recovery/object-deletion machinery.
- A real external standard/dependency identifier cannot be preserved within
  the explicit exception; do not rewrite standards to satisfy a substring ban.

Report such evidence for specification authority. Ordinary helper placement,
file layout, focused-test fixture choice and equivalent exact commands remain
implementation-owned. No renewed naming or compatibility approval is needed.

## Authority Links

- [Approved spec revision 10](../spec.md), especially REQ-017–022,
  INV-001–009 and AC-009–016.
- [TASK-005](TASK-005-server-integration.md); its retention decision is
  explicitly superseded by revision 10, not a constraint on this task.
- [AGENTS.md](../../../../AGENTS.md), especially §§2, 5, 8–12, 14–16;
  [agent rules](../../../../architecture/agent-rules.md).
- [Wyrd design](../../../../architecture/wyrd-design.md),
  [doctrine](../../../../architecture/wyrd-doctrine.mdx),
  [Bifrost design](../../../../architecture/bifrost-design.md),
  [reference router](../../../../architecture/references/README.md), and
  [current evaluation reference](../../../../architecture/references/domain/evaluation.md)
  (update its current name/links as part of the cutover).
- [Spec-driven development](../../../../architecture/references/languages/spec-driven-development.md),
  [implementation execution](../../../../architecture/references/languages/implementation-execution.md),
  [testing workflows](../../../../architecture/references/languages/testing-workflows.md).

## Implementation Evidence

Status: `IMPLEMENTED`. Candidate: commits `f711c6834`..`72d0fe4e0` on
`change/verifier-building-blocks-task-005` (the evidence commit follows).

### Acceptance matrix

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| AC-009 consumers and docs | Live contracts in `wyrd_spec::verification` (`ContextCapture`); unused Eval specs, errors, OpenAPI `Eval` tag, UI Evaluations routes/mocks, Python CLI eval tests and their fixtures removed; design, doctrine, references, docsite, skills, brand renders reworded; TS card/error types regenerated | `codegen:check`, `docs:check`, `check:examples`, `py:typecheck`, `ts:typecheck`, `ts:napi:check`, UI `pnpm run check` (0 errors / 724 files), served OpenAPI test | PASS |
| AC-010 gates | — | `fmt`, `lints`, `py:format`, `py:lints`, `ts:format`, `ts:lints`, `check:deps`, `check:tenant-isolation`, `codegen:check`, suites below | PASS |
| AC-011 vocabulary rule | `scripts/checks/deps.sh` vocabulary block: one regex over current surfaces; exempts `changes/`, applied migrations, refusal-test files, line-scoped refusal literals in production files with inline tests, and shell `eval`/`literal_eval`/`eval_metric`; authorities record the owner exception (AGENTS.md §12, architecture-constraints, testing-workflows) | RED probes each failed: `EvalScore` (Rust), `"evals:run"` (TS), `"vala.eval.runs"` (Python), `wyrd eval` (docs), `WYRD_EVAL_404_TASK_NOT_FOUND` (Rust), `// Resource::Evals` appended to the line-exempt `permission.rs`; legitimate-word probe (`evaluate_operator`, `retrieval`, `revalidate`, `WireValue`, `RETRIEVAL`, `gen_ai.evaluation.result`, `GenAiEvaluationResult`, `evaluation_results`) passed; `mise run check:deps` PASS | PASS |
| AC-012 refusals | CLI command deleted; `Resource::Evals` deleted; `vala.eval` tables/namespace removed; `list_tables` hides stale catalog rows | P1a, P1b, P2, P4, P5, P6, P7 | PASS |
| AC-013 retained behavior | `vala-eval` renamed `vala-verify`, live engine closure kept | P9, P10, `vala-verify --lib` 49/49 | PASS |
| AC-014 generated/served | `GenAiEvaluationResult` / `evaluation_results`; served OpenAPI rejects retired identifiers (RED: restoring the removed `Eval` tag failed the test; "Evaluation" passed) | P8a–c, `wyrd-spec --lib` 758/758, `test:principals:integration` PASS, P7 catalog assertion | PASS |
| AC-015 migration | `20261016000000_retire_eval_namespace.sql` deletes eval Forge claims/tasks, then re-adds the namespace constraint without `vala.eval` | P3, migration module 3/3 | PASS |
| AC-016 prose | Docs/architecture/UI mocks/SDK prose swept; vocabulary rule passes repository-wide | `docs:check`, `check:deps` | PASS |

### Commands run (all exit 0)

| Proof | Result |
|---|---|
| `cargo build -p wyrd-monty-worker`, `py:setup`, `ts:build`, `ts:build:testing` | built |
| P1a `retired_command_is_unknown` | 1 passed |
| P1b `retired_tables_are_unknown` | 1 passed |
| P2 `permission::tests::retired_permission_is_unknown` | 1 passed |
| P3 `pg_migration::pg_tests::retired_verification_namespace_upgrades` | 1 passed |
| P4 Rust `verify_in_real_time::retired_verification_contracts_are_refused` | 1 passed |
| P5 Python `test_retired_verification_contracts_are_refused` | 1 passed |
| P6 TS `retired verification contracts are refused` | 1 passed |
| P7 MCP `query::pg_tests::retired_verification_contracts_are_refused` | 1 passed |
| P8a / P8b / P8c GenAI projection and canonical event | 1 passed each |
| P9 `blocks::engine::tests::no_step_verifier_judges_inputs_and_traces` | 1 passed |
| P10 `submitted_answer_is_judged_in_the_background_and_resumes_by_id` | 1 passed |
| `vala-verify --lib` / `wyrd-spec --lib` / `wyrd-runtime` `permission::tests::` | 49 / 758 / 27 passed |
| `pg_migration::pg_tests::` module | 3 passed |
| `pg_openapi_contract::verification_contract_publishes_five_typed_operations_and_the_local_upload`; `test:principals:integration` | passed |
| `wyrd-cli --test cli` `loader::` / `card_lifecycle::` (renamed fixtures); `wyrd-config --lib` | 2 / 19 / 47 passed |
| `py:test:unit` | 633 passed |
| `fmt`, `lints`, `py:format`, `py:lints`, `py:typecheck`, `ts:format`, `ts:lints`, `ts:typecheck`, `ts:napi:check` | pass |
| `codegen:regen`, `codegen:check`, `check:deps`, `check:tenant-isolation`, `docs:check`, `check:examples` | pass |
| UI `pnpm --dir crates/wyrd/wyrd-server/wyrd-ui run check`, UI `pnpm test` (178 tests) | pass |

### Non-goals and limits

- Non-goals held: no engine rewrite, algorithm change, compatibility alias,
  permission alias, table view, historical reader, or checker framework.
- P2's RED was not captured before the resource was deleted; the test proves
  the refusal on the final tree.
- The migration's Forge-claim delete path has no seeded claim in P3; the task
  and constraint paths are covered.
- A stale role holding `evals:*` fails token issuance as
  `WYRD_PERMISSION_500_ROLE_CORRUPT`, which the OAuth `/auth/token` mapping
  renders publicly as `WYRD_SPEC_500_INTERNAL`; the journeys assert that
  public code.
- Retired-table SQL reports the same refusal as a never-existing namespace
  (`vala.absent`), which is `WYRD_VALA_400_QUERY_INVALID_SQL`, not a 404; the
  journeys assert equality with that baseline.
- Correction (TASK-006-R1): `ts:format:check` does exist. The `ts:format`
  run recorded here was write mode, not a check-mode execution; its only
  change was committed, and R1 records the check-mode run. Full `test:sql` and full journey suites were not run; they belong
  to change review.
- Brand SVG text renames were not visually inspected for overflow.


## Spec revision 11 note

The owner removed the repository-wide vocabulary scan from `check:deps` (`10fa9269c`) and approved spec revision 11, which drops the permanent check from REQ-022/AC-011. Evidence above for the `check:deps` vocabulary block and its probes is historical; the served OpenAPI assertion in `pg_openapi_contract.rs` (`retired_vocabulary`) and review are the remaining enforcement. After removal, `mise run check:deps` and `mise run docs:check` pass.
