# Full Sweep Architecture Review

Use this workflow for folder-scale Wyrd architecture reviews: multi-file phase
plans, security/auth designs, storage/server/runtime designs, Vala/Skald/Wyrd
integration plans, or any proposal where a single generic pass would hide
cross-file contradictions.

Full sweep mode produces a reviewed packet, not a pile of independent file
reviews. Split by concern, then aggregate and validate.

Subagent delegation is optional. Use subagents only when the user explicitly
requests delegated specialist reviewers and the active tool policy allows it.
Otherwise, run the specialist lenses locally and sequentially while preserving
the same intake-ledger and consensus-ledger outputs.

## Required Core Context

Every full sweep reviewer must read:

- `AGENTS.md` from the active Wyrd repo being reviewed
- the active Wyrd design authority named by that repo
- `references/review-rubric.md`
- `references/positioning-and-vocabulary.md`
- `references/architecture-constraints.md`

Load specialist references by concern:

- doctrine/vocabulary: `positioning-and-vocabulary.md`,
  `architecture-constraints.md`, `source-map.md`
- security/tenant/audit/production: `production-architecture-rubric.md`,
  `implementation-rules.md`, `rust-service-architecture.md`
- implementation feasibility: `implementation-rules.md`,
  `rust-service-architecture.md`
- public surfaces and agent/developer experience: `agent-first-review.md`,
  `surface-mapping.md`
- verification, migration, and predecessor parity: `source-map.md`,
  `codebase-map.md`, `surface-mapping.md`, `implementation-rules.md`
- Vala, observations, traces, eval/drift, OLAP, Bifrost, Iceberg, Arrow,
  DataFusion, object-store analytical storage, or Parquet:
  `observability-otel.md` and `olap-datafusion-iceberg-arrow.md`

The aggregator must also read the core references and any specialist reference
needed to validate disputed findings.

## Intake Ledger

Create an intake ledger before reporting findings. When the target is a
filesystem path and the workspace is writable, write it to:

`.dev/review/architecture/{REVIEW_ID}/intake-ledger.md`

Use this shape:

```markdown
# Architecture Review Intake Ledger

**Review ID**: {REVIEW_ID}
**Target**: {path or proposal}
**Review mode**: full sweep

## Files In Scope

| File | Lines | Role | Risk notes |
|---|---:|---|---|

## Packet Summary

- Planning surface:
- Doctrine layer(s):
- Affected crates/services:
- Affected public surfaces:
- Durable contracts changed:
- Explicit out-of-scope items:

## Locked Decisions And Invariants

- {file/line or lock id}: {decision}

## Reference Routing

| Lens | Required references | Reason |
|---|---|---|

## Initial Risk Areas

- {cross-file issue, high-risk file, security boundary, migration edge, etc.}
```

If the workspace is not writable or the review is chat-only, include the same
ledger inline. The intake ledger is a map, not a verdict. It should be factual
and compact.

## Specialist Lenses

Run each lens against the full packet and the intake ledger. Do not assign
files exclusively to one reviewer; cross-file consistency is the point.

### Doctrine And Vocabulary

Check core nouns, Card/Spec/Run/Observation fit, envelope, CardRef,
relationship/status ownership, service boundaries, forbidden predecessor
vocabulary, locked decision drift, and whether any new noun or public shape is
actually necessary.

### Security, Tenant, Audit, And Production

Check authn/authz boundaries, tenant/space isolation, service identity, token
or secret handling, policy and audit requirements, redaction, cache keys,
background jobs, retries, idempotency, failure modes, operational metrics, and
whether production claims have concrete targets and gates.

### Implementation Feasibility

Check crate ownership, dependency direction, PyO3-free boundaries, async and
blocking placement, SQL/cloud/DataFusion placement, public error codes,
generated artifacts, testability, and whether the plan names enough concrete
files, types, methods, and commands to implement without redesign.

### Public Surfaces And Agent DX

Check HTTP, Python, CLI, MCP, UI, docs, generated schemas, examples, stable
errors, idempotency, permission discovery, and whether a smaller literal agent
can understand allowed operations, side effects, recovery steps, and next
actions without hidden session context.

### Verification, Migration, And Parity

Check that verification gates prove the locked decisions, migrations are safe
for the stated data state, predecessor behavior is reused/adapted/rejected with
evidence, and grep/codegen/test gates are precise enough to prevent drift
without becoming brittle ceremony.

### Domain-Specific Lenses

Run only when relevant:

- Vala/observability: traces, observations, eval/drift, OTel, redaction,
  cardinality, correlation, and observation/run alignment.
- OLAP/Iceberg/DataFusion/Arrow: Bifrost placement, catalog boundaries,
  pushdown, partitioning, object-store layout, schema evolution, query
  governance, compaction, snapshot expiration, recovery, and retention.
- Skald/runtime: provider boundaries, prompt/runtime ownership, tool registry,
  workflow/agent semantics, and provider-specific wire handling.

## Right-Sized Architecture Pass

Always run this pass for full sweeps. Ask:

- Is there a smaller doctrine-compatible design that solves the same user
  workflow?
- Can an existing Wyrd foundation, service, public surface, or predecessor
  pattern replace a new abstraction?
- Is a new trait, crate, registry, compatibility layer, route family, helper,
  generated artifact, or state machine justified by current requirements?
- Is the PR or phase too large to review and implement safely?
- Is the plan implementing future or enterprise behavior instead of deferring
  it behind a named boundary?
- Can cross-file duplication collapse into one source of truth?

Report simplification only when the alternative is concrete,
behavior-preserving for the stated workflow, doctrine-compatible, and does not
weaken best practices, industry standards, security, tenant isolation,
auditability, reliability, observability, error quality, agent-first usability,
or verification.

## Consensus Ledger

After specialist reviews, validate and deduplicate findings. When the target is
a filesystem path and the workspace is writable, write:

`.dev/review/architecture/{REVIEW_ID}/consensus-ledger.md`

If the workspace is not writable or the review is chat-only, include the same
ledger inline. Use this shape:

```markdown
# Architecture Review Consensus Ledger

**Review ID**: {REVIEW_ID}
**Target**: {path or proposal}
**Decision**: approve | approve with changes | needs redesign | reopen locked decision

## Confirmed Findings

| ID | Severity | Lens | Location | Issue | Impact | Required change |
|---|---|---|---|---|---|---|

## Simplification Opportunities

| ID | Location | Current shape | Simpler safe shape | Why it preserves doctrine/standards |
|---|---|---|---|---|

## Merged Duplicates

| Duplicate IDs | Merged into | Reason |
|---|---|---|

## Eliminated Findings

| Finding | Source lens | Reason eliminated |
|---|---|---|

## Open Decision Points

- {question}: why it matters, who must decide, and what changes after decision

## Verification Required

- {gate}: what it proves and which finding/lock it covers
```

Aggregation rules:

- Merge duplicate findings only when they share the same root cause, doctrine
  rule, owner, impact, and required change.
- Keep separate findings when severity, affected contract, service boundary,
  public surface, implementation owner, or verification gate differs.
- Eliminate findings that are unsupported by the packet, contradict loaded
  references, are preference-only, or ask for complexity that the current
  workflow does not need.
- Do not let subreviewers independently decide doctrine. The consensus ledger
  is authoritative.

## Final Response

Lead with the consensus result. Include:

- review mode and target
- intake ledger path or inline summary
- consensus ledger path or inline summary
- confirmed finding count by severity
- simplification opportunity count
- eliminated finding count
- final decision

Keep raw specialist notes out of the final response unless the user asks for
them.
