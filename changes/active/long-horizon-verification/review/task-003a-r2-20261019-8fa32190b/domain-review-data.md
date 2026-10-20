# Persistent data, transaction, concurrency and clock review

**Result: PASS. Proposed finding ledger: empty.**

## Immutable subject and scope

Reviewed cumulative `22335c2c40a5ad24c27bfa0d4686f4da49f922c5...8fa32190b26b11588721e8db44b3eb94139e3739`, using `8376bb2aa..8fa32190b` only to locate R2 changes. Reviewed the approved revision-20 spec, original TASK-003a, R1 authority, amended R2 task and its appended evidence. Historical review findings were treated as hypotheses rather than current defects. CodeGraph exploration preceded source searches. No source edits, Cargo/test lanes, or concurrent PostgreSQL sessions were performed by this reviewer.

Domain: PostgreSQL pending activity, tenant capability/RLS, claim/consume concurrency, immutable snapshots, admitted-execution retry/settlement, terminal refusal shape, exact elapsed durations, and the tests proving these seams. Security-grant construction is covered only at its data boundary; the separate security reviewer owns authorization analysis.

## Authority and source coverage

| Boundary | Governing authority | Inspected owners and consumers |
|---|---|---|
| Tenant SQL and transaction lifecycle | AGENTS, agent-rules SQL capability/RLS rules; architecture patterns | `verifier_executions.rs` tenant methods use `&mut TenantConn`; due-tenant/depth reads use `OperatorPool`; `scheduler.rs` owns transaction commit; `20261018000000` pending-table forced RLS, composite binding FK and indexes |
| PostgreSQL coordination time | wyrd-design grouped verification and coordination-clock sections; current REQ-004, R2 FIND-2 | TriggerDuration parsing/write bound; frozen GroupEndSettings; ScribeSink activity producer; RECORD_GROUP_ACTIVITY_SQL; DUE_GROUP_SQL; ordinary schedule and lease SQL |
| Pending handoff and concurrent arrivals | REQ-004, original task decisions 2/7; accepted limits | ScribeSink::write/schedule_groups; VerifierExecutionQueue::record_group_activity/claim_due_group/consume_group/enqueue_group/refuse_group; scheduler group_pass/hand_off_groups/hand_off |
| Content identity and frozen inputs | REQ-002/004/010; task decisions 3–6 | GroupPoller/Reader/Sources/Visible/Payloads; PinnedGroup::new; INSERT_GROUP_SQL/EXISTING_SNAPSHOT_SQL; unique group-snapshot index; claim input decoding/status decoding |
| Completion/reaction order | REQ-006; task decision 8 | LOCK_GROUP_BINDING_SQL, COMPLETE_SQL, PREVIOUS_GROUP_SQL, INSERT_DISPATCH_SQL, complete; claim/retry/release/terminate/exhausted-lease siblings |
| Pre-admission terminal shape | revision-20 REQ-004; R1 selected shape; R2 FIND-7 | All four production execution INSERTs; inherited run constraints; 20261018 exception; new 20261019 constraint; SQL rejection loop and scheduler incomplete/error path |
| Visibility/backpressure durability limits | spec accepted tradeoffs; bifrost-design row identity/admission; analytical-operations reliability reference | Post-ACK scheduling before reserved submission handoff; Scribe batch replay fence; row lock handoff; rollback/read refusal behavior; no source settlement reconciliation or new recovery protocol |

## Acceptance evidence

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Accepted `1d` and `24h` mean 86,400 elapsed seconds; 365-day maximum stays write-time | `verifier_executions.rs:201–207` extracts epoch from the authored, validated interval and multiplies by one second before timestamptz addition. Parser accepts only positive whole s/m/h/d through 31,536,000 seconds; no month/calendar units or large floating quantities enter this expression. Authored text and upsert identity remain unchanged. | Existing P2 drives real record_group_activity with four bindings and compares daily/hourly due times across a verified DST transition; original hour/year assertions remain. Root rerun selected and passed one P2 test. | PASS |
| Refused execution has no accepted authority/input/window/record/snapshot and cannot run | New migration `20261019000000_verification_refused_shape.sql:5–16` adds all eight NULL requirements to the authority-free branch only. Required origin/status/attempts/group/result/lease/settled fields are nonnullable or explicitly tested; inherited terminal-error/deadline/lease invariants remain. | P2 inserts/reads valid refusal and checks attribution/no result/no snapshot/no claim/no dispatch. Six nested-transaction mutations each require this exact constraint to reject the update. Root P2 passed. | PASS |
| Existing authority-present writers remain valid | INSERT_SUBMISSION_SQL, INSERT_SCHEDULED_SQL, INSERT_GROUP_SQL retain authority. New constraint immediately permits authority-present rows; old ordinary shape constraints remain. INSERT_GROUP_ERROR_SQL alone omits authority and pin fields. | Existing P2 enqueues/claims/completes overlapping real group rows and reads statuses; new tests leave ordinary branch unchanged. | PASS |
| One pending row per exact tenant/binding/group; new activity postpones only that work | Batched arrays preserve tuple alignment; EXISTS matches each frozen exact subject/Verifier source and owner. DISTINCT prevents duplicate upsert targets; primary key and forced RLS scope identity. No request-path database read added. | Source audit plus current focused server pending-groups test, rerun by root. | PASS |
| Handoff cannot delete an arrival admitted after it | Claim holds FOR UPDATE SKIP LOCKED pending-row lock throughout read, enqueue/refusal and consume. Same TenantConn commits once. Concurrent activity upsert waits; after committed deletion it inserts fresh pending, after rollback it updates retained pending. Scheduler does not later delete on execution completion. | Existing server pending-groups test covers waiting background activity, handoff, capacity and rollback; root passed P1. | PASS |
| Only complete successful visible snapshot can enqueue; result without input retries then expires | Reader rejects failed/degraded terminal; incomplete check occurs before pin/enqueue. Due query derives >5-minute overdue solely in PostgreSQL. Scheduler refuses overdue incomplete in same transaction as consumption; incomplete earlier returns without commit and preserves pending. | Root P1 passed both pending handoff and incomplete_snapshots_retry_then_error tests. | PASS |
| Same digest never creates another execution, while distinct snapshots overlap | RFC-8785 content includes binding/group, ordered execution/result identities and exact declarations. One tenant/binding/group/digest unique index; conflict resolves that digest's existing execution regardless of status. Queue's ordinary leases own retries; no outstanding gate. | P2 duplicate and overlapping enqueue, selected result identity, pinned claim inputs and statuses. | PASS |
| Group reaction compares settlement order and is replay-idempotent | complete locks existing binding before completing; COMPLETE_SQL stamps database settlement; previous query uses same binding/group ordered settlement+execution ID; completed replay returns before recomparison. Dispatch uniqueness unchanged. No lock held across evaluation/provider IO. | P2 later-admitted execution settles first, unchanged verdicts stay quiet, changed verdict emits one dispatch with prior settled result; replay remains quiet. | PASS |
| No unapproved recovery, ledger, quota, source ceiling or SQL authority replacement | Existing queue remains sole execution owner. Only pending scheduling table is added. New constraint is an ordinary ALTER migration, not another authority or decoder fallback. | Cumulative source/migration diff inspection. | PASS |

## DST proof deviation assessment

R2 requested a fixed-date New York example. The implementation instead changes only this test transaction's TimeZone to a valid POSIX zone whose next transition starts within the next hour. It first asserts calendar-day addition really differs (23 hours), then executes the actual activity SQL, requiring due(1d)=due(24h) and due(1d)-due(1h)=23 hours. This is stronger coverage of the production statement than testing a copied expression at a historical literal; it changes no product or persistent decision. No test-only clock API or new harness is warranted.

I asked root to check the fixture's year-boundary uncertainty before accepting it. Root's separate, sequential managed-PG SELECT probes constructed the exact POSIX-rule form from explicit timestamps at 2026-12-31 23:30, leap-year 2024-12-31 23:30, 2026-01-01 00:30 and 2026-10-19 12:30. All returned calendar-day=23h; all transactions rolled back. Logs and command are recorded in this review's verification.md. The day-of-year and extended rule hours therefore work for the investigated leap/year boundary cases; no finding remains from that hypothesis.

## Migration/rebuild claim

`wyrd-sql/src/lib.rs:46` uses `sqlx::migrate!("./migrations")`; no crate build.rs migration-directory tracking exists. A migration-only local iteration can therefore require crate invalidation to update embedded migrations. This R2 commit also changes `wyrd-sql/src/queries/verifier_executions.rs`, which invalidates the crate normally when pulled; the broad claim that recipients of this actual commit must manually touch lib.rs is not established. Root reran migration tests and P2/P1 at the candidate without touching source. No new build machinery is necessary to accept this bounded fix.

The new ALTER migration also meets amended R2's explicit requirement not to change an applied checksum. It changes the existing constraint rather than adding compatibility readers, backfill, or a data conversion. The original pre-release migration policy does not justify rewriting an applied migration against this newer explicit remediation direction.

## Findings and limits

No material domain finding remains. FIND-TASK-003a-2 and FIND-TASK-003a-7 are closed by source and focused current proof. Exact validation of missing docs is the standards reviewer's scope. Existing source writers and settlement siblings were inspected rather than assuming the new constraint establishes application correctness alone.

Independent static source review was supplemented by root's current P2/P1 and read-only boundary-probe evidence, not by new domain-agent test runs. Full SDK/MCP journeys, benchmarking, arbitrary corruption of older authority-present rows and migration-only build-infrastructure improvements were not requested or rerun. Accepted post-ACK crash-signal loss, continuous postponement, visibility/retention limits and durable SQL backlog remain product limits, not remediation findings.
