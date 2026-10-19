# Owner notes after the advise proposal

Input for `$wyrd-spec`. `advise-proposal.md` is the advise output;
`architecture-diagram.html` draws the agreed shape over today's runtime.

## Hard constraints

1. Nothing in the machinery is coupled to something Wyrd does not control.
   Telemetry may be one place session evidence lives; it is data, not
   machinery. Sessions never require a client to use tracing.
2. Designed for load: hundreds of requests per second per deployment,
   multi-tenant, bursty (retail peaks).

## Decisions taken in discussion

- **Session is a runtime resource**, like a Verifier execution: server-created,
  has an ID, lifecycle and state, never authored or versioned. Not a Card kind.
  A sealed session is exactly the closed evidence bundle a Verifier already
  reads. An inline request bundle is a session sealed on arrival, so today's
  `/execute` and `/enqueue` paths keep working unchanged.
- **Sessions are earned.** Many producers writing to one session is extremely
  common in retail commerce (chat agent, human handoff, order, payment, CRM).
- **Keyed by a caller-supplied ID** (order ID, conversation ID), tenant-scoped.
  The first append for a key opens the session; producers never coordinate a
  Wyrd-minted ID. One open revision per key.
- **Producer attribution:** every contribution records the sending principal;
  Verifiers see it as data and audit stays attributable.
- **Two orders:** server acceptance order (authoritative for membership) and
  producer timestamps (what Verifiers sort by). Never conflate them.
- **Closure:** idle timeout plus optional explicit close by any authorised
  producer, bounded by a maximum age and size. No single producer knows the
  conversation is over.
- **Late evidence → session revisions.** A sealed revision and its verdict are
  immutable. Late evidence for the key opens revision N+1 = revision N (by
  parent reference, not copied) + new items. Revision N+1 is verified whole.
  Revision N's result stays as history, marked replaced; the session's current
  verdict is the latest revision's. A reopened revision has its own idle
  timeout, so a burst of late events yields one revision and one
  re-verification.
- **In-flight on revision:** a queued verification for an older revision is
  dropped (latest revision per session+binding only); a running one finishes
  and is marked replaced.
- **Operators react to verdict changes**, not every verdict.
- **Revision cap** per session in tenant quotas. **Finality window** (tied to
  retention, e.g. 90 days): after it, late evidence for the key is refused or
  starts a fresh unlinked session.
- No per-binding "rerun or ignore late evidence" option; re-verification is
  the default until cost proves otherwise.
- Budget exhaustion is an execution reason, not a new verdict
  (`passed | failed | inconclusive` stay).
- Human checks deferred.
- Per-contribution checks run once on arrival; revisions do not re-run them.
- Phase 1 (bounded admission, durable acceptance, durable results before
  terminal state) is needed regardless of long-horizon work.

## Open owner decisions (raise one at a time)

1. Durability domain of an acceptance receipt: pod loss only, or node/disk
   loss too (advise recommends node/disk for SaaS).
2. Retention and deletion of session evidence (retail sessions carry PII).
3. Whether human judgments are reproducible evidence or attributable review.
