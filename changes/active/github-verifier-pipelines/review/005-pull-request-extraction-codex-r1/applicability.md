# Applicability and immutable subject

First independent review of TASK-005 across `git diff b262696af...1f798d132`.
Candidate: `1f798d132c03cbc2474e1bad4bc36763e8047848`; base: `b262696af`.
Root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004`.
Authority: approved revision 8 of `changes/active/github-verifier-pipelines/spec.md`, original task `tasks/005-pull-request-extraction.md`, contracts and owning architecture.
No previous conclusions or acceptance evidence are carried forward. The old r1/r2 reviewer reports are excluded. The r1 remediation task and its documentation correction remain ordinary candidate scope.

## Shared starting navigation map

- Authored action: wyrd-spec `card/verifier/{mod,pull_request,validate}.rs`, vala-verify `blocks/engine.rs`, generated TS `card-types.ts`; consumers are declaration, block binding and direct eligibility.
- Server acquisition: GithubApp/GithubService -> hosted `ExecutionBinding` -> `components/github/extraction.rs` and `extraction/output.rs`; extends the existing Task-004 public pipeline and hosted extraction owner. Source comes from admitted PR execution, not user credentials.
- Custody/baselines: `verification/{engines,runner}.rs`, SQL `queries/verifier_executions{,/pull_requests}.rs`, migration `20261115000000_pull_request_source_extracted.sql`; consumers are completion, accepted-state recovery and later PR reviews.
- Metadata: `components/github/review_cache.rs` CodeReviewMetadata/Worker, existing SQL code_review queries; pipeline snapshot/context protocols and cleanup are sibling consumers. Compare verification Completions for existing batching pattern.
- Proof: server integration `pg_github_pull_request_extraction`, `pg_code_review_cache`, `pg_card_declaration_route`, `pg_github_pull_requests`, SQL `pg_verifier_executions`, existing media extract tests, output schema unit test.
- Documentation/authority: architecture wyrd-design Verifier/Agents/workspaces and bifrost-design custody; docs verifier-card and declare-a-repository; generated llms-full. Task obligations X1–X13 are the starting acceptance ledger, reconciled against spec revision 8.

## Contract coverage and selection

| Contract/invariant; owner, producers -> consumers | Role | Selection and source basis |
|---|---|---|
| Complete X1–X13 caller-to-result behavior; declaration/admitted queue -> engine -> evidence consumers | behavior-rev | Required first review; full cumulative range and regressions |
| Accepted extraction state, completion fact, baseline history, replay and cancellation; runner/SQL -> recovery/next execution | invariant-rev | Selected: new eligibility fact propagates through single/batched fenced completion and recovery |
| Rust/spec/SQL/TS/docs rules and generated parity | repo-rev | Selected: broad cross-tier changes and mandatory rustdoc/migration/tenant gates |
| New extraction/output owner and metadata worker abstraction vs existing hosted/pipeline/Completions owners | maintainer-rev | Selected: substantial new orchestration and batching ownership |
| GitHub/storage/Postgres failures, worker admission, checkpoint/restart and live consumption | system-rev | Selected: new metadata queues and changed acquisition/recovery propagation affect deployed shared server |
| Tenant/source authority and durable SQL integrity; activation/connection -> provider/evidence/history, RLS and fences | domain-rev (data-security) | Selected: one coherent end-to-end tenant-bound source/evidence custody domain including credentials, migration, history and completion |
| Independent ledger, materiality, correction burden and role coverage | ponytail-rev | Required after discovery; fresh agent, no intended verdict |

Reviewers independently expand this map; it is not a completeness claim. They read actual Git range and source, applicable nested AGENTS (SQL, docs, TS), maintainer guide and applicable authorities. CodeGraph precedes source navigation. Implementation Evidence entries are claims, not accepted conclusions. No source/doc/test/task edits; exact assigned report only. Runtime checks may be limited by available local dependencies; record what actually ran. Follow-up selection and final independent coverage validation will be appended after discovery.

## Discovery completion and comparison

All six selected discovery reports are present: behavior, invariants, standards, maintainer, system, and data-security. Behavior/invariants/maintainer/system/domain propose no material findings. Standards proposes STD-005-1, one shared mandatory changed-item rustdoc gate covering new test helper fields and a fallible fixture helper's panic documentation. No competing behavioral diagnoses, conflicting correction boundaries or unresolved traced path were reported, so a focused followup-rev is not required. Unique findings go directly to independent source validation; uniqueness alone does not trigger another discovery pass.

The validator receives all six reports, this applicability record, verification.md, original task/spec/contracts and the immutable range, without an intended verdict. No prior review conclusions or closure evidence are carried forward. Fresh PostgreSQL server lane is PASS 55/55; focused SQL lane is running and its result will be appended to verification.md. Full codegen/docs generators are not invoked because they rewrite source; reports distinguish static parity checks and historical claims from fresh execution.

Shared memory search returned maintainer preferences concerning approved hot-path waits and benchmark evidence. system-review.md inventories the changed waits and establishes that new acquisition/history/metadata waits are in explicitly approved queued PR paths; direct-eligible hot paths do not enter them. It does not infer a new broad benchmark requirement from queue-only acquisition. Approved spec revision 8 and task amendments govern over superseded packet text.
