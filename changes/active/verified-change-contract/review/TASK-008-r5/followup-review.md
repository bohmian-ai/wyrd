# Focused Follow-up Review

## Immutable subject and scope

- Base: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13`
- Candidate: `0973a03e5a389ea0fb3635c6d9175e25db0a6da0`
- Approved specification: `changes/active/verified-change-contract/spec.md`,
  revision 57
- Original task:
  `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Conflicts investigated only:
  1. `MNT-R5-001` versus the standards review's characterization of
     `stop_replicas` as a permissible narrow adapter; and
  2. `DUR-R5-001`, the unreviewed process-local audit-outbox handoff in the
     REQ-171 drain.

The candidate remained at the named commit throughout inspection. CodeGraph is
not indexed in this checkout, so tracing used Git and repository text. The
caller-directed deferral of `FIND-TASK-008-CLOSEOUT-13` remains non-blocking,
and this pass does not reopen the intentional report-before-teardown ordering.
This is a discovery pass and makes no final-verdict vote.

## Uncertainty 1: replica-stop owner shape

### Source path inspected

- `AGENTS.md` section 5 and `architecture/agent-rules.md`, especially the hard
  rule that stateful, multi-step workflows and internal orchestration belong on
  the concrete owner, and that passing dependencies as arguments does not make
  a workflow stateless.
- `architecture/references/languages/maintainer-style.md`.
- Full bodies and callers of
  `capacity/main.rs::{Benchmark::run,Benchmark::clean_up,stop_replicas}` and
  `capacity/step.rs::{Deployment,Deployment::run}`.
- Full process owner in
  `wyrd-testing/src/release_server.rs::{LocalServer,LocalServer::stop}`.
- Both callers of `stop_replicas`: `Benchmark::clean_up` and
  `a_slow_replica_stop_leaves_the_runtime_free`.

### Resolution and evidence

The conflict resolves in favor of `MNT-R5-001`.

`stop_replicas` at `capacity/main.rs:649-677` is not a stateless deterministic
helper or a single boundary conversion. It consumes the live replica
collection, derives a destination from each replica's identity and the output
root, enforces newest-first shutdown, places every synchronous process
termination on Tokio's blocking pool, converts join and process failures, and
then restores ordinal report order. Cancellation also has workflow-level
partial-progress semantics: the in-progress blocking stop continues while
later replicas fall back to `Drop`. Its two parameters are precisely live
lifecycle state and an IO dependency.

The production call at `capacity/main.rs:589-604` occurs inside the benchmark's
cleanup transition after client ownership has been removed and shut down.
`Benchmark` owns the complete setup-to-report lifecycle, its output root, and
the `Option<Deployment>`; `Deployment` owns the replica collection and already
owns deployment-wide runtime operations. `LocalServer::stop` at
`release_server.rs:365-405` correctly owns one process's synchronous
termination, reap, and log copy, but it does not own collection ordering or
ordinal result alignment. The test call at `capacity/main.rs:948-992` exists
only to exercise this newly extracted production workflow and does not make it
ownerless.

The standards report is correct that `spawn_blocking` is the required async
boundary, but calling the whole function a “narrow blocking-boundary adapter”
does not satisfy the more specific hard rule. The function performs collection
orchestration around that adapter. This is material because the candidate
newly introduced the shape while remediating the prior Tokio-worker defect;
existing functional drift is not being reviewed opportunistically.

### Smallest correction boundary

Keep `LocalServer::stop` synchronous and preserve `spawn_blocking`, newest-first
shutdown, per-ordinal log names, join-error conversion, cancellation behavior,
and ordinal result order. Move the collection operation onto the existing
owner of the collection, `Deployment`, and have `Benchmark::clean_up` request
that operation after its client-shutdown phase. The focused slow-stop proof
must exercise that owner path. No new helper type, trait, process abstraction,
or changed cleanup/report behavior is warranted.

This is the smallest cohesive boundary because `LocalServer` continues to own
one replica, `Deployment` owns the replica set, and `Benchmark` continues to
own ordering between client cleanup, deployment cleanup, and reporting.

### Proposed finding disposition

- Retain `MNT-R5-001` as a material repository-rule finding. No additional
  finding is proposed for this uncertainty.

## Uncertainty 2: process-local audit ownership during REQ-171 drain

### Source path inspected

- REQ-171 in revision 57, especially the requirement that every audit-outbox
  backlog after load stops drain within 60 seconds.
- The non-blocking audit authority in `AGENTS.md`,
  `architecture/agent-rules.md`, `architecture/bifrost-design.md:587-597`, and
  `architecture/wyrd-security-posture.md:357-387`.
- Full producer and writer lifecycle in
  `wyrd-server/src/oracle/query_audit.rs::{OracleQueryAudit,OracleAuditWriter}`.
- Oracle's call through
  `vala-bifrost-redux/src/oracle/mod.rs::{audit_read_decision,bind_execution_sources}`
  and the direct-verification sibling at
  `wyrd-server/src/components/verification/service.rs:500-536`.
- Workload completion and timestamp capture in
  `capacity/load.rs::{Lane::drive,Request::send}` and
  `capacity/step.rs::{Deployment::run,Deployment::drain}`.
- Durable evidence in
  `capacity/evidence.rs::{Backlog,Queue::backlog}` and metric scraping in
  `release_server.rs::{LocalServer::metrics,Metrics}`.
- Existing test-only inspection and producer proof in
  `wyrd-server/src/state.rs:634-641`,
  `wyrd-testing/src/server.rs:1350-1373,1606-1627`, and
  `wyrd-testing/tests/bifrost/server/audit_publication.rs:508-560`.
- Shutdown's independent ownership check in
  `wyrd-server/src/app/server.rs:876-883`.

### Reachability and obligation

The path is reachable and falls directly under REQ-171.

`OracleQueryAudit::stage` increments `pending` before `try_send`, and the writer
decrements it only after the drained batch has either committed or been counted
lost (`query_audit.rs:92-121,142-199`). Oracle calls this non-blocking stage
before binding/executing the query stream, and direct verification stages its
decision before returning its authorization result. `Lane::drive` waits for
all request tasks to return before `Deployment::run` captures the PostgreSQL
`stopped` timestamp (`load.rs:255-331`; `step.rs:320-360`). Therefore a request
can have returned, be included in the completed step traffic, and still leave
a decision queued or blocked in a tenant commit.

This is not hypothetical: the existing Postgres journey deliberately locks
`vala.audit_chain_head`, serves reads, and observes `audit_pending` at least as
large as the completed read count before releasing the lock
(`audit_publication.rs:508-557`). Shutdown separately waits on
`audit_outbox.shutdown`, confirming that process-local pending work is an owned
lifecycle state rather than synonymous with committed staging.

The sibling consumers matter. The same process owner receives Oracle read
decisions and direct-verification allowed/denied decisions; both operations are
part of the REQ-171 mix. Counting only Oracle query traffic would still miss
direct executions held at the same handoff. Gate's transactional audit owner
is a different path and should not be folded into this process-local count.

### Why current evidence and metrics do not close it

`Queue::backlog` counts only committed `vala.audit_staging` rows above the
publication watermark and only when `s.created_at <= stopped`
(`evidence.rs:223-264`). `Deployment::drain` first scrapes replicas and then
combines only Scribe metrics with that SQL result (`step.rs:395-419`). It reads
no process-local audit value.

No production `/metrics` family exposes `OracleQueryAudit::pending`. The only
exported audit-outbox metric is the cumulative
`oracle_audit_commit_failures_total`, which changes only for a full/closed queue
or a failed batch; it cannot distinguish zero pending decisions from a healthy
commit that is still waiting. The existing `audit_pending` inspection reads
the correct atomic owner but is feature-gated to `test-support` and available
only through the in-process `WyrdTestServer`; `bench:capacity` builds release
`wyrd-server --features cloud` processes and can observe them only through
their public metric scrapes. Thus neither existing metric solves the benchmark
gap.

The timestamp makes the omission persist across polls. A decision committed
after `stopped` receives a later database `created_at`, so the current
`created_at <= stopped` predicate never counts it. A future pending gauge alone
would not be sufficient: once the writer decrements pending after commit, a
later scrape can read zero while the SQL predicate permanently excludes the
now-durable row even if publication has not drained it.

### Minimum correction and proof

Reuse the existing `OracleQueryAudit::pending` owner; do not add another queue,
ledger, endpoint, or request wait. Export that owner as a process-local gauge
from each serving replica and include the sum in the capacity audit backlog.
Bridge the handoff by also counting every currently unpublished staged audit
row, including one whose commit completed after the load-stop timestamp. The
load driver has already joined all request tails before `stopped`, so a later
commit from those requests is still step-owned work; the upper `created_at`
filter cannot define that ownership. Preserve the publication watermark,
non-blocking request semantics, canonical staging path, and exact 60-second
edge. The existing commit-failure counter remains a loss signal, not a pending
gauge.

The focused closure proof must hold a tenant chain-head commit beyond the
public Oracle or direct-verification request's return and show that
`Deployment::drain` remains nonempty from the replica's process-local count.
After release, it must show the handoff remains nonempty while the resulting
staging row is above the publication watermark, including when its
`created_at` is later than the captured stop time, and reaches zero only after
publication. A small scrape/backlog combination test may support this, but it
cannot replace the real held-commit path already demonstrated by the sibling
audit-publication journey.

### New proposed finding

#### `FOLLOWUP-R5-001` — INCORRECT — REQ-171 audit drain omits decisions between request return and durable staging

- **Violated obligation:** REQ-171 requires every audit-outbox backlog after
  load stops to drain within 60 seconds.
- **Exact location:**
  `crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:223-264` and
  `crates/wyrd/wyrd-testing/src/bin/capacity/step.rs:395-419`; the omitted
  producer is `crates/wyrd/wyrd-server/src/oracle/query_audit.rs:48-121,142-199`.
- **Observable consequence:** a step can record an empty audit backlog and pass
  saturation while one or more replicas still own uncommitted decisions, or
  while a post-stop commit remains unpublished but is excluded forever by the
  timestamp predicate.
- **Correction boundary:** the existing process-local pending owner, the
  release replica metric scrape, and the existing durable staging-above-
  watermark query, with the handoff proof described above.

This corroborates and sharpens `DUR-R5-001`; validation should deduplicate them
as one finding rather than assign two final IDs.

## Overall follow-up result

**RESOLVED**

Both uncertainties are resolved from approved authority and reachable source
paths. `MNT-R5-001` remains a structural standards finding with a bounded owner
correction. `DUR-R5-001` is a real REQ-171 evidence gap; current production
metrics do not expose its process-local state, and its minimum correction must
cover both sides of the pending-to-staging handoff.
