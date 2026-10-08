# Concurrency and lifecycle domain review — `7fcb45fc1`

## Review Findings

### Critical

None.

### Important

- **`CONC-RR-001` / prior `FIND-TASK-001-1` — INCORRECT — leader work is not linearized with term revocation.** [`crates/vala/vala-bifrost-redux/src/forge/leadership.rs:205`](../../../../../../crates/vala/vala-bifrost-redux/src/forge/leadership.rs) clones the held `Arc<ForgeHeldTerm>` and releases the slot lock before checking revocation, while `set_held` replaces the slot and cancels the old term independently at lines 223–246; consequently `accept`, `serve_pull`, and `serve_report` can obtain an apparently live clone at lines 364–473, lose the term concurrently, and then mutate or pull from the revoked schedule. The observable harmful case is `serve_pull`: an old leader can return a dispatch after its database term has been revoked and a successor can own the term, violating the one-leader scheduling boundary. The closeout proof invokes the handlers only after revocation is complete, so it does not exercise revocation between `term()` and the schedule operation. Linearize validation plus the synchronous schedule operation against `set_held` with the same ownership guard (for example, keep a read guard on the held slot through the operation); repeated cancellation checks cannot close a check/use race. Add a deterministic interleaving test that pauses each handler after selecting the term, revokes/replaces it, then proves the handler either completed before revocation or refuses without any post-revocation effect.

- **`CONC-RR-002` / prior `FIND-TASK-005-R1-3` — INCORRECT — the lease-TTL timeout releases exclusive authority while deletion acceptance is explicitly unknown.** [`crates/vala/vala-bifrost-redux/src/forge/worker.rs:8037`](../../../../../../crates/vala/vala-bifrost-redux/src/forge/worker.rs) times out `attempt_cleanup_delete`, converts that timeout to `ExpiredCleanupOutcome::Uncertain` at lines 8203–8224, then drops `exclusive` and commits the authority transaction at lines 8040–8041; [`crates/vala/vala-bifrost-redux/src/forge/orphan_gc.rs:1555`](../../../../../../crates/vala/vala-bifrost-redux/src/forge/orphan_gc.rs) similarly drops a timed-out `apply_gc_deletions` future and commits the authority transaction at lines 1585–1586. The implementation comments correctly acknowledge that the store may already have accepted the request, so dropping the client future does not establish that no later destructive effect can occur; a new Oracle reader can therefore commit an old cut after the lock is released and before the accepted delete finishes. The focused tests `hung_cleanup_delete_surrenders_table_authority_at_the_lease_bound` and `hung_orphan_delete_surrenders_table_authority_at_the_lease_bound` intentionally prove this unsafe ordering by acquiring a shared authority before releasing the paused delete, which is the opposite of TASK-005-R2's accepted requirement that exclusive authority remain live through the external effect's known outcome. Keep exclusive authority until the submitted operation has a terminal known result, or use a cancellation primitive whose completed cancellation guarantees that the backend cannot later apply the effect; retained evidence and idempotent replay are recovery mechanisms, not reader serialization. The proof must pause a request after submission, trigger its bound/cancellation, race an Oracle acquisition, then allow the backend to finish and show either that acquisition stayed blocked through the terminal result or that completed cancellation made a later effect impossible.

### Suggestions

None.

## Overall verdict

**FAIL.** The remediation closes the worker-shutdown, retained-cleanup, Analytical ownership, immutable-deadline, sibling-maintenance, and SQL-ownership findings, but it does not close the leader-term check/use race or the destructive-effect/authority lifetime gap.

## Acceptance and prior-finding matrix

| Finding | Verdict | Classification | Plain conclusion |
|---|---:|---|---|
| `FIND-TASK-001-1` | FAIL | INCORRECT | A handler can select the old term immediately before revocation and use its schedule immediately after revocation. |
| `FIND-TASK-002-1` | PASS | — | Shutdown removes each still-dispatched claim once, closes it as cancelled, and emits exactly one `NotStarted` report only after the durable close succeeds. |
| `FIND-TASK-003-1` | PASS | — | A refused or uncertain prepared cleanup retains its exact frontier and replays rather than losing attempt ownership. |
| `FIND-TASK-005-R1-1` | PASS | — | The leader stream owns the Analytical graph, synchronously revokes followers and aborts local drivers before active-claim release, under the recorded human decision that unconsumed remote IO may finish after revocation. |
| `FIND-TASK-005-R1-2` | PASS | — | Initial acquisition and retry both derive remaining time from the same immutable attempt deadline. |
| `FIND-TASK-005-R1-3` | FAIL | INCORRECT | The TTL path deliberately surrenders the exclusive lock before an already-submitted delete has a known terminal outcome. |
| `FIND-TASK-005-R1-4` | PASS | — | The stale sibling-expiry ban is removed without weakening exact orphan evidence and survivor checks. |
| `FIND-TASK-005-R1-5` | PASS | — | Production table-layout SQL is owned by `vala-sql`; remaining direct table references are test support. |

## Boundary and authority coverage

| Boundary | Source traced at candidate | Result |
|---|---|---|
| PostgreSQL leader term → in-memory held term | `forge/leadership.rs`, `forge/scheduler.rs` | Renewal is serialized and refusal, error, or local-deadline expiry revokes the term, but handler use is not atomic with revocation (`CONC-RR-001`). |
| Held term → notify/pull/report/maintenance | `forge/leadership.rs`, `forge/gc.rs`, closeout journeys | Scheduler and maintenance loops observe cancellation; direct handler schedule operations retain the check/use race. |
| Pulled claim → shutdown closure/report | `forge/worker.rs` | The dispatched map owns the claim until one shutdown closure path removes it, commits cancellation, and reports `NotStarted`; no double-report path was found. |
| Prepared cleanup → refusal/uncertain replay | `forge/worker.rs`, Forge task SQL | The exact prepared cursor remains durable and does not advance for refusal or uncertainty. |
| Leader stream → Analytical descendants → active claim | `oracle/query_stream.rs`, `oracle/analytical.rs`, `oracle/planner.rs` | Explicit owner/drop order revokes the graph before claim release; no supervisor-owned query lifetime remains. |
| Attempt deadline → initial/retry claim rows | `oracle/planner.rs`, `catalog/bifrost_catalog.rs` | Both paths recalculate positive remaining duration from one immutable local deadline before PostgreSQL stamps the row. |
| Exclusive table authority → external destructive effect | `forge/expire.rs`, `forge/orphan_gc.rs`, `forge/worker.rs`, `forge/table_authority.rs`, `vala-sql` reader authority | Normal completed paths hold the conflicting row lock continuously; timeout paths release it with an unknown submitted effect (`CONC-RR-002`). |

## Happens-before assessment

- Term renewal itself has a sound bound: the renewal mutex serializes acquire/renew, and the local `timeout_at` revokes no later than the locally measured term end when PostgreSQL does not answer.
- Revocation does **not** happen-before every later schedule action: cloning the term and checking its cancellation state are outside the write-side critical section that replaces and revokes the slot.
- Worker shutdown closure has a single ownership handoff through the dispatched map; a claim admitted concurrently with shutdown is rechecked and then enters the same closure path.
- Prepared cleanup settlement happens only after the per-candidate authority transaction finishes, and a refused/uncertain outcome leaves the cursor replayable.
- Analytical graph revocation happens-before active-claim release structurally: `LeaderStreamOwners` drops the admitted graph owner first, which closes grants and aborts local drivers, then drops the active-read owner.
- The original deadline happens-before every active-cut SQL call as the sole source of its remaining duration; retry cannot rebase the deadline.
- A lease timeout does **not** establish that a remote delete was rejected or completed, so authority release after that timeout has no valid happens-before relation to the destructive effect's terminal outcome.

## Open Questions

- Is surrendering table authority at the lease TTL despite unknown backend acceptance a newly approved product decision? If so, it conflicts with TASK-005-R2 lines 61–71 and its focused proof at lines 99–107 and requires an explicit authority/spec revision; the current code cannot be accepted as closing `FIND-TASK-005-R1-3` under the reviewed authority.

## Verification Notes

- Review subject was the immutable tree at `7fcb45fc1`; the checked-out worktree had advanced only by an unrelated Bifrost-variant spec draft, so all reviewed source was read with commit-scoped Git operations.
- Authority read: revision-11 spec and history, TASK-001/002/003/005-R1, remediation TASK-001-R1/TASK-002-R1/TASK-003-R1/TASK-005-R2, prior ledger, `AGENTS.md`, `architecture/agent-rules.md`, and `architecture/bifrost-design.md`.
- Source and focused proofs inspected: leader acquisition/renewal/revocation and RPC handlers; scheduler and maintenance stop paths; worker dispatch admission and shutdown closure; expired-cleanup replay; Analytical graph ownership/drop/settlement; active-cut acquisition/retry; SQL table authority; snapshot expiry, orphan deletion, and expired cleanup; relevant unit, SQL integration, and Bifrost journey tests.
- Recorded candidate evidence reports the full `mise run gate`, `verify:bifrost`, Forge/Oracle journeys, SQL tests, principals integration, formatting, and lints green. This domain pass did not rerun those expensive environment-owning lanes; it statically falsified two claimed closures using source and the committed focused tests.
- The Analytical tests use lifecycle event hooks rather than a real remote transport cancellation acknowledgement. That is sufficient for the accepted revocation semantics, but it does not prove remote IO physically stops; the recorded human decision explicitly makes that stronger property non-blocking.
