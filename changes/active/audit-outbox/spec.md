---
id: SPEC-audit-outbox
revision: 4
status: approved
---

# One non-blocking audit outbox

## Objective and user value

Permissions block; audits do not. Every authorization decision is staged on
one process-wide, batched audit outbox and no request waits for, or is refused
by, an audit write. Audit throughput then scales with replicas instead of
serializing every request of a tenant on that tenant's audit chain-head row.

## Evidence

Capacity benchmark run 3 (2026-10-03, two `all` replicas, sustained 202/s):
the only statements slower than 1 s in the whole run were
`SELECT ... FROM vala.audit_chain_head ... FOR UPDATE` — zero on one replica,
about 200 per minute per replica on two (mean wait 1.3 s, max 3.9 s), almost
all on the noisy tenant. Callers: Gate writes of verification results, Drift
features, and Eval items, and `start_run`. Publication p95 rose from 0.5 s to
30 s; achieved throughput fell from 201.7/s to 179.7/s; the quiet tenant (own
chain head) stayed healthy. `AuditPublisher::freeze_publication_range` takes
the same row `FOR UPDATE NOWAIT` and failed nearly every sweep, even on one
replica, so staged audit was never published during load.

Current write modes (three, where doctrine allows one):

1. Batched outbox: `wyrd-server` `oracle/query_audit.rs` `OracleQueryAudit`
   (Oracle reads, tenant tripwires, direct verification).
2. Per-event background task: gateway invocation decisions.
3. Synchronous in-request, fail-closed with `*_AUDIT_UNAVAILABLE`: Gate writes
   (`bifrost/gate_audit.rs`), verification `start_run`, cards, operators,
   principals, admin, platform provisioning/identity/recovery, storage, OTLP,
   query service, auth (`wyrd-auth` token grants, API keys, revocation, card
   scope, platform authz/sessions), Oracle peer authority, and gateway
   administration.

The "surfaces not yet converted still append in the deciding transaction"
clause in `AGENTS.md` and `architecture/wyrd-design.md` permitted mode 3.

## Scope

- One generic outbox type in a shared crate (REQ-008), with audit as its
  first sink. The Eval run-request outbox (verified-change-contract REQ-077)
  becomes its second sink.
- One server-owned audit outbox (generalized from `OracleQueryAudit`) used by
  every audited surface in `wyrd-server`, `wyrd-auth`, Gate, Oracle, gateway,
  and verification.
- Removal of the per-event and synchronous append paths and every request
  refusal caused by an audit write.
- Audit publication state separated from the append lock.
- Doctrine, design, and generated-doc updates.

## Non-goals

- Changing what is audited, event content, hash chaining, retained history in
  `vala.system.audit_log`, or the single `AuditPublisher`.
- Durable (crash-safe) outbox queuing beyond the process; an event still
  queued at abrupt process loss may be lost, as doctrine already accepts.
- Engine-internal lineage (Scribe batch commits, Forge operations), which is
  not audit.

## Required behavior

### REQ-001 — One outbox

The server owns exactly one audit outbox per process. Every audit decision on
every surface is staged on it with a non-blocking enqueue and the request
proceeds. No other code path appends to `vala.audit_staging`.

### REQ-002 — Batched tenant commits

The outbox writer drains everything waiting, groups by tenant, and commits each
tenant's events with one chain-head lock and one commit per batch. Tenants are
committed concurrently up to a bounded number of writer connections, so one
contended tenant does not delay other tenants' audit.

### REQ-003 — Audit never refuses or delays a request

No request returns an error because an audit write failed, and no request
waits on an audit commit. The outbox has no count limit and never drops an
event because Postgres is slow or unavailable. A tenant batch whose connection,
append, or commit fails stays queued at the front of that tenant's queue,
absorbs that tenant's later events behind it, and is retried with exponential
backoff. Other tenants keep committing. Every failed attempt is logged with the
tenant and error and counted in `outbox_write_failures_total{outbox="audit"}`. No memory is preallocated for queues or batches. A batch may be
split only to fit one statement's parameter limit.

### REQ-003a — Accepted loss

An event is lost only when the process stops abruptly, or when graceful
shutdown reaches its deadline with the event still unwritten. Shutdown counts
every event it leaves unwritten in `outbox_events_lost_total{outbox="audit"}`
and logs it.

### REQ-004 — Audit-unavailable errors removed

Error variants and stable codes that exist only to report an audit write
failure are removed from server, Gate, Oracle, SDK, proto, schema, and docs
surfaces: `WYRD_VALA_500_AUDIT_UNAVAILABLE`, the query/Bifrost
audit-unavailable codes, and Gate `IngestError::AuditUnavailable`.

### REQ-005 — Decision before effect, audit after

A permission decision is evaluated and enforced before the operation. The audit
event is staged once the decision is known (allowed or denied), never inside
the operation's own transaction, and never holds a lock the operation needs.
`start_run` no longer holds the chain head across run enqueue.

### REQ-006 — Publication does not contend with appends

Publication progress (`published_seq`, `publishing_seq_hi`) lives in state the
appenders never lock. The publisher freezes and advances its range without
taking the chain-head row lock, so publication proceeds during sustained
appends. Staged rows remain garbage-collected only after the watermark passes
them.

### REQ-007 — Shutdown

Graceful shutdown stops accepting new events and keeps committing and
retrying what is queued until the shutdown deadline. It reports the remainder
and counts it as lost (REQ-003a).

### REQ-008 — Generic outbox machinery

The queue and writer are one generic type in a shared crate with no SQL
dependency. Audit and Eval run requests each create their own instance, so a
slow database on one side does not delay the other. Each use supplies only a
sink: its item type, a metric label, and one call that writes one tenant's
items in one transaction, all or nothing. The generic type owns the following.

- Non-blocking staging.
- The unbounded queue and the pending count, exported as the
  `outbox_pending{outbox}` gauge.
- Per-tenant grouping, with at most one write in flight per tenant.
- A bounded number of tenants written at once, set by the constructor (audit
  uses 4).
- Retry of a failed write, at the front of its tenant's queue, with backoff
  from 50 ms doubling to 5 s.
- Idle signalling for tests, graceful-shutdown flush, and loss counting.
- The metrics `outbox_write_failures_total{outbox}`,
  `outbox_events_lost_total{outbox}`, and `outbox_pending{outbox}`.

No other outbox implementation exists.

### REQ-009 — A committed audit write is never retried

The audit writer retries a batch only after Postgres confirms that the earlier
attempt did not commit. The writer records the transaction ID of each write
(`pg_current_xact_id()`). When the commit returns an error, it asks Postgres
for that transaction's outcome (`pg_xact_status`) and acts on the answer:

- committed: the write succeeded and is not retried;
- aborted: the batch is retried;
- in progress, or Postgres unreachable: the writer waits and asks again, and
  re-sends nothing until the outcome is known.
- unknown (Postgres no longer holds the status): the events are counted in
  `outbox_events_lost_total{outbox="audit"}` and are not re-sent.

Each decision is therefore staged, and retained, exactly once. Audit staging
and the retained audit log carry no event ID, and readers do no deduplication.
No second audit table, ledger, WAL, relay, or retirement delay is added.

## Invariants

- INV-001: Every permission check completes before its operation proceeds or
  is refused.
- INV-002: Per-tenant audit sequence and hash chain remain gap-free and
  ordered for committed events. A retried batch commits before any later event
  of the same tenant.
- INV-003: Tenant isolation: each batch commits under its own tenant binding.
- INV-004: One audit write path and one publisher.

## Expensive-to-reverse decisions

- Removal of the audit-unavailable public error codes (a public contract
  change across HTTP, gRPC proto, Python/TypeScript/Rust SDK error mapping, and
  generated docs).
- Audit for durable writes (Card registration, provisioning, key issuance) is
  no longer atomic with the write: a committed write whose event is lost to
  abrupt process loss or an expired shutdown deadline has no audit row. A
  failed commit is retried, never dropped.
- The generic outbox type and its sink trait in a shared crate (REQ-008).
- Resolving an unknown commit outcome from Postgres transaction status
  before any retry (REQ-009). No event-ID column exists on staging or on the
  retained audit log.
- Moving publication progress out of `vala.audit_chain_head` (migration).
- Removal of the "not yet converted" clause from `AGENTS.md` and
  `architecture/wyrd-design.md`.

## Acceptance criteria

- AC-001 (REQ-001, INV-004): No production code outside the outbox writer
  calls the staging append. Static check or test.
- AC-002 (REQ-003, REQ-004): For each surface family (Gate write, start_run,
  Card registration, auth token grant, admin, Oracle), an injected audit
  commit failure leaves the request successful and increments the failure
  counter. Once the failure clears, the event commits exactly once in chain
  order. Isolated tests. One user-journey test proves that a Gate write and a
  run start succeed while audit fails, and that their events commit after
  recovery.
- AC-003 (REQ-002, INV-002): Concurrent decisions for one tenant from two
  replicas commit gap-free, ordered chains. Integration test on Postgres.
- AC-004 (REQ-006): The publisher advances retained history while appends run
  continuously against the same tenant. Integration test.
- AC-005 (scale): Proved on the integrated branch after merge. In `mise run bench:capacity` (verified-change-contract
  REQ-171), the audit outbox meets that run's saturation SLO in every judged
  step and the two-replica scale-out step passes. No separate audit
  benchmark exists.
- AC-006 (REQ-004): `mise run codegen:check` and the OpenAPI contract test pass
  with the codes removed; no reference remains.
- AC-007 (REQ-007, REQ-003a): Shutdown commits queued events within the
  deadline. Events still unwritten at the deadline are counted in
  `outbox_events_lost_total{outbox="audit"}`.
- AC-008 (REQ-008): Focused tests of the generic type with a test sink prove
  the following.
  - A failed write is retried and committed exactly once, in order, ahead of
    that tenant's later items.
  - One failing tenant does not delay another tenant.
  - There is no count limit.
  - Shutdown flushes until the deadline and counts the remainder as lost.
  - `pending` returns to zero.
- AC-009 (REQ-009): A commit that succeeds in Postgres but returns an error to
  the writer is not retried. A commit that is aborted is retried and commits
  once. Repeated ambiguous commits, with the publisher retiring rows in
  between, leave exactly one retained row per decision and a gap-free chain.
  Postgres integration tests through the production writer and publisher.

## Open material decisions

None for revision 4.

## Authority links

- `AGENTS.md` §2 (audit is non-blocking, one write path, one publisher)
- `architecture/wyrd-design.md` (principal model, audit path)
- `architecture/bifrost-design.md` (Gate, Oracle audit)

## Revision history

- Revision 1 (2026-10-03, approved): Route every audit decision through one
  batched, non-blocking outbox after the capacity benchmark showed synchronous
  per-request chain-head locking reduced two-replica throughput below one
  replica.
- Revision 2 (2026-10-03, approved): A failed commit is retried with backoff and
  never dropped. The queue limit and preallocation are removed. Loss is limited
  to abrupt process stop and an expired graceful-shutdown deadline, and is
  counted. This aligns with the approved consistency principle in
  `architecture/bifrost-design.md` and with the Eval run-request outbox
  (verified-change-contract REQ-077, rev 60), which will share this outbox's
  machinery. Adds REQ-008: one generic outbox type with per-use sinks and
outbox-labelled metrics. Adds REQ-009: an audit event ID with (tenant, event
ID) uniqueness, so a retry after an unknown commit outcome cannot duplicate
audit. Source: r1 review FIND-AUDIT-OUTBOX-1. Approved by the user on
2026-10-03.
- Revision 3 (2026-10-03, approved by the user): r2 review FIND-AUDIT-OUTBOX-11 showed
  staging-only event-ID uniqueness cannot prevent a duplicate once the
  publisher retires the staged row before an unknown-outcome retry. Audit
  delivery is at least once: the event ID is carried into the retained audit
  log and readers collapse rows sharing it. Rejected: a retirement delay (not
  guaranteed, grows staging by the delay's volume) and a separate event-ID
  table (contradicts one audit write path).
- Revision 4 (2026-10-03, approved on user direction "write the code so these
  types of issues are not representable"): the r3 review showed that event-ID
  deduplication cannot bound duplicates, because each ambiguous commit followed
  by retirement adds another retained copy. Root cause: the writer retried
  without knowing whether the previous commit had succeeded. The writer now
  resolves the outcome from Postgres transaction status before any retry, so a
  duplicate cannot be written. The event-ID columns, the staging migration, and
  reader deduplication from revisions 2 and 3 are removed.
