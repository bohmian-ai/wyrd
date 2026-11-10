# Task 001 first review: immutable subject and navigation

Root: /home/thorrester/Documents/GitHub/wyrd-worktrees/code-review-workflow
Base: f4e79ea00523af5cd9212fdacde64f16c97ad554
Candidate: 365bfc94bfef1a153f213462f7ef104afa823741
Range: `git diff f4e79ea00523af5cd9212fdacde64f16c97ad554...365bfc94bfef1a153f213462f7ef104afa823741`
First review; full cumulative range; no carried-forward acceptance.

Source-location update: after the live checkout moved for a human-approved main integration, the human directed this original review to continue. Remaining source validation uses `/tmp/wyrd-task-001-review-365bfc94b`, detached at the identical original candidate. Frozen authority hashes match. Reports stay here. Never use moving live source for final validation. See subject.md; the integration is outside the reviewed range.

Approved specification: changes/active/verifier-review-pipelines/spec.md revision 16 (approved), supplements public-contracts.md and output-publication.md.
Original task: changes/active/verifier-review-pipelines/tasks/TASK-001-durable-agent-work.md.
Task and supplements are untracked; freeze them for this review. Read obligations and recorded verification evidence, not the implementation summary as proof. Other tasks supply context, not acceptance scope.

## Starting navigation; expand independently

- Admission: server verification/submissions.rs → SQL VerifierExecutionQueue admission/claim → runner.rs; SDK durable_verifier_work::queued_acceptance_survives_response_loss.
- Checkpoint/lifecycle: vala-verify blocks/engine.rs and scope.rs → server hosted/blocks.rs, runner.rs → SQL verifier_executions.rs and invocation/cancellation migrations. Tests engine/tests.rs, pg_verifier_executions, verification_runtime and SDK durable_verifier_work.
- Publications: server verification/hosted/publications.rs → workspace.rs snapshot and wyrd-storage service File lifecycle → SQL publication reservation/acceptance. Resolver in hosted/evidence.rs and Scope. Tests pg_durable_verifier_work.
- Projection: server verification/history.rs, results.rs, scribe_outbox.rs → SQL verifier_executions/events.rs and events migration → vala-bifrost-redux canonical execution_events/staged/gate registration. Tests verification_runtime.
- Inspection: components/verification/service/inspection.rs and routes.rs → SQL verifier_executions/inspection.rs and migration. Shared client verify/Execution → Python src/verify.rs/stubs, TS native/src/verification.rs/wrapper/types, CLI execution.rs, MCP verification.rs. Tests SDK durable work, MCP verification, pg_verification_routes/OpenAPI.
- Authorization: runtime permission.rs/builtin_roles.rs → inspection service; principal revoke in wyrd-auth and server auth/revoke.rs; exact-Verifier read/run and tenant RLS; local File content route and shared client storage/download dispatch.
- Contracts: wyrd-spec verification/execution.rs, ids.rs and card/verifier reference/validate; gen_schemas and gen_ts_types; docs projections; fixtures and existing verify_in_real_time journeys.

## Rules and verification

Read root AGENTS.md and applicable nested rules: crates/wyrd/wyrd-sql/AGENTS.md, sdks/wyrd-sdk-python/AGENTS.md, sdks/wyrd-sdk-ts/AGENTS.md, docs/AGENTS.md. Read changes/spec-driven-development.md, agent_docs/maintainer-style.md and applicable rust/errors/agent-surfaces guides. Architecture map: architecture/README.md. Focus owning Verifier/workspace, Bifrost durability/canonical tables/outbox, permission and Postgres tenant binding sections. Audit architecture amendments against approved spec and stronger owners.

CodeGraph exists: use `mise exec -- codegraph explore` before locating/reading code. This map is a starting point, not coverage or conclusions.
Verification results are recorded in the task's Implementation Evidence section; distinguish recorded from independently rerun results. Proof 2 selector correction and Python/TS publication-content limitations are recorded there. Inspect actual tests against obligations.
No concurrent Cargo jobs: ask orchestrator for focused checks. Do not run mutating format/codegen or edit source. Read Git range directly; no diff/source copies in review folder. Reports only. You are not alone: write only your assigned report; never revert others' changes.

## Selected reviewers

- behavior-rev: entire acceptance and supported regressions.
- invariant-rev: checkpoint/attempt/budget/fence/publication state flow.
- repo-rev: all languages/tiers, mandatory rules and changed-item rustdoc.
- maintainer-rev: ownership/reuse/abstraction and API discoverability throughout range.
- system-rev: process recovery, cancellation, dependency failure, outbox and service availability.
- domain-security: RBAC, revocation and publication/File trust boundaries.
- domain-data: persistent schema/RLS/transactions, indexes and projection integrity.
- domain-concurrency: leases, cancellation races, snapshot locking and replay identities.

Fresh independent ponytail validation follows discovery; focused follow-up only for conflict or unresolved paths. Full review requested by human; every selection has material changed boundaries.

Human concern, separate from independent findings: CLI `execution` may be too broad for verification. Assess authority/discoverability and record a bounded recommendation separately without FIND ID. No rename approved; preference alone is not a blocker.
