# TASK-003 r4 status — supplemental

Supplements `TASK-003-r4-canonical-support-desk-closeout.md`. Status as of
`740129647` on `wyrd-verification-closeout-task3`, superseded by the
uncommitted r4 closeout. Verdict: **IMPLEMENTED** — `mise run -c gate` exits 0
(2026-10-09); evidence and diagnoses are in the task file's "Implementation
Evidence (r4 closeout)" section. The sections below record the earlier
BLOCKED state.

## Done

| Item | Commit | Evidence |
|---|---|---|
| Observations correlate by `card_uid`; `for_card` injects it; server confirms it against the principal scope, or the tenant registry for unbound keys; duplicate Scribe lookup and server-stamped `card_ref` removed | `dadaa3ab9` | wyrd-spec/wyrd-client/wyrd-queue/bifrost/server lib tests; `py:test:unit`; `ts:test:unit` |
| Real-time verification judge deployed on the gateway; canonical describe exposes `card_uid` as optional text | `005c19b1e` | `verify_in_real_time` Rust/Python/TypeScript 14/14 each |
| Agent docs regenerated | `f6bc0b2f2` | `docs:check` |
| Role membership poller and readiness heartbeat outlive the process drain (temporary lifetime fix; ownership repair remains) | `21d02b174` | `query::oracle_only_pod_retains_audit_staged_before_its_drain` fails with the fix disabled (`lost=1`, retained 0), passes with it |
| Scribe outbox drains within half the remaining shutdown budget, so Bifrost always drains | `740129647` | `app::server::tests::outbox_gets_half_of_the_remaining_shutdown_budget`; `query::configured_default_deadline_is_shared_by_local_and_forwarded_queries` |
| Post-loss audit count waits for the victim to leave membership | `21d02b174` | `query::generated_grpc_and_scheduled_queries_share_audit_terminal_and_cleanup` |

Green lanes: `fmt`, `lints`, `py:format`, `py:lints`, `py:typecheck`,
`check:deps`, `codegen:check`, `ts:typecheck`, `ts:lints`, `ts:format:check`,
`ts:napi:check`, support-desk journeys (Rust, Python, TypeScript),
`test:gateway:journey`, `test:cards:integration`,
`test:principals:integration`, `check:tenant-isolation`, `check:examples`,
`docs:check`, `test:bifrost:journey:drift`, `test:bifrost:journey:server`
(35/35 after `21d02b174`).

## Diagnoses

- **Frozen membership (symptom fixed; ownership open).** Oracle and Scribe ran their snapshot poller
  and readiness heartbeat on the process shutdown token
  (`boot/mod.rs:1325` → `:1085`, `:2076`), which the supervised drain cancels
  first (`app/supervise.rs:302`). Every membership reader on a draining pod —
  Scribe outbox, forwarding, `pin_cut`, peer dispatch, query controls — read a
  frozen roster, and the pod's own row stayed `ready=true` with a frozen
  heartbeat. Each role now owns a `membership` token cancelled in its own
  shutdown (`state.rs`). This keeps the existing journey green, but starts two
  pollers against one process-wide `ClusterRegistry` snapshot when both roles
  run and leaves task ownership spread across role fields.
- **Outbox route depends on Oracle (root cause open).** `ScribeRoute::select`
  obtains `ClusterRegistry` and `BifrostPeerTls` through the Oracle runtime.
  Those are process dependencies already present in `compose_bifrost`; the
  outbox's peer write path has no reason to depend on an Oracle role. Its
  `OnceLock` route and late `ScribeRouteBinding` exist only because the route
  is built after the outbox starts.
- **Audit rows in measured state (open, group A).** `70a6f4a4e` moved audit
  onto the shared Scribe outbox and removed
  `without_audit_publication_for_test()` with nothing replacing it. Audit rows
  now land in process-wide metrics, tenant-wide Forge membership, one-shot
  faults, and Oracle queue slots that tests measure exactly.

## Left

1. **Group A test fixes.** Per independent review: assert on the named table;
   assert process-wide metrics by their aggregate meaning and prove the target
   table count separately; arm one-shot faults at the intended operation (add
   a table-scoped fault only where existing controls cannot). Do not restore a
   general audit opt-out. Prove each failure's cause individually.
   - scribe: `telemetry::scribe_hot_path_telemetry_reconciles`,
     `telemetry::staged_backlog_survives_abrupt_restart`,
     `budgets::scribe_shards_obey_global_and_tenant_budgets`,
     `lifecycle::scribe_tick_finishes_a_claim_that_failed_after_its_commit`,
     `source_boundary_recovery::scribe_failure_retry_replay_remain_atomic`,
     `lifecycle::scribe_shutdown_drains_or_preserves_replay` (not reproduced
     alone)
   - forge: `production_closeout::empty_maintenance_restart_protects_orphans`,
     `production_closeout::restart_recovers_hot_promotion_with_empty_schedule`,
     `live_rewrite::forge_promoted_files_rewrite_and_remain_exact_across_recovery`
   - oracle (suspected): `capacity::queued_tenants_are_granted_fifo_and_rotated`,
     `capacity::memory_refusal_preserves_oracle_health_and_next_query`
2. **Undiagnosed.** `forge live_rewrite::failed_worker_restarts_while_the_api_serves`,
   `oracle analytical_activation::transport_drop_retains_running_status_until_cleanup_joins`,
   `oracle distributed::hot_filtering_mechanisms_cover_all_table_kinds`.
3. **Oracle lane re-run.** The latest run hit `No space left on device` in 7
   `peer_network` tests and did not run 2; that run is invalid. Disk had
   recovered (174 GiB free) afterwards; the cause of the fill is unknown.
4. **Approved root-cause repair: membership lifetime and outbox routing.**
   Implement all of the following as one change:
   - In `cluster/mod.rs`, have `start_readiness_heartbeat(registered, ready)`
     return a handle that owns its cancellation token and task. Each Scribe or
     Oracle role stores and stops its own heartbeat handle at final role
     shutdown or abort. `RegisteredRole` remains a cloneable identity value.
     `deactivate` only publishes `ready=false`; it must not stop the heartbeat,
     which continues fencing the unready role during drain. Rename the
     engine/process cancellation field `role_shutdown` to `process_drain`.
   - Have `start_snapshot_poller()` return one process-owned handle with its
     own token and task. Start it once from `compose_bifrost` for the shared
     `ClusterRegistry`, retain it in `Bifrost`, and stop it after role drain in
     `Bifrost::shutdown` or in `Bifrost::abort`. Remove the per-role pollers,
     their abort handles, and the temporary per-role `membership` tokens.
     The server already drains `ScribeOutbox` before Bifrost shutdown, so its
     membership view remains live for the full bounded outbox drain.
   - In `scribe_outbox.rs`, select the existing `Local` route from the local
     Scribe implementation or the existing `Peer` route from the process
     `ClusterRegistry` and `BifrostPeerTls`. `boot/mod.rs` has those dependencies
     before the outbox is created. Construct `ScribeSink::outbox(route)` with
     the selected route; remove `RouteSlot`, `ScribeRouteBinding`, late binding
     in `Bifrost::assembled`, and Oracle-based route selection. Preserve the
     current Scribe peer channel cache, mTLS identity, RPC, retry identity,
     and accepted in-memory loss window. Update the ownerless test shell and
     direct outbox test constructors explicitly; make `reaches_scribe` inspect
     the selected route rather than the presence of an Oracle.
   - Prove that an Oracle-only pod started before a Scribe-only pod discovers
     the Scribe and delivers staged writes during bounded shutdown. Add a
     focused peer route check with no Oracle runtime, and cover mixed-role
     startup, `deactivate` while heartbeat continues, and shutdown/abort task
     cleanup. Do not add a new TLS pool, RPC, config switch, audit system, or
     general test-only audit opt-out.
5. **Decision: public error.** `CardScopeDenied { card_ref }` /
   `WYRD_VALA_403_BIFROST_CARD_SCOPE` is shared with the gateway; keep or
   rename to match `card_uid`.
6. **Required closeout: Postgres bootstrap wrapper.** Fix
   `scripts/postgres/with-test-postgres.sh` to execute `roles.sql` with the
   running Compose container's `psql`, taking SQL from stdin so no copy or
   host `psql` is needed. Preserve the configured role passwords, stop on SQL
   errors before running the requested command, and retain cleanup and the
   exported host database URLs. Update `scripts/postgres/test-contract.sh` so
   fake Docker proves the container-client invocation, SQL input, failure
   propagation, and cleanup. `scripts/postgres/test-roles.sh` also reruns
   `roles.sql` with host `psql`; make that check host-version-independent
   without dropping its idempotency assertion. Prove the wrapper under a host
   `PATH` whose `psql` is version 14 or a rejecting fake, then run the
   Postgres contract and roles lanes. The `PATH=/opt/homebrew/opt/libpq/bin`
   workaround must no longer be required for repository-managed tests.
7. **Evidence table** in the TASK-003 task file, then `IMPLEMENTED` only after
   the root-cause repair, Postgres wrapper closeout, Scribe/Forge/Oracle lanes,
   and final broad gate pass.
