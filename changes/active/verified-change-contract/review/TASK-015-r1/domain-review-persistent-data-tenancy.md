# Domain Review — Persistent Data and Tenancy

## Immutable Subject

- Candidate: `9b560d00569656ee7fdde19a9c76fa55c8d55fbb`
- Task: `changes/active/verified-change-contract/tasks/TASK-015-eval-runs-in-the-batch-fence.md`
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 60, specifically REQ-077, REQ-108, and AC-014
- Additional review scope: only the conflict resolution in merge `c5527627a50dd66a9f53d760d80f59bcb59609f9`
- Explicit exclusions honored: prior TASK-013/TASK-014 work, the already-reviewed audit outbox and benchmark, merge `3f8767a5f`, and generic-outbox internals not changed by TASK-015

The candidate remained at the exact commit above during this review.

## Reviewed Boundary

I traced the persistent-data and tenancy path end to end:

1. Gate invokes the observation hook only after the first durable Scribe commit. `ObservationEnqueue` derives each committed row's subject, record identity, and managed event time, then stages the item under `AuthContext::tenant` (`crates/wyrd/wyrd-server/src/verification/observations.rs:105-123`; `crates/vala/vala-bifrost-redux/src/gate/mod.rs:986-1009`; `crates/vala/vala-bifrost-redux/src/tables/eval/observations.rs:68-125`).
2. `ObservationRunSink::write` opens one tenant-bound transaction through `WyrdPostgres::tenant_conn`, calls the batch queue operation, and owns the single commit boundary (`crates/wyrd/wyrd-server/src/verification/observations.rs:62-87`). The SQL callee accepts `&mut TenantConn<'_>` and does not commit, preserving the repository transaction-ownership rule.
3. `VerifierRunQueue::enqueue_observation_batch` locks only `observations_ready` bindings visible through that transaction's forced RLS, in stable binding order; checks the exact owner's current activity and target readiness; then performs one multi-row insert (`crates/wyrd/wyrd-sql/src/queries/verifier_runs.rs:115-167,1640-1735`).
4. The inserted row freezes the locked binding/Card identities plus the request's exact record ID and event time. The insert uses `wyrd.current_tenant()`, the table is forced-RLS, and the durable uniqueness key is `(data_tenant_id, binding_id, input_record_id)` (`crates/wyrd/wyrd-sql/migrations/20260601000029_verifier_runs.sql:49-145`).
5. Duplicate records already stored or repeated within the same batch are removed before ordinal assignment. Ordered binding locks serialize concurrent overlapping batches; `ON CONFLICT DO NOTHING` provides the final retry fence. The ordinal uniqueness constraint is tenant-qualified (`crates/wyrd/wyrd-sql/src/queries/verifier_runs.rs:130-167`; `crates/wyrd/wyrd-sql/migrations/20260601000033_verifier_run_observation_ordinal.sql:1-29`).
6. Production composition retains the same outbox instance in `Bifrost`, hands it to Gate through `ObservationEnqueue`, and drains it after Bifrost shutdown closes acknowledgements (`crates/wyrd/wyrd-server/src/boot/mod.rs:1101-1134`; `crates/wyrd/wyrd-server/src/state.rs:1559-1608,1766-1775`; `crates/wyrd/wyrd-server/src/app/server.rs:867-881`).

## Authority and Source Coverage

| Concern | Authority | Source/test evidence | Result |
|---|---|---|---|
| Tenant identity and RLS | `AGENTS.md` tenant-isolation rules; `architecture/agent-rules.md`; architecture constraints | Authenticated Gate tenant is the outbox key; sink acquires `TenantConn`; inserts use `wyrd.current_tenant()`; `verifier_runs` has enabled and forced RLS | PASS |
| Transaction ownership | `architecture/agent-rules.md` caller-owned `TenantConn` rule | Queue method performs no transaction control; sink is the boundary that opens and commits the transaction | PASS |
| Exact active-owner admission | REQ-108 | `accepts_records` calls `binding_activity` for each locked binding before adding any row; inactive-owner coverage is included in `observation_batches_insert_once_per_binding_and_record` (`pg_verifier_runs.rs:1860-1966`) | PASS |
| Batch atomicity and one insert per tenant | REQ-077, TASK-015 Scenario 1 | One tenant sink call surrounds lock, activity/readiness resolution, one array-based insert, and commit | PASS |
| Retry idempotency and ordinal stability | REQ-077, AC-014 | Tenant/binding/record unique index, pre-number deduplication, binding locks, and ordinal uniqueness; SQL integration test proves repeat inserts zero and the next record receives the next ordinal (`pg_verifier_runs.rs:1921-1966`) | PASS |
| Outage retention, recovery, shutdown | REQ-077, AC-014 | Integrated journey proves ACK survives a temporary insert refusal, retry recovery creates exactly one run per binding, and replay adds none (`eval_verification.rs:1196-1289`); server integration proves pending retention and shutdown outcomes (`pg_verification_runtime.rs:2420-2493`) | PASS |
| Merge resolution preserves both owners | User-directed merge scope | The combined resolution in `c5527627a` retained both `observation_runs` and `audit_outbox` in boot composition and `BifrostComposition`; candidate boot still wires both (`boot/mod.rs:1104-1133`; `state.rs:1605-1608`) | PASS |

## Directed Test-Edit Assessment

### `continuous_eval_runs_the_terminal_matrix`

Deleting the permanently refused pre-phase and its "never a run" assertion does not weaken a revision-60 obligation. That phase asserted the former per-frame fail-open/drop behavior: one permanently refused request disappeared while later same-tenant work progressed. Revision 60 instead requires a failed flush to retain its batch and retry. Keeping the old phase would intentionally strand the tenant batch and prevent the terminal matrix from reaching the behavior it is meant to prove. The required outage/recovery proof remains at the user-journey tier in `integrated_enqueue_outage_preserves_ack_and_recovers` and at the server/Postgres seam in `observation_outbox_retains_through_an_outage_and_flushes_at_shutdown`.

Result: **PASS**.

### `sealed_replay_on_a_later_day_activates_once`

Replacing the blocked-transaction count with the exact outbox pending count strengthens the proof for the revision-60 architecture. Gate stages synchronously before returning the acknowledgement, the held table lock keeps the original request in flight, and the generic outbox permits only one in-flight write for that tenant. Therefore an exact pending count of two proves that only the original and distinct sentinel were staged; either replay being staged would increase the count. The later durable assertion still requires exactly `2 * AGENT_BINDINGS` runs and verifies the stored event-time behavior (`eval_verification.rs:1009-1095` and following assertions).

Result: **PASS**.

## Open Risk Classification

The implementer's noted risk—one permanently failing item retaining and blocking its tenant's later backlog—is **not a finding against TASK-015**. Revision 60 and REQ-077 require retention and retry when Postgres is slow or unavailable, accept only hard-kill/deadline loss, and do not authorize classifying or discarding a permanently invalid request. The approved task likewise requires failed batches to remain at the front of the tenant queue. Defining poison-item isolation or terminal discard semantics would require new persistent-data and failure-policy authority; it is not an omitted obligation of this task.

## Findings

No material findings.

## Verification Limits

- Per assignment, I ran no Cargo, `mise`, or test commands.
- I inspected the implementation evidence appended to TASK-015 and the named SQL, server integration, and Bifrost journey tests. Their recorded green results were not independently re-executed in this domain pass.
- Generic `Outbox` internals were treated as an already-reviewed dependency and were not reopened; this review verified the TASK-015 sink contract and its callers/consumers.
- `git show --remerge-diff c5527627a` could not create its temporary object directory in this read-only Git metadata environment. I instead inspected the merge's combined diff and compared the merge against both parents for the two conflicted files. Those views agree on the resolution described above.

## Overall Result

**PASS**

The candidate satisfies the persistent-data and tenancy obligations in REQ-077, REQ-108, and AC-014 within the directed scope. The tenant boundary, transaction ownership, active-owner gate, idempotency fence, ordinal assignment, lifecycle wiring, and specified merge resolution are preserved without a regression.
