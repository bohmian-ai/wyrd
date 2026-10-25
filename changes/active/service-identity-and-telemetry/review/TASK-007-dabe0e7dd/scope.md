# TASK-007 review subject and navigation

First task review; no prior TASK-007 acceptance is carried forward.
Repository: /Users/stevenforrester/Documents/GitHub/schema-change-task-7
Base: 76e095738d2cf89e10523a92bb9b4f585fa1dd3d
Candidate: dabe0e7dde58c6e26d584120ae0a48cfede79436
Range: git diff 76e095738...dabe0e7dd
Initial HEAD equals candidate; initial tracked working tree clean. No .codegraph directory exists. No nested AGENTS.md governs changed files.

Authority: AGENTS.md; agent_docs/index.md, rust.md, maintainer-style.md; changes/spec-driven-development.md; approved revision-16 changes/active/service-identity-and-telemetry/spec.md; original task at changes/active/service-identity-and-telemetry/tasks/TASK-007-verification-query-performance.md (use base version for original obligations, candidate appended evidence for available results); storage supplement section 'Verification evidence access and lookup performance'; architecture/bifrost-design.md 'Query: Oracle', relevant source visibility and terminal contract sections; TESTING.md and owning mise tasks. Read only additional authority needed for actual affected paths.

## Starting map (expand independently)
- oracle/live.rs: LiveUnionBoundary::{install,optimize}; LiveScribeExec; sorted/unsorted published sibling boundary and optimizer sanity check. Consumers: analytical planning session, UnionExec, distributed network coalescing and ordered consumers. Tests: boundary_keeps_a_sorted_sibling_sorted, installed_chain_rejects_a_lost_ordering; existing live tests.
- oracle/analytical.rs: AnalyticalExecutionHandle planning session installs the rule chain before distributed planner; distributed_rewrites_preserve_physical_requirements covers planner and leaf variants. Trace optimizer order, pinned planner boundary handling and error propagation.
- wyrd-testing/tests/bifrost/server/verification_runtime.rs: ScanEvidence and ResultLayoutJourney; exploration filter fixture and StoredBlooms; completed_result_lookup_preserves_evidence; window_lookup_preserves_population; group_lookup_preserves_source_occurrences; mcp_query and custom_query_preserves_evidence_and_limits. Existing trace journeys and profile regressions are referenced in task evidence.
- Existing retrieval owners: VerificationControl::{get_execution,read_result}; ObservationWindow/DriftEngine; GroupPoller/Sources; HostedTools/BoundedQuery; CallerTraces. Compare owners to evidence and preserve their authority, exact selection, limits and source visibility.
- Cargo.toml: dependency comment only, no version change.
- Task Markdown: appended inventory, measurements, diagnoses, verification claims and limits. Validate evidence against original obligations rather than accepting its own PASS table.

## Reviewer selection
behavior-rev: full task acceptance, evidence adequacy, repository rules and maintainability across every changed file, sibling callers and credible regression/failure paths.
domain-rev-query-execution: sensitive query correctness boundary: physical ordering/distribution, optimizer/install chain, distributed rewrite, query-local failure and existing memory/source authority. Selected because a shared production physical rule changes and applies to public SQL consumers.
No separate lifecycle/invariant reviewer: no new mutable shared state or lifecycle owner. No separate repo/maintainer reviewer: no broad cross-tier implementation or substantial ownership/abstraction replacement; behavior reviewer retains applicable standards, rustdoc and maintainability coverage. No separate resilience reviewer: failure change is confined to query planning; domain reviewer traces its request scope and propagation.
Fresh ponytail-rev is required after discovery for independent source validation, including an empty proposed ledger. Follow-up only for material conflict or unresolved path; selection will be recorded after discovery.

Review directory contains reports only. Source stays immutable; do not implement fixes or run format/mutation commands. Reviewers may run focused existing tests through mise; coordinate cargo runs with orchestrator. Never save diffs or source archives here.

## Discovery reconciliation and follow-up decision

Both selected discovery reports are complete. behavior-rev proposes BEH-001 (measurement record) and BEH-002 (populated exploration/policy coverage); query-execution domain review proposes no production findings. These results concern distinct obligations and do not conflict. Group the proposals by missing measurement versus missing representative population/policy proof, rather than by line. No focused followup-rev is selected: no material disagreement or untraced production common cause remains at this stage. A fresh ponytail-rev independently validates both proposals, authority/materiality, and remaining acceptance coverage. It may revise/reject either; discovery FAIL is not the final verdict. New validation-discovered blockers require separate focused source validation before retention.
