Review ID: skald-workflow-runtime-revision12
Target: changes/active/skald-workflow-runtime/spec.md Revision 12 and active task packet
Gate: Plan
Verdict: Ready
Architecture axis: PASS
Executability axis: PASS
Critical: 0
Major: 0
Task coverage: All implementation tasks
Rehearsal: PASS

## Evidence and scope

Reviewed user-approved Revision 12, TASK-001 carry-forward, TASK-002-cleanup, TASK-003–005, and README. Original TASK-002 is superseded history. Inspected changed repo-local workflow skills, Claude copies, execution/specification references, installed wyrd-implement-plan and references, design/doctrine, AGENTS.md, agent rules, security and Bifrost authority. No .codegraph directory exists; discovery used rg and direct reads. Plan-review rubric, execution-readiness, production-risk and public-workflow references governed completeness, severity, containment and SDK proof.

Source evidence: existing wyrd-loader load/projection; CardGraphHydrator/GraphTraversal/resolve_graph; Skald workflow_surface, ResolvedGraph and execution dependencies; server EffectiveSpecs/composite registration/pg_workflow_registration; SDK manifests, Python workflow/state and Cards CRUD integration, TS native/public integration, Rust verification_run; CLI manifest; mise.toml and family scripts.

Fresh independent [cold rehearsal](cold.md), agent /root/revision12_readiness/cold, and [system perspective](system.md), agent /root/revision12_readiness/system, were validated against source by the reviewer.

## Findings

No blocking findings.

Resolved during this review: R12-001 (Major INVALID_VERIFICATION_RECIPE) initially found cleanup misattributing UID-race assertions to the ordinary registration test and omitting the exact replacement-race command. Root corrected attribution and added the separate Postgres-wrapped nextest selector for existing refuses_stale_preflight_after_dependency_replacement. Reviewer verified source ownership at pg_workflow_registration.rs:873 and parsed every active task's inline mise recipe with bash -n. The repaired recipe now has valid quoting. Its runtime result remains implementation regression proof. Original independent discovery is retained in cold.md with re-review disposition.

## Architecture and execution readiness

Architecture PASS. Existing loader/graph/Skald owners remain authoritative. Thin shared Workflow facade earns the client IO boundary: Rust cannot add inherent methods to a foreign Skald type and Skald cannot depend on registry IO. It owns no duplicate parser/graph/validator/executor. Cards typed view reuses connection context; WyrdState remains Service-rooted; Workflow Card identity creates no principal or transferred authority. Server registration remains tenant SQL-owned with synchronous Skald validation, never client HTTP loading.

Fresh system pass traced author process → lazy authenticated refs → complete Workflow; compiled apply → effective bodies → declarative validation → audited UID-fenced write; registered loading → shared execution. Missing/denied refs fail the operation before dispatch, without shared-process failure. Loading cancellation publishes no partial Workflow and writes nothing. Secrets resolve only for selected execution routes; Cards-loaded gateway calls retain that connection.

Accepted remote jobs have process-local affinity and honest restart loss. Tracked preparation owns reservation and survives disconnected waiters; failure releases capacity. Captured scopes retain no bearer and replay cannot widen authority; live gateway admission remains required. Tracked read-tool response/settlement survives waiter abort and joins under original deadlines before capacity release; loss reports honest unconfirmed cleanup. BoundServer uses one shutdown deadline and closes Workflow before needed gateway/query owners. Graph resource proof requires sibling Cards/gateway/Bifrost responsiveness. No additional system finding was validated.

## Adversarial analysis

WyrdState accessors do not establish standalone Workflow-root semantics; keeping Service-root behavior avoids a new contract. CardGraphHydrator currently couples inventory/bundle publication; tasks explicitly extend its owner for in-memory authored/registered loading without artifact work and retain disk consumers. A facade is necessary projection, not permission to create replacement graph state. EffectiveSpecs owns distinct sibling/external caches; cleanup deletes its WorkflowGraph-driven missing-body orchestration while preserving resolution/validation/UID fence. That fence's omitted proof produced R12-001, now resolved by the explicit focused regression command.

Changed skills now require inspected whole-workflow reuse, implementer revalidation, independent necessity review and integrated consumer closure. Private task correction routes to planning, material changes to approved specification. Current executor delegates repo-local verdicts instead of creating a controller. No changed workflow instruction contradiction was established.

## Traceability and task coverage

P = PASS, F = FAIL. Static plan compilation, not implementation acceptance.

| Task | Outcome/scope | Owners/impact | Interfaces | Control flow | Acceptance | Tests | Verification | Adaptation | Rehearsal |
|---|---|---|---|---|---|---|---|---|---|
| TASK-001 carry-forward | P | P | P | P | P | P | P | P | P |
| TASK-002-cleanup | P | P | P | P | P | P | P | P | P |
| TASK-003 | P | P | P | P | P | P | P | P | P |
| TASK-004 | P | P | P | P | P | P | P | P | P |
| TASK-005 | P | P | P | P | P | P | P | P | P |

TASK-001 preserves candidate-bound native runtime/DTO/telemetry obligations. Cleanup owns Native file/registered loading across SDKs and registration replacement. TASK-003 owns shared local config and gateway/remote transport. TASK-004 owns bounded accepted lifecycle, scopes, tools and query settlement. TASK-005 owns CLI, compiled apply/team reuse, SDK route proof and integrated closure. README maps current obligations including REQ-054–059 and AC-029–031.

Independent cold rehearsal located owners/callers/consumers/manifests/projections/test fixtures/setup/recipes and walked first implementation/test steps for every task; see cold.md. Remaining internal methods, facade fields, authored provenance representation, guards/channels and fixture layout are reversible within selected owners. No material API/identity/persistence/lifecycle choice remains. Cleanup rehearsal passes after independently verifying the exact UID-race proof correction. Final inspected tasks have no remaining material plan gap.

## Static limitations

No builds, implementation tests, live requests or production edits. New tests are explicitly planned: static package/target/recipe inspection cannot prove their future filters select tests. Tasks require selected counts and RED/GREEN evidence. Rust SDK targets auto-discover but ignored journey needs --run-ignored all; SDK lane is lib-only. CLI autotests=false requires module wiring and separate ignored selectors. Python Cards lane selects CRUD file; TS integration selects directory. Anchored Vitest selector must match eventual full title. Rust json! object-input example requires closing existing WorkflowInput conversion (currently Map/String rather than JsonValue). These are bounded explicit implementation closures, not unresolved public decisions. Generated declarations, owning language runtimes and live UID/authorization behavior remain implementation proof.
