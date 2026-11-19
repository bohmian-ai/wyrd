# Persistent data and concurrency remediation review

Result: PASS

Immutable candidate: ec5471ca2421e9ba0049658546554f1b3c6dac8a. Prior candidate: 4d001e6d9. Implementation baseline: 16abc095e. HEAD remained the candidate during this review. Inspected the implementation diff and the prior-candidate diff for this boundary. No source changes or test commands were made.

## Boundary and authority

Scope is FIND-009-8 and fix-induced regressions in SQL cancellation, accepted revision identity, fenced Bifrost outcomes, current-status inspection, and restart replay. FIND-009-2, -6 and -9 remain excluded accepted limits. No stronger remote exactly-once guarantee is imposed.

Authorities inspected: root AGENTS.md; crates/wyrd/wyrd-sql/AGENTS.md; architecture/bifrost-design.md canonical execution_events contract at 470–509; architecture/wyrd-design.md pull-request delivery at 2531–2571; architecture/wyrd-security-posture.md Postgres tenant binding at 806–860; approved spec revision 11 REQ-013/014; original TASK-009 delivery obligations; prior findings-validation.md FIND-009-8 and domain-review-data.md; remediation.md obligations and evidence. CodeGraph preceded source navigation.

Revision 11 resolves the old authority conflict: SQL cancellation does not create a new accepted checkpoint, and its delivery uses a separate event at the existing revision. Public cancellation starts delivery after commit; principal revocation is recovered at next restart. The original task's older single-outcome wording is superseded by that explicit amendment.

## Coverage and acceptance matrix

All source paths below are repository-relative. Server paths are under crates/wyrd/wyrd-server/src unless stated otherwise.

| Obligation or invariant | Source evidence | Proof inspected | Result |
| --- | --- | --- | --- |
| Cancellation has a separate identity without replacing its accepted revision's outcome | components/github/delivery.rs:609–633 chooses github_delivery_cancelled only for Cancelled and hashes event/execution/revision. The ordinary event hashes the same bytes as before, preserving existing outcome identity. row at 647–692 and record at 570–605 use the same status-derived event. | outcome_identity_is_per_event_execution_and_revision at 713–730 asserts stable identity and differences for event, execution and revision. | PASS |
| Fence and query identify the same cancellation outcome; absence differs from unavailability | delivery.rs:353–384 derives the tenant/table fence from the status-selected event, reads committed_batches, returns None only for authoritative absence, and requires the matching visible row otherwise. verification/inputs.rs:304–343 quotes and filters execution, exact revision and event. | Inspected both negative branches and the cancellation inspection assertions below. No new direct fence-unavailable test was run. | PASS |
| Public cancellation starts delivery only after a successful tenant-scoped commit | components/verification/service/inspection.rs:1017–1043 authorizes existing Run action, calls existing queue.cancel through the authorized TenantConn, reads status, commits, then spawns only when this call cancelled a pull_request execution. Repeat cancellation keeps existing no-op behavior. | pg_github_delivery.rs:997–1044 calls the public cancel endpoint after waiting delivery, asserts cancelled status, delivered inspection, exactly one same-id Check with completed/cancelled, and unchanged POST/PATCH count after restart. | PASS |
| Inspection reads the outcome required by current status | components/verification/service.rs:298–311 assigns retained.status before passing it with retained.revision into recorded; no cancellation response or verdict mutation is added. Retained::status at inspection.rs:248–275 preserves accepted terminal/failure precedence and otherwise uses SQL status. | Cancellation journey asserts served status cancelled and github_delivery exactly delivered; its initial waiting_for call requires the earlier waiting outcome to have been delivered. The old ordinary outcome therefore cannot alone satisfy this read. | PASS |
| Principal cancellation recovers using the same status and revision | SQL verifier_executions.rs:616–623 and 3206–3222 cancel unfinished subject/actor executions without checkpoint changes. delivery.rs:222–248 enumerates all current PR executions; SQL verifier_executions/pull_requests.rs:85–94 has no unfinished-status predicate. presented at inspection.rs:584–662 reads SQL state plus accepted revision; cancelled status reaches the new event path. | Static producer-to-consumer trace. The connected test covers restart after an already recorded cancellation, rather than principal revocation before first cancellation delivery. This is a stated proof limit, not evidence of a failing path. | PASS |
| No accepted checkpoint before cancellation still has an identity | verification/state.rs:641–659 returns revision 0 and fresh state when no checkpoint fence exists; presented combines that with the SQL cancelled row, and the new event key accepts revision 0. | Static source trace. No separate cancellation-before-first-checkpoint test was run. | PASS |
| Waiting and cancelled outcomes cannot absorb each other's fenced record | delivery.rs:594–600 stages each under its event-specific key, and recorded queries that same event. scribe_outbox.rs:1466–1488 gives Fenced its own stable frame; fenced_batch_id at 1820–1822 also includes tenant and table. The first accepted outcome for one event stays immutable through the existing fence owner. | Unit key inequality plus connected waiting-to-cancelled same-revision journey. No new simultaneous-writer runtime test was run. | PASS |
| Tenant isolation and current-generation suppression remain intact | cancel retains the authorized TenantConn and caller.data_tenant_id. recorded uses a vala tenant connection and tenant-derived fence. records at service.rs:1105–1135 grants the tenant SYSTEM reader only existing verification tables. allowed at delivery.rs:512–555 still checks tenant installation access and current active generation before attempts. Deactivated/superseded executions remain excluded by current-generation recovery and the existing allowed path. | Existing round-1 data/security source evidence carried forward, checked against the changed handoff and query arguments. No permission, tenant key, raw pool, migration, SQL payload mirror or new durable owner was introduced. | PASS |

## Shared owners and concurrency assessment

The correction extends the existing GithubDeliveries owner and existing ScribeWrite::Fenced mechanism. It does not create a second cancellation writer, queue status mirror, progress ledger, lock, lease, transaction-spanning provider wait, or new persistence API. InputResolver remains the only typed outcome reader. The only changed query selector is the required event discriminator.

An in-flight waiting task can still record its ordinary event after cancellation, but that record cannot satisfy the cancelled event's fence/query. Its remote Check body is nonterminal and Write::send at delivery.rs:148–184 now skips an existing Check, so it cannot PATCH a cancelled Check back to in_progress. The explicitly accepted concurrent absent-create duplicate limit is preserved. No distributed serialization is necessary to satisfy this correction.

Accepted result/checkpoint precedence, cancellation notification and worker fencing, immutable state acceptance, and lookup-before-create remain existing owners. The fix does not alter them; no new evidence in this scope invalidates their carried-forward acceptance. Supersession/deactivation remains distinct from a public cancellation and still suppresses stale source delivery.

## Findings and limits

No material persistent-data or concurrency finding is proposed. FIND-009-8 is closed within revision 11's approved behavior. This domain report does not decide rendering, cooldown, full changed-item rustdoc, or the final review verdict.

Skipped broad journeys, live GitHub, and new runtime commands; the orchestrator owns focused execution. Principal-revocation-before-restart and simultaneous ordinary/cancelled recording were traced statically, not newly exercised here.
