---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-agents-trace-processor
spec_revision: 2
requirements: [REQ-010, REQ-011, REQ-012, REQ-013, REQ-014, REQ-015, REQ-016, REQ-017, INV-001, INV-002, INV-003, INV-004, AC-001, AC-003, AC-007, AC-008, AC-009, AC-010, AC-011, AC-012, AC-013]
depends_on: []
---

# Connect real agent instrumentation to Wyrd through standard OTEL

## Outcome and Value

One complete application workflow: configure Wyrd ambient auth once, connect
native framework instrumentation or the explicit OpenAI bridge, execute normally
with optional Run/Card scope, flush, and retrieve correctly related telemetry.
Prove Python Pydantic AI, Google ADK, and OpenAI Agents, plus TS OpenAI. Ship
current-convention docsite recipes alongside working runtime behavior.

This is the single task requested by the owner. It owns every obligation in
approved spec revision 2; the existing branch implementation is its starting
point, not evidence that the expanded outcome already passes. No task dependency:
the merged ambient-auth implementation is already an ancestor of this branch.

## Owners, Scope, Consumers, and Prohibited Changes

Runtime owners are the Python SDK OTEL module and WyrdState binding, the TS
OTEL utilities and WyrdState/Run wrappers, and the existing shared error catalog.
Consumers include actual frameworks, application-owned providers, public
Python typing/TS exports, package import checks, telemetry tests, docs readers,
and existing capability lane selection.

Keep shared auth and durable behavior in Rust owners. Native runtime glue is
justified by Python/Node provider and callback lifetimes; do not initialize
Python/Node inside Rust tests. No new server behavior, wire payload, table,
migration, framework discovery, monkey-patching, framework registry, transport,
export queue, token cache, Pydantic/ADK adapter, or Rust framework integration.
Do not revive removed ambient-auth helpers or publish existing draft docs.
Paths below guide ownership, not private implementation structure.

### Source-backed reuse map

Paths are repository-relative. Each row follows the workflow through consumers.

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Managed export | Python `python/wyrd/otel.py::_start_telemetry`, `span_exporter`; `src/state/mod.rs::start_telemetry`; TS `wyrd/src/otel.ts::startTelemetry`, `spanExporter`; `src/index.ts::WyrdState.startTelemetry` | Python `tests/integration/test_local_development.py`, TS `tests/integration/agents-trace-processor.test.ts` | Real native-framework proof; failed setup resource cleanup | Extend existing setup/exporter owners; preserve collision refusal | No replacement pipeline needed |
| Owned-provider correlation | Python `otel.py::install_run_correlation`, `_RunCorrelationProcessor`; TS `src/otel.ts::runCorrelation`; Python/TS Run scope | Python `tests/unit/state/test_observe_surface.py`, `tests/integration/observe/test_observation_surface.py`; TS `tests/integration/observation-surface.test.ts` | Failed Python registration leaves success marker; TS processor private | Retry-safe Python installation; public TS construction-time processor factory; same context | No new context store or lifecycle owner |
| Flush/shutdown | Python `src/state/mod.rs::flush/shutdown`; TS `src/index.ts::WyrdState.flush/shutdown`; shared `crates/shared/wyrd-client/src/state.rs::flush/shutdown` | Existing local-development, observation, OpenAI journeys | State flush omits OTEL; Python ignores false; OTEL failure bypasses native drain | Complete runtime-local sequencing with existing native drains and catalog errors | No shared durable lifecycle rewrite |
| OpenAI conversion | Python `otel.py::WyrdAgentsTraceProcessor`; TS `src/index.ts::WyrdAgentsTraceProcessor` | Both `test_agents_trace_processor.py` / `agents-trace-processor.test.ts` | Timing/content-safe error mapping, callback lifecycle, real provider/tool/stream proof | Extend retained adapters; read final attributes at end; preserve start-time scope | Framework's callback format earns this bridge only |
| Ambient auth | `crates/shared/wyrd-client/src/client.rs::WyrdClient::access_token`, `config.rs::resolve_credential`, `state.rs::client`; Python `client/_http_auth.py::HttpAuth`; TS `src/index.ts::WyrdClient.httpAuth` | Python `tests/unit/client/test_http_auth.py`, `tests/integration/gateway/test_client_http_auth.py`; TS `tests/unit/gateway.test.ts`; stock-client docs | New telemetry journeys currently inject explicit clients and custom models | Ambient state/client resolution; existing http_auth/httpAuth for stock gateway client; existing exporter per-request access_token | No new credential resolver/header adapter/cache |
| Real framework proof | Python `tests/integration/otel/test_agents_trace_processor.py`; `tests/integration/conftest.py::observed/observed_with_bifrost`, `fresh_tracer_provider`; TS `tests/support/observed-service.ts::observedServiceTest`, `tests/support/server.ts::serverTest` | Existing OpenAI custom model emits generation spans itself; observation tests use synthetic framework tracers; MCP framework tests exercise tools only | Three Python telemetry integrations; stronger TS bridge proof | Actual framework/provider instrumentation and local upstreams with existing server fixtures | No new harness; local fixture shape stays implementer-owned |
| Errors | `crates/wyrd-spec/src/error.rs::WyrdError`, including `SdkTelemetryProviderExists` | Python/TS runtime structured errors and state lifecycle consumers | No approved telemetry timeout/failure codes | Add only spec-approved derive-backed catalog errors and regenerate projections | Shared catalog extension, no parallel error system |
| Docs and lanes | Observe `record-a-run-and-observations.md`; Connect `call-wyrd-from-a-stock-client.md`; `docs/CONTRIBUTING.md`, `docs/PRODUCT.md`; `mise.toml` SDK/Bifrost leaves | Current language tabs, auth recipes, docs generation/checks; Python/TS explicit Bifrost test file lists | Native-framework/owned-provider recipes; wrong install name; OpenAI tests absent from capability leaves | Extend current task pages, links and generators; collect required journeys with optional deps | No new docs subsystem, permanent checker, or broad test framework |

Inspection baseline: Python lock has OpenAI Agents 0.23.1, Pydantic AI slim
2.56.0, ADK 2.12.0; TS Agents core 0.20.0 and OTEL SDK 2.11.0. Check these
against current locks when executing. Python's existing `mcp-frameworks` group
contains all three frameworks and requires Python >=3.11; it is not installed
by ordinary `py:setup`. TS OTEL 2 uses provider construction-time processors.
Respect these facts when arranging collection, typing, setup, and exact proof.

## Approach

1. Revalidate owners and reuse map; establish focused tests and positive
   collection counts for the expanded obligations.
2. Complete managed/owned-provider setup and correlation, preserving identity
   selection and the installed OTEL SDK's extension mechanisms.
3. Fix runtime flush/drain semantics and OpenAI callback/timing/privacy behavior
   within existing owners; project approved errors through the shared catalog.
4. Exercise real framework model/tool execution, streaming and failure with
   local upstreams; reuse ambient clients and stock auth transports on Wyrd calls.
5. Close optional-dependency, sampling, correlation, permission, and tenant edges;
   wire existing owner/capability lanes without silent skips.
6. Update current docsite workflows, public typing/exports, durable authority,
   and source-driven generated artifacts; run focused checks and audit closure.

## Ordered Implementation Scenarios

Proof is mixed: runtime changes use TDD; existing-correct behavior uses
verification-only evidence. Documentation/generation/lane closure uses static
proof below. Do not manufacture RED by damaging working behavior.

The command IDs below refer to exact commands in Verification and Evidence.
Planned test names are executable handoff selectors, not existing passing tests.
They may be moved/renamed only with an equivalently focused command recorded in
task evidence. Fixtures, private production shape, and parametrization mechanics
remain implementer-owned.

### Scenario 1 — One managed provider, optional packages

**Behavior.** Same-state setup is idempotent; existing/other-state provider is
preserved and refused; rejected setup leaks no resources. Generic imports work
without framework packages; missing OTEL setup names the correct extra.
REQ-011/012, INV-002/004, AC-007/008.

**RED.** Add/select the setup/optional-import cases in planned `test_telemetry_setup`
and TS `telemetry setup`, commands U1/T1. Confirm the missing cleanup or contract
case fails for behavior, not a missing binary/package. Existing collision/import
cases can pass as verification-only evidence.

**GREEN.** Complete existing setup/refusal paths without registering frameworks,
replacing global application configuration, or attaching a second exporter.
Rerun U1/T1 with positive collection counts.

**REFACTOR.** Simplify within the existing provider owners; keep optional imports
lazy or compatible with the existing optional boundary. No framework detector.

### Scenario 2 — Application-owned providers compose correlation

**Behavior.** Explicit providers keep resource/sampler/other export configuration
and own their lifecycle. Python registration can retry after failure; repeated
success is idempotent. TS users compose the approved factory at construction.
REQ-012/014, INV-003/004, AC-008/010.

**RED.** Planned `test_owned_provider_correlation` / TS `owned provider correlation`,
U2/T2, fails on Python's failed-registration marker and missing TS public factory.
Include nested/concurrent scope restoration without globally replacing providers.

**GREEN.** Extend current correlation owners and public exports/types. Rerun
U1/U2 and T1/T2. Later J2/K2 supplies the real export/readback proof.

**REFACTOR.** Share managed and owned-provider correlation implementation within
each runtime; no alternate context registry or unsupported provider mutation.

### Scenario 3 — Flush completed telemetry, then drain even on failure

**Behavior.** Managed flush/shutdown orders OTEL before native drain, recognizes
false/timeouts/exceptions, applies approved primary/secondary errors, leaves
active spans active, and preserves the installed provider after state shutdown.
Application-owned providers are untouched. REQ-013, INV-003/004, AC-009.

**RED.** Planned `test_managed_telemetry_lifecycle` / TS `managed telemetry lifecycle`,
U3/T3. Inject false, rejected flush, both-stage failure and native-only failure;
assert drain attempt and structured codes. Current omissions must fail.

**GREEN.** Complete existing state methods and catalog projections; use configured
OTEL bounds. Rerun scenarios 1–3. J1/J2/K1/K2 later prove successful readback
while alive and continued provider usability through real IO.

**REFACTOR.** Keep runtime-specific orchestration cohesive with state ownership.
No duplicate native drain, timeout policy, error catalog, queue, or provider owner.

### Scenario 4 — Faithful, content-safe OpenAI callback bridge

**Behavior.** Hierarchy/enclosing context, timestamps, final GenAI data, safe
error status, and start-time Run identity survive conversion. Duplicate/missing
callbacks and repeated shutdown are safe; unfinished spans are released and late
callbacks ignored. Python callbacks are thread-safe. REQ-010/014, INV-003,
AC-007/010/011.

**RED.** Planned `test_agents_callback_lifecycle` / TS `agents callback lifecycle`,
U4/T4, exercises duplicate starts, completion in another scope, timestamps,
late fields, shutdown, unknown type, missing parent, and sensitive sentinels.
Existing overwrite/no-cleanup/timing/raw-error cases fail for the stated cause.

**GREEN.** Extend retained adapters through the current provider. Rerun scenarios
1–4. Preserve generation and response mappings; do not synthesize missing data.

**REFACTOR.** Remove obsolete no-op cleanup and duplicated mapping logic where
cohesive. Keep callback work quick and free of network/provider lifecycle IO.

### Scenario 5 — Real framework execution through ambient Wyrd setup

**Behavior.** All three Python frameworks, and TS OpenAI, emit real model/tool
spans, flush while alive, and read back through the SDK. Check exact Card/Run,
parent IDs, enclosing trace, model and supplied input/output usage, and no
logical duplicates. Both managed and application-owned providers work.
REQ-010/011/012/016/017, INV-001/004, AC-001/007/008/009/013.

**RED.** J1/J2/K1/K2 target planned real framework/provider journeys. Replace the
sole custom-model/manual-generation-span proof with actual provider-model
instrumentation against local upstreams. Missing native recipes/owned correlation
and incomplete assertions supply RED; capture existing-correct cases as regression.

**GREEN.** Complete the setup recipes and seams using stock framework APIs.
Wyrd client/state/query/export resolve ambient credentials in isolated test
configuration. OpenAI journeys in Python and TS route a stock model client through
real Wyrd gateway with http_auth/httpAuth and a local upstream. Keep separate
native direct-provider cases proving telemetry does not require the gateway.
Run U1–U4/T1–T4, then J1/J2/K1/K2.

**REFACTOR.** Reuse server/provider/auth fixtures and public clients. Remove manual
framework-span compatibility substitutes, retaining focused logic tests.

### Scenario 6 — Errors, streaming, and scope edges remain attributable

**Behavior.** Actual framework model/tool failures and streaming completion retain
terminal data. Uncorrelated spans remain valid; nested and concurrent Runs keep
correct identity and ancestry, including completion after scope exit.
REQ-010/014/016, INV-001/003, AC-010.

**RED.** J3/K3 select planned framework edge journeys. Parameterize across all
three Python frameworks and TS OpenAI for supported ordinary/stream execution.
Failures must expose lost identity, parentage or terminal data, not absent remote
credentials. Existing-correct edges are verification-only.

**GREEN.** Correct only the in-scope SDK/bridge seam; preserve native framework
attributes instead of normalizing them. Rerun scenarios 4–6 and any changed
correlation tests from earlier scenarios.

**REFACTOR.** Reuse complete execution/readback paths with deterministic local
upstreams. Avoid sleeps, manual surrogate spans, or larger framework abstractions.

### Scenario 7 — Sampling, privacy, and authorization remain authoritative

**Behavior.** Native content controls and bridge metadata defaults exclude their
promised sensitive fields; any unavoidable native content is disclosed. Sampling
is honored. Denied export persists no records, and tenant A cannot attribute to
or read tenant B. REQ-014/016/017, INV-001/003, AC-011/013.

**RED.** J4/K4 select planned sampling/privacy/auth-negative journeys. Use sentinel
content and isolated scoped credentials; assert exporter/server refusal and SDK
readback, not only log messages. Do not count metadata-only SQL projection as
proof that payload was never stored. Existing server refusal can be regression.

**GREEN.** Configure upstream controls and safe bridge mapping; fix SDK auth/context
seams only. Preserve server authorization. Rerun scenarios 4–7 and check generic
uncorrelated telemetry still works under valid credentials.

**REFACTOR.** Remove redundant local policy/auth logic. No universal redaction,
second sampler, token-based tenant switching, or server permission relaxation.

### Scenario 8 — Ambient auth and fresh tokens without new auth plumbing

**Behavior.** Environment/global configuration supplies normal clients without
explicit credential arguments. Existing no-credential errors remain actionable.
Stock clients pointed at Wyrd use current http_auth/httpAuth, strip placeholders,
and carry one Wyrd credential; only the operator provider credential reaches the
local upstream. Repeated exports obtain tokens through the selected shared
client rather than a setup-time copy. REQ-017, INV-001/003, AC-013.

**RED.** J5/K5 provide ambient-auth gateway-to-telemetry and credential-negative
readback proof. Planned U5/T5 test exporter token freshness deterministically;
setup-time cached credentials would fail. R1/R2 are existing auth regressions,
not a reason to redesign their already-correct implementation.

**GREEN.** Reuse merged client resolution/transport and existing exporter auth.
No manual Authorization header, per-framework auth helper, long-lived token copy,
or Wyrd auth transport attached to an external provider. Rerun scenarios 5–8
and R1/R2. Keep shared renewal/cache in Rust.

**REFACTOR.** Delete redundant test/example token plumbing and reference the stock
client guide. Do not revive GatewayAuth/accessTokenSource/wyrdFetch aliases.

## Acceptance Criteria

| Spec obligations | Task owner / required outcome | Proof |
|---|---|---|
| REQ-010; AC-007 | SDK bridges preserve fidelity and safe callback lifecycle, optional imports | Scenario 1/4, U1/U4/T1/T4 plus J1/K1 |
| REQ-011; AC-001 | Three Python frameworks and TS OpenAI complete export/readback without manual internal spans | Scenario 5, J1/J2/K1/K2 |
| REQ-012; AC-008 | Setup idempotency, provider preservation, explicit owned correlation | Scenarios 1/2/5, U1/U2/T1/T2/J2/K2 |
| REQ-013; AC-009 | Managed lifecycle flushes correctly and drains on failure | Scenario 3, U3/T3 plus J1/J2/K1/K2 |
| REQ-014; AC-010/011 | Standard fidelity, scope, privacy and sampling | Scenarios 4/6/7, J3/J4/K3/K4 |
| REQ-015; AC-012 | Current docsite recipes and machine docs match implemented journeys | Non-TDD documentation proof below |
| REQ-016; AC-003 | Exact journeys collected with their dependencies, no masked integrations | Positive collection/results, lane closure and static checks |
| REQ-017; AC-013 | Ambient clients, stock auth transport, fresh per-export tokens | Scenario 8 plus all-framework ambient setup, U5/T5/J5/K5/R1/R2 |
| INV-001/002/003/004 | Tenant/auth/payload/provider/runtime boundaries all preserved | Scenario evidence plus dependency checks and final diff audit |

## Expected Write Set and Consumer Closure

- Python SDK: `python/wyrd/otel.py`, `src/state/mod.rs`, relevant authored
  `python/wyrd/state/__init__.pyi` and public export/type documentation;
  unit and integration telemetry tests and nearest support fixtures.
- TS SDK: `wyrd/src/otel.ts`, `wyrd/src/index.ts`, public declarations produced
  by the normal build, telemetry/bridge tests and nearby support.
- Shared catalog: `crates/wyrd-spec/src/error.rs` for the two approved errors;
  source-driven error code/document projections via existing generators.
- Existing dependency/test configuration as necessary to collect the three
  frameworks with installed groups; no new production dependency required.
  Existing `gateway`/`otel` extras and optional Agents boundary are sufficient.
- Existing `mise.toml` owner/Bifrost journey leaves and associated selection
  metadata, with required framework dependencies and no silent skip. Do not
  rename MCP tests as telemetry proof or weaken the default fast lane.
- Docsite Observe task page, related Connect/Understand cross-links, generated
  indexes/reference output, and the durable architecture sections owning the
  documented provider/lifecycle contract. Preserve draft publication state.
- Reuse merged ambient-auth owners and tests. Modify their production logic
  only for a proven in-scope defect; do not reimplement credential resolution.

## Verification and Evidence

### Focused command contract

Run commands from the repository root. One-time native setup is
`mise run py:setup`, and `mise run ts:build ts:build:testing` sequentially with
other Cargo work. For the three Python frameworks, the existing dependency
group requires a repository-supported Python >=3.11. These commands explicitly
request that group. The Postgres wrapper/migration precedes each exact journey;
no external provider credentials or live OIDC is needed.

Planning checked runner ownership against `mise.toml`, existing pytest/vitest
source, package metadata and the Postgres wrapper. New selectors below are
planned, not claimed to exist or pass. Before reporting evidence, collect the
exact selector and require a positive test count; a skipped matrix or zero-match
Vitest run is not proof. If a fixture requires additional repository-managed
setup, use the owning setup without weakening the test.

U1–U5 (planned additions in the existing Python unit owner):

```sh
# U1
mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/state/test_observe_surface.py::test_telemetry_setup
# U2
mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/state/test_observe_surface.py::test_owned_provider_correlation
# U3
mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/state/test_observe_surface.py::test_managed_telemetry_lifecycle
# U4
mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/state/test_observe_surface.py::test_agents_callback_lifecycle
# U5
mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/state/test_observe_surface.py::test_exporter_token_freshness
```

T1–T5 (planned runtime tests, fixture structure implementation-owned):

```sh
# T1
mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/unit/telemetry.test.ts -t '^telemetry setup$'
# T2
mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/unit/telemetry.test.ts -t '^owned provider correlation$'
# T3
mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/unit/telemetry.test.ts -t '^managed telemetry lifecycle$'
# T4
mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/unit/telemetry.test.ts -t '^agents callback lifecycle$'
# T5
mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/unit/telemetry.test.ts -t '^exporter token freshness$'
```

J1–J5 are planned tests in the existing Python telemetry directory. J1/J2/J3/J4
must cover all three frameworks through positive matrix collection; each command
selects that exact test and all its required parameter cases.

```sh
# J1
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- uv run --directory sdks/wyrd-sdk-python --group mcp-frameworks python -m pytest -q -m integration tests/integration/otel/test_framework_telemetry.py::test_managed_framework_telemetry'
# J2
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- uv run --directory sdks/wyrd-sdk-python --group mcp-frameworks python -m pytest -q -m integration tests/integration/otel/test_framework_telemetry.py::test_owned_provider_framework_telemetry'
# J3
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- uv run --directory sdks/wyrd-sdk-python --group mcp-frameworks python -m pytest -q -m integration tests/integration/otel/test_framework_telemetry.py::test_framework_execution_edges'
# J4
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- uv run --directory sdks/wyrd-sdk-python --group mcp-frameworks python -m pytest -q -m integration tests/integration/otel/test_framework_telemetry.py::test_framework_sampling_privacy_and_authorization'
# J5
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- uv run --directory sdks/wyrd-sdk-python --group mcp-frameworks python -m pytest -q -m integration tests/integration/otel/test_framework_telemetry.py::test_ambient_auth_gateway_telemetry'
```

K1–K5 extend the existing TS OpenAI telemetry journey owner:

```sh
# K1
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/agents-trace-processor.test.ts -t "^managed framework telemetry$"'
# K2
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/agents-trace-processor.test.ts -t "^owned provider framework telemetry$"'
# K3
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/agents-trace-processor.test.ts -t "^framework execution edges$"'
# K4
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/agents-trace-processor.test.ts -t "^framework sampling privacy and authorization$"'
# K5
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/agents-trace-processor.test.ts -t "^ambient auth gateway telemetry$"'
```

Existing per-request auth regressions, inspected in their current owners:

```sh
# R1
mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/client/test_http_auth.py::test_each_httpx_request_carries_a_fresh_token tests/unit/client/test_http_auth.py::test_async_requests_carry_a_fresh_token
# R2
mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/unit/gateway.test.ts -t '^httpAuth replaces provider key headers with a fresh access token on every request$'
```

The planned tests can use parameterized cases within these selectors. Record
which framework/provider/error/sampling/content case ran and its assertions.
Unit injections for runtime-local provider failure/unfinished callbacks are
allowed by REQ-016; they never replace successful or denied-export journeys.
Re-run failed tests with WYRD_LOG/RUST_LOG tracing as AGENTS.md requires.

### Owner/static proof

After focused GREEN, run the affected owner/static lanes: `mise run fmt`,
`mise run lints`, `mise run py:format:check`, `mise run py:lints`,
`mise run py:typecheck`, `mise run py:test:unit`, `mise run ts:format:check`,
`mise run ts:lints`, `mise run ts:typecheck`, `mise run ts:test:unit`,
`mise run ts:pack:check`, `mise run codegen:check`, and `mise run check:deps`.
If native TS exports change, also run `mise run ts:napi:check`; otherwise record
why its generated surface is unchanged. Regenerate catalog projections through
`mise run codegen:regen` before drift proof.

Do not run full journey suites or repository aggregates as this task's proof.
Integrated change review owns final capability/selection closure, including
existing Rust OTLP regressions and the broader affected SDK/Bifrost journeys.
Because `mise.toml` shared selection changes are in scope, integrated review must
apply AGENTS.md's shared-infrastructure verification rule rather than assume
focused selectors alone prove lane selection.

### Non-TDD documentation, generation, and selection proof

These obligations need no manufactured RED:

- Review updated Observe/Connect/Understand content against `docs/CONTRIBUTING.md`
  and `docs/PRODUCT.md`: correct frontmatter, no body H1, existing language-tab
  components, valid current links, preserved draft flags, no invented claims.
- Follow each framework recipe from prerequisites through query/readback using
  the corresponding real journey as executable evidence. Lead with ambient
  credentials, reuse http_auth/httpAuth for clients routed to Wyrd, and explain
  generic direct-provider telemetry separately. Use actual wyrd-ai extras,
  current runtime-compatible hooks and version-qualified content controls.
- Document managed/application-owned lifecycle, both approved failure codes,
  OpenAI add-versus-replace semantics, sampling limits, and residual native
  content. No provider secrets in examples, query output, or logs.
- Keep `architecture/wyrd-design.md` provider/auth/lifecycle contracts consistent.
  The small exporter-helper authority reconciliation is already in this packet;
  complete any lasting lifecycle documentation with the implementation.
- Run `mise run docs:generate`, inspect generated diffs, then
  `mise run docs:check`. Its current generated-drift check requires generated
  content to match the Git index; staging/commit remains caller-owned. If that
  blocks the aggregate, record the exact drift and component results without
  weakening the check or claiming aggregate PASS.
- Inspect changed lane commands and dependency installation: every required
  framework selector is collected by its owner and capability lane; Python's
  >=3.11 group and TS builds are supplied. Do not count MCP-only tests as proof.
- End with `git diff --check`, tracked/untracked diff audit, and an evidence map
  for every REQ/INV/AC. Report executed commands, features, collection counts,
  expected RED failures, GREEN results, static proof, and blockers honestly.

## Material Stop Conditions

Stop affected work for renewed authority if correctness needs new ingest/query
contracts, storage/migrations, server permission changes, a new credential or
retry subsystem, a different global-provider shutdown contract, another public
API/error beyond the approved surfaces, framework discovery/patching, or an
additional production dependency/Cargo feature. Do not silently widen scope.

Missing local services, current equivalent private APIs, test fixtures,
collection/dependency configuration, codegen, and small touched-owner defects
are ordinary implementation work. If a locked framework cannot produce the
required telemetry through its supported API, report source evidence and the
exact compatibility decision needed; do not fake spans or skip the framework.

## Authority Links

- [Approved spec r2](../spec.md)
- [AGENTS.md](../../../../AGENTS.md)
- [Agent rules](../../../../architecture/agent-rules.md)
- [Wyrd design](../../../../architecture/wyrd-design.md)
- [Doctrine](../../../../architecture/wyrd-doctrine.mdx)
- [Telemetry observations](../../../../architecture/references/domain/telemetry-observations.md)
- [Spec-driven development](../../../../architecture/references/languages/spec-driven-development.md)
- [Implementation execution](../../../../architecture/references/languages/implementation-execution.md)
- [Testing workflows](../../../../architecture/references/languages/testing-workflows.md)
- [Python API/stubs](../../../../architecture/references/languages/python-api-and-stubs.md)
- [TypeScript guide](../../../../architecture/references/languages/typescript-guide.md)
- [Ambient-auth completion and owners](../../../completed/2026/ambient-auth.md)
- [Docsite conventions](../../../../docs/CONTRIBUTING.md)
- [Docsite product standards](../../../../docs/PRODUCT.md)

## Implementation Evidence

Status: `IMPLEMENTED`. Commits `a0b932ef6`, `67805c933`, `3e6179915`,
`7d2ecb157`, `f7cb5d352`, and `aeaa22cbc` on top of packet `f7f0912d4`.

### Acceptance map

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-010; AC-007 — bridge fidelity, safe callback lifecycle, optional imports | Python `python/wyrd/otel.py::WyrdAgentsTraceProcessor` (lock-guarded, end-time attributes, start-time Run scope, payload-free error status, release on shutdown, late callbacks ignored); TS `wyrd/src/index.ts::WyrdAgentsTraceProcessor` (same) | U1 4 passed, U4 1, T1 1, T4 1; J1 3 passed, K1 1 passed | PASS |
| REQ-011; AC-001 — Pydantic AI, ADK, OpenAI Agents (Python) and OpenAI Agents (TS) export and read back with no manual spans | `tests/integration/otel/frameworks/app.py` runs stock framework agents against a local scripted upstream, one subprocess per app; TS `tests/integration/agents-trace-processor.test.ts` | J1 `test_managed_framework_telemetry` 3 passed (one per framework); J2 3 passed; K1/K2 1 passed each | PASS |
| REQ-012; AC-008 — idempotent setup, provider preservation, owned-provider correlation | Python `install_run_correlation` retry-safe; TS public `runCorrelationProcessor()` from `wyrd-ai/otel` | U1, U2 1 passed, T1, T2 1 passed; J2 `test_owned_provider_framework_telemetry` 3 passed; K2 1 passed | PASS |
| REQ-013; AC-009 — managed flush before native drain, drain on failure, two catalog errors | `crates/wyrd-spec/src/error.rs` (`WYRD_SDK_504_TELEMETRY_FLUSH_TIMEOUT`, `WYRD_SDK_503_TELEMETRY_FLUSH_FAILED`); `crates/shared/wyrd-client/src/state.rs::TelemetryFlushFailure` shared by `sdks/wyrd-sdk-python/src/state/mod.rs` and `sdks/wyrd-sdk-ts/native/src/cards.rs::telemetry_flush_error`; TS `WyrdState.flush/shutdown` | U3 `test_managed_telemetry_lifecycle` 7 passed; T3 1 passed; J1/J2/K1/K2 read back after flush; K4 asserts the 503 on a refused export | PASS |
| REQ-014; AC-010/011 — fidelity, scope, privacy, sampling | Start-time Run identity, enclosing-context parenting, `include_content=False` and `ADK_CAPTURE_MESSAGE_CONTENT_IN_SPANS=false` recipes | J3 `test_framework_execution_edges` 3 passed (model failure, tool failure, stream completed after scope exit, nested, concurrent, uncorrelated); J4 3 passed (always_off sampler, SECRET sentinels absent); K3/K4 1 passed each | PASS |
| REQ-015; AC-012 — docsite recipes match journeys | Observe `record-a-run-and-observations.md` sections "Trace an agent framework" and "Keep your own tracer provider", error rows, readback; Connect/Understand cross-links; `wyrd-ai[otel]` install name in quickstart and `otel.py`; `architecture/wyrd-design.md` managed flush contract. Draft flags preserved | `mise run docs:generate` (no drift), `mise run docs:check` exit 0 | PASS |
| REQ-016; AC-003 — exact journeys collected with dependencies, no masked integrations | `mise.toml`: `py:test:mcp-frameworks:inner` and `test:bifrost:journey:python:inner` run `tests/integration/otel/frameworks` under `--group mcp-frameworks`; `test:bifrost:journey:typescript:inner` lists `agents-trace-processor.test.ts`; `pyproject.toml` norecursedirs keeps the default lane free of frameworks | Default `tests/integration` collects 0 framework tests; the group dirs collect 19; every J/K selector reports a positive pass count | PASS |
| REQ-017; AC-013 — ambient clients, stock auth transport, fresh per-export tokens | Exporters call the shared client's `access_token` on every export; journeys set only `WYRD_API_KEY` and an isolated `WYRD_CONFIG_HOME`; the gateway path uses the existing `http_auth`/`httpAuth` | U5 1 passed, T5 1 passed; J5 1 passed (the upstream sees only `Bearer sk-local-upstream` and no `x-wyrd` headers; a run with no credential fails with the actionable `WyrdError`); K5 1 passed (`WYRD_CLIENT_401_NO_CREDENTIALS`); R1 2 passed, R2 1 passed | PASS |
| INV-001 — tenant and auth boundaries | No server change; relies on the server's per-span Card-scope rejection | J4/K4: a viewer key persists nothing; an other-tenant credential cannot attribute spans to this tenant's Card, and this tenant reads nothing | PASS |
| INV-002 — one managed provider, no global replacement | Collision refusal `WYRD_SDK_409_TELEMETRY_PROVIDER_EXISTS` retained | U1, T1 | PASS |
| INV-003 — payload-free bridge and errors | The bridge records names, models, usage and status only; the flush error `cause` is a type name | U4/T4 sentinel cases; J4 sentinel absence | PASS |
| INV-004 — runtime boundaries | No framework registry, patching, token cache, transport, migration, or production dependency; native glue lives only in the SDK runtimes | `mise run check:deps` exit 0; diff audit below | PASS |

### Commands

Static and owner lanes, all exit 0:
- `mise run fmt`, `lints`
- `py:format:check`, `py:lints`, `py:typecheck`, `py:test:unit` (610 passed at `aeaa22cbc`)
- `ts:format:check`, `ts:lints`, `ts:typecheck`, `ts:test:unit` (71 passed), `ts:pack:check`
- `ts:napi:check`, run because the native export `telemetry_flush_error` was added
- `codegen:check`, `check:deps`
- `docs:generate`, `docs:check`
- `git diff --check`

Focused selectors U1–U5, T1–T5, R1, R2 and K1–K5 ran exactly as listed above.
J1–J5 ran with the listed wrapper and `--group mcp-frameworks`, at the actual
path `tests/integration/otel/frameworks/test_framework_telemetry.py::<name>`.

### Deviations and material limits

- **Selector path.** J1–J5 live in `tests/integration/otel/frameworks/` rather
  than `tests/integration/otel/`. That directory is excluded from the default
  integration lane, so the lane never imports the optional frameworks. Each app
  runs in its own subprocess for two reasons: OTEL allows one global provider
  per process, and ADK binds its tracer at import.
- **RED evidence.** Unit scenarios U1–U5/T1–T5 were written alongside the fix,
  and no separate failing run was captured. The journeys supplied real RED for
  the TS findings: the missing public `runCorrelationProcessor`, and a refused
  export surfacing at shutdown.
- **TS Chat Completions usage.** `OpenAIChatCompletionsModel` in
  `@openai/agents-openai` 0.20 never sets usage on generation spans. K1–K3
  therefore assert the model but not token counts. `OpenAIResponsesModel` does
  report usage. The recipe documents this.
- **TS enclosing span.** TS `run.scope()` does not stamp the already-active
  enclosing span with `run_id`; Python's `Run.__enter__` does. The K journeys
  assert run and card only on spans started inside the scope.
- **Python denied export.** The Python OTEL SDK's `force_flush` returns true
  after a refused export and only logs the refusal, so Python flush does not
  raise for a 403. TS raises `WYRD_SDK_503_TELEMETRY_FLUSH_FAILED`. The error
  notes in the docs say so.
- **Removed proof.** `tests/integration/otel/test_agents_trace_processor.py`
  used a custom model with manual generation spans. It was deleted and is
  superseded by J1–J5.

### Non-goals and diff audit

None of the excluded scope was added:
- server behavior, wire payloads, tables, or migrations;
- framework discovery, monkey-patching, a framework registry, or a Pydantic/ADK adapter;
- a transport, export queue, or token cache;
- a Rust framework integration or a production dependency;
- any revived ambient-auth helper.

Draft docs remain `draft: true`. Every file changed since `f7f0912d4` is in the
expected write set, and the working tree has no untracked files.
