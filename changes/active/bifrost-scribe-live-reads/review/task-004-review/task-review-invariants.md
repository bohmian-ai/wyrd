# TASK-004 Review — Invariant Reviewer (invariant-rev)

- Subject: base `a56ab7569` .. immutable candidate `990803fc0`. Worktree HEAD `78bd1049b` adds only an unrelated draft spec, which is excluded.
- Authorities:
  - `changes/active/bifrost-scribe-live-reads/spec.md` (rev 24)
  - `changes/active/bifrost-scribe-live-reads/tasks/TASK-004-integrate-eval-server-and-unify-bifrost-memory.md`
  - `AGENTS.md`
- Method: static review only. All source was read with `git show 990803fc0:<path>`. No builds, mise lanes, Postgres, or benchmarks were run.
- The task's D1–D19 evidence and acceptance tables were treated as claims and checked against source.
- Line numbers refer to the candidate. "~" marks a line that was located in a function body, not pinned to the exact line.

## Invariant traces

### 1. One governor; all-or-nothing charges; single release; no surviving second ledger

- **One governor per process.**
  - `BifrostRuntimeResources` builds the single `BifrostResourceGovernor`.
  - Boot calls `detect_with_transport_message_limit` and `compose_roles` once (`crates/wyrd/wyrd-server/src/boot/mod.rs:556`).
  - `scribe/mod.rs::embedded_scribe_resources` builds a private governor only for embedded and test constructors. Those predate this task, and the server never uses them.
- **Fallible charges are all-or-nothing.**
  - `charge_locked` checks `governed_total + bytes <= managed_memory_bytes` under the state mutex before it mutates anything.
  - `ScribeMemoryLease::grow_locked` (`resources.rs` ~2960–2995) computes every next value first. It calls `charge_locked`, and only then commits the category and shard attribution.
  - `GovernedMemoryView::try_grow` checks the ceiling, then the governor, then the pool. It rolls back in reverse order when a later step fails.
- **Every charge is released once.**
  - `ScribeMemoryLease::release` is guarded by `released`, and `merge` marks the consumed sibling as released.
  - `release_locked` poisons the governor on underflow.
  - Dropping a `GovernedMemoryView` while it still holds bytes poisons the governor.
  - `BifrostTransportLease` releases on Drop. It is held across the gRPC `inner.call`, and HTTP holds it until the response is produced (`grpc/mod.rs` ~270–320, `http/middleware/body_limit.rs` ~110–200).
- **Zero-copy views are not double-charged.**
  - Native ingress resizes the transport-transferred lease to `held_material_bytes` (0 for native).
  - `charge_prepared` then resizes to the real memtable and WAL bytes.
  - Native slices are exact-capacity copies, so the transport body and the memtable bytes are never both counted for the same material.
  - OTLP holds `body_bytes` until `complete()` hands the lease over.
- **Infallible overshoot is recorded and released.**
  - `grow` splits the charge into governed bytes plus `infallible_headroom_bytes`, and `shrink` releases headroom first.
- **No surviving second ledger.** None of these remain in candidate sources:
  - `memory_breaker`, `ContentionLedger`/`Reserve`, `ScribeGlobalCapacity`, `max_active_tables`
  - `maximum_envelope_decision`, `parquet_candidate_incremental`, `EncodedFooterReservation`
  - `ORACLE_METADATA_MEMORY_BYTES`, `FooterSlot`, `try_pending`, `PEER_SLOT_*`, `OracleSlotManager`, `try_acquire_worker`
  - an unmanaged reserve, except in `config.rs` tests that assert it is rejected.

  `ORACLE_PARTITION_MEMORY_BYTES` survives only as a follower ceiling, not as a charge.
- **Exceptions:**
  - **INV-1:** acknowledged Eval wire bodies are retained after their transport charge is released.
  - **INV-2:** a dead byte-wait API survives.

### 2. ACK ordering

- **Normal path.** `prepare_and_dispatch` (`scribe/ingress.rs` ~500–591) runs `preprocess`, then `charge_prepared`, then `shards.try_send`, then awaits the durable ACK. The ACK fires only after the WAL sync and memtable insert.
- **Refusal leaves no WAL record or ACK.**
  - Admission refusal (root charge, or `resize_after_pressure_seal` ~691–727) returns before `try_send`.
  - `cgroup_tripwire_engaged` is consulted only after a root refusal, and it only suppresses the retry.
- **ObservationAck.** `gate/mod.rs:1049` captures the hook and frame. `gate/mod.rs:1079–1080` invokes it only when `admission.first_commit` is set, after Scribe returns success. A duplicate batch or a failed write does not call it.
- **A failed stage retains the WAL.**
  - `shards.rs::mark_front_retryable` (~2852) resets `submitted=false` and keeps WAL authority.
  - `flush_all` (~2003) calls `retry_pending()` (2351) first.
  - `submit_front` (2213) skips fronts that were already submitted.
- **D5 residue removal is safe.**
  - `member_stager.rs::encode_runs` (160–237) refuses when `member.staged.json` exists.
  - Otherwise it removes only unrecorded residue before re-encoding, so recorded members are never deleted.
- **Evidence:** the named tests exist:
  - `ingress.rs:932 decode_to_memtable_transfers_one_charge`
  - `member_stager.rs:560 unrecorded_member_residue_is_reclaimed_by_the_retry`
  - `shards.rs:5181 flush_all_resubmits_a_retained_generation`
  - `wyrd-testing tests/bifrost/scribe/write_read.rs:1109 acknowledged_rows_survive_stage_pressure_and_restart`

### 3. The query slot and its memory return only after children drain

- `oracle/admission.rs::release_physical_projections` (1373) is reached from `analytical_supervisor.rs::retain_admission` (676–700), so the slot is retained until the physical projections drain.
- On a Failed outcome, `drain_children` (1330) runs before the guard's Drop (~1300) (`query_stream.rs::settle_and_finish_stream` ~637).
- No violation was found.

### 4. Storage permit

- There is one `requests: Arc<Semaphore>` (`storage/mod.rs`).
- `acquire_request` (917–930) uses a biased select over owner cancel, `sleep_until(deadline)`, and `acquire_owned`. The wait is therefore bounded by one deadline.
- `attempt_once` (857–905) gets its permit first. It then races owner, deadline, `request_timeout`, and the attempt, so the per-attempt timeout starts after acquisition. The permit is dropped on every arm.
- `governed_decode` (459–505) takes a permit for each attempt, applies `timeout(request_timeout)`, and drops the permit before backing off.
- Tests:
  - `occupied_storage_permit_waits_within_operation_deadline` (1493)
  - `footer_decode_has_no_fixed_memory_slot` (1579)

### 5. Only the leader retries a pre-accept refusal

- `oracle/analytical.rs::reserve` loops over `reserve_round` (~3047–3215).
  - `Err(Some(rejected))` releases every reservation from that round and acknowledges each release. It then calls `dispatcher::wait_for_peer_capacity` (68–81). That wait is cancellable and does not wait when the hint passes the deadline.
  - `Err(None)` covers transport error, timeout, and cancel. It is never retried.
- The receiver only refuses; it does not retry.
- Coverage:
  - `analytical.rs` ~7099 `participant_cut_is_reserved_once_immediately_before_dispatch` calls `assert_refused_round_retries_within_deadline` (6964) and runs the ambiguous and expiry checks.
  - Journey: `wyrd-testing tests/bifrost/oracle/peer_network/analytical.rs:1482 two_leaders_retry_preaccept_capacity`.
- Exception: **INV-3**, stale evidence selector.

### 6. A WAL fault marks only Scribe unready; poison stays terminal

- `WalWriter::mark_faulted` (`scribe/wal.rs:1954`) is called from rollback failure and from `sync_segments_with_fault` (2317–2329). It cancels the fault token.
- `ScribeImpl::is_ready` (`scribe/mod.rs` ~1535–1551) requires `!wal.is_faulted()`.
- The server's `run_scribe_wal_fault_monitor` (`state.rs` ~900–1050) withdraws only Scribe's readiness and advertisement.
- Governor poison still makes `BifrostResourceHealth` terminal for the whole process.
- Tests:
  - `components/health/mod.rs:675 scribe_wal_fault_is_role_local_on_combined_targets`
  - `wyrd-testing tests/bifrost/server/owner_inspection.rs:151 scribe_wal_fault_is_role_local`

### 7. D19 tenant begin

- `crates/wyrd/wyrd-sql/src/tenant_conn.rs::TenantConn::acquire` calls `pool.begin_with(AssertSqlSafe("SELECT set_config('app.current_tenant', '<uuid>', true); BEGIN"))`.
  - The `true` argument makes the binding transaction-local.
  - Because the bind and `BEGIN` travel in one round trip, a failed bind never opens the transaction, so no aborted transaction is left on the pooled connection.
  - The interpolated value is a typed `DataTenantId` rendered through `Uuid` Display, so it cannot be injected.
- A Database error maps to `TxFailed`; any other error maps to `Connect`.
- `scripts/check_tenant_isolation.py` (24–27, 423–424) exempts only `tenant_conn.rs` for raw transaction control.
- Tests: `begin_tenant_sql_binds_then_begins` and `failed_bind_rolls_back_and_the_connection_stays_usable`. The second returns early when `WYRD_DATABASE_URL` is unset, so it proves nothing without the Postgres lane. The orchestrator's gate run covers that lane.

### 8. Forge spill

- `boot/data_root.rs::prepare` (95–200) creates the paths, takes `try_lock`, and only then calls `clear_directory(forge_spill)`, so stale cleanup happens under the lock. Write probes follow.
- Boot passes `spill_root: data_root.forge_spill()` (`boot/mod.rs:913`). This runs before Forge is composed, so cleanup precedes any Forge admission.
- `forge/managed/executor.rs::governed_context_for` (~395–440) uses `SpillLease::new(spill_root)` and `resources.rewrite_memory_pool()`, a view at the shared cap with holder Forge. There is no Forge-specific scratch cap.
- Test: `data_root.rs:295 prepare_clears_stale_forge_spill`.

## Findings

### INV-1 — INCORRECT: acknowledged Eval wire bodies are retained after their transport charge is released, uncharged to the one root

- **Violated obligation:**
  - Task Transport row (TASK-004 line 289): "charge the actual encoded bytes held … to the shared root. HTTP and gRPC body owners return that charge when the body is consumed or dropped."
  - Locked invariant: one governor ledger accounts for retained Bifrost body bytes.
  - TASK-004 acceptance item 3 (line 516) requires untracked allocations to be "documented honestly". This retention is neither charged nor documented.
- **Where:**
  - `crates/vala/vala-bifrost-redux/src/gate/mod.rs:1049` captures `frame.arrow_ipc.clone()`, a zero-copy `Bytes` reference to the transport body.
  - `gate/mod.rs:1080` hands that reference to `hook.acknowledged(...)`.
  - `crates/wyrd/wyrd-server/src/verification/observations.rs:111–118`: `acknowledged` spawns a tracked task that owns `frame: Bytes` until a Postgres tenant transaction, the per-key `enqueue_observation` calls, and the commit complete. Decoding happens inside that task (`observations.rs:68`).
  - `observations.rs:26` sets `PENDING_LIMIT = 256`.
  - Both of these came from subject commit `4ae6a1992`.
- **Evidence:**
  - The `BifrostTransportLease` (HTTP `body_limit.rs`, gRPC `grpc/mod.rs`) drops when the request returns, which is right after the durable ACK.
  - The spawned task still references the same body allocation through the `Bytes` clone. The lease does not move into it, and nothing charges it.
  - Each frame may be up to `max_frame_bytes = BIFROST_INGEST_REQUEST_LIMIT_BYTES = 16 MiB` (`gate/limits.rs:13,259`).
  - The backlog can therefore keep up to 256 × 16 MiB ≈ 4 GiB live outside the governor.
- **Consequence:**
  - Under Eval ingest while Postgres is slow, enqueue tasks pile up. The governor then reports the transport and root charges as free while up to about 4 GiB of request bodies stay resident.
  - That exceeds the 1-GiB default server headroom (Scenario 3), so the process can be OOM-killed while the governor says it has room. This is the failure mode the single ledger exists to prevent.
- **Required correction (testable):**
  - Either decode the `EvalObservationsTable::acknowledged` keys synchronously in `ObservationAck::acknowledged` before the spawn and move only the small keys into the task, dropping the body there,
  - or carry a governor charge (or the transport lease) into the task and release it when the task finishes.
  - Add a test:
    1. Hold an enqueue pending, for example with a blocked Postgres or a stub queue.
    2. Assert that the process can no longer reach the body bytes (keys-only), or that transport plus governed bytes still include them.
    3. Assert they return to 0 once the task completes.

### INV-2 — DRIFT: the producer byte-wait API survives with no production caller

- **Violated obligation:** TASK-004 line 96 says to "Delete … producer byte-based waiting on that estimate." R13-B (REFACTOR) requires one actual-byte owner with no second waiting mechanism.
- **Where:**
  - `crates/vala/vala-bifrost-redux/src/resources.rs:1494–1513`: `ScribeResources::memory_epoch` and `pub async fn wait_for_memory_change`.
  - `resources.rs:2406`: the governor's `wait_for_memory_change`.
  - The `memory_changed` notifications and `memory_epoch` bumps, for example in `resize_with_limit` and `release`.
- **Evidence:**
  - `git grep 'wait_for_memory_change\|memory_epoch()' 990803fc0 -- crates` finds only the definitions and the test `accepted_replay_capacity_wait_is_bounded_and_cancellation_safe` (`resources.rs:5899–5911`).
  - At base, the callers were `scribe/persistence.rs:1719,1757`, the producer byte-wait that this task deleted.
- **Consequence:**
  - A public async wait primitive and its epoch and notify bookkeeping stay on every resize and release with no consumer.
  - It invites the deleted byte-wait admission to be reintroduced, and it is still documented as an admission contract ("The caller must retry root admission after every successful wake").
- **Required correction (testable):**
  - Delete `memory_epoch`, `wait_for_memory_change`, the governor's epoch and notify state, and the test that only exercises them.
  - Alternatively, name and test a production waiter that the spec requires.
  - A `git grep` for these symbols should then return nothing outside the code being deleted.

### INV-3 — DRIFT (evidence): the R13-D acceptance row cites a deleted test

- **Violated obligation:**
  - TASK-004 acceptance item 6 (line ~520), "only leaders retry explicit pre-accept peer capacity", must be backed by credible verification.
  - AGENTS.md §11: "Every specifically named … test … must also include and run its exact focused command."
- **Where:** TASK-004 lines 202 and 761 cite `oracle::dispatcher::tests::leader_retries_only_preaccept_peer_capacity` as the focused command and mark it PASS.
- **Evidence:**
  - `git grep leader_retries_only_preaccept 990803fc0 -- crates` returns nothing. Commit `1a6ec2554` (D10) deleted the test.
  - The `-E 'test(=…)'` selector at line 202 therefore matches no test.
  - The behavior is actually covered by `oracle::analytical::tests::participant_cut_is_reserved_once_immediately_before_dispatch` and by the journey `peer_network::analytical::two_leaders_retry_preaccept_capacity`.
- **Consequence:** the recorded PASS evidence for R13-D cannot be reproduced. Rerunning the cited command proves nothing. The behavior itself is implemented correctly; see trace 5.
- **Required correction (testable):** replace the selector in TASK-004 lines 202 and 761 with `oracle::analytical::tests::participant_cut_is_reserved_once_immediately_before_dispatch`, rerun it, and record the result.

## Acceptance matrix

| Requirement/criterion/constraint/non-goal | Implementation evidence | Verification evidence | PASS/FAIL |
|---|---|---|---|
| One BifrostResourceGovernor per process (Scenario 3, AC-013) | `boot/mod.rs:556` single detect+compose; `BifrostRuntimeResources` | Boot tests (`boot/mod.rs` ~2400); static trace | PASS |
| Fallible charges all-or-nothing | `charge_locked`; `ScribeMemoryLease::grow_locked`; `GovernedMemoryView::try_grow` rollback | `scribe/admission.rs:350 one_root_charge_has_no_secondary_memory_ceiling` | PASS |
| No surviving second ledger, breaker, precharge, role floor, Forge estimate, or transport aggregate | Survivor grep clean (trace 1) | grep at candidate | PASS |
| Every charge released once; drop/underflow poisons | `release` guarded by `released`; `release_locked` poison; view Drop poison | resources.rs unit tests | PASS |
| Zero-copy views not double-charged | `held_material_bytes` (native 0); `charge_prepared` resize | `ingress.rs:932 decode_to_memtable_transfers_one_charge` | PASS |
| Transport charges actual held bytes and returns them when consumed or dropped | `gate/limits.rs`, `grpc/mod.rs`, `body_limit.rs` leases | gate/limits tests | **FAIL (INV-1)**: Eval ack retains the body after release |
| Infallible overshoot recorded and released | `grow` headroom split; `shrink` releases headroom first | resources.rs tests | PASS |
| Producer byte-based waiting deleted (TASK-004 line 96, R13-B) | persistence.rs waiters removed | grep | **FAIL (INV-2)**: API and governor epoch survive with a test-only caller |
| ACK after WAL sync and memtable insert; refusal leaves no WAL record or ACK | `ingress.rs` ~500–591, ~691–727 | ingress tests; `write_read.rs:1109` journey | PASS |
| ObservationAck only after first committed Eval batch | `gate/mod.rs:1079–1080` | gate first_commit tests (`gate/mod.rs` ~1492/1660/1716 fixtures) | PASS |
| Failed stage retains WAL; D5 residue; D7 flush_all retry_pending | `shards.rs` 2003/2213/2351/2852; `member_stager.rs` 160–237 | `shards.rs:5181`, `member_stager.rs:560`, `write_read.rs:1109` | PASS |
| Query slot and memory return after children drain (D13) | `admission.rs:1373`; `analytical_supervisor.rs` 676–700 | Oracle admission tests | PASS |
| One storage semaphore; bounded wait; per-attempt timeout after acquire; permit dropped on every exit (R13-C) | `storage/mod.rs` 459–505, 857–905, 917–930 | `storage/mod.rs:1493`, `:1579` | PASS |
| Leader-only retry of pre-accept refusal; round reservations released; ambiguous never retried (R13-D) | `analytical.rs` ~3047–3215; `dispatcher.rs` 68–81 | `analytical.rs` ~7099 test; journey `analytical.rs:1482` | PASS (behavior); **evidence FAIL (INV-3)** |
| WAL fault marks only Scribe unready; poison process-terminal | `wal.rs:1954`, 2317–2329; `scribe/mod.rs` ~1535; `state.rs` ~900–1050 | `health/mod.rs:675`; `owner_inspection.rs:151` | PASS |
| D19 transaction-local tenant bind, no aborted tx on pool, injection-safe | `tenant_conn.rs` `begin_with` + typed UUID | `begin_tenant_sql_binds_then_begins`; Postgres-gated rollback test (gate lane) | PASS |
| Forge spill confined to `forge-spill` under the data root; stale cleanup under the lock before Forge admits work | `data_root.rs` 95–200; `boot/mod.rs:913`; `executor.rs` ~395–440 | `data_root.rs:295 prepare_clears_stale_forge_spill` | PASS |
| Default 8 GiB → 1 GiB headroom / 7 GiB cap; invalid overrides rejected (Scenario 3) | `shared_memory_cap`; `validate_scribe_expanded_request` (`boot/mod.rs:469`) | `boot/mod.rs` ~2400 tests | PASS |
| Untracked allocations documented honestly (acceptance item 3) | — | — | **FAIL (INV-1)**: Eval body retention neither charged nor documented |
| Non-goal: no Forge-specific scratch cap | Forge uses shared-cap view only | static trace | PASS |

## Ponytail ladder on the changed abstractions

- **GovernedMemoryRoot, GovernedMemoryView, ScribeMemoryLease, transport lease:** each is the one owner for its holder. They are justified at rung 7, and the second ledgers were deleted rather than wrapped.
- **`memory_epoch` / `wait_for_memory_change`:** rung 1 fails; the code does not need to exist (INV-2).
- **`BifrostResourcePoisonReason::Volume`:** the variant is no longer produced. It belongs with INV-2's dead-code cleanup, but alone it is not reportable.
- **`ingress_sublimit_exceeded` / "ingress sublimit" wording in `grow_locked`:** the limit passed is `managed_memory_bytes` (`resize_ingress`), so it is the root cap under a stale name. It is used only for a metric label. This is a naming residue only, not a finding.
- **`TenantConn` single-round-trip begin:** rung 6. It is one statement, and its exemption in the isolation check is scoped to one file.
- **`dispatcher::wait_for_peer_capacity`:** a minimal cancellable sleep that reuses the caller's deadline.

## Overall result

FAIL
