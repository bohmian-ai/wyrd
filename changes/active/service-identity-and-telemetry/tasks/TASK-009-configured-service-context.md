---
id: TASK-009
spec: SPEC-service-identity-and-telemetry
spec_revision: 16
status: complete
kind: implementation
depends_on: [TASK-001, TASK-002]
requirements: [REQ-054, REQ-055, REQ-056, REQ-006, REQ-008, REQ-011, REQ-013, REQ-014, REQ-015, REQ-016, REQ-017, REQ-032, REQ-033, REQ-049, REQ-050, REQ-052, REQ-053]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008]
acceptance: [AC-026, AC-025, AC-002, AC-003, AC-004, AC-005, AC-010, AC-011, AC-024]
---

## Outcome and Value

A Service A application configures its release and environment once. Its
Service-bound check and Agent-bound check retain that same Service ownership,
while their subjects and binding occurrences remain distinct, including when
a check invokes the gateway or resumes after restart. This follows the assumed
implemented TASK-002; do not repeat its stable-account or delegation rewrite.

## What Changes and What Must Stay True

Workloads and family restrictions accept only Service Cards. Agents remain
ordinary registered subjects in Service component graphs. One Service-family
account authorizes v1 and v2; each loaded bundle selects its exact release.
No workload_kind, Agent root, version principal or larger JWT descriptor graph.

Add the public state.configure contract from REQ-050 on Rust/Python/TypeScript:
bundle defaults, matching explicit name/version, explicit/environment-variable/
null environment precedence and immutable local state before first use. Empty
explicit values fail. A later configure conflicts even after shutdown. Reuse
existing local bundle validation; no registration/authentication during load or
configure. Multiple configured states retain independent context and buffers.

Carry optional deployment_environment as typed invocation context.
Gateway calls and native Bifrost
gRPC writes use the optional x-wyrd-deployment-environment header/metadata; OTLP
uses the deployment.environment.name resource attribute; typed verification/MCP
requests use deployment_environment. No principal or tenant input is introduced. Preserve
the accepted value through gateway capture, queues, delegation/origin handling
and restart; token refresh cannot substitute a new environment. Activity stays
Service/account scoped, with no environment fanout or new group key. Table
columns, descriptor projection and physical defaults belong to TASK-003.

Use WYRD_AUTH_401_WORKLOAD_UID_REFUSED for non-Service/foreign/unknown root
selection without existence leakage; restriction kind validation uses the
existing typed restriction-validation error. Name/version mismatch and explicit
empty configure fields use WYRD_CLIENT_400_CONFIG_INVALID. Add the public
WYRD_SDK_409_STATE_CONFIG_FROZEN conflict for configuration after first use,
with remediation to configure before startup or construct another state. Runtime
schemas, generated error consumers and authored stubs must expose these errors.

## Approved stock-client API and example

Implement Python `state.http_auth` and TypeScript `state.httpAuth` as state-bound
forms of existing WyrdClient auth adapters. Add neither a public state.client
accessor nor an OpenAI/Anthropic-specific Wyrd client. Accessing the adapter
freezes lazy configure defaults without network authentication. Existing plain
client adapters and provider credential-slot mode remain supported.

The [auth supplement's approved Python/TypeScript example](../supplements/auth-system.md#stock-provider-sdk-context-and-approved-examples)
is this task's public API contract. It shows both provider clients constructed
once, configuration/startup, a Service Run and Agent Run, automatic headers and
expected captured dimensions. Implement the example, not a manually tagged
substitute. TASK-003 supplies full gateway-row projection after this task.

## Approach

1. Narrow existing root/family validation to Service without changing Card kinds.
2. Extend WyrdState's existing immutable client/Run context with local configure.
3. Extend state-bound HTTPX/HTTPX2 and fetch adapters to inject current Run
   context before token refresh; close sync/async and plain-client compatibility.
4. Freeze environment and exact binding occurrence in accepted execution inputs.
5. Close public language/HTTP/MCP surfaces, owning authority and focused proofs.

## Ordered Implementation Scenarios

### Scenario 1 — Accounts select Service releases, never Agent workloads

**Behavior.** One family account can select Service A v1/v2 but not another
Service, an Agent root, an Agent-family restriction, or a foreign/unknown UID.
Rootless authorized Card operations and Agent component observations still work.

**RED.** Add the planned Service-only authority test in [server proof](#server-proof);
expect existing two-kind root/family acceptance to fail the rejection assertions.

**GREEN.** Narrow existing account, issuer, header, federation and delegated-root
checks; keep signed Card-access states, grants and bounded graph resolution.

**REFACTOR.** Reuse the existing restriction/root validators across ingress.

### Scenario 2 — Configure establishes one immutable local context

**Behavior.** Omitted name/version match the bundle; explicit mismatches/empty
values fail. Environment precedence is explicit, then WYRD_DEPLOYMENT_ENVIRONMENT,
then null. Configure before use succeeds offline; first implicit use applies the
same defaults. Later configure fails, including after shutdown. Two states with
different releases/environments never overwrite each other. Automatic telemetry
startup against another state-bound or application-owned global provider refuses
with WYRD_SDK_409_TELEMETRY_PROVIDER_EXISTS, rather than exporting under the wrong
Service; the application retains its own providers and standard OTLP exporters,
with resource Service/version/environment and supported authentication. This
application-managed path does not require a state-bound exporter or state client
accessor. The server validates optional UID context independently of labels.

**RED.** Add [client proof](#client-proof) and the configure cases in the planned
SDK journeys below; missing API/defaults/freeze behavior must fail first.

**GREEN.** Extend existing state/client/Run lifecycle and thin SDK projections,
including telemetry started outside a Run. Preserve the application-managed
standard OTLP path and existing provider ownership. Rerun Scenario 1.

**REFACTOR.** Keep one existing state-owned context; remove exporter-specific
configuration copies rather than adding an ambient context singleton.

### Scenario 3 — Service and Agent checks retain distinct subjects and bindings

**Behavior.** Root Run verifies the Service-bound Verifier; component Run
verifies the Agent-bound Verifier. Exact Service, subject and binding occurrence
survive direct/submitted/gateway-backed calls, renewal, queue retry and restart.
Direct and queued verification capture one context
{service_name, service_version, deployment_environment} at acceptance. Queued
work stores it with the accepted execution; both paths write it unchanged to
the result and inputs rows. Nothing else reads it.

**RED.** Add the accepted-context cases in [server proof](#server-proof) and
[SDK proof](#sdk-proof) and [MCP proof](#mcp-proof); missing binding occurrence or frozen environment fails.

**GREEN.** Extend existing invocation contracts and accepted snapshots;
retain correlation context for inputs/results through restart. TASK-003
later proves full table projection; this task proves public invocation/readback
and the real accepted-execution seam. Rerun Scenarios 1–2.

**REFACTOR.** Reuse existing binding resolution and queue recovery. Do not infer
Service ownership from shared subject membership, accounts or gateway models.

### Scenario 4 — Stock provider clients inherit context on every request

**Behavior.** Create OpenAI and Anthropic clients once with state.http_auth or
state.httpAuth. Outside a Run use the Service subject and omit Run ID; inside
Service/component/nested Runs use the current exact subject/Run with unchanged
configured Service/version/environment. Snapshot context before token-refresh
await; concurrent states and overlapping Runs cannot cross-label calls. Remove
placeholder vendor credentials and send exactly one Wyrd bearer. Include active
W3C context without overriding explicit trace context. Matching explicit identity
headers work; conflicting root/subject/Run fails locally before any
request. Differing environment tags pass through unchanged. No readable-name
headers or shared default-header mutation is needed.

Python sync/async clients and both HTTPX bases use the existing adapter; TS uses
the existing fetch hook. Refreshing credentials never changes the dispatched
request context. Transport retries and stream completion preserve its snapshot;
provider SDK retries inside the active Run use the same context, without a new
cross-request deduplication guarantee. Existing plain client adapters retain
explicit subject/Run behavior; raw api_key mode still accepts explicit context.

**RED.** Add the planned [adapter proof](#stock-client-adapter-proof) and
[provider SDK journeys](#stock-provider-sdk-proof). The current hooks inject only
bearer/workload and have no state-bound accessor, so automatic subject/Run/env/
trace, snapshot-before-refresh and conflicting-header assertions fail.

**GREEN.** Extend existing shared state/client context and HTTPX/HTTPX2/fetch
adapters plus authored stubs/generated projections. Test actual OpenAI/Anthropic
clients against WyrdTestServer and local provider mocks. This task proves wire
context, auth/refusal and invocation behavior; TASK-003 adds full canonical Bifrost
row readback so the dependency remains one-way. Rerun Scenarios 1–3.

**REFACTOR.** Reuse the existing authentication/token cache and context owners;
remove duplicated header construction from touched examples. Do not mutate the
original caller client, add provider wrappers or invent context from cached calls.

## Acceptance Criteria

| Obligations | Required proof and failure |
| --- | --- |
| AC-026; REQ-050–053 | Managed startup preserves frozen state context and refuses an existing application provider without replacing or flushing it; the application-managed alternative uses standard resource settings and authenticated OTLP, not a state-taking exporter. Task003 owns final three-signal projection/readback. |
| REQ-006,008,011,033,049; AC-002,003,011 | Server proof rejects Agent roots/families across supported ingress while one Service-family account selects both releases and preserves grants. Broader root authority is failure. |
| REQ-013,032,050; AC-010,024 | Client and three SDK proofs exercise defaults, mismatch/empty/late refusal and concurrent states. Network IO during configure or mutable global context is failure. |
| REQ-014–017,052–053; AC-004,005,024 | Real accepted-work proof preserves Service/Agent subjects and exact bindings; direct/queued inputs/results read back the one accepted context unchanged, including optional environment and queued restart. Worker relabelling, authority widening or implicit schedule fanout is failure. |
| REQ-054–056; AC-025 | Adapter/unit proof pins exact outgoing headers and context before refresh; real provider SDK journeys prove Service/Agent dispatch, credential replacement, sync/async/concurrency/retry/stream behavior and local Service/subject/Run conflicts without upstream requests; differing environment tags pass through unchanged. Static code examples must match those journeys. TASK-003 owns canonical row readback. |
| INV-001–008 | Existing tenant/permission/Card-scope/delegation and accepted-authority regressions remain exact. TASK-003 owns full cross-signal schema/readback proof. |

## Implementation Reference

- Existing wyrd-client state.rs WyrdState::{from_path_with,client,pinned,run,
  run_for_card,bound_verifier}, observe/mod.rs Run::for_card and Observe::verify
  already pin a Service bundle and select subjects. The packet's Agent workload
  root allowance is wider than the actual loaded-state root. Extend these owners,
  preserve the exact binding occurrence instead of returning only Verifier/subject.
- Rust SDK otel.rs::start_telemetry constructs the provider from state.client()
  without configured resource context; Python state/mod.rs::start_telemetry and
  Python wyrd/otel.py use the same client-bound path. Extend those existing
  exporter/correlation owners for Wyrd-managed telemetry; application-managed
  providers supply their own standard resources and OTLP exporters. Do not add
  a state-taking exporter, global resource configuration or telemetry pipeline.
- wyrd-client WyrdClient::for_workload/auth renewal already isolate UID context.
  Extend propagation without replacing its transport/cache. Current claims and
  issuer/header/federation validators own Service-only enforcement.
- Server components/verification/service.rs direct result handling currently
  records binding_id None; state::bound_verifier resolves Service/component/Agent
  bindings. Carry the resolved occurrence through the existing request and
  AcceptedAuthority/execution-input/queue owners. The verification `results`
  and `execution_inputs` tables receive `deployment_environment` here; TASK-003
  owns the remaining row columns.
- Python client/_http_auth.py::HttpAuth already derives from httpx.Auth and
  httpx2.Auth, with sync/async flows sharing _authenticate; it strips vendor key
  headers and sets bearer/workload only. WyrdClient.http_auth in src/client.rs
  creates it with a bound client. Extend this existing owner with state context;
  add the state getter in PyO3/state stubs, not a public client accessor.
- TS WyrdClient.httpAuth in wyrd/src/index.ts already wraps fetch and refreshes
  its bearer. WyrdState.startTelemetry accesses its native bound client privately;
  reuse the native state context for the new state.httpAuth getter and existing
  Run.scope/forCard context. Include authored/generated declarations and type proof.
- Existing Python unit/client/test_http_auth.py and integration/gateway/
  test_client_http_auth.py cover HTTPX/HTTPX2 and actual OpenAI/Anthropic auth;
  TS integration/gateway-inference.test.ts covers both auth modes and explicit
  Card/Run headers. Extend these owners with state-bound context instead of
  adding SDK dependency, auth workflow or test binary. Repository provider SDK
  dependencies already include HTTPX2; test actual locked versions.
- Existing SDK tests/integration/observe_a_run.rs, Python test_observe_a_run.py
  and TS observe-a-run.test.ts drive the real server and shared checked-in Card
  fixtures. Existing pg_verification_runtime module is test-support gated and
  exercises recovery. MCP tests/bifrost/mcp/verification.rs::pg_tests already
  discovers/executes/enqueues Verifiers and reads executions through real rmcp
  clients. Extend them; do not add another test binary.

## Verification and Evidence

All added selectors below are **planned**. Manifests confirm package wyrd-ai
(the Rust SDK), target integration and module observe_a_run; wyrd-client lib;
wyrd-server integration with test-support and pg_verification_runtime. Python
and TS use their existing story runners and server fixture setup. Confirm actual
selectors after adding tests, before RED; empty selections are failure. Cargo
commands sharing this checkout run sequentially.

### Client proof

```bash
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=state::tests::configured_service_context_is_immutable)'
```

### Server proof

Real Postgres and accepted authority/recovery, including root/family negatives:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration -E "test(=pg_verification_runtime::configured_service_context_survives_acceptance_and_restart) | test(=pg_verification_runtime::workload_roots_and_families_are_services_only)"'
```

### SDK proof

Real SDK → WyrdTestServer + Postgres → SDK; assert Service versus Agent subject,
exact binding, accepted context and stable error codes using local provider mocks:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=observe_a_run::configured_service_context_reaches_both_verification_subjects)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run py:setup && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_observe_a_run.py::test_configured_service_context_reaches_both_verification_subjects'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run ts:build && mise run ts:build:testing && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/observe-a-run.test.ts -t "^configured service context reaches both verification subjects$"'
```

### Stock-client adapter proof

Planned Python test in existing unit/client/test_http_auth.py and TS test in
existing tests/unit/wyrd-client.test.ts. Exercise both HTTPX types, async token
refresh, concurrent scope snapshots, header conflicts and trace precedence;
these are language-runtime behaviors, not Rust emulations.

```bash
mise exec -- bash -lc 'cd sdks/wyrd-sdk-python && uv run python -m pytest -q tests/unit/client/test_http_auth.py::test_state_auth_snapshots_context_before_token_refresh'
mise exec -- bash -lc 'cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/unit/wyrd-client.test.ts -t "^state httpAuth snapshots context before token refresh$"'
```

### Stock-provider SDK proof

Planned cases in the existing Python and TS gateway files. Use the same checked-in
Service bundle and existing local provider fixture; cover OpenAI/Anthropic,
Python sync/async, Service/default/Agent subjects, concurrent states, renewal,
retry/streaming, and no header leakage or request on local conflict. Do not add
live provider credentials. Run py:setup / ts:build and ts:build:testing first.
Exact canonical row readback is separately Task003's gateway proof.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run py:setup && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/gateway/test_client_http_auth.py::test_state_http_auth_supplies_service_and_agent_context'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run ts:build && mise run ts:build:testing && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/gateway-inference.test.ts -t "^state httpAuth supplies service and agent context$"'
```

The new provider/unit selectors above are **planned**, not existing. Their files,
runners and setup were inspected. Confirm actual selectors after adding them
and before RED; record exact selected counts. Existing direct credential-slot
and plain adapter cases remain regression coverage, not substitutes for the
new state-bound usage example.

### MCP proof

Planned test in the existing mcp target and verification::pg_tests module;
exercise scoped tools, unchanged optional environment tag readback, exact
Service/Agent subjects and get_execution readback with no queue residue after
authority refusal:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E "test(=verification::pg_tests::configured_service_context_keeps_subject_and_environment)"'
```

Static/generated obligations do not need manufactured RED. Run format/lints for
the touched owners, codegen:check, check:deps, check:tenant-isolation and affected
Python stub/TypeScript type checks through mise tasks:

```bash
mise run fmt
mise run lints
mise run codegen:check
mise run check:deps
mise run check:tenant-isolation
mise run py:format
mise run py:lints
mise run py:typecheck
mise run ts:format
mise run ts:lints
mise run ts:typecheck
mise run ts:napi:check
```

Record exact named-test
counts and RED/GREEN, without claiming TASK-003's later warehouse proof here.
HTTP execution uses real SDK/server proofs; MCP uses the focused proof above;
full shipped-surface integration runs once on the integrated candidate.

## Material Stop Conditions

Stop for any new authorization state, token descriptor expansion, environment-
based grant, automatic schedule fanout, new group identity or global mutable
telemetry context. Coordinate Task003 projection rather than creating a second
schema/writer. A required new public behavior outside these decisions returns
to the spec owner.

## Authority Links

[Approved revision 16](../spec.md), [auth](../supplements/auth-system.md),
[storage/environment](../supplements/table-storage-and-correlation.md),
[telemetry](../supplements/telemetry-schemas.md), architecture/wyrd-design.md
Runtime identity, architecture/wyrd-security-posture.md, AGENTS.md,
changes/spec-driven-development.md, changes/implementation-execution.md and
TESTING.md. TASK-002 evidence remains historical; this task does not certify it.

## Revision-16 boundary

The telemetry setup clarification is maintainer-approved. The implementation
evidence below remains historical and does not certify the revised two-path
proof. TASK-003 owns canonical OTLP projection/readback; the review remediation
owns the bounded runtime context/admission defects already found.

## Implementation Evidence

Commits `43cdb7405`..`b1ff72dc3` on `task/service-identity-and-telemetry/TASK-002`.
Final GREEN re-run on `b1ff72dc3`; each named selector selected exactly 1 test
(the TS unit file: 3 tests, the Python adapter file: 4).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-006,008,011,033,049; AC-002,003,011: Service-only roots and families | `43cdb7405` (server auth, account restriction, exchange) | `pg_principal_roles::workload_roots_and_families_are_services_only` (the planned `pg_verification_runtime` path was wrong; this is its real owner) 1/1 PASS; RED not captured | PASS |
| REQ-013,032,050; AC-010,024: immutable configure, three SDKs | `8c70d0c53` (`wyrd-client` `state.rs` `configure`/`pinned`, Python/TS projections); `b1ff72dc3` `WyrdClient::deployment_environment` so `pinned` builds without `internal` | `state::tests::configured_service_context_is_immutable` 1/1 PASS; the SDK journeys below assert `WYRD_SDK_409_STATE_CONFIG_FROZEN` | PASS |
| REQ-014–017,052–053; AC-004,005,024: Service vs Agent subjects, frozen environment, restart | `76eb5bb90` (migration `20261026000000`, verification service reconciliation), `87453d2b7`, `cffa73b31` | `pg_verification_runtime::configured_service_context_survives_acceptance_and_restart` 1/1 PASS (RED: conflict accepted with 202 when enqueue reconciliation was disabled); Rust `observe_a_run::configured_service_context_reaches_both_verification_subjects` PASS (RED: SDK environment dropped); Python and TS same-named journeys PASS; MCP `verification::pg_tests::configured_service_context_keeps_subject_and_environment` PASS (passed on first run, so no RED) | SUPERSEDED: environment reconciliation/refusal proof; requires direct/queued context pass-through readback across restart. |
| REQ-054–056; AC-025: state-bound stock-client auth | Python `31d8f7d1a` (`WyrdState.http_auth`, `_STATE_RUNS`); TS `49cb746e7` (`WyrdState.httpAuth`, `STATE_RUNS`) | Python `test_state_auth_snapshots_context_before_token_refresh` PASS (RED: snapshot after refresh gave the wrong run id). TS unit, renamed `state httpAuth carries its own service, run, and environment` because TS resolves context synchronously before the native token await, PASS (RED: without the per-state Run filter, a nested other-state Run relabelled the request). Python `test_state_http_auth_supplies_service_and_agent_context` and TS `state httpAuth supplies service and agent context`: OpenAI sync/async, Anthropic streaming, concurrent states, local conflict with no upstream request, no header leakage, `vala.gateway.calls` card/run; PASS. In TS the provider SDK wraps the local refusal as `cause` of its connection error | SUPERSEDED for environment-conflict refusal; retain unrelated adapter evidence and prove differing-tag pass-through readback. |
| INV-001–008: existing regressions | `0c294f797`, `9ea25faee`: journeys whose premise was "an ambient admin or Agent-rooted writer is refused" now use a stale bundle outside the Service graph, a Service runtime account, and a per-test slice of the shared account | Rust `observe_a_run` + `gateway_inference` 19/19; Python observe/asgi/gateway targeted PASS; TS `observe-a-run` 13/13, `gateway-inference` 14/14; `wyrd-client --lib` 266/266 | PASS |
| Static and generated | `b1ff72dc3` regenerated `docs/src/content/docs/api/errors.md` and `docs/public/llms-full.txt` | fmt, lints, codegen:check, check:deps, check:tenant-isolation, py:format, py:lints, py:typecheck, ts:format, ts:lints, ts:typecheck, ts:napi:check, docs:check, `git diff --check`: all exit 0 | PASS |

Non-goals stayed excluded: no new authorization state, no token descriptor
growth, no environment-based grant, no schedule fanout, and no global mutable
telemetry context (Run context is per state: a Python state token and a TS
state reference). `deployment_environment` on verification `results` and
`execution_inputs` is implemented here; TASK-003 owns the gateway, drift and
audit environment columns, the other common dimensions and canonical row
readback.

Residual risks:
- A binding occurrence is matched by (owner, subject, verifier); explicit work
  records no binding id.
- Scenario 1 and the MCP proof have GREEN evidence only.
- The stale-premise journey repairs (`0c294f797`, `9ea25faee`) were diagnosed
  by the implementer against `wyrd-security-posture.md` (unrestricted accounts
  are `any_registered`; family-restricted Services are limited to their
  graph). The repository owner waived the independent diagnostician.
