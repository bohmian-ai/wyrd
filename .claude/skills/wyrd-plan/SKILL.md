---
name: wyrd-plan
description: Create an actionable Wyrd implementation plan with source-grounded decisions, cohesive tasks, claim-shaped acceptance obligations, and credible evidence expectations. Use when implementation should be decomposed without executing it; task shape is flexible and scales with risk.
---

# Wyrd Plan

Turn a Wyrd request into work an implementer can execute without reopening a
material product, contract, ownership, security, tenancy, migration, or
acceptance decision. Optimize for decision clarity and credible proof, not
packet conformity, task count, or executor utilization.

## Investigate what determines the plan

Read `AGENTS.md`, `architecture/agent-rules.md`, and the repository owners of
the requested behavior. Read `architecture/wyrd-design.md` and
`architecture/wyrd-doctrine.mdx` when the work changes governed behavior or a
contract. Follow CodeGraph instructions, inspect the nearest implementation and
tests, trace material consumers, and check manifests and relevant `mise` tasks.

Resolve ordinary implementation choices from repository evidence. Ask the user
only when a choice changes material behavior, a public or durable contract,
ownership, security, tenancy, migration, rollout, or the acceptance outcome.
Bounded read-only evidence scouts are useful for genuinely independent evidence
gaps; they gather facts and do not decide or write the plan.

### Architecture and expertise routing

Start at [the canonical reference router](../../../architecture/references/README.md).
Read [implementation execution](../../../architecture/references/languages/implementation-execution.md)
for every implementation plan, then select and completely read the smallest
additional set that covers the affected surfaces. Record the selected links in
the plan or applicable task packets and state the decision or constraint each
reference informs. Do not attach irrelevant references merely to fill a field.

| Reference | Select when the plan touches |
|---|---|
| [Wyrd protocol authority](../../../architecture/wyrd-design.md) | Card contracts, identity, doctrine, cross-service boundaries, or public surfaces |
| [Bifrost authority](../../../architecture/bifrost-design.md) | Scribe, Oracle, Forge, analytical storage, resources, or public query/ingest behavior |
| [Security posture](../../../architecture/wyrd-security-posture.md) | Authentication, authorization, credentials, tenant security, audit integrity, or external-network trust |
| [Operations authority](../../../architecture/operations/README.md) | Deployment, release, capacity, backup, recovery, SLOs, or incidents |
| [Positioning and vocabulary](../../../architecture/references/doctrine/positioning-and-vocabulary.md) | Card vocabulary, envelope, `CardRef`, v1 kinds, or removed concepts |
| [Architecture constraints](../../../architecture/references/doctrine/architecture-constraints.md) | Wyrd/Vala/Skald boundaries, deployment, tenant isolation, or observation identity |
| [Architecture patterns](../../../architecture/references/architecture/patterns.md) | Ownership, contract placement, or server/client/storage/provider/audit structure |
| [Implementation execution](../../../architecture/references/languages/implementation-execution.md) | Every plan: execution authority, adaptation, verification recovery, and completion evidence |
| [Rust core](../../../architecture/references/languages/rust-core.md) | Rust ownership, async, traits, allocation, or API shape |
| [PyO3 boundaries](../../../architecture/references/languages/pyo3-boundaries.md) | PyO3 classes, GIL, lifetimes, conversion, or module registration |
| [Python API and stubs](../../../architecture/references/languages/python-api-and-stubs.md) | Python exports, stubs, package layout, or typing |
| [TypeScript guide](../../../architecture/references/languages/typescript-guide.md) | TypeScript SDK, declarations, or napi boundaries |
| [Testing workflows](../../../architecture/references/languages/testing-workflows.md) | User journeys, integration tests, unit tests, or repository verification |
| [Agent harness](../../../architecture/references/languages/agent-harness.md) | MCP, agent-facing contracts, structured validation, or audit |
| [Errors](../../../architecture/references/languages/errors.md) | Stable errors and Rust/Python/TypeScript/HTTP/CLI mapping |
| [Vala architecture](../../../architecture/references/domain/vala-architecture.md) | Broad Vala ownership, Bifrost orientation, or cross-domain work |
| [Telemetry observations](../../../architecture/references/domain/telemetry-observations.md) | OpenTelemetry signals, correlation, observation identity, or payload sensitivity |
| [Evaluation](../../../architecture/references/domain/evaluation.md) | Eval Cards, scenarios, judge quality, scoring, or evidence |
| [Drift monitoring](../../../architecture/references/domain/drift-monitoring.md) | Drift signals, baselines, thresholds, alert noise, or monitoring policy |
| [OLAP serving](../../../architecture/references/domain/olap-serving.md) | Bifrost tables, ingest/query serving, admission, tenant safety, or analytical APIs |
| [Iceberg](../../../architecture/references/domain/iceberg.md) | Snapshots, catalogs, schemas, partitions, object storage, or compaction |
| [DataFusion](../../../architecture/references/domain/datafusion.md) | Logical/physical plans, provider pushdown, pruning, statistics, memory, or spills |
| [Arrow analytical interop](../../../architecture/references/domain/arrow-analytical-interop.md) | Arrow, RecordBatch, Parquet, PyArrow, FFI, or Python analytical boundaries |
| [Analytical operations reliability](../../../architecture/references/domain/analytical-operations-reliability.md) | Backpressure, durability, leases, repair, retention, SLOs, or failure recovery |

## Model obligations as claims

Use the proposed Wyrd Change trust model as the semantic guide without requiring
the Change service to exist or treating planning as a durable Change mutation.

A claim is an explainable obligation with a stable local ID, requirement,
rationale, disposition, and acceptable evidence. Evidence classes remain
orthogonal:

- `CustomerDeterministic`
- `PlatformDeterministic`
- `LlmEvaluation`
- `HumanAttestation`

Do not collapse them into a trust score or let one class silently replace a
required class. In an approved task, blocking obligations are `required`;
nonblocking work may be `optional`. Use `proposed` only for an explicitly
unresolved planning item and `rejected` only when retaining that decision record
is useful. A plan is not ready while a material required outcome remains merely
proposed.

Planning defines claims and their evidence expectations. It does not verify,
authorize, merge, promote, or finalize a Wyrd `EvidenceManifest`.

## Write a proportionate plan

For multi-task work, prefer a concise `plan.md` containing:

1. Objective and user value.
2. Source-grounded current state and constraints, including the inspected
   revision and relevant working-tree assumptions.
3. Locked material decisions and any unresolved material question.
4. Task inventory with primary outcome, owners, and direct dependencies.
5. Cross-task producer/consumer contracts.
6. Overall claims, integrated evidence, and progressive verification.
7. Initial tasks whose actual dependencies are already satisfied.

Use separate task packets when they add independent execution value or preserve
a material authority boundary. A single actionable plan is valid when further
decomposition would add only handoff overhead. Add a dependency only when the
successor needs the predecessor's integrated contract or behavior. Keep a
shared mutable seam cohesive; otherwise allow independent consumers of a fixed
contract to proceed independently. Stages may describe product milestones but
must not become fixed execution waves.

If the caller supplies a plan directory, write authoritative artifacts there.
Do not place planning authority in `.git`, controller state, or caches. Digests,
review records, and source SHAs may be recorded when useful, but are not
universal validity requirements.

## Preferred task packet shape

Use these sections when they improve execution; combine or omit them for smaller
tasks. Equivalent Markdown, YAML, tables, or supplied prose are valid.

1. **Task contract** — ID, primary outcome, owners, dependencies, optional
   write forecast, claims, evidence plan, and linked architecture/expertise
   references with the decision each informs.
2. **Outcome** — observable result and user/operator value.
3. **Context and authority** — only evidence that constrains implementation.
4. **Scope and non-goals** — owned behavior and intentional exclusions.
5. **Claims and evidence** — atomic, falsifiable obligations and direct proof.
6. **Design and invariants** — material ownership, state, IO, error, security,
   durability, and contract decisions; leave private mechanics open.
7. **Implementation guidance** — dependency-significant order and repository
   precedents, not prescribed helper names.
8. **Consumers and integration** — callers, projections, generated surfaces,
   journeys, and downstream claims.
9. **Verification** — diagnostics, direct claim evidence, and integrated
   evidence.
10. **Stop and escalate if** — material decisions or missing authority only.

For cross-boundary, identity, concurrency, tenancy, audit, durability,
migration, or parity work, an optional matrix is often clearer:

| Concern | Invariant | Owner | Required evidence |
|---|---|---|---|

Write exact existing paths, symbols, and tests when known. For proposed symbols
or tests, name the intended owner and what they prove. Avoid placeholders such
as “wire it up,” “as needed,” or “add tests.” Do not prescribe local refactors
when multiple repository-native implementations satisfy the claims.

## Evidence plan

Separate three proof roles:

- `diagnostics`: fast implementation feedback; never acceptance evidence alone;
- `claim_evidence`: direct proof for one or more required claims; and
- `integrated_evidence`: cross-task, journey, or system proof that cannot be
  established by one task.

Allow as many focused commands as the owned boundaries require. Prefer current
canonical `mise` commands and the smallest proof that can fail for the claimed
defect. User- or agent-facing behavior receives the journeys required by
`AGENTS.md`. Broad gates belong only where the plan's breadth earns them.

When exact command text is not knowable during planning, state the proof
obligation and owner rather than inventing a selector. Exact command identity is
binding only when it is itself part of the accepted contract.

## Readiness rule

A task is executable when its outcome, material constraints and decisions,
required claims, and credible evidence are clear from the task plus repository
authority. Missing preferred headings, YAML keys, claim IDs, digests, exact
command shapes, write forecasts, or planner-specific formatting is not a reason
to refuse implementation. Consumers normalize minor omissions and escalate
only when a material decision or acceptance outcome is genuinely unresolved.

Before handoff, confirm every requested outcome is covered, each material
contract has an owner and consumers, dependencies are real, required claims are
collectively sufficient, evidence can detect failure, and integrated journeys
cover user-facing behavior. Use `$wyrd-plan-review` when the caller requests
review or when independent readiness review is proportionate to risk. Correct
bounded findings without turning the review format into plan authority.
