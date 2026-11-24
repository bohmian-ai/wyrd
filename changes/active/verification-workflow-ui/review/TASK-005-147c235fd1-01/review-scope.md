# TASK-005 immutable review

Base: `05fc05138a0a72d59a2fd9b96fae5e6e4ad6f285`.
Candidate: `147c235fd19dd4a400da1515a908db0d5521477d`, initially clean HEAD.
Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/verification-workflow-ui-resume`.
Diff: `git diff 05fc05138a...147c235fd1` (12 commits, 41 paths).
Contract: `changes/active/verification-workflow-ui/spec.md`, approved revision 8; supporting authorization-model revision 9.
Original task: `changes/active/verification-workflow-ui/tasks/TASK-005-pr-review-sets.md`.
Maintainer ruling: AC-009 listed mock differences accepted; historical TDD VIOLATED; library integration suite not rerun after final commit. Assess latest changes before carrying forward evidence.
No `.codegraph/` exists. Reports only here; production source immutable. No implementation, commits, pushes or benchmarks.

## Navigation map (not coverage)

Admission: PullRequestActivation in wyrd-spec execution.rs, GitHub app.rs/pull_requests.rs, fake provider wyrd-testing/src/github.rs, Python verify stub/generated TS card types.
Read owner: UI packages/ui/src/lib/observe.ts::loadReviewSets and execution population, schemas/loaders/catalog/WyrdView, Node/MCP/custom hosts.
Inspection: execution.ts::loadExecution, VerificationExecution.svelte, recorded facts without definitions.
Navigation: ReviewSets.svelte, PR route +page.server.ts/+page.svelte, Repository/Verify/execution links, repositories-host.ts/verify-host.ts.
Marks: execution.ts::EXECUTION_MARKS, StatusMark, RecordTable, FacetRail, VerifierGraphNode, RepositorySheet.
Proof: ui-library.test.ts, ui-pages.test.ts, ui-library.integration.test.ts, Alice ui-pages.integration.test.ts; server pg_github_pull_requests and SQL pg_verifier_executions. Original task records selectors/results/visual rulings.
Authority: AGENTS and nested SQL/Python/TS AGENTS, changes/spec-driven-development.md, agent_docs/maintainer-style.md/rust.md/agent-surfaces.md, TESTING.md, architecture/wyrd-design.md Repository/Verifier/registry, architecture/wyrd-security-posture.md retained visibility, wyrd-ui skill and references.

## Assignments

behavior-rev: assigned requirements/scenarios, navigation/pins/history/shared marks, assertions/evidence, visual ruling scope, all rules not delegated.
domain-rev: retained-parent authorization, Repository/version visibility, no-definition/deleted-Card reads, query bounds/completeness and unchanged dependencies.
repo-rev: optional GitHub display-field propagation, admission identity, SDK projections/generated artifacts and affected Rust/Python/TS owner rules.
Fresh ponytail-rev validates all proposals, coverage/evidence freshness and specialist applicability, and supplies reconciled acceptance matrix.
No separate maintainer/system reviewer: no large owner replacement or new recovery topology; primary covers local maintainability/error paths. No separate invariant reviewer: state/query lifecycle assigned to domain, navigation to primary.
Discovery reviewers independently inspect source/authority without others' conclusions. Coordinate executable checks through root; no overlapping Cargo operations.
