---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-agent-runtime-composition
spec_revision: 3
requirements: [REQ-012, REQ-013, REQ-014, REQ-015, REQ-016, REQ-017, REQ-018, INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008, AC-007, AC-008, AC-009, AC-010, AC-011]
depends_on: []
---

# Delete public SDK Agent execution and Run invocation

## Outcome and Value

Remove the public executable Wyrd Agent and `Run.invoke`/`Run.invoke_on` from
all three language SDKs. Close exports, native registration, typing, generated
declarations, examples, docs, and actual consumers in the same change.
Application code owns inference; Wyrd retains its existing verification and
assurance boundary. This one task replaces the revision 2 composition task.

Use `$wyrd-implement`. All revision 3 obligations in the metadata belong to this
task. There are no task dependencies and no replacement runtime to build.

## Owners, Scope, Consumers, and Prohibited Changes

- Remove public executable Agent functionality wherever actually exposed.
  Python currently exports the runtime through `wyrd.agent`, root imports,
  and native aggregation. Rust and TypeScript do not have equivalent public
  Agent constructors; prove absence and remove their Run invocation surfaces.
- Remove runtime-exclusive companion APIs, native registration, exports,
  authored stubs, generated declarations, and dependencies/features rendered
  unnecessary by this deletion. Trace their consumers before removing them.
- Keep declarative `AgentCard`/`AgentSpec`, registration, hydration, Prompt
  navigation, verification bindings, and other existing SDK capabilities.
  A shared identifier, error code, or framework trace processor is not an
  executable SDK Agent merely because its name contains Agent or Session.
- Preserve internal Skald execution and Vala/server Verifier execution.
  Do not refactor their lifecycle, entry points, tool binder, workspaces,
  limits, budgets, cleanup, or route restrictions as part of this task.
- Migrate real consumers of the deleted execution APIs to application-owned
  inference through existing gateway/native-client integration surfaces.
  Preserve their caller identity, Run/Card correlation, observations,
  verification, negative flows, and useful assertions.
- Keep existing gateway/MCP authentication, permissions, audit, tenant
  isolation, correlation validation, and provider-credential ownership.
- No aliases, deprecation shims, public Agent/Runner/Prompt-execution
  replacement, client MCP toolset, MCP transport relocation, new server
  execution endpoint, persisted/wire migration, or unrelated API change.
- Paths below identify likely owners and consumer closure, not a private
  implementation allowlist. Private deletion mechanics remain implementer-owned.

### Source-backed reuse map

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Remove public runtime | Python `src/lib.rs`: registration of `wyrd._wyrd.agent`; `python/wyrd/agent/__init__.py` and `.pyi`; root `__init__` exports; Skald `python.rs::python_register` | Python runtime Agent unit tests and Prompt/live-smoke users; Rust SDK `src/lib.rs` and TS `wyrd/src/index.ts` exports | Python ships public construction, execution, tools, callbacks, results, sessions, and test helpers; Rust/TS constructor absence needs preservation | Delete SDK exposure and runtime-only consumer closure; retain internal owner where used | None; do not introduce replacement registration or runtime |
| Remove Run execution | `crates/shared/wyrd-client/src/observe/invoke.rs`: `Run::invoke`, `invoke_on`; Python `src/observe/mod.rs`; TS native `cards.rs::invoke` and public `Run.invoke` | Three support-desk examples, support-desk-app Rust library, SDK gateway-inference/local-development/support-desk journeys; client state tests | SDK Run constructs and executes Skald Agent | Delete implementation and every native/public/typing projection after migrating consumers | None; remove execution glue rather than rename it |
| Preserve application inference and correlation | Native gateway endpoints; existing shared HTTP auth; Python `WyrdClient.http_auth`, TS `httpAuth`; gateway `wyrd-run-id`/`wyrd-card-uid` handling | Rust gateway-inference attribution test; Python/TS lone-run/Card capture tests; stock-client guide and support-desk example journeys | Existing invocation consumers depend on deleted convenience to send calls and attach headers | Use existing application-owned request path and correlation headers in each affected consumer | No transport, correlation store, SDK inference facade, or new runtime |
| Preserve Cards and verification | `WyrdState::agent`; Vala `blocks/agent.rs::AgentExecutor::invoke`; simulator; server `HostedTools::open`/`descriptor` | Register-and-hydrate and Verifier building-block journeys in each SDK | Deletion must not sever declarative Agent or internal execution | Retain existing owners; migrate only consumers actually coupled to SDK runtime | No new owner or binder |
| Remove stale public guidance | Existing SDK exports/stubs/declarations, example consumers, docs runtime/Run invocation examples | `docs/src/content/docs/understand/runs-observations-and-evidence.md`; stock-client guide; three support-desk examples | Published docs currently describe deleted APIs | Update source docs and generators; regenerate native declarations | No permanent name-ban/check framework |

## Approach

1. Trace runtime and Run invocation exports and their actual consumers;
   distinguish removed executable APIs from surviving Cards/internal execution.
2. Prove the affected application journeys through existing gateway request
   paths with preserved identity/correlation before removing invocation glue.
3. Delete SDK runtime exports/registration and Run execution implementations,
   their exclusive helpers, and all public/native/typing projections.
4. Migrate examples, tests, docs, and any remaining consumers; preserve coverage
   of surviving behavior and remove tests solely for the intentionally retired API.
5. Regenerate affected artifacts and run focused journeys, owner checks,
   typing, dependency, package, docs, formatting, and lint proof.

## Ordered Implementation Scenarios

### Scenario 1 — Migrated callers retain correlated application behavior

**Behavior.** REQ-015/016/017 and AC-009/010: the migrated support-desk,
local-development, and gateway callers execute model requests in application
code, preserve outputs and caller/Run/Card capture, and still receive the
server's gateway/subject-authorization refusals. Correlation headers and Wyrd
credentials do not leak to the provider. No executable Wyrd Agent or Run
invocation is needed to complete those workflows.

**RED.** Extend the existing focused migration journeys J-R/J-P/J-T below
with assertions on the application-owned request path and captured Run/Card
identity. Exercise the proposed migration before completing its request/auth/
correlation wiring and record the precise missing-path or missing-correlation
failure. A compiler/import failure or unrelated server setup failure alone
is not behavioral RED. Keep server and provider real/mock respectively as
in the existing journey owner; no new in-process engine substitute.

**GREEN.** Migrate the consumers using existing authenticated gateway request
patterns and existing subject headers. Make the new assertions and earlier
workflow assertions pass, including permission failures and evidence readback.
Then remove the replaced Run invocation implementation and SDK runtime
exposure, and rerun the same focused commands.

**REFACTOR.** Delete stranded runtime-only glue, dependencies, test support,
and stale docs. Reuse existing transport/auth and example patterns without
creating a new SDK execution wrapper. Keep all migration and preservation
journeys green.

## Acceptance Criteria

| Obligation | Required evidence and owner |
|---|---|
| REQ-012/013; AC-007 | SDK maintainers remove public executable Agent and runtime-exclusive companion exposure from public imports, native registration, authored typing, generated declarations, package contents, and docs. Rust/TS existing absence is verification-only. |
| REQ-014; AC-008; INV-005/006/007/008 | Existing Card hydration and Verifier journeys P-R/P-P/P-T pass. Internal Skald/Vala/server owners and their portable resource/permission semantics remain intact. |
| REQ-015/016; AC-009 | No Run invocation method or alias survives in the shared client or any SDK projection. J-R/J-P/J-T prove migrated application inference, outputs, correlation, and refusal behavior. |
| REQ-017; AC-010; INV-001/002/003/004 | External-framework gateway/MCP and telemetry integrations remain supported. Existing owner tests and migrated capture/negative-flow assertions preserve authentication, authorization, tenancy, audit, and secret boundaries. |
| REQ-018; AC-011 | Updated architecture/docs/examples describe surviving APIs, and the actual deletion's typing, generated artifacts, dependency, package, docs, formatting, and lint checks pass. |

## Expected Write Set and Consumer Closure

- `sdks/wyrd-sdk-python`: native aggregation, Agent-runtime exports/package,
  authored stubs, runtime-exclusive boundary helpers, affected tests/typechecks.
  Preserve the distinct `wyrd.cards.agent` contract module. Relocated test-only
  support must remain test-only, not a runtime escape hatch in the shipped wheel.
- `crates/shared/wyrd-client`: Run invocation module/export/callers, state tests,
  and any gateway-caller glue proven exclusive to the deleted path.
- `sdks/wyrd-sdk-rust` and `sdks/wyrd-sdk-ts`: native/public Run methods,
  declaration sources/generated declarations, docs, and affected consumers.
- Existing owner-crate Python aggregation/features only where actually unused
  after SDK deletion. Do not remove Rust Skald symbols needed by internal
  Vala/server execution or unrelated surviving Prompt authoring.
- `examples/support-desk`, `examples/support-desk-app`, other actual runtime
  and invocation consumers, SDK journey support, applicable docs/architecture
  prose, and affected manifest/lockfile/generated-output sources.
- Discover the full consumer closure before deleting. Public server error
  codes still emitted by verification remain valid SDK error contracts.

## Verification and Evidence

The deletion/export/docs/dependency portion uses static absence, compilation,
package, generated-artifact, and regression proof; it needs no manufactured
RED. The executable consumer migration uses the scenario above. Existing
surviving behavior that is already correct receives regression-only proof.
Retire tests only when their sole contract is the intentionally deleted API;
never discard an assertion about a surviving capability to make a check pass.

Commands run at repository root. Sources/manifests confirm Rust package
`wyrd-ai`, integration target `integration`, Python pytest nodes, and TS Vitest
files/titles below. Build prerequisites after native changes are
`mise run py:setup`, `mise run ts:build`, and `mise run ts:build:testing`.
Run Cargo-backed work sequentially, and rerun failures with tracing before
source diagnosis. Confirm each exact selector selects tests.

### Focused migration journeys

J-R/J-P/J-T select only the existing affected workflows. Extend their assertions
and migrate their consumers; no new named test or fixture structure is mandated.
Record any equivalent exact-selector correction if an invocation-specific test
is renamed to describe its surviving gateway contract.

```bash
# J-R: support desk, local workflow, and gateway model/permission behavior
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=support_desk::support_desk_answers_verifies_and_explains_every_request) | test(=local_development::admin_key_completes_the_local_workflow) | test(=gateway_inference::agent_routes_and_pre_run_overrides) | test(=gateway_inference::gateway_call_records_whichever_attribution_it_carries)"'
# J-P
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/test_support_desk.py::test_support_desk_answers_verifies_and_explains_every_request tests/integration/test_local_development.py::test_admin_key_completes_the_local_workflow tests/integration/test_gateway_inference.py::test_agent_prompt_routes tests/integration/test_gateway_inference.py::test_gateway_call_records_a_lone_run_or_card'
# J-T: anchored exact test titles across the three affected files
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/support-desk.test.ts tests/integration/local-development.test.ts tests/integration/gateway-inference.test.ts -t "^(support desk answers, verifies, and explains every request|admin key completes the local workflow|agent prompt routes|gateway call records a lone run or card)$"'
```

### Focused preservation regressions

P-R/P-P/P-T prove declarative Cards and internal Agent verification survive
through the real SDK/server boundary. These are existing tests, not new
capabilities or a requirement to redesign unrelated surfaces.

```bash
# P-R
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=register_and_hydrate::service_graph_registers_and_hydrates) | test(=verifier_building_blocks::policy_review_runs_every_action_and_check_kind) | test(=verify_in_real_time::submitted_answer_is_judged_in_the_background_and_resumes_by_id)"'
# P-P
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/test_register_and_hydrate.py::test_service_graph_registers_and_hydrates tests/integration/test_verifier_building_blocks.py::test_policy_review_runs_every_action_and_check_kind tests/integration/test_verify_in_real_time.py::test_submitted_answer_is_judged_in_the_background_and_resumes_by_id'
# P-T
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/register-and-hydrate.test.ts tests/integration/verifier-building-blocks.test.ts tests/integration/verify-in-real-time.test.ts -t "^(service graph registers and hydrates|policy review runs every action and check kind|submitted answer is judged in the background and resumes by id)$"'
```

### Owner/static verification

- `mise run test:shared`, `mise run py:test:unit`, `mise run ts:test:unit` for
  touched client/SDK behavior; `mise run test:skald` if its Python export or
  feature ownership is edited, and `mise run test:vala` if that owner is edited.
  Extend existing integration/telemetry regressions only where deletion touches
  their imports or dependencies; record exact focused commands for any test named
  in implementation evidence.
- `mise run fmt`, `mise run lints`, `mise run py:format`, `mise run py:lints`,
  `mise run py:typecheck`, `mise run ts:format:check`, `mise run ts:lints`,
  `mise run ts:typecheck`, `mise run ts:napi:check`,
  `mise run ts:pack:check`, and `mise run check:py-wheel-no-testing`.
- `mise run check:deps`; `mise run codegen:check` if generated contracts/errors
  are affected. Generate native declarations from source. `mise run docs:check`
  for docs-site edits. Use `mise run check:examples` for shared example changes;
  the support-desk journeys above already drive the checked-in examples.
- Inspect surviving public/native exports, package contents, authored typing,
  generated declarations, docs, and actual caller closure. Prove the removed
  APIs have no alternate public path; keep Cards, external-framework adapters,
  and internal runtime symbols. This is one-time deletion proof, not a new
  permanent name-ban script.
- Run `git diff --check` and inspect tracked/untracked changes. Record the
  final obligation-to-evidence matrix, scenario results, features, exact
  commands, and any bounded command corrections. Full integrated verification
  runs once at change review, not through broad journey suites in this task.

## Material Stop Conditions

- A surviving Card, verification, telemetry, or framework integration requires
  a deleted public runtime symbol: inspect its real ownership and adapt within
  existing boundaries. Stop only if preservation requires a new material
  public API or architecture decision; do not restore the runtime as an alias.
- Migration requires changed permission/credential/audit/tenancy semantics,
  persisted contracts, or new execution machinery: return for specification
  authority with source evidence. Do not expand this deletion into a framework.
- Required verification remains red with no in-scope recovery: finish
  unaffected proof and report the precise blocker. Never weaken tests or gates.

## Authority Links

- [Approved deletion specification, revision 3](../spec.md)
- [Repository standards](../../../../AGENTS.md)
- [Agent rules](../../../../architecture/agent-rules.md)
- [Protocol authority](../../../../architecture/wyrd-design.md)
- [Runtime boundary](../../../../architecture/wyrd-doctrine.mdx)
- [Spec-driven development](../../../../architecture/references/languages/spec-driven-development.md)
- [Implementation execution](../../../../architecture/references/languages/implementation-execution.md)
- [Testing workflows](../../../../architecture/references/languages/testing-workflows.md)
- [Canonical commands](../../../../mise.toml)

## Implementation Evidence

Commits `178129c6c`..`c2d0db662` on `change/agent-runtime-composition`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-012/013; AC-007 | Deleted `python/wyrd/agent/`, the `agent`/`tool`/`providers` native submodules in `sdks/wyrd-sdk-python/src/lib.rs`, root `wyrd` exports/stubs, and the `python` features plus `python.rs` of `skald-agent`/`skald-tool`/`skald-runtime`. Rust (`wyrd-ai`) and TS exports never had an Agent constructor; still none. `AgentError`/`SessionError`/`ToolError` codes remain. | Import probe: `wyrd.agent` and `_wyrd.agent`/`tool`/`providers` absent, `AgentCard` imports; `py:test:unit` (597 passed), `py:lints`, `py:typecheck`, `check:py-wheel-no-testing`, `ts:pack:check`, `ts:napi:check` | PASS |
| REQ-014; AC-008; INV-005..008 | `AgentCard`/`AgentSpec`, hydration, Prompt authoring, and internal Skald/Vala/server execution untouched beyond removing python-only cfg items | P-R 3/3, P-P 3/3, P-T 3/3 (exact commands above; requires `mise run build:monty-worker`); `test:skald`, `test:shared` | PASS |
| REQ-015/016; AC-009 | Deleted `wyrd-client` `observe/invoke.rs`, `gateway_caller.rs` and their tests; Python `Run.invoke` pymethod/stub; TS native `invoke` and `Run.invoke`; regenerated `index.d.ts`/`index.d.cts`. Consumers render the Agent's Prompt and call the gateway with `wyrd-run-id`/`wyrd-card-uid` themselves: `examples/support-desk/{python,rust,typescript}`, `examples/support-desk-app/rust/lib.rs`, and the gateway-inference, local-development, and support-desk journeys in all three SDKs | J-R 6/6 (task selectors plus `gateway_inference::run_invokes_its_agent_through_the_gateway` and `::agent_routes_reach_named_native_ingress`), J-P 4 passed, J-T 4/4; `test_cli_gateway_agents` via `test_installed_cli_gateway_setup` (`test:local-dev:journey` step) PASS | PASS |
| REQ-017; AC-010; INV-001..004 | Refusals now asserted as gateway responses: 403 `WYRD_PERMISSION_403_DENIED_RBAC` (viewer, granted deployment), 403 `WYRD_VALA_403_BIFROST_CARD_SCOPE` (foreign Card), 400 `WYRD_GATEWAY_400_INVALID_REQUEST` (incompatible adapter, via the existing `wyrd-prompt-provider` header); correlation headers asserted absent upstream | Same journeys as above; external-framework/stock-client gateway tests unchanged | PASS |
| REQ-018; AC-011 | Updated `runs-observations-and-evidence.md`, `examples/gateway-setup/README.md`, `examples/README.md`, `examples/rust/README.md`, `wyrd-sdk-rust` lib docs, `wyrd-utils` rustdoc | `docs:check`, `check:examples`, `check:deps`, `fmt`, `lints`, `py:format`, `ts:format:check`, `ts:lints`, `ts:typecheck`, `git diff --check` | PASS |

Retired tests and assertions (sole contract was the deleted API):
`tests/unit/runtime/agent/`, `test_callbacks.py`, `test_session.py`,
`test_validate_registrable.py`, the live smoke's local-tool-loop case and
its AgentRun result/iteration/callback assertions (TASK-001-R1 restored the
other 17 cases of `tests/smoke/test_prompt_live_providers.py` as rendered
native requests sent straight to each provider), the Rust examples
`agent_smoke`/`agent_openai` and Python `agent_smoke`/`structured_pipeline`,
`wyrd-client` `gateway_caller` and `local_development::cli_gateway_agents`
(the Rust child of the Python CLI journey, with its native-route trap), the
state test `invoke_refuses_an_agent_with_tools_before_io`, the malformed-route
and non-Agent-Run refusals, the local Vertex-Prompt refusal, and the "override
never changes the declared route" assertion. Test names were kept, so the task
selectors are unchanged.

Material limits: the docs-only `examples/support-desk-app/rust/lib.rs` is not
compiled by any lane; it was formatted and mirrors the compiled support-desk
`answer`. TypeScript has no Prompt render API, so TS callers bind
`{{question}}` in the portable messages themselves. The Python `test_cli_gateway_agents`
runs only under the installed-CLI fixture.
