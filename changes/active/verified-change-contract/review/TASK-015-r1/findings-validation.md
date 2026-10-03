# TASK-015 structured finding validation

## Immutable subject

- Candidate: `9b560d00569656ee7fdde19a9c76fa55c8d55fbb`.
- TASK-015 range: `3f8767a5f6a9b9c8605a53c424c7a7056a3b6786..9b560d00569656ee7fdde19a9c76fa55c8d55fbb`.
- Scoped merge resolution: `c5527627a50dd66a9f53d760d80f59bcb59609f9`, parents `ca2950856a37786a5cad25c73c1786c5fa7a1822` and `52e1144b5c186ccacd85d9c779a4e60c3cce5ac2`.
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 60, including REQ-077, REQ-108, and AC-014.
- Original task: `changes/active/verified-change-contract/tasks/TASK-015-eval-runs-in-the-batch-fence.md`.

The candidate identity was checked before source inspection and immediately
before this report was written. CodeGraph was skipped because this checkout has
no `.codegraph/` directory. No Cargo or `mise` command was run, and no
production source or test was modified.

## Validation method

I read the complete TASK-015 diff, the scoped merge's combined conflict hunks
and both-parent resolution, every discovery report, and the focused follow-up.
For each proposed finding I traced the produced state through its durable
consumer, inspected sibling consumers and callers, established reachability,
and applied the required delete/reuse/native-platform/installed-dependency/
minimum-correction ladder. Agreement among discovery reviewers was not treated
as proof.

## Proposed-finding dispositions

### `DOMAIN-CONCURRENCY-1` — REVISED and retained

The finding is correct, but its remediation cannot be selected from the
approved specification. It is retained below as `FIND-TASK-015-1` with
`SPEC_REVISION_REQUIRED`.

Producer-to-consumer proof:

1. Continuous Eval's occurrence is the successfully committed observation
   (`spec.md:110-113`; `architecture/wyrd-design.md:1093,1294-1304`).
2. Gate calls the acknowledgement hook after Scribe's first durable commit
   (`crates/vala/vala-bifrost-redux/src/gate/mod.rs:997-1009`).
3. The acknowledged key and staged `ObservationRecord` preserve only subject,
   record ID, and committed event time
   (`crates/vala/vala-bifrost-redux/src/tables/eval/observations.rs:66-124`;
   `crates/wyrd/wyrd-server/src/verification/observations.rs:105-123`;
   `crates/wyrd/wyrd-sql/src/queries/verifier_runs.rs:881-891`). They preserve
   neither occurrence-time owner activity nor an admitted binding set.
4. After an outbox delay, `enqueue_observation_batch` lists every current
   matching `observations_ready` binding and `accepts_records` evaluates
   `binding_activity` at that later flush statement
   (`verifier_runs.rs:115-128,1640-1735`). The activity query reads current
   principal/Card status and the latest `last_authenticated_at` against the
   current `statement_timestamp()`
   (`crates/wyrd/wyrd-sql/src/queries/verification.rs:96-117,521-548`).
5. A qualifying later exchange advances `last_authenticated_at`
   (`verification.rs:59-75,382-434`). A record committed while an exact binding
   owner was inactive can therefore become eligible when the retained outbox
   request is retried after that later exchange. This is reachable when an
   active publisher observes a subject also used by another exact owner's
   binding: matching is by `subject_card_uid`, not publisher identity.

The existing inactive-owner SQL test evaluates inactivity only at the instant
`enqueue_observation_batch` executes
(`crates/wyrd/wyrd-sql/tests/pg_verifier_runs.rs:1858-1967`), so it does not
close this temporal path.

Ladder result: the obligation cannot be deleted because REQ-108 explicitly
forbids later-authentication backfill. Existing state cannot reconstruct the
fact: `last_authenticated_at` is overwritten by renewal, Gate authentication
is database-free, and the staged item has no occurrence-bound eligibility
fact. The standard library, platform, and installed dependencies cannot supply
missing domain history. A consumer-side comparison between event time and the
latest authentication is not safe: it would also reject an occurrence that was
valid under an earlier activity stamp before an ordinary renewal. Likewise,
synchronous binding lookup in the acknowledgement hook would violate its
non-I/O, non-blocking boundary. A safe correction therefore requires an
approved concurrency and persistent-data decision about how occurrence-time
eligibility is represented or whether eligibility is intentionally redefined;
this review does not invent a downstream guard.

### `MNT-015-001` — REVISED and retained

The finding is correct in substance. The repository does not mandate a literal
heading named `# Cancellation`, so the finding is narrowed to missing required
cancellation and partial-progress content. It is retained below as
`FIND-TASK-015-2`.

`ObservationRunSink::write` is a new async durable operation that awaits
connection acquisition, insert, and commit
(`crates/wyrd/wyrd-server/src/verification/observations.rs:67-87`). Generic
outbox shutdown may cancel its in-flight future at the deadline
(`crates/shared/wyrd-runtime/src/outbox.rs:189-227,266-285`). Its current docs
cover returned errors and retry idempotence, but not cancellation before or
during commit, the potentially unknown commit outcome, or the resulting
partial-progress interpretation. The trait and shutdown documentation let a
reviewer infer pieces of that behavior, but the per-item rule requires the
materially changed async/durable method itself to state applicable
cancellation and partial progress (`AGENTS.md:718-732`;
`architecture/agent-rules.md`, Rust documentation;
`architecture/references/languages/rust-core.md:753-771`).

Ladder result: this cannot be deleted or delegated to a new abstraction. The
existing trait text does not satisfy the repository's per-item documentation
rule, and no native or dependency mechanism supplies the missing contract.
The minimum correction is sink-local rustdoc explaining deadline cancellation,
the commit/rollback or unknown-outcome boundary, deadline loss reporting, and
why repeating the stable tenant/binding/record identity remains safe. No
behavioral change or new test harness is needed.

## Explicitly validated empty areas

### Eval journey edits

No finding is retained for either edit.

- Deleting the permanently refused pre-phase from
  `continuous_eval_runs_the_terminal_matrix` follows revision 60. The deleted
  phase required an acknowledged request to disappear permanently while later
  same-tenant work progressed, contrary to REQ-077's retained retry. AC-014's
  required temporary-outage path remains covered end to end by
  `integrated_enqueue_outage_preserves_ack_and_recovers` and at the SQL/server
  seam by `observation_outbox_retains_through_an_outage_and_flushes_at_shutdown`.
- Replacing the blocked-Postgres-backend count in
  `sealed_replay_on_a_later_day_activates_once` with exact `pending() == 2`
  follows the new owner. Gate stages synchronously before acknowledgement
  returns, the table lock keeps the original request pending, and the generic
  outbox permits one in-flight write for that tenant. The count therefore
  represents exactly original plus sentinel; either replay reaching the hook
  would increase it. Later assertions still prove the exact durable run count
  and frozen event-time/day behavior.

### Permanent poison/backlog risk

No finding is retained. REQ-077 requires retention and retry when Postgres is
slow or unavailable and does not define poison-item classification,
dead-lettering, batch splitting, or progress around a permanently invalid
same-tenant item. Adding any such policy would choose new loss, ordering,
concurrency, and persistent-state semantics. The generic outbox was not changed
by TASK-015 and remains outside this review's directed scope.

### Scoped merge resolution

No finding is retained. The combined resolution and both-parent comparison
show that Gate stages reached allow/deny decisions through the shared
`AuditOutbox` without waiting, the audit-unavailable public error paths remain
removed (with the protobuf identity reserved rather than reused), and
TASK-013/014's SYSTEM-token issuer/tests remain deleted. Boot and state retain
both the audit outbox and Eval run-request outbox. TASK-015's later composition
edit does not undo those choices.

### Other regression and scope checks

No additional finding is retained. The TASK-015 diff replaces the hand-written
queue/writer with `Outbox<ObservationRunSink>`, preserves the server-owned
tenant transaction and existing batch insert, keeps shutdown wiring and metric
labels, and does not modify generic-outbox internals. The Rust SDK conversion
is an infallible widening cleanup with no behavioral change. The supplied
verification record covers the focused SQL/runtime tests, SQL and server
integration lanes, the 31-test server journey lane, format, and lints; those
results were not independently rerun in this review.

## Final deduplicated ledger

### `FIND-TASK-015-1`

- **Discovery source:** `DOMAIN-CONCURRENCY-1`.
- **Validation status:** REVISED.
- **Classification:** INCORRECT.
- **Violated obligation:** REQ-108 requires an inactive occurrence to create no
  activation or run and forbids backfill after later authentication. The
  successfully committed Eval observation is the occurrence.
- **Exact location:**
  `crates/wyrd/wyrd-server/src/verification/observations.rs:105-123` and
  `crates/wyrd/wyrd-sql/src/queries/verifier_runs.rs:1640-1735`, consuming
  mutable activity from
  `crates/wyrd/wyrd-sql/src/queries/verification.rs:96-117,521-548`.
- **Evidence:** the staged item has no occurrence-time activity or admitted
  binding fact; delayed flush evaluates current activity, so later qualifying
  authentication can make an old inactive occurrence eligible. Subject-based
  binding lookup makes the path reachable even when the publisher is active
  and another exact binding owner is inactive.
- **Observable consequence:** a historical Eval observation can create a
  binding-driven run only because its owner authenticated after that
  observation committed, violating the promised no-backfill behavior.
- **Correction boundary:** do not add a speculative event-time guard or
  synchronous Postgres lookup to Gate. The specification must choose the
  concurrency/persistent-data representation for occurrence-time eligibility,
  or explicitly redefine the eligibility time. Only after approval can a
  bounded implementation task identify the owning state and consumer changes.
- **Focused closure proof:** after that decision is approved and implemented,
  a focused Postgres-backed outbox test must retain a request for an occurrence
  whose exact owner is inactive, perform a qualifying later authentication
  before retry succeeds, prove the historical record never creates a run, and
  prove the next eligible committed observation does. It must also cover an
  occurrence valid before an ordinary renewal so the correction does not
  convert renewal into false rejection, and preserve non-blocking
  acknowledgement.
- **Specification revision required:** YES — the current architecture has no
  occurrence activity history or eligibility snapshot, and choosing one is a
  new concurrency and persistent-data decision.

### `FIND-TASK-015-2`

- **Discovery source:** `MNT-015-001`.
- **Validation status:** REVISED.
- **Classification:** VIOLATION.
- **Violated obligation:** `AGENTS.md` section 16 and the repository Rust
  documentation rule require applicable cancellation and partial-progress
  behavior on every materially changed async/durable method.
- **Exact location:**
  `crates/wyrd/wyrd-server/src/verification/observations.rs:67-87`.
- **Evidence:** the docs state returned failures and retry idempotence but omit
  cancellation at the outbox deadline and the commit outcome that may already
  be durable or unknown when the future is dropped.
- **Observable consequence:** a maintainer changing shutdown or retry handling
  can incorrectly assume cancellation proves rollback, risking an unsafe
  retry or unnecessary downstream guard at this durable boundary.
- **Decision-complete correction:** extend the method's own rustdoc, preferably
  with a `# Cancellation` section, to state that the shutdown deadline may
  cancel the in-flight transaction; cancellation before commit rolls back,
  cancellation while commit resolves may leave the outcome unknown; the
  outbox reports deadline-abandoned items; and a later repeat is safe because
  tenant/binding/record uniqueness absorbs it. Preserve the implementation and
  generic outbox unchanged.
- **Focused closure proof:** static source review confirms the complete
  cancellation/partial-progress contract on the method; run the existing Rust
  documentation/lint lane. No new runtime test is required because behavior is
  unchanged and existing outbox coverage already exercises deadline loss and
  retry.
- **Specification revision required:** NO.

## Validation recommendation

The validated ledger is not empty. `FIND-TASK-015-1` requires a new approved
concurrency and persistent-data decision before a safe correction can be
prescribed, so the appropriate orchestrator outcome is
`SPEC_REVISION_REQUIRED`, not a speculative remediation task. After that
decision, `FIND-TASK-015-2` remains a bounded documentation correction to
include in the resulting implementation work.
