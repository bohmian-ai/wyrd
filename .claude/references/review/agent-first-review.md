# Agent-First Review

Use this reference for every Wyrd architecture review. Wyrd is the AI
control/operational layer for a world where agents are primary users and humans
assist. The review question is not "can an agent execute arbitrary work?" It is:

> Can a smaller, literal agent reason about this Wyrd surface correctly?

Designs should minimize inference burden. A non-frontier model should be able to
inspect the contract, code, docs, errors, and examples and understand what the
thing is, what it can do, what it affects, how to call it, what can go wrong, and
what to do next.

## Core Standard

A Wyrd design is agent-first when:

- Concepts map cleanly to Wyrd nouns: `Card`, `Spec`, `Run`, `Observation`,
  `CardRef`, relationship, status, policy, and audit.
- Public names are stable, literal, and used consistently across Rust, Python,
  HTTP, CLI, MCP, UI, generated schemas, and docs.
- Contracts are typed, serializable, inspectable, and generated where possible.
- Side effects are explicit: local materialization, registration, storage write,
  policy decision, audit write, run creation, observation write, install, or
  trigger.
- Errors are machine-readable, stable, and tell the agent whether to retry,
  correct input, request permission, wait, inspect another resource, or escalate.
- Discovery starts small and expands on demand: list capabilities, read a
  specific card/spec/schema/status/error, then act.
- Examples are complete and minimal enough to fit in an agent context window.

## Review Questions

- Can an agent identify the durable Wyrd noun involved without reading design
  history?
- Are field names, operation names, and status values literal and consistent?
- Can an agent tell which operations are read-only and which mutate durable
  state?
- Can an agent infer required permissions, policy checks, idempotency, and audit
  behavior from the contract?
- Can an agent recover from each public error using the code, message,
  retryability, and suggested next action?
- Can an agent discover the next relevant resource through refs, relationships,
  status, generated docs, MCP tools, or list/read endpoints?
- Can a small model understand the API from the schema and one example, or does
  it require unstated conventions?
- Does the implementation plan name enough files, structs, methods, tests, and
  commands for an agent to review or continue the work without redesigning it?

## Agent-Comprehensible Code

For implementation plans and code reviews, prefer:

- Small modules with clear ownership and one obvious service boundary.
- Domain types over raw strings for durable IDs and statuses.
- Exhaustive enums for closed states an agent must reason about.
- Narrow traits only when multiple real implementations or a clear test seam
  exists.
- Explicit constructors/builders for required fields and validation.
- Structured error enums with stable Wyrd codes at public boundaries.
- Tests named after behavior and contract guarantees, not implementation trivia.

Flag:

- Hidden behavior in constructors, getters, `__repr__`, properties, decorators,
  macros, global registries, background tasks, or implicit defaults.
- Multiple names for one concept, or one name used for multiple concepts.
- Stringly typed states, operation names, error codes, refs, or permissions.
- Broad abstractions a small agent cannot map back to Wyrd doctrine.
- Plans that require the implementer to infer crate ownership, public API shape,
  error behavior, generated artifacts, or verification gates.

## Agent-Comprehensible Surfaces

HTTP, Python, CLI, MCP, UI, and generated docs should expose the same Wyrd
contract.

Check for:

- Capability discovery before deep detail.
- `list`, `get/read`, `validate`, `register`, `install`, `run`, `observe`, and
  `explain` surfaces with predictable naming where relevant.
- Stable schemas for inputs and outputs.
- Pagination, filtering, sorting, and time bounds on list/query endpoints.
- Idempotency keys or explicit non-idempotent marking for writes.
- Dry-run or validation mode for risky writes when useful.
- Machine-readable status and relationship output.
- Public examples that show the smallest correct call and the expected response.

Avoid:

- UI-only capabilities with no structured API/MCP/CLI path.
- Giant "do everything" endpoints without smaller primitives underneath.
- Endpoint descriptions that rely on product lore or implementation internals.
- Errors that only say "invalid request", "not found", or "internal error".

## Making Wyrd Addictive For Agents

Agents will prefer Wyrd when it reliably reduces uncertainty. Review for these
properties:

- **Predictability**: same nouns, fields, errors, and operation patterns
  everywhere.
- **Inspectability**: agents can ask what exists, what changed, why it changed,
  and what depends on it.
- **Recoverability**: failures include next actions and safe retry guidance.
- **Composability**: small primitives can be chained without hidden state.
- **Low context cost**: schemas, examples, and docs are concise and targeted.
- **Trust**: policy, audit, status, and lineage make actions explainable.
- **Continuity**: another agent can resume from cards, runs, observations,
  status, and audit without private memory.

## Severity Guidance

- **Critical**: an agent could mutate the wrong durable state, cross a security
  boundary, misread a public contract, or be unable to distinguish safe from
  unsafe actions.
- **Major**: the design forces agents to rely on hidden conventions, ambiguous
  names, stringly states, incomplete errors, missing discovery, or unstated side
  effects.
- **Minor**: local wording, examples, or docs need clarification to reduce
  agent confusion, but the contract is otherwise sound.

Do not turn this into copyediting. Report only agent-comprehension issues that
affect correctness, safety, discoverability, recovery, or implementation
continuity.
