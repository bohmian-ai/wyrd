# Review Orchestration Contract

Use this contract for the full Wyrd v3 terminal review. It separates the
question being reviewed from the capability and concrete agent that performs
it.

## Assignment identity

Every review assignment records:

- `assignment_id`: stable ID such as `baseline-security`;
- `domain`: the review question;
- `requirement`: `baseline` or `triggered`;
- `required_capability`: engine-neutral specialist capability;
- `actual_agent_role`: concrete platform role;
- `reviewer_id`: unique agent/task identity;
- prompt path and SHA-256 digest;
- resolved target SHA;
- assigned paths, symbols, boundaries, requirements, and trigger IDs;
- report path and completion status.

A domain, capability, role, and reviewer instance are different concepts. Two
lenses performed by one reviewer are not independent. A report filename does
not prove specialist dispatch.

## Baseline domains

The terminal review always covers all seven domains:

Resolve the concrete role through `role-map.json`; the table below is the
human-readable contract and must remain synchronized with that file.

| Domain | Required capability | Required role |
|---|---|---|
| `correctness` | `correctness-specialist` | `code-reviewer` |
| `security` | `security-specialist` | `security-auditor` |
| `code-quality` | `quality-specialist` | `code-reviewer` |
| `maintainability` | `maintainability-specialist` | `pedantic-maintainer` |
| `tests` | `test-specialist` | `test-engineer` |
| `developer-experience` | `dx-specialist` | `code-reviewer` |
| `architecture-contracts` | `architecture-specialist` | `code-reviewer` |

Dispatch each row to a separate agent instance. Never combine
baseline domains. A narrow assignment may return clean or not-applicable
evidence after inspection; it may not be skipped. If a required role cannot be
dispatched, return `REVIEW_BLOCKED` without a merge-readiness verdict. If a role
override conflicts with full-history forking, use a context-light fork with the
complete immutable packet. Never silently remove the role.

Reviewer independence does not require simultaneous execution. When the root
orchestrator plus reviewers exceed the harness's active-agent capacity, dispatch
the roster in waves and reuse released slots. Every assignment still receives
a distinct reviewer instance and reviewer ID.

## Triggered domains

Triggered assignments add coverage and never replace a baseline assignment:

- `persistence-storage`: SQL, migrations, durable state, databases, object
  storage, deletion, retention, compaction, replay, idempotency, tenant-keyed
  persistence, or database/object-store coordination;
- `async-reliability`: async functions, concurrency primitives, queues,
  background work, external calls, retries, timeouts, shutdown, flush, drain,
  or backpressure;
- `pyo3-cross-language`: PyO3, native bindings, Python exports or stubs,
  cross-language types, GIL behavior, or runtime bridging;
- `vala-data-plane`: Vala, Bifrost, DataFusion, Arrow, Parquet, Iceberg,
  analytical ingestion, query, or archival behavior;
- `wyrd-ui`: frontend paths or browser-visible workflows;
- repository specialists: candidate-producing specialists explicitly declared
  by target-snapshot review policy.

Record every trigger with matched paths or symbols. Unknown production scope
is a coverage gap. Triggered assignments use `code-reviewer` unless the
engine's installed role map declares a narrower supported role.

Repository integration skills that assign verdicts or invoke planning are
orchestrator policy. They never satisfy baseline or triggered assignments.

## Independence and validation

Independence requires distinct `reviewer_id` values. Critical boundaries may
require multiple assignments, but different domain names or prompt files from
one reviewer do not satisfy that floor.

Independent validation uses a reviewer that produced no candidate report. It
audits source decisions, deduplication, baseline roster completeness, role and
capability compliance, trigger completeness, and reviewer independence.

## Finding information contract

Every candidate and final finding must make these five blocks explicit:

1. `Maintainer summary`: 2-4 plain-language sentences naming the current
   behavior, consequence, and correction without relying on other artifacts;
2. `Problem`: the single root cause, expected behavior, exact divergence, and
   source/caller/contract/test proof;
3. `Failure scenario`: a concrete entry point and input/state sequence,
   observable result, affected parties, blast radius, detectability, and
   recovery;
4. `Recommended correction`: the named natural owner and symbols, ordered
   behavioral changes, constraints, precedent, and explicit non-goals;
5. `Verification`: named test tier/location, setup, action, exact assertions,
   and focused static closure inspection.

These blocks are the maintainer handoff, not placeholders around an evidence
link. Each must contain finding-specific facts. Never substitute the title,
location list, specialist report, or generic repository invariants for the
explanation.

Deduplication merges only candidates with the same root cause. Preserve every
candidate ID, assignment ID, reviewer ID, material location, distinct impact,
and dissent. The ledger links specialist evidence to validation and final
findings; the final finding links to canonical planning requirements and tasks.
