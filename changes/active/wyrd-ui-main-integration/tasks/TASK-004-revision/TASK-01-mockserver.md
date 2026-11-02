---
id: TASK-004-REV-01
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-main-integration
spec_revision: 18
requirements: [REQ-037, AC-020, INV-001, INV-002, INV-004, INV-005]
depends_on: []
parent_task: TASK-004
remediates: []
---

# TASK-01 — Optional MockServer for local UI development

## Outcome and Value

A developer can start a local OpenAI-compatible fixture service and use Wyrd's
existing gateway configuration to drive application inference and server-side
model judges without external inference. This is development tooling, not a
new Wyrd provider capability. Task 02 consumes it to populate the live UI.

Before this task, dev startup has no reproducible fixture service. Afterwards,
`mise run dev:mockserver` starts the configured service and reports fixture
readiness; a normal Wyrd OpenAI deployment can complete a tool exchange and a
structured judge response against it.

## What Changes and What Must Stay True

- Use official MockServer 9.2.0, pinned to the multi-platform image index
  `ghcr.io/mock-server/mockserver:9.2.0@sha256:016f7d16bc88d2e1aa52d5a177f1aafee758cf70bea5e0624b0ab2c32f394df1`.
  Its index was checked for Linux amd64/arm64; macOS arm64 uses the existing
  local container prerequisite. Container execution remains implementation proof.
- Extend existing development process/container owners. Provide the foreground
  `dev:mockserver` entrypoint and reusable initialization/readiness/shutdown
  behavior for `dev:full`; do not create a second environment controller.
- Bind the fixture/admin listener to loopback or the owned private container
  network. Load checked-in expectations at startup, fail initialization on
  invalid configuration, and await `GET /mockserver/ready`, not merely a TCP
  connection or the container's serving-status check. Bound readiness waits.
- Use `[gateway.endpoints].openai` in the existing development server profile.
  The base ends in `/v1`; actual inference requests reach `/v1/chat/completions`.
  Configure ordinary nonempty dummy OpenAI credentials through existing owners.
  The override affects all OpenAI calls in this dev server, including queued
  judges, because they use the same dispatcher. There is no inherited scope.
- Keep normal authentication, authorization, audit, routing, admission,
  credential resolution, limits, deadlines, capture and settlement. Provider
  identity stays `openai`; normal usage/pricing/budgets remain. No special
  `mock` identity, nonbillable rule, table field or ledger policy is introduced.
- The service is optional outside this local workflow. Existing real-provider
  configuration and `dev:both` remain available without MockServer. `dev:full`
  uses the fixture service by default. This adds no production configuration
  mode and does not relax production endpoint restrictions.

### Minimum expectations

Use repeatable static expectations and ordinary OpenAI JSON responses. Do not
use shared consumable response sequences, conversation counters, response
templating, proxy fallthrough or real-provider fallback. Match only the
configured path/model and the canonical request's relevant message fields.
Concurrent requests must not consume each other's expected response.

The canonical source is `bohmian-ai/wyrd-example-py` at
`03428e1a4a35c8f23f71a95cae08e956ca317c9b`:

| Request | Scripted response |
|---|---|
| `gpt-4o`, initial shipping/refund question for A-1001, A-1002 or A-1003 | A normal `lookup_order` tool call with that literal order ID. |
| `gpt-4o`, tool-result turn for a shipping question | `Your order ships tomorrow.` |
| `gpt-4o`, tool-result turn for a refund question | `You have a guaranteed refund.` |
| `gpt-6-luna`, canonical policy-judge structured response | A valid verdict with outcome `passed`, reason `scripted development verdict`, and citation slot `order`, locator `json_pointer: /price`. |
| Unmatched model/path/body | A visible HTTP failure, without proxying to an upstream. |

The latest user message distinguishes shipping/refund on the tool-result turn;
the final tool message alone does not. Application-specific strings live in
expectations, never gateway code. Use literal response bodies with ordinary
message/tool-call IDs, finish reasons and usage. The judge is deliberately a
scripted verdict, not a policy-quality evaluation. Direct refund verification
still fails and the real verifier still persists/cites its outcome.

No runtime Python scope, request header, SDK feature, ProviderAdapter variant,
mock tenant/key/deployment, alternate Wyrd inference route, verification
snapshot, queue propagation, migration or schema-evolution work belongs here.
Do not restore or take ownership of the caller's rejected-code cleanup.

## Approach

1. Revalidate the current development lifecycle and existing OpenAI override.
2. Add the pinned service and minimal static expectations to that lifecycle.
3. Exercise initialization, repeatable OpenAI exchanges and the ordinary Wyrd
   gateway against it; keep all failures explicit.
4. Document startup/cleanup and hand its usable configuration/readiness to Task 02.

## Ordered Implementation Scenarios

### Scenario 1 — Initialized fixtures through the ordinary gateway

**Behavior.** The service becomes ready only after expectations load. A normal
authenticated Wyrd client receives the tool call, runs its actual tool and
receives the final response; a normal judge-model request receives the valid
structured verdict. Repeated concurrent exchanges return the same case-specific
answers. Gateway capture names `openai` and addresses remain unchanged.

**RED.** Add the planned gateway journey selected by V1. Confirm missing fixture
startup/answers fail for the expected behavior, rather than missing Docker,
credentials, compilation or an unselected test. Static expectation declarations
receive configuration review, not a manufactured unit-test RED.

**GREEN.** Extend existing dev/container setup and feed the ordinary dispatcher
its existing override/credential configuration. Pass V1 without a provider
implementation, new client transport or server workflow.

**REFACTOR.** Reuse the existing gateway journey/process helpers; remove duplicate
startup/configuration while preserving the ordinary pipeline assertions.

### Scenario 2 — Failed setup and unmatched requests remain visible

**Behavior.** Invalid initialization, an occupied listener, readiness timeout
or an unmatched request produces an explicit failure; no real inference occurs.
Interruption cleans up owned service resources. Never stop an unrelated
container/process or claim readiness after initialization failed.

**RED.** Add the planned negative journey selected by V2 and show the missing
failure/readiness/cleanup behavior. Retain the initial successful exchange as
regression proof; do not accept a failed setup as evidence of an unmatched call.

**GREEN.** Complete bounded setup/error/cleanup in existing lifecycle owners.
Use MockServer's existing initializer and readiness behavior, not a readiness
service or background repair controller. Rerun V1 and V2.

**REFACTOR.** Consolidate cleanup with current process/container ownership;
leave production gateway behavior and real-provider configuration unchanged.

## Acceptance Criteria

| Obligation | Observation and proof |
|---|---|
| REQ-037; AC-020 | V1 proves actual OpenAI requests through the real gateway to initialized MockServer, including tool/final/judge responses and concurrent repeatability. A custom Rust provider or SDK switch violates scope. |
| REQ-037; INV-002/004 | V2 proves explicit initialization/unmatched failures and owned cleanup; no upstream fallback or authorization bypass. Production still rejects its existing prohibited endpoint overrides. |
| INV-001/005 | Image identity and native-platform manifest evidence are recorded; run the fixture service on the implementation host and retain supported-platform evidence without OS-specific code gates. |
| Existing contracts | Diff/static review confirms no retained schema, generated contract, provider enum, pricing rule, queue or SDK changes. Existing gateway regressions affected by the configuration still pass. |

## Implementation Reference

Paths guide discovery; they are not a file/type allowlist.

- `mise.toml::dev:full/dev:both`, `scripts/postgres/dev-server.sh`, and
  `with-test-postgres.sh` own existing dev processes/cleanup. Missing behavior:
  fixture-service initialization and a reusable ready endpoint. Extend them,
  with a small owning script/configuration only if the existing task body
  cannot appropriately hold the behavior; do not duplicate orchestration.
- `wyrd-server/src/config.rs::GatewayConfig/BuiltinEndpoints/validate` already
  supports development OpenAI endpoint overrides and rejects them in production.
  `wyrd-gateway/src/adapter/http.rs::HttpProviderDispatch::client` consumes them.
  `skald-providers/src/clients/openai.rs` appends `/chat/completions` directly;
  preserve `/v1` in the fixture base even though an old comment suggests otherwise.
- `verification/hosted/blocks.rs::HostedBlocks` and
  `components/gateway/caller.rs::ServerWyrdGatewayCaller` already use the same
  gateway. No new independent-judge selection mechanism is required.
- `wyrd-testing` declares the existing `gateway` integration target; its
  `compatible`, `operations` and `resilience` modules provide real-server
  gateway fixture/client patterns. Extend that target for V1/V2, using the
  existing Postgres and test-server owners; do not ship test-server code in dev.
- Canonical `src/desk.py`, `service/policy-judge.yaml` and `test/conftest.py`
  establish message/response needs. Its old separate-provider test fixture is
  historical input, not the acceptance workflow.
- Official sources: [initialization/readiness](https://www.mock-server.com/mock_server/initializing_expectations.html),
  [expectations](https://www.mock-server.com/mock_server/creating_expectations.html),
  [OpenAI actions](https://www.mock-server.com/mock_server/llm_response_mocking.html),
  [official images](https://www.mock-server.com/where/docker.html).

## Verification and Evidence

V1/V2 are **planned tests** in the confirmed `wyrd-testing --test gateway`
target, under a new `dev_mockserver` module. Add them, confirm actual nextest
selectors before RED, and fail if the selector runs no tests. These commands
do not claim the tests exist or pass yet. Docker and repository Postgres are
required; reuse the gateway lane's journey profile and migration owner.

**V1 — configured fixture exchange**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E "test(=dev_mockserver::configured_openai_fixture_serves_tool_and_judge)"'
```

**V2 — fixture failure and cleanup**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E "test(=dev_mockserver::unmatched_and_broken_initialization_fail_explicitly)"'
```

**V3 — manual development service smoke, after implementation**

```bash
mise run dev:mockserver
```

Record image digest/platform, initialized expectations, actual request path,
tool/final/judge responses, unchanged target/provider fields, request failures
and cleanup. Configuration-only declarations use static review plus these
runtime smoke observations. Run repository format/lints for changed Rust test
code (`mise run fmt`, `mise run lints`) and `git diff --check`; add only owner
checks actually required by the implementation write set. No broad gateway
aggregate, live paid inference, or full change gate belongs to this task.

## Material Stop Conditions

Stop for any need to change a production provider API, endpoint policy,
authentication/tenancy, retained schema, ledger policy, queue semantics or
supported-platform contract. A new fixture server framework or Rust mock
provider is outside scope. Ordinary fixture matcher corrections and local
container setup are implementation-owned; preserve fail-closed behavior.

## Authority Links

- [Approved scope, revision 18](../../spec.md), [task order](../../plan.md).
- [Repository rules](../../../../../AGENTS.md), [execution](../../../../implementation-execution.md),
  [testing](../../../../../TESTING.md), [workflow](../../../../spec-driven-development.md).
- [Wyrd design](../../../../../architecture/wyrd-design.md),
  [security](../../../../../architecture/wyrd-security-posture.md).

## Execution Evidence

Planning only. No service, gateway journey or implementation PASS is recorded.
