# TASK-001 R1 remediation review scope

Repository: /home/thorrester/Documents/GitHub/wyrd-worktrees/verifier-code-mode.
Original base: 5234961d51798581a026834cfffd5efaadc226b4.
Prior candidate: 0e51af2a2332f362ab44b7c5b88b51c3cb4f8011.
Runtime candidate: 0393740405bb47ab95ff1a404a20be44597fbdeb.
Cumulative reviewed candidate/HEAD: f8b45a7208dbc3e8ec6c3cdac3605f6f84b6cc18.
The last commit records the governing packet/evidence only; source is identical
to runtime candidate. Review range: git diff <prior-candidate> <cumulative-candidate>.
Cumulative original-base range remains acceptance context.
Authority: revision 2 spec from candidate commit (sha256
cc1b3c37d28201f3207769901776cc34b6a73c2810e9c42c53b50c2033cd4055),
original TASK-001, first-review verdict/ledger, TASK-001-R1 with appended evidence.
Local benchmark spec revision 3 and TASK-003 are pre-existing uncommitted planning
changes, excluded from this runtime review. Read revision 2 using git show or
its exact preserved local spec-revisions/revision-2.md snapshot. Do not change
source, existing evidence, local planning, index or HEAD. Write reports only here.

## Scope and carry-forward

Check closure of findings 1–6 and 8–11, remediation diff and affected callers,
consumers and proofs. ID 7 remains withdrawn; CodeTool is accepted. Carry
forward the first review's unchanged authority/tenancy, catalog, budget arithmetic,
public reduction, historical/additive contracts, privacy, portability/config,
normal workspace semantics and governing-document rows with their source-backed
reports. Reopen only where fixes affect those paths or specific new evidence
establishes a material defect. This is the first remediation review, not a full
discovery restart. No target count of findings; no general optimization.

## Shared navigation map

- Vala blocks/tools.rs: ToolSession::with_settlement/close, Halt::observe,
  Dispatch::call/permit, Delivery, hold_permit; records, permits, task-local
  active-operation retention and publication boundary.
- Vala blocks/agent.rs: AgentExecutor session close versus engine timeout/drop;
  blocks engine and tool sibling callers available for affected flow tracing.
- Server verification/hosted/tools.rs: Invocation code/extract/blocking,
  host_stop, settlement binding; workspace.rs owned writer/admission.
- Server hosted/sandbox.rs: worker resumption/delivery, answer, integer conversion.
- Vala integration/blocks_agent.rs: admitted waiters, blocking permit retention,
  bridge delivery, exact Oracle QueueFull versus ordinary resources exhaustion.
- Server hosted/tools/pg_tests.rs: extraction settlement and held-writer deadline.
- Server integration/pg_hosted_artifacts.rs: real bridge, capacity recovery,
  deadline/cancellation/queue composition and mounts.
- Rust SDK integration/verifier_building_blocks.rs: failed step/inconclusive
  check/no extra model request and fresh-session proof.
- First-review verdict.md / findings-validation.md / standards-review.md /
  domain-review-*.md / system-review.md supply accepted rows and prior traces.
- R1 appended implementation evidence supplies commands and claimed proof;
  assertions and source must be checked, not accepted from summary.

## Contract coverage and reviewer selection (before discovery)

| Contract/invariant; owners/producers/consumers | Selected coverage / omission rationale |
| --- | --- |
| Original finding closure, delivered values/records, integer domain, mandatory changed-item rustdoc and proof relevance | Fresh ponytail discovery reviews all 10 closures and affected consumers. Existing standards evidence carries unchanged cross-tier fields/generated/docs. No new schemas/SDK contracts/dependencies; no broad repo specialist required. |
| Shared lifecycle state, settlement, writer/workspace/admission and record publication | Fresh invariant-rev traces ownership through Invocation/TaskTracker/ToolSession/AgentExecutor and real worker delivery. New settlement uses existing owners; review whole-workflow reuse here and in closure review. |
| Sensitive concurrency: request budget, semaphore handoff, active blocking work and cancellation races | Fresh domain concurrency reviewer traces shared state and sibling dispatch; material changes demand dedicated boundary coverage. |
| Failure propagation/recovery: uncaught generated error, deadlines, engine timeout/cancellation, exact Oracle queue-full and queued deferral | Fresh system-rev traces request/session/engine and existing queue consumer. In-process timeout/drop and detached blocking work are explicitly in scope. |
| Security/tenancy and durable result contract | Carry first-review security/records coverage; no new grants, tenant selectors, SQL/storage/public contracts. Closure/invariant reviewers inspect changed safe-error/delivery consumers for regression; no independent security/data specialist unless source identifies a material boundary change. |
| Maintainability/ownership | Existing ToolSession/Invocation/Workspace extended, with no new runtime/service/store/transport. Closure and invariant reviewers must compare installed TaskTracker/permit/delivery mechanisms against existing owners and inspect changed docs. No substantial cross-owner replacement; separate maintainer reviewer omitted. |

Selected reports: task-review-closure.md, task-review-invariants.md,
domain-review-concurrency.md, system-review.md. After discovery, a distinct fresh
ponytail validator writes findings-validation.md and independently validates
selection/coverage and every proposal. A validator-discovered blocker needs
separate focused source validation. Follow-up only for material unresolved paths.

No source test changes or broad gates are authorized by review. Reviewers may run
focused tests with shared build resources coordinated by orchestrator. Report
unrun checks as evidence limits. Final candidate must remain unchanged.
