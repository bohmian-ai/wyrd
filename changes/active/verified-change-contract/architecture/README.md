# Verified Change architecture

## Purpose and authority

This folder is the working design reference for the Verified Change contract.
It records agreed product decisions, user ergonomics, public contract shapes,
and the reasoning needed to keep later decisions consistent.

Only decisions explicitly accepted by the human reviewer are **agreed**.
Unresolved choices remain **open**; ideas intentionally excluded from the
current change are **deferred**. The approved `../spec.md` remains the formal
change authority. These architecture notes supply the concrete design that the
spec summarizes.

For continuous Drift and Eval, the current client contract is
[`logic/run_api.md`](logic/run_api.md): `WyrdState` owns a Bifrost facade and
its existing pooled queue; startup describes the fixed system tables and
dynamic `observe.record(table, value)` describes a user table on first use;
each scoped `observe.drift(...)` / `observe.eval(...)` call converts its input
to the existing typed record and fixed table rows before queue insertion.
Python Runs also provide an optional, fail-open OpenTelemetry context manager
that applies the selected CardRef and invocation ID to spans emitted inside
the scope without changing explicit observation or Bifrost lifecycle semantics.
Shared `wyrd-client` and `wyrd-queue` do not own Verifier-specific projection.
The locked runtime flow is [`verification-control-flow.html`](verification-control-flow.html);
`verification-runtime.html` is an earlier, superseded proposal.

Do not design from Rust types inward. Begin with the exact artifact or command
a customer writes, resolve it to the durable wire contract, and only then
choose implementation types.

## First principles

1. **One obvious workflow.** A human opens or revises a Change Request through
   `wyrd change`; an agent performs the same operation through the typed server
   contract exposed by MCP or HTTP.
2. **Four public concepts.** A Subject is what changes, a Claim says what the
   change intends, Evidence is what was observed, and a Verifier judges the
   Evidence against either a Claim or the whole Change Request.
3. **Verifiers are reusable Cards.** `Verifier` is a registrable Card kind that
   users, teams, repositories, and organizations can author once and reuse.
4. **Real YAML before abstract schema.** A public field is not settled until a
   complete customer-authored YAML example makes its purpose obvious.
5. **Exact identity underneath simple ergonomics.** The CLI may infer context,
   but every revision and result resolves to exact repositories, commits, Card
   versions, and Evidence digests.
6. **The server owns durable truth.** CLI, MCP, HTTP, SDK, and future UI are
   projections of the same contract. Clients do not own revision, execution,
   result, enforcement, or audit semantics.
7. **External work is durable.** A long-running API or MCP Verifier continues
   independently of the request that started it and can recover after a Wyrd
   server restart.
8. **Failure is not judgment.** Execution failure and a Verifier's
   `passed | failed | inconclusive` verdict remain distinct.
9. **No hidden authority.** Repository- and organization-enforced Verifiers
   are visible on the resolved revision and cannot be removed by its author.
10. **Add structure only when a real workflow needs it.** No capability
    selector, parameter bag, Verifier DAG, mapping language, or new durable
    noun without a customer example that cannot work without it.

## Agreed foundation

The following decisions are the current baseline:

- A Change Request contains one or more Subjects, Claims with Claim-scoped
  Verifiers, and optional global Verifiers outside Claims.
- A Claim describes intended behavior. Its Verifiers judge that Claim.
- A global Verifier judges the whole Change Request without a synthetic Claim.
- Repositories and organizations may enforce global Verifiers.
- Pull requests are optional. `wyrd change open` supports no pull request,
  linking an existing pull request, or creating and linking one.
- `wyrd change revise` supports the same choices and refreshes linked pull
  requests and all Subjects to exact current commits.
- `Verifier` is a reusable registrable Card kind.
- One Verifier Card declares one task, runs one judging operation, and produces
  one verdict. Claims compose multiple Verifier CardRefs when needed.
- A Verifier's single task may execute a referenced or inline Wyrd `Workflow`
  Card. The Workflow owns multiple agentic steps but returns one judgment.
- Customers author Verifier Cards as normal Wyrd YAML and load them through
  the existing Card loader, following the ergonomics established by Eval
  Cards.
- Verifiers must cover Wyrd built-ins, customer-defined LLM judges, Wyrd
  Workflows, Python, external APIs, and MCP tools.
- Verification can use the exact code change and CI/CD test results as
  Evidence.
- Built-in, customer-defined, API, and MCP execution must normalize to one
  Verification Result contract.

Everything beyond this list remains open until its design slice is reviewed.

## Design method

Each design slice must show, in this order:

1. The user goal and representative workflow.
2. The exact CLI interaction or complete customer-authored YAML.
3. The resolved durable JSON/wire shape when it differs from authoring syntax.
4. Validation, lifecycle, success, failure, and edge behavior.
5. Security, tenancy, provenance, and audit consequences.
6. Decisions proposed for agreement.
7. Decisions still open or explicitly deferred.

Pseudo-YAML that cannot be loaded is not design evidence. Verifier examples
must use the common Card envelope and the same `ref`, `path`, or `inline`
authoring forms supported by the final typed reference slot.

## Decision order

The order is dependency-driven. Later slices must not invent answers owned by
an earlier slice.

### 1. Claim and Verifier authoring

Start with one complete vertical example:

> Claim: Service A introduces new behavior—all checkouts subtract 1.

The customer attaches:

- a passing-tests Verifier that reads CI/CD results; and
- an LLM Verifier that reads the code change and test results, checks that the
  implementation matches the Claim, and checks that introduced tests prove it.

Lock together:

- the exact Claim contract;
- how Claim Verifiers are referenced;
- how global Verifiers are referenced;
- the common Verifier Card fields;
- how a Verifier declares the Evidence it needs;
- the exact single-task Verifier shape;
- the minimum common task fields; and
- the common result required from every Verifier.

Deliverables:

- one complete Change Request authoring example;
- loader-valid YAML for `passing-tests`;
- loader-valid YAML for the LLM code-and-tests Verifier; and
- their resolved CardRefs and expected Verification Results.

This slice must pass the ergonomic test before external execution is designed.

### 2. Verifier creation and reuse

Work through the complete customer journey:

```text
write verifier YAML
  → validate locally
  → wyrd apply
  → receive a versioned Verifier CardRef
  → attach it to a Claim or Change Request
  → revise it by registering a new Card version
```

Decide:

- required and optional metadata;
- local validation behavior and errors;
- `ref`, `path`, and `inline` support;
- whether one-off inline Verifiers are allowed;
- versioning behavior when Evidence needs or judging behavior changes;
- how Verifiers are listed, inspected, and tested before use; and
- how repository or organization defaults select registered Verifiers.

### 3. Evidence context

Define Evidence from what Verifiers actually consume, not from a generic
upload abstraction.

Start with only:

- the exact code change between base and candidate; and
- CI/CD test results for the candidate.

Decide:

- canonical names and typed shapes;
- how Wyrd obtains each Evidence type;
- provenance and digest requirements;
- readiness and freshness;
- inline versus referenced delivery;
- size, media-type, redaction, and retention limits; and
- the exact context presented to Claim-scoped and global Verifiers.

### 4. Built-in and LLM Verifiers

Define the simplest execution families first.

For built-ins, decide:

- closed built-in names;
- variant-specific YAML fields;
- CI provider and check selection; and
- deterministic result and error behavior.

For LLM judges, decide:

- reuse of the existing Eval `InlineableRef<AgentSpec>` shape;
- whether instructions belong in the Verifier, Agent, or Prompt;
- the structured judge response;
- retries, timeout, abstention, and invalid-output behavior; and
- the exact Claim and Evidence context disclosed to the judge.

### 5. API Verifiers

Design against two concrete customer examples: one synchronous custom API and
one API that runs for 20–25 minutes.

Decide:

- supported HTTP methods;
- URL, header, authentication, and JSON body authoring;
- how typed Claim, Change Request, Subject, and Evidence values are injected
  into a customer-specific request shape;
- whether existing Wyrd string templating is sufficient or typed JSON
  insertion is required;
- synchronous completion;
- asynchronous job identity and durable retrieval;
- polling request and terminal-state mapping;
- cancellation, timeout, retry, and restart recovery;
- response extraction into status, verdict, summary, and findings; and
- whether callbacks are supported or deferred.

The initial caller receives a Wyrd verification-run identity. It never holds a
CLI, HTTP, or MCP request open for the lifetime of an external review.

### 6. MCP Verifiers

Design against one synchronous MCP tool and one long-running MCP service.

Decide:

- the referenced `Mcp` Card and selected tool or tools;
- discovery-time tool-schema validation;
- how canonical context becomes customer-specific tool arguments;
- Evidence delivery and disclosure;
- direct terminal results versus start-plus-retrieve tools;
- external run identity, polling, timeout, cancellation, and recovery; and
- mapping arbitrary tool results into the common Verification Result.

Reuse API execution semantics where they are genuinely identical, but do not
hide MCP tool discovery and schema behavior behind an HTTP-shaped abstraction.

### 7. Results and aggregation

Lock:

- verification-run lifecycle;
- the common Verification Result;
- exact revision, Claim, Verifier, Subject, and Evidence binding;
- `passed`, `failed`, and `inconclusive` semantics;
- Claim satisfaction;
- global Verifier aggregation;
- revision-level Verified derivation;
- stale-result rejection and safe result reuse; and
- the separation between verification, authorization, approval, and merge.

### 8. Subjects, pull requests, and revisions

Lock the exact behavior of:

- `wyrd change open` with no pull request;
- linking an existing pull request;
- creating and linking a pull request;
- multi-repository and stacked subjects;
- `wyrd change revise` refreshing every Subject and linked pull request;
- changing, adding, removing, or unlinking Subjects;
- immutable revision identity;
- optimistic concurrency and idempotent retries; and
- whether incomplete work is a mutable draft or an unpublished revision.

### 9. Enforcement and trusted configuration

Decide:

- where repository-enforced Verifiers are declared;
- where organization-enforced Verifiers are declared;
- precedence and deny/removal behavior;
- how the resolved revision explains why a Verifier is enforced;
- what happens when the candidate changes its own Verifier YAML or repository
  configuration; and
- which trusted base supplies enforcement and Verifier definitions.

### 10. Public service surfaces

After the domain contract is stable, lock the thin projections:

- CLI commands and machine-readable output;
- HTTP resources and request/response types;
- MCP tools and write scopes;
- Rust, Python, and TypeScript SDK methods;
- stable errors; and
- future UI operations.

### 11. Security, durability, and operations

Confirm the cross-cutting contract for:

- tenant isolation;
- transactional audit at each durable transition;
- secret references and rotation;
- least-privilege Evidence disclosure;
- SSRF-safe external API access;
- durable scheduling and restart recovery;
- bounded concurrency, retries, payloads, and execution time;
- duplicate paid-run suppression; and
- retention and deletion.

### 12. Acceptance journeys

Map the final contract to real user journeys:

- human author creates, opens, revises, and closes a Change Request;
- agent performs the same workflow through MCP;
- passing-tests and LLM Claim Verifiers run against one exact candidate;
- built-in and enforced global code review run;
- synchronous and long-running API Verifiers complete;
- synchronous and long-running MCP Verifiers complete; and
- missing, stale, malformed, unauthorized, timed-out, and inconclusive paths
  fail closed.

## Planned architecture slices

Create a file only when active design work begins:

```text
architecture/
├── README.md
├── verifier-contract.md
├── claim-contract.md
├── evidence-context.md
├── builtin-and-llm-verifiers.md
├── api-verifiers.md
├── mcp-verifiers.md
├── results-and-aggregation.md
├── subjects-and-revisions.md
├── enforcement-and-trust.md
└── public-surfaces.md
```

Security and acceptance remain cross-cutting sections in each slice until the
design proves that separate documents would reduce duplication.

## Immediate next decision

The active design slice is [`verifier-contract.md`](./verifier-contract.md).
The one-task, one-verdict boundary is agreed. The next decision is the exact
base Card fields and loader-valid YAML for `passing-tests`.
