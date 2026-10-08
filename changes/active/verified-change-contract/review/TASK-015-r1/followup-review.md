# Focused follow-up review — TASK-015 r1

## Subject and limits

- Immutable candidate rechecked before this report:
  `9b560d00569656ee7fdde19a9c76fa55c8d55fbb`.
- Task range inspected: `3f8767a5f..9b560d00569656ee7fdde19a9c76fa55c8d55fbb`.
- Merge context inspected only as the conflict resolution in merge
  `c5527627a50dd66a9f53d760d80f59bcb59609f9`, through its combined diff and
  parent identities. `git show --remerge-diff` could not create Git's temporary
  object directory in this checkout; that limitation does not affect either
  uncertainty investigated here.
- This is a discovery pass resolving only `DOMAIN-CONCURRENCY-1` and
  `MNT-015-001`. It does not vote on TASK-015 or validate the rest of the
  discovery finding union.
- No Cargo or `mise` command was run. No production code or tests were changed.

## Inspected authority, source, and reports

- Approved contract and task:
  `changes/active/verified-change-contract/spec.md` revision 60, especially the
  Trigger-activation definition, REQ-075–REQ-077, REQ-102–REQ-108, and AC-014;
  `changes/active/verified-change-contract/tasks/TASK-015-eval-runs-in-the-batch-fence.md`.
- Current architecture:
  `architecture/wyrd-design.md` (continuous Eval and Trigger),
  `architecture/references/domain/evaluation.md`, and the applicable Rust
  documentation rules in `AGENTS.md`, `architecture/agent-rules.md`,
  `architecture/references/languages/rust-core.md`, and
  `architecture/references/languages/maintainer-style.md`.
- Conflicting reports:
  `domain-review-concurrency-durability.md`, `maintainer-review.md`,
  `standards-review.md`, plus the behavior, invariant, and system reports for
  the claimed activity-at-flush interpretation.
- Runtime path:
  `vala-bifrost-redux/src/gate/{auth.rs,mod.rs}`,
  `vala-bifrost-redux/src/tables/eval/observations.rs`,
  `wyrd-server/src/verification/observations.rs`,
  `wyrd-runtime/src/outbox.rs`,
  `wyrd-sql/src/queries/{verification.rs,verifier_runs.rs}`, and
  `wyrd-auth/src/issuance.rs`.

## Conflict 1 — temporal meaning of REQ-108

### Authority resolution

The approved contract requires activity at the occurrence, not merely at an
arbitrarily delayed successful flush.

- The specification defines continuous Eval's Trigger activation as a
  **successfully committed observation** (`spec.md:110-113`). Current Wyrd
  authority says the same thing: each committed Eval observation activates its
  subject's matching active binding (`architecture/wyrd-design.md:1093,
  1294-1304`; `architecture/references/domain/evaluation.md:9-16`).
- REQ-108 requires the Eval flusher to restrict new work to runtime-active exact
  owners, but immediately adds the temporal invariant: an **inactive
  occurrence** creates no activation or run and must not be backfilled after a
  later authentication (`spec.md:588-601`). Changing revision 59's owner from
  the Scribe batch-fence insert to the run-request flusher did not delete or
  qualify that no-backfill sentence (`82f142580`, the revision-60 approval
  diff).
- REQ-077 makes the delay real: a failed flush retains the committed record and
  retries later (`spec.md:772-795`). It does not redefine the committed
  observation's occurrence time as the time of a later retry.

The contrary activity-at-flush reading satisfies only REQ-108's first
sentence. It cannot satisfy the next sentence when an owner becomes eligible
between the committed occurrence and the first successful flush. Therefore
`DOMAIN-CONCURRENCY-1` identifies a real conflict with approved behavior.

### Reachable producer-to-insert sequence

1. A qualifying API-key or workload exchange updates the exact owner's
   `last_authenticated_at` with PostgreSQL `statement_timestamp()` in the token
   transaction (`verification.rs:59-75,382-430`;
   `issuance.rs:441-443`). An owner that has not performed such an exchange, or
   whose stamp is stale, is inactive under REQ-107.
2. An active publisher can commit an Eval observation for a subject that has
   bindings owned by another exact Service/Agent version. Observation routing
   is by subject, and the batch lookup selects every `observations_ready`
   binding whose `subject_card_uid` matches; it does not require the binding
   owner to be the publisher
   (`verifier_runs.rs:115-128,1640-1693`). Shared/reused subject Cards therefore
   make an inactive binding-owner occurrence reachable without requiring an
   inactive principal to authenticate the ingest call.
3. Gate awaits Scribe durability, and only the first committed attempt calls
   the acknowledgement hook (`gate/mod.rs:970-1009`). The acknowledged Eval
   key contains only `record_id`, `card_uid`, and committed event time
   (`tables/eval/observations.rs:66-75,89-124`).
4. `ObservationEnqueue` stages an `ObservationRecord` containing only subject,
   record ID, and event time (`observations.rs:105-123`;
   `verifier_runs.rs:881-891`). It captures neither the eligible binding set nor
   any activity-at-occurrence fact.
5. A required outage or other retained-write delay keeps that record in the
   outbox. Before its first successful flush, the previously inactive exact
   owner performs a qualifying machine exchange, advancing
   `last_authenticated_at`.
6. On retry, `ObservationRunSink::write` calls
   `enqueue_observation_batch` (`observations.rs:76-87`). For every matching
   binding, `accepts_records` calls `binding_activity`, whose predicate reads
   the principal's **current** status and latest `last_authenticated_at` against
   the flush statement's `statement_timestamp()`
   (`verifier_runs.rs:1678-1693,1709-1734`;
   `verification.rs:96-117,521-548`). The later exchange now makes the old
   committed occurrence eligible, and the insert creates its run.

This is exactly the backfill REQ-108 excludes. Existing tests that make an
owner inactive at `enqueue_observation_batch` execution prove only the current
predicate; they do not distinguish activity at the committed occurrence from
activity acquired before a delayed retry.

### Correction boundary

The behavioral obligation is clear, but a safe correction is not selected by
the approved design. Current state cannot reconstruct occurrence-time activity:

- Gate authentication is database-free and `AuthContext` carries the verified
  principal, tenant, request ID, and delegation chain, but no database activity
  stamp (`gate/auth.rs:1-11,34-50,101-126`).
- `last_authenticated_at` is overwritten/advanced by later qualifying
  exchanges, so the current principal row is not activity history.
- Capturing the active binding set synchronously would add PostgreSQL IO to the
  acknowledgement hook, whose contract explicitly requires synchronous,
  no-IO handoff (`gate/mod.rs:299-316`) and would conflict with REQ-077's
  non-blocking acknowledgement boundary.
- Conservatively rejecting every record older than the latest authentication
  prevents this backfill but also drops records that genuinely occurred while
  the owner was active and were followed by an ordinary renewal. Current state
  cannot distinguish those cases.

Closing the gap therefore needs a specification decision among materially
different semantics/state: redefine eligibility as activity at successful
flush; persist/query activity history; or introduce an occurrence-bound
eligibility snapshot through a new asynchronous/durable boundary. That is a
cross-service concurrency and persistent-data decision, not a bounded
TASK-015 remediation that can be prescribed from revision 60 alone.

**Resolution: RESOLVED.** `DOMAIN-CONCURRENCY-1` is supported by the approved
temporal invariant, but its correction requires a specification decision.

## Conflict 2 — cancellation rustdoc on `ObservationRunSink::write`

### Exact repository rule

The repository does not literally require a heading spelled `# Cancellation`.
It does require every new or materially modified Rust item to have substantive
rustdoc, and requires async or durable operations to document applicable
cancellation, partial progress, idempotency, and retry behavior
(`AGENTS.md:718-731`; `architecture/agent-rules.md`, “Rust documentation”;
`architecture/references/languages/rust-core.md:753-771`). The maintainer guide
uses a `# Cancellation` section as the clear in-repository form
(`maintainer-style.md:160-186`).

### Application to the sink method

`ObservationRunSink::write` is both async and durable. It awaits connection
acquisition, an insert inside a tenant transaction, and commit
(`observations.rs:67-87`). Generic-outbox shutdown can cancel and drop that
in-flight future at its deadline (`outbox.rs:189-227,266-285`). Cancellation
before commit ordinarily rolls the transaction back; cancellation while commit
is resolving can leave the durable outcome unknown to the caller. This is why
the binding/record uniqueness fence matters across a later repeat.

The method's own docs explain idempotence and returned-error retry, but say
nothing about cancellation, abandoned partial progress, or an ambiguous commit
(`observations.rs:67-75`). The inherited `OutboxSink::write` contract explains
retry after an unknown commit outcome (`outbox.rs:50-77`), but it also omits
cancellation, and `Outbox::shutdown` documents cancellation on a different
item. Those surrounding docs help a reviewer infer the behavior; they do not
satisfy the repository's per-item requirement for this materially changed
durable async implementation.

`MNT-015-001` is therefore correct in substance and should be narrowed only in
wording: the defect is missing cancellation/partial-progress documentation,
not violation of a rule that mandates one exact heading. The smallest
correction remains sink-local rustdoc (a `# Cancellation` section is the
clearest form) explaining deadline cancellation, the possible unknown commit
outcome, deadline loss reporting, and why a later repeat is idempotent. It
requires no production behavior or new test harness.

**Resolution: RESOLVED.** Retain `MNT-015-001` as a documentation finding with
the governing rule stated as required cancellation content rather than a
mandatory heading.

## Proposed finding disposition

| Discovery ID | Follow-up result | Reason |
|---|---|---|
| `DOMAIN-CONCURRENCY-1` | Supported; correction needs specification decision | Revision 60 preserves occurrence-time no-backfill, while the retained request has no occurrence activity fact and retry reads mutable current activity. |
| `MNT-015-001` | Supported with wording revised | The exact heading is not mandated, but cancellation/partial-progress documentation is, and neither the method nor inherited trait contract states it. |

No additional independent finding was discovered beyond these two resolved
claims.

## Overall follow-up status

**RESOLVED**
