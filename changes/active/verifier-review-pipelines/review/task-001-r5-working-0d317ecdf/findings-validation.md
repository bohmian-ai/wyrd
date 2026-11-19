# Independent R5 findings validation

**Recommended verdict: FIX_REQUIRED.** Retain FIND-TASK-001-19 and
FIND-TASK-001-23 with revised residual scope, and new FIND-TASK-001-27. Carry the
original, unnumbered hosted-dependent proof obligation separately. No new
production recovery defect or specification decision is established here.

## Subject and validation method

Reviewed HEAD `0d317ecdf2d28d2e77b6f394ebe0f443d256d580` plus the user-submitted
working source pinned in `subject-sha256.txt`, with packet/authority identity in
`packet-sha256.txt`. Both manifests passed at entry. R3
`d89356985566ab3492699682942438c25c8322fd` and original base
`0e8a31b14d336fadf62e1d8525a7fa030ab6c4eb` supply cumulative acceptance context.
The expressly submitted working-tree subject is the recorded exception to the
committed-subject convention. Report files do not change the candidate.

Read all four current reports and verification.md, original Task 001 including
Revision 1, R4/R5 remediation, R3 verdict and findings-validation, the applicable
root/SQL rules, Rust/maintainer guidance, implementation-execution and
spec-driven-development authority, and the owning terminal/recovery/security
sections of the approved supplement and architecture. CodeGraph preceded source
navigation. Inspected actual HEAD-to-working and R3-to-working changes rather
than accepting implementation closure claims.

Validation traced complete affected terminal owners and their callers:
Frozen/Offer/Work, execute/record/conclude/result/deliver, attempt/deliver_owed,
both runner settlers, RunState select/fail/decide/durable and StateScribe write;
SQL terminate/release/cancel/principal cancellation, delivery claims and shared
settlement, frozen-input/Trigger and pending row decoding; request cancellation
and principal revocation transaction composition. Read the complete capacity
test and T6, their setup/reclaim seams, Outbox reserve/admit/drop, runtime builder
outbox selection, the plan test/walker and owning discovery query, and the cited
changed-item rustdoc. Selected-result result reader, group logical-row decoding
and existing result ACK owner were inspected for changed-consumer consequences.
No Cargo, source/test/task/authority edit, staging or commit was performed. This
report is the sole authored output.

## Specialist applicability and coverage

| Boundary | Evidence and independently validated coverage | Decision |
| --- | --- | --- |
| Shared decision, coverage/ACK, exhausted attempt and settlement lifecycle | Current invariant report traces common operation and both callers; current system report independently traces interruption/recovery; actual source confirms their ownership map | Required invariant review present and sufficient for this round |
| Substantial terminal owner consolidation and Rust rules | Maintainer report covers changed runner/state/SQL/test owners, builder reuse and static R4 obligations; its exact documentation omissions are checked below | Required maintainer review present |
| Persistent queue, tenant capability, delivery fencing and migration | Data report covers all terminal SQL producers, TenantConn transactions, operator discovery, frozen identity, immutable migration, current fence proof and affected library plan test | Required data/concurrency domain review present |
| Failure propagation, restart, result consumers and sibling availability | System report covers actual producer commits, source/SYSTEM/ACK/settlement failures, existing supervision and result/group/Operator consumers | Required system review present |
| Security/RBAC | R3 security/data validation provides unchanged exact refs, current grants, File custody and scoped internal authority. Current data/system reports specifically recheck immediate revoke/cancel and server-owned delivery; source retains same-transaction authority removal and no cancelled-principal computation | Separate fresh security specialist reasonably omitted: changed control/delivery seam is covered, no new token/grant/security mechanism |
| Broad cross-tier standards, provider, SDK/UI/native lifetimes | R3 standards/validation carries source-backed unaffected boundaries; this round changes server/SQL/test owners covered above, with no new SDK/wire/platform mechanism. R4 proto reservation is unchanged in the rebuild and checked statically | No broad repo or additional domain review required |
| Independent final validation | This fresh validator checks every selected report, prior closure, source, correction burden and omitted-role justification | Satisfied |

Unchanged files alone are not the carry-forward argument. R3 verdict/validation
and their referenced concurrency, data, standards and security reports covered
accepted-prefix loading, generic batch replay, short memory locks/shared waiters,
publication/File custody, ordinary permissions and direct final fast return.
R5 reuses those owners. Changed terminal deadline/orchestration and affected
producer/consumer paths receive current review instead of carrying R4 acceptance.
The four selected reports are available and cover their assigned boundaries.
There is no material unresolved reviewer conflict requiring another discovery
pass. No validator-only new blocker was discovered.

## Proposal reconciliation

| Proposal | Disposition | Stable identity / reason |
| --- | --- | --- |
| INVAR-R5-19, SYS-R5-01 | REVISED | FIND-TASK-001-19: production mapping is corrected; explicit reserved-share failure/recovery observation remains missing. One cause, not two new runtime findings |
| MNT-R5-001 | REVISED | FIND-TASK-001-23: old three locations close, but the cumulative changed-item documentation gate remains open at the new SQL helper and counting methods |
| DATA-R5-01 | CONFIRMED | FIND-TASK-001-27: changed plan-test assertion rejects the retained required delivery index; independently recorded exact execution corroborates the source diagnosis |
| INVAR-R5-PROOF, MNT-R5-002, SYS-R5-02 | CONFIRMED, consolidated | One original unnumbered hosted A/B→C observation remains incomplete; do not invent a new numbered production defect |
| R5 Limits claim that capacity proof requires unauthorized capacity machinery | REJECTED | Existing slot reservation and existing builder selection provide a bounded test composition; no payload filling or production configuration is needed |
| An inferred global exactly-once physical result-row requirement | REJECTED as a current finding | No selected reviewer establishes such a regression or obligation. Logical selected identity and completion idempotency do not imply one physical row across independently restaged slices |

## Retained deduplicated ledger

### FIND-TASK-001-19 — REVISED / MISSING — reserved-share recovery proof remains open

Discovery sources: INVAR-R5-19 and SYS-R5-01. Original obligation: REQ-055/058,
AC-049, R4 scenario 4's separate source and reserved-share outages, explicitly
retained by R5 scenario 2/P3. This is incomplete closure of a previously proven
final-attempt exhaustion defect, not a newly conjectured production failure.

`pg_verification_runtime.rs:2242–2286` changes only the batch-fence SELECT grant.
It proves source outage recovery, same selected ID, unchanged attempts and zero
engine entries. Its complete body never causes reserved-share refusal.
`runner.rs::execute:568` refuses before loading state; `record:635` has the same
reservation boundary. Both return Defer via busy. `attempt:1369–1375` maps an
exhausted Defer/Retry/Release to LeaseLost, so `settle` opens no transaction;
ordinary queue release would instead refund an attempt. The static correction
is credible, but the early reserve-refusal path is not exercised by the source
outage or low-level T5 byte-admission test.

Meaningful consequence: a required regression observation at normal outbox
backpressure remains absent, so those tests cannot detect reintroduction of
false exhaustion, refund or premature retirement on that path. Do not describe
the current code as demonstrated to lose work.

**Smallest decision-complete correction:** extend the existing final-attempt
failed-settlement scenario with a separate interval of actual reservation
refusal on the recovery runner's outbox. Hold its existing reserved item share,
observe an exhausted reclaim with no computation, budget change or terminal
mutation, release the reservation, then reclaim and observe the same selected
result completing. Keep the original source-outage observation.

Existing mechanism suffices: `Outbox::reserve:435–458` charges an AtomicUsize
slot count, `admit:900–906` uses checked addition against the item limit, and
Reservation::drop releases untouched slots (`:714–725`). This does not allocate
a million payloads or a GiB. The runtime builder in `verification/mod.rs:553–560`
uses `state.bifrost.scribe_outbox()` when publication_fault is absent. Existing
Harness::runtime always installs that fault, so use a test-local recovery
composition omitting it, and reserve the actual shared server outbox. The first
failed-settlement setup may keep its existing fault outbox. Existing OutboxFull
item limit information can establish the reserved share; do not hard-code or
add a production capacity setting. No new governor, service, fault framework,
public option or synthetic host load is justified.

Focused closure: run P3's existing exact selector through repository Postgres,
with assertions proving actual refusal and restoration as well as identity,
attempt conservation and zero engine entries. The test-only change and its
existing narrow format/lint lane are sufficient; no broad journey rerun.

### FIND-TASK-001-23 — REVISED / VIOLATION — cumulative changed-item rustdoc gate

Discovery source: MNT-R5-001. Root AGENTS requires documentation for every
new/changed Rust item and Errors on fallible items; `agent_docs/rust.md:235–240`
explicitly includes private helpers and requires workflow/invariant information.
This mandatory gate is independent of runtime probability or green compilation.

The original Checkpoint::prepare, execution_state test helper and Leased::writer
omissions are corrected. However, new
`VerifierExecutionQueue::settle_delivery` at
`verifier_executions.rs:3397–3411` returns Result and propagates database failure
while its sole doc sentence references the statement. It lacks Errors and the
caller-owned transaction/fence contract. Both delivered and redeliver use this
same helper, so document that owner once. New cumulative R3-to-working
`Counted::write` and `Counted::flush` in `state.rs:410–416` have no method
rustdoc. They were rebuilt byte-identically from R4, which does not remove them
from cumulative task acceptance.

**Smallest correction:** local rustdoc only: explain the SQL helper's matching
claim fence, caller-owned transaction and actual database-error condition;
document the counting sink's saturating count/no retained buffer and always-Ok
flush/write behavior. A Result signature does not justify inventing an error
condition for the infallible implementation. Preserve types and runtime behavior.
Batch these few edits with the same remediation, never a separate style round.
Static changed-item inspection and existing format/lint checks close the gate;
no new checker or runtime test is warranted.

### FIND-TASK-001-27 — CONFIRMED / REGRESSION — plan test excludes retained delivery index

Discovery source: DATA-R5-01. Root Testing rules prohibit bypassing a red gate;
R5 preserves bounded tenant discovery and affected SQL regressions. The exact
library test is already registered in tasks/verification.md.

`verifier_executions.rs:4397–4400` now permits only verifier_runs_due and
verifier_runs_leased. The actual query at `:757–773` retains owing/delivering
reads over delivery_due_at with no computation-status restriction. The retained
migration's verifier_executions_delivery_due partial index is the correct owner
for those reads. `execution_reads:4305–4319` visits every relation-read node,
including these subplans even with no owed rows. HEAD-to-working diff explicitly
removes that index from the accepted alternatives and test rustdoc.

The orchestrator's exact traced library run in verification.md selected one
test and failed at `:4398:13` on Index Only Scan via
verifier_executions_delivery_due. Its migration apply/idempotence passes and
the separate 34-test integration module do not close this library regression.
No production query performance defect is inferred from this failure.

**Smallest correction:** restore that retained index as an allowed alternative
and in this test's documentation, retaining the index-scan-only condition.
This preserves the original bounded-scan property and acknowledges the approved
third index. Do not allow arbitrary plans, remove the owing query, delete the
test, alter the migration, or introduce a harness. Closure is the exact existing
library nextest selector with repository PostgreSQL and tracing, as recorded in
verification.md, plus narrow checks on the local test edit.

## Original required proof, separately carried

INVAR-R5-PROOF, MNT-R5-002 and SYS-R5-02 identify the same open observation from
original Task 001 R1.3, R3 verdict/validation and R4 scenario 6. T6 at
`pg_durable_verifier_work.rs:3012–3106` begins left/right directly, fabricates
settlements, joins writer.settle, awaits both, compares snapshots, and manually
begins combine. Reclaim compares stored entries/attempts. There is no real
hosted graph, C value consumption or provider-call observation. This useful
state-owner proof can pass when an actual engine consumer is wrong.

**Confirmed missing proof, no new FIND ID:** drive the existing hosted engine
and checkpoint adapter with parallel A/B and a real C dependent on both complete
values. Reuse the existing held-Scribe/reclaim facilities. Observe C cannot
enter before accepted coverage, consumes both values after acceptance, and
recovery reuses accepted settlements without repeating settled calls. Retain
the original accepted-prefix/interruption distinctions; unchanged fence proof
may carry its existing evidence. A manual begin or snapshot equality cannot
replace the consumer observation. The burden is a bounded integration test
completion justified by an explicit original requirement; no production engine,
scheduler, state format, new harness or performance threshold is authorized.
If it exposes a runtime failure, diagnose that failure before changing code.

## Requirement and prior-closure matrix

| Obligation | Implementation / verification evidence | Reconciled result |
| --- | --- | --- |
| USER-TERMINAL-002 rollback and applied migration preservation | R5 records rollback inventory and migration exception; current cumulative diff and empty HEAD migration delta match rebuilt queue coordination | PASS with temporal action evidenced by the record, not provable solely from final bytes |
| USER-TERMINAL-001 / FIND17 one terminal decision→required ACK→settlement operation | execute/record converge on conclude and existing builder/deliver; accepted failures prevent dispatch; shared queue settlement separates acknowledged from owed; P1/P2 commit assertions and P4/P5 actual producer/recovery assertions | CLOSED within reviewed boundary; recorded R5 RED/GREEN and current source support closure |
| Immediate cancellation/revocation, restart custody and exact producer identity | Same TenantConn suspension/cancel, durable due marker, record-only server delivery and claim fences; P4 actual cancellation/revoke/admission/refused-group paths; T3 retained group/owner and negative access assertions | PASS; FIND16 CLOSED |
| Accepted selection waits for all required records and stale work cannot retire replacement delivery | conclude selected → builder → result receipt; matching computation/delivery fences; P5 and latest-claim SQL proof | PASS |
| Exhausted unknown state/capacity never spends/refunds computation or becomes false exhaustion | Defer→LeaseLost source correct; P3 source outage passes but no capacity refusal | PARTIAL; FIND19 remains for explicit capacity proof |
| Variable payload accounting and stable admission/release | Checkpoint counts full serialized values conservatively and stores immutable charge; T5 variable-growth/refusal/release assertions and fresh recorded GREEN | FIND22 CLOSED |
| Rustdoc and protobuf retirement | Old rustdoc locations corrected; tag 10/name state reserved; recorded codegen GREEN; new cumulative methods above undocumented | FIND26 CLOSED; FIND23 remains |
| Concurrent accepted settlements release real hosted C and survive recovery | T6 only drives state methods; no hosted consumer | FAIL original unnumbered proof |
| Bounded tenant discovery regression | Production owing index remains; modified exact library assertion rejects it and independently fails | FAIL FIND27 |
| Accepted-prefix recovery, publication/File custody, generic fence/replay, short locks/shared waiters, original bounds, direct final fast return, provider/platform exclusions | R3 source-backed verdict/validation and referenced specialists, current unchanged owners plus changed terminal-path checks | Carried PASS within original proof limits |
| Stable selected identity, group/Operator effects and result replay | Builder preserves selected ID/interval; reader joins exact ID; group decoder deduplicates logical result IDs; completion/dispatch idempotency retains existing owner | PASS for approved logical replay; no physical-row uniqueness certificate |

Human-directed USER-TERMINAL-001/002, USER-PG-001 and USER-DURABILITY-001 remain
human-directed identities, separate from independently numbered findings.
The concurrent advisory question about possible idempotent result writes does
not amend this review's approved obligations. New slices may append equivalent
physical result rows; generic same-sealed-batch replay dedup and logical selected
result identity are distinct. No concrete changed-consumer regression was
proposed or established. Do not turn an advisory improvement into remediation
or infer global exactly-once external effects.

## Verification and convergence recommendation

verification.md correctly distinguishes implementation-recorded 83/83 server,
34/34 SQL integration, T5 and format/lint/codegen evidence from independent
execution. Its independently run exact library plan test is red and corroborates
FIND27. A GREEN label for P3/T6 covers their assertions, not the missing scenarios.
This validator ran no Cargo and does not claim fresh runtime/migration execution.

R5 materially closes the rejected split terminal workflow and its recovery and
result-ACK consequences. Remaining capacity and hosted gaps are incomplete
carried proof; FIND27 is a fix-induced test regression, and FIND23 is an explicit
cumulative static gate. They require no new architecture or specification
decision. The next bounded scope is two existing-seam observations, one local
plan-test repair and a few local rustdoc edits, with focused selectors and
affected narrow checks. Carry accepted production evidence forward rather than
redesigning terminal delivery or restarting comprehensive Task 001 discovery.

Skipped independent Cargo execution and unrelated discovery. Risk: the explicit
capacity/hosted observations remain unproved and the affected library gate is red.
