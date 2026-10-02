# Independent cold rehearsal — Revision 12

Identity: fresh agent `/root/revision12_readiness/cold`. Inputs: approved Revision 12 specification, TASK-001 carry-forward, TASK-002-cleanup, TASK-003–005, repository authority and source. Static only: no builds, test execution, or implementation edits. `.codegraph` is absent, so source discovery used rg/read operations.

## Located owners and first steps

- Cleanup: existing `wyrd-loader::load`; `CardGraphHydrator` and `GraphTraversal`/`resolve_graph`; Cards handle selectors; Skald `Workflow::from_card_with_agent_resolver`, `ResolvedGraph::resolve`, `run`/`run_with_options`; server `EffectiveSpecs::validate_workflows`; stopped `workflow_loader.rs`. First steps are in-memory extension of existing graph ownership, synchronous Skald declarative lowering/validation, thin client facade, and replacing server/client consumers before removal. Public Python wrapper is `sdks/wyrd-sdk-python/src/workflow.rs`; existing static load uses native ToolResolver/PromptResolver; TypeScript native bindings and public integration layout supply the Node boundary. Rust SDK package is `wyrd-sdk-rust`, import name `wyrd_sdk`; integration targets are auto-discovered.
- TASK-003: `WyrdClient::submit_idempotent`, crate-private `HttpTransport::request_json_with_headers`, GlobalConfig, native provider error categories, and public gateway ingress owners are present. First steps compose selected execution dependencies in shared client, then add transport-only lifecycle facade and immutable gateway call projection. Local configuration selection precedes secret resolution; loading remains secret-free.
- TASK-004: reviewed explicit admission/replay/capacity/authority/query-owner/shutdown sequence and config types. Source seeds are existing Cards effective-body resolution, audit authorization, GatewayInvocation, Caller, MCP query collection, RunningQueryControls, and BoundServer shutdown. Server manifest has test-support feature. First step is bounded run owner and permission/config/route wiring; then tracked preparation and production-shaped PG lifecycle tests. No durable queue or Workflow principal is required.
- TASK-005: CLI manifest explicitly has autotests=false and `cli` target. Current gateway journey module is the fixture precedent. First steps add command/selectors, project shared APIs, wire Workflow tests into `tests/cli.rs`, and extend cross-language journeys. CLI invocation owns connection overrides while graph/execution machinery stays in shared owners.
- TASK-001: carry-forward explicitly preserves native runtime obligations, binds historical evidence to its candidate, requires outstanding findings to close, and reopens changed invariant-bearing code. No new acceptance is inferred.

## Remaining choices

Existing-owner internal methods, authored provenance input representation, facade field layout, private synchronous declaration adapters, native wrapper mechanics, and fixture layout are reversible implementation choices within named owners. No remaining material API, identity, persistence, authorization, or lifecycle decision was found in the reviewed Revision 12 seams.

## Verification inspection

The Python focused recipe selects the existing Cards CRUD integration file; its aggregate lane selects that same file. TypeScript build/testing tasks and Vitest integration directory are valid current definitions. Planned SDK tests are correctly marked planned: their eventual selection remains implementation evidence, not current proof. Rust SDK ignored journey is correctly identified as outside the lib-only SDK lane. CLI ignored tests explicitly require wrapper/selection and do not rely on ordinary family selection.

### COLD-001 — Major: omitted exact UID replacement regression

Location: TASK-002-cleanup Scenario 2, RED and verification section.
Type: INVALID_VERIFICATION_RECIPE.
Issue: the task attributes UID-race assertions to `registers_only_valid_explicit_workflow_graphs`, but that test does not contain the write-time replacement race. The race is the separate existing `refuses_stale_preflight_after_dependency_replacement`, `crates/wyrd/wyrd-server/tests/pg_workflow_registration.rs:873`.
Impact: cleanup materially replaces graph/server validation machinery but its specified proof does not execute the exact UID fence it requires preserving.
Evidence: the two focused cleanup registration commands select only `registers_only_valid_explicit_workflow_graphs` and `fetches_and_executes_locked_workflow_graph`. `mise.toml` test:cards:integration:inner selects pg_cards_register, pg_card_registration_route, pg_verification_routes, and verification_run, not pg_workflow_registration. Broad later closeout is not cleanup acceptance proof.
Required edit: correct attribution and add the exact repository-managed command:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test pg_workflow_registration -E "test(=refuses_stale_preflight_after_dependency_replacement)"'
```

Retain the assertion that stale validated UID A replaced by UID B refuses with no durable commit, and a fresh request validates B independently.

## Task compilation disposition

| Task | Scope/owners/interfaces/order/acceptance/tests/adaptation | Verification | Rehearsal |
|---|---|---|---|
| TASK-001 carry-forward | PASS as unchanged candidate-bound scope | Historical evidence, no newly executed proof | PASS static carry-forward |
| TASK-002-cleanup | PASS | FAIL: COLD-001 | FAIL until exact race recipe added |
| TASK-003 | PASS | PASS static recipes; planned selectors unexecuted | PASS static |
| TASK-004 | PASS | PASS static recipes; planned selectors unexecuted | PASS static |
| TASK-005 | PASS | PASS static recipes with explicitly separate ignored journeys | PASS static |

Static limitation: this is a focused independent rehearsal record, not runtime verification or a standalone integrated readiness verdict. Parent review must integrate system/security findings and validate the final reviewed packet after corrections.

## Re-review of repaired packet

COLD-001 is **resolved**. Source inspection confirms Scenario 2 now attributes sibling/external collision assertions to `registers_only_valid_explicit_workflow_graphs` and separately requires `refuses_stale_preflight_after_dependency_replacement` through the exact Postgres-wrapped nextest command. Static `bash -n -c` validation of the extracted command succeeds; its test name matches current source. No tests or builds were run.

For the repaired candidate, TASK-002-cleanup verification recipe and static cold rehearsal are **PASS**. The earlier FAIL rows above document the original discovery, not the final repaired disposition. All five task rehearsals are PASS with the unchanged static limitation.
