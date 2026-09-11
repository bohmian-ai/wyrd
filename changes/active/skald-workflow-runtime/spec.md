---
id: SPEC-skald-workflow-runtime
revision: 1
status: draft
---

# Skald workflow runtime

## Objective and user value

Finish the existing Skald workflow capability so one declarative Workflow Card
can be authored in YAML, executed locally before registration, registered with
`wyrd apply`, fetched and executed locally, or invoked on `wyrd-server`.

The result gives users one workflow definition and one Skald execution model
for single-agent and multi-agent DAGs. A user can test a code-review workflow
locally, register the same card and its referenced Agents and Prompts, then
choose local execution or tenant-bound server execution without rewriting the
workflow.

## Research baseline

The change completes existing behavior rather than introducing another
workflow engine:

- `wyrd-spec::card::workflow::WorkflowSpec` already declares workflow inputs,
  steps, dependencies, step inputs, conditions, timeouts, retries, outputs,
  governance, and observation hooks.
- `skald-workflow::Workflow` already loads Workflow Card YAML and executes
  locally through `DagExecutor`.
- `DagExecutor` already validates and executes dependency-ready steps in
  concurrent topological stages, invokes Skald Agents through a supplied
  `ProviderRegistry`, retries calls, emits observer events, and captures task
  responses.
- Skald Prompt binding already supports `${name}` and `{{name}}` parameter
  substitution.
- `wyrd apply` and the shared loader already resolve Card-level `path`,
  `inline`, and `ref` forms and register Cards in dependency order.

The missing behavior is contract-to-runtime completeness: current lowering
discards declared step inputs, conditions, timeouts, retry backoff, workflow
outputs, and non-Agent actions; referenced Agents are not hydrated by the
ordinary YAML execution path; concurrent structured outputs share a
collision-prone flat map; final output is inferred from the last topological
step; Workflow DAG validation is not enforced during loading or registration;
and no CLI or server invocation surface exists.

## Definitions

- **Workflow definition:** the `spec` of a `kind: Workflow` Card.
- **Local file execution:** execution from a local Workflow Card YAML document
  or bundle without requiring registration.
- **Local registered execution:** fetching a registered Workflow Card and its
  locked dependencies, then executing them in the caller's process.
- **Server execution:** asking `wyrd-server` to resolve and execute a registered
  Workflow Card under the authenticated tenant and server runtime.
- **Step output:** the normalized text and/or structured JSON produced by one
  Agent step, retained under that step's stable ID.
- **Workflow output:** a named JSON value selected or composed by the
  Workflow's declared `outputs` after all required steps complete.
- **Execution environment:** the local process or `wyrd-server`; it supplies
  providers, tools, credentials, observers, deadlines, and resource bounds but
  does not change workflow semantics.

## User ergonomics

### Define a multi-agent workflow in YAML

A workflow bundle is ordinary Wyrd YAML. Agents and reusable Prompts remain
separate Cards, and the Workflow connects the Agents into a DAG:

```text
code-review/
├── workflow.yaml
├── agents/
│   ├── security.yaml
│   ├── correctness.yaml
│   └── final-reviewer.yaml
└── input.json
```

`workflow.yaml` declares two independent reviewers followed by one final
reviewer. The final step names both dependencies and explicitly binds their
namespaced outputs:

```yaml
apiVersion: wyrd/v1
kind: Workflow
metadata:
  space: engineering
  name: code-review
  version: "1.0.0"
spec:
  description: Review a change concurrently, then validate and consolidate findings.
  inputs:
    code:
      type: str
      value: ""

  steps:
    - id: security
      action:
        type: agent
        target: ./agents/security.yaml
      inputs:
        code:
          type: str
          value: "{{input.code}}"
      timeout_seconds: 60
      retry:
        max_retries: 1

    - id: correctness
      action:
        type: agent
        target: ./agents/correctness.yaml
      inputs:
        code:
          type: str
          value: "{{input.code}}"
      timeout_seconds: 60
      retry:
        max_retries: 1

    - id: final_review
      action:
        type: agent
        target: ./agents/final-reviewer.yaml
      depends_on: [security, correctness]
      inputs:
        code:
          type: str
          value: "{{input.code}}"
        security_review:
          type: str
          value: "{{steps.security.output.text}}"
        correctness_review:
          type: str
          value: "{{steps.correctness.output.text}}"
      timeout_seconds: 60
      retry:
        max_retries: 1

  outputs:
    review: "{{steps.final_review.output.text}}"
```

Each Agent owns its instruction Prompt. `agents/security.yaml` is representative:

```yaml
apiVersion: wyrd/v1
kind: Agent
metadata:
  space: engineering
  name: security-reviewer
  version: "1.0.0"
spec:
  prompt:
    provider: openai
    model: gpt-5-5
    system: |
      You are a security reviewer. Report only exploitable security findings.
      For every finding, identify the affected code and a concrete failure path.
    messages:
      - |
        Review this change:
        {{code}}
    variables: [code]
    response_type: text
  tool_names: []
  run_config:
    max_iterations: 1
    timeout_ms: 60000
```

`agents/correctness.yaml` changes the instruction while preserving the same
input contract:

```yaml
apiVersion: wyrd/v1
kind: Agent
metadata:
  space: engineering
  name: correctness-reviewer
  version: "1.0.0"
spec:
  prompt:
    provider: openai
    model: gpt-5-5
    system: |
      You are a correctness reviewer. Find reachable bugs, data loss, races,
      and contract violations. Ignore style-only concerns.
    messages:
      - |
        Review this change:
        {{code}}
    variables: [code]
    response_type: text
  tool_names: []
  run_config:
    max_iterations: 1
    timeout_ms: 60000
```

`agents/final-reviewer.yaml` receives both prior outputs through ordinary Prompt
variables and validates them against the original input:

```yaml
apiVersion: wyrd/v1
kind: Agent
metadata:
  space: engineering
  name: final-reviewer
  version: "1.0.0"
spec:
  prompt:
    provider: openai
    model: gpt-5-5
    system: |
      You are the final reviewer. Validate each proposed finding against the
      supplied code, remove duplicates and unsupported claims, and return one
      prioritized review.
    messages:
      - |
        Code:
        {{code}}

        Security review:
        {{security_review}}

        Correctness review:
        {{correctness_review}}
    variables: [code, security_review, correctness_review]
    response_type: text
  tool_names: []
  run_config:
    max_iterations: 1
    timeout_ms: 60000
```

`input.json` is the invocation payload:

```json
{
  "code": "diff --git a/src/auth.rs b/src/auth.rs\n..."
}
```

The same bundle supports all requested journeys:

```bash
# Test the unregistered bundle in the local process.
wyrd workflow run \
  --file ./code-review/workflow.yaml \
  --input-file ./code-review/input.json \
  --execution local

# Register the Workflow, Agents, and Prompts as one resolved Card graph.
wyrd apply ./code-review

# Fetch the registered graph, but execute it in the local process.
wyrd workflow run \
  --space engineering \
  --name code-review \
  --version 1.0.0 \
  --input-file ./code-review/input.json \
  --execution local

# Ask wyrd-server to resolve and execute that same registered graph.
wyrd workflow run \
  --space engineering \
  --name code-review \
  --version 1.0.0 \
  --input-file ./code-review/input.json \
  --execution server
```

### Switch between direct providers and the gateway

Provider routing is execution configuration, not Workflow YAML. The Card above
does not change when the user switches routing.

For local Rust execution, the current Skald default registry uses the provider
configuration already available to the process:

```rust
let workflow = Workflow::load(
    "./code-review/workflow.yaml",
    &tools,
    prompt_resolver,
)?;
let run = workflow.run(input).await?;
```

To route the same workflow through an OpenAI-compatible gateway, the caller
supplies a gateway-backed Skald provider registry:

```rust
let providers = ProviderRegistry::for_provider(
    &ProviderName::OpenAi,
    "https://gateway.example.com/v1",
    Some(gateway_token),
)?;

let run = workflow.run_with(&providers, input).await?;
```

The CLI uses the same runtime selection through its provider environment. A
direct call uses the provider's normal endpoint and credentials:

```bash
OPENAI_API_KEY="$OPENAI_API_KEY" \
  wyrd workflow run --file ./code-review/workflow.yaml \
  --input-file ./code-review/input.json --execution local
```

An OpenAI-compatible gateway changes only the endpoint and credential supplied
to the provider registry:

```bash
OPENAI_BASE_URL="https://gateway.example.com/v1" \
OPENAI_API_KEY="$GATEWAY_TOKEN" \
  wyrd workflow run --file ./code-review/workflow.yaml \
  --input-file ./code-review/input.json --execution local
```

For server execution, the operator chooses direct or gateway-backed providers
when constructing the server's Skald provider registry. The client command and
registered Workflow Card remain identical. The future shared gateway therefore
needs a Skald `Provider` implementation or compatible registry entry, not a new
Workflow schema or execution path.

## Required behavior

### One declarative workflow model

- **REQ-001:** A runnable workflow MUST be a normal `apiVersion: wyrd/v1`,
  `kind: Workflow` Card. Local execution MUST NOT introduce a second YAML
  format or require a registration wrapper.
- **REQ-002:** A Workflow MUST support one or more Agent steps identified by a
  stable step ID. An Agent step MUST accept an inline Agent spec, an authored
  `path` to an Agent Card, or a versioned Agent `ref` through the existing
  reference model.
- **REQ-003:** Agent Prompts MUST continue to use the existing Agent-to-Prompt
  reference and Skald Prompt binding mechanics. A Workflow MUST NOT introduce
  a parallel system-prompt or provider-request schema.
- **REQ-004:** Direct Prompt and MCP workflow actions MUST NOT be executable
  step kinds. The unimplemented `WorkflowAction::Prompt` and
  `WorkflowAction::Mcp` contract variants and the untyped, unimplemented step
  `condition` field MUST be removed. A Prompt is invoked through an Agent; MCP
  tools are resolved through the Agent tool registry. Conditional branching is
  not part of the requested basic DAG.

### DAG and context semantics

- **REQ-005:** `depends_on` MUST define an explicit acyclic step graph. Step
  declaration order MUST NOT determine dependency order.
- **REQ-006:** Steps whose dependencies are satisfied MUST be eligible to run
  concurrently. A dependent step MUST observe a stable snapshot of completed
  dependency outputs; it MUST NOT observe partial or racing writes from its
  peers.
- **REQ-007:** Workflow invocation input MUST be a JSON object. Values declared
  in `WorkflowSpec.inputs` are defaults; invocation values with the same names
  override them. A missing value required by a step binding MUST fail before
  that step invokes a provider.
- **REQ-008:** Step inputs MUST bind prompt variables from literals, workflow
  input, or prior step outputs using the existing `${...}` / `{{...}}`
  interpolation style and these stable roots:
  `input.<name>`, `steps.<step_id>.output.text`, and
  `steps.<step_id>.output.structured` (including nested JSON fields).
- **REQ-009:** A step MAY reference only its declared transitive dependencies.
  A reference to another step without a dependency path MUST be rejected as a
  hidden dependency rather than silently changing execution order.
- **REQ-010:** Step outputs MUST remain namespaced by step ID. Concurrent steps
  producing the same structured key MUST NOT overwrite one another.
- **REQ-011:** Dependency outputs MUST enter a downstream Agent only through
  its declared step-input bindings. `depends_on` alone MUST NOT implicitly
  prepend provider-specific message history.
- **REQ-012:** Workflow `outputs` MUST explicitly select or compose the final
  named JSON outputs using the same binding roots. Runtime order or the last
  lexicographic/topological step MUST NOT implicitly select the final result.

### Validation and execution

- **REQ-013:** The shared pure contract validation MUST reject an empty step
  graph, empty or duplicate step IDs, missing dependencies, self-dependencies,
  cycles, duplicate dependencies, invalid bindings, bindings to inaccessible
  steps or fields, and absent workflow outputs.
- **REQ-014:** The same Workflow validation MUST run for local file loading,
  Card registration, registered local loading, and server invocation. A
  workflow rejected in one environment MUST NOT be accepted unchanged in
  another.
- **REQ-015:** The Skald runtime MUST execute the same validated workflow plan
  locally and on the server. Environment adapters MAY differ only in dependency
  resolution, provider/tool configuration, authorization, audit, observation,
  and resource limits.
- **REQ-016:** Per-step timeout and retry declarations MUST be honored in both
  environments. A retry MUST be visible in the step result and observer stream.
  A timed-out attempt MUST not continue running in the background.
- **REQ-017:** Ready-step concurrency MUST be bounded by the execution
  environment. The server MUST enforce an operator-configured ceiling and total
  request deadline regardless of the workflow declaration; local Rust callers
  MUST be able to supply equivalent execution limits.
- **REQ-018:** On a terminal step failure, already-running peers in the same
  stage MAY finish, but no new step may start. The runtime MUST return a failed
  WorkflowRun containing every completed, failed, and unstarted step rather
  than discard partial results in an error-only return.
- **REQ-019:** Caller cancellation MUST stop scheduling new work, cancel active
  provider/tool operations where their interfaces permit it, and return or
  record a distinct cancelled status. Cancellation MUST NOT be reported as a
  provider failure.

### Results and intermediate output capture

- **REQ-020:** Local and server execution MUST expose one semantically
  equivalent WorkflowRun envelope containing a run ID, workflow Card identity
  when registered, terminal workflow status, named workflow outputs, and a map
  of step results keyed by step ID.
- **REQ-021:** Each step result MUST expose status, normalized text and/or
  structured output, attempt count, timing, and a stable error when applicable.
  Unstarted steps MUST be distinguishable from skipped, cancelled, failed, and
  completed steps.
- **REQ-022:** Intermediate step results MUST be retained for the lifetime of
  the returned WorkflowRun and emitted through the existing Skald observation
  boundary. Provider-specific raw responses MAY remain a local Rust diagnostic
  but MUST NOT become the portable Workflow wire contract.
- **REQ-023:** Workflow output and step-result ordering MUST be deterministic
  for the same completed graph even when independent steps finish in a
  different order.

### Local authoring and execution surfaces

- **REQ-024:** Rust MUST support all three user journeys through the shared
  Skald runtime: load and run an unregistered local YAML bundle; fetch, hydrate,
  and run a registered Workflow locally; and invoke a registered Workflow on
  the server.
- **REQ-025:** Local YAML loading MUST use the shared loader's `path`, `inline`,
  and `ref` semantics. `path` dependencies are loaded from disk without being
  registered. A `ref` requires registry access and MUST fail clearly when no
  registry client is available.
- **REQ-026:** The CLI MUST expose one `wyrd workflow run` command. It MUST
  accept exactly one source: `--file <path>`, `--uid <uid>`, or the complete
  registered selector `--space <space> --name <name> --version <version>`.
  It MUST accept JSON input from `--input <json>` or
  `--input-file <path>`, and `--execution local|server` with `local` as the
  default. File sources MUST be local; `--execution server` MUST require a
  registered selector. A registered selector with local execution MUST fetch
  and hydrate the locked Card graph before running it in the CLI process. The
  existing `--server <url>` connection option continues to select the Wyrd
  endpoint and MUST NOT be overloaded as an execution-mode flag.
- **REQ-027:** CLI success output MUST support machine-readable JSON containing
  the full WorkflowRun. Human output MAY summarize status and final outputs but
  MUST provide an explicit way to include intermediate step results. CLI
  failures MUST retain stable Wyrd error codes.

### Registration and server execution

- **REQ-028:** `wyrd apply <path>` MUST register a valid Workflow Card and its
  local Agent and Prompt dependencies through the existing composite
  registration path. Registration MUST remain declarative and MUST NOT execute
  the workflow.
- **REQ-029:** Server execution MUST require a registered, active, exactly
  versioned Workflow Card. The server MUST resolve the locked Agent and Prompt
  graph within the authenticated tenant before starting the first step.
- **REQ-030:** `wyrd-server` MUST expose a typed synchronous
  `POST /v1/workflows/invoke` operation accepting a Workflow CardRef, JSON
  input, and an optional caller deadline bounded by the server ceiling. It MUST
  return the WorkflowRun envelope for completed, failed, or cancelled runtime
  outcomes.
- **REQ-031:** `wyrd-client` MUST own the shared remote invocation method used
  by Rust and CLI. Local execution MUST continue to use Skald directly; the
  shared client MAY compose registry fetching and Skald hydration but MUST NOT
  duplicate the workflow executor.
- **REQ-032:** Server execution MUST authorize a typed `workflow:invoke`
  permission, derive tenant and actor from verified credentials, enforce Card
  state and policy, and fail closed before provider or tool side effects when
  identity, authorization, policy, dependency resolution, or acceptance audit
  fails.
- **REQ-033:** Server execution MUST durably audit invocation acceptance before
  the first external effect and record the terminal outcome afterward,
  correlated by workflow CardRef, run ID, request ID, and actor. Audit records
  MUST NOT contain raw workflow input, prompts, credentials, or model output.
- **REQ-034:** Server provider credentials and tool capabilities MUST come from
  server configuration and the registered Agent graph, never from invocation
  input or secret values stored in Workflow Cards.

### Provider and gateway portability

- **REQ-035:** Workflow execution MUST continue to depend on Skald's provider
  abstraction/registry rather than provider-specific workflow branches.
- **REQ-036:** Direct provider execution and future shared-gateway execution
  MUST be runtime configuration choices. No gateway URL, credential, routing
  policy, or provider implementation detail may be persisted in the Workflow
  Card solely for this change.
- **REQ-037:** A gateway-backed provider implementation MUST be able to replace
  direct providers without changing DAG, binding, output, retry, timeout, or
  observation semantics.

## Invariants and boundaries

- **INV-001:** Skald owns the reusable workflow runtime. `wyrd-server` hosts
  that runtime; it does not reimplement it. Vala may consume Skald workflows
  but Skald must not depend on Vala or Eval types.
- **INV-002:** `wyrd-spec` owns pure, synchronous, IO-free Workflow contracts,
  validation, schemas, and public errors. It remains PyO3-free.
- **INV-003:** Workflow Cards remain declarative. They contain no live provider,
  registry, server, database, credential, or tool objects.
- **INV-004:** Local execution remains available without registration and does
  not acquire server tenancy, policy, or audit semantics merely because the
  same definition can also run on a server.
- **INV-005:** Registered local and server execution use the exact registered
  Workflow, Agent, and Prompt versions; resolution must not silently float to a
  newer dependency.
- **INV-006:** Server execution preserves tenant isolation across registry
  reads, policy, provider/tool configuration, audit, observations, caches, and
  returned results. Invocation input never selects tenant identity.
- **INV-007:** Every public surface projects the same WorkflowRun and stable
  error semantics. CLI or Rust convenience must not create a competing durable
  contract.
- **INV-008:** The runtime may perform only declared Agent behavior and the
  tools authorized for those Agents. A workflow is not an arbitrary code or
  shell execution engine.

## Non-goals

- Python or TypeScript workflow execution surfaces in this change. Their later
  projections must reuse the same contracts and runtime/client boundaries.
- A durable workflow queue, scheduler, background job service, resume/replay
  service, or server-side run registry.
- Persisting local or server WorkflowRun payloads beyond existing observation
  and audit integrations.
- Dynamic graph mutation, loops, branches that add steps, map/reduce fan-out,
  human approval pauses, compensation transactions, or nested Workflow steps.
- A new expression language or conditional branching. Bindings use bounded
  interpolation over the declared input and step-output roots.
- Gateway implementation. This change preserves the provider-registry seam
  required to adopt the shared gateway later.
- Direct Prompt or MCP workflow steps; Agents already own Prompt execution and
  MCP tool use.
- Provider-specific raw responses as a cross-language or server wire contract.

## Expensive-to-reverse decisions

1. One Workflow Card drives both local and server execution; there is no
   server-only workflow definition.
2. Workflow steps invoke Agents only. Prompt and MCP composition remains inside
   Agent contracts and runtime registries.
3. Dependencies are explicit; data references never create hidden graph edges.
4. Step outputs are namespaced and workflow outputs are explicit. Flat shared
   parameter mutation and last-step-wins output are not public semantics.
5. Server invocation is synchronous and bounded. Durable asynchronous jobs are
   deferred until a real use case requires lifecycle, queue, and resume
   semantics.
6. Runtime failures return a terminal WorkflowRun with partial results;
   request/auth/validation failures remain structured Wyrd errors.
7. Gateway use is injected through Skald provider configuration and does not
   alter the Workflow Card.

## Acceptance criteria and evidence

- **AC-001:** A checked-in code-review Workflow YAML with two independent
  reviewer Agents and one dependent final-review Agent loads and executes
  locally through Rust and CLI, with both reviewers running in the same DAG
  stage and their distinct outputs bound into the final reviewer.
- **AC-002:** The same YAML bundle registers through `wyrd apply`; its stored
  Workflow relationships point to the exact Agent and Prompt Card versions.
- **AC-003:** The registered code-review Workflow can be fetched and executed
  locally through Rust and CLI with results equivalent in shape and binding
  semantics to unregistered execution.
- **AC-004:** A real Rust client invokes the registered Workflow through a real
  `wyrd-server` and receives final plus intermediate results. The matching CLI
  server invocation exercises the same client path.
- **AC-005:** A concurrency-sensitive test proves independent steps overlap and
  a dependent step starts only after all declared dependencies complete.
- **AC-006:** Contract tests prove duplicate/missing/self/cyclic dependencies,
  hidden step references, missing inputs, invalid output selectors, empty
  outputs, and unsupported actions fail with stable field-specific errors
  before provider execution.
- **AC-007:** A collision test proves two parallel steps can both emit a field
  named `summary` and the final step receives the intended
  `steps.<id>.output.structured.summary` values deterministically.
- **AC-008:** A failure journey proves a terminal step error returns a failed
  WorkflowRun containing completed sibling/intermediate results and distinct
  unstarted downstream steps.
- **AC-009:** Server journeys prove unregistered/inactive/cross-tenant Workflow
  refs, under-privileged callers, unresolved dependencies, excessive input,
  excessive concurrency, and deadline expiry are rejected without unauthorized
  provider/tool execution.
- **AC-010:** Audit evidence proves accepted and terminal server invocations are
  correlated and redacted; an acceptance-audit failure prevents the first
  provider call.
- **AC-011:** A provider-registry test runs the same workflow with direct and
  gateway-compatible mock providers without changing the Workflow Card.
- **AC-012:** Generated Workflow schema, OpenAPI, error catalog, and CLI JSON
  output agree with the Rust source contract and regenerate without drift.
- **AC-013:** Focused unit and integration evidence covers pure validation,
  Skald runtime execution, loader hydration, registration, client transport,
  server authorization/audit, and CLI behavior. Real Rust/CLI
  client-to-server user journeys provide the completion evidence; unit tests
  alone are insufficient.

## Open material decisions

None. Revision 1 proposes the smallest complete agent-workflow surface matching
the requested journeys. Any change to action kinds, binding roots, execution
failure semantics, or synchronous server invocation requires a material spec
revision before implementation planning.

## Revision history

- **Revision 1 — draft (2026-09-10):** Initial proposal after repository
  research. Preserves existing Skald local execution, completes the
  WorkflowSpec-to-runtime path, adds Rust/CLI local registered execution and
  typed client-to-server invocation, namespaces intermediate outputs, and
  leaves gateway routing behind the existing provider abstraction. Expanded
  with the intended multi-agent YAML, local/registered/server CLI journeys,
  and direct-provider versus gateway runtime configuration.

## Authority links

- `AGENTS.md` §§2–12, 14
- `architecture/agent-rules.md`
- `architecture/wyrd-design.md` §§Workflow, Spec-file authoring, Runtime authz
- `architecture/wyrd-doctrine.mdx` §§Service boundaries, Runtime boundary,
  Public surfaces
- `architecture/wyrd-security-posture.md` §§Authorization and policy, Tenant
  and data isolation, Audit integrity and privacy
- `architecture/references/doctrine/architecture-constraints.md`
- `architecture/references/architecture/patterns.md`
- `architecture/references/languages/agent-harness.md`
- `architecture/references/languages/errors.md`
- `architecture/references/languages/testing-workflows.md`
- `architecture/references/languages/spec-driven-development.md`
- `architecture/references/domain/evaluation.md`
