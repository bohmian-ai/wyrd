---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-agent-readiness
spec_revision: 2
requirements: [REQ-002, REQ-003, REQ-004, INV-001, INV-002, INV-003, INV-004, INV-005, AC-002, AC-003, AC-004, AC-005, AC-011]
depends_on: []
---

# Make the Card lifecycle declare, get and pull

## Outcome and Value

An agent locally checks and declares a Card graph, reads one envelope with
get, and pulls a complete runnable bundle through consistent public contracts.
The old public verbs and stored declaration/bundle names have no aliases.
This task owns the entire lifecycle consumer closure so SDK, CLI, wire, SQL,
audit and bundle readers do not require intermediate compatibility machinery.

Execute with `$wyrd-implement`. Also use `$wyrd-ui` for the existing UI's Card
declaration wording. Independent task review follows implementation. TASK-002
owns the common CLI JSON/error-stream contract; TASK-003 owns onboarding prose
and guide `start` after the final names and machine output exist.

## Owners, Scope, Consumers, and Prohibited Changes

Paths and baseline symbols identify inspected owners, not a private layout
prescription. Revalidate them against the implementation checkout.

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Local check/declaration | `wyrd-loader::load/build_registration_input`; CLI `card.rs::plan_tree/dispatch_plan/apply`; `Cards::register_from_path/register/register_with_progress` and existing saga | CLI `card_lifecycle` local-reference/artifact refusals and real lifecycle; Rust/Python/TS lifecycle stories | Old verbs; local checking is a separate plan command | Reuse loader/check path under `declare --check`, and rename shared declaration workflow through consumers | None; no new loader, validator, traversal or transport |
| Durable declaration | `wyrd-spec::registry::{CreateCardRequest,CreateCardResponse,RegistrationReceipt,RegistrationReplaySeed}`; server Card routes/registry engine; SQL `commit_registration_operation` | Server authenticated replay and served-OpenAPI tests; SQL migration-shape and subject/actor replay tests | Registration names/values in wire, SQL and retained audit | Change declaration projections and schema sources in place, preserving transaction, artifact and replay semantics | No new store, migration/compatibility layer or orchestration |
| Get/pull | `Cards::get/hydrate`; `CardGraphHydrator`; `registry/hydrated.rs`; `WyrdState::from_path`; CLI `get/load` | CLI complete/metadata bundle journey; three SDK graph/load stories | CLI get downloads; bundle names say hydration; CLI load materializes only one Card | Get reads the envelope; reuse graph downloader as pull with the spec's manifest types/fields/discriminator; remove public CLI load | Existing staged, integrity-checked publication is the complete download owner |
| MCP exact reads | Server `mcp/cards.rs::{GetCardArgs,GetCardByRefArgs,descriptors}` delegates to existing Card route operations | MCP `verification::pg_tests::agent_discovers_service_and_component_cards` | Two public read names | One get descriptor/input with exact-ref or kind-qualified UID selectors; preserve domain authorization | No separate read engine or permission shortcut |
| Runtime/language projections | Python `PythonCardRegistry`, `RegistryGetQuery` and per-kind get/declare wrappers; TS native `cards.rs` and public `Cards`; Rust reexports; CLI testing projections | Python generic/per-kind CRUD and state tests; SDK lifecycle stories; CLI embedding callers | Old methods/types; Python per-kind get can eagerly download artifacts | Project final lifecycle contracts; make every get artifact-free, retain runtime object loading through existing state/materialization owners | No new public load replacement or language-specific durable semantics |
| Human-facing vocabulary | UI Cards inventory route, `ServiceDefinition.svelte` and Card mock descriptions | Current UI text says registration is API-first and includes MCP declaration | Stale declaration wording/surface claim | Change only Card declaration wording and affected contract consumption; CLI/SDK declare, MCP inspect/verify | No UI feature, styling redesign or client-side registry logic |

Follow the exact command/type/manifest and persisted rename contracts in
REQ-002/003/004. Include per-kind Python registries, progress/receipt/input
types, SQL constraints/indexes/RLS/FKs, replay seed JSON, audit discriminators,
CLI embedding and testing exports, generated types and public errors. Keep
`POST /v1/cards`, registry nouns, independent lifecycle/idempotency outcomes,
and runtime object hydration distinct. Python Model/Data eager get branches
must not remain artifact-download exceptions to the approved get contract;
retain their useful runtime-loading coverage at the existing loading boundary.

Only mechanical consumer and authoritative vocabulary updates belong here.
Update architecture's declaration/read/bundle descriptions and stale
`wyrd apply -f` spellings. Keep current guide references executable when get
tools merge; do not add `start` or rewrite the onboarding workflow yet.
Necessary edits to example calls/snippets keep them compiling; TASK-003 owns
the published recipe and its additional proof. Historical change records and
unrelated runtime/module registration are not rename targets.

No MCP declaration/upload/check tools, server-aware local check, data migration,
old-format readers, aliases, new runtime, credential changes, detached server
mode, sibling-repository edits or unrelated UI work. You are not alone in the
codebase: preserve others' edits and adjust to the inherited candidate.

## Approach

1. Pin the local check and complete lifecycle scenarios using existing owners
   and tests; record baseline selectors before any rename.
2. Replace declaration contracts, server/SQL/audit projections and client/SDK
   methods together, preserving the whole existing artifact/replay workflow.
3. Separate get from pull across CLI/SDK/MCP, replace bundle contracts and
   remove public CLI load while preserving runtime object loading.
4. Close exports, generated types, fixtures, mechanical docs/UI references and
   all compile-time/runtime consumers; regenerate from sources.
5. Run the focused proof below, audit the diff and append acceptance evidence.

## Ordered Implementation Scenarios

P1-P12 below identify current test owners and exact baseline commands. They
are not a requirement to preserve old test/module names. Where vocabulary
changes a selector, confirm the new collected identity and record/run its
exact equivalent; a zero-test match never counts as proof.

### Scenario 1 — Check local files without contacting a configured server

**Behavior.** `declare PATH --check --format json` reports candidates and
diagnostics locally, including missing/unreadable transitive files, artifact
sources, malformed refs and cycles. It makes no server contact with or without
configured credentials/endpoints. Old apply/plan commands are refused
(REQ-002/003, AC-002).

**RED.** Extend P1/P2 with the final command, unavailable/local-listener server
configuration and disk-error cases. Current parser lacks declare/check. Assert
stable located error metadata and no connection; use no credentials or server
fixture for this offline behavior. Add the needed transitive-file cases at
the current loader boundary, recording any new exact selectors after collection.

**GREEN.** Expose the existing local load/validation projection as check,
validate all required local sources before declaration and retain authoritative
server reference checks only at real declaration. Pass P1/P2 and earlier cases.
Keep diagnostics structured; TASK-002 completes common stream formatting.

**REFACTOR.** Delete obsolete plan/apply dispatch and duplicate wrappers;
reuse the loader and shared declaration owner rather than adding a checker.

### Scenario 2 — Declaration retains durable identity and replay semantics

**Behavior.** The final CLI/SDK calls and HTTP request/response/schema names
agree with persisted declaration/audit names and `declared` outcomes. Exact
references, versioning, principal identity, artifact completion, tenancy,
idempotent replay/conflict and independent lifecycle outcomes keep their
semantics (REQ-003, AC-002/003).

**RED.** Change expected public/SQL/replay/audit projections in P3/P4/P5 and
the real declaration portion of P6/P10-P12. Baseline names/values must fail the
new expectations. Retain denial/conflict assertions and inspect stored replay
seed data rather than assuming it is the complete HTTP response.

**GREEN.** Rename the owning contracts and every necessary consumer without
aliases or conversion, including per-kind registries and language testing
exports. Update existing schema sources in place as approved. Regenerate the
outputs and rerun the focused scenarios, including local check regressions.

**REFACTOR.** Remove obsolete names and mappings, not error handling or saga
stages. Keep durable policy/auth/SQL behavior with its existing server owners.

### Scenario 3 — Get reads one envelope through every surface

**Behavior.** CLI/SDK get fetches one envelope without artifact IO. MCP get
accepts either supported selector, rejects ambiguous combinations and preserves
missing/foreign-tenant/underprivileged refusal behavior. There is no get-by-ref
alias (REQ-004, AC-004).

**RED.** Extend P6/P7/P8 with no output-directory get, both MCP selectors,
invalid combinations and no artifact requests. Extend Python per-kind coverage
at P9 to catch eager-download behavior as well as generic get; baseline CLI,
catalog and eager-get expectations conflict with this contract.

**GREEN.** Delegate envelope reads to the current get owners and merge only
the MCP public selector/descriptor projection. Remove get's download behavior
and unsupported eager-download parameters; keep useful object-loading behavior
through the existing state-loading boundary. Rerun earlier declaration proofs.

**REFACTOR.** Reuse shared selector/authorization logic and delete the second
tool path. Do not add a public replacement command or a parallel reader.

### Scenario 4 — Pull publishes a runnable graph and load is no longer a command

**Behavior.** Pull publishes the complete exact graph with verified payloads
and the new bundle contract; all SDK state loaders consume it. Metadata-only
is inspectable and rejected for runtime use. Corrupt/missing bytes do not
replace a good bundle or become success. Old download verbs, metadata format
and public CLI load have no compatibility path (REQ-004, AC-005).

**RED.** Extend P6/P8/P10-P12 to use pull and inspect the new manifest,
artifact integrity, offline loading and metadata-only rejection. Pin old
command/API/format refusals. Retain the current failed-publication and runtime
hydration assertions, replacing only assertions for deliberately removed
public behavior. The baseline exposes hydrate and the old manifest.

**GREEN.** Project the existing staged graph downloader through pull and
change its manifest readers/writers together. Remove public CLI load/exports,
retaining necessary internal materialization. Rerun all prior scenario proofs.

**REFACTOR.** Delete obsolete projections and use the existing atomic bundle
publication and runtime loaders; add no new graph format adapter or traversal.

## Acceptance Criteria

| Criterion | Spec obligations | Proof |
|---|---|---|
| Local check validates disk references with no server contact; real declare remains authoritative | REQ-002, AC-002 | P1/P2 plus collected local-file edge cases and actual CLI declaration in P6 |
| Public declaration names and in-place SQL/replay/audit agree; old names have no aliases | REQ-003, AC-003 | P3-P6/P9-P12, runtime exports, source review and generated/served contracts |
| Get is one envelope without artifacts; merged MCP get preserves selectors and refusals | REQ-004, AC-004 | P6-P9 and language get assertions in P10-P12 |
| Pull yields complete loadable bundles; metadata-only and failures remain safe; public CLI load removed | REQ-004, AC-005 | P6/P8/P10-P12 and retained integrity/refusal coverage |
| Existing ownership, auth/tenancy, runtime loading and durability remain | INV-001-005, AC-011 contribution | Focused regression, boundary/source review and recorded non-goal closure |

## Expected Write Set and Consumer Closure

- `crates/wyrd-spec` declaration, bundle, audit and applicable error sources;
  `wyrd-loader`; shared `wyrd-client` Cards/saga/reads/download/state owners.
- `crates/wyrd/wyrd-sql` declaration schema/query sources; server Card routes,
  registry/audit projections, MCP Card reads and all direct consumers.
- CLI Card parser/dispatch/progress/embedding and testing projections; all
  three SDK native/public exports, authored stubs, declarations and tests.
- Source-generated contracts; fixtures and in-repository examples needed by
  the changed calls; owning architecture vocabulary and mechanical current
  docs/guide references. No onboarding rewrite or `start` topic yet.
- Existing UI Card declaration wording/contract consumers only, under
  `wyrd-ui`; do not rename unrelated workspace registration or redesign UI.
- Relevant test selectors and mise references if their owning files/modules
  change. No arbitrary new lane, permanent old-name grep gate or review packet.

## Verification and Evidence

The current targets were inspected: CLI target `cli` includes
`card_lifecycle::pg_tests`; server/SQL/Rust SDK targets are `integration`;
MCP target `mcp` includes `verification::pg_tests`. Build the required installed
CLI/native bindings through the existing repository build/setup tasks before
their proof. Keep Cargo-backed invocations sequential. The wrapper provides
isolated Postgres; fixtures run their own owning migrations. Preserve any
existing source-declared test environment requirements.

| Proof | Exact focused baseline command |
|---|---|
| P1 local check projection | `mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=card_lifecycle::plan_is_local_and_returns_machine_readable_card_projection)'` |
| P2 missing local reference / artifact source | `mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=card_lifecycle::invalid_local_reference_returns_card_load_error_without_credentials) or test(=card_lifecycle::apply_rejects_an_unreadable_artifact_manifest_before_authentication)'` |
| P3 SQL schema and replay attribution | `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-sql --test integration -E 'test(=pg_cards_register::consolidated_registration_migration_has_locked_shape) or test(=pg_cards_register::subject_actor_creation_and_replay)'` |
| P4 authenticated declaration/replay | `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_card_registration_route::registration_replays_through_public_authenticated_route) or test(=pg_card_registration_route::non_active_and_cross_tenant_dependencies_leave_no_writes)'` |
| P5 served Card contract | `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_openapi_contract::card_contract_publishes_typed_lifecycle_and_problem_shapes)'` |
| P6 actual CLI lifecycle | `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=card_lifecycle::pg_tests::card_lifecycle_cli_journey)'` |
| P7 MCP exact reads/catalog/denials | `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(=verification::pg_tests::agent_discovers_service_and_component_cards)'` |
| P8 complete/metadata-only CLI bundle | `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=card_lifecycle::pg_tests::multi_card_service_get_hydrates_complete_and_metadata_bundles)'` |
| P9 Python per-kind declaration/get/runtime boundary | `mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/cards/test_cards_crud.py::test_prompt_cards_register_get_list_resolve_latest_and_delete tests/integration/cards/test_cards_crud.py::test_data_card_custom_interface_get_requires_interface_and_loads_artifacts tests/integration/cards/test_cards_crud.py::test_model_card_custom_interface_get_requires_interface_and_loads_artifacts` |
| P10 Rust graph and exact kind reads | `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=register_and_hydrate::service_graph_registers_and_hydrates) or test(=register_and_hydrate::cards_get_returns_every_kind_typed)'` |
| P11 Python graph and exact envelope reads | `mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/test_register_and_hydrate.py::test_service_graph_registers_and_hydrates tests/integration/test_register_and_hydrate.py::test_cards_get_returns_every_kind_as_envelope` |
| P12 TS graph and exact kind reads | `mise exec -- scripts/postgres/with-test-postgres.sh -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/register-and-hydrate.test.ts -t '^(service graph registers and hydrates|cards get returns every kind typed)$'` |

All three language proofs are required here because this task changes their
public lifecycle APIs, not as a default request to run unrelated SDK suites.
Record exact collected commands for added/refactored tests, including local
no-contact, integrity and removed-export assertions. Static UI wording and
mechanical documentation changes need no manufactured RED.

Run `mise run fmt`, `mise run lints`, `mise run check:deps`,
`mise run check:tenant-isolation`, `mise run codegen:regen` and
`mise run codegen:check`. For the changed bindings run `mise run py:format`,
`mise run py:lints`, `mise run py:typecheck`, `mise run ts:napi:check`,
`mise run ts:format:check`, `mise run ts:lints`, `mise run ts:typecheck`;
rebuild the native packages with their existing setup/build tasks first.
For the limited UI edits run `mise exec -- pnpm --dir
crates/wyrd/wyrd-server/wyrd-ui check` and the same package's `build`.
Use touched example/static checks as required by the actual closure.

Full journey suites and the final deterministic aggregate belong to integrated
change review, not this task. Append a compact spec-to-evidence matrix, actual
commands/results, any updated exact selectors, non-goal closure and diff audit.
Run `git diff --check`; inspect tracked and untracked changes. No staging,
commit, branch switch or other contributor's revert is authorized.

## Material Stop Conditions

Return to `$wyrd-spec` if correctness needs compatibility readers/migrations,
new public loading semantics beyond the approved get/pull/runtime boundary,
changed authority/versioning/replay/durability or tenant/security behavior.
Report an unavailable required artifact/platform as an evidence blocker; do
not claim publication or weaken proof. Reversible helper layout, test
placement and renamed exact selectors remain implementation-owned. New
dependency/feature or replacement machinery needs the applicable authority;
ordinary cross-surface renames do not require renewed naming decisions.

## Authority Links

- [Approved spec revision 2](../spec.md).
- [AGENTS.md](../../../../AGENTS.md), [agent rules](../../../../architecture/agent-rules.md),
  [design](../../../../architecture/wyrd-design.md), [doctrine](../../../../architecture/wyrd-doctrine.mdx).
- [Router](../../../../architecture/references/README.md), [spec workflow](../../../../architecture/references/languages/spec-driven-development.md),
  [execution](../../../../architecture/references/languages/implementation-execution.md), [testing](../../../../architecture/references/languages/testing-workflows.md).
- [Agent harness](../../../../architecture/references/languages/agent-harness.md), [errors](../../../../architecture/references/languages/errors.md),
  [security](../../../../architecture/wyrd-security-posture.md), [Bifrost](../../../../architecture/bifrost-design.md).
