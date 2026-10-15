# Immutable TASK-001 review subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/deployment-addressing`
- Base: `1d3b5ffa0d34f75fa5b22c9b68f6ec0fc8d0cc70` (captured origin/main and merge base).
- Candidate: `0aae38f9ad3886977d7684fc17d708cbb15d1f17`, branch `change/deployment-addressing`.
- Full cumulative range: 45 commits, 439 files, +8,880/-32,860 lines.
- Approved authority: `changes/active/deployment-addressing/spec.md` r5.
- Original task/recorded verification: `changes/active/deployment-addressing/tasks/TASK-001-deployment-addressing.md`.
- Full actual diff: `cumulative.diff`; paths: `changed-paths.txt`; history: `commits.txt` beside this file.
- Initial cumulative review; no prior task-review artifacts at capture. Tracked worktree/index clean.
- Review writes only this new review directory. No source/task/authority edits, staging, commits or test/build processes during discovery. Candidate/working-source movement must be reported immediately and blocks review.

## Caller authority and evidence

The user explicitly approves adjacent changes outside TASK-001. Scope adjacency alone is not DRIFT. All candidate changes remain subject to correctness and repository rules. Inspect current stronger authority and base for main-integration seams, including inline Prompt materialization, rather than blindly enforcing superseded private mechanics.

The task's appended verification table is available as evidence claims: distinguish recorded commands/commit/timing from source proof. Do not rely on an implementation completion summary. No aggregate execution is requested during discovery. The integrated aggregate belongs to change review; assess missing focused proof precisely.

## Common starting map; expand independently

1. Deployment/IDs/access: `crates/wyrd-spec/src/gateway/{deployment,mod,record,policy}.rs`, auth scope and shared runtime permission. Trace ProviderDeployment, ModelRef, GatewayAccess and contains_object_scoped.
2. Credentials/persistent data: defining gateway SQL migration, SQL gateway queries/rows, gateway snapshot/CredentialResolver and server GatewayAdministration. Trace tenant/provider/host/list/access through persistence and locks.
3. Invocation/visibility: gateway CallPlan/engine; server gateway invocation/batches/ingress/caller/capture/ledger. Trace requested/fallback/actual targets, provider-held state, spend, subject/actor, outbox and cancellation.
4. Prompt: skald-spec authoring/prompt/request/error, skald-prompt builder/prompt/coerce, wyrd-spec card/prompt spec/hash/validate. Two forms, identity once, media/settings/schema compiler, authored storage/native options.
5. Agent: card/agent.rs, skald-agent gateway/agent/loop_runtime, shared client gateway_caller and observe/invoke. Required declared versus effective route across local/native/server/SDK and tool loops.
6. Judges: vala-eval orchestrator/judge, server verification and Bifrost gateway record/table contracts; accepted subject/actor and structured output.
7. Main registration seams: shared Cards/hydrate/state, loader, server cards resolve/service/mapping, refs/metadata/graph. Follow exact Prompt materialization/navigation and stronger current authority.
8. Workflow deletion: contracts/runtime/crate/server/jobs/client/SDK/CLI/UI/Operator/catalog/ref/config removal; verify reusable Agent/gateway/judge and independent Eval DAG behavior survive.
9. Setup: CLI gateway single/list command, server dev owner, examples/gateway-setup, CLI pg/gateway journeys, Python installed-CLI driver and fresh TS client, mise selection.
10. Language/headless/schema/docs: SDK exports/stubs/generated types, CLI/MCP actual catalog, served OpenAPI, UI kind readers, fixtures/examples/architecture.
11. Approved adjacent changes: release/protoc retry/swap, telemetry/tonic publishing, selection/tooling and changed runtime/query helpers. Inspect correctness; do not demand rollback merely for adjacency.

## Review contract

Read AGENTS.md, agent-rules, spec-driven-development, maintainer-style and the reference router; completely load applicable references/authority. Follow CodeGraph. Inspect cumulative source and callers independently. Findings require source-local ID/classification, exact location, violated authority, producer-to-consumer reachability, consequence, smallest safe correction and focused proof; include sibling users of a shared cause. No speculative improvements. Missing rustdoc blocks under AGENTS; cosmetic preference without a consequence is a note.

Behavior and invariant reviewers each audit all 32 obligations/non-goals in explicit implementation/verification PASS/FAIL matrices. Other roles supply all required coverage/failure-path tables. Each report ends PASS/FAIL/BLOCKED and states verification limits. No builds/tests during discovery; request a precise repro from the orchestrator if needed.
