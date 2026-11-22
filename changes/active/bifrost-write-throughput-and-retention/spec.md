---
id: SPEC-bifrost-write-throughput-and-retention
revision: 3
status: approved
---

# Bifrost write throughput and bounded retry history

## Problem and outcome

Bifrost currently keeps PostgreSQL batch-acceptance fences indefinitely. Those
receipts both recognize transport retries and select accepted WAL attempts during
recovery; application history also relies on them. Deleting old receipts without
replacing those dependencies could hide an approval or restore a losing duplicate.
Meanwhile, the selected logs-c64 baseline delivers 58.589 input MiB per server
CPU-second. Faster WAL writes alone have not established better CPU efficiency.

This change has two outcomes: safely retire old retry receipts, and reach
**80 input MiB per server CPU-second** on the unchanged OTLP logs-c64 comparison
without sacrificing reads, durability or sustainable background delivery.

For example, an approval remains final after its retry receipt is removed because
the approval's Bifrost record remains authoritative. A transport resend of an old
ordinary batch after its retry identity has actually been removed may insert again.
Finite transport deduplication does not expire stored rows or application decisions.

## Agreement and approach

The finalized [design](design/notes.md), [CPU design](design/throughput.md) and
[cleanup-health amendment](design/cleanup-health-proposal.md) supply this revision.
The amendment replaces the earlier receipt quota; held proposals and exploratory
review alternatives are not selected. The owning contracts are
[Bifrost architecture](../../../architecture/bifrost-design.md) and the existing
[change workflow](../../spec-driven-development.md).

The client still buffers writes and delivers them in the background. Scribe still
syncs WAL, commits the batched PostgreSQL acceptance transaction, makes rows visible
and acknowledges. After every slice of an accepted batch has independent durable
custody in staged or published data, a persisted shard checkpoint allows its old
receipt to be removed. One background Scribe cleaner handles deletion; existing
admission pauses fresh writes if cleanup fails or falls behind.

The CPU work removes repeated scanning, allocation and copying: reuse checked
input facts, reserve projector capacity, carry compressed Arrow IPC chunks through
vectored WAL writes, reuse bounded encoder scratch, and coalesce redundant append
inspection snapshots. The durability cycle and read representation stay unchanged.

## Required behavior

### Acceptance, retries and application history

**REQ-001 — Preserve producer and durable completion boundaries.** Buffered
`wyrd-client` enqueue returns without waiting for PostgreSQL or WAL. Its background
queue retains the same batch identity on retry; flush/shutdown wait for delivery
under their existing contracts. A server success still requires WAL fsync, the
batched tenant PostgreSQL acceptance transaction and memtable visibility. This
change adds no ACK-path SQL roundtrip, durable write, fsync or cleanup wait. The
local admission-health check below is permitted. Cross-pod acceptance arbitration
remains PostgreSQL-owned on the shared write path.

**REQ-002 — Bound recent retry history by age and count.** Retry-history defaults
are **one hour from acceptance and 10,000 accepted batch identities per
tenant-qualified logical table**, across clients and Scribe pods. A receipt is
outside recent history when either its age has elapsed or it falls outside the
newest 10,000 identities; PostgreSQL owns coordination time. Count can therefore
shorten protection below one hour. While a receipt remains authoritative, matching
retries and contradictory content retain the existing identity, version and
conflict semantics for client-supplied batch IDs. A Gate-derived OTLP batch ID
is itself the XXH3-128 content identity, so its retries are recognised by ID
alone. After actual deletion, an old identity may be accepted again.
The in-memory committed-batch mirror must not acknowledge identities whose backing
receipt has been removed. These limits do not expire stored rows.

**REQ-003 — Preserve permanent domain decisions independently of receipts.**
Execution revisions, approvals, evidence and GitHub delivery outcomes use their
actual Bifrost records for lasting history, including retry/conflict decisions.
After receipt deletion, the same retained decision must still be found and applied;
an execution's recovered revision sequence must not reset or truncate merely
because receipts expired. Consumers read only their Bifrost records. A record accepted
but not yet visible reads as absent; its fixed batch id makes any second write for
it a retry or a refused conflict, never an overwrite, and the consumer's next read
finds it. A failed read is unavailable and retries.
Domain conflict rules remain in force after transport retry protection expires.

### Recovery and safe deletion

**REQ-004 — Recover accepted writes, suppress never-accepted attempts.** A durably
accepted batch survives restart even if its response was lost. For data still
dependent on WAL, recovery requires committed acceptance authority and restores
only its canonical attempt. Complete but never-accepted attempts, including another
pod's losing retry, restore no rows, even after the winning receipt is later
deleted. Independently staged/published data restores from its own durable records
without requiring an expired receipt. Unknown, unavailable or contradictory
authority fails closed rather than being treated as a confirmed missing receipt.
The intentional cost is giving up opportunistic salvage of never-accepted writes;
their producer must retry, and may lose them if it cannot.

**REQ-005 — Persist a whole-batch recovery checkpoint.** Extend the existing local
shard manifest with a monotonic safe frontier qualified by node, writer epoch and
shard. It covers only a contiguous prefix of closed WAL segments for which every
accepted batch touching the prefix has **all** slices in fsynced, validated,
recoverable staged/published custody. A crossing batch, unresolved ownership or
earlier gap prevents advancement. Persist the frontier with the existing atomic
manifest mechanism before deleting receipts or discarding its supporting evidence.
Replay loads it before interpreting missing receipts; stage-record retirement and
torn-tail validation must remain safe after individual fences disappear. A maximum
per-seal-key watermark, physical file absence or successful retirement call is not
this proof. An unavailable/offline stream never becomes safe merely by timing out.

**REQ-006 — Delete only safely handed-off receipts outside recent history.**
Deletion requires both exclusion from the age/count history and the recovery-safe
handoff. In tenant-scoped SQL, recheck the current canonical stream/shard coordinates
and require its terminal COMMIT segment at or below that stream's persisted safe
frontier. Stale candidates cannot delete a replacement acceptance. Failed staging
or checkpoint persistence must leave recovery-required receipts protected. Receipt
removal neither deletes accepted rows nor releases their remaining custody roots.

### Background cleanup and ingestion pressure

**REQ-007 — Keep cleanup off the serial writer and ACK path.** One bounded
background Scribe cleaner per process runs on the existing runtime. Coalesced
custody/count notifications request reconsideration; a periodic wake handles age
and missed notifications through durable-state discovery. Full notification
channels do not block ingestion. The cleaner uses bounded tenant SQL DELETE
batches and the safe predicate above; notifications alone authorize no deletion.
Forge does not own this cleanup, and the serial shard command loop does not run
or await cleanup SQL.

**REQ-008 — Verify cleanup coverage before admitting more work.** Startup begins
cleanup-unverified and completes an initial due-work sweep before fresh write
admission, reconstructing ownership and checkpoints including this stable node's
retained epochs. Each sweep captures finite ownership scope and a policy cutoff
using PostgreSQL time, then reconciles all safely eligible receipts in that scope
at that boundary. New arrivals belong to subsequent sweeps. Publish successful
coverage only after the whole captured scope is resolved and deletions are confirmed
committed; ambiguous SQL requires reconciliation. Empty eligible scope counts as
success; receipts pinned solely for recovery do not constitute cleanup failure.

**REQ-009 — Refuse fresh writes when cleanup fails or cannot keep up.** The existing
admission owner checks cleanup health locally. SQL failure/ambiguity or completed
sweep coverage older than a bounded progress deadline causes retryable-busy refusal
of new writes. Freshness is anchored to the sweep boundary, not the eventual finish
time of an old sweep. Heartbeats and small successful DELETE batches cannot renew
health without completed coverage; a hung/exited worker must also become stale.
Already admitted writes may finish under the unchanged ACK contract, bounded by
existing in-flight admission. Reads, accepted-row visibility, unrelated workloads,
staging, recovery and cleanup remain usable; do not poison memory accounting or
process readiness. Completing a current sweep within its deadline automatically
restores admission, subject to existing resource pressure.

Existing workload, memory, queue, in-flight and unstaged-backlog admission remain
the ingestion-pressure owners. No receipt quota or credit ledger is added. Cleanup
health prevents indefinitely continuing acceptance during failed/inadequate cleanup;
it does **not** guarantee an instantaneous receipt-row or PostgreSQL disk-byte cap.
Recovery pins and pending sweeps can retain more than 10,000 receipts; physical
space reuse also depends on PostgreSQL vacuum. Cleanup failure can refuse writes
even when INSERT itself still works. Private batch sizes, cadence and associated
health deadline remain implementation decisions within these guarantees.

### CPU throughput and compatibility

**REQ-010 — Reuse checked preparation work without changing accepted input.**
Reuse bounded preflight counts/facts and reserve decoder/projector capacity rather
than repeatedly scanning/growing the same data. Preserve protobuf merge, oneof,
fallback, error-selection and refusal semantics, including admission before costly
allocation. Preserve existing borrowed log decoding and canonical Arrow ownership.
Coalesce only redundant append-only inspection publication after mailbox drain;
retain control-command publication and publication required before ACK.

**REQ-011 — Carry compatible compressed IPC through WAL.** Join each slice's
pinned-encoder output once into one exact-length payload that the WAL writes in a
single vectored write; carrying unjoined chunks measured slower. Keep each slice an
independent standard IPC stream with exact compressed-byte parity to the current
writer under the existing settings. Logical identities and WAL payload digests are
XXH3-128 (16 bytes), replacing SHA-256 in place in the WAL, replay and PostgreSQL
receipts with no compatibility path, since no installation holds earlier data.
Preserve ordered lengths/CRC, the COMMIT binding of ordered slice digests,
canonical logical identity input and its existing volatile-field exclusions. Many-chunk writes,
partial/interrupted/zero-progress writes and errors retain existing rollback and
replay behavior. Payload owners and memory leases remain live until IO settles.

**REQ-012 — Reuse bounded encoder scratch safely.** Narrowly extend the existing
pinned Arrow encoder's context lifecycle to retain compression/metadata scratch
between independent slice streams on existing workers. Reset schema, dictionaries
and end-of-stream state for every slice; decoding must not depend on another stream
or tenant. Bound retained scratch and charge actual retained allocation capacity,
including sliced buffers' backing allocations; release oversized scratch. Failure
and cancellation return or release context without sharing live output allocations.
No custom IPC codec, global encoder lock, unbounded pool or new worker count.

**REQ-013 — Meet the CPU target without moving costs onto readers or later work.**
On the unchanged `otlp-logs-c64` input population, reach **80 input MiB/server
CPU-second**, with improvement outside normal repetition spread, using whole-server
CPU accounting. Do not average other payloads into that result or substitute stored
Arrow bytes. Report the full write matrix separately; other populations must not
materially regress. Preserve existing hot/Parquet read criteria and read tails
against the matching baseline, bounded memory, query visibility, sustainable
staging/publication backlog, drain and recovery under competing load. Calibrate
the competing workload from improved write capacity so reads face the new offered
ingest load. A short ACK-only run, deferred background CPU or faster waiting with
unchanged CPU efficiency does not meet this requirement.

Keep serial durability, current ingress/persistence concurrency, canonical columns,
Variant representation, sorting, learned layouts, Parquet geometry, compression,
indexes and statistics. Do not bypass validation, weaken fsync, move materialization
to queries or bypass claim assembly with general staged-file passthrough.

## Scope and material constraints

This standalone packet covers the shared Scribe write path, its PostgreSQL receipt
owner, the four affected domain consumers and the narrow pinned Arrow extension.
To keep REQ-013's hot-read criteria under improved ingest, it also covers two
read-side CPU reductions with unchanged results: pruning captured memtable batches
before building their projected statistics, and physically planning Oracle's
already-optimized logical plan without a second optimization pass.
Tenant isolation and existing permission/content checks apply on every retry path.
Necessary unreleased SQL/index changes edit existing migrations; no new migration
file, database table, receipt archive, per-fence UPDATE or per-batch retirement ledger.

The prior eager two-group pipeline, extra shards, PL/pgSQL/held-connection fencing,
WAL-only acceptance and independent receipt quota are excluded. Snapshot protection,
Forge maintenance membership, orphan cleanup and stored-row retention stay outside
this change. The separate trace-ingest design keeps its own scope; unimplemented
trace work is not counted as a logs saving.

The owning architecture is updated with the agreed accepted-only recovery policy
and receipt lifecycle. These are changes to the existing recovery contract, not a
claim that current implementation already conforms. No unresolved material design
decision remains. SQL/index details, private representation and dependency API shape
remain implementation-owned within the selected constraints.

## Acceptance and delivery

Planning produces exactly **two tasks, three logical scenarios each**, fence first
and CPU second. The following is the single requirement-to-proof mapping; equivalent
tenancy, concurrency and failure variants stay inside these six scenarios.

| Task / scenario | Required observation | Obligations |
|---|---|---|
| Fence / lasting history | Accept decisions through their real consumer paths, expire receipts, and recover/query/retry the four domain records. History and conflicts remain correct; unreadable accepted rows can appear absent, conflicting saves are refused, and failed queries retry. | REQ-001–003 |
| Fence / recovery handoff | Accept, stage, persist the frontier, delete eligible receipts, restart and query exact rows. Repeat with lost responses, crossing batches, earlier gaps, failed stage/manifest persistence and another pod's losing WAL attempt. No accepted data is lost and no losing/unaccepted attempt reappears. | REQ-004–006 |
| Fence / cleanup and pressure | Exercise age/count eviction, missed/coalesced wakes, SQL failure/ambiguity, hung worker, slow partial progress, restart debt, entirely recovery-pinned scope and catch-up. Assert safe deletion, retryable refusal/resumption, completion of admitted work and continued reads/staging. | REQ-002, REQ-006–009 |
| CPU / preparation | Pass equivalent valid and invalid inputs through checked preparation/projection; observe identical canonical rows, refusals and required inspection visibility. | REQ-001, REQ-010 |
| CPU / chunked WAL | Compare exact compressed bytes and identities across nested/null/dictionary/sliced inputs; retain leases through many-chunk and short/error writes, then recover accepted rows through the real WAL path. | REQ-001, REQ-004, REQ-011 |
| CPU / bounded reuse and combined target | Decode consecutive independent streams using reused context, exercise failure/cancellation and retained-memory bounds, then establish the combined CPU target with read and sustained-delivery protections. | REQ-012–013 |

Establish a matching baseline before the first scenario. After each scenario's
focused correctness checks pass, run the same checkpoints serially through existing
mise benchmarks: writes `otlp-logs-c64`, hot reads `point-memtable-c64`, and Parquet
reads `point-c64`. These are the agreed meaning of logs-c64/read-c64; no new benchmark
is introduced. Compare each with both the original baseline and previous accepted
checkpoint using unchanged fixtures, settings, CPU accounting, normal repetitions
and supported report readers. Inside-spread results are unchanged, not a gain.
Resolve material regressions or invalid/inconclusive checkpoints before progressing.
The 80 target applies to final CPU-task acceptance, not every intermediate scenario.

At completion, run full writes, hot reads, Parquet reads and the existing mixed
competing-load qualification once, under [benchmark rules](../../../benchmarks/AGENTS.md).
Scenario subsets are progress evidence, not full qualification. Required owner,
integration and user-journey checks follow repository policy; the plan supplies
exact existing runners and implementation records actual results. If the implemented
design misses 80 or regresses protected behavior, report the evidence and return
the unmet outcome to design; do not lower the target or begin another redesign.

## Revision and approval

Revision 1 records the finalized agreement. The maintainer authorized proceeding
with this revision by requesting its decomposition into implementation-ready tasks
on 2026-11-20. Status is **approved**. Planning does not start implementation.

Revision 2 is approved by the maintainer’s instruction on 2026-11-21 to fix the
spec and rerun review. It records the R1 Bifrost-only readers: newest-readable
recovery, temporary absence for unreadable accepted rows, fixed-identity conflict
protection and retry on query failure. The owning architecture and acceptance
wording are reconciled with that decision; other requirements are unchanged.

Revision 3 is approved by the maintainer on 2026-11-21. It adds to scope two
read-side CPU reductions that keep REQ-013's hot-read criteria under improved
ingest, with unchanged results: memtable pruning before projected statistics, and
a single logical optimization pass per Oracle query. Other requirements are unchanged.
