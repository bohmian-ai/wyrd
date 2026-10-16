# TASK-003 independent system-resilience review

Result: **FAIL**.

Subject: cumulative base `41b0f01d9b47d9b07b078b2431fcdd4cba56a9b0` through candidate `57c69c3579e4c331e8d3258e00b22ee7aa0155cb`. No source changes. Deployment composition, environment parsing, queue integration, container packaging, and final journeys belong to TASK-005 and are not findings here. Intrinsic sandbox and invocation limits explicitly assigned to TASK-003 remain acceptance obligations.

## Authority and scope

Reviewed the immutable subject, task/spec snapshots, changed-surface inventory and cumulative changed owners, AGENTS.md instructions, architecture/agent-rules.md, task-review skill, reference router, spec-driven-development, maintainer-style, testing-workflows, architecture-constraints, applicable Wyrd design verification/topology contracts, and operations readiness/reliability authority. CodeGraph was used before locating runtime paths. Dependency behavior below was checked against installed locked monty-pool, monty-fs, and monty-proto 1.1.0 source rather than inferred from API names. No other discovery report was read.

The operational boundary is the operation/worker: refuse unsafe Python execution and fail its affected operation; retain unrelated server capabilities. Deployment limits require hard aggregate memory, cumulative per-invocation work/Python limits, bounded waiting and parent-serviced IO, and discarded compromised sessions. REQ-008/AC-004 require crash/hang containment; scenarios 4–5 require component failure/cancellation/resource proof now.

## Deployed-path evidence

| Path | Owner/process/dependencies | Failure propagation and capabilities |
|---|---|---|
| Existing hosted evaluation | verification/eval.rs TenantRegistry, old judge, Vala/Skald, registry Postgres, gateway/provider | Resolver/error helpers relocate to Agent blocks; old judge conversion retained. Connection acquisition remains retryable and invalid/missing Cards terminal. Executes in serving process. |
| Agent action/check | AgentExecutor → existing Agent::run_prompt → gateway/provider; per-call HostedTools session for steps | Provider failures remain call failures. Checks refuse tools/workspaces. Model waits do not acquire Monty workers. Existing Skald loop executes tools concurrently through buffer_unordered(concurrency_cap), loop_runtime.rs:527–663. |
| Evidence/extract | TenantEvidence read/execute/located → tenant object storage → Extraction on shared blocking pool | Missing/corrupt evidence terminal, backend outage retryable. Parsing is in parent process, outside worker memory. File/digest/format checks precede use. No added durable write/scheduler. |
| Workspace/filesystem | process Workspaces semaphore → evidence reads → blocking materialization → per-session Workspace/TempDir | Permit held until owner drops. Cancellation before blocking materialization releases pending reads/permit; already-started blocking operations continue until completion and eventual TempDir drop. fs.read/search also use blocking work. |
| Protected built-ins | invocation Caller → audit::authorize → existing Card/Bifrost owners/BoundedQuery | Invoke/domain denial remains local, with shared nonblocking audit. Query retains own deadline. Bifrost is not moved into a Python worker. |
| Python | Sandbox in verification-runtime server process → one monty_pool::Pool → separate wyrd-monty-worker subprocess per checkout | Shared across tenants when composed by TASK-005. Parent owns admission, output, mounts, deadlines, watchdog; child owns Monty/Linux confinement. A child fault should end its operation while parent/siblings survive. |

Wyrd all/server targets can colocate public serving and Bifrost subsystems. The sandbox does not exclusively own a process/pod. Parent OOM or an unrelated-process kill can affect Cards, gateway, audit outbox, and verification. No separate Python service/pod is constructed here. Focused seam/test consumers exercise the concrete executors; production composition intentionally belongs to TASK-005. This does not defer intrinsic resource correctness.

## Failure-path coverage and proof

| Failure | Source/proof assessment | Result |
|---|---|---|
| Missing/unavailable evidence | Length/digest/format validation, redacted transient backend error; no added retry loop | PASS at component boundary |
| Provider failure / tool-less check | Existing Skald route/loop/error classification; no worker reservation for checks/model waits | PASS for resilience scope |
| Worker crash | Dependency Worker uses empty environment, pipes, kill-on-drop; Checkout Drop discards worker/releases capacity; failed maps crash to operation terminal | Source plus real-worker tests support local containment |
| Running timeout / full pool | Parent timeout_at covers checkout/feed/mount servicing; semaphore refuses beyond running+waiting capacity | Existing timeout/saturation tests support ordinary path |
| Allocation/process memory | Monty allocator cap and parent prlimit(RLIMIT_AS) before feed | Allocator breach/recovery tested; AS enforcement source inspected, distinct AS breach not tested |
| Aggregate memory | Only sampled RSS, not a kernel group limit | FAIL SYS-003-1 |
| External cancellation | Dependency kills worker/releases capacity, but busy registration survives future drop | FAIL SYS-003-2 |
| Repeated writes | Fresh mounts reset counters while /work persists | FAIL SYS-003-3 |
| Concurrent Python near cumulative budget | Calls copy same duration before awaiting and debit afterwards | FAIL SYS-003-4 |
| Unicode flooding | Byte slice can panic in parent callback | FAIL SYS-003-5 |
| Ordinary Sandbox drop | Aborts watchdog; dependency kills owned workers | Source supports normal drop; abrupt parent death/restart and interrupted blocking-IO cleanup remain unqualified by supplied tests |

No full suites, journeys, load generation, or additional harnesses ran in this review. Task evidence claims passing focused tests; orchestrator separately reports passing 9 Agent, 21 hosted unit, 5 actual-worker cases. I inspected those test sources. Setting group ceiling to one byte proves eventual sampled killing, not a hard ceiling. Print test is ASCII; mount test checks one mount quota; saturation test uses completion/timeouts rather than aborting a running future. The workspace cleanup test opens and drops a completed workspace rather than cancelling materialization. These green cases do not close the findings below.

## Material proposed findings

### SYS-003-1 — INCORRECT: sampled memory limit permits a worker fault to reach the shared server

**Obligation:** Deployment MONTY_POOL_MEMORY_MIB is a hard combined ceiling independent of allocator accounting, counting idle workers. Pressure must fail an affected worker without taking down parent. Scenario 5 owns OS group enforcement.

**Location:** crates/wyrd/wyrd-server/src/verification/hosted/sandbox.rs:283–311, construction 80–94, registration 180–190.

**Trace:** Up to eight workers allocate independently under 384 MiB AS ceilings. Parent samples idle/registered-busy RSS every 100 ms and sends one SIGKILL only after exceeding 2 GiB. No kernel group limit or reservation prevents overshoot; spawning/configuring workers are neither idle nor registered busy. Source explicitly acknowledges overshoot and calls hard enforcement an upgrade.

**Consequence:** Workers can exceed the approved group envelope between samples; under pod/host pressure the kernel can kill the shared server before watchdog intervention. Colocated capabilities and pending in-memory outbox share that failure domain. Eventual-kill proof cannot establish hard containment.

**Correction:** Provide actual OS-enforced aggregate worker containment with parent outside that kill/memory domain. Do not shorten sampling or weaken the contract. Unsupported containment must refuse unsafe sandbox capability/work, not require crashing unrelated serving roles. TASK-005 owns container wiring, but concrete owner must support/prove the required mechanism now.

**Closure:** Actual workers, small valid group envelope, normal bounded concurrent allocations; inspect kernel/config evidence of ceiling/parent exclusion, affected operation failure, subsequent healthy operation, and idle/starting-worker inclusion.

### SYS-003-2 — INCORRECT: cancelled calls leave dead PIDs in watchdog kill authority

**Obligation:** Scenario 5 cancellation/recovery and INV-005/007 isolation; release state and fail only affected worker/operation.

**Location:** sandbox.rs:184–190, 234–239; watchdog 293–311.

**Trace:** After busy insertion, an enclosing deadline/abort drops exec during feed/mount IO. Cleanup after await is skipped. Dependency Checkout::drop kills child/releases capacity, but cannot remove caller-maintained BTreeSet entry. It persists for Sandbox lifetime. Watchdog later reads /proc/PID/status and SIGKILLs the raw PID without owned-process fencing.

**Consequence:** Repeated cancellations grow supposedly bounded bookkeeping indefinitely. After PID reuse on a long-lived host, memory pressure can cause counting/killing an unrelated process, violating worker-only fault containment. Correctly killing original child does not close this path.

**Correction:** Bind accounting/termination authority to owned worker lifetime including future cancellation, rather than normal-return cleanup/raw PIDs. Prefer deleting this watchdog authority if hard containment from SYS-003-1 makes it redundant; do not add a second lifecycle owner downstream.

**Closure:** Abort an actual running checkout, establish exit and restored capacity/accounting, repeat bounded cancellations, then healthy calls. If watchdog remains, prove exited-worker identity cannot target a non-owned process.

### SYS-003-3 — INCORRECT: code.run resets cumulative writable quota

**Obligation:** Deployment WORK_WRITE_MAX_MIB is cumulative per invocation (50 MiB); scratch maximum 250 MiB. Scenarios 4–5 require bounded writable workspace across calls.

**Location:** crates/wyrd/wyrd-server/src/verification/hosted/tools.rs:249–278, especially 263–267; workspace.rs:192–200 bounds only materialized input scratch.

**Trace:** /work persists in invocation Workspace. Each Invocation::run creates a fresh ReadWrite MountSpec with full allowance; each feed builds a new mount. Locked monty-fs Mount::with_root initializes write_bytes_used=0. No invocation-owned cumulative writer/scratch counter exists. Input-placement scratch accounting is not updated by tool writes.

**Consequence:** Two tools can each write 30 MiB to different files and exceed 50 MiB cumulative; more calls exceed 250 MiB scratch. Parent-serviced filesystem lies outside worker memory limits and can exhaust shared disk/tmpfs, affecting unrelated tenants/capabilities. Per-mount quota test misses this.

**Correction:** Enforce cumulative successful writes and total scratch through existing invocation/workspace resource owner across its Python calls, including concurrent calls and failures/cancellation after a write. Preserve fresh interpreters/mount containment. A new per-call quota is not the invocation authority.

**Closure:** Real workers, one workspace, small limits; individual writes fit but sum does not; excess rejected before changing files and scratch stays bounded. Include interleaved calls and failed call after successful write.

### SYS-003-4 — INCORRECT: concurrent tools overspend cumulative Python budget

**Obligation:** Cumulative Python-operation wall time per Agent invocation; waiting/execution use remaining budget; spec prohibits resets across tool calls/retries.

**Location:** tools.rs:251–254, 269–283; full initialization per session 117–124.

**Trace:** Existing supported Skald loop runs tools concurrently (loop_runtime.rs:527–663). Each code tool copies same current budget, unlocks, and gets deadline using that unreserved duration. Only completion charges it. Two calls with one second left may both run/wait one second. Later saturating subtraction hides overspend. External cancellation before post-await debit loses charge. Reopening attempt session recreates full budget.

**Consequence:** Checkout bound does not enforce caller resource share; a single invocation exceeds promised cumulative Python envelope and contends with other tenants. This is reachable through actual Agent tools, not only direct executor calls.

**Correction:** Retain one enclosing invocation budget authority across tools/attempts and enforce during admission/execution/cancellation, rather than copying then debiting. Preserve concurrent non-Python tools; changing global Skald concurrency broadens correction. TASK-005 may compose authority, but tooling seam must support retention instead of reset.

**Closure:** Scripted Agent emits concurrent Python tools under small remaining budget; total admitted waiting/execution stays within budget, cancellation does not restore consumed budget, retried attempt receives remaining allowance. Use normal bounded concurrency.

### SYS-003-5 — INCORRECT: Unicode stdout can panic parent verification task

**Obligation:** Bounded output/isolated failure in scenarios 4–5; untrusted output produces bounded result/failure rather than server-task panic.

**Location:** sandbox.rs:194–196.

**Trace:** Worker prints valid UTF-8 but parent slices at arbitrary byte allowance. Default 65536 bytes and print('x' * 65535 + 'é') can produce text[..65536], splitting final character. This panics before BlockError handling/normal PID cleanup. Hosted workspace.rs already has a character-boundary-aware truncate helper.

**Consequence:** Ordinary Unicode output unwinds parent verification task, loses typed result and triggers SYS-003-2 registration leak. Current unwind profiles do not alone prove whole-server death: concrete consequence is parent-task loss rather than worker-contained truncation/refusal.

**Correction:** Truncate prints at UTF-8 boundaries while preserving byte ceiling and normal cleanup. No dependency/protocol change needed.

**Closure:** Actual worker with multibyte text spanning a small output limit returns bounded valid UTF-8 without panic; next sandbox operation succeeds. Cover exact boundary and split-character boundary.

## Recovery limits and assessment

Healthy, timeout, single-worker allocation failure, and simple saturation have credible component evidence. Hard aggregate containment, cancellation accounting, cumulative filesystem/Python budgets, and safe Unicode output are contradicted by source. Intrinsic TASK-003 gaps cannot be deferred to composition/final journeys.

Abrupt parent death remains an unqualified deployment path: kill-on-drop is Rust cleanup, while computing child does not read pipe EOF until current child.handle returns. Worker installs no parent-death signal or independent execution-time ceiling. This report does not claim universal orphan behavior under TASK-005's unwritten container/supervisor lifecycle or prescribe deployment policy. Final composition needs process-tree/restart proof as well as focused fixes above.

Skipped: full service/SDK journeys and new runtime experiments; no source/authority edits. Risk: five proposed material findings require independent validation before acceptance.
