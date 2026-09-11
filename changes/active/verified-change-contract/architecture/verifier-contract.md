# Verifier contract and authoring

**Status:** working design

This document defines the customer-facing Verifier Card, how customers author
and register Verifiers, which execution kinds Wyrd supports, and the smallest
useful set of Verifiers Wyrd should ship.

Decisions in this document are not authoritative until explicitly agreed and
reflected in `../spec.md`.

## Product goal

A customer should be able to describe one reusable verification judgment in one
normal Wyrd YAML Card, apply it, and attach its CardRef to either a Claim or a
whole Change Request.

```text
write verifier YAML
  → wyrd apply verifier.yaml
  → receive Verifier CardRef
  → attach CardRef to Claim or Change Request
  → Wyrd gathers declared Evidence
  → Wyrd runs the Verifier
  → Wyrd records one Verification Result
```

The Card must be understandable without knowing how Wyrd executes it.

## Terms that must remain distinct

### Distribution

Distribution describes who authored and maintains a Verifier Card:

- **Wyrd-shipped:** maintained and versioned by Wyrd.
- **Customer-defined:** maintained and versioned by a user, team, repository,
  or organization.

This is provenance, not a task kind. A customer-defined Verifier may use a
Wyrd built-in task, an LLM judge, Python, an API, or MCP.

### Verifier Card

A Verifier Card is one reusable judging operation. It declares:

- where it may be used: against a Claim, a whole Change Request, or both;
- the Evidence it requires; and
- the single task that produces its verdict.

### Verifier task

A task is the single executable operation inside a Verifier. Its `kind` selects
how the Verifier runs.

The proposed MVP task kinds are:

- `builtin`
- `llm_judge`
- `workflow`
- `python`
- `api`
- `mcp`

### Evidence

Evidence is immutable material supplied by Wyrd to tasks. The initial Evidence
types are:

- `code_diff`: the exact base-to-candidate code change; and
- `test_results`: CI/CD execution and individual test results for the exact
  candidate.

The Verifier declares required Evidence once. A Change Request author does not
wire inputs, sources, or arbitrary parameters when attaching that Verifier.

## Agreed direction

- `Verifier` is a registrable Wyrd Card kind.
- One Verifier Card declares exactly one task, produces exactly one verdict,
  and contains no internal workflow or task graph.
- That single task may execute a reusable Wyrd `Workflow` Card. The Workflow
  owns its internal agentic steps and returns one judgment to the Verifier.
- A Claim composes multiple Verifier CardRefs when it needs multiple judgments.
- Verifiers use the normal Card envelope and `wyrd apply` loader.
- Verifiers may be attached to a Claim or globally to a Change Request.
- Wyrd, repositories, and organizations may provide reusable Verifier Cards.
- Customers may define their own Verifier Cards.
- Wyrd must support deterministic, LLM, Python, API, and MCP-backed
  verification.
- Python Verifiers use a Wyrd-owned `monty-pool`; they do not require CPython,
  PyO3, virtual environments, or packages installed into `wyrd-server`.
- Monty parses, compiles, and type-checks Python during Verifier registration so
  unsupported code is rejected before the Card is accepted.
- During execution, Wyrd injects only the resolved verification context and
  declared Evidence into the checked-out Monty worker.
- Long-running API or MCP work continues under a durable Wyrd run after the
  initiating request returns.
- Every task kind produces the same Verification Result contract.

### VC-001: one execution and one verdict

**Status:** agreed on 2026-09-10

One Verifier Card represents one independently runnable judging operation and
produces one Verification Result. It may consume multiple Evidence types, but
it cannot contain multiple tasks, dependencies, gates, or internal aggregation.

The Claim is the composition boundary. For the motivating checkout Claim, it
references two Verifier Cards:

```text
Claim: all checkouts subtract 1
├── passing-tests
└── claim-evidence-review
```

This keeps retries, versioning, reuse, caching, audit, enforcement, and results
independent. It also prevents the Verifier Card from becoming a second workflow
engine. Wyrd may short-circuit or schedule these runs efficiently without
exposing a public DAG.

An API or MCP service may perform many internal steps. That remains opaque to
Wyrd: one invocation or durable external job still produces one Verifier
verdict.

A Workflow-backed Verifier follows the same boundary. `workflow_ref` may be a
local YAML path, registered Workflow CardRef, or inline `WorkflowSpec`, matching
the authoring ergonomics of an LLM judge's `judge_ref`. The Workflow receives
one Verification Input and returns one structured judgment; its intermediate
steps do not become Verifier tasks or Verification Results.

## Proposed base Card shape

The current proposal borrows Eval's typed task ergonomics without borrowing its
workflow shape: `task` is a closed `kind`-tagged union.

```yaml
apiVersion: wyrd/v1
kind: Verifier
metadata:
  space: checkout-team
  name: passing-tests
  version: "1.0.0"
spec:
  description: Requires the submitted CI tests to pass
  applies_to: both
  evidence: [test_results]
  task:
    kind: builtin
    check: passing_tests
```

This is proposed customer YAML, not yet a loader-valid contract. Each field
must be accepted, changed, or removed through the decisions below before the
shape enters `wyrd-spec`.

## Proposed base fields

| Field | Meaning | Current status |
|---|---|---|
| `description` | Human-readable purpose | Proposed |
| `applies_to` | `claim`, `change`, or `both` | Proposed |
| `evidence` | Required Evidence types | Proposed |
| `task` | One typed execution definition | Agreed |

No generic `inputs`, `sources`, `parameters`, `config`, or custom result schema
belongs on the base Card.

Open questions:

- Is `applies_to` necessary, or should placement alone determine context?
- Are all declared Evidence types required, or does the first version need
  optional Evidence?
- May Verifiers be authored inline inside a Change Request, or must every
  Verifier be a registered CardRef?

## Task kinds

### Built-in

A built-in task selects deterministic or Wyrd-operated behavior with a closed
name and typed variant-specific fields.

Candidate shape:

```yaml
task:
  kind: builtin
  check: passing_tests
```

Built-ins are useful inside both Wyrd-shipped and customer-defined Verifier
Cards. A customer composes a `passing-tests` Verifier with a separate LLM or
Python Verifier without reimplementing CI parsing.

Open decisions:

- the closed built-in names;
- whether CI provider/check selection belongs in `test_results` Evidence or on
  the task; and
- the common task result returned by a built-in.

### LLM judge

An LLM judge receives the optional Claim and declared Evidence, invokes one
constrained Agent, and returns a structured judgment.

It should follow the established Eval ergonomics for `judge_ref` and bounded
retries. It must not use Eval's boolean assertion result unchanged;
verification requires `passed`, `failed`, or `inconclusive`.

Open decisions:

- whether `InlineableRef<AgentSpec>` is reused unchanged;
- whether judge-specific instructions live on the Verifier, Agent, or Prompt;
- the exact injected context; and
- the structured judge output schema.

### Workflow

A Workflow task runs a Wyrd `Workflow` Card as the Verifier's single
execution:

```yaml
task:
  kind: workflow
  workflow_ref: ./code-review-workflow.yaml
```

`workflow_ref` supports a local YAML path, a registered Workflow CardRef, or an
inline `WorkflowSpec`. The Workflow owns its internal Agent, Prompt, and MCP
steps. It receives one Verification Input and must return one structured
judgment containing `verdict`, `summary`, and `findings`.

The public contract is a Wyrd `Workflow` Card. Skald is its execution engine,
not a second customer-authored workflow format. Wyrd adds run identity,
provenance, Evidence digests, and execution status around the returned
judgment.

### Python

A Python task runs Monty-compatible source without installing a Python runtime
into `wyrd-server`.

Candidate shape:

```yaml
task:
  kind: python
  code: |
    def verify(context):
        tests = context["evidence"]["test_results"]["tests"]
        failed = [test for test in tests if test["outcome"] != "passed"]
        return {
            "verdict": "failed" if failed else "passed",
            "summary": "Checkout tests failed" if failed else "Checkout tests passed",
            "findings": failed,
        }
```

Registration must validate YAML, parse, compile, type-check against Wyrd's
context stubs, validate the entrypoint, and persist the accepted source digest.
Registration does not prove runtime correctness.

Execution checks out a bounded Monty worker, injects immutable values, invokes
one entrypoint, validates the returned result, and discards or safely recycles
the worker. The Python task receives no host functions, environment, network,
filesystem, packages, or credentials in the initial contract.

Open decisions:

- full `def verify(context)` source versus a function body Wyrd wraps;
- inline `code` only versus loader support for a `.py` file;
- the exact context and return types;
- registration diagnostics and stable errors; and
- worker reuse and OS-level isolation policy.

### API

An API task maps Wyrd's canonical context into a customer-defined HTTP request,
starts external work, and maps the eventual response into the common result.

The contract must support:

- a synchronous API;
- an asynchronous API returning an external job identity;
- durable polling for 20–25 minute reviews;
- custom methods, headers, authentication references, and JSON bodies;
- typed context and Evidence insertion into customer-specific request shapes;
- terminal-state and result extraction; and
- restart-safe retry without duplicate external jobs.

No API YAML is proposed yet. The request-injection and retrieval contract must
be designed from complete customer examples rather than guessed field names.

### MCP

An MCP task maps canonical context into a registered MCP server's tool
arguments and maps tool output into the common result.

The contract must support:

- one synchronous verification tool; and
- a start tool plus a status/result tool for long-running verification.

MCP-specific design must preserve tool discovery and input-schema validation;
it should share durable run and result semantics with API tasks without being
forced into an HTTP-shaped abstraction.

No MCP YAML is proposed until synchronous and long-running customer examples
establish the necessary fields.

## Concrete MVP Verifier Cards

### 1. Passing tests

**Placement:** Claim or global.

**Evidence:** `test_results`.

**Behavior:** deterministically verifies that the CI execution completed and
the selected tests passed. A failed or incomplete CI execution remains distinct
from test failures.

**Implementation:** one `builtin` task using `passing_tests`.

This is the first Card to design because it fixes the minimum Card, Evidence,
result, registration, and execution contracts without LLM or external
integration complexity.

### 2. Claim evidence review

**Placement:** Claim only.

**Evidence:** `code_diff` and `test_results`.

**Behavior:** judges whether the code implements the Claim and whether the
introduced tests meaningfully prove it. It may use test outcomes as evidence,
but it does not replace the deterministic `passing-tests` Verifier.

**Implementation:** one `llm_judge` task.

The Claim attaches this Card and `passing-tests` when it requires both
judgments.

### 3. Code review

**Placement:** global.

**Evidence:** `code_diff` and, when required by the final contract,
`test_results`.

**Behavior:** reviews the whole change for correctness, security, regressions,
and maintainability without pretending there is a Claim.

**Implementation:** an LLM-judge or Workflow-backed Card shipped by Wyrd. A
team may define a repository-specific multi-agent Workflow and reference it
from one Verifier. Repository or organization policy may enforce an exact
version.

### 4. Customer Python rule

**Placement:** Claim, global, or both according to the final base contract.

**Evidence:** selected from the supported Evidence types.

**Behavior:** performs small deterministic customer logic without requiring
the customer to deploy a service or Wyrd to install CPython.

**Implementation:** one `python` task executed by `monty-pool`.

This is an extension mechanism, not one named Wyrd-shipped rule.

### 5. Customer API review

**Placement:** Claim or global.

**Evidence:** declared by the Card and disclosed explicitly.

**Behavior:** invokes a customer or vendor API, returns immediately with a Wyrd
run identity, and durably retrieves the eventual judgment.

**Implementation:** one `api` task.

This is the MVP external-service integration. Vendor-specific built-ins should
be added only when a real vendor cannot be represented honestly by the API
contract.

### 6. Customer MCP review

**Placement:** Claim or global.

**Evidence:** declared by the Card and mapped to discovered tool arguments.

**Behavior:** invokes a synchronous verification tool or starts and retrieves
a long-running MCP verification.

**Implementation:** one `mcp` task or the smallest paired-tool shape established
by the MCP design slice.

## MVP boundary

The proposed MVP consists of:

- three concrete Wyrd-shipped Verifier Cards: `passing-tests`,
  `claim-evidence-review`, and `code-review`;
- six task kinds: `builtin`, `llm_judge`, `workflow`, `python`, `api`, and
  `mcp`; and
- one common Verification Result contract.

The task kinds are extensibility mechanisms, not five default Verifiers.

Explicitly deferred:

- arbitrary CPython and third-party Python packages;
- filesystem, network, environment, or general host capabilities for Python;
- weighted scores or configurable pass gates;
- optional/advisory tasks;
- automatic Verifier selection;
- a public marketplace; and
- vendor-specific integrations without a concrete API/MCP incompatibility.

## Decisions required next

Resolve these in order:

1. Is `applies_to` authored, or does attachment placement alone determine
   Claim versus Change Request context?
2. What are the exact base Card fields and Evidence names?
3. What is the common Verification Result?
4. What loader-valid YAML defines `passing-tests`?
5. What loader-valid YAML defines `claim-evidence-review`?
6. What exact context does each task receive?
7. What Python source/entrypoint contract does Monty validate?
8. What API and MCP request, retrieval, and result-mapping YAML is necessary?

Do not design API or MCP field names before decisions 1–6 produce a stable
canonical context.
