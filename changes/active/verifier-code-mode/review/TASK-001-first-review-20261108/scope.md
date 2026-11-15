# TASK-001 review subject and specialist applicability

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/verifier-code-mode`.
- Round: first cumulative task acceptance review; no carried-forward implementation review evidence.
- Base: `5234961d51798581a026834cfffd5efaadc226b4` (parent of first implementation commit `d57c54781`).
- Candidate: `0e51af2a2332f362ab44b7c5b88b51c3cb4f8011` (HEAD at review start).
- Range: `git diff 5234961d51798581a026834cfffd5efaadc226b4...0e51af2a2332f362ab44b7c5b88b51c3cb4f8011`.
- Approved spec: `changes/active/verifier-code-mode/spec.md`, revision 2; SHA-256 `cc1b3c37d28201f3207769901776cc34b6a73c2810e9c42c53b50c2033cd4055`.
- Original task with appended verification evidence: `changes/active/verifier-code-mode/tasks/TASK-001-bounded-code-orchestration.md`; SHA-256 `494bca6fa15325ace56ffe116e45d7d4f745361c56567e5b8351af6cb9fb1521`.
- Spec/task packet is untracked, deliberately accepted as local governing input, not a committed source change; do not edit it during review. No commit/push authorization.
- Tracked working tree is clean. Review only immutable committed code; reports are written only in this review directory. No diffs, patches or source snapshots belong here.

## Navigation map (starting points, not complete coverage)

| Workflow | Owners/symbols | Callers, consumers and proof |
| --- | --- | --- |
| Agent execution and shared dispatch | `vala-verify/src/blocks/tools.rs`: ToolSession, Dispatch, ToolBridge, RecordedTool, Halt, ToolCalls; `blocks/agent.rs`: AgentExecutor | `blocks.rs` BlockCall/output types, `blocks/engine.rs` attempt/settlement; `tests/integration/blocks_agent.rs`; hosted tools |
| Parent-mediated Python bridge | `wyrd-server/src/verification/hosted/sandbox.rs`: Sandbox::exec, request, answer, drive, JSON/Monty conversion; `hosted/tools.rs`: Invocation::call/code, CodeTool implementation, Reservation | Monty protocol/allocator/worker existing owner; authored CodeExecutor; actual-worker server integration and Rust SDK journeys |
| Workspace and lifecycle | `hosted/workspace.rs`: writer, mounts, extraction, write-room accounting; Sandbox shutdown; query cancellation token/drop guard | fs.* and Monty writes; pending futures; worker/workspace admission; queue Busy consumers; pg_hosted_artifacts and sandbox_worker tests |
| Wire and retained evidence | `wyrd-spec/src/verification/execution.rs`: ToolCallRecord recipient, RequirementResult calls/defaults; `error.rs`; engine-owned ToolCalls | results StoredOutput/result reader, sensitive outcome capture/payload permissions, Rust SDK re-exports, TypeScript generated wire/error fields, historical/custom data decoding |
| Configuration/discovery/authority | HostedLimits; VerificationConfig checked env limits; changed Verifier and security architecture paragraphs | startup limits, descriptor catalog, accepted caller/domain permissions, BoundedQuery/Oracle audit; unchanged downstream permission owners |
| Public proof | Rust `verifier_building_blocks` integration story, evidence-reduction and policy-desk Cards | real SDK/server/Postgres/worker, local provider script, immediate/retained result, denial and recovery, model-visible byte comparison |

Prefixes: Vala `crates/vala/vala-verify`; server `crates/wyrd/wyrd-server`; SDK `sdks/wyrd-sdk-rust`.

## Scope and coverage obligations

TASK-001's four user scenarios and all mapped REQ/INV/AC obligations are reviewed, including required proof, source-backed root causes, unchanged capability protections and task/non-goal boundaries. TASK-002's focused Python/TypeScript/MCP workflows are not TASK-001 acceptance, but mechanical generated consumer correctness and required source buildability are. The approved spec governs over candidate design prose and implementation evidence. Evidence appended to TASK-001 contains reported commands/results and diagnoses; these are claims to verify, not acceptance or permission to waive proof.

| Material contract/invariant | Producer → owner → consumer | Selected independent specialist / reason |
| --- | --- | --- |
| Complete user behavior, errors, result visibility, requirement/acceptance/non-goal closure | Agent declaration → Vala/hosted runtime → SDK result/model | behavior-rev: comprehensive acceptance audit of actual code and tests |
| Retry-spanning mutable state, terminal record completion, leases/cleanup and nested future lifetime | Engine BlockCall/ToolCalls → shared session/dispatch → action/check settlement | invariant-rev: complex shared state spans attempts, cancellation and result assembly |
| Broad cross-tier rules, rustdoc, generated consumers/test integrity | pure spec + Vala + server + SDK + docs/fixtures → shared projections | repo-rev: broad independent Rust/TypeScript/generated/docs boundary coverage |
| New orchestration/ownership abstractions and repository reuse | previous ToolSession/RecordedTool/Sandbox → Dispatch/ToolBridge/ToolCalls/CodeTool and workspace coordination | maintainer-rev: substantial shared workflow/ownership changes require independent necessity/reuse audit |
| Failure propagation, Busy/timeout/cancellation/shutdown and subsequent recovery | parent server → pool/worker/tool dependencies → queue/public execution outcome | system-rev: actual failure boundaries and shared-service recovery materially changed |
| Authorization, tenancy, error safety and worker trust | accepted caller → common protected dispatcher → Card/Bifrost/fs capabilities/audit | domain-security-rev: sensitive RBAC/tenant/delegation and parent/worker boundary |
| Concurrent limits, waiters, permits and workspace mutation | model/direct + nested futures → Dispatch/shared workspace → result association/limit accounting | domain-concurrency-rev: explicit concurrency/resource boundaries require domain expertise, beyond lifecycle tracing |
| Additive durable evidence, capture/privacy and historical/custom-data preservation | ToolCalls → RequirementResult/StepResult → StoredOutput/result reader/SDK | domain-records-rev: retained evidence integrity and capture permission sensitive boundary; no migration owner changes assumed |
| Independent validation of all claims, applicability and correction burden | every selected report + actual source → deduplicated ledger | ponytail-rev: always required after discovery, fresh agent not a discovery reviewer |

No listed specialist omitted. External provider/standard research is not selected: no external protocol/provider behavior changed; repository authorities define this task. No implementation or test mutations authorized to reviewers. No overlapping Cargo commands: reviewers perform static review and ask orchestrator for any focused test reproduction, supplying the exact command. Repository CodeGraph directory absent at review start.

## Assigned reports

- behavior-rev: `task-review-behavior.md`
- invariant-rev: `task-review-invariants.md`
- repo-rev: `standards-review.md`
- maintainer-rev: `maintainer-review.md`
- system-rev: `system-review.md`
- domain-security-rev: `domain-review-security.md`
- domain-concurrency-rev: `domain-review-concurrency.md`
- domain-records-rev: `domain-review-records.md`
- ponytail-rev (after discovery): `findings-validation.md`

Follow-up is selected only for material conflict/unresolved path; record the decision after comparing reports. Candidate, spec and task hashes must be unchanged at closeout.
