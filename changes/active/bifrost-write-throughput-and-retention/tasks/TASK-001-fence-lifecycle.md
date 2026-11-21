---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-bifrost-write-throughput-and-retention
spec_revision: 1
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009]
depends_on: []
---

# Safely retire retry receipts without losing accepted history

## Outcome and scope

A client can retry a recent batch, and the four application consumers can recover
their retained decisions after its transport receipt expires. Scribe removes old
receipts only after accepted data has independent durable custody. Failed or stale
cleanup pauses fresh writes through existing admission while reads and recovery
remain usable. This establishes finite recent retry history without promising a
strict instantaneous receipt-row or physical PostgreSQL-byte ceiling.

Implement [spec revision 1](../spec.md) using the
[selected implementation and deletion proof](../design/notes.md#locked-deletion-proof-and-backlog-enforcement)
and [final cleanup-health amendment](../design/cleanup-health-proposal.md).
The [shared execution/checkpoint recipe](README.md) applies after each scenario.
No prior task is required; inputs are existing client batches, PostgreSQL acceptance
records, Bifrost domain rows, WAL attempts and staged/published custody records.
The owning `architecture/bifrost-design.md` already records the settled recovery
and receipt-lifecycle policy from spec authoring; consume it as authority and keep
its description consistent with the delivered behavior, rather than reopening it.

## Existing owners and connected implementation

- `vala-sql/src/queries/scribe_batch_commits.rs` owns tenant-scoped acceptance,
  replay resolution and recent receipts. Its `committed_batches` consumers currently
  assume fences never disappear. Keep the ordinary batched transaction and conflict
  resolution; extend this owner for bounded eligible deletion and durable ownership
  discovery. Retention is one hour / 10,000 identities per tenant-qualified logical
  table across clients/pods, with PostgreSQL time and canonical-coordinate rechecks.
- `wyrd-server/src/verification/{state,approvals,evidence}.rs` and
  `components/github/delivery.rs` consume permanent acceptance history.
  `verification/inputs.rs` already owns Bifrost domain reads. Extend the existing
  domain read/retry paths so actual retained records establish history and conflicts;
  recent receipts bridge accepted-but-invisible records. Do not add a parallel
  PostgreSQL history store or read only through the receipt list.
- `vala-bifrost-redux/src/scribe/{shards,wal,staging_runtime}.rs` and the existing
  rotation-cohort/manifest owners know canonical WAL coordinates and durable slices.
  Extend the local manifest with the agreed contiguous closed-segment frontier.
  Persist whole-batch custody before deletion; load it before missing-receipt replay
  decisions, and retain safe stage-record retirement and torn-tail validation.
  Replace ordinary-table unfenced salvage with accepted-only recovery, preserving
  accepted lost-response cases and suppressing losing cross-pod WAL attempts.
- The existing Scribe runtime and `scribe/admission.rs` own background work and
  ingestion pressure. Add one bounded background cleaner there: coalesced push plus
  periodic discovery, bounded SQL batches, finite sweep coverage and local admission
  health. Reuse workload/memory/in-flight/unstaged bounds. Neither the current shard
  loop nor Forge supplies this off-ACK cleanup workflow; do not duplicate it in Forge.
- The committed-batch mirror must reconcile expiry with durable receipt authority.
  Wire cleaner startup/shutdown to existing Scribe lifecycle; stale/exited workers
  cannot leave admission healthy. No cleanup SQL or wait enters append admission.

Do not enable automatic receipt deletion until the domain readers and durable
recovery safeguards are in place. Scenario 1 and 2 may establish absent-receipt
states through internal integration fixtures; that is not permission for raw SQL
or private controls in a public SDK journey. Existing staged custody is the input,
not an invented per-batch retirement record.

Outputs are authoritative domain reads, durable safe frontiers, safely removed
receipts and cleanup-health refusal/resumption. Task 2 consumes these behaviors,
their regression tests and this task's last accepted benchmark checkpoint.

## Ordered acceptance scenarios and evidence

### 1. Retained domain history survives transport-receipt expiry

**Obligations:** REQ-001–003.

**Start/input:** Through the real existing producer paths, accept an execution's
ordered revisions, an approval decision, deferred evidence and a GitHub delivery
outcome. Their actual Bifrost records are retained and independently durable.
Use internal fixture setup to remove the corresponding receipts; retain separate
recent-acceptance fixtures whose rows are not yet visible. Ordinary recent batches
also exercise the existing stable-identity retry/conflict boundary.

**Action/result:** Reconstruct the execution and read/retry each domain operation
through its real consumer. The revision sequence and exact decisions survive;
matching domain retries preserve their result and contradictory decisions still
conflict. Recent accepted-but-invisible rows and unavailable authority produce
unavailable/retry results rather than absence. Same-identity transport retries
while a receipt remains authoritative retain original version/content semantics,
including cross-pod arbitration and tenant separation. Buffered enqueue and durable
completion keep their existing boundaries.

**Proof owner/tier:** Add/extend `wyrd-server` integration coverage beside
`pg_approvals`, `pg_deferred_evidence`, `pg_verification_runtime` and
`pg_github_delivery`, using real Postgres and Bifrost consumer paths. Use the
existing `wyrd-testing` server/SDK journey owners for client-visible behavior;
SQL fixtures belong in the internal tier. Pure row interpretation tests cannot
substitute for the consumer reconstruction and retry paths.

**Expected RED:** Current consumers interpret missing receipts as never accepted;
retained history becomes absent or execution reconstruction truncates. Add focused
assertions for that missing behavior before changing production code. Existing
already-correct enqueue/ACK behavior is verification-only.

**Evidence:**

- Consumers read recent receipts first, then their Bifrost rows; a visible row is
  authoritative, a held receipt without a visible row is unavailable, and neither
  is absence. A receipt is removed only after its row is visible, so one gone
  before the receipt read names a row the later row read sees. Execution recovery
  starts from `InputResolver::latest_revision`, probes receipts above it and
  re-reads the visible revision until it is unchanged, so a receipt removed
  during the probe cannot truncate recovery. Owners: `verification/state.rs`
  `RunState::accepted`, `verification/approvals.rs` `decided`,
  `verification/evidence.rs` `accepted`, `components/github/delivery.rs`
  `recorded`, `verification/inputs.rs` `latest_revision`; `committed_batches`
  documents recent receipts.
- Execution revisions —
  `wyrd-testing` `lifecycle::execution_recovery_outlives_its_revision_receipts`:
  receipts removed after publish, takeover recovers the same accepted state; a
  recent receipt for an invisible next revision is `StateError::Unavailable`.
  RED: `lifecycle.rs:826` "recovery neither resets nor truncates the accepted
  revisions" (recovery came back fresh). GREEN.
- Approvals — `pg_approvals::a_decision_outlives_its_retry_receipt`: after
  receipt removal the completed execution serves the same decision, actor and
  time; the same decision replays the original receipt; a contradictory one is
  `WYRD_VERIFICATION_409_APPROVAL_NOT_PENDING`; an undecided occurrence with a
  recent receipt and no visible row is 503, never pending. RED:
  `pg_approvals.rs:688` result `Null`, delivery `pending`. GREEN, 6/6 repeats.
- Evidence — `pg_deferred_evidence::accepted_evidence_outlives_its_retry_receipt`:
  after receipt removal a republish replays the original id and time, different
  content is `WYRD_VERIFICATION_409_EVIDENCE_CONFLICT`, a later pull request
  consumes it and passes; with receipt SELECT revoked a new publication is 503.
  RED: `pg_deferred_evidence.rs:886` status `accepted`, expected `replayed`. GREEN.
- GitHub delivery — `pg_github_delivery::a_recorded_outcome_outlives_its_retry_receipts`:
  after receipt removal inspection serves the same result and `delivered`, and
  nothing is sent again; with receipt SELECT revoked inspection is 503. RED:
  `pg_github_delivery.rs:726` result `Null`, delivery `pending`. GREEN.
- Limitation: evidence and delivery batch keys are private, so their
  accepted-but-invisible case is proven through unreadable authority (revoked
  receipt SELECT) rather than a synthetic receipt; approvals and execution state
  use a synthetic recent receipt. Same-identity transport retry, cross-pod
  arbitration, tenant separation and enqueue/ACK boundaries are unchanged and
  covered by the existing suites below.
- Diagnosis (Codex, read-only): first GREEN attempt read rows before receipts;
  `pg_approvals` failed 1 in 5 with 503 "a recorded outcome is not visible yet".
  Cause: Scribe commits the receipt (`scribe/shards.rs:4273`) before memtable
  visibility (`:5097`), so a batch landing between the row read and the receipt
  read looked accepted but invisible. Fix site: read order in all four
  consumers. A second Codex review confirmed the execution re-read is required:
  count retention can remove a receipt for a revision that became visible
  between the first visible read and the probe.
- Removed `pg_verification_runtime` assertion that a completed run stages no
  audit: recovery now reads Bifrost as SYSTEM, and each read decision is audited
  by design; the assertion pinned an incidental absence of reads, not a
  requirement.
- Commands:
  `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 --no-fail-fast -E "test(/^(pg_approvals|pg_deferred_evidence|pg_verification_runtime|pg_github_delivery)::/)" && cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E "test(/^lifecycle::/)"'`
  → 99/99 and 10/10 passed. `mise run fmt && mise run lints` clean;
  `git diff --check` clean.
- Benchmark checkpoint: pending.

### 2. Accepted recovery survives the durable whole-batch handoff

**Obligations:** REQ-004–006.

**Start/input:** Feed real admitted batches into Scribe/WAL, including an accepted
batch with a lost response, a complete never-accepted attempt, and another pod's
losing duplicate WAL attempt. Include a batch crossing closed-segment boundaries,
multiple slices/seal keys and an earlier unresolved gap. Stage through the existing
cohort owner, not synthetic maximum watermarks.

**Action/result:** Advance and persist the frontier only after every accepted slice
touching the contiguous prefix has independent durable custody. Internally remove
only eligible receipts under the safe predicate, restart on the same durable state,
and query exact accepted rows. Independently staged rows restore without the receipt;
losing and never-accepted WAL restores no rows. Load the frontier before interpreting
missing receipts. A crossing batch/gap, failed stage/manifest persistence, unavailable
authority or stale/offline stream cannot manufacture safe deletion or speculative
recovery. Recheck current canonical coordinates against stale delete candidates.

Interrupt between custody, frontier persistence, proof disposal and receipt removal;
exercise stage-record retirement and a torn tail after old fences disappear. The
outcome must preserve accepted rows, reject ambiguous authority and delay unsafe
cleanup. Returning from physical retirement or leaving a file behind is not the proof.

**Proof owner/tier:** Extend inline WAL/shard tests for manifest ordering and replay,
real-dependency redux integration under `tests/integration/scribe/`, and existing
`wyrd-testing/tests/bifrost/scribe/` recovery/fencing/source-boundary journeys. Use
the real SQL receipt owner and the multi-process harness for cross-pod/restart proof.
The connected journey must show accepted input → actual staged custody → persisted
frontier → receipt removal → restart → exact query result; isolated manifests alone
do not prove it. SQL eligibility/concurrency checks live in `vala-sql` integration.

**Expected RED:** Ordinary-table replay currently salvages unfenced complete attempts,
and existing per-seal-key manifests cannot establish the required whole-shard
frontier. Tests must expose those missing behaviors before the respective changes.

**Evidence:** Pending. Append focused RED/GREEN, connected restart proof and the
three benchmark checkpoint reports/comparisons here before scenario 3.

### 3. Background deletion keeps up or pauses fresh admission safely

**Obligations:** REQ-002, REQ-006–009.

**Start/input:** Use scenario 2's durable custody/frontier with receipts both inside
and outside one-hour/10,000 history, across tables, tenants, pods and retained epochs.
Include old recovery-pinned receipts, an already admitted write and queryable rows.
Use repository-owned time/failure seams rather than wall-clock sleeps or changes to
production retention defaults.

**Action/result:** Trigger custody/count pushes and periodic age discovery. The
single bounded Scribe worker removes only eligible receipts in bounded tenant SQL
batches. A quiet table's age cleanup needs no new write; missed/full/coalesced wakes
recover from durable discovery without blocking admission. Count covers the whole
tenant/table; deleting one tenant's receipts cannot affect another. An actually
removed ordinary batch identity may be accepted again, and its mirror cannot return
the former acceptance. Retained domain rows remain queryable and decisions remain final.

Start/restart cleanup-unverified with old-epoch debt; fresh writes wait for an initial
due-work sweep. Then fail or make SQL ambiguous, hang/exit the worker, and let small
DELETE batches progress while whole-sweep coverage falls behind. Assert local
retryable-busy admission, anchored to captured sweep boundaries rather than heartbeat
or old-sweep finish time. New arrivals cannot make a captured sweep infinite. Reconcile
ambiguous commits before publishing success. Empty eligible scope and entirely
recovery-pinned scope can complete successfully. Restore progress and complete a
current sweep: admission resumes subject to ordinary resource pressure.

While refused, already admitted work can finish its unchanged ACK, accepted rows
and reads stay usable, and staging/recovery/cleanup continue. Existing unstaged and
in-flight bounds still refuse their own overload. Crashes around notification, SQL
commit and health publication delay cleanup/admission rather than lose accepted data.
Record the local-only healthy-request check; no added request SQL, fsync or cleanup
wait, process-readiness failure, memory poison, receipt quota or per-batch job ledger.

**Proof owner/tier:** Real Postgres tests in `vala-sql` receipt integration for the
transactional policy; inline Scribe cleaner/admission tests for bounded coverage and
stale-worker behavior; redux lifecycle integration for durable discovery/restart;
`wyrd-testing` Scribe backpressure/recovery/write-read journeys for actual producer
refusal, completion and query behavior. Keep failure injection inside existing
internal test support; do not add public operational controls.

**Expected RED:** No safe background expiry/coverage signal currently drives
admission. Eligible rows remain indefinitely, or writes remain admitted when the
new cleanup obligations are failed/stale. Separately expose cache expiry and startup
debt; do not claim a fixed row ceiling as the assertion.

**Evidence:** Pending. Append focused RED/GREEN and the three benchmark checkpoint
reports/comparisons here. Supply this accepted checkpoint to task 2.

## Verification and boundaries

Use the focused-test discipline and benchmark commands in [README](README.md).
Owning runners, confirmed in `mise.toml`:

```bash
mise run test:bifrost:integration:sql
mise run test:bifrost:integration:redux
mise run test:bifrost:journey:scribe
mise run test:bifrost:journey:server
```

The standard Bifrost server-integration lane omits several affected domain modules.
Run their actual registered suites explicitly under the repository environment:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(/^(pg_approvals|pg_deferred_evidence|pg_verification_runtime|pg_github_delivery)::/)"'
```

Choose relevant previously green focused tests while iterating; run each affected
owner lane once at task verification. Finish `mise run fmt`, `mise run lints`,
`mise run check:tenant-isolation`, and `mise exec -- git diff --check`. Run dependency
or generated-contract checks only if their inputs changed. Full shipped-surface
journeys/aggregate checks remain integrated-review work.

No ACK protocol change, extra durable acceptance marker, per-fence UPDATE, new
database table/migration file, receipt quota, Forge scheduling, snapshot/orphan or
stored-row retention work. Necessary unreleased migration edits stay in place as
authorized by the spec. Private SQL/index and health timing choices remain local.
Do not silently redesign if evidence contradicts a locked guarantee; return the
specific conflict to the maintainer with its evidence.
