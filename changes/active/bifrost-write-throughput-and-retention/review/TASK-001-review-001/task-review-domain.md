# TASK-001 domain review

Subject: `66f44aee1..bf438a8d7985aadd29db3beb73ed0269c2b2f7ca` (19 commits), approved SPEC-bifrost-write-throughput-and-retention revision 1, original `tasks/TASK-001-fence-lifecycle.md`, including maintainer remediation R1. HEAD matched the candidate during inspection. This is independent discovery, not independent validation.

## Scope and authority

Reviewed REQ-002 and REQ-006–009: receipt SQL, tenant boundaries, mirror coherence, cleaner discovery/sweep/health, admission, startup and shutdown. Recovery checkpoint construction belongs to the invariant reviewer; this report consumes its persisted frontier contract without independently accepting it. Sources: root AGENTS, `changes/spec-driven-development.md`, `TESTING.md`, `agent_docs/{index,rust,maintainer-style}.md`, `architecture/bifrost-design.md` receipt handoff/cleanup and mirror rules, and security posture tenant/operator SQL boundaries. CodeGraph exploration preceded source tracing. No nested AGENTS exists under the changed Vala owners.

Navigation: SQL `RetryHistory`, `receipt_streams`, `delete_handed_off` → cleaner `sweep`/`frontiers`/`sweep_and_report` → `CleanupHealth` → admission `try_reserve`; shard `append_and_sync_batch_commits` → `record_committed` → `CommittedBatches` → ingress duplicate ACK; Scribe construction/replay → server boot role activation. Assertions inspected in SQL receipt integration tests, cleaner inline/PG tests, admission tests, and lifecycle expiry/failure/losing-duplicate journeys.

## Coverage observations

- SQL uses actual `TenantConn` and forced tenant RLS for ranking/deletion; operator discovery uses `OperatorPool`, returns coordinates only, and filters stable node. Count ranks the tenant/table across all nodes; one-hour and 10,000 defaults match REQ-002. Outer DELETE rechecks stream/shard/segment rather than trusting the ranked identity alone. SQL tests cover age, shortened count history, uncovered/foreign coordinates, replacement above frontier, cutoff and multi-tenant/epoch discovery.
- Cleaner snapshots stream scope and PG cutoff, reads manifests off async workers, deletes bounded batches, commits before publishing whole-sweep success, and forgets its local mirror before commit. Failed commit cannot publish success; subsequent discovery/deletion reconciles remaining work. Empty/recovery-pinned scopes succeed. The worker is separate from the serial writer and ingress adds only a local health check.
- Health unit assertions anchor freshness to sweep start, expire at 120 seconds, clear on error and reject late coverage. PG tests use batch size two with five covered receipts, multiple tenants/epochs and pinned/foreign receipts; revoked DELETE proves failure/recovery. Admission assertions prove no item moves before cleanup refusal. The one-pod journey proves real expiry, reacceptance, retryable refusal, accepted reads and resumption.
- Acceptance is incomplete because the two concrete paths below violate REQ-002 and REQ-009. Existing tests do not exercise either path. In particular, the two-pod losing-duplicate journey restarts the losing pod immediately after deletion, clearing the stale mirror and concealing the first defect. Cleanup failure testing returns a SQL permission error; it does not hold startup cleanup pending while checking service availability.
- Reported task executions are 483/483 redux, 52/52 Scribe journeys and 9/9 SQL receipt tests before R1, with later R1 execution records. Their assertions were inspected; this reviewer did not execute Cargo or reproduce the two proposed failures. No benchmark execution was performed in this boundary review.

## Proposed blocking findings

### DOMAIN-1 — INCORRECT: deletion leaves another pod's duplicate mirror authoritative

Requirement: REQ-002 explicitly forbids the in-memory mirror acknowledging an identity whose backing receipt has been removed; Bifrost architecture likewise requires mirror expiry with durable receipt authority.

Locations: `scribe/shards.rs:3705–3724` (`record_committed`), `:4318–4338` (AlreadyCommitted handling); `scribe/receipt_cleaner.rs:185–210`; `scribe/ingress.rs:778–808`; `scribe/committed_batches.rs:60–72,79–105`.

Supported starting state and trigger: pod A accepts and publishes a batch. Send an identical retry, using the same tenant/principal/table/version, to pod B. B's SQL resolution is `AlreadyCommitted`, so insertion is suppressed by `spent_batch_ids`, but `record_committed` still records it: only `accepted_under` and `conflicted` are excluded. Age A's receipt beyond one hour and allow A's own cleaner to delete it after custody handoff. Keep B running and resend the batch to B.

Expected: B must cease acknowledging from the expired receipt; authoritative Scribe arbitration decides this now-unprotected identity (and may accept it as a new batch).

Actual: A's cleaner forgets only its own `Arc<CommittedBatches>`. B discovers/deletes only receipts with B's node id, so it neither sees A's expired receipt nor invalidates its cached duplicate. B's matching mirror entry produces immediate `first_commit=false` ingress ACK with no SQL or new insertion, indefinitely on a quiet pod until FIFO eviction/restart. The mirror contains no expiry or owner coordinate capable of reconciling this. The two-pod setup is already supported by the existing losing-duplicate journey.

Consequence: transport retry history persists past authoritative deletion and is inconsistent across pods; a client receives an obsolete duplicate acknowledgement after finite retry protection ended.

Smallest correction boundary: make the shared mirror producer/invalidation policy incapable of retaining another owner's expired acceptance, reusing durable arbitration for entries without valid local lifetime authority. Preserve current matching/conflicting retry behavior while receipts remain authoritative, and keep new cleanup waits/SQL off ordinary ingress. A focused two-pod journey should cache B's retry, have A's background cleaner delete its aged receipt, keep B alive, then resend to B and assert the durable receipt/new row outcome rather than merely successful ACK. Run the Scribe journey owner and focused mirror/receipt tests. No new public control or ledger is required by this finding.

### DOMAIN-2 — INCORRECT: initial cleanup holds recovered service readiness

Requirement: REQ-008 requires cleanup before fresh admission; REQ-009 separately requires reads/recovery remain usable and forbids process-readiness coupling. Architecture receipt cleanup and the approved cleanup-health amendment explicitly reject readiness as this signal.

Locations: `scribe/mod.rs:2714–2721`, `scribe/receipt_cleaner.rs:152–157,184–211`, and `wyrd-server/src/boot/mod.rs:762–787`.

Supported starting state and trigger: restart a mixed server with accepted rows in published custody and an old, handed-off receipt. A concurrent tenant transaction holds a row lock on that receipt. Normal WAL/staging recovery can complete without needing this safely handed-off receipt, but the cleaner's DELETE waits on the row lock. The background cleaner starts during construction; replay may itself enter the blocked sweep or wait on its whole-sweep mutex.

Expected: recovered reads/other roles become available; fresh writes remain retryable-busy until an initial current sweep completes. Cleanup debt cannot hold service recovery/readiness hostage.

Actual: after WAL recovery succeeds, `replay_wal_async` awaits `sweep_and_report` before setting `recovery_ready=true` or returning. The sweep and mutex have no timeout/cancellation selection; default pool configuration bounds acquiring a connection, not executing a statement or waiting on the row lock. Boot itself synchronously awaits replay before Scribe role activation and continuing boot. Thus a cleanup-only blocked DELETE can indefinitely delay startup/service availability, even though custody recovery succeeded. Catching a returned sweep error does not resolve a pending sweep. No default statement/lock timeout was found in the production pool owner.

Consequence: maintenance row contention propagates into service/recovery unavailability instead of the approved local fresh-write refusal.

Smallest correction boundary: decouple cleanup completion from recovered role/process readiness at the existing Scribe replay/cleaner lifecycle owner; retain cleanup-unverified admission until the first sweep completes, including under a blocked/hung initial sweep. Add a real-dependency startup/restart regression with a blocked cleanup DELETE that proves retained reads and service availability, fresh-write busy, and automatic admission resumption once released. Do not weaken the initial sweep requirement or make recovery-required WAL authority optional.

## Process evidence and limits

Task evidence explicitly records non-test-first health, cleaner PG and journey coverage. Preserve those TDD violations separately from behavioral acceptance; subsequent mutation checks strengthen tests but do not supply historical RED. SQL cutoff/discovery and admission have recorded expected RED before GREEN. This review introduces no implementation, test changes, migration changes, new requirements, or benchmark claims. The two findings are source-traced proposals awaiting an independent validator.
