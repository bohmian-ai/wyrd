# TASK-008 round-eight system-resilience review

## Immutable subject and scope

- Candidate: `c4bc77a5c877c508191dc606b3cd3bb78047dc29`.
- Remediation range: `ea0ed46fa..c4bc77a5c877c508191dc606b3cd3bb78047dc29`.
- Authority: `changes/active/verified-change-contract/spec.md`, approved revision 57.
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`.
- Prior finding: `FIND-TASK-008-CLOSEOUT-17` in `TASK-008-r7`.

The candidate stayed at the named commit throughout this review. The checkout
has no `.codegraph/` directory, so navigation used Git, `rg`, and direct source
inspection. Per caller direction, this review covers only whether the range
closes `FIND-TASK-008-CLOSEOUT-17` and whether the range introduces a deployed
or recovery regression capable of making the capacity benchmark falsely pass
or fail. Earlier accepted code is not reopened. The full default
`bench:capacity` run remains deferred as `FIND-TASK-008-CLOSEOUT-13`.

## Deployed path and affected capabilities

The range changes no production server, audit writer, publisher, database
schema, public API, process target, or deployment configuration. Its only
runtime change is the release capacity harness's observation path in
`crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:307-353`; the other
changed file records remediation evidence.

The measured deployment consists of one or two release `wyrd-server` processes
sharing Postgres and storage. Each `LocalServer` exposes its own `/metrics`
endpoint (`release_server.rs:325-334`), and `Deployment::drain` scrapes every
replica before handing the combined reading to `Drain::judge`
(`capacity/step.rs:383-422`). Oracle authorization decisions enter one bounded,
process-local `OracleQueryAudit` queue. The process raises
`audit_outbox_pending` before enqueueing (`query_audit.rs:101-116`), commits the
canonical `vala.audit_staging` transaction, and decrements pending only after
commit or counted loss (`query_audit.rs:155-215`). `AuditPublisher` then moves
staged rows through local Scribe and advances the tenant watermark; transient
failures leave the durable row and frozen bound for later retry
(`audit/publication.rs:179-209,259-285`).

The affected capability is therefore only capacity evidence and its saturation
verdict. Serving, query execution, authorization, audit durability,
publication, and unrelated server capabilities execute unchanged.

## Closure trace for `FIND-TASK-008-CLOSEOUT-17`

`Queue::poll` now observes `S1 -> Q1`, returns immediately if that combination
is nonempty, and otherwise observes `S2 -> Q2`, returning the latest combined
reading (`capacity/evidence.rs:339-352`). This closes the exact prior false-zero
schedule:

1. `S1` sees no process-local pending decision.
2. A step-owned Oracle operation stages a decision; its request can complete
   while the writer is blocked.
3. `Q1` sees all expected runs terminal and no committed audit row.
4. The writer commits after `Q1`, then decrements the replica gauge before
   `S2`.
5. `S2` sees zero, but `Q2`, which starts after `S2`, sees the committed row
   above `published_seq`, so the poll remains nonempty.

The ordering also remains conservative at the two adjacent ownership states.
A decision still pending at `S2` is counted from that scrape even if the
process releases or crashes afterward. A row published between `S2` and `Q2`
may read zero at `Q2`, but that is a genuine recovery completion: the decision
is neither process-owned nor owed above the durable watermark. Because `Q1`
was empty only after every expected queued run was created and terminal, the
step has no later queued producer that can first appear after `S2`; direct
drivers have already joined before drain begins (`capacity/step.rs:320-361`).
The existing outer loop retains repetition and the exact 60-second boundary.

Result: **CLOSED**. The terminating consumer can no longer turn the successful
pending-to-durable handoff into a false PASS.

## Failure and recovery assessment

| Failure or interruption | What stops or remains | Recovery and verdict effect | Result |
|---|---|---|---|
| Oracle writer or Postgres append is delayed across `Q1` | The request remains non-blocking; the replica gauge continues to own the decision until commit or counted loss | Pending at `S2` remains nonzero; if commit/decrement crosses `Q1`, `Q2` sees staging. The drain continues rather than falsely passing. | PASS |
| Audit publication is delayed, its dependency is unavailable, or a publisher restarts | Oracle requests and unrelated serving remain available; committed staging and its watermark/bound survive | `Q2` and later polls keep the audit cell nonzero. The existing publisher retries later with the same frozen range and Scribe dedup identity. A 60-second expiry is an accurate saturation failure, not an observation regression. | PASS |
| A serving replica is unavailable during a scrape | That replica's metrics request fails | The error propagates out of `Queue::poll` and the benchmark command; it cannot be converted to an empty backlog or false PASS. Recovery requires the benchmark deployment/caller to restore or rerun the replica, as before this range. | PASS |
| Postgres is unavailable during `Q1` or the new `Q2` | Durable observation stops; process-local serving may otherwise continue | The query error propagates and no PASS/FAIL capacity cell is fabricated. The range adds one conditional read only after an empty first observation and does not retry or amplify the database fault in an inner loop. | PASS |
| Poll cancellation or command deadline | The read-only observation is dropped; server audit and publisher tasks continue under their existing owners | No durable state is mutated by the poll. The benchmark's outer deadline and process cleanup remain unchanged. | PASS |

The extra `Q2` can conservatively move a boundary observation past 60 seconds,
but `Drain::judge` already defines a first empty reading completed after the
limit as expired. Accepting the earlier, unpaired `S2` would recreate the known
false PASS; the new read is the minimum evidence needed to decide the required
audit-backlog SLO and does not change the deadline.

## Proof assessment

The environment-owned proof at `capacity/evidence.rs:557-790` exercises both
handoffs through the real public Oracle client, the production audit owner,
canonical Postgres staging, the pending gauge, and `AuditPublisher`. Its added
phase holds the tenant chain head, creates the decision after `S1`, keeps the
commit absent through `Q1`, releases it inside the second scrape, waits for
pending to reach zero before capturing `S2`, then proves the returned audit
cell is one from `Q2`, remains one above the watermark, and clears only after
publication (`capacity/evidence.rs:725-786`). The assertion query at the start
of the second callback does not replace `Q1`: the actual `Q1` has already run
while the same fence was held, so it also necessarily observed no committed
row.

The implementation record reports the focused Postgres proof passing, an
intentional no-`Q2` red failure, all 20 capacity-target tests passing with
ignored tests enabled, and the server journey passing. In this review, the two
focused pure checks passed. A fresh execution of the Postgres proof was blocked
before setup because this sandbox cannot access the Docker API; no test body
ran. Source ordering and the available recorded environment run provide direct
closure evidence, while the unmodified full default benchmark remains the
explicit integration deferral.

## Material proposed findings

None. The remediation range closes the prior system-resilience gap and
introduces no reachable regression within the caller-directed boundary.

## Overall result

**PASS**
