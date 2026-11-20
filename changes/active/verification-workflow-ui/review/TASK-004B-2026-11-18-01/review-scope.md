# TASK-004B review subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Branch at initiation: `task/wyrd-ui-main-integration/TASK-005`
- Base: `2ee8bddcab3fff94a75ba35fbcdc4a91a9b9f49f`
- Candidate: `7e2b3f6fab8c81fb85769eee37d0c7fd6b633146`
- Direct review range: `git diff 2ee8bddca...7e2b3f6fa` (11 commits).
- Contract: `SPEC-verification-workflow-ui`, approved revision 6; supporting
  `SPEC-wyrd-ui-main-integration`, revision 28.
- Original task: `changes/active/verification-workflow-ui/tasks/TASK-004B-ui-query-consolidation.md`.
- First review of this task; no prior closure findings carried forward.

The caller initiates independent acceptance review. The user reports a maintainer
override excluding an L1 benchmark for the hosted charge-order correction. This
review records the missing performance measurement as a verification limit and
respects that explicit override; it does not silently treat a benchmark as run.

## Navigation and assignments

This is an initial navigation map, not a coverage conclusion.

| Owner/path | Changed behavior and consumers | Independent reviewer |
|---|---|---|
| UI portable `observe.ts`, loaders and route composition | Grouping sets for facets, overview, stats, Verify discovery; browser/Node/MCP loader consumers and query diagnostics | `behavior_rev` → `task-review-behavior.md` |
| UI `streamed.svelte.ts`, Observe/Verify routes, Card memo, tenant layout | Pending/error sections, stale result suppression, address restoration, cancellation, Live sequencing, request context and session lifecycle | `invariant_rev` → `task-review-invariants.md` |
| Oracle `oracle/mod.rs`, public query projections, SDK/MCP journeys | Bound EXPLAIN planning/execution, authorization, private-locator redaction, admission and terminals | `domain_rev` → `task-review-domain.md` |
| Hosted `sandbox.rs`, test server holds, SDK capacity assertions | Retained input charge ordering, process pressure attribution, direct/queued refusal and deadlines | `domain_rev` → `task-review-domain.md` |
| All requirements and reports | Independent source validation and reconciliation | Fresh `ponytail_rev` after discovery |

Primary retains all unassigned requirement and repository-rule coverage. The
lifecycle specialist covers shared state/concurrency; the domain specialist
covers security, tenancy, public query and hosted resource boundaries. Separate
repo, maintainer and system reviewers are omitted because those checks are owned
by the primary and these two specialists; the changes introduce no new durable
storage, deployment topology, or owner abstraction requiring another boundary.

## Authority and source handling

Reviewers read the task/spec independently, the repository AGENTS and applicable
nested SDK/docs AGENTS, spec-driven development, maintainer style, TESTING, and
the relevant architecture owners. Bifrost's Oracle read terminal/EXPLAIN contract
and Wyrd's hosted capacity contract govern the server boundaries. CodeGraph is
present and used before source navigation.

At initiation HEAD equals the candidate. Existing unrelated working-tree edits
are confined to the UI skill and github-verifier-pipelines design, plus untracked
design/change files. They are preserved. Committed versions of dirty skill files
provide a reproducible review reference; current explicit user instructions
remain higher authority. Review artifacts are the only intended tracked-path
writes. No implementation, staging, commit, push, or benchmark is authorized here.

## Verification collection

The task records focused real-server UI, Oracle, MCP and shipped SDK execution,
operator comparisons, format/lint results, and the timing/order limits of those
runs. Reviewers assess the actual assertions and candidate applicability. The
orchestrator independently reruns the focused UI unit/component files and
typecheck, recording actual outcomes in `verification.md`. Full journey suites
are not repeated by default in this bounded task review.
