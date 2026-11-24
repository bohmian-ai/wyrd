# TASK-003: pipeline invariants specialist

Subject: base `74034459d`, candidate
`c0900bff523e83d3666658440daa81355067cb34`. Approved working packet:
SPEC-bifrost-write-throughput-and-retention revision 4, TASK-003. Candidate HEAD
was unchanged at the end of this pass. This report changes no reviewed source.

Scope: REQ-016–018 and their REQ-019 proof: durable acceptance, partial tenant
failure, unresolved SQL, owned IO, recovery, memory, generation mutation,
shutdown, receipt custody. Authority read: supplied/root AGENTS, the task-review
skill, `changes/spec-driven-development.md`, maintainer style, testing guidance,
and Bifrost ingest, live recovery/shutdown, receipt handoff/cleanup sections.
CodeGraph preceded source tracing. Benchmark qualification is deliberately
deferred by the maintainer; its absence is not a finding in this code review.

## Coverage

- **Ordered acceptance and ownership:** `shards.rs:3910–3989, 4218–4375` retain
  stage futures in their slots across select cancellation; WAL writes cannot
  overlap another group's WAL stage. Only a durable oldest group enters fencing.
  `complete_fenced_group:4389` inserts and publishes before the normal ACK. A
  younger IO failure is independently refused without undoing the older group.
  The repeated four-group production-loop test and younger IO-failure test
  exercise real WAL and a held real PostgreSQL fence, rather than a scheduler
  model. The test IO hold is after the worker executes the sync and before its
  result reaches the retained future (`execution_lanes.rs:2421–2455`).
- **Tenant SQL and partial acceptance:** `fence_group:5113` groups fences by
  authenticated tenant and `fence_tenant:5185` retains batched TenantConn queries,
  conflict rollback/reoffer and retained-version verification. An operational
  error after a preceding tenant accepted poisons ownership (`5160–5171`).
  `a_later_tenant_fence_failure_after_an_earlier_commit_fails_closed:12405`
  holds the first fence while the younger group syncs, fails the second tenant,
  checks neither younger nor failed fences land, keeps transient charges,
  drains IO, then replays exactly the accepted row into the next epoch. These
  assertions are credible evidence for the two-slot poison consumer and actual
  canonical recovery boundary.
- **COMMIT resolution:** `commit_batch_control_fences:5424` opens a fresh tenant
  transaction after a failed COMMIT, retries absent fences and poisons on failed
  connection or lookup. `an_unresolved_older_commit_holds_the_younger_group_unfenced:13016`
  installs a real deferred PostgreSQL trigger, proves repeated absent resolutions
  while younger WAL progresses, and checks no ACK/visibility/later fence until
  the trigger is removed. It establishes the retry path, not the failed-lookup
  path. See INV-1 below.
- **Generation/resource barriers:** `run:1498`, `route_command:1621`, and
  `dispatch_next:4129` defer generation-mutating controls and pressure until no
  dispatched group remains, block replenishment behind barriers, and account
  older pending JSON/Arrow once in `rotation_trigger:4667`. New tests cover
  threshold rotation, FlushAll mailbox position, selective age/pressure seals,
  shutdown with two dispatched groups, client cancellation, conflicts and
  retained insertion. Per-request transient categories reach zero after
  settlement/drain; the fail-closed case keeps nonzero retained charges.
  Existing governor ownership APIs perform the actual checked transfers; no
  second ledger was added.
- **Post-fence insertion:** real insertion errors poison readiness and retain
  remaining ownership for WAL restart (`4455–4463`). The injected test fault
  deliberately retains a whole insert owner without poisoning and retries it
  before the younger fence (`a_post_fence_insertion_failure_is_retried_before_the_younger_group_fences:12886`).
  That test alone is not proof of every real insertion failure, but the existing
  serial insertion tests plus common poison consumer above cover the relevant
  established boundary. No concrete new insertion defect was established.
- **Receipt custody:** structural tracing supports the unchanged frontier
  protection, rather than an additional claim of a concurrent-cleaner test.
  Controls that selectively seal/retire or close generations do not run beside
  dispatched groups. `finish_stage:4239` registers segments before subsequent
  controls can consume them; `write_and_sync_commits:5082` associates every group
  segment with every affected key. Generation/cohort retention and
  `submit_at:2542` close-if-unowned preserve references before persistence can
  retire. `wal.rs:3037–3185` excludes current segments and persists only a
  contiguous released closed-segment prefix before deletion; the cleaner reads
  that persisted frontier. WAL segment rollover is distinct from a lifecycle
  generation close, so the task's sentence that files close only when idle is
  an abbreviated explanation, not the entire safety argument. No pipeline
  cleaner defect was established. The reported existing receipt journeys add
  execution evidence for the unchanged downstream dependency.

## Proposed finding INV-1 — missing failed live COMMIT-lookup proof

Classification: **EVIDENCE_GAP**. Requirement: REQ-017, REQ-019; task scenario 3.
Locations: `shards.rs:5454–5471` (fresh reconciliation failures),
`shards.rs:13016–13112` (the new ambiguity test), and task scenario 3's claim
that existing ambiguity tests cover serial poisoning.

Starting state and trigger: an older synced group receives an ambiguous fence
COMMIT response; its fresh tenant connection or exact durable lookup is
unavailable, with a younger group submitted. Expected: readiness fails closed,
the complete uncertain owner remains charged and unanswered, submitted younger
IO drains, and no younger fencing occurs. This is an explicitly required safety
boundary for a normal database outage after COMMIT, not a speculative input.

The new deferred-trigger test keeps reconciliation available and returning
Absent, so it never reaches either poisoning branch. The multi-tenant test
poisons from a known later-tenant failure after an earlier acceptance, exercising
the downstream consumer but not this uncertain-COMMIT producer. Source tracing
found no executable live `commit_batch_control_fences` failure-of-reconciliation
test in the shard tests, Scribe tests/integration suites, or SQL owner tests.
SQL tests classify conflicts and replay tests reject unavailable authority;
neither executes this live uncertainty transition. The evidence's unspecific
claim of existing one-group poisoning tests could not be bound to an assertion.
The code appears to perform the intended poison transition; this finding does
not allege an implementation defect.

Smallest correction: supply an exact existing executable test and valid
candidate-bound execution evidence if one exists; otherwise add a focused real
WAL/PostgreSQL shard test at the existing owner boundary that drives a failed
fresh reconciliation after ambiguous COMMIT and asserts retained ownership/no
ACK/no younger fence/drained submitted IO. Reuse the existing PipelineFixture
and production run loop. Covering the common unavailable-lookup outcome need
not create a matrix of every outage mechanism or change production semantics.
Record the focused nextest selector and execution result in scenario 3. No
production refactor is justified solely by this evidence gap.

## Verification limits

This pass inspected source and actual assertions and reviewed the task's
reported candidate-bound results; it did not rerun Cargo or benchmarks. Runtime
checks were coordinated elsewhere to avoid simultaneous Cargo in this checkout.
TDD remains **VIOLATED** for the disclosed scenarios; later passing assertions
do not repair test-first history. The independent validator must validate INV-1
and reconcile this specialist report with the primary coverage matrix.
