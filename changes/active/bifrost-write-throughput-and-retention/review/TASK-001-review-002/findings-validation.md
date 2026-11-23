# TASK-001 focused independent review: FIX_REQUIRED

## Reviewed subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004`.
- Base: `66f44aee193077ef3565aa76140a942d8eb593f9`.
- Runtime candidate and prior candidate: `bf438a8d7985aadd29db3beb73ed0269c2b2f7ca`.
- Authority/documentation overlay: the tracked `git diff` with SHA-256
  `da4f96b4380cfed6c829a75ddb87e67518637f4c03ede8c02d80a4027a597cc3`.
- Contract: SPEC-bifrost-write-throughput-and-retention revision 2, approved by
  the maintainer's instruction to fix the spec and rerun review; original TASK-001,
  maintainer R1, and prior independent `TASK-001-review-001` remain context.

The candidate is the pinned commit plus the explicitly identified documentation
overlay, not a newly committed revision. No commit was authorized or created.
Inspection of the actual overlay found architecture/spec/task wording changes and
one Rust module-rustdoc correction; no executable code or test change. The runtime
diff from the prior candidate is empty. The cumulative runtime range remains
`66f44aee1..bf438a8d7`. The tracked overlay was fixed for this review; review reports
and remediation documents are outside that tracked diff.

This is a fresh independent focused remediation review under `wyrd-task-review`.
CodeGraph preceded tracing. Applicable root/nested SQL rules, the review skill,
prior reports and validated matrix, current specification, and the owning Bifrost
and execution architecture were considered. Existing source-backed acceptance
evidence is carried forward only where authority and dependencies remain valid.
No Cargo/Postgres test, formatter/lint lane or benchmark was rerun.

## Contract correction and independent comprehension

The previous contract hold is **closed**. The user has directed the correction;
the current review does not require another approval question. Revision 2 and the
owning architecture now agree that consumers read Bifrost alone, a temporarily
unreadable accepted record can appear absent, queued recovery selects the newest
readable compatible revision (or fresh state), and failed queries defer as
unavailable. Fixed revision identities refuse conflicting saves and cause recovery
retry; settlement still waits for acknowledged saves. Receipt expiry does not
expire lasting domain history or its conflict rules.

The practical cost is that recovery can repeat computation/effects from an older
readable state before a conflicting save forces retry. The updated architecture
states that tradeoff and preserves execution identity, pinned inputs/policy and
original deadline. This matches `InputResolver::latest_state` at
`verification/inputs.rs:280` and `RunState::accepted` at `state.rs:628`, whose query
and None/error branches were independently checked. The existing R1 lifecycle
assertions exercise newest-readable recovery followed by Stale refusal rather
than proving the superseded exact-accepted-prefix behavior. The spec acceptance
row, TASK-001 scenario wording and evidence module-rustdoc are reconciled. Historical
RED/GREEN records in the prior task are historical evidence, not competing authority.

No change to receipt arbitration, whole-batch custody, cleanup health requirements
or final CPU outcome follows from this correction. It does not close the two
implementation defects below. The bounded comprehension pass found no remaining
material decision needed to review the approved R1 semantics.

## Reconciled acceptance matrix

The detailed source/assertion mapping in the prior independent reports remains
applicable to the unchanged runtime. This is the canonical current matrix.

| Existing requirement / task scenario | Evidence and applicability | Assessment |
|---|---|---|
| REQ-001 / scenario 1 and unchanged completion boundaries | Prior primary/invariant reports trace unchanged buffered enqueue, WAL/fence/visibility before ACK, and terminal `persisted()` boundaries. The documentation overlay changes none of those paths. | PASS on inspected source and recorded execution. |
| REQ-002 / scenarios 1, 3 | SQL age/count/cutoff assertions and one-pod expiry/reacceptance proof carry forward. The independently retraced live second-pod mirror still outlives deletion. | FAIL — FIND-TASK-001-1. |
| REQ-003 / scenario 1 | R1 producer/consumer tests retain execution, approval, evidence and GitHub decisions after real receipt deletion. Newest-readable recovery/Stale assertions now match approved revision 2 and corrected authority. Query errors remain unavailable. | PASS on inspected source/assertions and recorded R1 execution; prior authority hold closed. |
| REQ-004 / scenario 2 | Prior independent canonical replay and never-accepted/losing-attempt suppression traces and connected restart assertions carry forward; no runtime or governing recovery-custody change. | PASS on recorded evidence. |
| REQ-005 / scenario 2 | Group-wide ownership, contiguous persisted frontier ordering, skip-before-replay, failed persistence, crossing/gap and staged/torn-tail assertions remain unchanged. | PASS on recorded evidence. |
| REQ-006 / scenarios 2, 3 | Tenant-scoped canonical-coordinate deletion predicate and connected custody → frontier → removal → restart proof remain unchanged. | PASS on recorded evidence. |
| REQ-007 / scenario 3 | Bounded background worker, periodic/coalesced discovery and off-ACK SQL retain prior structural and recorded proof. | PASS; initial readiness coupling is assessed under REQ-009. |
| REQ-008 / scenario 3 | Unverified startup admission, finite cutoff/scope, confirmed whole-sweep health and empty/pinned-scope assertions retain proof. They authorize delaying admission, not recovered readiness. | PASS for admission safety and sweep semantics. |
| REQ-009 / scenario 3 | Health freshness/failure/resumption and admitted-work/read assertions remain credible after startup. The independently retraced initial sweep still holds recovered boot pending. | FAIL — FIND-TASK-001-2. |

Prior execution records include 483 redux / 52 Scribe journeys and nine receipt
SQL tests at scenario-3 close; R1 records 139 server integration, 15 lifecycle,
35 verification-runtime, 41 execution-SQL and 12 unit tests. These are inspected
implementation records, not fresh execution in this review. Existing tests do not
exercise the two failing variants below. No new user-facing API, generated
projection or runtime dependency changed in the overlay.

## FIND-TASK-001-1 — CONFIRMED, still open

**Classification:** INCORRECT. **Authority:** revision 2 REQ-002 and the Bifrost
mirror-lifetime rule (`architecture/bifrost-design.md`, retry mirror section).

**Locations:** `scribe/shards.rs:3705,4318`, `scribe/committed_batches.rs:60,79,113`,
`scribe/receipt_cleaner.rs:185–211`, `scribe/ingress.rs:778–808` in
`crates/vala/vala-bifrost-redux/src/`.

**Supported trigger:** A accepts/publishes a batch. B handles an identical retry
under the same tenant, principal, table/version and batch identity. A's cleaner
later removes the aged safely handed-off receipt; B remains running. Retry on B.

**Expected:** B cannot acknowledge from a removed receipt; durable arbitration
must decide the now-unprotected identity.

**Actual independently traced path:** `AlreadyCommitted` enters `spent_batch_ids`
and suppresses duplicate insertion, but `record_committed` excludes only
`accepted_under`/`conflicted` and still caches this identity on B. `contains`
compares the stored identity without expiry. Cleaner discovery is scoped to its
own node, and deletion forgets only that process's mirror. B therefore retains
A's deleted acceptance until unrelated FIFO eviction or restart. Ingress can
return `first_commit=false` from it without a new shard/SQL attempt. The existing
two-pod losing-duplicate journey restarts B after deletion, removing the stale
cache instead of testing this live-pod case.

**Consequence and correction:** The retry window can outlive authoritative deletion
on a supported live pod. Correct the existing mirror population/lifetime owner;
reuse durable arbitration when the mirror lacks valid lifetime authority, preserving
matching/conflicting retries while authoritative and avoiding new ordinary-ingress
cleanup SQL or a ledger. Closure needs the same two-pod setup with B kept alive
through A's actual cleanup, then a retry asserting receipt/new-row behavior rather
than ACK success alone. No code or new proof in this overlay closes the defect.

## FIND-TASK-001-2 — CONFIRMED, still open

**Classification:** INCORRECT. **Authority:** revision 2 REQ-008–009 and Bifrost
receipt cleanup rules: initial sweep gates fresh admission; reads/recovery and
process readiness remain available.

**Locations:** `scribe/mod.rs:2715`, `scribe/receipt_cleaner.rs:152–211`, and
`crates/wyrd/wyrd-server/src/boot/mod.rs:762`.

**Supported trigger:** Restart with independently durable accepted rows and eligible
receipt debt while another tenant transaction holds a row lock on a safe receipt.
WAL/stage recovery can finish, but cleanup DELETE remains pending.

**Expected:** The recovered service can become available and read retained data;
fresh writes stay retryable-busy until initial cleanup completes.

**Actual independently traced path:** Successful `replay_wal_async` awaits
`sweep_and_report` before setting `recovery_ready` or returning. Boot awaits replay
before role activation/continuing startup. Sweeps take a whole-sweep mutex and
await DELETE/commit without a timeout/cancellation branch around pending work.
The call either enters a blocked sweep or waits behind the background sweep.
Catching a returned error does not resolve a pending operation. The existing
permission-error test returns promptly and cannot establish this availability
property under a pending DELETE.

**Consequence and correction:** Cleanup-only contention delays recovered service
startup rather than merely refusing fresh admission. Decouple recovered readiness
from initial sweep completion at the existing Scribe/cleaner lifecycle owner;
retain unverified admission and all WAL recovery authority checks. Closure needs
a real-Postgres startup/restart regression with cleanup locked, retained reads
and fresh-write busy observed, then lock release and automatic write resumption.
No executable change in the overlay closes this defect.

## Coverage, remediation and limits

One fresh focused reviewer is sufficient for this documentation-only follow-up.
The prior primary, lifecycle invariant specialist and SQL/concurrency/system
specialist reports plus independent validation remain present and cover unchanged
boundaries. No new specialist boundary or new finding was introduced. The two
retained IDs are unchanged; this is neither expectation expansion nor a failed
attempt to implement their corrections.

The R2 correction bounds remain appropriate under revision 2: mirror lifetime and
cleanup/readiness only, with focused connected dependency tests. The new R2 copy
was checked: its metadata references revision 2, its contract hold is removed,
and its scope preserves approved R1 behavior without reopening approval.

Prior wait inventory remains applicable: deleted consumer receipt probes, sanctioned
Bifrost reads/ACK waits, background SQL/manifest IO and frontier persistence. The
unsanctioned initial sweep wait is FIND-TASK-001-2. No wait is added by this overlay.
VALID dirty diagnostic benchmark subsets at `31af0fe55` remain progress evidence,
not clean full qualification of this candidate. The maintainer's disclosed open
hot-read regression remains open; no performance PASS is inferred. Full CPU and
integrated shipped-surface qualification remain as recorded in the task packet.

Behavioral acceptance and TDD remain separate. The recorded post-implementation
recovery/cleaner tests retain their TDD violations; subsequent mutation checks
cannot establish historical RED. R1's recorded RED/GREEN assertions remain useful.
No historical test churn is required to address the two concrete defects.

**Verdict: FIX_REQUIRED.** The specification/architecture hold is resolved; the
two settled implementation defects remain open. No further clarification is
necessary to produce the bounded remediation task.

Final immutable check: HEAD remains `bf438a8d7985aadd29db3beb73ed0269c2b2f7ca`,
and the tracked overlay SHA-256 remains
`da4f96b4380cfed6c829a75ddb87e67518637f4c03ede8c02d80a4027a597cc3`.
`git diff --check` passed. Only the assigned review report was written by this
reviewer; no implementation changes were made.

Skipped: fresh tests, lints and benchmarks. Risk: the recorded execution does not
cover the two source-traced failing cases; neither defect has been corrected.
