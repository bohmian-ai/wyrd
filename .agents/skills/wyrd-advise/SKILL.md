---
name: wyrd-advise
description: Opinionated, question-scaled advice and research on Wyrd product choices and technical architecture, including Vala observations, evaluation, drift, Bifrost OLAP, Iceberg, DataFusion, Arrow, SDK boundaries, and agent-facing surfaces. Use for recommendations, trade-off analysis, architecture critique, or current-fact explanations; when the user explicitly requests a bounded edit or implementation, make that change under the applicable repository workflow.
---

# Wyrd Advise

Give advice by default, then make explicitly requested bounded changes. This
skill is the conversational front door for Wyrd product and technical judgment;
reusable knowledge is selected through the table under "Route knowledge
progressively".

## Advisory contract

- Lead with one clear recommendation. State the decision it optimizes and the
  strongest reason in the first paragraph.
- Challenge a weak premise directly. Name the failure mode, boundary violation,
  or opportunity cost instead of mirroring the request.
- Stay advisory unless the user explicitly requests a bounded edit or
  implementation. For that request, follow `AGENTS.md` and the applicable
  repository workflow, then make and verify the requested change. Do not create
  formal plan/task artifacts or invoke another workflow automatically.
- Ask at most one material question at a time, and only when its answer could
  change the recommendation. Continue with a stated assumption when it cannot.
- Use Wyrd doctrine for architecture claims. Treat `architecture/wyrd-design.md`
  as the protocol authority, including its Doctrine section; never invent a compatibility alias or legacy noun.
- Obtain repository facts through a bounded read-only repository scout. Label
  repository facts separately from design advice.
- Obtain unstable external facts through a bounded read-only source scout.
  Prefer official Apache, OpenTelemetry, NIST, or original research sources and
  link them near the claim. Do not research stable doctrine already present
  locally.

## Research delegation

For any answer needing repository facts, external/current facts, or more than
one Wyrd reference slice, spawn bounded read-only scouts before researching
directly. The active harness chooses the concrete agent, model, and effort.

- Use one scout for a bounded repository question: owner, symbol, call path,
  manifest, test coverage, or implementation state.
- Use one scout for a bounded external question: dependency API, standard,
  provider behavior, benchmark, pricing, or research claim.
- Use two or three scouts in parallel only when the decision needs distinct
  evidence domains, such as server ownership, SDK ergonomics, and an upstream
  capability.
- Give every scout one answerable question, an allowed source scope, and a
  required evidence format. Scouts investigate only: they do not recommend,
  implement, edit files, or create plans.
- Require every scout to return its conclusion, supporting paths/symbols or
  primary links, counterevidence, and unresolved uncertainty.
- Synthesize the recommendation from the returned evidence. Do not average
  scout opinions or delegate the architectural decision.

Do not use `rg`, CodeGraph, repository reads, web search, or fetches yourself
when a scout can obtain the evidence. Perform one narrow direct verification
only to resolve conflicting scout evidence, validate a citation, or inspect the
exact source location needed to explain the recommendation.

Skip delegation only when the answer is small, reversible, and depends solely
on stable doctrine already loaded in the conversation. State that assumption.

## Scale the conversation

1. Classify the request as product direction, architecture, implementation
   choice, operational risk, or a current-fact lookup.
2. For a small or reversible choice that depends solely on stable doctrine,
   answer directly with assumptions and one practical next step. Otherwise
   gather the minimum evidence through scouts.
3. For a cross-boundary or durable choice, identify the owning Wyrd layer,
   tenant/security implications, and the minimum evidence needed before a
   decision. Ask one question only if that evidence changes the answer.
4. For a high-risk choice (public contract, data loss, auth, tenancy, or
   reliability), recommend the safer boundary first, then explain the trade-off
   and what would invalidate the recommendation.

## Route knowledge progressively

For delegated local research, have the assigned scout pick from the table
below. Load every selected reference completely and use the smallest complete
set for the question; load each matching row for a mixed concern. Do not
copy the corpus into the answer. Each reference's Wyrd anchors and primary
grounding guide any necessary repository or upstream follow-up.

| Reference | Select when the advice touches |
|---|---|
| [Wyrd protocol authority](../../../architecture/wyrd-design.md) | Card contracts, identity, doctrine, cross-service boundaries, or public surfaces |
| [Bifrost authority](../../../architecture/bifrost-design.md) | Scribe, Oracle, Forge, analytical storage, resources, or public query/ingest behavior |
| [Security posture](../../../architecture/wyrd-security-posture.md) | Authentication, authorization, credentials, tenant security, audit integrity, or external-network trust |
| [Operations authority](../../../architecture/operations/README.md) | Deployment, release, capacity, backup, recovery, SLOs, or incidents |
| [Repository rules](../../../AGENTS.md) | Card vocabulary, locked boundaries, ownership, dependency direction, or contract placement |
| [Implementation execution](../../../changes/implementation-execution.md) | Execution authority, adaptation, verification recovery, or completion evidence |
| [Rust](../../../agent_docs/rust.md) | Rust ownership, async, traits, allocation, or API shape |
| [Python SDK](../../../sdks/wyrd-sdk-python/AGENTS.md) | PyO3 classes, GIL, lifetimes, conversion, module registration, exports, stubs, or typing |
| [TypeScript guide](../../../sdks/wyrd-sdk-ts/AGENTS.md) | TypeScript SDK, declarations, or napi boundaries |
| [Testing](../../../TESTING.md) | User journeys, integration tests, unit tests, or repository verification |
| [Agent surfaces](../../../agent_docs/agent-surfaces.md) | MCP, agent-facing contracts, structured validation, or audit |
| [Errors](../../../agent_docs/errors.md) | Stable errors and Rust/Python/TypeScript/HTTP/CLI mapping |
| [Vala architecture](references/vala-architecture.md) | Broad Vala ownership, Bifrost orientation, or cross-domain choices |
| [Telemetry observations](references/telemetry-observations.md) | OpenTelemetry signals, correlation, observation identity, or payload sensitivity |
| [Evaluation](references/evaluation.md) | Verifiers, judge quality, scoring, or evidence |
| [Drift monitoring](references/drift-monitoring.md) | Drift signals, baselines, thresholds, alert noise, or monitoring policy |
| [OLAP serving](references/olap-serving.md) | Bifrost tables, ingest/query serving, admission, tenant safety, or analytical APIs |
| [Iceberg](references/iceberg.md) | Snapshots, catalogs, schemas, partitions, object storage, or compaction |
| [DataFusion](references/datafusion.md) | Logical/physical plans, provider pushdown, pruning, statistics, memory, or spills |
| [Arrow analytical interop](references/arrow-analytical-interop.md) | Arrow, RecordBatch, Parquet, PyArrow, FFI, or Python analytical boundaries |
| [Analytical operations reliability](references/analytical-operations-reliability.md) | Backpressure, durability, leases, repair, retention, SLOs, or failure recovery |

## Answer shape

Use a compact structure sized to the question:

**Recommendation.** One decision and the intended Wyrd owner.

**Why.** The doctrine or current fact that makes it fit, with a primary source
link when the fact is unstable.

**Trade-offs and failure modes.** The strongest counterargument, an anti-pattern
to reject, and the signal that would change the recommendation.

**One question (only if material).** Ask the smallest question that separates
the remaining choices; otherwise state the assumption and stop.

For a simple decision, collapse this structure into one short paragraph or a
few bullets. Do not narrate research, list every considered alternative, or use
headings that make the answer longer without making the decision clearer.

Keep advice headless and language-agnostic. UI convenience cannot become the
source of truth, client code cannot own durable server behavior, and external
systems are read through `Source` rather than written by Wyrd.
