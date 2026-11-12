# Task 006 review scope

## Immutable subject

Root: /home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004
Base: 2165fe606efe498db4e0455866d29236e85d889c (obligation record immediately before first implementation).
Candidate: efda8cb2c59c8206b52dbe1d92fcae1ba34be430.
First formal task review: full `git diff 2165fe606...efda8cb2c`; no prior Task 006 acceptance is carried forward. Implementation checker summaries are not formal review evidence.
Original task and verification evidence: changes/active/github-verifier-pipelines/tasks/006-pr-admission-and-supersession.md at candidate; seed all 18 obligation rows, adding omitted task obligations.
Approved spec: `git show 0f08e6c9f:changes/active/github-verifier-pipelines/spec.md`, revision 5 approved. Current revision 6 is draft and relates to Task 004 storage; tasks/README.md explicitly retains revision 5 basis for other tasks. Relevant contracts and approved design supplements provide context, not replacement authority.

## Navigation map (starting point, not coverage limit)

- Signed webhook: components/github/service.rs and routes.rs -> pull_requests.rs observation/admission -> selected_repository and pull_request_bindings.
- Authority: verification/authority.rs, wyrd-auth/issuance.rs; declaring principal, current permission, audit; installation routing and TenantConn.
- Lifecycle: wyrd-sql queries/verifier_executions/pull_requests.rs reserve_pull_request, activate_pull_requests, supersession, deactivation/access loss; migration 20261114000000; repository selection locks and queue claim/settlement siblings.
- Durable handoff: verification/submissions.rs SubmissionSink, inputs.rs ExecutionInputs, scribe_outbox.rs -> queue activation -> runner/results/services. Compare complete existing scheduled/group/submission workflow before accepting new machinery.
- Recovery: github/app.rs paginated failed deliveries, github/service.rs maintenance and connection lifecycle; same admission pipeline after restart.
- Public projections: wyrd-spec verification/execution.rs -> Python authored verify stub, generated TS card-types, existing SDK/CLI/MCP execution inspection; mcp/guide.rs and docs guide/concept.
- Proof: pg_github_pull_requests.rs, pg_verifier_executions.rs, pg_github_connection and existing pg_verification_* siblings; Rust repository_declaration.rs, Python test_repository_declaration.py, TS repository-declaration.test.ts, MCP github.rs and CLI github_journey.rs; wyrd-testing/github.rs and Python/TS mock bindings.

## Specialist applicability and assignments

| Contract / invariant | Owners, producers -> consumers | Selection and reason |
|---|---|---|
| Complete acceptance A1-A18, eligibility, public projections, non-goals | webhook -> queue -> engine -> SDK/CLI/MCP/docs | behavior-rev required first review, full matrix and caller-to-result evidence |
| Frozen fanout identity, generations, fencing, partial commit | observation + policy selection -> queue reserve/activate -> claim/settlement | invariant-rev: materially complex lifecycle spanning owners |
| Cross-tier rules, rustdoc, generated parity, SQL capabilities | spec/server/SQL/testing -> Rust/Python/TS/MCP/CLI/docs | repo-rev: 42 changed files across independently governed layers; include helpers and changed-item rustdoc |
| Ownership and reuse of admission/recovery orchestration | GithubPullRequests, VerifierExecutionQueue, SubmissionSink and siblings | maintainer-rev: substantial new orchestration and SQL module; compare complete existing alternatives |
| Retry/ACK, outage, restart, failed-delivery recovery | App maintenance + Scribe -> submissions -> queue/runner | system-rev: material failure/recovery changes and shared process availability |
| Security, RBAC, credentials, tenant association | signature/installation routing + declared authority -> tenant admission/status | domain-rev-security: changed authority and tenant isolation require end-to-end coverage |
| Persistent state and concurrency | migration, observation/selection locks -> reservations -> sibling queue transitions | domain-rev-data: new durable columns, idempotency and concurrent writers |
| Independent validation | all reports + immutable source -> final ledger | fresh ponytail-rev after discovery; separate source validation for any new finding it discovers |

No materially affected specialist boundary is omitted. GitHub protocol is covered by behavior/system/security; no OIDC/JWT profile changes are prescribed. UI, model quality and OLAP redesign are outside scope.

## Rules and verification

Read root and nested AGENTS.md (SQL, Python, TS, docs), changes/spec-driven-development.md, agent_docs/index.md and applicable topic guides; architecture/README.md maps owning sections (Trigger/queued activation, Scribe handoff, security/tenant binding, operations reliability). Follow CodeGraph. Review source remains unchanged; reports only in this directory. Do not run source-mutating formatters/codegen, implement, commit or push. Available execution evidence is in the task; inspect assertions and distinguish reported results from independently executed checks. Coordinate DB runs with orchestrator. Final review verifies HEAD and source immutability.
