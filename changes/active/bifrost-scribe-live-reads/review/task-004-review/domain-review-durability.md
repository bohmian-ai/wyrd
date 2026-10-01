# TASK-004 domain review: durability and persistent data

Reviewer role: domain-rev (Scribe WAL, staging, publication, Forge rewrite publication, persisted formats).
Subject: base `a56ab7569` .. candidate `990803fc0` (static review; worktree HEAD `78bd1049b` differs only by an unrelated draft spec).

## Reviewed boundary

The write path end to end. Ingest goes to WAL append/sync, then memtable, then ACK. After that: rotation, `ScribeMemberStager::encode_runs` (D5), `publish_ready`, the staging runtime `register_member`, claim assembly, `publish_claim` with its one transfer chunk, the fenced `file_list`/Iceberg publish, member retire, and WAL retirement.

Also reviewed:
- **Failed stage retry:** `fail_persistence_completion` → `mark_front_retryable` → `retry_pending` / `flush_all` (D7) → `submit_front`.
- **WAL fault:** the `WalWriter` fault token, refusal of appends, deferral of deletions, `ScribeImpl::is_ready`, and the server `run_scribe_wal_fault_monitor`.
- **Restart:** `replay_wal_async`, which runs restore_staging, recover_staged_publications, resume claims, then WAL replay. Its boot handling is in `compose_bifrost`.
- **Shutdown:** shutdown no longer publishes residue.
- **Forge:** the per-attempt governed context, `forge-spill` under `BifrostDataRoot`, and stale-spill clearing.
- **Persisted formats:** the proto edits, the audit `slot_units`, and the migration edits.
- **Journeys:** the three named journeys.

## Authority and source coverage

- **Task TASK-004:**
  - Revision 13 sections "Exact read and write flow", including After ACK.
  - The R13-B deletions table rows for persistence, parquet_writer and memory.
  - The R13-B scenario.
  - "One memory charge contract", including the Forge row and Forge spill.
  - "Scribe failure boundary".
  - Acceptance criteria 1, 3 and 6.
  - Material Stop Conditions.
  - Evidence D5, D7 and D10.
- **Spec rev 24:** INV-001 and the lines on WAL authority and retry (spec.md:314, 440).
- **`architecture/bifrost-design.md`:** append/rotation/staging, assembly and publication, live tail/recovery/shutdown (lines 230–327), and Forge spill (line 713).
- **Migration policy:** `architecture/operations/deployment-and-release.md` §Migration contract and `sql-foundation.md` §Migration, read against verified-change-contract REQ-158 at `0e9c6e98c`. REQ-158 approves editing unshipped migrations.
- **Source read at the candidate:**
  - `scribe/{member_stager.rs, hot_stage.rs (publish_record/transition/retire/recover), staging_runtime.rs, execution_lanes.rs (submit/stage_member), persistence.rs (try_submit/stage_member/publish_claim/close/abort_retained), shards.rs (flush_all/flush_expired/submit_front/retry_pending/mark_front_retryable/fail_persistence_completion/retire_committed), wal.rs (fault, rollback, sync, delete_closed_segments, remove_failed_segment), mod.rs (shutdown/is_ready/wal_fault/flush_staged/replay_wal_async), replay.rs header}`
  - `wyrd-server/src/state.rs` (fault monitor)
  - `boot/mod.rs:611–870, 1270–1300`
  - `boot/data_root.rs`
  - `forge/managed/executor.rs` (`governed_context_for`)
  - `wyrd.v1.proto` diff
  - `wyrd-spec/src/vala/audit_detail.rs` (`slot_units`)
  - every migration diff in the range
  - journeys `scribe/write_read.rs:1079–1230`, `server/owner_inspection.rs:127–255` and `forge/live_rewrite.rs:2052–2200`

## Verified (no finding)

- **ACK ordering is unchanged.** The WAL group sync failure path now faults the writer (`wal.rs` `sync_segments_with_fault` → `mark_faulted`). Every later append returns `IngressClosed` before mutation. A failed append whose rollback succeeds restores the pre-append boundary and does not fault. A failed rollback or failed-segment unlink faults.
- **A faulted WAL keeps its files.** `delete_closed_segments` defers paths and does not delete them. `remove_failed_segment` only removes a segment whose only record failed before any ACK. Nothing touches process health. The server monitor withdraws `advertise_ready`, drains the lifecycle and deactivates the fence. `ScribeImpl::is_ready` includes `!wal.is_faulted()`, so Gate refuses before ACK.
- **D5 residue removal is safe.** `publish_record` is atomic: `.tmp` write, fsync, rename, then fsync of the member and key directories. `transition` replaces the record by rename, so a recorded member never passes through a record-less state. `encode_runs` refuses whenever `member.staged.json` exists, so it never deletes a member that recovery restores. Recovery only restores recorded members.
  - Record-less residue can only come from a failed or cancelled attempt whose rows are still WAL-owned, because the WAL retires only after `publish_ready` returns.
  - In process, one front per seal key is submitted at a time (`submit_front` checks `submitted`). Persistence awaits have no timeout or select, so the only early drop is `abort_retained` at shutdown, after `close()`, when no retry can follow. A detached Rayon encode therefore never races a retry of the same member.
  - Across restarts the member path includes the writer epoch, which is the boot fencing token, so a new-epoch generation cannot collide with an old member.
- **D7 keeps FIFO and never double-submits.** `flush_all` calls `retry_pending` first, which submits only an unsubmitted queue front. A failed completion calls `mark_front_retryable` on that same front.
- **The publish_claim transfer reservation refuses safely.** A refusal leaves the claim outstanding and the members staged, and it cleans only the scratch. Ambiguous commit is still reported through `unacknowledged`.
- **Shutdown publishes nothing.** Residue stays staged and is restored before WAL replay. This matches `bifrost-design.md:323–327`, which this range updates consistently.
- **Restart recovers before ready.** `replay_wal_async` clears `recovery_ready`, restores and reconciles staging, resumes claims, then replays WAL. Boot activates the fence only after that.
- **Forge.** Each attempt builds a fresh `ManagedExecutionContext` over a governed pool view and a `SpillLease` on `forge-spill` (`executor.rs:419–439`). Retry is a later durable task pass after the attempt future ends. `BifrostDataRoot::prepare` clears only the entries of `forge-spill`, and only after `try_lock` succeeds (`data_root.rs:121–149`). WAL, stage and output-scratch contents are never removed.
- **Persisted formats.**
  - `ReserveNodeSlotsRequest` reserves 4 and 5.
  - `PeerContext` keeps `claims_bytes = 2` and reserves 1 and 3 plus their names.
  - `QueryTerminalFrame` field 8 changes enum type, but `QueryClass` keeps the same numbering (0/1/2) as `QueryExecutionPath`, so the wire is compatible.
  - `ScribeProviderCut` reserves 7 and 8 with their names.
  - Audit `BifrostQueryReadDecision.slot_units` is kept and is now always 1.
  - The new migrations (`20260930000000_file_list_unpublished_idx`, `20260601000033_verifier_run_observation_ordinal`) are additive.
  - Edits to existing unshipped migrations and `roles.sql` come from the approved TASK-006 replay and are authorized by VCC REQ-158 ("no Wyrd release has shipped … checksums MAY be reset").
- **R13-B journey (`write_read::acknowledged_rows_survive_stage_pressure_and_restart`).** It proves the ACK of a near-maximum request after WAL sync, and a live read. It proves a bounded flush under a full root that publishes 0, a live read afterwards, and a retry that publishes exactly 300,000 rows. It proves the ACK-time WAL segments are gone and that restart reads back the same rows. It also proves that resending the ACKed batch after restart neither duplicates nor republishes.
- **Forge journey (`forge::live_rewrite::failed_memory_attempt_retries_without_partial_publication`).** It asserts that at least one attempt failed under a full root. It asserts the live cut, snapshot count and committed rewrites are unchanged. It asserts the retry consumes every promoted input exactly once, with one snapshot per committed group and exact public rows.

## Findings

### DUR-1 — MISSING: a failed restart replay takes the whole server down instead of leaving only Scribe unready

- **Violated obligation:**
  - TASK-004 §"Scribe failure boundary" (task lines 307–328): "On a later restart, stage recovery and WAL replay finish before Scribe advertises ready; failed replay leaves only Scribe unready for operator repair. A combined target keeps Oracle queries and the verification worker available if their own dependencies are healthy … A Scribe-only target is unready."
  - Acceptance criterion 1: "The missing TASK-003 WAL boundary is completed: uncertain WAL stops only Scribe …".
- **Location:**
  - `crates/wyrd/wyrd-server/src/boot/mod.rs:845–851` at `990803fc0`:
    ```rust
    if let Err(error) = scribe.replay_wal_async().await {
        ... shutdown_role(...) ...
        return Err(ServerBootError::Scribe(format!(
            "WAL recovery failed before role activation: {error}"
        )));
    }
    ```
  - That error propagates through `compose_bifrost` (`boot/mod.rs:611`) to the server boot caller with `?` (`boot/mod.rs:1278–1297`). No caller turns `ServerBootError::Scribe` into a role-local state.
- **Evidence:**
  - `replay_wal_async` fails on exactly the conditions the WAL fault exists to preserve: corrupt or contradictory WAL, staged-record validation failure, or unknown version (`scribe/mod.rs:2482–2510`; `hot_stage.rs` `recover`/`validate`).
  - The fault path in this range keeps the ambiguous WAL on disk specifically so that "only a restart's replay clears it" (`wal.rs` `mark_faulted`).
  - The only restart coverage, `owner_inspection::scribe_wal_fault_is_role_local`, exercises a successful replay. No test covers a failed one.
  - The task's evidence sections (D1–D19) record no decision deferring this behavior.
- **Observable consequence:** after a WAL integrity fault, a combined Scribe+Oracle+Forge+Eval target restarts into a WAL whose replay fails, and the process exits at boot. Published Oracle reads and the verification worker become unavailable, and the pod crash-loops instead of staying up with Scribe unready for operator repair. Readiness never reports the role-local reason. The process-wide failure boundary the task removed for live faults comes back at the next restart.
- **Required correction (testable):**
  - When stage recovery or WAL replay fails at boot, do not return a boot error. Leave the Scribe role reserved-but-unready or withdrawn, with no fence activation, no write admission, and WAL/staged files untouched.
  - Report a distinct Scribe readiness reason in the `/readyz` body, and continue composing and serving the other selected roles.
  - A Scribe-only target stays up and unready.
  - The shared-governor poison path stays process-terminal.
  - Add a real-server journey, for example in `server/owner_inspection.rs`, that:
    1. corrupts a retained WAL segment or staged record in the durable data root;
    2. restarts the combined target;
    3. asserts `/readyz` is 200 with the Scribe reason set to replay-failed, Oracle published reads succeed, the verification check is ok, and writes are refused;
    4. asserts the WAL and staged files are byte-identical afterwards.

## Verification limits

- This was a static review only. No lanes, Postgres journeys, or builds were run, and the named journeys' pass status relies on the orchestrator's gate run (verification.md).
- **R13-B journey:** it does not assert that the stage attempt under the full root actually ran and failed (no failure counter or error is checked). "Published 0" would also hold if the attempt were skipped. The retry mechanism is pinned at unit level by `shards::tests::flush_all_resubmits_a_retained_generation` and `member_stager::tests::unrecorded_member_residue_is_reclaimed_by_the_retry`, and D5's own history shows the failure did occur. Its restart happens after full publication and WAL retirement, so the journey does not exercise replay of a retained, unpublished generation through D5's residue reclaim. That path is unit-covered only.
- **WAL-fault journey:**
  - `std::fs::read_dir(wal_root)?.next().is_some()` is satisfied by the stream directory alone. Retention of segment files is proven by the unit test `wal::tests::sync_fault_is_sticky_and_keeps_files`, not by the journey.
  - The post-restart `starts_with([1,2,3]) && ends_with([6])` check would not detect duplicated rows.
- **Interpretation of "drains accepted work without deleting WAL or staged files":** after a WAL fault, already-staged members can still publish and then retire through the normal lifecycle. I read "without deleting" as forbidding fault-driven deletion of WAL or unpublished staged authority, which the code honors. Published-member retire is the normal post-commit transition.
- **Out of scope here:** the TASK-008 rename of the audit enum variant `TenantRow` → `TenantFile` in `audit_detail.rs` is left to that task's review. REQ-158 states that nothing has shipped.

## Overall result

FAIL
