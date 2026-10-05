---
id: TASK-005
kind: implementation
status: ready
spec: SPEC-skald-workflow-runtime
spec_revision: 12
requirements: [REQ-054, REQ-055, REQ-056, REQ-057, REQ-058, REQ-059, AC-029, AC-030, AC-031, REQ-024, REQ-026, REQ-027, REQ-028, REQ-031, REQ-044, REQ-047, REQ-050, INV-004, INV-007, INV-015, INV-016, AC-001, AC-002, AC-003, AC-004, AC-012, AC-013, AC-014, AC-015, AC-016, AC-017, AC-018, AC-023, AC-024, AC-026]
depends_on: [TASK-001, TASK-002-cleanup, TASK-003, TASK-004]
---

# CLI Workflow journeys and integrated contract closure

Implementation skill: `$wyrd-implement`.

## Outcome and Value

Users can run the checked-in bundle before/after registration and on the server,
detach, inspect, and cancel with the same typed Rust contracts. Integrated
Rust/Python/TypeScript/HTTP/CLI evidence closes the shipped route/protocol/security/lifecycle
journeys and architecture records the explicit local/accepted-job behavior.
This is observable CLI delivery plus its necessary full capability proof, not
a separate mechanical documentation or test-only phase.

## Owners, Scope, Consumers, and Prohibited Changes

CLI owns parsing/configuration/rendering/process cancellation and awaits Skald
or shared Workflows APIs. Shared client owns transport/remote lifecycle;
Skald owns local execution; existing apply path owns registration. Architecture
documents own durable doctrine/security/deployment rules, not task history.
No duplicate executor/transport/configuration owner, API-only workflow format,
Python/TS server-run lifecycle or new MCP Workflow surface,
migration docs or compatibility aliases, persistent queue/recovery/affinity
implementation, arbitrary tool/code registration or credential administration.

## Source-backed reuse map

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| CLI loading/run | shared Workflow/Cards API from cleanup; existing CLI config/commands | current cli target, gateway_server_journey.rs | Workflow command projection | Call agreed shared APIs and existing signal/output owners | Clap command only; no CLI graph/runtime/config owner |
| Apply | existing CLI apply and `Cards::register_from_path` | Cards registration/CLI journeys | Actual Workflow bundle proof | Existing apply path unchanged except native syntax consumers | No new registration path |
| Remote commands | TASK-003 Workflows | server host and transport tests | CLI detach/status/cancel/wait | Thin shared handle projection | No polling transport duplicate |
| Language closure | existing SDK runtime wrappers; WyrdTestServer and @wyrd/testing | cleanup SDK tests, Python Cards CRUD, TS cards-state | Public API and selected route journeys across three runtimes | Extend those journeys and current selectors | No new fixture framework |
| Local config | GlobalConfig and TASK-003 dependency composition | SDK run methods and CLI GlobalConfig consumers | CLI consumes same binding setup | Delegation | No per-language binding/secret parser |

## Approach

1. Add exact CLI commands/selectors/input/execution/detach contracts using
   TASK-003 shared local configuration and existing secret references.
2. Compose shared Workflow::from_path, Cards.workflow loading, shared local
   execution dependencies/PublicWyrdGatewayCaller and remote Workflows; preserve ordinary `--server` and authentication behavior.
3. Render portable snapshots/errors in JSON and human mode, printing accepted
   run ID before default polling; interrupted wait leaves run active.
4. Drive actual example apply/run/local/server/status/cancel and registered
   route/protocol/security matrix through compiled CLI and real Rust client/server.
5. Synchronize architecture/security/gateway/docs/examples, then execute full
   integrated proof including the three agreed SDK local surfaces and no additional remote APIs.

### Packet-local CLI/config seams

`wyrd workflow run` has exactly one source: `--file`, `--uid`, or all of
`--space --name --version`. Input is JSON object from `--input` or
`--input-file`, mutually exclusive. `--execution local|server` defaults local;
file cannot run server-side; server requires exact registered selector. Existing
`--server URL` remains connection endpoint. `--detach` is server-only and returns
after acceptance; server default prints run ID then waits. `status <run-id>` and
`cancel <run-id>` are server-only; both use shared get/cancel. JSON success is
the direct WorkflowRun and uses named outputs/steps, not legacy final_output.
Human mode exposes an explicit intermediate-results option. Interrupt polling
reports the already-accepted ID, never cancels or replays/submits again.

Consume TASK-003's current `wyrd_client::GlobalConfig` (in `src/global_config.rs`, not
workspace `wyrd-config`) with its `workflow: LocalWorkflowConfig`, serde default/
deny-unknown contract:

```rust
pub struct LocalWorkflowConfig {
    pub external_gateway_bindings:
        BTreeMap<CredentialBindingName, ExternalGatewayBindingConfig>,
}
// Pure ExternalGatewayBindingConfig from native packet:
// { protocol: ExternalGatewayProtocol, origin: Url,
//   secret_headers: BTreeMap<String, SecretRef> }
```

Default map empty. Shared-client composition resolves SecretRef only for selected
local execution dependencies, never during loading/registration or into Cards.
CLI delegates that work rather than adding its own config parser/secret loop. Programmatic
native callers can supply runtime ExternalGatewayBindings directly. Local
bindings allow explicitly configured private addresses but still enforce origin,
protocol, headers/TLS/redirect/bounded IO. Public WyrdGateway uses Wyrd auth only;
Native keeps existing provider configuration, not gateway credentials.

Existing real CLI target is `wyrd-cli --test cli` because `autotests = false`;
wire Workflow journey module into `tests/cli.rs` alongside current modules, not
an unregistered standalone test file. Existing CLI journey fixture precedent:
`tests/gateway_server_journey.rs` production server + compiled CLI; reuse its
bootstrap/local upstream/auth boundaries and current Postgres wrapper.

## Ordered Implementation Scenarios

All names are **planned**, default features. Rust CLI lifetime proof executes
the compiled binary, not just Clap parsing. Real-server journey tests are gated
`#[ignore]` as existing CLI journeys and must be explicitly selected below;
`WYRD_CLI_E2E=1` matches the current CLI environment convention. Every GREEN
reruns earlier scenarios and REFACTOR keeps them green.

### Scenario 1 — CLI choices and local configuration are unambiguous

**Behavior.** REQ-026–027/050: valid selectors/input/modes/config choose exact
native APIs; malformed/mixed/incomplete selectors, both inputs, nonobject input,
server file or local detach fail before side effects with stable errors.
Local secret refs resolve at runtime with no plaintext values/secret leakage;
existing --server means endpoint, not execution mode. JSON and human output
address outputs/steps by name.

**RED.** Add `workflow_journey::workflow_cli_contract` in existing `cli` target;
`mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=workflow_journey::workflow_cli_contract)'`.
This pure/compiled help-validation case is not ignored. Expect missing commands
or wrong defaults; assertions include native DTO JSON and error codes.

**GREEN.** Thin CLI/config projection into shared/native owners, with safe
errors and no network/secret resolution for invalid invocation choices.

**REFACTOR.** Reuse existing input/config/output conventions and server/auth
plumbing; no new CLI transport or configuration catalog.

### Scenario 2 — The actual bundle runs before and after apply

**Behavior.** AC-001–003/013: compiled CLI loads actual bundle, runs independent
reviewers concurrently and explicit final bindings, applies exact dependency
graph without execution, fetches locked registered graph and runs locally with
equivalent portable results. Real Rust SDK Workflow/Cards/Skald path proves same
bundle and input. Native, direct external, and public WyrdGateway registered local
routes reach deterministic local upstreams with correct stored fallback and
step-over-workflow route precedence; local private external binding works.

**RED.** Add `workflow_journey::workflow_file_apply_registered_local`;
`mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_CLI_E2E=1 mise exec -- cargo nextest run --locked -p wyrd-cli --test cli --run-ignored all -E "test(=workflow_journey::workflow_file_apply_registered_local)"'`.
Expect absent CLI wiring. Runtime-selected local fixture endpoints/keys may be
injected into execution environment, not substituted simplified Prompt schemas
or direct SQL registration. Count upstream calls so apply cannot execute unnoticed.

**GREEN.** Use shared loader/Cards registration/exact fetch, one native executor
and approved dependencies. Preserve actual YAML/schema roundtrip evidence.

**REFACTOR.** One CLI composition per mode; no second format/parser or automatic
binding conveniences.

### Scenario 3 — Server acceptance, detach and interruption preserve the job

**Behavior.** AC-004/018/019: CLI/server accepts and prints ID before wait;
default wait and detach→status use same shared client; cancel is idempotent;
interrupted/dropped wait leaves accepted job active with ID, not a cancellation
or duplicate submission. Lost response recovery, owner/foreign denial, partial
failure, total deadline and stored graph mutation have exact native snapshots.

**RED.** Add `workflow_journey::workflow_server_detach_status_cancel`;
`mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_CLI_E2E=1 mise exec -- cargo nextest run --locked -p wyrd-cli --test cli --run-ignored all -E "test(=workflow_journey::workflow_server_detach_status_cancel)"'`.
Expect absent run/status/cancel or polling semantics. Use process output and
deterministic provider gates; no wall-clock sleep to assume acceptance/start.

**GREEN.** Await shared Workflows methods; interruption stops client polling
only and retains already printed ID and stable error/result semantics.

**REFACTOR.** Keep server lifecycle in its owner and reuse CLI signal/output
patterns; no client resubmission on interruption.

### Scenario 4 — Supported protocols and security constraints hold end to end

**Behavior.** REQ-044 and AC-014–017/023/026: registered local Native/ExtGateway/
public WyrdGateway and server in-process WyrdGateway/direct ExtGateway journeys
cover Chat, Responses, Anthropic, Gemini and Vertex over supported combinations,
gateway-owned translation, local Vertex refusal and server Vertex success.
Incompatible protocol/capability, bound-origin/header/secret/DNS/redirect/response
or deadline violations dispatch no unauthorized data. Native/Vault/Operator
regression remains unchanged. Local custom tool uses same AgentTool contract
and contributes named outputs only when declared/caller supplied.

**RED.** Add `workflow_journey::workflow_registered_route_protocol_matrix`;
`mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_CLI_E2E=1 mise exec -- cargo nextest run --locked -p wyrd-cli --test cli --run-ignored all -E "test(=workflow_journey::workflow_registered_route_protocol_matrix)"'`.
Use table-driven real Rust client→server registry→local engine/public model edge
and Rust client→server run→client journeys, plus CLI projection. Expect uncovered
route adapters/CLI configuration. Controlled upstreams verify typed requests
and dispatch counts; no credentialed live provider or engine-only substitute.

**GREEN.** Close supported route/adapter matrix using current gateway capabilities;
reuse TASK-001, TASK-002-cleanup, TASK-003–004 owners/fixtures and retain exact errors/bounds/cancellation.

**REFACTOR.** Keep fixtures narrowly production-shaped; no speculative tool
onboarding/platform/scheduler added to complete the evidence.

### Scenario 5 — Team reuse and route execution agree across SDKs

**Behavior.** REQ-054–059, AC-029–031: a team applies versioned Agent/Prompt
Cards; another repository combines their refs with a new local Agent, runs
from_path/fromPath, applies the Workflow bundle, then Cards-loads/runs it.
All three SDKs retain exact UIDs/bindings/outputs, no principal/privilege
transfer and no dependency floating. SDK local Native/ExtGateway/public
WyrdGateway uses TASK-003 common preparation, with existing protocol support.

**RED.** Extend cleanup's named SDK journey tests to exercise compiled CLI apply
and selected local routes, reusing their exact focused commands recorded in
TASK-002-cleanup (reproduce them in implementation evidence). Extend this task's
`workflow_file_apply_registered_local` selector to prove actual team reuse.
Expected missing behavior is language/CLI route integration, not a new format.
Setup uses existing server/bootstrap and local upstreams; count dispatches so
load/apply cannot execute. Assert all supported per-language route combinations
and existing refusal behavior; no live provider credential or Node/Python
lifetime emulation in Rust tests. Exact SDK focused recipes are repeated below.

**GREEN.** SDK and CLI call the shared facade/config and existing runtime;
registration uses the same compiled apply command and Cards path.

**REFACTOR.** Retain common fixtures and shared owner corrections. Do not add
per-language execution or credential administration to satisfy this journey.

## Acceptance Criteria

Every shipped CLI/Rust/HTTP path has happy/negative/edge journey proof; actual
bundle succeeds locally/registered/server without implicit data forwarding;
accepted run IDs survive detach/interruption; route/protocol matrix and
security/lifecycle invariants remain exact. All current specification obligations have
implementation/evidence closure in the packet index, and all non-goals remain
excluded. Architecture/security/docs explicitly match approved behavior.

## Expected Write Set and Consumer Closure

CLI argument/command/render/config composition; consumers of TASK-003 shared GlobalConfig; existing
`cli` test target/module/fixtures; Rust SDK projection and real-client journey
support when needed; checked-in YAML/input examples and native source-derived
docs; `architecture/wyrd-design.md`, `wyrd-doctrine.mdx`,
`wyrd-security-posture.md`, relevant gateway/operations documentation. No
compatibility/migration narrative or changed shared Vault source. Architecture
must record ephemeral affinity/restart loss, capability-scoped surfaces,
explicit local bindings/results, native inline Prompt versus existing standalone
helpers, accepted scoped job authority and live gateway ownership, authenticated
fallback header, graph/full-snapshot/terminal reserve bounds.

## Verification and Evidence

Run every named focused selector above with selected counts during RED/GREEN.
Also retain earlier tasks' exact tests in the cumulative evidence; do not claim
existing tests cover planned selectors before they exist. Docs/codegen/authority
alignment is non-TDD static/regression proof; no manufactured RED.

SDK focused commands (planned tests, same owners as cleanup):

- Rust: `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sdk-rust --test workflow_loading --run-ignored all -E "test(=workflow_loading_journey)"'`.
- Python: `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run py:setup && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/cards/test_cards_crud.py -k test_workflow_loading_journey'`.
- TypeScript: `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run ts:build && mise run ts:build:testing && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/workflow-loading.test.ts -t "^workflow loading journey$"'`.

The task owns selection closure for new SDK journeys. Rust's ignored new target
is outside test:wyrd-sdk (lib only); run its exact command as separate final
proof. Python's focused Cards integration lane selects test_cards_crud.py;
placing its journey there retains selection. TS integration selects its entire
integration directory. Confirm actual gate selection before claiming coverage.

Final aggregate: `mise run gate`, followed by `git diff --check` and final
tracked/untracked diff inspection. This mixed contract/runtime/client/server/
CLI/Python/egress change has no complete Workflow capability gate. Do not rerun
or prescribe its component lanes separately as final verification.
**Known gap:** gate's CLI gateway journey filter does not select the three new
ignored Workflow CLI journey tests. Run their three exact wrapper commands
above as separate final proof after the aggregate; this is deliberate specific
coverage outside gate, not a duplicate component lane. Existing gate family
tests cover unignored server/shared-client tests; gate includes gateway/Vault
journeys, Bifrost, codegen, Python typing/tests, format/lints and boundaries.
No live cloud/model credential lane is required by this capability.

## Material Stop Conditions

Stop if a supported route/protocol requires changed gateway capability, an additional unapproved
public language surface, new credential/tool administration, changed explicit
bindings/results/accepted-job authority or durable lifecycle. Private CLI
composition/fixtures remain implementation-owned; preserve proof rather than
silently narrowing required scenarios.

## Authority Links

- [Approved Revision 12](../spec.md); TASK-001 through TASK-004
- `AGENTS.md`; `architecture/agent-rules.md`
- `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`
- `architecture/wyrd-security-posture.md`; `architecture/bifrost-design.md`
- `architecture/references/languages/{agent-harness,errors,spec-driven-development,implementation-execution,testing-workflows}.md`

## Implementation Evidence

Authority: approved spec Revision 14 (one ProviderRequest variant per wire
schema; `Prompt.provider` is the optional native destination), which
supersedes this task's Revision 12 references.

Commits: `cb2417666` (CLI), `99c613e6e` (CLI journeys), `57cf922c4`
(architecture/docs), `2cbdf4e00` (regenerated Prompt card reference),
`2167650cf` (Python/TypeScript journeys), `44d113c70` (Rust SDK journey).

### Reuse map revalidation

| Capability | Owner reused | Gap closed |
|---|---|---|
| CLI local run | `wyrd_client::Workflow::from_path`, `Cards::workflow().load`, `Workflow::run` | Clap projection in `crates/wyrd/wyrd-cli/src/workflow.rs`; no CLI loader, executor, or config parser |
| CLI server run | `wyrd_client::Workflows::{create,get,cancel,wait}`, `crate::client::from_global` | Detach, wait with Ctrl-C (exit 130, run left active), status, cancel |
| Registration | existing `wyrd apply` / `Cards::register_from_path` | None; journeys drive it unchanged |
| Local bindings | `GlobalConfig.workflow.external_gateway_bindings` via shared run preparation | None; CLI and SDKs read the same `config.toml` |
| SDK journey fixtures | `WyrdTestServer` provider root, Python `gateway_server` + `gateway/support.py`, TS `startTestServer(providerBaseUrl)` | Python `gateway_server` fixture moved to `tests/integration/conftest.py` so the Cards journey reuses it |

### Acceptance

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| S1 CLI choices unambiguous; invalid choices fail before side effects with stable errors; secrets never printed; JSON/human named outputs/steps | `crates/wyrd/wyrd-cli/src/workflow.rs`, `src/cli.rs`, `src/lib.rs` | `workflow_journey::workflow_cli_contract` | PASS |
| S2 actual bundle runs concurrently from file, apply executes nothing, registered exact/UID runs match; team reuse pinned; native/ext/public routes, step precedence, fallback | `tests/workflow_journey.rs` | `workflow_journey::workflow_file_apply_registered_local`, `workflow_journey::workflow_registered_route_protocol_matrix` | PASS |
| S3 server acceptance prints ID before wait; detach/status/cancel idempotent; SIGINT exits 130 leaving the run active with no resubmission; unknown ID 404 | `RunArgs::run_server`, `RunIdArgs` | `workflow_journey::workflow_server_detach_status_cancel` | PASS |
| S4 Chat/Responses/Anthropic/Gemini/Vertex matrix; local Vertex refused before dispatch, server Vertex succeeds; protocol mismatch 422 with no dispatch; ext secret sent, no Wyrd authorization | `tests/workflow_journey.rs` | `workflow_journey::workflow_registered_route_protocol_matrix` | PASS |
| S5 team reuse across SDKs with exact UIDs/no floating; SDK local Native/ext_gateway/public wyrd_gateway; registration dispatches nothing | `sdks/wyrd-sdk-rust/tests/workflow_loading.rs`, `sdks/wyrd-sdk-python/tests/integration/cards/test_cards_crud.py` (team and mixed registration through the installed `wyrd apply`), `sdks/wyrd-sdk-ts/wyrd/tests/integration/workflow-loading.test.ts` | Rust, Python, TypeScript focused journeys below | PASS |
| Architecture/security/operations/docs record affinity and restart loss, capability-scoped surfaces, explicit bindings/results, native inline Prompt, route ownership, fallback header, graph/snapshot/terminal-reserve bounds | `architecture/wyrd-design.md`, `wyrd-doctrine.mdx`, `wyrd-security-posture.md`, `operations/deployment-and-release.md`, `operations/reliability-and-recovery.md`, `docs/src/content/docs/how-to/build-a-workflow.svx` | `mise run docs:check` | PASS |

The fallback header was already recorded in `wyrd-design.md` and the served
OpenAPI document; no change was needed.

### Commands

All prefixed with `CARGO_TARGET_DIR=<repo>/target CARGO_BUILD_JOBS=12`.

- `mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=workflow_journey::workflow_cli_contract)'`
- `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_CLI_E2E=1 mise exec -- cargo nextest run --locked -p wyrd-cli --test cli --run-ignored all -E "test(=workflow_journey::workflow_file_apply_registered_local)"'`
- `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_CLI_E2E=1 mise exec -- cargo nextest run --locked -p wyrd-cli --test cli --run-ignored all -E "test(=workflow_journey::workflow_server_detach_status_cancel)"'`
- `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_CLI_E2E=1 mise exec -- cargo nextest run --locked -p wyrd-cli --test cli --run-ignored all -E "test(=workflow_journey::workflow_registered_route_protocol_matrix)"'`
- Rust SDK: `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sdk-rust --test workflow_loading --run-ignored all -E "test(=workflow_loading_journey)"'`
- Python: `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run py:setup && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/cards/test_cards_crud.py -k test_workflow_loading_journey'`
- Python fixture move: `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/gateway'` (9 passed)
- TypeScript (corrected): `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run ts:build && mise run ts:build:testing && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/workflow-loading.test.ts -t "^Workflow loading workflow loading journey$"'`. Vitest matches `-t` against the full `describe` + `it` name, so the recorded `-t "^workflow loading journey$"` selects nothing and skips the file.
- `mise run docs:check`
- `mise run gate`, then `git diff --check`

### Gate failure diagnoses

**check:mocks-scope** (first gate run; every test passed before it).
- Symptom: `scripts/checks/mocks-scope.sh` flagged wiremock in `skald-workflow` (`Cargo.toml`, `src/workflow.rs`), `skald-providers/src/endpoint.rs`, `wyrd-client/src/workflow/mod.rs`, and `wyrd-client/tests/workflow_transport.rs`.
- Evidence: each hit is a `[dev-dependencies]` wiremock used only inside a `#[cfg(test)] mod tests` or a `tests/` target.
- Cause: earlier Workflow route tests were not added to the allowlist.
- Fix site: the check's documented per-file allowlist, following the `wyrd-client` `cards/handle.rs` and `tests/storage_dispatch.rs` precedent (`9b3fb0238`, approved by the team lead).

**check:ci-selection** (second gate run; 52 passed, 1 failed).
- Symptom: the case "shared Skald change packages Python and server" expected `package_typescript=false` for `crates/skald/skald-cache/src/lib.rs`, but the selector reported `true`.
- Evidence (read-only diagnostician, given only the command, trace and diff): `cargo tree --offline -i skald-cache` shows `skald-cache → skald-runtime → wyrd-client → wyrd-sdk-ts` and `skald-cache → skald-runtime → skald-agent → skald-workflow → wyrd-client → wyrd-sdk-ts`. All eight Skald crates now reach `wyrd-sdk-ts`. `select-ci.py` `closure()` follows normal and build edges and sets `package_typescript` when `wyrd-sdk-ts` ships, which is correct.
- Cause: `wyrd-client` now depends on `skald-runtime`, `skald-workflow`, `skald-tool` and `skald-providers`. The spec requires this so TypeScript `Workflow.fromPath`/`cards.workflow.load` can run locally through the shared facade (`sdks/wyrd-sdk-ts/native/src/workflow.rs` wraps `wyrd_client::Workflow`). The case's premise that Skald is outside the TypeScript cone is stale.
- Fix site: that single expectation in `.github/scripts/tests/test-detect-changes.sh`. It is retitled "shared Skald change packages every SDK and server" and now expects `package_typescript=true` (approved by the team lead). No other case rests on that assumption, and the selector is unchanged.

**check:from-pools-allowlist** (third gate run).
- Symptom: `wyrd-server/src/components/workflow/tools.rs` test helper `state()` called `WyrdPostgres::from_pools` over a lazily connected pool.
- Cause: the helper built its own Postgres and storage owners instead of using the shared test-support constructors.
- Fix site: the helper now composes `test_support::{test_server_postgres, test_storage, test_catalog}`, the `query/service.rs` `state_without_oracle` pattern. The test already initialized that shared fixture through `test_catalog()`. No allowlist entry was added.
- Verification: `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --lib -E "test(=components::workflow::tools::tests::tools_use_the_prepared_run_deadline)"'` PASS.

### Limits

- Scenario 5 deviation (approved by the team lead): Scenario 5 asks every SDK journey to register through the compiled CLI `wyrd apply`. The Rust and TypeScript journeys cannot, because their test runtimes have no CLI binary; reaching one would need a new mechanism (a wyrd-cli dev-dependency, an in-test cargo build, or a `target/` binary lookup), and none was added. Those two journeys register through the SDK Cards `register_from_path`, the same Cards path `wyrd apply` calls, and add the local route coverage. The compiled-CLI `wyrd apply` and team-reuse proof lives in the wyrd-cli `workflow_journey` (`workflow_file_apply_registered_local`, which runs the binary via `CARGO_BIN_EXE`). The Python journey runs the installed `wyrd apply`.
- Non-goals stayed excluded: no Python/TS server-run lifecycle, MCP Workflow surface, persistent queue/recovery, credential administration, or compatibility aliases.
