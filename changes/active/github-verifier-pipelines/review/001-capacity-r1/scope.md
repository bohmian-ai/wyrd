# Task 001 first acceptance review

Repository: /home/thorrester/Documents/GitHub/wyrd-factory-worktrees/code-review-workflow-github-verifier-pipelines
Base: 395274d58bf76a2075f245ca3a5c1bd044925027
Candidate: 34fb45bb387582d2aedfee77fb4e09fa2753b80d
Range: git diff 395274d58...34fb45bb3 (all four implementation commits).
Original task: changes/active/github-verifier-pipelines/tasks/001-isolate-queued-capacity.md
Approved authority: changes/active/github-verifier-pipelines/spec.md revision 3, REQ-015/016, INV-001/005/007, AC-010; design/approved-runtime-and-github.md capacity and direct eligibility sections.
Round: first, full cumulative review; no prior verdict or carried evidence.
Source remains immutable. Only reports may be written in this review directory. Never mutate production/tests or write patches/diff archives. Check HEAD at completion. Reviewers independently read actual diff, obligations and evidence; evidence claims are inputs to challenge, not acceptance.

## Navigation map (starting points, not exhaustive coverage)

- HostedLimits / LaneLimits (verification/hosted/mod.rs), Config::apply_limits_env and validation (config.rs), boot/mod.rs: configuration producer -> HostedBlocks::start and verification runtime.
- HostedBlocks / Lane / Execution (verification/hosted/blocks.rs): retained sandbox/workspace/model pools; lane selection and direct execution permits; engine constructs existing CodeExecutor, HostedTools and gateway caller.
- components/verification/service.rs: resolve/direct_eligibility -> direct admission -> common engine; verification/runner.rs: claim/reclaim -> ExecutionMode::Queued -> same engine.
- components/gateway/caller.rs: ServerWyrdGatewayCaller::admitted/invoke -> lane permit -> existing GatewayInvocation -> tenant/subject/budget/accounting. Non-verification callers also consume this owner.
- hosted/sandbox.rs, hosted/workspace.rs: lane-specific resource admission, Busy behavior and shutdown.
- wyrd-testing/src/logs.rs, server.rs: process-global log capture and test hold capabilities consumed by SDK journey.
- sdks/wyrd-sdk-rust/tests/integration/isolated_capacity.rs and fixtures/cards/isolated_capacity: real SDK/server/Postgres held-capacity, deferral/reclaim, gateway attribution, cancellation and no-borrowing proof. Existing hosted_agent_tools, verify_in_real_time, verifier_orchestration and server hosted/sandbox/durable tests are affected consumers.
- architecture/wyrd-design.md Agents, tools, and workspaces; operator configure-verification-runtime and reference verification-runtime-limits; generated llms files.
- Task Evidence section records exact commands, asserted lines, results and one failing orchestration test with diagnosis. Audit that diagnosis independently; do not infer pass from summaries.

## Specialist applicability

| Contract/invariant | Owner, producers and consumers | Selection and source-backed reason |
|---|---|---|
| Four admissions isolated, no borrowing, direct eligibility and ordinary graph behavior | Config -> HostedBlocks -> service/runner -> executors | behavior-rev comprehensive acceptance, all O1–O19 and omitted task obligations |
| Shared lane resources, direct permit lifetime, queued claim/reclaim, model wait cancellation | HostedBlocks::Lane, service, runner, gateway caller, Sandbox shutdown | invariant-rev required: materially changed shared state and lifecycle |
| Admission synchronization and sensitive concurrency | Arc<Semaphore>, Sandbox and Workspaces -> parallel execution/model callers | domain-rev concurrency required: independently trace permit ownership, waits, races, no borrowing |
| Timeout, shutdown, startup failure and saturation propagation | boot, HostedBlocks::start/shutdown, gateway waits, service refusal, runner defer/reclaim | system-rev required: changed failure propagation/recovery and shared server availability |
| Ownership and abstraction | HostedBlocks adds Lane, config adds LaneLimits; gateway caller and LogCapture extended | maintainer-rev required: substantial resource ownership/composition and harness changes; compare existing owners and alternatives |
| Cross-tier repository rules, rustdoc, tests, docs and generated parity | server, testing library, SDK journey, fixtures, docs | repo-rev required: broad rules across server/harness/SDK/docs; include changed test helpers and rustdoc |
| Security/RBAC/tenant identity and durable data | Existing accepted Caller, GatewayInvocation, queue/storage paths | No separate security or persistence domain specialist: no new identity, SQL/schema, credential, authorization or durable handoff contract. behavior/invariant reviewers trace preservation through unchanged consumers; concurrency/system specialists cover waits and retries. Validator must challenge omission if tracing reveals material impact. |
| Python/TS projection APIs, UI | No changed SDK API, Python/TS sources or UI | No language/UI specialist; server runtime behavior needs existing boundary proof but task introduces no SDK operation. repo/behavior reviewers assess journey tier and coverage. |

## Assigned reports

behavior-rev: task-review-behavior.md
invariant-rev: task-review-invariants.md
repo-rev: standards-review.md
maintainer-rev: maintainer-review.md
system-rev: system-review.md
domain-rev concurrency: domain-review-concurrency.md
Fresh independent validation after all discovery: findings-validation.md.

Cargo commands must run sequentially across this checkout. Discovery is source review; request any runtime check through orchestrator. Existing evidence is in task; no new checks yet. No implementation summary is supplied to fresh reviewers. Every reviewer follows .agents/skills/wyrd-task-review/SKILL.md for its role and materiality threshold; no intended verdict.
