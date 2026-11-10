# Focused follow-up: request ownership and queue authority

Subject: base `0e8a31b14d336fadf62e1d8525a7fa030ab6c4eb`, candidate `cd905f4cd929785bda25696326d58b4a4ae88dee`. HEAD remained the candidate. Specification revision 18, durability supplement revision 2, original TASK-001 and Revision 1 apply.

Scope: only STD-003, SYS-2, governing authority and affected producers/consumers. Read no other discovery findings. Used CodeGraph before source searches and inspected the cumulative changed-owner diff. No source edits or Cargo commands.

## STD-003 — RESOLVED

The applicable passage belongs to verification execution, not an unrelated application-Run contract. `architecture/wyrd-design.md:1801–1828` explicitly names inline `verify`, `submit`, inputs and `wyrd.verifier_executions`. “Neither Run entry point writes Postgres on the request path” is immediately explained by background Scribe/outbox admission. The amended common-state lifecycle at `:1857–1892` adds checkpoint durability; it does not repeal that ownership constraint.

There is no contradiction between this constraint and **internal Bifrost fence SQL**. `architecture/bifrost-design.md:786–803` requires the Scribe shard to commit durable batch fences after WAL sync. `:481–486` authorizes extending acceptance for writer, revision, cancellation, expiry and replay. The supplement retains ordinary PostgreSQL owners and authorizes existing Bifrost recovery/acceptance extensions. None requires request-owned head installation or permits all generic SQL on the request path. Generic head metadata is not itself a prohibited execution-payload mirror; the issue is who drives the mutation.

The concrete violation is proven:

1. `components/verification/service.rs:568–584`: inline direct execution awaits `RunState::fresh`.
2. `verification/state.rs:386–399`: fresh constructs its distinct writer and awaits claim.
3. `state.rs:335–353`: claim acquires a Vala tenant connection, calls `scribe_state_heads::claim`, then commits before returning to the request.
4. `vala-sql/src/queries/scribe_state_heads.rs:66–96`: claim inserts or updates the head.

Every otherwise admitted direct request therefore has a request-owned mutation/commit dependency and failure boundary before its checkpoint handoff. Proving “no queue row” does not prove the broader explicit constraint. This finding does not prohibit the generic head table or Scribe SQL, and does not require deleting queued takeover claims because direct shares the helper.

Smallest permitted correction boundary: preserve RunState and the existing generic claim/expected-revision mechanism, but drive fresh direct registration through the existing background Scribe/outbox handoff. Retain authoritative absence/recovery, distinct identities, installation before accepted advancement, ordered revisions and receipts. Do not add a direct queue, second writer, or missing-head bypass accepting progress unfenced. Existing claim semantics can be retained; merely moving the existing operation to its background owner requires no new product or direct-resume policy. An implicit initialize-on-first-advance protocol is not required by this finding.

Burden: bounded existing-owner handoff work and direct-path proof. Verify no direct request-owned INSERT/UPDATE/commit while its first accepted checkpoint still has one writer, exact-next acceptance and an outbox receipt. If implementation discovers this move requires a new persistent/concurrency contract, use TASK-001's stop condition; this pass does not establish that necessity for direct registration alone.

## SYS-2 — defect RESOLVED; correction decision UNRESOLVED

Required behavior is explicit: REQ-015 fences subsequent commits on cancellation; REQ-057 preserves expiry/cancellation/deadline protection. The supplement's “Queue leases and stale-worker protection” expressly says a stale or expired writer cannot gain acceptance merely because no replacement ran. `wyrd-design.md:1870–1883`, `bifrost-design.md:481–486`, TASK-001 scenario 3 and R1 retain that obligation.

The failing ordering follows source end to end:

1. `state.rs:631–681`: durable confirms the lease, then freezes a pending revision. `StateScribe::confirmed` (`:231–235`) calls `HeldLease::confirm`; `leases.rs:68–77` checks/renews the queue lease and releases its transaction before staging.
2. `state.rs:275–295`: the pending operation reserves capacity and stages immutable ExecutionState. Its fence contains execution key, writer, revision and offered `not_after` only. `runner.rs:485–489` sets that bound to the original execution deadline, not queue lease expiry.
3. Hold the staged frame before Scribe acceptance, then cancel. `service/inspection.rs:602–624` commits queue cancellation before returning cancelled status. `verifier_executions.rs:612–628`, `:3148–3165` clear running status and lease expiry in only the queue row. Principal cancellation (`:3179` onward) does likewise. `wyrd-auth/src/revoke.rs:104–114` calls it within revocation; admission's `fence_principals` (`verifier_executions.rs:2048–2073`) is another caller. None changes the Scribe head.
4. Outbox projection (`scribe_outbox.rs:1267–1284`) carries the immutable fence into the frame. Dropping the waiter cannot retract the outbox-owned item, also an explicit R1 requirement. No queue revalidation occurs at its generic acceptance.
5. `scribe_batch_commits.rs:478–505` advances fresh offers in the batch-fence transaction. `scribe_state_heads.rs:147–160` requires matching head writer, expected revision and an unelapsed offered original deadline. Cancellation changed none. The held frame advances after cancellation without a replacement claimant.

The checkpoint can contain a start, settlement, publication or terminal choice. It becomes authoritative recoverable progress after cancellation won. Materiality does not depend on recalling external effects or proving a cancelled waiter receives success. The cumulative diff removes old invocation begin/settle predicates requiring the current running, unexpired queue lease at the write boundary, replacing them with preflight confirmation.

Sibling authority writers constrain a correction: claim/reclaim changes token, attempts and expiry (`verifier_executions.rs:410–462`); renewal uses `wyrd.renew_verifier_leases` (`20261022000000_verifier_execution_batches.sql:80–106`); completion, retry, termination and release end running authority (`verifier_executions.rs:494–529`, `:580–608`). Fixing only the inspection handler misses revocation and other writers. Replacement currently propagates through a separate RunState claim; cancellation has no equivalent propagation.

Expiry is the same missing acceptance input, but a short lease is not the production default: defaults are ten-minute lease/five-minute execution timeout (`verification/mod.rs:209–210`). Cancellation alone proves the supported failure. An expiry proof must establish its actual lease/deadline ordering rather than assume defaults exercise it. The original-deadline predicate exists and must not be reported as wholly absent.

Reuse existing replay: `scribe/shards.rs:1882–1907` suppresses unfenced state-table commits; `:3913–3943` handles StateRefused before active visibility. Cancellation currently produces no refusal, so the late frame obtains a legitimate fence. Additional replay suppression cannot repair that incorrectly accepted frame.

Existing proof misses this ordering. `pg_durable_verifier_work::cancelled_work_keeps_checkpoints_and_fences_stale_writes` cancels before the next begin, so preflight catches it. `batched_checkpoints_recover_exactly_the_accepted_batch` holds a frame but releases it before replacement and later tests the replaced writer. Its existing HeldRevisions seam can support cancel-before-release proof.

### Correction readiness and burden

The required outcome and owners are settled: PostgreSQL remains the queue lease/cancellation authority; generic Scribe acceptance must order that authority against expected-next advancement. Inherit acceptance-first progress; reject invalidation-first frames, including after restart. However, no declared committed queue-authority handoff into generic acceptance exists in the inspected candidate. Its head API is claim/head/advance, with no invalidation or lease-authority reference. Claim changes writer by generation and does not observe cancellation or expiry.

`crates/wyrd/wyrd-sql/AGENTS.md`, Transactions, requires a declared committed handoff with an idempotent consumer for cross-owner durable effects, and forbids extending transactions through private cross-crate queries. TASK-001 scenario 3 and its material stop conditions independently prohibit private cross-tier SQL/transactions and a second lease scheduler. The task and supplement do not select how queue invalidation/renewal reaches generic acceptance with one authoritative ordering and failure recovery.

None of these is a decision-complete local correction: another confirmation (same race); asynchronous cancellation notification alone (acceptance can overtake delivery); changing writer only on explicit cancellation (misses expiry/renewal and other authority writers); querying the private Wyrd queue from generic Vala SQL (cross-owner coupling); or duplicating lease scheduling in Bifrost. A generic authority bridge could be valid, but selecting its atomicity, invalidation acknowledgement, renewal visibility and crash reconciliation is a material cross-owner concurrency/persistent-authority decision, not a helper detail. This report does not authorize one.

Retain the material defect, but do not issue an open-ended “extend the boundary” task as though it were decision-complete. Under the review skill and TASK-001 stop conditions, route the unresolved correction as **SPEC_REVISION_REQUIRED** unless independent validation identifies an already declared mechanism supplying this ordering. The bounded decision is the queue-to-generic-fence authority handoff and its failure/acknowledgement ordering, preserving existing owners; it does not authorize changing publications or adding a lease service.

After that decision, focused proof should hold a revision after confirmation/staging, commit explicit cancellation and separately principal revocation, release without replacement, and assert unchanged head/recovered publications/terminal selection. Restart on retained WAL and retain the converse acceptance-first case. Exercise real queue expiry/renewal with documented bounds at the same seam. Reuse the existing harness; no broad suite is required for this local proof.

## Disposition

| Uncertainty | Evidence | Correction readiness |
| --- | --- | --- |
| Request-owned direct claim versus permitted generic fence SQL | RESOLVED: STD-003 applies; internal fence SQL remains authorized | Existing-owner background handoff; retain fencing |
| Staged frame after queue cancellation/expiry | RESOLVED for cancellation; expiry narrowed against actual defaults | UNRESOLVED: declared authority handoff/order absent; settle specification decision before prescribing machinery |

Overall: **UNRESOLVED**, limited to decision-complete queue-authority integration. No unrelated finding proposed. These conclusions require subsequent independent validation, not a vote on the discovery reports.

Skipped: runtime/Cargo execution and unrelated reports, per assignment; no source edits.
Risk: prescribing queue/head synchronization without settling the committed-handoff boundary can replace this integrity bug with an unapproved second authority system.
