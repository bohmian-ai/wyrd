---
id: TASK-018
kind: implementation
status: proposed
execution_hold: true
spec: SPEC-github-verifier-pipelines
approved_spec_revision: 17
proposed_spec_revision: 18
obligations: [REQ-018, REQ-025, REQ-026, REQ-027, REQ-028, INV-001, INV-002, INV-003, INV-004, INV-005, INV-007]
depends_on: []
---

# Supplemental task: shared Bifrost writes and production retention

> HELD HISTORICAL PROPOSAL. Moved from the GitHub Verifier pipelines packet at
> the maintainer's direction. This proposal remains on hold and includes
> superseded scope. It is not the selected design or an implementation input.
> The current design is `changes/active/bifrost-write-throughput-and-retention/design/notes.md`.


Increase throughput for every Scribe ingress path and stop retaining acceptance
fences forever, without making an accepted approval, evidence record, execution
revision or GitHub delivery disappear during recovery. Make Forge maintain idle
and continuously busy tables and separate transport retries, snapshot history,
orphan cleanup and evidence-row lifetime.

## Authority and pickup

The maintainer requested this supplemental task on 2026-11-20 from the selected
recommendation. Its behavioral obligations are defined once in the
[draft revision-18 supplement](../spec-revisions/18-bifrost-writes-and-retention.md).
Revision 17 does not authorize the changed fence/retention contracts. Approval
and incorporation of that amendment, and reconciliation of the owning
architecture, precede affected production edits. This task is proposed, not
ready; no approval of revision 18 or runtime result is claimed.

Inputs are the already integrated Scribe, outbox, Forge and acceptance-reader
implementations. No output from profiling Tasks 015–017 or completion of the
hot-read task is required. Do not reorder Tasks 013/014 or reopen completed
allocation work. One cohesive task owns the consumer migration and deletion
activation together so cleanup cannot ship ahead of safe recovery readers.

The reviewed breakdown had a historical four-round-trip fence baseline. The
current dirty tree instead contains an unproven PL/pgSQL function and held
per-shard connection. Remove that replacement and restore the existing batched
tenant transaction, while preserving unrelated transport/driver improvements.
Recheck the actual diff at pickup; this instruction does not authorize reverting
other contributors' work. WAL can retire after validated query-registered local
staging, before hot publication; retirement and publication are separate inputs
to fence cleanup eligibility.

## Existing owners and selected approach

**Shared append.** Extend `ShardOwner` in
`crates/vala/vala-bifrost-redux/src/scribe/shards.rs` and the existing
`WalSegment` append-counter/sync ownership in `scribe/wal.rs`. The serial group
lifecycle cannot overlap I/O today; merely spawning `process_group` would race
reservations, duplicate suppression, rotation and uncertainty. Keep one ordered
owner and allow at most two in-flight groups per shard: preceding durable
completion overlaps assembly/append of the next group. Keep accumulating ready
requests under existing grouping and byte bounds; introduce no smaller fixed
groups or artificial collection timer. Charge overlap to existing admission.

Use the existing multi-row INSERT and batched conflict resolution in
`vala-sql/src/queries/scribe_batch_commits.rs`. Run independent tenant fence
transactions concurrently, capped at four across a shard. Preserve ordering
within each tenant/table and prevent later dependent work passing an uncertain
earlier group. Each transaction has ordinary tenant-scoped RLS and releases its
connection after completion; never combine tenants in one transaction or hold
the connection over unrelated WAL/object work. Complete successful independent
tenant members without coupling their ACK to another tenant's transaction.

All native, OTLP and outbox producers continue through this owner. Reuse
`wyrd-server/src/scribe_outbox.rs` destination batching; direct SDK appends retain
their existing ACK boundary. Do not add client-side durable behavior.

**Acceptance and fence cleanup.** Migrate these consumers before activating
finite deletion:

- `wyrd-server/src/verification/approvals.rs`: accepted decisions;
- `wyrd-server/src/verification/evidence.rs`: accepted subscriber evidence;
- `wyrd-server/src/verification/state.rs`: accepted execution revision sequence;
- `wyrd-server/src/components/github/delivery.rs`: settled delivery outcomes.

Read their existing Bifrost records by domain identity, with live fences covering
accepted-but-not-visible records. Batch IDs are not stored on every data row;
do not fabricate such a column or substitute a new receipt archive. Revision
recovery must not reset to zero when old fences disappear. Conflict/retry paths
must consult retained domain decisions, not just change inspection readers.

Extend Forge's existing bounded maintenance and `file_list` completion owners
for tenant-scoped fence deletes. Retain settled publication proof until its
dependent aged fences are safely removed, then allow ordinary proof/file-list
cleanup. Preserve newest-fence torn-tail recovery and shared-cohort replay
safety. Use PostgreSQL admission timestamps for age and no per-fence UPDATE.
Provide the necessary existing-table deletion index and normal vacuum support
without introducing a new table, job service, migration file or ACK-path query.

**Maintenance and row lifetime.** Extend the existing Forge leader/scheduler,
`gc.rs`, `expire.rs`, `orphan_gc.rs`, `settings.rs`, worker and SQL task owners.
Use `bifrost_table_maintenance_authority`, snapshot supersessions and existing
task evidence to rediscover work each term and during bounded passes. Separate
maintenance eligibility from compaction debt while sharing the existing
exclusive table lease, resource limits and uncertain-operation reconciliation.

Carry explicit per-table row retention through the existing table configuration
and maintenance flow. Reuse the table's authoritative time field and Forge's
mutation/commit/reachability protocols. Preserve the absence-of-policy default;
do not infer durations from table kind. Required contracts and thin projections
belong in `wyrd-spec`, `wyrd-client` and the shipped SDK/HTTP/MCP/CLI surfaces
that expose table configuration, with no UI-owned policy. Keep new public
configuration limited to the retention policy needed for REQ-028.

Before production changes, reconcile the owning sections of
`architecture/bifrost-design.md` (append/group commit, acceptance, maintenance
discovery, snapshot and definition retention), security posture and operations
recovery contract. Preserve six-hour reader protection, the 24-hour orphan age
floor and no per-query SQL. These owners already handle leases, recovery and
reachability; a parallel cleanup coordinator is unnecessary.

## Ordered acceptance scenarios and implementation evidence

Implement one scenario at a time. Add actual assertion locations, managed
focused commands, observed RED, GREEN and limitations to these entries. Existing
correct safeguards use existing proof; do not manufacture RED or fault hooks.

1. **Shared durable write and overlap** — REQ-025; INV-001,003.
   Starting with real Postgres and Scribe, submit concurrent native Arrow,
   OTLP traces/logs/metrics and server-outbox batches from two tenants through
   their production ingress. Owner integration proof establishes that subsequent
   admitted work can assemble/append while preceding durable completion waits,
   within the two-group/four-transaction bounds. Real-server journeys assert
   successful batches return their exact rows only to their tenant and preserve
   each ingress's ACK/staging contract. The serial implementation should fail
   the overlap assertion; a one-surface optimization is insufficient. Extend
   redux integration and `wyrd-testing` Scribe/OTLP/server journeys, with public
   SDK ingress coverage in the existing language owners.

2. **Retry, refusal and restart preserve accepted writes** — REQ-025;
   INV-001,003–005. Through the same production clients, retry identical and
   contradictory batches within the window, exercise concurrent tenants and
   resource pressure, then restart and recover accepted work. Assert exact
   row counts, existing conflict/pressure outcomes, no early ACK and no lost
   acknowledged rows or double-released ownership. Exercise ordinary shutdown
   and restart, ambiguous fence recovery and the existing repeated-identity
   guards using established owner fixtures. These are regression proofs around
   the pipeline, not newly invented failure contracts. Tier 1 Scribe journeys
   and tier 2 redux/SQL own the connected and transactional assertions.

3. **Accepted domain state survives expired transport fences** — REQ-026;
   INV-002–005. Obtain an accepted approval, deferred subscriber evidence,
   multiple execution revisions and a GitHub delivery outcome through the real
   server paths. Publish them, expire their eligible fences through Forge and
   restart/retry the consuming workflow. Assert the original decision/evidence,
   latest accepted revision and settled provider outcome are recovered; a
   conflicting decision remains refused, no settled verification/provider work
   repeats and unavailable reads never imply absence. Include accepted records
   not yet query-visible to prove the live-fence bridge. Existing permanent-fence
   readers should fail after cleanup. `wyrd-testing` server journeys with mocked
   provider transport and real Bifrost prove delivery; `wyrd-server` integration
   covers the four consumer seams and conflict paths.

4. **Fence expiry respects both time and recovery roots** — REQ-026;
   INV-001,003–004. With PostgreSQL-aged batches and real WAL/staging/publication,
   run Forge before the hour, after the hour with unsettled custody, and after
   safe retirement/publication. Assert only eligible tenant-qualified fences
   disappear, publication proof outlives its dependent fences, and restart
   retains exact acknowledged data, including shared cohorts and torn tails.
   An identical native/OTLP resend inside the window deduplicates; after actual
   eligible deletion it may append again. Repeated cleanup is idempotent and
   normal aged, settled history drains instead of accumulating forever.
   Redux/SQL integration owns deletion and proof ordering; Scribe/Forge
   journeys assert real consumer-visible rows. Age-only or publication-only
   deletion must fail this proof. Assert bounded retained ownership/backpressure
   for unresolved work without claiming a fixed physical PostgreSQL size.

5. **Idle and busy tables retain maintenance service after takeover** —
   REQ-027; INV-001,003. Create due maintenance from real committed tables, leave
   one idle and another with continuing compaction debt, then replace the Forge
   leader without new commits to the idle table. Assert existing durable owners
   rediscover and service both tables through bounded passes, preserving tenant
   fairness, exclusive ownership and replayable uncertain effects. Volatile
   membership and compaction-based orphan skipping should fail. Forge tier-1
   leadership/maintenance journeys and tier-2 scheduler/SQL proof own this case.

6. **Expiration preserves readers and unfinished writes** — REQ-027;
   INV-001,003–004. Run an Oracle query against a snapshot that becomes
   superseded, then exercise expiry and object cleanup before/after the allowed
   supersession window. Assert the captured query stays correct through its
   permitted lifetime and only unrooted expired objects are reclaimed. Exercise
   young never-published output and an unresolved attempt: the age floor and
   reachability protection prevent deletion. Use existing Forge/Oracle journeys
   and redux expiry/cleanup integration; reuse already-correct protection proof
   and extend it only where scheduling changes affect the actual flow.

7. **Configured row retention removes only eligible data** — REQ-028;
   INV-001–005. Configure a table through the supported public table surface,
   write rows before/at/after a pass cutoff into whole-expired and mixed files,
   and run Forge. Assert rows strictly before the cutoff disappear only when
   no workflow/recovery root protects them, surviving rows remain exact, and a
   late expired arrival becomes eligible on the next pass that sees it. A table
   without a policy and a foreign tenant's table retain their rows. Restart
   around mutation/publication and prove safe recovery and snapshot-protected
   reclamation. Missing row TTL should fail the removal assertion. Forge tier-1
   journeys prove mutation-to-query behavior; shipped SDK/HTTP/MCP/CLI table
   configuration journeys prove the actual typed policy reaches that owner.

8. **All-write throughput qualification** — REQ-025 and existing REQ-018.
   Use existing compatible full clean L1 records, then the unchanged full
   `mise run bench:l1:bifrost:writes` experiment after implementation. Compare
   all declared write populations, not just OTLP logs c64, for throughput,
   latency, CPU efficiency, memory and exact reconciliation. Retain inherited
   verification/gateway no-regression qualification for the shared outbox and
   workload owners. No new percentage allowance, numerical speedup promise,
   diagnostic/profile phase or compulsory L2 experiment is introduced. Existing
   evidence that already answers the comparison is reused; an unmet target or
   regression is reported, not hidden by VALID or exit zero. Follow the
   benchmark owner's report/compare workflow and run experiments serially.

## Verification and exclusions

Use existing `integration` targets in `vala-bifrost-redux`, `vala-sql` and
`wyrd-server`, plus `wyrd-testing` Scribe/OTLP/Forge/server journey targets.
Managed runners are `mise run test:bifrost:integration:redux`,
`mise run test:sql`, `mise run test:bifrost:integration:server`, and the affected
`mise run test:bifrost:journey:<capability>` lanes. Shipped language/configuration
proof uses the existing SDK/MCP runners in the packet README. No existing test
is prescribed by name; confirm and record exact nextest/pytest/vitest commands
for added tests, with managed Postgres/storage setup, before RED. Integrated
review owns complete shipped-surface lanes; unit-only evidence is insufficient.

Run affected contract regeneration and `mise run codegen:check`, Rust
`mise run fmt` / `mise run lints`, `mise run check:deps` and
`mise run check:tenant-isolation`; apply SDK checks where projections change.
Performance commands are the existing `mise run bench:l1:bifrost:writes`,
`mise run bench:l1:verification` and `mise run bench:l1:gateway`, under
`benchmarks/AGENTS.md`. These qualify the delivered change; they are not a
pre-implementation diagnosis assignment. Do not commit automatically to obtain
a clean benchmark checkpoint. End with focused diff inspection and
`mise exec -- git diff --check`.

No UI, new service/table, permanent receipt archive, retry epoch, per-fence
UPDATE, compression/window/export-timer change, shard-count increase,
asynchronous durability ACK, arbitrary evidence TTL default or new migration
file. Existing tests and boundary checks may not be weakened to admit the
implementation. Until revision 18 is approved and authority reconciled, this
document is a supplemental proposal only.

## Comprehension handoff

An independent read-only reader checked the current spec, draft amendment,
this task and its README entry together. It found no material comprehension
gap, contradiction or unsupported obligation, and correctly distinguished
finite transport deduplication from retained domain acceptance, the protected
row-retention flow and the pending approval boundary. This checks the document
handoff only; implementation feasibility and runtime results remain unverified.
