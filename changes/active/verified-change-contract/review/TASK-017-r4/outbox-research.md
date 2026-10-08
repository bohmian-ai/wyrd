# Outbox research: Scribe-bound server writes

This is research only and makes no recommendation. Paths are relative to the
repository root. Abbreviations used below:

- `srv/` = `crates/wyrd/wyrd-server/src/`
- `wsql/` = `crates/wyrd/wyrd-sql/`
- `vsql/` = `crates/vala/vala-sql/`
- `redux/` = `crates/vala/vala-bifrost-redux/src/`

Branch: `wyrd/verified-change-contract/TASK-017` at `b2d118450`.

## 0. Corrections to the handoff table

- **Eval run requests are not a Scribe write.** `ObservationRunSink` inserts
  into `wyrd.verifier_runs` in Postgres (`srv/verification/observations.rs:89-100`).
  It runs *after* a Scribe ack. It is a Scribe → Postgres hop, the reverse
  direction of the target pattern.
- **Audit has two queues, not one.** Events go into an in-memory
  `Outbox<AuditSink>`, then into Postgres `vala.audit_staging`, then through
  `AuditPublisher` into Scribe. The in-memory stage loses events on a crash
  (`vsql/src/audit_outbox.rs:1-16`; `crates/shared/wyrd-runtime/src/outbox.rs:18-22`).
- **Result `batch_id`s are random, not derived.** `ResultPayload::encode`
  mints each one with `Uuid::now_v7()` (`srv/verification/results.rs:142-162`).
  The ids only become stable because `store_result` persists them.
- **Peer delivery needs an Oracle on the sending pod.** `CaptureRoute::Peer`
  takes its TLS identity and roster from
  `oracle.lifecycle_transport().peer_tls()` and `oracle.cluster()`
  (`srv/components/gateway/capture.rs:796-810`). A pod with no Scribe and no
  Oracle gets `Unavailable`.
- **The only server-owned `ingest_frame` callers** are:
  - `AuditPublisher` (`srv/audit/publication.rs:353`)
  - `GatewayCapture::submit` (`capture.rs:964-974`), used by both capture and
    `write_result`
  - the peer receiver (`srv/grpc/capture_peer.rs:118`)

  The Gate OTLP and native paths (`redux/gate/mod.rs:923, 1018`) are
  client-authenticated ingest. `FaultScribe` (`srv/verification/fault.rs:88-115`)
  is a test wrapper. Forge, scribe_tail, Oracle forwarding, workflow runs and
  the drift fitter never call `ingest_frame`.
- **Two other outbox-like types exist; neither is a reuse candidate.**
  `crates/shared/wyrd-queue` is a client-tier write buffer. Its consumers are
  `wyrd-client` (`crates/shared/wyrd-queue/src/lib.rs:1-17`), so server use
  would cross the tier boundary in AGENTS.md §2. The second is
  `wyrd_runtime::outbox` (§5).

## 1. Per-writer table

| Writer | Producer | Current outbox / staging | Consumer | Route | Cleanup | Loss behavior |
|---|---|---|---|---|---|---|
| Audit | Every audited surface calls `AuditOutbox::stage` (`vsql/src/audit_outbox.rs:3-10`). The process instance is built at `srv/boot/mod.rs:1060`. | (1) An in-memory `Outbox<AuditSink>` (`audit_outbox.rs:35, 61-63`). (2) Postgres `vala.audit_staging`, written through the hash-chained append (`vsql/src/queries/audit_staging.rs:70`). Progress is tracked in `vala.audit_publication`. | `AuditPublisher` freeze → publish → settle (`srv/audit/publication.rs:290-304`), spawned at `srv/app/server.rs:604-617`. | **Local only.** `from_state` returns `None` without a local Scribe or an operator pool (`publication.rs:133-149`). | `settle_publication` deletes `seq <= seq_hi`, advances the watermark, and clears the bound, all in one transaction (`audit_staging.rs:503-532`). | In-memory stage: lost on a hard kill or at the shutdown deadline, counted in `outbox_events_lost_total{outbox="audit"}` (`outbox.rs:18-22, 475`). An unresolvable commit status is also counted as lost (`audit_outbox.rs:100-110`). Staging onward: none lost; retried every 5 s (`publication.rs:52-58`). |
| Gateway capture | `GatewayInvocation::capture` (`srv/components/gateway/invocation.rs:806-835`), after the call settles. | **None.** The writer "holds no tenant state, queue, or backlog" (`capture.rs:778-783`). Payload objects are written to object storage first (`capture.rs:826-829, 856-861`). | `GatewayCapture::publish` → `deliver` → `until_deadline(submit)` (`capture.rs:837-866, 1004-1024`). | `Local`, `Peer`, or `Unavailable` (`capture.rs:768-776, 796-810`). | Nothing to clean up. Orphaned objects "are left for the bucket lifecycle to expire" (`capture.rs:827-829`). | Dropped once the call deadline passes, or immediately on a terminal refusal. Counted in `wyrd_gateway_capture_total{outcome}` (`capture.rs:977-990`). The whole capture is also capped by the deadline at `invocation.rs:832-834`. |
| Eval observations | Client SDK → Gate → Scribe (`redux/gate/mod.rs:1003-1018`). | Not a server write. | — | — | — | — |
| Eval run requests | Gate's ack hook `ObservationEnqueue::acknowledged` (`srv/verification/observations.rs:126-155`). | In-memory `Outbox<ObservationRunSink>` (`observations.rs:55-63`), built at `srv/boot/mod.rs:1145`. | `enqueue_observation_batch` → INSERT `wyrd.verifier_runs` with `ON CONFLICT DO NOTHING` (`wsql/src/queries/verifier_runs.rs:1690-1735`). | Postgres, not Scribe. | Not applicable. | Lost on a hard kill or at the shutdown deadline, counted under `eval_run_requests` (`observations.rs:13-17, 81-88`). This loss is accepted by `architecture/wyrd-design.md:1472-1478`. A frame that fails to decode is counted as lost (`observations.rs:144-153`). |
| Verifier results (queued runs) | `VerifierRunner::produce` → `stage` (`srv/verification/runner.rs:392-428, 866-929`). | Postgres `wyrd.verifier_run_results`, one row per run holding the IPC payloads and batch ids (`wsql/migrations/20261003000000_verifier_run_results.sql:11-29`). `store_result` writes it under the lease fence (`verifier_runs.rs:1411-1450`). | `runner.publish` → `GatewayCapture::write_result` per batch, details before summary (`runner.rs:494-550`; `capture.rs:892-915`). | Local or peer, through the same `GatewayCapture` (`srv/verification/mod.rs:434-473`). The runner is not composed if no Scribe is reachable (`mod.rs:444, 473`). | `complete` or `terminate` deletes the row in the settlement transaction (`verifier_runs.rs:333-335, 1511-1550, 1601-1625`). It is also removed by `ON DELETE CASCADE` from the run (migration :12-13). | Never dropped for time. Retried until acked or the lease is lost (`capture.rs:874-915`). A terminal refusal leads to `Retry`, and an exhausted budget to `terminate`, which deletes the staged row (see §2). |
| Realtime (direct) Verifier | `DirectExecutor` | None | None | None | None | Writes nothing to Bifrost (`srv/verification/direct.rs:1-8`; `architecture/wyrd-design.md:796-797`). D6 would change this (§4.5). |

## 2. Run-queue state machine (`wyrd.verifier_runs`)

The lifecycle is documented at `verifier_runs.rs:12-14`. Every transition below
is fenced on `lease_token` and on `status='running'`.

| Op | SQL | Effect | Staged result |
|---|---|---|---|
| enqueue / observation enqueue | `INSERT_RUN_SQL` :83, `INSERT_OBSERVATION_RUNS_SQL` :145 | → `pending` | — |
| claim | `EXHAUST_EXPIRED_SQL` :250, then `CLAIM_RUN_SQL` :271 (`claim` :1365) | Expired runs on their final attempt become `errored` (`EXHAUST_EXPIRED_SQL` skips runs that have a staged result, :258). Due `pending`/`retrying` runs, or expired `running` runs, become `running` with `attempts+1` and a new token. | Read back into `ClaimedRun.staged` (`STAGED_RESULT_SQL` :305, :1394-1398). |
| renew | `RENEW_LEASES_SQL` :344 (`renew` :1459) | Extends the lease after a third of it has passed. An expired lease is never revived. | — |
| store_result | `LOCK_HELD_RUN_SQL` :314 + `STORE_RESULT_SQL` :323 (:1411) | Lease-fenced insert with `ON CONFLICT (run_id) DO NOTHING`. | Written |
| complete | `COMPLETE_RUN_SQL` :358 (:1511) | → `completed`. In the same transaction it inserts Operator dispatches when the verdict is failed (:1533-1548). | Deleted (:1530) |
| retry | `LEASED_ATTEMPTS_SQL` :405, `RETRY_RUN_SQL` :417 (:1563) | → `retrying`, or `terminate(errored)` once the budget is spent. | Kept on retry, deleted on exhaustion |
| terminate | `TERMINATE_RUN_SQL` :427 (:1601) | → `errored`, `timed_out`, or `cancelled`. | Deleted (:1620-1622) |
| await_trace | `AWAIT_TRACE_SQL` :452 (:1634) | Requeue with the attempt refunded, or `timed_out` past the deadline. | Kept / deleted |
| release / defer | `RELEASE_RUN_SQL` :437 (:1771) | Requeue with the attempt refunded. | Kept |

Runner flow (`runner.rs:343-383`):

1. If `run.staged` exists, replay it (the "later claimant writes the stored
   batches" rule, `runner.rs:12-13`).
2. Otherwise `produce`, then `store` (one transaction, committed).
3. `publish` (Scribe).
4. `settle` (one transaction). The settlement match is at `runner.rs:559-650`.

**Where a single-transaction "complete + stage result" would fit:** at step 4,
in the `Transition::Complete` arm of `settle` (`runner.rs:571-580`), which
already calls `complete` and `delete_staged` on one `TenantConn`.

**Reachability.** Both pools use the same `dsns.app` DSN:

- `wsql/src/postgres.rs:84`
- `vsql/src/postgres.rs:82-86`
- `srv/postgres.rs:9-39`

`ValaPostgres::tenant_conn` returns the same `wyrd_sql::TenantConn`
(`vsql/src/postgres.rs:135-138`). So `wyrd.verifier_runs` and a `vala.*` table
can be reached from one connection. The `wyrd_app` grant on each table was not
verified.

**Consequence.** If the outbox row is written in the settlement transaction,
the result reaches Scribe *after* `completed` commits. Today the order is the
reverse: Scribe ack comes first, then `completed` (`runner.rs:489-493`).
`complete` also inserts the Operator dispatches, so they would commit before the
verdict exists in Bifrost.

**What breaks if `verifier_run_results` is removed:**

- `store_result`, `delete_staged`, the `staged` read in `claim`, and the
  staged-row guard in `EXHAUST_EXPIRED_SQL` (`verifier_runs.rs:258`).
- The runner's replay branch (`runner.rs:355-376`).
- The only stable source of result batch ids (`results.rs:162`).
- Tests:
  - `pg_verifier_runs.rs:1152` `stored_results_are_lease_fenced_and_deleted_at_settle`
  - `pg_verification_runtime.rs:974` `lost_result_ack_replays_the_identical_sealed_batch_and_scribe_deduplicates`
  - `pg_verification_runtime.rs:2104` `crash_after_detail_ack_reclaims_the_same_run_before_dispatch`
  - the row-count probe at `pg_verification_runtime.rs:2796`

**Existing gap: partial publish.** Take a run whose detail batch was acked and
whose summary was then refused terminally. It goes to `Retry`. Once retries are
exhausted, `terminate` deletes the staged row (`runner.rs:529-540`;
`verifier_runs.rs:1577-1580`). The detail rows remain in Bifrost with no summary
and no completed run.

## 3. Scribe dedup fence

- **Order of operations.** WAL write → fsync → COMMIT record, then
  `commit_batch_control_fence`, then memtable insert, then ack
  (`redux/scribe/shards.rs:3071, 3590, 3632, 3663, 3988`).
- **Key.** `(data_tenant_id, logical_table_fqn, batch_id)`
  (`vsql/migrations/20260910000021_scribe_batch_commits.sql:19`). The insert is
  `ON CONFLICT DO NOTHING` (`vsql/src/queries/scribe_batch_commits.rs:295`).
  The design doc agrees (`architecture/bifrost-design.md:86`).
- **Durability.** The fence is a Postgres row in `vala.scribe_batch_commits`,
  under RLS. Grants are SELECT and INSERT only (migration :22-28).
- **Retention.** None. No DELETE exists anywhere in `crates/`. The design
  doc's "idempotency-retention window" (`bifrost-design.md:94`) is not
  implemented, so the effective window is infinite and the table only grows.
- **Resend after a crash is always absorbed when the logical bytes are
  identical:**
  - Before the fence: the batch is admitted fresh, and no ack had been sent.
  - After the fence: `AlreadyCommitted`; the batch is acked and its rows are
    discarded (`scribe_batch_commits.rs:154-163, 337-339`;
    `shards.rs:3632-3641`).
  - WAL replay restores the batch or suppresses it (`shards.rs:1827-1848,
    1872-1884`).
  - An ambiguous commit triggers a re-resolve (`shards.rs:3663-3713`).
- **Same id, different content.** `slice_set_digest` covers the schema
  fingerprint plus a SHA-256 of the logical data (`redux/scribe/wal.rs:558-578,
  2176-2183`). A mismatch is refused as an invariant violation and can poison
  Scribe (`scribe_batch_commits.rs:240-248, 341-350`; `shards.rs:3708-3711`).
  **Any outbox must therefore resend byte-identical logical content.**

## 4. Batch identity per writer

The question for each writer is whether the id is fixed by the time an outbox
row would be written.

| Writer | Derivation | Deterministic then? |
|---|---|---|
| Audit | `derive_batch_id(tenant, seq_lo, seq_hi)` (`redux/tables/audit/projection.rs:86, 309-322`). The range comes from the frozen bound (`audit_staging.rs:384-445`). | Only once the bound is frozen. A plain watermark plus a growing tail would give two publishers overlapping ranges and different ids, "which no dedup fence can absorb" (`bifrost-design.md:665-674`). |
| Gateway capture | `CaptureBatch::derive_id(tenant, call_id, table)` (`capture.rs:372-397`), required by REQ-180 (`spec.md:2279-2281`). | Yes, at projection time. The content must also be byte-stable on resend (§3). |
| Verifier results | `Uuid::now_v7()` at encode (`results.rs:162`), persisted by `store_result`. | Yes, once stored. Not derivable again from the inputs. |
| Realtime (D6) | Not built yet. The R4 plan uses the same encode, so a random id. | Only if it is stored or kept in memory. |

## 5. Constraints the single pattern must satisfy or would change

**AGENTS.md**

- AGENTS.md:129-137: "Permissions are blocking; audits are non-blocking … a
  failed commit is logged and counted and never fails the operation … no error
  exists to report an audit write failure." A Postgres outbox write sitting on
  the request path must not make audit blocking.
- AGENTS.md:138-142: "There is one audit write path and one publisher. Every
  audit event is committed to `vala.audit_staging` through the canonical
  append, and only the `AuditPublisher` moves staged rows … The process audit
  outbox is the only caller of that append; no other audit table, WAL, relay,
  or log sink exists." **A shared outbox table or consumer for audit changes
  this rule.**
- AGENTS.md:146-153: "Publication progress is a per-tenant monotonic watermark
  plus at most one frozen in-flight upper bound; no lease, claim, or owner
  token exists … Staged rows are garbage-collected once the watermark has
  advanced past them, in the same transaction that clears the matching bound."
  **A delete-on-ack row model changes this rule.**

**Audit hash chain**

- `append_audit_events` locks `vala.audit_chain_head` `FOR UPDATE`, chains
  `entry_hash = SHA256(prev_hash, seq, event)`, and assigns a gapless `seq`
  (`audit_staging.rs:23-35, 70-170, 535-560`).
- This happens at the Postgres staging write, not at Scribe. The chain and
  `seq` are what the frozen range and the batch id are built from
  (`audit_staging.rs:360-372`).
- Delete-on-ack of individual rows would still need the chain to be assigned
  before Scribe.

**Architecture documents**

- `bifrost-design.md:130-136`: derived work is "batched through server-owned
  outboxes … An outbox retries rather than dropping work … graceful shutdown
  flushes it."
- `bifrost-design.md:661-663`: "The publisher runs only in a process that owns
  a local Scribe and calls it directly; Gate holds no retained-audit path."
  Commit `f105e703d` (2026-09-11) shows local-only was deliberate, not an
  omission.
- `bifrost-design.md:157`: an ack means the WAL append and the batch fence are
  durable.

**`changes/active/audit-outbox/spec.md`** (a separate active change)

- Non-goal at :59-61: "Durable (crash-safe) outbox queuing beyond the process."
  **This directly conflicts with a Postgres outbox for every writer.**
- REQ-008 at :127-147: "No other outbox implementation exists."
- REQ-003a, REQ-006 and REQ-009 (:92-160) cover accepted loss, GC only past
  the watermark, and never retrying a committed write.

**`changes/active/verified-change-contract/spec.md`**

- REQ-180 at :2271-2281: "Capture holds no per-tenant queue, client, or
  in-memory backlog beyond the in-flight attempt"; a capture is dropped at the
  deadline. Revision 54 (:3235-3248) records the user's approval of this.
  `wyrd-design.md:617-620` agrees. **A durable capture outbox reverses REQ-180
  and revision 54.**
- REQ-178 at :2259-2261: the peer RPC refuses all but 2 capture tables and 3
  result tables. "Widening it to other server-internal writers requires a spec
  revision." Audit over peer needs that revision. The receiver also refuses
  `SYSTEM_OWNER` (`capture_peer.rs:59-63`), which is where platform audit is
  staged.

**R4 draft (untracked, revision 72 not yet approved)**

- `review/TASK-017-r4/TASK-017-R4-canonical-support-desk-journey.md:71` (D6):
  realtime verdicts are written "through the existing result writer … never
  delays or fails the caller. A loss is logged and counted, like the audit
  outbox."
- :72 (D7): `result_id = execution_id`; no `verifier_runs` row and no dispatch.
- :73 (D8, LOCKED):
  - managed `run_id` = the application Run (the caller's
    `ExecuteVerificationRequest.run_id` for realtime, null for scheduled
    Drift);
  - managed `card_uid` = the Verifier, through `CardRefScope::own`;
  - payload `subject_card_uid` = the judged Card;
  - the principal is the tenant SYSTEM principal (`R4:592`);
  - `VerifierAttribution.run_id` stays the Verifier run on the peer wire.
- REQ-218 (:910-920).
- The plan at :369 and :595 adds a new in-memory
  `Outbox<DirectResultSink>`. That is a new sink, not Postgres.

**Peer plane**

- Served only on Scribe pods.
- Admits only the `wyrd-peer` mTLS identity, with no token
  (`srv/grpc/mod.rs:387-391, 423-435`).
- Checks attribution by parsing only. It does not verify that the principal is
  the tenant's SYSTEM principal or that the run exists (`capture.rs:295-320`;
  `capture_peer.rs:54-102`).
- Documented at `architecture/wyrd-security-posture.md:70-81`.

**Gateway capture payloads**

- JSON ceiling is `GATEWAY_JSON_MAX_BYTES = 1_048_576`
  (`crates/wyrd-spec/src/gateway/record.rs:78`).
- Redaction and object references are applied at projection, before any write
  (`capture.rs:1-13, 826-829`).
- Durable capture would put up to about 1 MiB of redacted content per call into
  Postgres, plus object references, in object-store-before-row order. No spec
  REQ pins redaction timing; this is code only.

## 6. Cleanup and accumulation today

- **`vala.audit_staging`.** Deleted at settle.
  - Accumulates for tenants that are not `active`, or are soft-deleted:
    `list_tenants_owing_publication` skips them (`audit_staging.rs:318-340`).
  - Accumulates on any pod topology with no local Scribe or no operator pool,
    because no publisher runs there (`publication.rs:133-149`).
- **`wyrd.verifier_run_results`.** Deleted at complete or terminate, or by
  cascade.
  - A staged run is never exhausted by an expired lease
    (`verifier_runs.rs:258`), so it is reclaimed repeatedly until a settlement
    lands.
- **`vala.scribe_batch_commits`.** Never deleted (§3).
- **In-memory audit and run-request outboxes.** Unbounded, with nothing
  persisted (`outbox.rs:18-22`).

## 7. Reuse candidates (unchanged) vs. what each writer would stop doing

**Reusable unchanged:**

- `wyrd_runtime::outbox::{Outbox, OutboxSink}`: the per-tenant queue, retry,
  backoff, shutdown, and metrics (`outbox.rs:1-23, 55-78`).
- `AuditSink::resolve_commit`: the `pg_xact_status` check for an unknown commit
  outcome (`audit_outbox.rs:83-125`).
- `GatewayCapture::submit` / `write_result`: local and peer routing with
  retryable classification (`capture.rs:892-974`).
- `CaptureBatch` / `into_frame`: the frame and the SYSTEM attribution
  (`capture.rs:330-425`).
- The Scribe batch fence as the idempotency mechanism (§3).
- The freeze/settle SQL pattern with `GREATEST` and matching-bound guards
  (`audit_staging.rs:384-532`).
- The `StagedBatch` shape: table, batch_id, ipc
  (`verifier_runs.rs` / migration :22-24).

**What each writer would have to stop doing:**

- **Audit**
  - Stop the in-memory-first stage? AGENTS.md:131-133 forbids audit becoming
    blocking.
  - Stop the watermark plus frozen bound, which AGENTS.md:146-153 locks.
  - Stop local-only publication, which `bifrost-design.md:661-663` locks.
- **Gateway capture**
  - Stop the drop at the deadline and the "no backlog" rule (REQ-180, revision
    54).
- **Run results**
  - Stop the publish-before-complete order and the staged replay
    (`runner.rs:355-383`).
- **Run requests**
  - Nothing Scribe-bound. Only the in-memory stage, whose loss is accepted
    (`wyrd-design.md:1472-1478`).
- **Realtime**
  - Nothing exists today. R4 plans another in-memory sink.

## 8. Tests that pin current paths

`W` means the test needs Postgres. Wrap it as:

```
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && <cmd>'
```

**Audit**

- `vsql/tests/integration/pg_audit_outbox.rs:108,150,192` (W):
  `mise exec -- cargo nextest run --locked -p vala-sql --test integration -E 'test(=pg_audit_outbox::<name>)'`
- `vsql/tests/integration/pg_audit_staging.rs:218-737`: 9 tests (W), same
  target. The nested path is unconfirmed; check with `nextest list`.
- `srv/audit/publication.rs:619,652,686`:
  `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=audit::publication::tests::<name>)'`
- `crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs:366-1144`:
  9 journeys, including crash/replay at :366 and ambiguous commits at :1000.
  (W) `mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=audit_publication::<name>)'`

**Gateway capture**

- `capture.rs:1742-2042`: 8 unit tests.
  `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::gateway::capture::tests::<name>)'`
- `srv/components/gateway/pg_invocation_tests.rs:3101,3707,3818` (W), same lib
  form.
- `wyrd-testing/tests/gateway/peer.rs:115` and `compatible.rs:76` (W):
  `-p wyrd-testing --test gateway -P journey --run-ignored=all -E 'test(=peer::oracle_only_gateway_captures_through_the_peer_scribe)'`

**Runner and queue**

- `wsql/tests/integration/pg_verifier_runs.rs`, at 1152 (store/delete), 1251
  (renew), 1322 (fence), 1431 (retry), 1529 (terminate), 1735 (release) and
  2160 (await_trace) (W):
  `mise exec -- cargo nextest run --locked -p wyrd-sql --test integration -E 'test(=pg_verifier_runs::<name>)'`
- `srv/../tests/integration/pg_verification_runtime.rs`, at 592, 974 (replay),
  1059 (defer), 1084, 1481 (crash), 2104 (crash after detail ack) and 2754 (W):
  `mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration -E 'test(=pg_verification_runtime::<name>)'`
- `wyrd-testing/tests/bifrost/server/verification_runtime.rs:67`, the peer
  result write (W), journey form.

**Observation enqueue**

- `pg_verifier_runs.rs:708,811,1880,1994,2069` (W).
- `pg_verification_runtime.rs:2563`
  `observation_outbox_retains_through_an_outage_and_flushes_at_shutdown` (W).
- `observations.rs:201`, lib.

**Generic outbox**

- `outbox.rs:728-904`: 8 tests.
  `mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(=outbox::tests::<name>)'`

No test exercises audit publication over a peer, because that path does not
exist.

## 9. Open questions for a human

1. Does the target pattern supersede the audit-outbox spec non-goal "durable
   outbox beyond the process" (`audit-outbox/spec.md:59-61`)?
2. Should audit keep the watermark and frozen bound (AGENTS.md:146-153), or move
   to delete-on-ack? If it moves, where is the hash chain assigned?
3. May audit publish over the peer plane? That means revising REQ-178 and the
   `SYSTEM_OWNER` refusal, and changing `bifrost-design.md:661-663`.
4. Should gateway capture become durable, reversing REQ-180 and revision 54?
   If so, what is the Postgres payload budget, given about 1 MiB per call?
5. For run results, should Scribe stay ahead of `completed` and Operator
   dispatch, or may `completed` commit before Bifrost has the verdict?
6. Is the partial-publish case (details acked, run terminated, staged row
   deleted) a bug to fix in this change?
7. Should realtime verdicts (D6) be durable in Postgres, or in memory as R4
   currently plans?
8. Should the in-memory Eval run-request loss (`wyrd-design.md:1472-1478`)
   stay accepted?
9. `vala.scribe_batch_commits` has no retention, while `bifrost-design.md:94`
   describes a window. Is one intended?
10. Should the peer receiver verify the result principal and run, or keep
    trusting the mTLS identity alone?
