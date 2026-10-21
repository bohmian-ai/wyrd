---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-guided-setup
spec_revision: 5
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-021, REQ-022, REQ-023, REQ-024, REQ-025, REQ-026, REQ-027, REQ-028, REQ-030, REQ-031, REQ-032, INV-005, INV-007, INV-008, INV-009, AC-001, AC-002, AC-003, AC-012, AC-013, AC-014, AC-015, AC-016]
depends_on: []
parent_task:
remediates: []
---

# Guided local setup and ASGI application integration

## Outcome and Value

A developer starts Wyrd with `wyrd init`, follows the same first-verification
steps through an agent prompt or manually, and can add the Python ASGI adapter
to an application without hand-managing writer startup, request Runs and drain.
The result is stored verification evidence and explicit lifecycle failures.

## What Changes and What Must Stay True

Own local bootstrap and its CLI/MCP prompt handoff, the Python ASGI adapter,
its public exports/stubs and journeys, and the docsite learning/navigation and
integration changes. Keep PostgreSQL and installed runtime prerequisites
visible. Preserve bootstrap persistence, gateway behavior, credential safety,
JSON-only stdout and foreground supervision. Never launch a coding agent.

Use the existing WyrdState lifecycle and execution-local Run context. The
adapter does not own auth, registration, a second writer, or HTTP spans. It
must work with an administrative/local client now and propagate any shared
client startup refusal unchanged after TASK-002 integrates. Registration is
still explicit; the adapter only starts an already loaded state.

Both tasks have no prerequisite task and can start on the same approved
revision. TASK-002 owns auth, key management, shared-client version selection,
state startup validation, SDK auth projections and deployment examples. This
task owns CLI bootstrap/MCP prompt behavior and the new Python integration
package; avoid editing shared auth/state internals to implement the adapter.
Coordinate only touched shared file sections and generated output publication;
there is no API handoff to invent between tasks.

The platform guide and combined documentation publication require TASK-002's
real deployment examples. Author navigation and audience summaries against the approved
contract in parallel; do not publish placeholder examples or claim those
integrated checks passed before combining the changes. This is a final
integration dependency, not a reason to serialize implementation.

Non-goals remain the spec's: no framework auto-discovery, additional framework
adapters, collector identity, credential migration, or registration redesign.
Hard-kill delivery is not guaranteed; drain failure must remain visible.

## Approach

1. Extend installed bootstrap with the renamed command and prompt-only mode.
2. Project one versioned prompt through readiness, MCP and documentation.
3. Close the manual first-verification path through each public language SDK.
4. Wrap the existing state lifecycle and Run context with pure ASGI middleware.
5. Exercise failure, concurrency, instrumentation ordering and worker behavior.
6. Update docsite guides, navigation and generated projections from their owners.

## Ordered Implementation Scenarios

### Scenario 1 — Offline prompt and local command

**Behavior.** `init --print-prompt` emits only the release prompt without configuration, network or writes. Setup options conflict with it. Normal init retains bootstrap behavior; the old command fails parsing.

**RED.** Add planned CLI test `guided_init_print_prompt_is_offline`; its focused command is C1 below. Expect the missing command/mode to fail, not a missing server.

**GREEN.** Expose the new command/mode through the installed CLI; leave the existing bootstrap owner responsible for server lifecycle and settings. Rerun C1.

**REFACTOR.** Reuse existing parsing and output policy; remove obsolete command exposure without a compatibility alias.

### Scenario 2 — Ready server and authenticated MCP prompt

**Behavior.** Interactive and scripted bootstrap retain readiness, saved settings, gateway, partial-failure and shutdown behavior. Readiness JSON and authenticated MCP return the same release prompt; host commands preserve literal content without executing agents.

**RED.** Add planned CLI journey `pg_tests::guided_init_lifecycle_and_prompt` and Python journey `test_guided_setup_prompt_surfaces` in the existing local-development module (C2/C3). Observe missing prompt/command behavior against real server startup.

**GREEN.** Extend the existing readiness and MCP handlers, retain credential/host installation behavior and provide paste instructions for unsupported launch commands. Rerun C1–C3.

**REFACTOR.** Keep one prompt source and existing host metadata; eliminate duplicated prompt text and duplicate bootstrap orchestration.

### Scenario 3 — Manual first verification in every supported language

**Behavior.** Documented commands create a Service and bound Verifier and retrieve stored observation/verdict evidence without a model key. Language ambiguity and conflicting files do not overwrite project content.

**RED.** Add planned parameterized Python journey `test_guided_manual_setup` to the existing local-development module (C4), driving installed Python, Rust and TypeScript commands. Confirm the missing handoff/projection fails; reuse the current example/server tooling.

**GREEN.** Make prompt instructions and manual examples follow the same working workflow, pin releases, and make unsupported/unavailable setup stop with an actionable diagnostic. Rerun C2–C4.

**REFACTOR.** Extract no new setup engine; reuse existing quickstart examples and their language runners.

### Scenario 4 — ASGI lifespan startup and drain

**Behavior.** The wrapped application lifespan still runs. Startup failure signals startup.failed, cleans partial resources and hides no primary error. Application shutdown finishes before Wyrd drain. Failed drain signals shutdown.failed and logs its code; blocking SDK calls do not stall the loop.

**RED.** Add planned Python `test_asgi_lifespan` in a new real-server ASGI integration test module (C5). Exercise successful startup/shutdown plus writer, provider and wrapped-application failures. Expect missing middleware/protocol handling.

**GREEN.** Adapt existing state methods and ASGI lifespan messages. Preserve missing-extra errors, provider conflicts, cancellation and existing bounded shutdown semantics. Rerun C3 and C5.

**REFACTOR.** Keep a pure ASGI adapter and existing lifecycle ownership; remove any duplicate queue, flush or provider logic.

### Scenario 5 — Per-request Run isolation

**Behavior.** Each HTTP/WebSocket invocation has its own Run, including streaming and application completion work. Exceptions, disconnect and cancellation restore context. Invalid aliases fail before application execution. Unrelated scopes pass through.

**RED.** Add planned `test_asgi_run_isolation` (C6), overlapping real requests and asserting stored Run/Card attribution and cleanup on negative branches. Missing context isolation must fail an assertion.

**GREEN.** Enter the existing Python execution-local Run context for the ASGI call and preserve application exceptions. Rerun C5/C6.

**REFACTOR.** Reuse the public Run context; introduce no global request registry or new Run persistence behavior.

### Scenario 6 — Application-owned instrumentation

**Behavior.** Wyrd outside OTel instrumentation correlates inner spans without generating HTTP spans. Default telemetry refuses an existing provider; telemetry=False leaves application provider installation and flushing to the application.

**RED.** Add planned `test_asgi_telemetry_ownership` (C7), checking stored spans, absence of duplicate instrumentation and provider ownership. Expect missing integration/order behavior.

**GREEN.** Use existing telemetry/provider and correlation hooks and document working middleware order. Rerun C5–C7.

**REFACTOR.** Keep provider management in its current owner and avoid framework monkey-patching.

### Scenario 7 — Worker-local resources

**Behavior.** Two Gunicorn/Uvicorn workers each start state resources in lifespan after fork and deliver evidence. Graceful worker shutdown drains them. The guide requires enabled lifespan and states unsupported hard-kill guarantees.

**RED.** Add planned `test_asgi_worker_lifecycle` (C8), running actual worker processes against WyrdTestServer. Expected RED is missing worker integration, never absent executables.

**GREEN.** Complete the adapter and tested app startup example; provide the compatible worker test dependencies through the Python dev environment if absent. Rerun C5–C8.

**REFACTOR.** Retain per-worker use of the existing SDK lifecycle; add no cross-process writer or supervisor.

## Acceptance Criteria

| Obligations | Required observation and proof |
|---|---|
| REQ-001–004, INV-008, AC-001 | C1/C2 establish offline purity, removed old command and unchanged startup/persistence/failure behavior. Existing local bootstrap regression journeys remain passing. |
| REQ-005–006, INV-007, AC-002 | C2/C3 and projection checks show one release-labelled prompt through CLI, JSON, MCP and docs without write authority. |
| REQ-007–008, AC-003 | C4 runs the manual path in all three languages and checks persisted observation/verdict; prompt content checks cover safe language/file decisions. |
| REQ-021–023, AC-014 | C5/C7 prove explicit ASGI failure signaling, partial cleanup, final drain and provider ownership. |
| REQ-024–025, AC-012 | C6/C7 prove per-request context, alias/error/cancellation handling, real span correlation and no duplicate HTTP spans. |
| REQ-026, INV-009, AC-013 | C8 observes independent workers and graceful drain; portable core behavior remains independent of Linux-only examples. |
| REQ-027–028/030–031, AC-015 (learning and integration portion) | Docs present three audience paths, agent/manual setup and the FastAPI integration shape, with journey-backed example imports. Platform release content is closed with TASK-002 at integration. |
| REQ-032, AC-016 | Published docs, generated text, help and remediation omit the old command; historical records may describe the rename. TASK-002 owns removal of the unused attribute constant. |
| INV-005 | Local examples register with registration credentials; ASGI examples load existing bundles and never declare during deployed startup. |

## Implementation Reference

- **Bootstrap:** `crates/wyrd/wyrd-cli/src/server/dev.rs::{DevArgs, DevReady}`
  and its existing orchestration retain install, settings, gateway and child
  ownership. `src/mcp/hosts.rs::McpHost` owns host variants. Inspected
  `tests/cli.rs`, `tests/pg_tests*`, Python's installed CLI journeys and the
  `test:local-dev:journey` recipe. Missing behavior is renamed exposure and
  prompt handoff, not server orchestration.
- **MCP:** `crates/wyrd/wyrd-server/src/mcp/mod.rs::WyrdMcpHandler` owns the
  authenticated request handler. Extend its protocol prompt operations;
  `src/mcp/principals.rs` remains TASK-002's credential-tools owner. The setup
  prompt is neither a registry Card nor a new execution service.
- **Python lifecycle:** `sdks/wyrd-sdk-python/src/state/mod.rs` already owns
  `start_bifrost`, `start_telemetry`, `shutdown`; `src/observe/mod.rs` owns
  execution-local Run enter/exit. Existing consumers are local-development
  journeys and `tests/integration/otel/frameworks/test_framework_telemetry.py`.
  Add the explicit public ASGI adapter around those workflows, not copies of
  their native implementations. Public package exports, authored stubs,
  typing and test/dev dependency closure belong to this task.
- **Docs:** existing `examples/quickstart`, `docs/AGENTS.md`, get-started pages
  and generated `llms*.txt` are the source/example projection pattern. Own
  docs navigation, setup/integration pages and generation. TASK-002 supplies
  tested deployment example source; coordinate final imports instead of
  hand-writing copyable untested snippets.
- **Authority:** own Wyrd-design local CLI/setup and Python-adapter prose,
  operations local-bundle command references, and the rename changelog entry.
  TASK-002 owns auth/identity sections, migrations and key replacement notes.
  Shared documents are edited by section; preserve other worktree edits.

## Verification and Evidence

All C1–C8 selectors below are **planned**, not claims of existing tests. Confirm
actual collection and selectors after adding them and before RED. Existing
Cargo target `wyrd-cli --test cli` explicitly includes the `pg_tests` module;
Python integration collection and installed-CLI prerequisites are defined by
`mise.toml`. Use repository Postgres setup and real WyrdTestServer.

For CLI startup journeys, first run `mise run py:setup`, `mise run ts:build`,
`mise run ts:build:testing`, `mise exec -- cargo build --locked -p wyrd-cli
-p wyrd-server --bins --tests`, then the UI install/build commands from
`test:local-dev:journey`. These are preparation, not a second broad journey run.
Run Cargo builds sequentially even while the two task branches are active.

| Command | Purpose |
|---|---|
| **C1** `mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=guided_init_print_prompt_is_offline)'` | Offline/purity and parsing |
| **C2** `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-cli --test cli --run-ignored=all -E "test(=pg_tests::guided_init_lifecycle_and_prompt)"'` | Installed bootstrap and readiness |
| **C3** `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/integration/test_local_development.py::test_guided_setup_prompt_surfaces'` | Real authenticated MCP prompt projections |
| **C4** `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/integration/test_local_development.py::test_guided_manual_setup'` | Each documented language workflow |
| **C5** `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/integration/test_asgi_integration.py::test_asgi_lifespan'` | Lifespan success/failure/drain |
| **C6** `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/integration/test_asgi_integration.py::test_asgi_run_isolation'` | Concurrency and scope terminal paths |
| **C7** `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/integration/test_asgi_integration.py::test_asgi_telemetry_ownership'` | Span correlation and provider ownership |
| **C8** `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/integration/test_asgi_integration.py::test_asgi_worker_lifecycle'` | Real workers and graceful shutdown |

Apply migrations inside the managed Postgres lifecycle before the focused tests
when the test owner requires them; follow `db:migrate:all:inner` as the existing
local-dev and SDK recipes specify. Never assume a shell already has DB values.

For changed executable owners run `mise run fmt`, `mise run lints`,
`mise run py:format`, `mise run py:lints`, `mise run py:typecheck`,
`mise run codegen:check` and `mise exec -- git diff --check`.
Docs changes use static/example proof, not manufactured RED: regenerate with
`mise run docs:generate`, inspect rendered paths/examples and run the existing
command checks. Full `mise run docs:check`, installed bootstrap regressions,
all SDK journeys and cross-task ASGI identity refusals run once on the combined
candidate at change review. Record that integration evidence as pending until
it actually passes; no whole `gate` or blanket SDK suites in this task.

Record each scenario's expected RED, GREEN and final focused result, including
which platforms/workers were actually exercised. Source inspections alone are
not runtime proof.

## Material Stop Conditions

Stop for a changed public prompt, altered bootstrap security/persistence,
nonportable core behavior, a need for a second lifecycle/registry owner, or
new durable Run/telemetry semantics. A private adapter detail or corrected test
selector is implementer-owned. Do not implement TASK-002's auth changes here.
A cross-task integration failure remains work to close after integration.

## Authority Links

- [Approved revision 5](../spec.md)
- [Root rules](../../../../AGENTS.md)
- [Change workflow](../../../spec-driven-development.md)
- [Execution discipline](../../../implementation-execution.md)
- [Testing](../../../../TESTING.md)
- [Wyrd design](../../../../architecture/wyrd-design.md)
- [Deployment/release](../../../../architecture/operations/deployment-and-release.md)
- [Python rules](../../../../sdks/wyrd-sdk-python/AGENTS.md)
- [Docs rules](../../../../docs/AGENTS.md)
