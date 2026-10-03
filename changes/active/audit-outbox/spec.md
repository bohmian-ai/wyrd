---
id: SPEC-audit-outbox
revision: 1
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
waits on an audit commit. A full queue or failed commit is logged with the
operation and request id and counted in one `audit_outbox_commit_failures_total`
counter labelled by surface; the event is lost.

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

Graceful shutdown stops accepting new events, commits what is queued until the
shutdown deadline, and reports the remainder.

## Invariants

- INV-001: Every permission check completes before its operation proceeds or
  is refused.
- INV-002: Per-tenant audit sequence and hash chain remain gap-free and
  ordered for committed events.
- INV-003: Tenant isolation: each batch commits under its own tenant binding.
- INV-004: One audit write path and one publisher.

## Expensive-to-reverse decisions

- Removal of the audit-unavailable public error codes (a public contract
  change across HTTP, gRPC proto, Python/TypeScript/Rust SDK error mapping, and
  generated docs).
- Audit for durable writes (Card registration, provisioning, key issuance) is
  no longer atomic with the write: a committed write whose event is lost to a
  failed commit or abrupt process loss has no audit row.
- Moving publication progress out of `vala.audit_chain_head` (migration).
- Removal of the "not yet converted" clause from `AGENTS.md` and
  `architecture/wyrd-design.md`.

## Acceptance criteria

- AC-001 (REQ-001, INV-004): No production code outside the outbox writer
  calls the staging append. Static check or test.
- AC-002 (REQ-003, REQ-004): For each surface family (Gate write, start_run,
  Card registration, auth token grant, admin, Oracle), an injected audit
  commit failure leaves the request successful and increments the counter.
  Isolated tests; one user-journey test proves a Gate write and a run start
  succeed with audit failing.
- AC-003 (REQ-002, INV-002): Concurrent decisions for one tenant from two
  replicas commit gap-free, ordered chains. Integration test on Postgres.
- AC-004 (REQ-006): The publisher advances retained history while appends run
  continuously against the same tenant. Integration test.
- AC-005 (scale): In `mise run bench:capacity` (verified-change-contract
  REQ-171), the audit outbox meets that run's saturation SLO in every judged
  step and the two-replica scale-out step passes. No separate audit
  benchmark exists.
- AC-006 (REQ-004): `mise run codegen:check` and the OpenAPI contract test pass
  with the codes removed; no reference remains.
- AC-007 (REQ-007): Shutdown commits queued events within the deadline.

## Open material decisions

None for revision 1.

## Authority links

- `AGENTS.md` §2 (audit is non-blocking, one write path, one publisher)
- `architecture/wyrd-design.md` (principal model, audit path)
- `architecture/bifrost-design.md` (Gate, Oracle audit)

## Revision history

- Revision 1 (2026-10-03, approved): Route every audit decision through one
  batched, non-blocking outbox after the capacity benchmark showed synchronous
  per-request chain-head locking reduced two-replica throughput below one
  replica.
