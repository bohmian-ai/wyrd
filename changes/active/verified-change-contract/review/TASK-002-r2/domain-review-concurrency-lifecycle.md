# Domain Review: Concurrency, Lifecycle, and Retry Semantics

## Review Boundary

- Immutable subject: base `c8bb490ad814c0c7770cac33ed7779897ff776e4`, candidate `a000c201ae86f584fd5b80349f375e087902fd78`.
- Domain boundary: the one `Bifrost` lifetime shared by all clones of a `WyrdState`; ambient and configured startup; fixed-table preflight; terminal shutdown and start/shutdown interleavings; same-handle retry after ambiguous drain; dynamic-table describe convergence; shared bounded producer ownership; audit-publication lock contention; and the cumulative Forge enqueue/release clock-domain correction.
- Caller closure inspected: every production caller of `WyrdState::{start_bifrost,start_bifrost_with,start_bifrost_with_config,shutdown}`, `Bifrost::writer_table`, `freeze_publication_range`, and `ForgeTasks::retry`; every production `NewForgeTask` constructor; the Python and TypeScript lifecycle projections; the Rust/Python/TypeScript journey coverage; and the complete bodies of `BifrostLifecycle`, `StartClaim`, `Bifrost`, `WriterPool`, producer shutdown/control, `AuditPublisher::{sweep,publish_tenant,freeze}`, Forge scheduler construction, and worker cancellation/partial-pass release.

## Authority and Source Coverage

| Concern | Authority | Source and proof inspected | Result |
|---|---|---|---|
| One state-owned writer; default/configured start; fixed-table preflight | spec rev. 32 `REQ-123`, `REQ-127`, `REQ-133`; original TASK-002 Scenario 1; `run_api.md` | `wyrd-client/src/state.rs:420-487`; `observe/lifecycle.rs:80-204`; `bifrost/{facade.rs,mod.rs}`; `wyrd-client/src/lib.rs:26`; Rust/Python/TypeScript bindings and Rust journey configured call at `sdks/wyrd-sdk-rust/tests/observe_run.rs:344-349` | PASS |
| Successful shutdown is terminal across never-started, started, and start/shutdown race states | `REQ-133`; TASK-002 Scenario 1; `run_api.md` lifecycle contract | `observe/lifecycle.rs:121-219`; `state.rs:511-540`; `observe/tests.rs:440-523`; `WriterPool::shutdown` and producer admission/control | PASS |
| Ambiguous shutdown preserves the same writer and retained batch for retry | `REQ-126`, `REQ-133`; `AC-028`; TASK-002 Scenario 1; analytical reliability guidance | `observe/lifecycle.rs:136-150`; `bifrost/facade.rs:466-516`; `bifrost/handle.rs:267-315`; `wyrd-queue/src/producer.rs:709-728`; state-level test `observe/tests.rs:525-597` | PASS |
| Concurrent dynamic-table first use converges without another schema or producer cache | `REQ-128`; `AC-025`; TASK-002 Scenario 6; `run_api.md` | `bifrost/facade.rs:279-345`; `observe/mod.rs:248-260`; `observe/tests.rs:840-921`; `WriterPool::producer_for` | PASS |
| One bounded client budget and terminal producer admission | `REQ-076`, `REQ-126`, `REQ-128`; `bifrost-design.md` resource/shutdown invariants; analytical reliability guidance | `bifrost/handle.rs:28-155,157-230,267-315,332-380`; producer bounded channel, byte budget, retry ownership, and shutdown control | PASS |
| Audit freeze contention reports an honest transaction result and retries unchanged state | repository single-publisher/audit transaction rules; `bifrost-design.md`; analytical reliability guidance | `vala-sql/src/queries/audit_staging.rs:199-287`; sole publisher path `wyrd-server/src/audit/publication.rs:180-269,347-366`; `pg_audit_staging.rs` lock-timeout test | PASS |
| Forge immediate enqueue/release uses the same database clock as fair claim | `bifrost-design.md` Forge scheduling/recovery; analytical reliability guidance | `NewForgeTask::ready_at`; both enqueue statements; every production constructor; `ForgeTasks::retry`; worker cancellation and partial orphan-scan callers; PostgreSQL Forge tests | PASS |

## Verification Limits

- I independently ran the exact focused `wyrd-client` lifecycle/convergence set through `mise exec -- cargo nextest`: `shutdown_without_startup_closes_permanently`, `shutdown_during_start_stays_closed`, `ambiguous_shutdown_retries_the_same_batch_on_the_same_state`, and `concurrent_first_records_describe_each_table_once`. All four passed.
- I did not rerun the live-Postgres audit test, the ignored real SDK journeys, or the broad language/Bifrost lanes in this shared review checkout. I inspected their complete test bodies and relied on the candidate's recorded successful `verify:bifrost`, SQL-backed focused audit test, SDK integration lanes, and format/lint/boundary evidence.
- The owner-wide describe gate intentionally serializes rare cache misses for different FQNs. This is the approved remediation ceiling, stays bounded by callers and network completion, and does not affect cache hits; no evidence makes a per-key subsystem necessary.
- Candidate identity was `a000c201ae86f584fd5b80349f375e087902fd78` before report creation. The only pre-existing worktree change was the untracked TASK-002-r2 review directory used by the independent reviewers.

## Prior-Finding Closure

| Prior finding | Closure evidence | Result |
|---|---|---|
| `FIND-TASK-002-1` | `WyrdState::start_bifrost_with_config(&WyrdClient, Option<TableConfig>, QueueConfig)` now claims the same lifecycle and delegates to `Bifrost::connect_with_config`; the default configured form delegates with `QueueConfig::default()`. The existing `QueueConfig` is re-exported through `wyrd_client` and therefore `wyrd_sdk`, and the Rust real journey compiles and calls the locked signature. | CLOSED |
| `FIND-TASK-002-6` | `Bifrost::writer_table` performs cache check, owner-local async gate, cache recheck, then one describe and insertion into the existing map. The concurrent test drives eight first calls across two FQNs and proves one describe and one producer per FQN. | CLOSED |
| `FIND-TASK-002-7` | Shutdown changes `NotStarted` or `Starting` to `Closed` while holding the lifecycle mutex. `StartClaim::complete` publishes only from `Starting`, and `Drop` rolls back only `Starting`; a racing shutdown therefore cannot be reopened by completion or cancellation. A successful started drain moves to `Closed`, while a failed drain leaves `Started` for retry. | CLOSED |
| `FIND-TASK-002-8` | The state-level ambiguous-drain test enqueues through the production lifecycle, forces the sink's first outcome ambiguous, observes the retained batch identity, retries `shutdown` on the same state until that identity settles exactly once, then proves writes and restart are refused. | CLOSED |
| `FIND-TASK-002-9` | `freeze_publication_range` no longer maps PostgreSQL `55P03` to `Ok(None)`; the sole publisher receives `AuditPublicationError::Staging`, logs it, and retries on the next sweep. The focused SQL test proves the timed-out transaction is dropped, the bound remains absent, and a fresh transaction freezes the unchanged owed range after the holder releases. | CLOSED |

Related cumulative fixes also remain sound: immediate Forge task creation passes `ready_at: None` and both enqueue paths use `COALESCE($12, statement_timestamp())`; non-consuming `ForgeTasks::retry` has no host-clock argument and stamps `statement_timestamp()`. Every production constructor and retry caller follows those APIs, while delayed failures continue to use the database-clock backoff already owned by SQL.

## Proposed Findings

None. I found no reachable defect in the reviewed concurrency, lifecycle, retry, audit-publication, or Forge clock-domain boundary that is required by the approved task and remains uncorrected.

## Overall Result

**PASS** — prior findings `FIND-TASK-002-1`, `FIND-TASK-002-6`, `FIND-TASK-002-7`, `FIND-TASK-002-8`, and `FIND-TASK-002-9` are closed, the cumulative adjacent concurrency fixes preserve their owners and retry identities, and the focused race/lifecycle proof passes.
