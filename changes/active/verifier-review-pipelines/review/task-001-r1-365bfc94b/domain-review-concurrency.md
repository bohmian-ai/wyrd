# Concurrency domain review

Result: **FAIL**

Subject: base `f4e79ea00523af5cd9212fdacde64f16c97ad554`, candidate `365bfc94bfef1a153f213462f7ef104afa823741`. HEAD remained the candidate at inspection. First-round review; no carried-forward evidence. Reviewed only concurrency and replay boundaries; no source edits or Cargo jobs.

## Authority and coverage

Read the task, approved spec revision 16 (especially REQ-012–015, REQ-046–047, INV-004, AC-006–009/040), output-publication supplement, root and SQL rules, maintainer guide, change workflow, architecture authority map, and owning verification/admission/storage sections. Used CodeGraph before source navigation. Inspected the cumulative range and expanded navigation to the renewal owner, captured Caller, gateway caller, workspace sibling writers, submission sink, and auth revocation owner.

| Boundary / obligation | Implementation evidence | Verification inspected | Result |
|---|---|---|---|
| Durable admission and matching keyed replay | `service.rs` submit staging; `SubmissionSink::write`; SQL `accept_submissions`, `key_holders`, deterministic `RequestKey::execution_id` | Recorded queued-response-loss proof; SQL request-key tests | PASS for key serialization and acknowledgement after commit |
| Reclaim/cancel versus checkpoint writes | `CLAIM_SQL`, `BEGIN_INVOCATION_SQL`, `SETTLE_INVOCATION_SQL`, `CANCEL_SQL`; `ExecutionCheckpoints::fenced`; engine begin-before-dispatch and commit-before-dependents | `invocation_checkpoints_are_fenced_and_survive_reclaim`; `cancelled_work_keeps_checkpoints_and_fences_stale_writes` | PASS for changed token/terminal state; FAIL for expired token before reclaim (CONC-004) |
| Revocation versus admissions still in transit | `revoke_principal_in_conn`, `cancel_principal`, submission sink, runner resolve/dispatch, `Caller::accepted` | Recorded revocation proof covers already admitted work, not late admissions | FAIL (CONC-003) |
| Snapshot writer exclusion and upload transaction separation | `Workspace::freeze`; `HostedTools::run`, extract writer; `Publications::path/complete/accept` | Directory writer-held proof at `pg_durable_verifier_work.rs:482–504` | PASS on ordinary held-writer path; proof limits below |
| Sequential publication replay independent of mutable scratch | `Publications::path/accept`, SQL acceptance | `publication_replay_preserves_attempt_identity` replays unchanged scratch only | FAIL (CONC-001) |
| Concurrent same-call acceptance and unique name | SQL `ACCEPT_PUBLICATION_SQL`, publication unique index | Replay test is sequential | FAIL (CONC-002) |
| Immutable completed content before manifest | File completion before SQL connection; acceptance transaction and event trigger commit atomically | File/JSON immutability and directory snapshot proofs | PASS for no partial accepted manifest on upload failure |

## Material proposed findings

### CONC-001 — INCORRECT: a durable receipt cannot be recovered without the old scratch source

Obligation: REQ-047 / AC-040 and Task scenario 5 require the same original call and arguments to recover its original receipt; later scratch edits cannot alter the accepted snapshot.

Location: `crates/wyrd/wyrd-server/src/verification/hosted/publications.rs:116–149,201–243` (`Publications::path`, `accept`).

Producer-to-consumer path: `path` freezes current workspace bytes, completes fresh uploads and builds a current kind before asking SQL whether this is a replay. After a first file publication commits, lose the reply, remove its scratch source (or retry elsewhere without that source), and repeat the original call/attempt/name/path: `Workspace::freeze` returns missing and SQL never sees the replay. If scratch changes from a file to a directory at that same path, the replay instead returns `Directory` from current `content.kind()`, although the durable row still holds the original `File`. A storage/inventory outage likewise prevents a previously committed path call from recovering its receipt. The JSON path also derives its response locally, but JSON arguments fix its kind; it does not have this mutable-source failure.

Consequence/materiality: an explicitly required lost-ack recovery depends on non-durable scratch and transient storage, and can return a receipt inconsistent with its durable publication. This is the core backend delivered by this task even though Task 3 wires Agent tools later.

Smallest correction: let the existing publication SQL owner resolve the original invocation/name/call/argument identity under the current fence before source freezing or uploads, returning the stored receipt kind for matching calls and conflict for mismatches. Only an absent reservation/publication should need source IO; keep acceptance revalidation after IO. No new upload service or queue is needed. Preserve stale/cancelled lease refusal.

Focused proof: publish a file, delete scratch, replay the same call and assert the original receipt and unchanged manifest; replace scratch with a directory and assert the original file kind; the replay should need no new File upload/inventory write.

### CONC-002 — INCORRECT: concurrent identical publication calls can report a name conflict

Obligation: REQ-047 / AC-040 same-call replay returns the original receipt, while a *different* same-attempt call conflicts.

Location: `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs:690–713` (`ACCEPT_PUBLICATION_SQL`).

Evidence: both sessions share-lock the execution successfully. A inserts one name but does not commit. B begins the same acceptance statement with identical identity/arguments while A is uncommitted. B's `INSERT ... ON CONFLICT DO NOTHING` waits on A's unique-index entry. After A commits, B inserts nothing, but its final `same_call` subquery uses B's pre-commit statement snapshot and cannot see A's row. The SQL comment itself states the outer read uses the statement snapshot. It therefore returns `(true,false,false)` and the Rust method maps that to `Conflict`. Shared execution locks do not serialize these two publication writers.

Consequence/materiality: a transport retry racing an original call's slow commit receives a false publication-name error despite satisfying the required replay identity. The unique index prevents duplication, but does not establish correct acknowledgement semantics.

Smallest correction: preserve SQL uniqueness and resolve a conflicted name on a fresh statement snapshot after the insertion settles (or reuse a transaction-scoped identity serialization mechanism before lookup). This belongs in the existing SQL acceptance owner; caller retries must not be required to distinguish a false conflict. Coordinate with CONC-001's receipt lookup rather than create a second replay implementation.

Focused proof: two real tenant transactions accept identical calls; hold A uncommitted until B is waiting on the unique conflict, commit A, and require B to return `Replayed`, with exactly one stored publication. A distinct call must still conflict.

### CONC-003 — INCORRECT: revocation misses accepted work whose queue admission commits later

Obligation: REQ-014 says authority revocation cancels affected active work and prevents recovery dispatch; Task scenario 7 requires active work cancellation.

Location: `crates/wyrd/wyrd-auth/src/revoke.rs:47–106`; `crates/wyrd/wyrd-server/src/verification/submissions.rs:236–306`; SQL `INSERT_SUBMISSIONS_SQL` at `verifier_executions.rs:112–140` / `accept_submissions:1955`; runner `resolve:338`, `dispatch:496–567`.

Producer-to-consumer path: the HTTP/MCP shared submit path freezes `AcceptedAuthority` before its Scribe-to-submission-outbox handoff (`components/verification/service.rs:723`). Revoke commits principal suspension plus `cancel_principal` over queue rows visible then. A submission awaiting Scribe acknowledgement or the queue writer has no queue row for that scan to cancel. The later submission transaction checks Cards and idempotency but never locks/rechecks the principal's active state; it inserts `pending` with the earlier authority. Runner claim/resolve checks target Cards, not the captured principal; `Caller::accepted` clones the captured Principal. Pure code actions need no later gateway authorization and can execute after revocation; gateway callers also use the captured principal's grants.

Sibling writers: ordinary submissions, scheduled/group candidates admitted by the same sink, and direct SQL enqueue helpers all produce execution rows. Revocation is a separate writer over existing rows. No common principal/admission serialization or tombstone check bridges these owners.

Consequence/materiality: a real asynchronous admission window permits newly runnable work after a successful authority revocation. This violates the required revocation boundary, independent of whether already queued rows cancel correctly.

Smallest correction: coordinate admission with the existing principal revocation authority in the admitting transaction: share the principal's lock/check so either admission precedes the cancellation scan, or it observes suspension and cannot create runnable work. Apply at the common durable admission owner and cover all affected admission producers; do not add repeated checks to each model/tool consumer. Keep stored accepted grants bounded and avoid holding transactions over Scribe/model IO.

Focused proof: pause a real submission before queue admission, revoke its principal and commit, release the submission handoff, and assert no invocation dispatches. Exercise the converse ordering to prove the admitted row is caught by the revocation transaction. Include scheduled/group admission if it can carry that principal's earlier accepted authority.

### CONC-004 — INCORRECT: new checkpoint/publication fences accept an expired lease before reclaim

Obligation: REQ-014 / INV-004 require a current execution lease for dispatch/checkpoint/publication acceptance; output-publication contract says stale leases prevent later commits. Owning architecture says an expired lease is never revived.

Location: `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs:650–710` (BEGIN/FORGET/SETTLE and publication fenced SELECTs); enclosing `runner.rs::ExecutionCheckpoints::fenced` and `hosted/publications.rs::accept`.

Evidence: the new SQL fences check only `status='running'` and token equality. Expiry alone leaves both unchanged. `wyrd.renew_verifier_leases` explicitly rejects `lease_expires_at <= statement_timestamp()`, but no renewal is required before these writes. `LeaseRenewal::run` sleeps lease/6; `renew` processes tenants serially and only cancels a token after `renew_tenant` returns, or an error observes its local confirmation age. A delayed renewal/claim scheduler, process pause, or connection wait therefore permits the database deadline to pass while no reclaim changes the token and `lost` remains unset. On resumption, a checkpoint/acceptance can win scheduling before renewal: SQL applies it, and a new invocation can dispatch. The backend can also be called with its trusted ExecutionLease without holding a renewal guard, as its existing integration proofs do. Neither `ExecutionCheckpoints::fenced` nor `Publications::accept` adds an expiry check.

Consequence/materiality: elapsed database authority can still create work and accept immutable evidence during dependency delay/recovery, contrary to the current-lease boundary. Token replacement and terminal cancellation work, but do not close the expiry window.

Smallest correction: enforce unexpired database-owned lease authority in the existing SQL fence predicates for new invocations/checkpoints/publications. Preserve legitimate replay semantics for already committed results and administrative release behavior rather than blindly modifying every lifecycle statement. No extra watcher, lock service, or local clock is needed.

Focused proof: let a lease expire using PostgreSQL time without reclaiming or renewing it; BEGIN/SETTLE/ACCEPT must refuse, unchanged accepted evidence must remain readable, and a subsequent current claim must work. Existing tests currently change token or terminal state before asserting refusal and therefore miss this case.

## Proof limits and rejected expansions

- Static source/SQL review only; recorded verification is not independently rerun. Concurrency interleavings above are source-derived; no new fixture, external load, or Cargo process was created.
- Snapshot ordinary writer exclusion is implemented: `freeze` holds the workspace mutex through copying and returns owned bytes before storage IO; acceptance opens its tenant transaction only after all completions. No SQL transaction spans upload or model IO in the reviewed path.
- Existing `HostedTools::execute` keeps an extraction writer guard outside `spawn_blocking`; dropping its waiter does not stop a running extraction. This is a lock-lifetime concern, but no current production publication caller concurrently freezes that abandoned invocation was established (Task 3 adds tool wiring). It is not retained as a separate blocking finding here.
- Cancellation committing after an invocation's BEGIN is treated as cancellation of already admitted in-flight work, not a requirement for an impossible atomic SQL/provider dispatch. Later BEGIN/SETTLE/ACCEPT after terminal cancellation correctly refuse through the status predicate.
- No requirement for exactly-once provider execution or immediate analytical visibility was inferred. The report does not review unrelated SDK presentation, RBAC scope completeness, or broad storage retention policy.

The retained findings require independent validation. Candidate source was not changed.
