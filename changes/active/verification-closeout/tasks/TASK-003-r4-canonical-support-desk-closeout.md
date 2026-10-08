---
id: TASK-003
kind: implementation
status: proposed
spec: SPEC-verification-closeout
spec_revision: 1
requirements: [REQ-006, REQ-007, REQ-008, REQ-009, REQ-010, INV-001, AC-004, AC-005, AC-006]
depends_on: [TASK-001, TASK-002]
provenance: TASK-017-R4
---

# R4: Canonical support-desk closeout

## Outcome and Value

One support-desk example proves that an authenticated user can deploy and run
an Agent-backed Service, correlate every signal of a request, execute continuous
and realtime verification, and explain the result through public surfaces in
Rust, Python, and TypeScript.

## Owners, Scope, Consumers, and Prohibited Changes

- Extend existing Card registration, verification, gateway, telemetry, client,
  and example owners. Do not introduce replacement orchestration.
- Realtime results use Task 2's `ScribeOutbox`; this task does not create
  another result sink or publisher.
- Principal authority, Card attribution, saved-login behavior, and stock-client
  authentication come from Task 1 and are not reimplemented.
- Preserve server-owned durable behavior and language-parity through the shared
  client and generated contracts.
- Exclude Agent tools, structured output, Workflow correlation, new MCP tools,
  table evolution/drop controls, telemetry metrics/log setup, and UI work.

## Reuse Map

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Declared tables | Card registration and Bifrost table registration | card registration and Bifrost integration tests | A Service cannot declare and ensure a dataset | Extend registration to validate and ensure through the existing catalog owner | None |
| Binding activation | verification binding projector, scheduler, observation enqueue | card and Eval integration tests | Activation is required and unbound writers enqueue nothing | Permit no activation and reuse subject binding selection for unbound writers | None |
| Result correlation | result projection, Eval report, direct verification | result and route tests | Results carry verifier-run identity or are absent | Carry the application Run and send canonical batches through Task 2 | None |
| Gateway correlation and judging | gateway ingress/capture and judge invoker | gateway and verification journeys | Calls lack Run/Card correlation; judge bypasses gateway | Extend existing gateway request facts and use the existing gateway caller | A bounded principal cache is retained only if queued judging cannot recover caller authority otherwise |
| Agent invocation | existing Agent runtime and gateway adapter | gateway inference and workflow tests | No public one-Agent Run invocation | Expose the existing single-Agent gateway path through Run | None |
| Telemetry scope | existing SDK telemetry/export helpers and Run types | OTLP and observation journeys | No consistent setup/scope in all SDKs | Compose existing exporter and context mechanisms | Language runtime adapters only |

## Approach

1. Add declared Service tables and optional verification activation using the
   current Card, registry, catalog, scheduler, and observation owners.
2. Carry application-Run identity through continuous and direct result
   projection; record direct results through Task 2 without creating a durable
   verification run.
3. Add gateway Run/Card correlation, route judge and single-Agent invocation
   through the existing gateway, and preserve the initiating principal.
4. Complete the shared client and three SDK telemetry/Run-scope surfaces.
5. Build one checked-in support-desk example and matching three-language
   journeys that prove the complete public workflow and its key refusals.

## Required Contract Detail

The public types and surfaces are fixed by the spec:

- `ServiceTable { name, schema }`; `ServiceSpec.tables` defaults empty and is
  omitted when empty.
- `VerificationBinding.runs_on` becomes optional and is omitted when absent.
- `ExecuteVerificationRequest.run_id` is optional and omitted when absent.
- Gateway correlation uses `wyrd-run-id` (non-empty, at most 128 bytes) and
  `wyrd-card-ref` (the CardRef text grammar); both or neither.
- The gateway-call subject carries typed Run and Card identities while existing
  Workflow steps continue to pass none.
- Python, TypeScript, and Rust expose idiomatic `Run.invoke`, telemetry setup,
  and Run scope; `observe.verify` keeps its public shape while sending Run ID.

The example layout is retained:

```text
examples/support-desk/
  README.md
  service/
    support-desk.yaml
    support-agent.yaml
    support-prompt.yaml
    answer-quality.yaml
    judge-prompt.yaml
    no-refund-promise.yaml
  python/support_desk.py
  typescript/support-desk.ts
  rust/support_desk.rs
  rust/main.rs
```

Each implementation exports `deploy`, `serve`, `wait_for_verdicts`, and
`explain` (idiomatic casing only), plus a runnable main. `deploy` ensures the
four-field `vala.datasets.tickets` table, metadata capture, and model
deployment. `serve` handles 100 requests inside correlated Agent Runs. Ten
questions contain “refund” and receive a refund promise. `wait_for_verdicts`
waits through the public verification surface. `explain` uses only MCP
`bifrost.query` with a fresh token.

Required journey assertions are:

| Proof | Expected result |
|---|---|
| declared table | `tickets` exists with the declared four fields before Bifrost starts |
| Agent invocation | receiver answer returned; captured call has the application Run and Agent Card |
| span correlation | `support-desk.request` carries the Run; Eval observation shares its trace |
| joined explanation | observation, service record, span, gateway call, continuous verdict, and realtime verdict join for passing and failing requests |
| verdict totals | 100 `answer-quality` passes; 90 `no-refund-promise` passes; 10 failures |
| schema refusal | conflicting tickets schema returns `WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH` before writes |
| authorization refusal | `viewer` Agent invocation returns `WYRD_PERMISSION_403_DENIED_RBAC` before upstream IO |

The stable refusal contract also includes unsupported declared schema,
insufficient table permission, unavailable Scribe role, half/malformed gateway
correlation, unresolved/out-of-scope Card, invalid Agent invocation, denied
judge gateway access, missing judge model, and foreign telemetry provider.

## Ordered Implementation Scenarios

### Scenario 1 — Registration and activation support the example

**Behavior.** Service registration validates and ensures declared tables before
activation. Bindings without automatic activation are realtime-only, while an
unbound writer's observation activates matching subject bindings.

**RED.** Extend the existing Card-registration and Eval integration targets
with declared-table success/conflict, realtime-only binding, and unbound-writer
activation cases. Current contracts fail these cases.

**GREEN.** Extend the existing Card specs, registration/catalog workflow,
binding projection, scheduler, and observation enqueue selection.

**REFACTOR.** Share current schema validation and table-ensure behavior; remove
only branches made unreachable by optional activation.

### Scenario 2 — Every request and verdict is correlated and recorded

**Behavior.** Gateway calls accept authorized Run/Card correlation. Continuous
and realtime result rows carry the application Run, Verifier Card, subject,
and stable result identity. Direct execution returns independently and records
through `ScribeOutbox` without creating a verification run or dispatch.

**RED.** Extend existing gateway-capture, result-projection, and direct-route
tests with correlated continuous and direct cases plus malformed, partial, and
unauthorized correlation refusals.

**GREEN.** Carry the existing typed identities through gateway facts and result
projection, and use Task 2's shared outbox for completed direct judgments.

**REFACTOR.** Remove obsolete documentation and assertions that direct
execution writes no result; add no second result-writing path.

### Scenario 3 — Agents, judges, and telemetry use the public runtime

**Behavior.** A Run invokes its Agent through the gateway; LLM judges use the
same gateway as the initiating principal; each SDK installs telemetry and
scopes spans to the Run and Card. Existing gateway authorization, accounting,
capture, and audit apply.

**RED.** Extend the existing verification, gateway, OTLP, and SDK observation
targets with gateway-owned judge calls, one-Agent invocation, Run-scoped span
attributes, token refresh, shutdown flush, and foreign-provider refusal.

**GREEN.** Expose and compose the existing gateway adapter, issuer, telemetry
exporter, and runtime context mechanisms through the shared client and SDKs.

**REFACTOR.** Delete the separate judge-provider plumbing once all judge calls
use the gateway; keep language-specific code limited to runtime integration.

### Scenario 4 — One support-desk journey proves the closeout

**Behavior.** Rust, Python, and TypeScript run the same checked-in support-desk
story against a real server. The journey deploys the declared table and Agent,
invokes requests, records telemetry and observations, produces continuous and
realtime verdicts, and queries joined evidence through MCP. It proves one
passing request, one policy failure, schema-conflict refusal, and
under-privileged invocation refusal.

**RED.** Add the same support-desk journey name and fixture story to the three
existing SDK integration targets. The story fails until Scenarios 1–3 and Tasks
1–2 are integrated.

**GREEN.** Add the smallest example, fixtures, public-SDK journey code, and mise
entrypoints required to run that story identically in all three languages.

**REFACTOR.** Consolidate only shared checked-in Card inputs and existing test
harness setup; keep each language example idiomatic and readable.

The three journey files are fixed at:

- `sdks/wyrd-sdk-python/tests/integration/test_support_desk.py`;
- `sdks/wyrd-sdk-ts/wyrd/tests/integration/support-desk.test.ts`;
- `sdks/wyrd-sdk-rust/tests/integration/support_desk.rs`.

Their focused commands are:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_support_desk.py"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise run ts:build && mise run ts:build:testing && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/support-desk.test.ts"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E 'test(/^support_desk::/)'"
```

## Acceptance Criteria

- AC-004, AC-005, and AC-006 pass.
- No task-local result sink, gateway transport, auth model, or telemetry
  provider duplicates Task 1 or Task 2 ownership.
- Direct execution stores exactly one result through `ScribeOutbox` and creates
  no durable verification run, observation, or Operator dispatch.
- The support-desk journeys use the same story and observable outcomes in all
  three SDKs and no test-only publication hook.
- Public contracts regenerate cleanly and documentation matches the proved
  workflow.

## Expected Write Set and Consumer Closure

- Contracts/generated surfaces: Service and Verifier specs, verification and
  correlation requests, generated schemas, OpenAPI, Python stubs, and
  TypeScript declarations.
- Server: Card registration, Bifrost table ensure, verification binding/
  observation/result/direct owners, gateway ingress/invocation/capture, judge
  integration, and server state only where caller authority is retained.
- Shared client and SDKs: Run invocation, verify correlation, telemetry setup
  and scope, plus idiomatic Rust/Python/TypeScript projections.
- Evidence: existing focused server tests, `fixtures/cards/support_desk`, one
  support-desk journey per SDK, the example and its concise documentation, and
  existing mise example/journey membership.

## Verification and Evidence

- `mise run test:cards:integration`
- `mise run test:principals:integration`
- `mise run test:gateway:journey`
- `mise run test:bifrost:journey:observe`
- `mise run test:bifrost:journey:otlp`
- `mise run test:bifrost:journey:mcp`
- `mise run check:examples`
- `mise run codegen:check`
- `mise run check:deps`
- `mise run check:tenant-isolation`
- `mise run fmt`
- `mise run lints`
- `mise run py:lints`
- `mise run py:typecheck`
- `mise run docs:check`

The integrated candidate runs the three SDK verification lanes and the broad
`test:bifrost` aggregate once during change review rather than repeating them
inside this task.

## Material Stop Conditions

- Registration cannot ensure a table in supported deployment topologies using
  an existing server-to-Bifrost boundary.
- A queued judge cannot recover the initiating principal without introducing a
  new durable credential or weakening authorization.
- Application-Run identity cannot reach result projection without changing the
  documented result meaning beyond this specification.
- A first-class language runtime cannot propagate Run scope through its native
  async context or a required stock client lacks the needed extension hook.

## Authority Links

- `../spec.md` revision 1: REQ-006..010, AC-004..006
- `AGENTS.md`; `architecture/agent-rules.md`;
  `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`;
  `architecture/bifrost-design.md`;
  `TESTING.md` (definitive Wyrd test-writing and lane-selection guide)
