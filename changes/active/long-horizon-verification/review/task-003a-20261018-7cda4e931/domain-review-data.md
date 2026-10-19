# SQL and data boundary review

Overall: **FAIL** — one narrow duration-contract finding (DATA-001). No additional transaction, tenancy, snapshot-identity, or settlement defect was established.

## Subject and boundary

Reviewed base `22335c2c40a5ad24c27bfa0d4686f4da49f922c5` to candidate `7cda4e93159ab328de2a74068d5e1978a2590818`, against the original TASK-003a at base and approved specification revision 17. Scope is pending scheduling persistence, post-ACK activity, source snapshot identity/selection, tenant capabilities, and group settlement concurrency. No source was changed. No superseded or other reviewers' reports were read.

## Review Findings

### Critical

None.

### Important

None.

### DATA-001 — Delete the undeclared limit that changes an accepted `after`

- **Classification:** DRIFT / INCORRECT; bounded lower-severity contract violation, not a near-term operational urgency.
- **Location:** `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs:191–210`, with the cap bound by `record_group_activity` at line 1730.
- **Violated obligation:** TASK-003a locked decisions 1–2 preserve existing `TriggerDuration` and compute `due_at = statement_timestamp() + binding.after`. Specification REQ-004 and `architecture/wyrd-design.md` Grouped verification prescribe that same exact relative duration. Neither authorizes a second private duration ceiling.
- **Producer-to-consumer evidence:** `TriggerDuration::parse` (`crates/wyrd-spec/src/card/trigger.rs:93–118`) accepts positive counts up to nine digits and retains the authored duration. Registration freezes `after` into `GroupEndSettings`; `ScribeSink::schedule_groups` passes acknowledged matching activity to the sole `VerifierExecutionQueue::record_group_activity` owner. Its SQL uses `LEAST(EXTRACT(EPOCH FROM after::interval), 3155760000)`. For valid `after: 1000000h`, the declared duration is 3,600,000,000 seconds, while the pending row receives only 3,155,760,000 seconds. Both dates are representable in PostgreSQL; this discrepancy is not required by the timestamp-range rationale. The due-row predicate later uses the shortened value, making this binding expire about fourteen years earlier than declared. All source-input/result producers converge on this same writer, so no per-caller correction is necessary.
- **Smallest correction:** Remove `GROUP_AFTER_MAX_SECONDS`, its fifth bind, and the `LEAST` shortening from this SQL owner. Preserve the database-clock calculation and existing SQL-error/outbox-retry behavior; do not introduce another duration option, clock, or scheduling mechanism. This finding does not require redefining extreme timestamp-overflow behavior or adding a new public validation policy.
- **Focused closure proof:** Extend the existing SQL scheduling test to record activity for a valid, PostgreSQL-representable duration above the private cap, such as `1000000h`, and assert the persisted `due_at` equals that statement's database time plus the authored duration. This is an immediate database assertion, not a long-running clock test. Keep the existing ordinary `1h` and debounce/claim checks. Run its existing exact C9 command under the repository-managed Postgres wrapper.

## Authority and source coverage

| Boundary | Authority inspected | Source and conclusions |
|---|---|---|
| SQL capability, RLS and transaction lifecycle | User-provided AGENTS rules, complete `architecture/agent-rules.md`; reference router; architecture patterns and security posture's SQL/trust principles | `TenantConn`, `OperatorPool`; pending migration; binding migration; queue activity/claim/consume and due-tenant query. New pending PK/FK is tenant-qualified, binding deletion cascades, forced RLS has both USING and WITH CHECK, and due/claim indexes match the two access patterns. Tenant methods accept `TenantConn` and do not commit; cross-tenant discovery accepts `OperatorPool`. |
| Exact scheduling and atomic handoff | Original task locked decisions 1–2, 7, 9; spec REQ-004; design Grouped verification/Coordination clock/Trigger | `ScribeSink::{write,schedule_groups}`, `VerifierExecutionQueue::{record_group_activity,claim_due_group,consume_group,enqueue_group}`, scheduler's complete group handoff path, boot composition. ACK precedes SQL; fallible scheduling SQL precedes reservation consumption; failed SQL returns the identical slice to existing retry. Pending row remains locked through snapshot/insert/delete/commit; a blocked arrival recreates a row after successful deletion. DATA-001 is the exception. |
| Logical evidence and immutable identity | Original task decisions 3–6; spec selection/identity obligations; design Grouped verification; Bifrost Durability and visibility; complete OLAP-serving and analytical-reliability references | `GroupPoller`, complete `Sources`, `Visible`, `Earliest`, `Payloads`, `PinnedGroup::new`, group INSERT/conflict lookup, digest migration. One successful UNION statement selects exact owner/subject/Verifier/group rows. Logical input/result deduplication, selected-result tie break, shared per-tuple list order and null alignment agree with authority. RFC-8785 digest includes binding/group, ordered execution/result IDs and pinned declarations. Unique partial index and matching conflict lookup preserve unchanged-snapshot idempotence across execution statuses. |
| Settlement and result replay | Task decision 8; design Grouped verification and queue/Operator contracts; spec verdict-change behavior | `LOCK_GROUP_BINDING_SQL`, `COMPLETE_SQL`, `PREVIOUS_GROUP_SQL`, `INSERT_DISPATCH_SQL`, complete `VerifierExecutionQueue::complete`, actual `VerifierRunner::settle`, claim/renew/retry owners. Binding lock precedes execution update; the next statement's database time follows lock acquisition; predecessor is latest other settled verdict. Replay returns before comparing against later results, lease-token fencing remains, and dispatch insertion stays in the caller's transaction. No provider/Bifrost IO occurs under the settlement lock. |
| Bounded active work and sibling paths | Task decision 7; AGENTS runtime rules; Bifrost resource/failure invariants; spec accepted limits | Complete scheduler group/cron loops, `ClaimLoop` changes and capacity ownership, runtime builder, runner, Operator capacity caller. Snapshot work and runner share one pool, snapshot permits drop after handoff, and cron has its own loop. Existing source inputs/results, gateway/audit writers and post-ACK submission handoff were traced. |
| Test evidence | AGENTS test ownership/workflow; task C8/C9/C13 and candidate's recorded commands | Read changed `pg_verifier_executions` snapshot/settlement test and `pg_verification_groups` available-row, debounce, concurrent arrival, bounded-batch and cron-isolation tests. These support the ordinary timing, identity and transaction paths, but omit a duration above the new cap. |

The complete cumulative changed-file list and domain diffs were inspected, including in-place execution/binding migrations, the new pending migration, queue/binding queries, outbox, scheduler, capacity composition, and adjacent contract/test callers. Other language/document-only changes are covered here only where they project these persistence contracts; this is not the separate standards or full acceptance review.

## Verification Notes and limits

- Source/static review only; no test, build, database environment, or synthetic load was started by this reviewer. Root reports its coordinated C13 rerun passed under the repository Postgres wrapper. Candidate-recorded C8/C9/C13 passing results were examined as claims, not results independently rerun by this reviewer.
- Existing tests exercise pending-row exclusion between claimers and activity racing a committed handoff. The settlement test explicitly reverses completion order and replays earlier completions, but performs those completions sequentially; real concurrent settlement interleaving was established from the binding lock and statement boundaries, not an executed stress test.
- The missing-table fallback was considered and not reported: production Scribe ingress and tenant provisioning require eager built-in registration, so an ordinary results-only missing-input-table path was not established. No speculative remedy is requested.
- Accepted visibility gaps, post-ACK/pre-pending crash loss, outbox loss, indefinite postponement, overlapping distinct snapshots, settlement-order reactions, unbounded durable backlog, optional live Agent reads and pending benchmarking remain accepted; this report requests no recovery, visibility barrier, quota, serial execution gate or additional ledger.

Skipped executable reruns to avoid competing with root's coordinated environment. Residual risk: concurrency evidence here is source-based; DATA-001 is directly determined by the stored SQL expression.
