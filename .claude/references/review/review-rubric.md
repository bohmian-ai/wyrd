# Review Rubric

Lead with issues. Severity should reflect concrete doctrine, product,
reliability, security, migration, or DX impact.

## Severity

- **Critical**: Violates a locked decision, breaks the core doctrine, corrupts
  durable contracts, creates audit/governance risk, or makes implementation
  likely to encode the wrong public API.
- **Major**: Creates a likely reliability, security, performance, or DX failure;
  leaks service internals into public surfaces; misses a required predecessor
  parity decision; or requires major rework later.
- **Minor**: Ambiguous ownership, missing verification, naming drift, avoidable
  coupling, documentation gaps, or localized maintainability risk.

## Finding Threshold

Do not report generic style nits, preference-only naming comments, wording
polish, speculative abstractions, or optional documentation improvements unless
they can:

- Mislead implementation or encode the wrong public contract.
- Weaken security, tenant isolation, policy, audit, or redaction.
- Create reliability, performance, cost, migration, or operational risk.
- Harm agent/developer ergonomics on a public surface.
- Cause meaningful rework later.

## Dimensions

### Agent-First Comprehensibility

Wyrd is built for a world where agents are primary users and humans assist.
Review whether a small/basic agent can reason about the contract, code, docs,
errors, side effects, and next steps without hidden context or frontier-level
inference. Use `agent-first-review.md`.

### Doctrine Fit

Does the proposal preserve the layers: core nouns, card kinds, foundations,
services, and surfaces? Push back when a surface invents a public shape or a
service bypasses shared foundations.

### Nouns, Kinds, And Specs

Check whether the durable fact is a `Card`, `Spec`, `Run`, or `Observation`.
Card kinds must specialize `Card`, not introduce separate registries, envelopes,
route families, or SDK ontologies.

### Foundations

Check envelope shape, `CardRef`, versioning, metadata, derived relationships,
server-managed status, serialization, generated schemas, and stable error
codes.

### Lifecycle And Side Effects

Check local save/load versus durable registration, storage-owned bytes,
`ArtifactCard` linkage, registry/client ownership, policy checks, audit writes,
and hidden side effects.

### Rust, Python, And PyO3

Check `wyrd-spec` purity, per-type PyO3 feature gates, thin `python/py-wyrd`,
Python exports/stubs, public import tests, maturin verification, and
Python-lifetime test placement.

### Surfaces And Agents

Check HTTP, Python SDK, CLI, UI, MCP, generated docs, OpenAPI, schemas, errors,
and agent-facing discoverability for semantic alignment.

### Predecessor Parity

Check whether opsml, scouter, or potatohead already have relevant behavior and
whether Wyrd will reuse, adapt, replace, or omit it with a concrete reason.

### Verification

Plans should name concrete gates: targeted Rust tests, `mise run py:setup`,
`mise run py:test:unit`, `mise run codegen:check`, boundary checks, markdown
review, and demo commands where relevant.

### Right-Sized Architecture

Review whether the proposal is the simplest architecture that correctly solves
the user workflow while preserving Wyrd doctrine, local best practices, and
industry-standard engineering safeguards.

Flag designs that:

- introduce a new durable noun, service, crate, public route family, trait,
  registry, compatibility layer, state machine, or generated surface when an
  existing Wyrd foundation or service can express the same fact cleanly
- implement future phases, enterprise concerns, or speculative variants instead
  of naming them as deferred decisions
- split one coherent behavior across too many abstractions, files, PRs, or
  surfaces in a way that raises implementation or review risk
- combine unrelated concerns into one PR or contract when a smaller phase would
  reduce risk without losing architectural coherence
- duplicate predecessor or local behavior instead of reusing, adapting, or
  explicitly rejecting the existing pattern

Do not report "simpler" as a goal by itself. A simpler recommendation must be
concrete, doctrine-compatible, behavior-preserving for the stated workflow, and
must not weaken security, tenant isolation, auditability, reliability,
observability, error quality, agent-first usability, verification, or accepted
industry standards.

### Production Architecture

For service, storage, runtime, observability, or high-volume plans, also check
scale targets, security boundary, idempotency, retries, backpressure, leases,
snapshot/freshness semantics, retention, capacity testing, and operational
metrics. Use `production-architecture-rubric.md`.

### OLAP, Arrow, And Observability

For Vala, observations, traces, eval/drift results, or analytical query paths,
check Bifrost/Iceberg placement, DataFusion pushdown, object-store layout,
Arrow schemas, high-cardinality telemetry, redaction, OTel context propagation,
and run/observation alignment. Use `olap-datafusion-iceberg-arrow.md` and
`observability-otel.md`.

### Rust Service Architecture

For implementation plans, check crate layering, typed APIs, trait boundaries,
async ownership, backpressure, public errors, PyO3 placement, dependency
hygiene, and verification gates. Use `rust-service-architecture.md`.

## Finding Format

Use:

`[Severity] Title - evidence -> doctrine/impact -> recommendation`

Include file and line references when available.
