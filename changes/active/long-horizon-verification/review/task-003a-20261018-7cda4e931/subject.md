# TASK-003a immutable acceptance audit

- Repository: /home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-001
- Base: 22335c2c40a5ad24c27bfa0d4686f4da49f922c5 (approved revision-17 packet, after TASKS 001–003 and main integration).
- Candidate: 7cda4e93159ab328de2a74068d5e1978a2590818.
- Subject range: `git diff 22335c2c40a5ad24c27bfa0d4686f4da49f922c5...7cda4e93159ab328de2a74068d5e1978a2590818`.
- Original approved task: `git show 22335c2c40a5ad24c27bfa0d4686f4da49f922c5:changes/active/long-horizon-verification/tasks/TASK-003a-simplify-grouped-verification.md`.
- Specification: changes/active/long-horizon-verification/spec.md, approved revision 17 (at both subject commits).
- Verification evidence: candidate task's appended Implementation Evidence; exact planned commands C1–C13 appear before that appendix. Evaluate evidence credibility independently. No root rerun has occurred when establishing this subject.
- Initial candidate worktree: clean. Only this new report directory may be written. Source, specification, task and generated outputs remain immutable. No review commit, implementation, merge, push or deployment.

## Navigation map (starting points, not completeness or conclusions)

| Path/owner | Symbols and seams | Caller/consumer proof starting points |
|---|---|---|
| wyrd-server/src/scribe_outbox.rs | ScribeSink::write, ACK → pending activity upsert → reserved submit handoff | direct verify/result writers, runner, gateway capture, peer audit; outbox tests |
| wyrd-sql/src/queries/verifier_executions.rs | pending upsert/claim/consume; VerifierExecutionQueue::enqueue_group, claim, complete; digest and settled-order predecessor | scheduler, runner, leases; pg_verifier_executions snapshot/settlement test |
| wyrd-sql/migrations/{20260601000027_verification_bindings,20261016000000_verifier_executions,20261018000000_verification_group_pending}.sql | pending schema, exact source matching, RLS, FK/index/uniqueness; pre-release in-place changes | tenant/operator query owners, migration/SQL evidence |
| wyrd-server/src/verification/{scheduler,mod,claims,runner}.rs | VerificationScheduler separate cron/group loops, 64 tenant / 32 item bounds, shared execution capacity | process composition/boot, operators/fitter siblings, C13 and pressure journeys |
| wyrd-server/src/verification/groups.rs | GroupPoller::poll, Reader, Sources::read/decode, Visible/Earliest/pin, exact-group Bifrost snapshot | ScheduledQueryCaller; pg_verification_groups C8/C13; SDK source/alignment scenarios |
| wyrd-spec/src/{card/trigger,verification,verification/execution}.rs | after-only Trigger/settings and minimal GroupSnapshot projection | registration service; OpenAPI C7; Python stubs; generated TS types |
| wyrd-server/src/{query/scheduled,verification/fault}.rs | test-support schema-stall/failure seam; ordinary query authorization/stream terminal behavior | C13, other scheduled queries, recorded diagnosis |
| SDK Rust/Python/TS verify-in-real-time journeys; MCP verification journeys | C1–C6/C10/C11, selected evidence/order, source isolation, original coding/media/pressure stories | existing runtime harness, shared fixtures; scheduled C12 regression |
| docs + architecture; fixture changes | user model, after, accepted tradeoffs, removed ceiling/coverage/terminology | generated contracts; original scenario definitions |

The diff changes 54 files (+2214/-2055). Inspect the complete file list and range directly; this map does not constrain coverage.

## Authority and scope

Read AGENTS.md, architecture/agent-rules.md, reference router and applicable authorities, spec/task, and your assigned role in .agents/skills/wyrd-task-review/SKILL.md. CodeGraph first for code navigation when indexed. Read surrounding owners/callers, not only diff hunks. No implementation or test edits. Reports only at your assigned path. You are not alone; do not revert or overwrite another agent's files. Do not run Cargo-heavy tests in parallel or start broad suites. If a focused executable proof is necessary, send the exact proposed command to root for coordination. All executed tests use mise.

This is the first audit of TASK-003a, not a continuation of the older TASK-001 assessment packets. Older packets under this change's review directory are explicitly superseded; do not inspect or revive them. This current directory is authorized by the newly invoked assessment workflow.

Accepted revision-17 limits are requirements, not missing safeguards: available Bifrost evidence is not a PG acceptance ledger; ACK need not mean full query visibility; no extra group crash-recovery protocol; post-ACK/pre-pending crash signal loss; continuously arriving activity postpones expiration; distinct snapshots may overlap and settle out of evidence order; source retention/outbox loss apply; durable SQL backlog can grow; no global quota/strict fairness; no ceiling verdict; optional Agent trace reads are live. REQ-015/AC-009 benchmarking is intentionally pending.

Apply acceptance thresholds from the skill: required behavior, source-backed regression/violation or deletable unearned machinery only. Do not introduce machinery to address accepted limits. Naming/style without behavioral consequence is non-blocking except missing substantive rustdoc on changed items per AGENTS. Each suspected finding must trace producer→consumer and siblings and select the smallest in-owner correction.
