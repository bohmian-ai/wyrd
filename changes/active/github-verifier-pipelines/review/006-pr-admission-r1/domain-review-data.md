# Persistent data and concurrency review — Task 006

Result: **FAIL**. Two material proposed findings remain; both violate the approved admission/cancellation ordering contract.

## Subject and boundary

Reviewed immutable base `2165fe606efe498db4e0455866d29236e85d889c` to candidate `efda8cb2c59c8206b52dbe1d92fcae1ba34be430`; HEAD still equals the candidate. Authority is revision 5 at `git show 0f08e6c9f:changes/active/github-verifier-pipelines/spec.md`, the original Task 006 (including A1–A18), root AGENTS.md, SQL AGENTS.md, `architecture/wyrd-design.md` Trigger/coordination clock, `architecture/wyrd-security-posture.md` Postgres tenant binding, and maintainer guidance. The revision 6 draft is not treated as authority.

CodeGraph was consulted before locating source. This review follows the persistent-state boundary: installation/access mutation → PR observation and selected policy → reservation/frozen fanout → canonical Scribe input handoff → queue activation → claim/settlement/cancellation. It also inspected existing selection, principal-fencing, scheduled/group/submitted admission, and terminal-delivery owners to identify shared mechanisms and sibling consumers.

## Acceptance evidence and limits

| Invariant / obligations | Source and assertion-level evidence | Assessment |
|---|---|---|
| Tenant-scoped persistence; no new table or queue (A16/A18) | Migration `20261114000000_pull_request_admission.sql` adds three columns, constraints and one partial index on the existing queue. New tenant SQL methods take `TenantConn`, use `wyrd.current_tenant()` for insertion, and inherit existing forced RLS. Queue RLS originates in `20260601000029_verifier_runs.sql:163–170` and survives the rename. `deliveries_admit_only_into_the_connected_tenant` asserts separate per-tenant row sets after deliveries to two installations. | Source supports isolation; no new bypass found. |
| Frozen policy/source/authority and one recoverable fanout (A2/A6) | `reserve_pull_request` resolves every binding once and stores execution identity, policy identities, authority and PR pins before handoff. `CURRENT_SQL` recovers those rows; replay does not resolve a new policy. `interrupted_handoff_completes_the_same_fanout_after_restart` compares every recovered execution ID with the pre-restart reservation and checks both rows are pending/due. Re-selection test checks old IDs remain and later generations freeze the new version. | Covered, subject to ordering findings below. |
| ACK before due, idempotent activation (A5/A6) | Reservation is pending with `next_attempt_at = infinity`, excluded by `CLAIM_SQL`. Scribe frames key PR inputs by execution UUID (`scribe_outbox.rs:1303–1310`); payload timestamps/pins come from the frozen generation. `SubmissionSink::write` calls `activate_pull_requests`, commits, then sends replies (`submissions.rs:312–350`). ACTIVATE_SQL modifies only still-reserved pending rows, so superseded/cancelled rows cannot reactivate. SQL `pull_request_generations_reserve_activate_and_supersede` asserts no claim before activation; partial activation returns first subset, replay returns only remainder, second replay returns empty. | Source supports the handoff and activation fence. |
| New generation cancels unfinished rows and preserves history (A8/A12) | SUPERSEDE_SQL updates pending/retrying/running only. Same status fence as existing CANCEL_SQL; COMPLETE/RETRY/TERMINATE/RELEASE require running plus lease token. SQL generation test checks old pins retained and late terminate returns StaleLease. Server lifecycle/close tests check both queued and running cancellation and late complete StaleLease; completed-baseline test preserves earlier completion. | Covered for rows already present at cancellation. |
| Authority revocation across reservation (A4) | `reserve_pull_request` calls existing `fence_principals` after inserting accepted authorities; this share-locks principal state and cancels inactive owners through the same queue owner used by other admissions. Refused targets insert terminal errored rows with no accepted authority and no runnable due row. SQL refusal test asserts exact refusal code/status; server test asserts revoked-principal rows errored and claim empty. | Existing shared owner reused; no distinct issue found. |
| Latest provider observation / access-loss ordering (A9/A10) | Per-PR advisory transaction lock serializes PR writers, but access writers mutate the installation slot and queue rows without sharing that lock; PR admission does not share their slot lock. Latest observation is inferred from timestamps that are not advanced for every accepted observation. | **Fails DATA-1 and DATA-2.** |
| Existing scheduled/group/submitted consumers (A17) | Existing candidate sorting, accept_submissions/accept_scheduled/accept_grouped, principal fencing, and queue claim/settlement remain; PR activation adds a separate vector to the same writer rather than replacing these owners. | No supported-use regression found within this boundary. |

Verification here is an independent static review of complete relevant source bodies and actual assertions, not a claim of rerunning implementation evidence. No database commands or source edits were made; DB execution is coordinated by the orchestrator. Existing tests cover first deactivation after an existing active generation and new-generation ordering, but do not assert the two races below.

## DATA-1 — Admission can commit runnable work after repository access has been revoked

**Classification:** VIOLATION; material integrity/authority fence failure.

**Authority:** Approved REQ-001 says disconnect stops connection use and affected unfinished work before releasing association; REQ-004/INV-001 constrain admission by current tenant access. Task lifecycle and A10 require removal, suspension, uninstall and disconnect to cancel affected work; A12 requires this at shared authority/queue owners.

**Location:** `crates/wyrd/wyrd-server/src/components/github/pull_requests.rs:224–238` (provider read → reservation transaction), `:274–295` (reservation without installation fence). Existing cancellation owner is `service.rs:368–407`, `:677–697`, `:742–752`; queue cancellation SQL is `verifier_executions/pull_requests.rs:116–134`.

**Producer-to-consumer trace:**

1. A connected tenant has selected Repository bindings, but no execution yet for PR N. A signed opened delivery routes to it. `installation_token` verifies the slot/revision after token minting, then returns (`service.rs:937–949`).
2. The PR read starts and obtains an open source, but its answer is delayed at the existing `hold_next_pull_request_read` seam. The token check has already finished.
3. Disconnect commits. `fence_and_release` locks/releases the installation slot and calls `cancel_pull_request_repositories` in that transaction. With no existing execution for N, the UPDATE cannot persist a per-PR deactivation or cancel future work. Removal/suspension/uninstall share this same access-loss owner and have the same issue.
4. The held read returns. Admission opens a new transaction, locks only the PR advisory key, finds empty history, reads still-selected Repository bindings, and reserves ordinary pending executions. It never checks or locks the now-released installation slot.
5. Scribe acknowledges inputs; the shared submission writer activates them. `CLAIM_SQL` considers status/due/worker format, not installation access. They are runnable after disconnect, with a 2xx webhook answer. Principal fencing does not help because disconnect does not revoke the declaring principal.

This path needs ordinary concurrent provider IO and disconnect, not malicious input. Existing rows often mask it because cancellation supplies a timestamp; the first generation has none. Current access must be ordered with durable admission, rather than inferred solely from whether earlier execution rows existed.

**Smallest safe correction boundary:** Reuse the existing installation-slot lock/revision/access owner in the PR reservation transaction and order it consistently with installation mutations (slot before execution writes). Confirm the same connected installation still reaches the repository and is unsuspended before reservation/replay can commit. Carry the relevant observed connection identity/revision where needed so an in-flight delivery from an old association cannot become a new association's work. Keep provider reads outside the transaction; keep cancellation in the access mutation's transaction; keep scheduled/group/submitted behavior. A check before the external read or a claim-only guard is insufficient because it leaves the admission race or accepted obsolete state. No new queue, table, or generic lock layer is needed.

**Focused closure proof:** Using the existing held-read seam, start the first open observation, commit disconnect while it is held, release it, and assert no runnable rows/claim appear. Cover removal/suspension via the same root access check. Also order reservation first and access loss second and assert the committed rows cancel with `repository_access_lost`. Assert the callback neither restores the slot nor transfers the delayed work to a replacement connection.

## DATA-2 — Newer accepted observations can leave no watermark, allowing older source reads to win

**Classification:** VIOLATION; material current-generation integrity failure.

**Authority:** INV-006, Task A9, and the amended Trigger contract require an observation older than the newest accepted observation or last deactivation to be obsolete. Draft/close must prevent stale open state from becoming current.

**Location:** `crates/wyrd/wyrd-sql/src/queries/verifier_executions/pull_requests.rs:79–97`, especially `pr_deactivated_at = COALESCE(...)` and the WHERE clause; shared source replay branch at `components/github/pull_requests.rs:270–295`. LATEST_SQL uses only max(pr_observed_at, pr_deactivated_at).

**Concrete non-force-push failure:**

1. Admit source A at t0. Convert the PR to draft and deliver that state at t1. Rows become cancelled/deactivated with watermark t1.
2. Make it ready at source B. Start a ready-for-review observation at t2 and hold its provider response after it captured open B.
3. Convert it to draft again. A second converted-to-draft observation at t3 reads the current draft and commits first, answering 2xx.
4. Every existing row is already terminal and deactivated, so DEACTIVATE_SQL's WHERE excludes all rows. Even without that filter, COALESCE would retain the first t1 deactivation. No t3 watermark is persisted.
5. Release the t2 open-B response. `history.obsolete(t2)` compares with t1, returns false, and reserves/activates B despite the later confirmed draft t3.

A sibling manifestation uses a same-source replay: A is admitted at t0; an observation captures B at t1 and stalls; the author returns the PR to A and a newer observation t2 is accepted as replay. The replay writes no ordering timestamp. Releasing t1 then supersedes A with obsolete B. Force-push back to an earlier commit is supported GitHub PR behavior; no new generation is required merely to acknowledge an unchanged current source.

The root defect is treating first-generation/first-deactivation timestamps as a complete last-accepted-observation watermark. The per-PR lock serializes the transactions but cannot recover an observation time that a completed transaction never saved.

**Smallest safe correction boundary:** Advance the queue-owned PR ordering watermark on every accepted current-source observation and every newer deactivation, including repeated deactivation of already-terminal rows. Preserve frozen generation identity/pins, status/history, and the original observation time used to construct byte-identical Scribe input retries; do not simply rewrite that canonical input timestamp to serve as the ordering watermark. Extend existing queue metadata only as needed, under the existing PR lock. Reuse the same monotonic ordering for the access-loss path rather than introducing another consumer-side stale guard. This is private coordination metadata, not a new product contract or inbox.

**Focused closure proof:** Extend the existing deterministic held-read lifecycle test with the draft → held-ready → draft sequence above; the delayed answer must return without any new claimable B execution. Add the unchanged-A replay with a held intermediate-B read and assert A's original IDs/pins remain current. Assert replay/partial handoff still stages the original canonical bytes/identities, and terminal history stays terminal. These prove the missing ordering property directly; broad suites alone do not.

## Final assessment

The existing queue, Scribe handoff, RLS transaction boundary and status/lease fences are substantially reused, and their normal-generation assertions are meaningful. Neither proposed finding requires a new engine or broader product behavior. The missing serialization of access authority and incomplete accepted-observation watermark are enough to violate required cancellation/latest-source behavior on ordinary reachable races.

Not independently executed: PostgreSQL lanes, provider integration, or static gates. No source changes, commits, or additional reports were made; findings require independent validation and focused race proof.

Orchestrator verification update received after this review: fresh repository-managed PostgreSQL migrations passed and all 15 server PR/connection tests passed; see `verification.md`. This corroborates the inspected existing assertions, but those selectors do not exercise DATA-1/DATA-2's missing interleavings. I did not independently execute those commands.
