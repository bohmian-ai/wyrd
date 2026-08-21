---
name: wyrd-review-and-plan-v3
description: Run the final full static review of an immutable Wyrd implementation produced by wyrd-implement-plan-v3, against its approved intent, plan, task packets, and integrated code. Use as the last merge- and push-readiness gate after integrated verification; never for working-tree or per-task candidate review.
---

# Wyrd Review And Plan v3

Act as the independent terminal acceptance reviewer for one completed
`$wyrd-implement-plan-v3` execution. Review the entire immutable integrated
target, not merely its task reports. This is the last review gate before the
controller may declare the implementation ready to push.

Model the workflow after the global `$review-and-plan` full review: independent
specialist discovery, explicit coverage, adversarial probes, root source
validation, a durable candidate ledger, and one authoritative consolidated
review. Apply Wyrd authorities and the v3 remediation lifecycle described here.

Remain read-only. Never modify production source, review a working tree, or run
project tests, builds, formatters, linters, generators, migrations, servers, or
repository gates. Integrated verification belongs to the v3 controller and
task candidates. State in every report:
`Static analysis: no runtime verification performed.`

Use `.agents/model-routing.md`. Never invoke v1 or v2 implementation, review,
or planning workflows.

## Required input

Require:

- stable request ID and repository root;
- immutable execution baseline, integrated target, and merge-base SHAs;
- output directory outside the reviewed worktree;
- approved intent, plan, plan-review, and task-packet paths and digests;
- for every integrated task: task ID, packet digest, approved candidate SHA,
  integration commit, candidate manifest ref/digest, proof ref/digest, and
  `$wyrd-review-v3` artifact ref/digest;
- integrated-verification proof refs/digests from the controller.

Resolve every ref and digest before review. Reject a dirty or moving target,
unreadable authority, missing task identity, contradictory lineage, candidate
that was not approved, or integration commit that does not contribute the
recorded candidate. Never infer missing identity from branch names, chat, or
mutable controller state.

Read completely at the target snapshot:

- `AGENTS.md` and `architecture/agent-rules.md`;
- `architecture/wyrd-design.md` and `architecture/wyrd-doctrine.mdx`;
- the approved intent, plan, plan review, and every task packet;
- applicable manifests, ownership rules, generated-contract owners, and local
  architecture authorities;
- the complete committed diff and enough unchanged callers, consumers,
  contracts, tests, and precedents to evaluate it.

Use CodeGraph only when its index is proven to match `target_sha`; otherwise
use `git show` or a detached read-only worktree.

## Establish terminal review coverage

Create a review ID and persist this tree under the supplied output directory:

```text
<review-dir>/
├── review.md
└── evidence/
    ├── packet.md
    ├── coverage.json
    ├── ledger.json
    ├── validation.md
    └── specialists/
        └── <assignment>.md
```

`review.md` is the only authoritative verdict and remediation handoff. Evidence
records how the conclusion was reached; it must not be required to understand
a final finding.

Derive the impact graph from `base_sha..target_sha`: changed owners and
symbols, callers, consumers, state transitions, public projections,
persistence and lifecycle paths, tests and user journeys, manifests, generated
artifacts, deployment surfaces, and release gates. Map every plan requirement,
task acceptance criterion, changed production file, and high-risk boundary to
an assignment before dispatch.

Audit existing candidate, proof, review, and integrated-verification artifacts
semantically by content-addressed reference. They are evidence, not acceptance
authority for the integrated result. Do not copy their commands, logs, task
prose, or nested traces into terminal artifacts.

## Dispatch the full review roster

Dispatch seven independent baseline specialists, each with a distinct reviewer
identity and one primary domain:

1. `correctness` — behavior, lifecycle, performance, state transitions,
   concurrency, cancellation, recovery, cleanup, and failure handling.
2. `security` — authentication, authorization, tenant isolation, RLS, secrets,
   audit boundaries, stable errors, unsafe input, and abuse paths.
3. `code-quality` — Rust/Python/TypeScript idioms, local patterns, clarity,
   dependency use, error handling, and needless complexity.
4. `maintainability` — ownership, struct-centered Rust, cohesion, dependency
   cones, duplication, extensibility actually required by the task, and YAGNI.
5. `tests` — plan and acceptance traceability, assertion strength, negative and
   edge flows, user journeys, lane placement, and whether recorded proof binds
   to the reviewed commits.
6. `developer-experience` — public SDK, HTTP, MCP, CLI, UI, generated docs,
   errors, discoverability, and agent-facing usability.
7. `architecture-contracts` — Wyrd doctrine, owner boundaries, durable and
   public contracts, async/PyO3 boundaries, deployment topology, and contract
   projection parity.

Add separate triggered specialists for every applicable domain:

- persistence, SQL, storage, migrations, durability, replay, or idempotency;
- async, queues, external calls, timeouts, retries, shutdown, flush, or drain;
- PyO3, Python exports/stubs, Node bindings, or cross-language lifetime work;
- Vala, Bifrost, Arrow, DataFusion, Parquet, Iceberg, ingestion, or query work;
- Wyrd UI or browser-visible behavior;
- another repository specialist required by the impact graph or plan.

Triggered assignments add coverage; they never replace a baseline assignment.
Auth, tenancy, destructive persistence, migrations, concurrency, recovery, and
public wire contracts require two distinct reviewers. A changed user-facing
capability must be covered by the tests/user-journey specialist.

If required capability or reviewer independence is unavailable, persist the
coverage gap and return `REVIEW_BLOCKED`. Never combine or omit a baseline role
to fit capacity; dispatch in waves when needed.

Every specialist receives the same immutable review tuple and only its scoped
impact slice. Specialists remain read-only, write one evidence report, return
namespaced candidate IDs, and do not assign final IDs, plan, remediate, launch
other reviewers, or communicate with the user.

Each specialist report records assignment identity, inspected and sampled
coverage, applicable requirements and task criteria, at least one material
adversarial probe, candidate findings, clean evidence, and static limits. A
clean report must name the most dangerous relevant invariant challenged, the
strongest realistic counterexample attempted, and why it survived inspection.

## Validate and consolidate

The root reviewer—not the specialists and not prior task approval—is final
static-review authority. Preserve every specialist candidate in `ledger.json`.
For each candidate:

1. Re-read its cited source and complete owning symbol at `target_sha`.
2. Inspect affected callers, consumers, contracts, tests, and local precedent.
3. Test the claim against approved intent, plan, packet, Wyrd authority, and
   the actual integrated flow.
4. Attempt to disprove its reachability, consequence, and proposed closure.
5. Reject speculative, preference-only, duplicate, stale, working-tree-only,
   or unsupported claims with a concrete disposition.
6. Merge only candidates with the same root cause and required outcome.
7. Assign stable `REV-NNN` IDs only after validation and deduplication.

Before returning a clean verdict, independently challenge the three most
dangerous changed invariants across the integrated impact cone and record how
each survived, became a finding, or remains a static limit.

Classify each confirmed finding:

- `REVERSIBLE`: approved intent already determines a bounded implementation,
  test, generated-output, or evidence correction;
- `TASK_CONTRACT_REPAIR`: a mechanical task-packet field or proof obligation is
  defective without requiring a new material decision;
- `MATERIAL`: correction requires a new product, public or durable contract,
  owner, dependency, security, tenancy, audit, migration, or acceptance choice.

Every confirmed finding must stand alone and contain:

- severity, confidence, class, affected task IDs, owners, exact locations,
  authorities, requirements, source reviewers, and candidate IDs;
- a plain-language maintainer summary;
- the current flow, expected behavior, exact divergence, and source evidence;
- a concrete reachable failure scenario, consequence, blast radius,
  detectability, and recovery;
- the bounded required outcome, constraints, local precedent, and non-goals;
- observable acceptance assertions with stable assertion IDs;
- required verification command IDs and test tier/location;
- when structural Rust work is required, the natural concrete owner, composed
  state, inherent public/private methods, justified pure helpers, sync/async
  boundary, rustdoc closure, and forbidden shapes.

Do not replace these facts with a title restatement or evidence pointer.

## Authoritative output

Write `review.md` with:

1. review metadata and immutable identities;
2. executive assessment and explicit push-readiness conclusion;
3. scope, intent, plan, and task closure;
4. baseline and triggered lens coverage;
5. requirement and acceptance traceability;
6. confirmed findings;
7. follow-ups, deferrals, and static limits;
8. validation-ledger summary;
9. remediation or material-decision handoff.

Use exactly one terminal verdict:

- `CLEAN`: no confirmed findings, complete required coverage, valid lineage,
  and adequate prior proof/review bindings; the implementation is ready for
  the controller's final push handoff.
- `REMEDIATION_REQUIRED`: one or more `REVERSIBLE` or
  `TASK_CONTRACT_REPAIR` findings remain; the implementation is not ready to
  push.
- `MATERIAL_DECISION_REQUIRED`: at least one `MATERIAL` finding requires new
  authority; the implementation is not ready to push.
- `REVIEW_BLOCKED`: immutable identity, mandatory authority, evidence, roster,
  independence, or coverage is insufficient; make no push-readiness claim.

`coverage.json` records the full roster, reviewer identities, triggers, changed
files, requirements, high-risk boundaries, user-facing capabilities, and any
gaps. `ledger.json` records every candidate and its final disposition.
`validation.md` records the root source-validation decision for every
candidate. Validate the artifact structure with the global full-review
artifact and evidence validators when they are available; these are the only
commands this static-review skill may run.

## V3 remediation routing

Do not create a second implementation plan for ordinary terminal findings.
The v3 controller routes each `REVERSIBLE` remediation contract unchanged to
`$wyrd-implement-v3`, grouped only by actual owner and write overlap. Every
replacement candidate receives complete implementer verification,
`$wyrd-review-v3`, serial integration, integrated checks, and a fresh terminal
review of the new immutable target.

Route `TASK_CONTRACT_REPAIR` through the bounded v3 packet-repair and reapproval
path, then apply the same candidate lifecycle. Historical integrated task
identities remain immutable; remediation is additive from the current target.

Invoke `$wyrd-plan-v3` only for `MATERIAL` findings. Supply the consolidated
finding and exact committed identities, then stop before implementation. Never
invent a review-private plan or task format.

The final response returns the review ID, base/target/merge-base SHAs, verdict,
risk, `review.md` path, evidence directory, finding counts by class, and either
the remediation contracts or the material-plan path. Always repeat:
`Static analysis: no runtime verification performed.`
