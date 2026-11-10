# TASK-004 first immutable review

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/code-review-workflow`.
Base: `8f1bb789e4d1ad4f481a9cefba2a0eddd82b7ef9`.
Candidate: `487ccb0b60a2a3b40b54ee9e12099880d042ba99`.
Range: `git diff 8f1bb789e...487ccb0b6` (22 commits, 138 files).
No prior TASK-004 acceptance or findings are carried forward. Review the complete task.

## Authority and evidence

- Original task: `changes/active/verifier-review-pipelines/tasks/TASK-004-github-repository-automation.md`.
- Approved baseline: revision 18 of `spec.md`, its approved amendments recorded in current revision 22, `public-contracts.md`, and `execution-state-and-durability.md`. Unrelated draft wait/approval decisions are excluded. Read the task's linked scope and applicable requirement/invariant/acceptance IDs.
- Root and nested AGENTS, `changes/spec-driven-development.md`, `agent_docs/index.md` and applicable guides, `architecture/README.md` authority map, owning architecture sections, and `TESTING.md` govern.
- Task evidence table and verification records are claims to inspect against source and assertions. Report rerun commands separately from recorded checks. No live GHES compatibility claim.
- Source is immutable. Existing local changes to two skills and untracked change packets predate review; do not alter them. Each reviewer writes only its assigned report. No patches or source archives in this directory. Recheck HEAD and source cleanliness before final verdict.
- All commands through mise. CodeGraph exists: use `mise exec -- codegraph explore "..."` before locating/reading code. No concurrent Cargo invocations; request orchestration before any build/test/lint/codegen. Static Git/source inspection is independent.

## Shared starting navigation map

This map is a starting point, not complete coverage or a conclusion. Expand against source and callers.

| Contract/workflow | Producers, owners, consumers and proof |
|---|---|
| Integration, repository, request/result and permission wire shapes | `wyrd-spec/src/repository.rs`, `verification/{execution,state}.rs`, auth scope; runtime permissions; server github routes/service; shared client github/verify; Python, TS, CLI and MCP projections; schema generator/OpenAPI tests |
| App authentication, association, sealed credentials and access | server `components/github/{provider,service,credentials,webhook}.rs`; auth issuance; SQL github queries, migration, postgres capabilities; runtime permission; keyring and storage rotation; repository journey association/access |
| Trusted base configuration and local declarations | `wyrd-config/src/config.rs`; shared loader lib/path/validate and client cards saga; server cards service; `verification/repositories.rs`; existing resolve/validate/order/declaration owners; declaration/branch journeys |
| Durable event, generation, bindings and worker lifecycle | `verification/repositories.rs` RepositoryWorker/Intake and existing verification control/queue/authority/runner; SQL repository_reviews, github, verifier_executions and verification_target; boot/mod; event/ingress/backlog journeys and SQL pg assertions |
| Check projection, fencing, delivery retry and recovery | same worker CheckPublisher; library Check Runs; SQL repository_reviews claims/advancement; existing operator/lease/claim mechanisms; retained Bifrost execution reader; Check/lifecycle/stalled-repository journeys |
| Public parity, docs and fixtures | Rust `tests/integration/repository_verification.rs`, Python `test_repository_verification.py`, TS `repository-verification.test.ts`, CLI `card_lifecycle.rs`, MCP `verification.rs`, server `pg_openapi_contract.rs`; testing github fixtures; docs and generated TS artifacts |
| External protocol library | Cargo.toml/lock immutable revision `9fdf6dc77a329f34d96b7ad4f397331d2d18216b` of bohmian-ai/bohmian-code-review; inspect available local checkout/Cargo source at that commit, actual GitHubClient/App/Check/webhook implementation and tests |

## Specialist applicability (before delegation)

| Material contract/invariant | Selected roles and source-backed reason |
|---|---|
| Complete original task and negative flows | behavior-rev: first review requires comprehensive acceptance matrix, all task ledger rows plus omitted obligations, caller-to-result traces and actual test assertions |
| Event monotonicity, dedup, claims, bindings, Check and execution lifecycles | invariant-rev: new 1,425-line repository coordinator, 1,359-line review SQL and 306-line migration materially change shared durable transitions and sibling writers |
| Cross-tier contracts, SQL capabilities, Rust docs and SDK declarations | repo-rev: 138-file range spans Rust/SQL, Python/PyO3, TS/N-API, schemas, docs, fixtures; each has mandatory independent rules |
| Loader/declaration, provider and worker ownership/reuse | maintainer-rev: new owners and materially extended shared loader/saga/runner require comparing complete existing workflows, beyond leaf-call reuse |
| Provider stalls, lost replies, Bifrost uncertainty, restart and worker recovery | system-rev: independently retryable durable delivery, spawned server worker, queue integration and ambiguous remote effects change deployed recovery behavior |
| Security, RBAC, tenant boundary and credentials | domain-rev-security: new callback state, webhook trust, installation authority, publishing/execution principals, scopes, secrets and tenant isolation are materially affected |
| Persistent-data integrity and concurrent writers | domain-rev-data: new RLS tables/unique identities, claims/fences/leases and transactions coordinate multiple replicas and external effects; distinct from security permission policy |
| Independent validation | fresh ponytail-rev after discovery: validates applicability, coverage, every proposal, materiality and minimal correction; no intended verdict |

No materially applicable role above is omitted. No UI specialist: no UI changes or UI-owned behavior in task/range. No separate ML evaluation specialist: ordinary existing Verifier judgement is reused; acceptance still traces its results and status projection. No new review roles are added.

## Reports

Each role owns its named report in this directory: `task-review-behavior.md`, `task-review-invariants.md`, `standards-review.md`, `maintainer-review.md`, `system-review.md`, `domain-review-security.md`, `domain-review-data.md`. Validation owns `findings-validation.md`. Orchestrator writes comparison, verdict, and remediation if required.
