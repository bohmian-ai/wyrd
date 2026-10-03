# Durability and persistent-data domain review

## Immutable subject

- Base: `6714ae35d732814240fdcc42fc226c079b14d3f0`
- Candidate: `5a5542cbb965af99e92a3983a2ddf586412cea73`
- Approved authority: `changes/active/audit-outbox/spec.md`, revision 2
- Remediation task: `changes/active/audit-outbox/review/r1/TASK-AUDIT-OUTBOX-R1-remediation.md`
- Domain: audit-event identity, retry idempotency, hash-chain append, publication, retained history, watermark settlement, staging garbage collection, and schema migration/backfill

`HEAD` matched the candidate before this review. The repository has no
`.codegraph/` directory, so source navigation used repository search and direct
caller/consumer tracing.

## Boundary and authority coverage

| Boundary | Authority and source inspected | Result |
|---|---|---|
| In-memory identity and retry ownership | Spec REQ-003, REQ-003a, REQ-007 through REQ-009; `wyrd-runtime/src/outbox.rs`; `vala-sql/src/audit_outbox.rs` | The generic writer retains the same `StagedAuditEvent`, including its v7 `event_id`, at the front of the tenant queue after a reported write failure. Retry ordering, bounded concurrency, backoff, pending accounting, and deadline abandonment match the approved shape. |
| Canonical append and hash chain | Spec REQ-009 and INV-002; `vala-sql/src/queries/audit_staging.rs`; `vala.audit_chain_head`; `vala.audit_staging` | Concurrent append/retry while the original row remains in staging is serialized correctly: the chain-head lock covers the event-ID lookup, sequence allocation, insert, and head update. Already-present IDs consume no new sequence. The identity fence is not durable for the entire retry lifetime; see `DUR-PDATA-001`. |
| Publication and garbage collection | Spec REQ-006 and REQ-009; `wyrd-server/src/audit/publication.rs`; `freeze_publication_range`, `list_publication_range`, and `settle_publication`; `architecture/bifrost-design.md` | Publication is intentionally independent of the append lock. After Scribe durably accepts a frozen range, settlement advances `published_seq`, clears the matching bound, and deletes the staging prefix in one transaction. That deletion also removes the only persisted event ID. |
| Retained audit schema and Scribe fence | `vala-bifrost-redux/src/tables/audit/projection.rs`; `tables/audit/audit_log.rs`; Scribe batch identity derived from tenant and frozen sequence range | `event_id` is neither loaded into `AuditStagingRow` for publication nor projected into `vala.system.audit_log`. Scribe deduplicates replay of the same frozen sequence range, not a later append of the same logical decision under a new sequence. It therefore cannot absorb the failure in `DUR-PDATA-001`. |
| Migration and existing-row backfill | `20260802000000_vala_audit_staging.sql`, `20261003000000_audit_publication_progress.sql`, `20261003000001_audit_staging_event_id.sql`, and `pg_migration.rs` | The forward migration gives each existing staging row a UUID, drops the default, and adds tenant-scoped uniqueness. The event ID is deliberately outside the chain hash, so backfill does not invalidate existing hashes. Named-column consumers remain compatible. No defect was established in the SQL ordering or backfill itself, but no upgrade test seeds old staging rows and applies this exact event-ID migration. |
| Required proofs | AC-002, AC-005, AC-007, AC-008, AC-009; `pg_audit_outbox.rs`; the Bifrost audit-publication journey; remediation evidence | Failure-before-commit recovery, shutdown, tenant independence, and retry while rows remain staged have recorded coverage. There is no proof of retry after an unknown successful commit once publication has garbage-collected staging. AC-005 remains an integration gate and was not run in this candidate; the named `bench:capacity` task is not present in this worktree. |

## End-to-end durable identity trace

1. `StagedAuditEvent::from` assigns one `event_id`, and `OutboxWriter` returns
   that same item to the front of the tenant queue when `AuditSink::write`
   reports an error.
2. `append_audit_events` locks `vala.audit_chain_head`, reads matching IDs only
   from `vala.audit_staging`, allocates sequence numbers for the IDs not found,
   inserts them, and updates the head in the same transaction.
3. A PostgreSQL commit can succeed while the client observes a connection or
   commit error. The outbox must then retain and retry the original item, as
   required by REQ-003 and REQ-009.
4. The independent `AuditPublisher` may freeze the newly committed sequence,
   project it through Scribe, and call `settle_publication`. Settlement advances
   the watermark and deletes that staging row. No publication payload or other
   durable table retains its `event_id`.
5. On the scheduled retry (initially 50 ms, later up to 5 s), the canonical
   append no longer finds the ID. It treats the decision as fresh, consumes the
   next sequence number, and computes a new valid hash over the same logical
   decision. A later publication range has a different sequence-derived Scribe
   batch ID, so it is retained again.

This race does not require an unusually fast five-second sweep. A sweep can
already be active, Tokio's first interval tick is immediate, and an ambiguous
commit response may be delayed while the committed row is visible to other
connections. More importantly, neither the architecture nor the implementation
orders staging garbage collection after the outbox learns the commit result.
Timing makes the race more or less likely; it cannot make the staging-only
identity fence durable.

## Material proposed finding

### DUR-PDATA-001 — publication can erase the retry identity before an unknown-commit replay

- **Classification:** `INCORRECT`
- **Violated obligation:** REQ-009 and AC-009 require a write retried after an
  unknown commit outcome to produce no duplicate and consume no second sequence
  number. The `OutboxSink` contract also requires repeating the same write to
  leave the same durable state. AC-002's exactly-once recovery promise is not
  established for an ambiguous successful commit. REQ-003's retry makes this
  path mandatory rather than optional.
- **Exact location:**
  `crates/vala/vala-sql/src/queries/audit_staging.rs:111-130` limits deduplication
  to IDs still in `vala.audit_staging`;
  `crates/vala/vala-sql/src/queries/audit_staging.rs:489-520` deletes the
  published staging prefix;
  `crates/vala/vala-bifrost-redux/src/tables/audit/projection.rs:172-265` and
  `crates/vala/vala-bifrost-redux/src/tables/audit/audit_log.rs:47-63` omit the
  event ID from retained history;
  `crates/shared/wyrd-runtime/src/outbox.rs:342-373` necessarily retries every
  reported sink failure after backoff.
- **Evidence and reachability:** A commit that succeeds server-side and loses
  its response is the explicit failure REQ-009 was added to handle. Publication
  and settlement run in another task and use progress state appenders never
  lock. Once settlement deletes the row, no sibling consumer or durable fence
  can recognize the replayed ID. The remediation report itself records this as
  a residual risk. The current AC-009 test at
  `crates/vala/vala-sql/tests/pg_audit_outbox.rs:185-215` performs the second
  write before any freeze, Scribe append, watermark advance, or deletion, so it
  proves only the narrower live-staging case. The surface journey injects an
  insert failure that certainly rolls back; it does not create an ambiguous
  successful commit.
- **Observable consequence:** One authorization decision can appear twice in
  authoritative retained history under two sequence numbers and two valid hash
  entries. The hash chain remains gap-free, so ordinary chain validation does
  not expose the duplicate. Audit cardinality, compliance evidence, incident
  reconstruction, and policy/accountability consumers can therefore report two
  decisions where one occurred.
- **Required correction boundary:** `SPEC_REVISION_REQUIRED`. The smallest safe
  correction must preserve an event-ID fence beyond staging-row garbage
  collection, or otherwise establish a durable ordering/acknowledgement that
  makes deletion impossible before the ambiguous writer is resolved. Revision
  2 fixes the expensive-to-reverse decision as uniqueness on transient
  `vala.audit_staging`; the governing architecture also fixes publication
  progress to exactly a watermark plus one bound, requires garbage collection
  after the watermark, prohibits another audit table/WAL/relay, and makes
  retained-history shape a non-goal. Choosing where the identity survives, how
  long it survives, how it is indexed/garbage-collected, and how concurrent
  publishers and writers coordinate is a new persistent-data and concurrency
  decision. This review therefore does not prescribe a table, retained-schema
  field, tombstone, grace period, or lock protocol as implementation
  remediation.
- **Focused closure proof after approval:** Force an actual ambiguous commit
  outcome while retaining the original in-memory `StagedAuditEvent`; drive the
  production freeze -> Scribe append -> settlement path until the first staging
  row is deleted; retry the exact original batch; then prove the chain head did
  not advance, staging contains no reintroduced row, and retained history has
  exactly one decision. Include a later same-tenant event to prove it neither
  overtakes nor merges with the replay. The migration proof should also seed
  pre-event-ID staged rows, apply the exact forward migration, verify distinct
  non-null IDs and unchanged chain bytes, and publish those rows through the
  normal consumer.

## Prior-finding closure and regression assessment

- `FIND-AUDIT-OUTBOX-1` is materially improved: failed batches stay pending,
  retry at the front, and no longer disappear on ordinary acquire/append/commit
  errors. Its unknown-success branch is not closed end to end because the new
  dedup identity can disappear independently while the batch remains pending.
- `FIND-AUDIT-OUTBOX-4` now has a real Oracle request in the shared failure
  journey, and the request remains independent of audit persistence. That proof
  covers deterministic staging refusal and recovery, not ambiguous commit plus
  publication/garbage collection.
- The prior publication-progress, frozen-range replay, Scribe batch-fence,
  watermark, and transactional garbage-collection conclusions remain sound for
  replay of one already-frozen range. `DUR-PDATA-001` is a different replay:
  append assigns a new sequence after the original event-ID evidence has been
  deleted, so it necessarily derives a new publication range identity.
- No additional migration/backfill, chain-hash, tenant-isolation, shutdown, or
  cross-tenant-progress regression was established from the reviewed source.

## Verification limits

- This review inspected source, migrations, consumers, the cumulative diff,
  prior review artifacts, and the recorded focused/broader lane results. It did
  not rerun the broad lanes.
- No existing test supplies the exact unknown-commit -> publish -> garbage
  collect -> retry ordering. Green retry, publication, and hash-chain tests are
  therefore not evidence against `DUR-PDATA-001`.
- AC-005's capacity proof and the integration `gate` were explicitly deferred
  in the remediation evidence. `mise.toml` in this candidate has no
  `bench:capacity` task, so this review cannot close AC-005. This is recorded as
  an integration verification limit rather than a second durability defect.
- The candidate remained unchanged at `5a5542cbb965af99e92a3983a2ddf586412cea73`
  during source inspection.

## Overall result

**FAIL**

`DUR-PDATA-001` is a reachable exactly-once durability failure at the seam
between the new retry loop and the existing transient-staging publication
lifecycle. Closing it requires a human-approved persistent-data/concurrency
decision before implementation remediation; the current revision does not
authorize a safe minimum correction.
