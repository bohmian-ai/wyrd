---
id: TASK-005
kind: implementation
status: proposed
spec: SPEC-skald-workflow-runtime
spec_revision: 5
requirements: [REQ-024, REQ-025, REQ-026, REQ-027, REQ-028, REQ-030, REQ-031, REQ-044, INV-004, INV-005, INV-007, AC-001, AC-002, AC-003, AC-004, AC-013, AC-014]
depends_on: [TASK-003, TASK-004]
parent_task:
remediates: []
---

## Outcome and Value

The shared Rust client and `wyrd workflow` CLI expose the complete approved user
journey: run an unregistered bundle locally, register it, fetch and run the
locked graph locally, or submit a long server run and wait, detach, inspect, or
cancel by run ID. The checked-in code-review workflow proves the whole path
against deterministic providers and a real server.

## Owners, Scope, Consumers, and Prohibited Changes

`wyrd-client` owns typed create/get/cancel/poll transport and stable
idempotent submission. `wyrd-cli` owns source selection, local-vs-server
execution ergonomics, output, interruption behavior, and actionable errors.
Existing loader, Cards, Skald, and server owners remain authoritative.

Do not duplicate HTTP transport, registry hydration, validation, or execution in
the CLI; do not overload `--server`; do not auto-register file workflows for
server execution; do not add Python, TypeScript, MCP, UI, durable queue, or
credential-administration surfaces.

## Approach

1. Add one discoverable shared Rust Workflow-run client capability using the
   existing transport and idempotent submission support.
2. Add `workflow run`, `status`, and `cancel` with the approved mutually
   exclusive source, input, execution, wait, detach, and output semantics.
3. Route local file and registered-local modes through loader/hydration and
   Skald; route server mode only through `wyrd-client`.
4. Preserve run IDs across polling interruption and stable Wyrd errors across
   human and JSON output.
5. Prove the code-review bundle and all execution modes with real Rust/CLI →
   server → Rust/CLI journeys and deterministic local upstreams.

## Ordered Implementation Scenarios

### Scenario 1 — Shared Rust client creates, polls, cancels, and retries safely

**Behavior.** The shared client sends one stable idempotency key across a create
retry, accepts first-create and replay responses, polls monotonic snapshots to a
terminal state, and performs typed get/cancel with structured errors. This
proves REQ-030, REQ-031, INV-007, and AC-004.

**RED.** Add `http::workflow_create_reuses_stable_idempotency_key` to the
existing `wyrd-client` `transport` target and run:

```bash
mise exec -- cargo nextest run --locked -p wyrd-client --test transport \
  -E 'test(=http::workflow_create_reuses_stable_idempotency_key)'
```

It must fail because `wyrd-client` has no Workflow-run capability.

**GREEN.** Compose the typed capability from the existing HTTP transport and
idempotent submission method.

**REFACTOR.** Keep transport mechanics private and expose Workflow operations
through one cohesive client handle rather than sibling ad hoc functions.

### Scenario 2 — CLI local modes use one loader and Skald path

**Behavior.** `workflow run` accepts exactly one file/UID/full-selector source,
accepts one JSON input source, defaults to local execution, rejects server mode
for files, loads an unregistered bundle without registration, and fetches the
exact registered graph for local execution. JSON and human output expose the
approved result and optional intermediate detail. This proves REQ-024 through
REQ-028, INV-004, INV-005, AC-001 through AC-003, and AC-013.

**RED.** Add
`workflow_local_protocol::workflow_run_file_and_registered_selector_share_semantics`
to the existing CLI `cli` target and run:

```bash
mise exec -- cargo nextest run --locked -p wyrd-cli --test cli \
  -E 'test(=workflow_local_protocol::workflow_run_file_and_registered_selector_share_semantics)'
```

It must fail because the CLI has no Workflow command group or local registered
execution path.

**GREEN.** Compose existing loader, Cards hydration, and Skald APIs; keep CLI
logic limited to validation, orchestration, and rendering.

**REFACTOR.** Reuse existing source/input/output argument patterns and remove
any duplicate parsing introduced during GREEN.

### Scenario 3 — CLI server mode supports wait, detach, status, cancel, and interruption

**Behavior.** Server run prints the accepted run ID before polling, waits by
default, detaches on request, later status/cancel uses the same endpoint/auth,
JSON emits the current snapshot, and interrupted polling neither cancels nor
resubmits and reports the existing ID. This proves REQ-026, REQ-027, REQ-030,
REQ-031, INV-007, and AC-004.

**RED.** Add
`workflow_server_protocol::workflow_run_wait_detach_status_and_cancel` to the
CLI `cli` target and run:

```bash
mise exec -- cargo nextest run --locked -p wyrd-cli --test cli \
  -E 'test(=workflow_server_protocol::workflow_run_wait_detach_status_and_cancel)'
```

It must fail until CLI commands consume the shared Workflow client.

**GREEN.** Implement the minimum command behavior around the client; never call
server route details directly from argument handlers.

**REFACTOR.** Share existing server/auth/output plumbing and keep polling policy
behind the client or one CLI orchestration owner.

### Scenario 4 — The code-review journey proves every shipped boundary

**Behavior.** A checked-in three-Agent review bundle runs unregistered locally,
registers with exact Prompt/Agent relationships, runs registered locally through
Native and direct ExtGateway, runs local WyrdGateway through public ingress, and
runs server WyrdGateway/ExtGateway asynchronously. Independent reviewers overlap,
the final reviewer receives both explicit bindings, final/intermediate outputs
match, Native server execution is refused, and all providers are local mocks.
This proves REQ-044 and AC-001 through AC-004, AC-013, and AC-014.

**RED.** Add
`workflow_journey::code_review_runs_local_registered_and_server` to the CLI
`cli` target and run it with the repository-managed server environment:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:all:inner && WYRD_CLI_E2E=1 mise exec -- \
  cargo nextest run --locked -p wyrd-cli --test cli --run-ignored all \
  -E 'test(=workflow_journey::code_review_runs_local_registered_and_server)'"
```

It must fail until the complete client/CLI/server seam and bundle exist.

**GREEN.** Add the smallest deterministic bundle and real-server journey that
drives every shipped surface without live credentials.

**REFACTOR.** Reuse existing test-server/provider fixtures and keep the bundle
representative rather than creating a second test-only Workflow format.

## Acceptance Criteria

- Rust callers discover create/get/cancel/poll through `wyrd-client`; CLI uses
  that exact implementation for server mode.
- CLI source and input combinations fail clearly and never trigger unintended
  registration, execution, cancellation, or resubmission.
- Human and JSON output retain run ID, stable status/errors, final outputs, and
  opt-in intermediate results.
- The real code-review journey covers every new Rust/HTTP/CLI boundary and
  route with no credentials.
- Existing Python and TypeScript SDKs and MCP catalogs gain no Workflow
  invocation surface and remain regression-clean.

## Expected Write Set and Consumer Closure

Likely surfaces include a focused Workflow handle under
`crates/shared/wyrd-client`, `crates/wyrd/wyrd-cli` commands and integration
modules, checked-in YAML fixtures/examples, and narrowly required test-server
support. Existing CLI, Cards, transport, and error owners must be reused; the
CLI may add the existing Skald Workflow owner as a workspace dependency for
local execution.

## Verification and Evidence

Run each focused scenario sequentially, then:

```bash
mise run test:shared
mise run test:skald
mise run test:wyrd
mise run test:cli:journey
mise run codegen:check
mise run check:client-tier
mise run check:cli-client-tier
mise run check:pyo3-scope
mise run fmt
mise run lints
mise run gate
git diff --check
```

`mise run gate` is required at integrated closeout because this change spans
contracts, Skald, shared client, server, CLI, generated artifacts, and repository
authority. It is not an iteration command for the earlier tasks.

## Material Stop Conditions

- The CLI needs a second transport, validator, registry, or executor.
- A file workflow must be uploaded or implicitly registered to support server
  execution.
- Completion requires Python, TypeScript, MCP, UI, live credentials, a durable
  run service, or a new third-party dependency/Cargo feature.
- The approved CLI flags, asynchronous API, route behavior, or output contract
  must change.

## Authority Links

- `changes/active/skald-workflow-runtime/spec.md` Revision 5
- `AGENTS.md` §§2–3, 9, 11–12
- `architecture/agent-rules.md`
- `architecture/wyrd-design.md` §§Client model, Workflow, Spec-file authoring
- `architecture/references/architecture/patterns.md` §Client Pattern
- `architecture/references/languages/errors.md`
- `architecture/references/languages/testing-workflows.md`
