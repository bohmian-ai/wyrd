# TASK-003 resource and concurrency domain review

**Result: FAIL.** Immutable candidate `57c69c3579e4c331e8d3258e00b22ee7aa0155cb`, base `41b0f01d9b47d9b07b078b2431fcdd4cba56a9b0`.

## Boundary and coverage

Reviewed process-local workspace admission, shared Python pool admission and lifecycle, cancellation, deadlines, memory, output and scratch accounting, and concurrent Agent Python operations. Authority: supplied AGENTS.md, architecture/agent-rules.md, design/doctrine and reference routing, task-review domain instructions, approved revision-6 spec snapshot (especially Deployment limits and INV-005/007), and original TASK-003 scenarios 2, 4, 5 and 6. CodeGraph navigation preceded source inspection.

| Surface | Evidence inspected | Assessment |
|---|---|---|
| Pool ownership/admission | hosted/sandbox.rs, HostedLimits, real-worker integration tests; installed monty-pool 1.1.0 pool/checkout/worker sources | Shared injected owner; max workers and total admitted operations bounded; actual checkout waits use caller/operation deadline; dependency default checkout timeout is None |
| Fresh sessions and worker loss | Sandbox::exec; Checkout::drop/release_worker; worker binary serve/configure/allocation handling | A dropped checkout kills its process and releases capacity; one checkout per worker prevents interpreter reuse |
| Memory | Sandbox::cap/watch, worker allocator, installed monty-types resource.rs | Allocation budget and process address-space cap exist; aggregate ceiling is not hard (RES-01) |
| Agent parallel tools and budgets | HostedTools::open, Invocation::run, AgentExecutor::invoke; existing Skald loop_runtime.rs and run.rs | No worker held through model waits; concurrent execution and lifetime accounting violate cumulative limits (RES-02/03) |
| Workspaces | Workspaces::open, Workspace::materialize/place/archive/read/search/list; workspace tests | Semaphore permit belongs to materialization/workspace and is released on failure/drop; workspace lifetime bounds files; cumulative work/scratch enforcement fails (RES-02) |
| Parent outputs | sandbox print sink, Invocation::run, RecordedTool::invoke, Workspace::list/read/search | Output cap is late for several collectors; Unicode print truncation can panic (RES-05/06) |
| Proof | Original task's evidence and actual source of five worker tests, hosted tool/workspace tests | Existing worker tests prove healthy isolated execution, ordinary memory/time refusal, one-worker/one-waiter admission and subsequent recovery; do not cover findings below |

Inspected pinned dependency source directly under `/home/thorrester/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`; no assumption about an undocumented Monty API is needed. Final server wiring, startup environment parsing, background queue coordination and SDK journeys belong to TASK-005 and are not findings in this report.

## Material proposed findings

### RES-01 — VIOLATION: sampled RSS is substituted for the approved hard group ceiling

**Location:** `crates/wyrd/wyrd-server/src/verification/hosted/sandbox.rs:283-311` (and start at 90-94).

**Obligation:** Deployment limits require a **hard combined** 2-GiB ceiling across all workers, independently of allocator accounting, including idle workers. TASK-003 scenario 5 requires actual OS memory enforcement and says to stop when the selected runtime cannot meet it.

**Evidence and consequence:** `watch` samples every 100 ms and kills one largest busy PID only after summed RSS exceeds the ceiling. It cannot prevent the group crossing the ceiling between samples or while the parent is delayed. Eight allowed workers can exceed 2 GiB before any kill, despite their independent 384-MiB limits. There is no group OS limit in the worker, PoolConfig or Sandbox. The implementation comment explicitly acknowledges this overshoot; its task evidence does not authorize relaxing the spec. A test with a one-byte threshold and an infinite loop establishes eventual killing, not a hard ceiling. Shared server availability can be lost to host/container memory pressure before the sampler recovers.

**Required correction/proof:** Enforce the approved limit at the OS-owned worker-group boundary, including idle/new workers before they can execute, and refuse unsupported enforcement rather than silently using sampled enforcement. Keep failures local to affected Python operations. Prove aggregate pressure with multiple actual workers and continued parent availability; retain independent process/allocation bounds. A new configurable architecture is unnecessary: the hard boundary is already approved.

### RES-02 — INCORRECT: writable-work accounting resets on every Python call and ignores remaining scratch

**Location:** `crates/wyrd/wyrd-server/src/verification/hosted/tools.rs:259-267`; `workspace.rs:181-201`.

**Obligation:** `WORK_WRITE_MAX_MIB` is cumulative per invocation (50 MiB), and scratch is at most 250 MiB per invocation; `/work` persists across that invocation's tool calls.

**Evidence and consequence:** Each `Invocation::run` constructs a fresh writable MountSpec with the full write limit. Installed `monty-pool` checkout.rs:2236-2246 constructs a fresh `monty_fs::Mount::with_root`, whose mount_table.rs:267-273 initializes `write_bytes_used` to zero. Thus two successive code.run calls can each create different 40-MiB files: both succeed under the default 50-MiB mount limit but cumulatively write 80 MiB. Parallel calls receive independent full allowances too. Workspace `scratch` exists only as a materialization-local counter; no retained remaining allowance is consulted for `/work`. A near-limit unpacked input workspace can exceed 250 MiB after even its first otherwise-legal work write. Disk usage can therefore grow beyond both approved bounds through the ordinary Agent loop.

**Required correction/proof:** Retain cumulative write and total scratch authority on the invocation/workspace that survives tool calls; parent-serviced writes must charge the shared allowance atomically before committing, including concurrent calls. Reuse Monty's capability-scoped filesystem dispatch while preserving fresh interpreter sessions and read-only inputs. Prove successive and concurrent writes cannot collectively exceed the allowance, and inputs plus work cannot exceed scratch. Checking each result after a write is insufficient.

### RES-03 — INCORRECT: concurrent Python calls overspend the same remaining time budget

**Location:** `crates/wyrd/wyrd-server/src/verification/hosted/tools.rs:246-283`; reset at `tools.rs:123`.

**Obligation:** Cumulative Python-operation wall time within one Agent invocation is 60 seconds by default, includes waiting, and does not reset across tool calls or retries.

**Evidence and consequence:** `run` reads `code_budget` under a short lock, releases it across execution, and subtracts elapsed only afterwards. The existing Skald loop explicitly runs tools through `buffer_unordered(concurrency_cap)` (loop_runtime.rs:661), so this is a reachable producer, not hypothetical direct concurrent API use. With one second left, two code.run calls both read one second and both run up to that amount; saturating subtraction conceals the excess. With default limits a batch of eight near-ten-second operations can spend approximately 80 cumulative seconds against a 60-second budget. `HostedTools::open` also creates a fresh full budget for each attempt, so the seam does not preserve any spent amount when the caller retries the enclosing invocation.

**Required correction/proof:** Own one cumulative budget at the enclosing invocation lifetime and account/reserve it atomically for concurrent operations, charging checkout waiting and preserving charges on cancellation/failure. Preserve the existing bounded Skald tool loop and shared pool. Prove concurrent calls starting near exhaustion cannot overspend, and reopening an attempt for the same invocation does not restore spent budget. Do not solve this by globally serializing unrelated tenants or by reserving workers during model calls.

### RES-04 — INCORRECT: cancellation leaks checked-out PIDs into the process-wide watchdog registry

**Location:** `crates/wyrd/wyrd-server/src/verification/hosted/sandbox.rs:184-190, 199-239, 293-311`.

**Obligation:** Cancellation must clean up invocation resources and process-wide accounting; worker failures must remain isolated.

**Evidence and consequence:** PID insertion precedes the await of the feed/resume loop, whereas removal is a normal continuation after that await. Dropping the outer exec future while running skips removal. Monty's `Checkout::drop` correctly kills the worker and frees pool capacity, but knows nothing about Wyrd's `busy` set. The semaphore permit releases, allowing arbitrarily many later cancellations to accumulate stale PIDs. The watchdog clones/scans that retained set every 100 ms. This is persistent per-process memory/IO growth; if the OS reuses a stale PID, the watchdog may account or signal a process that is no longer the tracked checkout. The same missing cleanup applies when the print callback unwinds.

**Required correction/proof:** Tie busy registration to checkout lifetime with unconditional drop cleanup (or remove this duplicate PID authority if the final group enforcement obviates it). Prove dropping an actual running exec removes its registration and frees capacity, followed by successful execution; include repeated cancellation. Do not patch only timeout/error branches because cancellation bypasses them.

### RES-05 — INCORRECT: the bounded print sink panics on valid non-ASCII output

**Location:** `crates/wyrd/wyrd-server/src/verification/hosted/sandbox.rs:194-196`.

**Obligation:** Output flooding must remain safely bounded and sandbox-originated behavior must not crash the parent execution path.

**Evidence and consequence:** `&text[..min(...)]` slices arbitrary UTF-8 at a byte offset. Under a 64-byte cap (already used by the real-worker test), `print('€' * 30)` produces a fragment whose byte 64 is inside a character; the parent callback panics. At the default 65536-byte cap the same failure occurs with a sufficiently large euro-character print. Installed Monty callback handling forwards a `&str` fragment without promising that the host's arbitrary byte cap is a character boundary. This fails in parent Rust, rather than becoming a bounded Python result, and also reaches RES-04's leaked registration path. Existing print tests use ASCII only.

**Required correction/proof:** Truncate at a UTF-8 character boundary using the existing workspace truncation behavior/pattern or equivalent minimal local logic. Exercise actual worker Unicode prints spanning the cap, assert a bounded valid string and successful subsequent execution without panic.

### RES-06 — INCORRECT: tool output is bounded after allocating the complete result

**Location:** `crates/wyrd/wyrd-server/src/verification/hosted/tools.rs:285`; `crates/vala/vala-eval/src/blocks/tools.rs:172-188`; `workspace.rs:391-412`.

**Obligation:** Deployment TOOL_RESULT_MAX_KIB is a maximum bounded **while collecting**, and parent output buffers need separate bounds from worker memory.

**Evidence and consequence:** CodeRun constructs the entire `value.py_repr()` and its JSON object; RecordedTool then serializes the whole object into a new Vec before checking length. A code.run last expression such as `'x' * (10 * 1024 * 1024)` makes multi-megabyte parent strings/buffers before the default 64-KiB refusal. Independently fs.list collects all matching input/work paths into Vec<Value> with no collection limit; work code can create many zero-byte files (byte-write accounting does not limit that count). The final recording guard prevents sending the oversized result to a model but does not bound collection memory, the explicit requirement. This affects sibling tools under the same session, so fixing only prints or only the final serializer does not close it.

**Required correction/proof:** Bound each hosted result collector at its producer before retaining/formatting beyond the configured serialized-result allowance, with bounded representation/encoding for Python results and bounded listing. Retain the shared final refusal as a boundary check. Prove large Python values and large directory listings return a bounded result/refusal without first constructing the full result; also retain escaped JSON/Unicode coverage since text byte length is not serialized byte length.

## Verification limits and non-findings

This was a static immutable-source audit; I did not rerun tests or edit production/test source. The orchestrator reported passing 9 Agent, 21 hosted-unit and 5 real-worker cases during this review. Those tests do not exercise aggregate hard memory enforcement, cumulative/concurrent work/time budgets, running-future cancellation accounting, non-ASCII print truncation, or producer-bounded repr/list collection. These are source-backed gaps, not an inference that reported healthy cases failed.

The pool dependency has no independent one-second checkout timeout by default, so no such finding is proposed. Startup prewarming is present; steady-state warm replenishment is not elevated to a new requirement. RLIMIT_AS is independently enforced and is stricter than an RSS-only process limit; no claim that it is absent is made. Workspaces hold permits during detached blocking materialization and drop their TempDir once that work returns; immediate forced cancellation of an OS-blocked thread is not demanded. Server wiring/capacity publication is explicitly left for TASK-005.

Skipped broad service suites and live host-pressure experiments; no production files changed. Remaining risk is acceptance of the six material resource/isolation failures above.
