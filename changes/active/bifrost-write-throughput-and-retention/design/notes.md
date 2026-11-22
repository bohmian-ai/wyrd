# Bifrost write throughput and bounded retry history

Status: finalized — throughput, retention, cleanup-health admission, retry defaults
and two-task/six-scenario delivery approved and locked

## Two explicit objectives

1. Align retry, deduplication and fence lifetime with the agreed separation of
   accepted data, bounded recent retry identities and permanent domain history.
2. Increase write throughput, with an initial target of **80 input MiB per CPU
   second**, across the shared write path. Retention alone does not satisfy this.

The maintainer explicitly requested multiple independent Codex reviewers to
review, reconcile and propose this throughput design while preserving read
latency and the existing correctness/resource guarantees. Performance proof
comes after implementation; the design must be concrete and evidence-grounded
now, rather than waiting for new measurements. Their independent reviews are
complete and reconciled in [throughput.md](throughput.md). The earlier pipeline
is deferred; the maintainer approved and locked the CPU-first design on 2026-11-20.
A bounded two-group pipeline with independent completion and deliberate group
collection is recorded, unapproved, in
[group-commit-pipeline-proposal.md](group-commit-pipeline-proposal.md); adopting
it requires a spec revision.

Wall throughput, ACK latency and CPU efficiency are separate outcomes. The
maintainer reports vectored writes reduced WAL-write time, while CPU efficiency
remained approximately unchanged. Do not promise the CPU target from overlapping
waits alone. Existing reported experiments remain reported evidence, not newly
reproduced measurements.

## Locked throughput design

Three independent Codex reviewers inspected existing code and supported benchmark
readers, challenged their findings, and explicitly aligned on the same CPU-first
solution. The complete connected proposal, source evidence, budget, ownership,
read protections and later proof are in [throughput.md](throughput.md), with the
three independent reviews linked there. The maintainer explicitly agreed and
requested this design be locked on 2026-11-20. Its selected mechanisms,
80 MiB/CPU-s target and read-latency protections are settled; implementation
and performance proof follow later.

Retain compressed IPC chunks through WAL vectored writes instead of flattening
and shrinking them; narrowly extend the existing pinned Arrow encoder to reuse
bounded, charged compression/metadata scratch across independent slice streams;
reuse checked preflight counts and reserve projector columns; coalesce redundant
append-only inspection snapshots. Preserve the current serial durability flow,
canonical Arrow/Parquet read layouts, sorting, compression and resource admission.
Existing borrowed log decoding and canonical Arrow ownership are preserved
optimizations, not newly claimed savings.

This supersedes the earlier two-group pipeline as the selected CPU-efficiency
solution. No pipeline implementation is selected. The 80 input MiB/CPU-s target
requires approximately 26.76% less CPU per byte relative to the fixed logs-c64
baseline. Reviewer agreement selects the mechanism, not measured performance;
implementation must establish 80 and preserve competing-read latency and
sustainable staging/publication. The maintainer explicitly places that proof
later, after implementation. No production changes or new experiments were run.

## Locked direction

The maintainer explicitly agreed and requested this direction be locked:

- Preserve buffered client enqueue and durable background delivery. Durable
  batch completion still waits for grouped WAL sync and the batched PostgreSQL
  acceptance transaction. Do not add another durable step to that ACK path.
- Actual rows live in WAL, then independently recoverable staged Parquet and
  published storage. PostgreSQL holds recent accepted-batch receipts.
- A receipt suppresses repeated transport batches. While its accepted data
  still depends on WAL, it also selects the accepted attempt during recovery.
  Retain that overlap until the whole batch has independently durable custody.
- After the safe handoff, individual receipts serve bounded retry recognition
  and may expire under the agreed count/time policy. Their deletion never
  deletes the data or makes an accepted application decision disappear.
- Permanent approvals, evidence, execution revisions and GitHub delivery history
  come from their Bifrost records, with explicit treatment of accepted records
  not yet visible. Missing expired receipts do not establish historical absence.
- Count/time apply to server retry receipts per tenant-qualified logical table,
  across clients and Scribe pods: **one hour and 10,000 accepted batch identities**.
  The maintainer agreed to these values and explicitly confirmed they are locked.
  Either limit can end retry protection, subject to the settled recovery-safety rule.
- One bounded background Scribe cleaner receives coalesced wake-ups for custody
  progress/count pressure and periodic age checks. ACKs do not await cleanup.
- Existing workload/resource admission owns ingestion pressure. Cleanup health
  feeds retryable refusal through that same owner on failed or stale sweep
  coverage; no independent outstanding-receipt quota is selected. Reads remain
  available and staging/cleanup continue while new write admission is paused.
- Recovery guarantees accepted writes, including lost-response cases;
  never-accepted WAL attempts are left to producer retry.
- Retention prevents permanent metadata accumulation. It is not a claim that
  fsync or PostgreSQL waits become faster. Additional throughput changes need
  their own evidence; no failed pipeline or new acceptance architecture is revived.

The maintainer confirmed that count/time retention, durable recovery handoff,
and cleanup-health admission are settled mechanisms. Their
implementation must prove the stated crash/retry and capacity invariants; that
proof is not a reopening of the design. The retry defaults are locked at one
hour and 10,000 identities per tenant-qualified logical table. The exact
private field/helper layout remains implementation-owned. No implementation,
specification approval or readiness verdict is implied.

## Problem

`vala.scribe_batch_commits` retains acceptance fences permanently. Those rows
currently serve retry deduplication, WAL recovery decisions, and accepted
application-history reads. Deleting them by age or original-stream retirement
alone can change the meaning of an old duplicate WAL attempt and hide retained
application decisions.

## Desired outcome

Stop accumulating settled acceptance fences forever while preserving
acknowledged writes, tenant isolation, explicit retry semantics, and recovery
of approvals, evidence, execution revisions and GitHub delivery. Establish a
safe deletion rule, bounded retry history and pressure when cleanup cannot
keep up. This does not promise a strict instantaneous fence-row or disk-byte
ceiling, or a fixed expiration deadline for recovery-required receipts.

## Constraints and non-goals

- This is a standalone change packet. The maintainer directed all of this
  work out of the GitHub Verifier pipelines packet. The earlier held proposal
  is retained under `design/held-proposal/` for reference only; it is not
  approved input to this design.
- One shared write path; ordinary batched tenant transaction restored by the
  implementor. Vectored WAL writes are separate step-2 work.
- Preserve acknowledged data. No extra ACK-path SQL roundtrip, durable write,
  fsync or cleanup wait; the agreed local admission-health check is permitted.
  No per-fence UPDATE.
- Add state only when a concrete requirement and source evidence justify it.
  Previously rejected retry epochs, receipt archives, retirement tables and
  failed pipelines are not reopened implicitly.
- No new migration file; any necessary unreleased migration edits stay in place.
- Leave settled snapshot, orphan, maintenance scheduling and row-retention
  policies unchanged. No diagnostic or benchmark detour during this design.
- Design authoring changes no production code, spec approval or task readiness.

## Selected implementation

The maintainer agreed to guarantee durably accepted writes and leave
never-accepted writes to producer retry. This gives up opportunistic salvage
of complete but never-accepted batches. Lost responses do not undo durable
acceptance. Already staged/published custody follows its own durable recovery
evidence; it is not discarded because a fence expires.

With the current architecture, a batch becomes accepted only after WAL sync
and PostgreSQL fence commit. The recorded policy agreement does not authorize
moving that fence to background: doing so while recovery still requires it
would permit a returned success whose acceptance authority disappears on crash.
The maintainer agreed to keep PostgreSQL in durable batch completion and retain
the existing buffered producer/background delivery pattern. Do not replace it
with a WAL-only or Kafka-style acceptance architecture in this design. The
maintainer identifies internal Scribe/WAL contention and waiting as the
throughput work to address; a fast enqueue return does not establish background
drain capacity, and a fast INSERT alone does not eliminate commit or queue waits.

The handoff/checkpoint and cleanup-health mechanisms below carry the agreed
behavior. Implementing them still requires exact SQL/index choices, history-read
transitions and connected proof. Do not replace that engineering work with a
time-only DELETE or claim verification already exists.

The maintainer explicitly aligned with the ClickHouse-style separation of a
bounded recent-insert identity log from stored data and lasting application
history. The proposed fence lifecycle therefore has two independent conditions:
retain retry protection for its declared window from acceptance, and retain
any fence still required for recovery of acknowledged WAL-dependent data.
Reaching staged custody ends the latter dependency only once the existing
durable ownership evidence proves the entire canonical batch recoverable.
Staging completion alone must not expire a receipt still inside both retry
limits; age or count alone must not remove the sole recovery authority for
acknowledged data.

This is the agreed eligibility rule, not a verified SQL DELETE implementation.
The user-facing policy choice is the retry guarantee and its resource limits.
Identifying crash-safe handoff evidence and collector sequencing is engineering
design work, not a request for the maintainer to approve undocumented mechanics.
If handoff stalls, the recommendation is to preserve acknowledged custody and
use existing resource refusal/backpressure rather than deleting its last
recovery proof. Existing resource pressure and the cleanup-health mechanism below
are the selected enforcement; their executable proof comes after implementation.

## Evidence and rationale

- [Bifrost acceptance and retry identity](../../../../architecture/bifrost-design.md)
  already describes deduplication within a retention window. Its duration is
  locked here at one hour, with a 10,000-identity count limit per tenant/table.
- [Current SQL replay resolution](../../../../crates/vala/vala-sql/src/queries/scribe_batch_commits.rs:230)
  returns `Unfenced` for a missing fence and suppresses attempts whose coordinates
  differ from the surviving canonical fence.
- [Current Scribe replay](../../../../crates/vala/vala-bifrost-redux/src/scribe/shards.rs:1910)
  suppresses unfenced batches for fenced-only tables, but restores uncontested
  unfenced batches for ordinary tables. A second pod's old losing retry can
  therefore reappear after its winning fence is deleted. This is source-backed
  reasoning, not a runtime reproduction performed in this design.
- [Current acceptance-history contract](../../../../crates/vala/vala-sql/src/queries/scribe_batch_commits.rs:363)
  explicitly says fences never disappear and treats a missing fence as never
  acknowledged. Its four application consumers need a safe replacement before
  deletion is enabled.
- Bifrost's append/staging authority says WAL retires only after every cohort
  member has fsynced, validated, query-registered staged custody. Publication is
  later. The startup/recovery section explicitly preserves genuinely unfenced
  complete-commit recovery today. These are distinct facts; WAL retirement must
  not be described as proof of hot-object publication.

External grounding for the recovery decision:

- [Kafka delivery semantics](https://kafka.apache.org/41/design/design/)
  distinguishes committed data from a producer's receipt of an acknowledgement.
  A network failure can leave the producer uncertain; retries and producer
  idempotence address that ambiguity. This supports an explicit acceptance
  boundary, not an assertion that every high-throughput system uses Wyrd's
  PostgreSQL-fence implementation or discards every unacknowledged request.
- [Stripe idempotent requests](https://docs.stripe.com/api/idempotent_requests)
  permits pruning API-v1 retry keys after at least 24 hours and treating reuse
  after pruning as a new request. This illustrates finite retry protection; it
  does not select Wyrd's retry duration or safe recovery-metadata deletion rule.
- [OpenTelemetry Collector resiliency](https://opentelemetry.io/docs/collector/resiliency/)
  documents bounded queues/retries and optional persistent sending queues. A
  producer retry is not a guarantee of eventual delivery when its own queue is
  lost, fills or exceeds its retry limit. Requiring client retry therefore
  explicitly gives up opportunistic server salvage of never-accepted writes.

### Producer return versus backend completion

Verified `wyrd-client` buffered `insert_rows` calls the producer's synchronous
`enqueue_rows` (`bifrost/handle.rs:205`), and `wyrd-queue::Producer` admits rows
without waiting (`producer.rs:483`). The background queue waits for
`BatchSink::send` and retains the identical batch on ambiguous results
(`queue.rs:510`); `Producer::flush` waits through durable acknowledgement
(`producer.rs:655`). An application enqueue return is not a server ACK and waits
for neither WAL fsync nor PostgreSQL. PostgreSQL still contributes to background
delivery latency, drain throughput, queue occupancy and eventual backpressure.

[KafkaProducer](https://kafka.apache.org/41/javadoc/org/apache/kafka/clients/producer/KafkaProducer.html)
similarly buffers `send` calls and completes background delivery/futures according
to `acks`; `flush` waits for completion and exposes failures. Kafka's partition
leader, producer sequence deduplication and replicated log supply its acceptance
authority. Its replication protocol explicitly avoids requiring fsync on every
write ([Kafka design](https://kafka.apache.org/41/design/design/)); copying its
ACK timing without replication/ownership would not copy its durability model.

[ClickHouse async inserts](https://clickhouse.com/docs/concepts/features/operations/insert/asyncinserts)
batch on the server. Recommended `wait_for_async_insert=1` waits for flush;
mode 0 returns from an in-memory buffer and can lose data or hide flush errors.
Self-managed [ReplicatedMergeTree](https://clickhouse.com/docs/reference/engines/table-engines/mergetree-family/replication)
also performs Keeper/ZooKeeper metadata coordination on inserts. ClickHouse
therefore supplies direct counterevidence to the earlier suggestion that
high-throughput ingestion inherently requires eliminating per-batch shared
coordination. Its deduplication log has a finite scope; Kafka producer/sequence
deduplication has a different identity and ownership model.

Agreed direction: retain Wyrd's buffered producer plus durable background
completion pattern and design bounded retry/deduplication state within the
existing Scribe/PostgreSQL owners. Permanent application acceptance/history
comes from Bifrost records. PostgreSQL remains the synchronous cross-pod
deduplication arbiter for each accepted batch; successful application enqueue
still waits for neither PostgreSQL nor WAL. This preserves settled boundaries;
it is not evidence that fence transactions have no throughput cost or that
retention alone will improve throughput. The deletion and admission mechanisms
below are settled; their executable safety and capacity proof comes later.

## Decisions and alternatives

The problem, scope and earlier-proposal hold come from the maintainer's direct
instructions. The maintainer explicitly agreed to durable acceptance as the
recovery guarantee, retaining accepted writes even when their response was lost,
and leaving never-accepted writes to producer retry. The tradeoff is possible
loss of those never-accepted writes when the producer cannot retry, in exchange
for removing ambiguous unfenced-WAL salvage. This agreement does not itself
prove a safe fence deletion predicate, a throughput improvement, or relocation
of acceptance authority from PostgreSQL into WAL.
The maintainer subsequently agreed to retain the existing buffered delivery
and PostgreSQL acceptance architecture, bound retry-deduplication history, and
move permanent acceptance-history reads to Bifrost records. The CPU-first
throughput design in [throughput.md](throughput.md) now supplies the locked
80 MiB/CPU-s objective and mechanism; the failed pipeline remains excluded.
The retry defaults are one hour and 10,000 identities per tenant/table. The
separate receipt quota has been replaced by cleanup-health checks through the
existing admission owner. This adds no retry-history sizing decision.

### Step-2 implementation update supplied by the maintainer

The implementor reports vectored writes reduced mean WAL-write time from
7.9 ms to 4.0 ms in the first benchmark repetition. The supplied aggregate
request/throughput medians improved about 6.5%, within reported run-to-run
variation of about 8%; CPU efficiency was effectively unchanged and errors
were zero. Stage-time numbers represent only the first repetition, while the
other supplied results are medians. Record this as reported local-stage
improvement, not independently verified whole-run attribution or a proven
end-to-end throughput gain. No benchmark records were inspected or runs
executed during this design update. Remaining serial WAL-sync/fence waits are
relevant to the throughput objective; this summary alone does not authorize a
new concurrency or durability mechanism.

### Retry-window proposal and ClickHouse grounding

Current upstream ClickHouse `ReplicatedMergeTree` settings declare
`replicated_deduplication_window = 10000` and
`replicated_deduplication_window_seconds = 3600`. Its setting documentation says
old entries can be removed by either count or age; the age is relative to the
newest record, not wall time, and a sole record can remain indefinitely.
[Primary source](https://github.com/ClickHouse/ClickHouse/blob/master/src/Storages/MergeTree/MergeTreeSettings.cpp#L1496-L1528).
These are upstream defaults checked during this conversation, not settings
verified on any particular deployed ClickHouse cluster. Older defaults differ.

Locked Wyrd defaults: a one-hour retry-history target and 10,000 accepted batch
identities per tenant-qualified logical table. Both
limits bound retry protection, as in ClickHouse; a count limit can shorten the
effective time window at high batch rates. Do not claim a guaranteed hour if
eviction by count can remove an identity sooner. Size the cap in accepted
logical batches, not rows or fsync groups. The maintainer selected these values
after the count/time explanation. For illustration only, the breakdown's
reported 1180 batches/s implies 10000 identities cover about 8.5 seconds and a
full hour needs about 4.25 million identities at that rate. These are arithmetic
examples, not fresh capacity measurements or approved sizing requirements.

Recovery-required fences remain ineligible for eviction until independently
durable custody is proven. Existing unstaged-resource pressure and cleanup-health
admission below prevent continued acceptance when their respective work cannot
keep up; no independent receipt-count ceiling is promised. A retry-log count cap
cannot authorize data loss.

The maintainer aligned with applying both count and time to server-side retry
receipts per tenant-qualified logical table, across clients and pods, and chose
the discussed one-hour and 10,000-identity values. Either
limit can end retry protection once safe eviction is possible; neither changes
stored-data retention. Existing recovery obligations still constrain deletion.

### Agreed cleanup owner and triggers

The maintainer agreed to Scribe-owned cleanup on its existing runtime, outside
`ShardOwner::process_group` and the client ACK wait: coalesced push notifications
to one bounded background cleanup worker per Scribe process, with a periodic
wake for age expiry. The serial writer does not await that worker or its SQL.
The current shard owner handles successful staging (`handle_persistence_completion`,
`scribe/shards.rs:2769`) and owns the rotation-cohort WAL references. Forge's
current maintenance pass (`forge/gc.rs:160`) instead selects Iceberg-maintenance
members and has no local cohort/WAL custody proof. Assigning fence deletion to
that pass would require another durable eligibility handoff and must not be
justified by merely calling every kind of cleanup Forge work.

Agreed flow: Scribe observes safe custody progress and wakes the bounded
background cleaner; a periodic wake also handles elapsed age without another
insert; count pressure can request a coalesced wake. The cleaner uses tenant SQL
transactions and bounded DELETE batches. It must neither await cleanup on ACK
nor perform SQL inline on the serial shard command loop. A background worker is
justified by the required off-ACK count/time cleanup, not a new service, per-table
task pool or replacement maintenance scheduler. No such worker exists or is
implemented by this design record. Notifications request reconsideration of
affected identities, not permission to delete them and not a new per-batch
receipt ledger. Duplicate wake-ups coalesce; a full notification channel must
not block ingestion. Retry/periodic discovery must recover missed work from
authoritative state rather than make correctness depend on a volatile message.

Time/count are cleanup triggers, not deletion authorization. The cleaner must
add independently established recovery-safe eligibility and current canonical
stream identity. Across pods, count ordering covers the whole tenant/table;
local cache counts cannot enforce it. The locked mechanisms below define the
recovery checkpoint and admission pressure; their restart/replica behavior and
SQL/index implementation require connected proof. No dead or unavailable stream
is assumed safely retired. Forge is not the cleanup owner; no Scribe-to-Forge
eligibility handoff or change to its hourly maintenance policy is selected.

### Locked deletion proof and backlog enforcement

The maintainer confirmed these mechanisms as settled. The descriptions below
state what implementation must achieve; they are not claims of runtime proof.

**Recovery proof:** extend the existing local shard manifest with a monotonic
recovery-safe segment frontier, qualified by node, writer epoch and shard.
The existing rotation-cohort owner advances it only across a contiguous prefix
of closed segments after every accepted batch touching that prefix has all of
its slices in fsynced, validated, recoverable staged/published custody. A batch
straddling the boundary keeps it from advancing until the whole batch is safe.
Unresolved ownership or an earlier gap blocks advancement. Do not compute this
as the maximum of the current per-seal-key `sealed_lsn` values.

Persist the frontier through the existing atomic manifest mechanism before
SQL deletion or disposal of the evidence used to establish the prefix. A
notification then wakes the agreed cleaner. Its deletion predicate requires
the current row's exact canonical stream/shard, terminal COMMIT segment at or
below that stream's recorded safe frontier, and exclusion from count/time retry
history. SQL rechecks the current canonical coordinates so a stale candidate
cannot delete a replacement acceptance. A physical retirement function merely
returning success is not proof: current code may defer deletion or retain files
after a fault. The durable frontier is authority, not inferred file absence.

Use one frontier per stream/shard because current manifests contain
per-seal-key watermarks, not a durable whole-shard prefix. This extends an
existing recovery owner rather than adding a per-batch ledger or PostgreSQL
retirement table. The coarse checkpoint is an agreed local persistence change. Its
admission/corruption/replay effects and compatibility with existing stage-record
removal require connected verification; exact representation is implementation-owned. Replay loads this authority before
using removed fences as absence; WAL-dependent unfenced attempts follow the
agreed rejection policy. Independently staged data still restores from its
own records. No stale/offline stream gains a frontier by timeout.

**Recovery/staging pressure:** reuse the existing `AdmissionController`
unstaged immutable-byte bound (`scribe/admission.rs:27,105`), which already
refuses new requests as retryable busy when staging falls behind. It is not
proof of a fence-table count bound: it measures retained Immutable bytes and
does not count already-staged receipts awaiting SQL cleanup.

**Cleanup pressure:** the existing admission owner also checks a local cleanup
health signal. The complete locked mechanism is in
[cleanup health through existing admission](cleanup-health-proposal.md). Startup
verifies due work across this node's retained epochs before fresh admission.
Each background sweep captures a finite scope and policy cutoff, deletes safely
eligible receipts in bounded batches, and reports coverage only after completing
that scope. Successful small DELETE batches or heartbeats do not establish that
cleanup is keeping up. SQL failure/ambiguity or increasingly stale completed
coverage causes retryable refusal of new writes; already admitted work may
finish, reads stay available and staging/cleanup continue. Catch-up to a current
sweep restores admission. Recovery-pinned receipts alone are not cleanup failure.

This is a local admission health check, not another SQL roundtrip, fsync or
cleanup wait on the ACK path. Its failure behavior and throughput cost belong in
implementation qualification. No independent receipt quota, credit ledger,
per-fence UPDATE, new database table or per-table worker is selected. A restart
must not erase overdue cleanup obligations. A permanently abandoned node's
accepted local-only custody still requires recovery or an explicit decommission
or data-loss decision, never expiry by timeout.

The in-memory committed-batch mirror must honor receipt expiry so it cannot keep
acknowledging identities after their backing authority is gone. This mechanism
prevents indefinite continued acceptance during failed or inadequate cleanup;
it does not provide a strict instantaneous PostgreSQL row-count or physical-byte
ceiling. The maintainer accepted that distinction when replacing the quota.

**Connected proof:** accept data, stage it, persist the frontier, remove eligible
fences, restart and query exact rows; repeat with another pod's old duplicate WAL
and confirm it does not reappear. A failed stage/manifest write leaves receipts
protected. Hold staging or stall/fail cleanup while offering writes; prove the
appropriate existing resource or cleanup-health refusal, then restore progress
and prove admission resumes. Exercise ambiguity, missed wakes, startup debt,
small-batch progress without completed sweep coverage, and entirely pinned scope.
Crashes between frontier persistence, notification, SQL delete and health
publication must delay cleanup or admission, never lose acknowledged data.
No runtime proof has been executed.

### Responsibility separation and throughput limits

Explain the overlap explicitly: before independent staged custody, the current
PostgreSQL fence is both the cross-pod retry arbiter and the authority selecting
the accepted WAL attempt during recovery. The proposal does not fully separate
those two responsibilities at acceptance. It preserves that overlap to avoid
adding another per-batch durable accepted marker or another sync to the ACK
path. After the whole-batch handoff, staged records and the agreed coarse
recovery checkpoint replace the recovery dependency; surviving individual fence
rows then serve bounded retry recognition only. Lasting application history is
read from Bifrost records throughout, with transitional not-yet-visible acceptance
handled explicitly.

The intended write cycle stays grouped WAL writes/sync, one batched tenant
fence transaction, visibility and ACK. Checkpoint and SQL cleanup remain
background work. Cleanup-health admission is failure containment, not a throughput
optimization. Neither bounded retention nor the agreed checkpoint has been
shown to reduce the current fsync/fence wait. They prevent indefinitely growing
coordination state while preserving the low-latency WAL acceptance boundary.
Reducing that service cycle still requires separate source-backed improvements;
do not sell retention cleanup, extra counters or a renamed failed pipeline as
an established throughput gain.

External grounding: ClickHouse limits retry hashes separately from stored parts
and has insert delay/refusal thresholds when parts or cleanup debt accumulate
([settings source](https://github.com/ClickHouse/ClickHouse/blob/master/src/Storages/MergeTree/MergeTreeSettings.cpp)).
Kafka commits through its replicated partition log, requires recovering replicas
to resynchronize, and applies retention so consumers may fall outside retained
history ([design](https://kafka.apache.org/41/design/design/),
[consumer reset policy](https://kafka.apache.org/41/configuration/consumer-configs/#auto.offset.reset)).
Those are precedents for independently bounded retry/recovery state and explicit
failure policy; neither source proves Wyrd's frontier or cleanup-health implementation.

## Standards conflicts

`architecture/bifrost-design.md` startup/recovery currently requires genuinely
unfenced complete-commit recovery and forbids speculative missing-fence
suppression. Recovering only accepted batches revises that intentional behavior.
The concrete justification is the old cross-pod duplicate replay ambiguity after
finite fence deletion. Disposition: maintainer agrees to the acceptance policy;
the recovery and checkpoint mechanisms are locked above. The owning authority
update accompanies revision 1 of [the draft specification](../spec.md): the
owning recovery and receipt-lifecycle sections now record this settled policy.
This does not claim that implementation already conforms.

## Configuration defaults and implementation proof

The maintainer approved replacing the independent receipt quota with
[cleanup health through existing admission](cleanup-health-proposal.md).
Retry age/count and all other locked decisions remain unchanged. The agreed
tradeoff is progress-based failure containment rather than a strict
outstanding-row ceiling. No receipt-budget sizing decision remains.

Count/time retention, durable recovery handoff, the background Scribe cleaner
and cleanup-health admission are settled. Do not send these back
to design as unanswered choices.

Retry-count and retry-age defaults are settled: **10,000 identities and one
hour per tenant-qualified logical table**. They are not open design questions.
No separate per-node receipt quota is selected. Cleanup batch/cadence,
sweep-health deadline and private representation may be chosen
within the agreed bounded, off-ACK behavior without another design round.

Implementation owns SQL/index details, checkpoint ordering within existing
owners, cache expiry, accepted-history transitions and exact error handling.
Its proof must establish safe restart and retry, refusal when staging resources
or cleanup health require it, and resumption after progress returns. A
contradiction with a locked guarantee returns to design with direct evidence;
ordinary implementation uncertainty does not reopen the mechanisms.

Keep throughput proof distinct from retention safety. Vectored writes are
separate implemented work with the reported evidence above. The additional
CPU-first throughput design is finalized in [throughput.md](throughput.md);
the failed pipeline remains excluded.

## Locked delivery structure

The maintainer requests exactly two tasks, with a small number of logical
scenarios and write/read benchmark checks after every scenario. The maintainer
approved and locked this split on 2026-11-20:

1. **Fence lifecycle: three scenarios.**
   - Retained application history remains authoritative after retry receipts
     expire; recent retries and accepted-but-not-yet-visible records retain
     their agreed behavior across the four domain consumers.
   - Recovery preserves accepted writes across the WAL-to-staged handoff and
     rejects never-accepted/losing attempts, including cross-pod duplicates;
     the durable contiguous checkpoint survives interruption and restart.
   - Background Scribe cleanup removes only eligible receipts. Failed, hung or
     increasingly stale sweeps pause fresh writes through existing admission;
     restart verifies old debt and catch-up resumes admission. Cover missed
     wakes, ambiguous SQL and recovery-pinned receipts; keep reads and staging
     available. Count/time governs retry history, not stored rows.
2. **CPU throughput: three scenarios, after the fence task.**
   - Prepare/project identical accepted rows using checked capacity facts and
     coalesced append-only inspection snapshots, preserving input refusals.
   - Carry independent compressed IPC chunks through vectored WAL writes,
     preserving byte/digest parity, leases, partial-write behavior and replay.
   - Reuse bounded, charged encoder context between independent streams,
     preserving dictionary isolation and failure cleanup; establish the
     combined 80 input MiB/CPU-s target on the fixed comparison workload.

Equivalent success/failure, concurrency and tenancy variants belong within
these six logical scenarios, not separate benchmark-triggering scenarios.
Each scenario is a coherent test-first implementation increment; the final
task packet must state its connected starting state, input, output and proof.

Establish one matching baseline before the first scenario. After each scenario's
focused correctness checks pass, run the same logs-c64 and read-c64 checkpoint
serially. Compare both against the original baseline and the previous accepted
checkpoint, so small cumulative regressions cannot disappear. Use unchanged
fixtures, settings and CPU accounting, supported readers, normal repetitions
and existing criteria; a result inside spread is unchanged, not a gain.
Stop progression on a material regression or invalid/inconclusive checkpoint
and resolve it before the next scenario. The 80 target is final CPU-task
acceptance, not a requirement that every intermediate scenario reach 80.

The repository's write selector is `otlp-logs-c64`. There is no literal
`bifrost-read-c64` mise task/step in the inspected owners. Agreed read coverage
uses the existing hot `point-memtable-c64` and Parquet `point-c64` selections,
without introducing a new benchmark. Scenario subsets are progress evidence,
not full qualification. Keep the previously agreed full-write/read and competing
load qualification once at completion, outside additional scenario subdivision.

This is a locked delivery design, not an executable task packet or specification
approval. Retry age and count are locked at one hour and 10,000 identities per
tenant-qualified logical table; do not ask the maintainer to settle them again.
Cleanup-health admission replaces the separate per-node receipt budget.
Private SQL, index, scratch, cleanup-batch
and cadence choices remain implementation-owned within the locked guarantees;
implementation and performance proof are later work, not open design decisions.

## Handoff

The throughput design, retention mechanisms and two-task/six-scenario delivery
structure are locked, including write/read checkpoints after every scenario.
The retry-age/count defaults and cleanup-health amendment are locked. No material
design decisions remain open. The design is ready for specification; this does
not approve a specification or start planning/implementation automatically.
There is no approved spec or executable task
in this new packet yet. No production edits, new experiments, runtime proof or
reactivation of the held Task 018/revision-18 proposal is implied.
