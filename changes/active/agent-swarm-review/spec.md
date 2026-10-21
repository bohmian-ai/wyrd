---
id: SPEC-agent-swarm-review
revision: 2
status: draft
---

# Bounded Verifier step fan-out

## Objective and authority

Let teams run independent reviewers, exchange evidence between bounded review
rounds, and confirm findings inside one Verifier execution. Code review of a
change plus CI reports and test data is the motivating use case; the building
blocks apply to any Verifier preparation action. Multiple reviewers can improve
finding coverage, but Wyrd does not guarantee model independence or correctness
from a vote count.

This draft builds on the **approved revision 13** of
[long-horizon verification](../long-horizon-verification/spec.md). Its grouping,
`verifiers.<name>` references, `group_end`, source ordering/ceilings, accepted
outbox durability, and empty assertion `each` inconclusive semantics are fixed
dependencies. That change deliberately excludes internal step fan-out; this
separate change proposes it without revising that specification or making it
necessary for externally run swarms. The owner resolved OD-001 in favor of
Option A: mutually exclusive step `repeat` and `each`. The specification as a
whole remains draft pending explicit approval.

### Verified repository baseline

Links identify inspected current source, not claims that the proposed behavior
already exists.

| Existing owner/source | Verified behavior and missing capability |
|---|---|
| [`VerifierSpec::plan`, `Validator`](../../../crates/wyrd-spec/src/card/verifier/validate.rs), [`walk`](../../../crates/vala/vala-verify/src/walk.rs) | References and explicit dependencies form DAG stages. Independent runnable tasks in a stage already run concurrently; the stage barrier records settlements in declaration order. Reuse this workflow for round barriers. |
| [`HostedBlocks::engine`, `STEP_CONCURRENCY`](../../../crates/wyrd/wyrd-server/src/verification/hosted/blocks.rs) | Direct and queued executions share the hosted engine. Its hard-coded per-execution concurrency is four; it is not a per-step authored limit. |
| [`HostedTools::open`, `Invocation::call`](../../../crates/wyrd/wyrd-server/src/verification/hosted/tools.rs), [`Workspace::materialize`](../../../crates/wyrd/wyrd-server/src/verification/hosted/workspace.rs) | Step Agents receive declared evidence files in bounded workspaces and registered built-ins including `code.run`, `bifrost.query`, and filesystem tools. Each tool checks invoke and domain authority. Hosted tool registration does not install Agent delegation. |
| [`Validator::validate_reference`](../../../crates/wyrd-spec/src/card/verifier/validate.rs), [`Scope::resolve`, `select`](../../../crates/vala/vala-verify/src/blocks/scope.rs) | Validation allows `[*]` only in assertion `each`, although runtime selection already maps wildcard paths over JSON arrays. A whole upstream output array can already be bound without a wildcard. Runtime selection alone does not enable per-item actions. |
| [`Step`](../../../crates/wyrd-spec/src/card/verifier/mod.rs), [`VerificationEngine::attempt`](../../../crates/vala/vala-verify/src/blocks/engine.rs) | Step fields include action, dependencies, condition, requirements, timeout and retries; there is no step-level repeat, each, or max_parallel. Retryable action failures and per-attempt timeouts already use bounded step retries. |
| [`AgentDelegateTool::invoke`](../../../crates/skald/skald-agent/src/delegate.rs), [`DELEGATION_DEPTH_CAP`](../../../crates/skald/skald-agent/src/delegation.rs) | Skald has Agent-as-tool delegation with a depth cap of three. Its existence does not establish a hosted Verifier delegation contract. |
| [`GatewayInvocation::admit_call`, `admit`](../../../crates/wyrd/wyrd-server/src/components/gateway/invocation.rs) | Gateway calls authorize, route and commit admission through the existing gateway ledger before provider dispatch, including cost reservations. Reuse this per-call budget protection. |

The existing building-block journeys in
[Rust](../../../sdks/wyrd-sdk-rust/tests/integration/verifier_building_blocks.rs),
[Python](../../../sdks/wyrd-sdk-python/tests/integration/test_verifier_building_blocks.py),
and [TypeScript](../../../sdks/wyrd-sdk-ts/wyrd/tests/integration/verifier-building-blocks.test.ts)
already exercise real hosted steps, files, code/query tools, permission failures
and provider failures. Extend that complete registration → evidence upload →
execute/enqueue → result workflow; do not build a swarm service beside it.

## Scope and invariants

- **INV-001 — Generic, finite verification.** Expand preparation actions within
  an existing Run Verifier DAG. No new Card kind, swarm resource, execution
  service, aggregation language, loop, or recursive expansion. Window Verifiers
  retain their no-preparation-steps rule.
- **INV-002 — Accepted authority.** Every item inherits the execution's tenant,
  subject, accepted principal authority, exact Card references and route
  override for its parent step. Neither item data nor another reviewer's output
  grants permissions. Preserve tool/domain checks, gateway admission, bounded
  workspaces, evidence citation validation and non-blocking audit.
- **INV-003 — Existing lifecycle.** One Verifier execution/result contains the
  expanded work. Retain current direct deadlines, queued admission, leases,
  whole-execution retry/reclaim and ScribeOutbox retention/loss semantics.
  No child-execution queue, durable per-item checkpoints or exactly-once model
  calls. Portable macOS/Linux, x86_64/arm64 behavior remains mandatory.

Supported interactions are independent parallel actions; explicit DAG rounds
that broadcast the previous round's complete outputs; confirmation over a list
derived inside the execution; and existing code/agent aggregation plus assertion
`each`. An application-run swarm uses approved grouping with no new work here.

Non-goals: free-form peer-to-peer messaging between live agents; a
general-purpose multi-agent runtime; GitHub webhooks or external-event Triggers;
automatic PR retrieval, posting, merging or code mutation; new consensus/dedupe
operators; precise swarm-wide dollar/token guarantees; cluster-wide quotas or
strict fairness. Coordinator delegation through `AgentDelegateTool` is deferred:
hosted allowlisting, authority, routing, budgets and provenance need a separate
contract, so merely wiring the existing adapter is insufficient.

## Required behavior

### REQ-001 — Finite step expansion

Under owner-selected Option A, a preparation step may declare **one** of:

- `repeat: N`: a positive integer, running the same action N times with the
  same resolved evidence and configuration in independent invocation contexts.
- `each: <ref>[*]`: one action per JSON array element of a list produced by an
  earlier step in the same execution. The selector targets an upstream step
  output and has one terminal wildcard;
  preceding fields/indexes use existing local reference grammar. `item` and
  its ordinary field/index selectors name the current JSON element in that
  action's input/workspace bindings only. They are invalid outside an each
  action. No nested fan-out or cross-item references are added.

`repeat` and step `each` are mutually exclusive. Their absence retains one
action and the existing scalar output. An expanded step always returns a list,
including repeat=1 or a one-element source. Neither repeat=0 nor negative,
fractional or non-integer counts are valid. Known non-array each targets,
invalid item positions, unresolved references and cycles are registration
errors, as are each sources that do not reference an upstream step output;
runtime missing/non-array targets are missing evidence and make
dependent judgments inconclusive, never an empty success. Explicit null array
elements are items, not omitted elements.

Resolve an each list once after its dependencies settle; its membership and
order stay fixed for that step's item retries. Each item has a fresh agent
conversation and isolated scratch; siblings cannot inspect each other's live
state. Shared original evidence remains read-only. Repetition does not vary
prompts, models or seeds automatically. Different reviewer roles can already be
separate DAG steps, with explicit Agent/Prompt configurations.

### REQ-002 — Ordered outputs, barriers and judgments

`steps.<id>.output` for an expanded step is the ordered JSON list of item
outputs: repeat ordinal order, or original each-array order, regardless of
completion order. An action returning a list occupies one element; expansion
does not implicitly flatten it. Existing code steps may flatten or deduplicate.
An exhausted failed item keeps its position as null, with an inspectable
failure record distinguishing it from a successful JSON null output. Successful
siblings remain visible; a partial list must not masquerade as complete work.

The existing step result gains a bounded ordered per-item breakdown containing
ordinal, status, attempts, duration, reason and existing provenance/tool-call
evidence. Output values live in the step output list. Parent status is succeeded
only when all items succeed; any exhausted failure makes it failed. Existing
step requirements run once over the parent list after settlement, not as a new
per-item requirement language. Failure retains existing dependent-inconclusive
behavior; successful partial outputs are inspectable but do not silently feed
normal dependent actions as complete evidence.

A round is an ordinary expanded step. Each next-round agent receives the
entire prior round list through an explicit whole-output binding, e.g.
`steps.round1.output`. It starts only after that upstream step and its
requirements settle. No wildcard extension in ordinary action bindings is
needed for broadcast. Same-round outputs are unavailable until the barrier;
the author declares a finite number of rounds with ordinary dependencies.

Aggregation, quorum, ranking and deduplication remain authored code/agent steps
and existing requirements/`decide`. A confirmation Agent is a preparation
action producing JSON; tool-less Agent checks retain their existing Verdict
contract and validated original-evidence citations.

### REQ-003 — Empty lists and explicit completeness

An empty each source invokes no action, records output `[]` and a bounded
`empty_selection` reason, and makes attached/dependent judgments that require
its output inconclusive. It must not turn the skipped work into
`not_applicable` or a vacuous pass. A false parent `when` retains the existing
intentional-skip semantics and is evaluated once before expansion.

Preserve the approved rule that assertion `each` over zero values is
inconclusive/`empty_selection` everywhere. A workflow that accepts "no findings"
must explicitly judge successful reviewer coverage and the candidate count
through existing code/requirements, and intentionally gate confirmation when
there are no candidates. The gate permits the existing `not_applicable`
semantics; it does not redefine an executed empty each as success.

### REQ-004 — Bounded parallelism and cost

Replace the fixed hosted per-execution concurrency of four with a positive
deployment-configured active-block cap. A step may set positive integer
`max_parallel`; its effective parallelism is the smaller of that value and the
deployment cap. Omission uses the deployment cap, whose default remains four.
The cap also bounds the total active blocks within an execution, so several
independent expanded steps cannot each consume that cap simultaneously.
Existing process-wide runtime, sandbox, workspace and gateway admission still
bound aggregate load across executions. This is a maximum, not a promise that
all permitted work starts simultaneously. It is inspectable with the effective
step limit; lowering it changes scheduling, not output cardinality/order.

One positive deployment item ceiling bounds each expanded step, for repeat and
resolved each alike; exactly the ceiling is allowed. Validate static counts
before dispatch and dynamic list cardinality before starting any item of that
step. Over-limit work runs no partial prefix and yields an inconclusive
dependent judgment with reason `step_item_limit_exceeded`, identifying the step
and ceiling. Invalid authoring gets field-specific structured validation;
limits enforced during execution remain evidence/infrastructure outcomes, not
subject failures. Existing input/output byte bounds also apply; do not
materialize an unbounded list merely to discover overflow.

Every model call, including retries and later rounds, retains existing gateway
cost reservation/admission and usage settlement. Iteration, tool concurrency,
workspace, output, sandbox and deadline limits remain in force per invocation;
item ceilings and finite rounds bound multiplication. Budget refusal does not
become a negative finding or a successful vote. Do not introduce a second cost
ledger or imply that a per-call reservation guarantees completion of a swarm.

### REQ-005 — Item retries and cancellation

Apply the existing step `retries` policy and per-attempt `timeout_seconds` to
each item independently, within the existing execution deadline. Retry only a
retryable failed/timed-out item; successful siblings do not rerun because of
that retry. Retry attempts do not create extra list entries or extra votes.
Keep ordinal and evidence fixed and record all attempts in the item breakdown.
Existing capacity-refusal behavior does not consume action retries.

Cancellation/deadline stops scheduling remaining items and cancels active work
under the existing execution contract. Inspection distinguishes unstarted,
failed and successful work; no completed/passed full-swarm result is invented.
Whole-execution retry/reclaim may rerun successful items because no new durable
checkpoint guarantee is added; previously incurred provider cost is not undone.

### REQ-006 — One public contract and existing owners

`wyrd-spec` owns step fields, scoped item references, validation, schemas and
result projections. `vala-verify` extends its existing DAG walk, scope and step
retry/evaluation workflow. `wyrd-server` supplies deployment bounds and reuses
hosted tools/workspaces, accepted authority and gateway reservations. Skald
continues owning Agent/provider/tool primitives. No ownership move is needed.

Rust, Python and TypeScript expose the same authored step behavior and ordered
inspection through the shared `wyrd-client` and existing SDKs. HTTP and MCP use
existing Card registration, verification execute/enqueue and execution reads;
no swarm endpoints/tools or alternate execution workflow. Preserve ordinary
non-expanded Verifiers and result shapes apart from additive expansion data.
Generated types/schemas, served OpenAPI, runtime MCP catalogs, authored Python
stubs, CLI authoring/inspection and documentation must agree.

Before implementation lands, update the owning design/doctrine Verifier step,
reference and result descriptions for the approved OD-001 choice and bounded
parallelism. This draft does not authorize unrelated architecture edits or
changes to grouping, Bifrost serving, Triggers or Operator behavior.

## Acceptance criteria and user journeys

**AC-001 — Journey evidence (REQ-001–006; INV-001–003).** Run J-1 and J-2 through
each real Rust, Python and TypeScript SDK against WyrdTestServer plus managed
Postgres, hosted executors and storage. HTTP/MCP discover → register/invoke →
inspect variants prove the same contract and scoped writes. Deterministic local
provider responses may control findings and timing; replacing the execution
engine with an in-process stub is not journey evidence. J-3 reuses approved
grouping journeys as compatibility evidence and adds no grouping requirements.

### J-1 — Eight independent reviewers, consensus and confirmation

Surfaces: Rust, Python, TypeScript SDKs; direct HTTP and MCP verification tools.

1. Register/bind a Run Verifier; submit a pinned PR diff, CI report and relevant
   test data through existing evidence declarations/uploads. Enqueue eight
   identical independent Agent reviews with repeat=8, max_parallel=4.
   Each receives the declared files/tools and no sibling conversation.
2. A code aggregation step receives all eight ordered outputs, canonicalizes
   candidate identities and retains findings reported by at least two distinct
   reviewer ordinals. Two attempts of one reviewer count as one review.
3. A confirmation step runs each derived candidate against the original diff
   and submitted evidence, with `item` naming that candidate. Existing code
   and assertions judge reviewer completeness, confirmations and final policy.
4. Poll/inspect a completed result with eight item records, ordered outputs,
   candidate list, per-candidate confirmations and original-evidence lineage.
   A confirmed blocking defect yields failed; a complete clean review yields
   passed by explicit count/completeness logic. Both are ordinary results.

Edges/negatives: reversed completion order preserves ordinal order; one
retryable reviewer failure retries only that reviewer; exhausted failure is
inconclusive and cannot yield a reduced "eight-reviewer" consensus. Eight
clean outputs give no candidates: explicitly gate confirmation and pass the
clean-review requirement; a separate executed empty each is inconclusive.
At the ceiling all items run; ceiling+1 executes none of that expanded step.
Test max_parallel=1, a requested limit above the deployment cap, cancellation,
invalid counts/selectors/item references, missing CI evidence and malformed
Agent output. Deny verifier, tool, query-table and cross-tenant evidence access
in separate variants; no fallback gains authority. Gateway budget exhaustion
never becomes a confirmed finding or pass. Existing `bifrost.query` limits and
permissions are retained when the reviewer optionally reads recorded CI data.

### J-2 — Three bounded rounds of five reviewers

Surfaces: Rust, Python, TypeScript SDKs; HTTP/MCP registration, enqueue and reads.

1. Author three dependent review steps, five actions per round, each with the
   same pinned diff and CI report. Round 1 is independent review.
2. Every round-2 reviewer receives all five round-1 outputs; every round-3
   reviewer receives all five round-2 outputs. Explicit Prompt bindings request
   critique/reconsideration. Existing code/agent aggregation and requirements
   judge the final five outputs.
3. Observe exactly 15 logical reviewer items, three round barriers and the
   final result, with output ordering and provenance independent of finish time.

Edges/negatives: a slow reviewer prevents premature broadcast; max_parallel=2
still gives five outputs per round; siblings see no live messages. Retry stays
within its round/item; exhausted upstream failure makes dependent rounds
inconclusive instead of broadcasting a silently shortened list. Reject a
round dependency cycle. Deadline/cancellation and gateway exhaustion stop the
workflow under existing semantics, with no fourth round or hidden loop.

### J-3 — User-run swarm verified through group_id

Surfaces: Rust, Python and TypeScript SDK verify/submit, including a mixed-language
producer run; HTTP/MCP invocation and group execution inspection.

1. The user's own runtime runs reviewers. It verifies/submits each review and
   its declared change/CI evidence to an ordinary bound source Verifier with
   common `group_id`, under the same authenticated owner Service.
2. A group Verifier declares selected whole review inputs through
   `verifiers.<name>.inputs.<slot>` and, as needed, source requirement outcomes
   through `verifiers.<name>.requirements.<id>.outcome`. Its existing code/agent
   steps and assertions implement consensus.
3. Approved `group_end` discovery queues the judgment over the ordered visible
   snapshot. Inspect group_id, exact source identities, selected values and
   immutable result through the existing surfaces. Wyrd runs no reviewer swarm.

Edges/negatives retain approved behavior: enqueue replay adds no source identity;
wrong owner/tenant/group rows are excluded; absent outcomes retain aligned nulls;
late visible evidence may create a later serial judgment within finality.
An empty selected list yields assertion inconclusive/empty_selection, and the
existing source-execution ceiling yields inconclusive without truncation.
Flush/drain the positive fixture; submit staging is not durable membership,
and Bifrost lag/outbox loss remain accepted limits. No step expansion is needed.

**AC-002 — Supporting evidence (REQ-001–006).** Pure validation/scope/ordering
tests and engine tests cover invalid syntax, array/null/empty distinctions,
requirements evaluated once, cardinality limits, mixed independent steps sharing
the cap, per-item retries and incomplete-work propagation. Real hosted integration
evidence proves isolated conversations/scratch, unchanged tool authorization,
gateway reservation refusal, cancellation and ordinary-step compatibility.
Observed concurrency must never exceed configured bounds; no performance SLO
or new benchmark harness is introduced.

**AC-003 — Contract parity (REQ-006).** Source-driven schema/type drift checks,
Python typing/exports, served OpenAPI and runtime MCP tests prove the selected
syntax and result shape on every surface. Use existing capability/SDK and
sandbox lanes from [mise.toml](../../../mise.toml), including
`codegen:check`, `test:principals:integration` for served OpenAPI and owning MCP
tests. Follow AGENTS.md for focused commands when implementation tests are
named. These are future acceptance obligations, not verification claimed by
this draft.

## Resolved material decision

**OD-001 — Resolved by the owner: Option A.** A step may declare `repeat: N`
for N identical independent runs on the same input, yielding an ordered list,
or `each: <ref>[*]` for one run per element of a list produced earlier in the
same execution, with `item` naming the current element. The fields are mutually
exclusive. Repeat exposes no synthetic `item`; its actions receive identical
authored bindings. REQ-001–005 define ordering, scope, limits and retry behavior.

The selected surface expresses reviewer counts directly without synthetic lists
or preparation boilerplate. Both forms share bounded item execution semantics;
no range DSL is added. Option B (`each` only) is not selected.

## Open material decisions

None remain. Resolving OD-001 does not approve the specification as a whole.

## Revision history and authorities

| Revision | Status | Change |
|---|---|---|
| 1 | draft | Initial generic bounded step-expansion proposal, verified repository baseline, three user journeys, fixed grouping dependency and open OD-001. No tasks or implementation changes. |
| 2 | draft | Owner resolved OD-001: Option A, mutually exclusive repeat/each with ordered outputs and scoped item binding. Each consumes a list produced by an earlier step in the same execution. Removed tentative syntax wording; no open material decisions remain. Overall specification approval remains outstanding. |

Authorities: [AGENTS.md](../../../AGENTS.md),
[agent rules](../../../AGENTS.md),
[Wyrd design](../../../architecture/wyrd-design.md#verifier),
[doctrine](../../../architecture/wyrd-design.md),
[approved grouping revision 13](../long-horizon-verification/spec.md),
[reference router](../../../architecture/wyrd-design.md),
[evaluation](../../../.agents/skills/wyrd-advise/references/evaluation.md),
[architecture patterns](../../../architecture/wyrd-design.md),
[agent harness](../../../agent_docs/agent-surfaces.md),
[spec workflow](../../../changes/spec-driven-development.md).
