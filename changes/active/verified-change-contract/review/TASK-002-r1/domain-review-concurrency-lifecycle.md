# Domain Review: Concurrency, Lifecycle, and Retry Semantics

## Review Boundary

- Immutable subject: base `c8bb490ad814c0c7770cac33ed7779897ff776e4`, candidate `fbfc2591a985b288935180098f892aecdf3b8b49`.
- Domain boundary: the single Bifrost lifetime shared by `WyrdState` clones and all three SDK projections; concurrent/duplicate startup; graceful and ambiguous shutdown; invocation/Card-view immutability; explicit-table schema-cache convergence; shared bounded producer admission; the changed audit-publication lock wait; and Forge enqueue/release clock ownership.
- Caller closure inspected: Rust `WyrdState`, `Run`, `Observe`, `Bifrost`, `WriterPool`, and `Producer`; Python `PyWyrdState`; TypeScript `NativeWyrdState` and public wrapper; the three SDK journeys; `AuditPublisher::freeze`; every `freeze_publication_range` caller; every production `NewForgeTask` constructor; and every `ForgeTasks::retry` caller.

## Authority and Source Coverage

| Concern | Authority | Source and test evidence inspected | Result |
|---|---|---|---|
| One state-owned writer, one successful start, terminal shutdown, same-handle ambiguous retry | spec rev. 32 `REQ-123`, `REQ-126`, `REQ-127`, `REQ-133`; TASK-002 Scenario 1; `run_api.md`; `AGENTS.md` §§6, 11 | `wyrd-client/src/{state.rs,observe/lifecycle.rs,bifrost/{facade.rs,handle.rs}}`; `wyrd-queue/src/producer.rs`; Rust/Python/TS wrappers and journeys; `observe/tests.rs` | **FAIL** (`CONC-001`, `CONC-003`) |
| Immutable invocation/Card views and correlation ownership | `REQ-118`, `REQ-121`, `REQ-123`; `wyrd-design.md` Runtime identity | `observe/mod.rs`, projection modules, SDK wrappers and journeys | PASS |
| Dynamic-table concurrent first use and writer-lifetime schema authority | `REQ-128`; `AC-025`; TASK-002 Scenario 6; `run_api.md` | `bifrost/facade.rs`, `bifrost/table.rs`, `observe/mod.rs`, `observe/tests.rs`, SDK journeys | **FAIL** (`CONC-002`) |
| Bounded shared queue, producer shutdown, and ambiguity retention | `REQ-076`, `REQ-126`; analytical reliability reference; `bifrost-design.md` | `bifrost/handle.rs`, `wyrd-queue/src/producer.rs`, existing backpressure/drain tests | PASS at the underlying pool/producer boundary; state-level retry proof is missing (`CONC-003`) |
| Audit publication lock concurrency | `AGENTS.md` single audit publisher rule; `bifrost-design.md`; analytical reliability reference | `vala-sql/src/queries/audit_staging.rs`; `wyrd-server/src/audit/publication.rs`; SQL and server audit-publication journeys | PASS. The 3-second `lock_timeout` preserves queued `FOR UPDATE` reuse when acquired and yields a sweep cycle on `55P03`; no owner token or second audit path was added. |
| Forge enqueue/release clock and retry ownership | `bifrost-design.md` Forge scheduling/recovery; analytical reliability reference | `vala-sql/src/{queries, row_types}/forge_tasks.rs`; `planning_scheduler.rs`; `worker.rs`; all constructors/callers and PostgreSQL tests | PASS. Immediate enqueue/release now derives eligibility from PostgreSQL's `statement_timestamp()`, matching the fair-claim clock; delayed retry/backoff paths remain database-clock-owned. |

## Verification Limits

- I relied on the immutable candidate source and TASK-002's recorded green command matrix; I did not rerun the broad Cargo/Postgres/language lanes in this shared review checkout.
- Existing lifecycle tests prove sequential duplicate start, failed-start retry, successful started-state shutdown, and no-op shutdown before start. They do not prove concurrent start versus shutdown or same-handle retry after an ambiguous state-owned shutdown.
- Existing dynamic-table tests prove sequential cache reuse only. No reviewed test launches concurrent first writes to the same table and asserts one describe request.
- The audit and Forge changes were traced through production callers. The audit journey exercises a locked tenant and independent-tenant progress, while the Forge production-route tests exercise database-stamped immediate eligibility; neither finding below depends on those adjacent fixes.

## Material Findings

### CONC-001 — INCORRECT — Successful shutdown does not always close the state

- **Violated obligation:** `REQ-133` requires that after a successful shutdown the `WyrdState` is closed to Bifrost writes and cannot restart; TASK-002 Scenario 1 requires permanent closure after successful shutdown.
- **Exact location:** `crates/shared/wyrd-client/src/observe/lifecycle.rs:134-140`; exposed by `crates/shared/wyrd-client/src/state.rs:501-502` and all Python/TypeScript shutdown projections.
- **Evidence:** `BifrostLifecycle::shutdown` converts every error from `started()` into `Ok(())` at lines 135-137. In `Phase::NotStarted`, shutdown therefore succeeds without changing the phase, so `claim()` can immediately transition the same state to `Starting`. In `Phase::Starting`, a concurrent shutdown also returns success; the outstanding `StartClaim::complete` can subsequently publish `Phase::Started`.
- **Observable consequence:** a caller can receive successful graceful shutdown and then successfully start the same state, or can receive shutdown success while a concurrent startup later installs a live writer. The reported durability/terminal lifecycle boundary is false.
- **Required testable correction:** make shutdown decide and transition under the lifecycle mutex. A never-started successful shutdown must transition `NotStarted -> Closed`. A `Starting` state must not be reported successfully shut down while its claim can still publish; synchronize with the in-flight transition or return a stable non-success outcome that requires retry. Preserve `Started` on ambiguous drain failure and move only a successful drain to `Closed`. Add focused tests for shutdown-before-start followed by restart refusal and a controlled concurrent start/shutdown interleaving.

### CONC-002 — INCORRECT — Concurrent first writes do not coalesce the table describe

- **Violated obligation:** `REQ-128` and `AC-025` require concurrent first uses to converge and evidence one first-use describe per dynamic table; TASK-002 Scenario 6 requires cache convergence.
- **Exact location:** `crates/shared/wyrd-client/src/bifrost/facade.rs:304-315`; sequential-only proof at `crates/shared/wyrd-client/src/observe/tests.rs:686-717`.
- **Evidence:** `writer_table` checks the cache under one lock, releases it, performs the HTTP describe, and only then inserts. Every concurrent miss can therefore issue its own describe. The method's own rustdoc at lines 279-282 explicitly acknowledges that concurrent first calls may each describe. The reviewed test performs the two calls sequentially and cannot exercise this race.
- **Observable consequence:** a burst of first observations for one table performs duplicate authenticated catalog reads/audit decisions instead of one first-use schema IO. This violates the locked first-use behavior and makes latency/load scale with the number of racing callers.
- **Required testable correction:** coalesce in-flight resolution by FQN so callers for the same table await one describe and receive the same cached `WriterTable`, while unrelated table names may still resolve independently. Add a barrier-controlled concurrent test asserting one server call, one cached schema, and one pooled producer for the shared FQN.

### CONC-003 — MISSING — The required state-level ambiguous-shutdown retry proof is absent

- **Violated obligation:** `REQ-133`, `AC-028`, and TASK-002 Scenario 1 explicitly require retrying failed/ambiguous shutdown on the same handle without replacement or stranded rows.
- **Exact location:** lifecycle tests in `crates/shared/wyrd-client/src/observe/tests.rs:413-443`; lower-level retry mechanics in `crates/shared/wyrd-client/src/bifrost/handle.rs:267-315` and `crates/shared/wyrd-queue/src/producer.rs:858-887`.
- **Evidence:** the state tests cover only successful shutdown and shutdown before startup. Queue tests cover retained-batch retry inside `Producer`, but no test drives an ambiguous sink outcome through `WyrdState::shutdown`, verifies that lifecycle remains on the original `StartedBifrost`, retries that same state, and proves terminal closure after the retained batch settles. Repository rules state that lower-tier seam coverage does not substitute for the required user-visible lifecycle proof.
- **Observable consequence:** regressions that replace/close the state after ambiguity, lose the retained producer, or fail to close after a successful retry can pass the recorded suite even though the public Rust/Python/TypeScript contract is broken.
- **Required testable correction:** use the existing injected-sink/state test seam to force one retained ambiguous batch, assert the first `WyrdState::shutdown` fails, retry shutdown on the same state/producer and batch identity, assert durable settlement, then assert writes and restart are refused. No new harness is needed.

## Overall Result

**FAIL** — the candidate does not satisfy the terminal `WyrdState` lifecycle or concurrent dynamic-table first-use contract, and it lacks the explicitly required same-handle ambiguous-shutdown proof.
