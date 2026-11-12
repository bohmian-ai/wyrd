# Task 006 invariant review

**Result: FAIL.** Two material lifecycle defects remain: observations that do not create an execution fail to fence older observations, and a new source fails to supersede existing work when the selected policy has no PR bindings.

## Subject and coverage

- Base: `2165fe606efe498db4e0455866d29236e85d889c`.
- Candidate / inspected HEAD: `efda8cb2c59c8206b52dbe1d92fcae1ba34be430`.
- Approved authority: revision 5 of `spec.md` read with `git show 0f08e6c9f:changes/active/github-verifier-pipelines/spec.md`; original task `tasks/006-pr-admission-and-supersession.md`.
- First review, full cumulative diff; no prior acceptance carried forward. Assigned scope: shared admission/lifecycle state, source/policy fanout freeze, canonical identities, observation ordering, supersession, access cancellation, handoff/recovery and sibling queue consumers.
- Read root instructions, SQL `AGENTS.md`, task-review skill, `changes/spec-driven-development.md`, `agent_docs/index.md` and maintainer guide, architecture authority map, queued Trigger authority at `architecture/wyrd-design.md:2209-2263`, and operations PR recovery owner. Used CodeGraph before source searches.
- No source changes, commits, test modifications or DB mutations. No test suite rerun: proof below distinguishes inspected assertions and recorded execution evidence from independent execution. Only this report was written.

The existing owners traced were signed webhook dispatch -> `GithubConnections::pull_request` -> `selected_repository` / `pull_request_bindings` / `BackgroundAuthority` -> queue reservation -> execution-keyed Scribe input frame -> `SubmissionSink::write` -> activation -> ordinary queue claim/runner -> fenced settlement. Sibling scheduled, grouped and submitted admissions still use the same SubmissionSink and claim owner. Access-change writers use `GithubConnections::access_lost` -> `cancel_pull_request_repositories`; principal revocation uses the existing `fence_principals` / `cancel_principal`. Repository reselection is owned by `settle_repository_selection`, which changes selection without cancelling existing execution rows, as required for no sweep.

## Acceptance matrix for the assigned invariants

Rows are seeded from A1-A18. PASS here means the named state invariant has source and assertion support, not independent execution of the full surface/security/standards obligation. Full eligibility (A3), App pagination/protocol behavior (A7), public surface parity (A13), documentation completeness (A14), and amendment completeness (A15) remain the assigned behavior/system/repository reviewers' coverage; their state consequences are covered below.

| Obligation / invariant | Implementation evidence | Assertion-level verification evidence inspected | Result |
|---|---|---|---|
| A1/A18: selected connected identity and tenant stay attached through queue handoff | `pull_requests.rs:196-239,314-378`; queue writes require `TenantConn`; Scribe stage carries the same tenant | `pg_github_pull_requests.rs:1301-1341`: second installation admits zero rows into first tenant; first installation produces exactly two rows in its tenant and count zero in the other | PASS within shared-state scope |
| A2: fanout, exact policy, authority and PR pins freeze before staging | `reserve_pull_request` inserts every target in one caller transaction; `RESERVE_SQL` copies resolved Verifier/subject/owner/Trigger/Operators and accepted authority; handoff reads persisted generation | `pg_github_pull_requests.rs:374-450` asserts full served PR pins, exact inline/referenced frozen bindings and zero mismatched declaring principals; `policy_reselection_affects_only_later_admissions` asserts unchanged old rows and new Repository 1.0.1 only on later source | PASS |
| A4: refusals are durable terminal state, without runnable authority | `REFUSE_SQL`; target authority refusal maps to persisted errored row and handoff ignores non-reserved rows | `revoked_accountable_principal_settles_a_durable_refusal` asserts both rows errored with `WYRD_AUTH_401_CREDENTIAL_REVOKED`, HTTP 202 and empty claim; SQL refusal test asserts status/error/pins | PASS within shared-state scope |
| A5: consequential success follows activated fanout or genuine obsolete/refusal state | `SubmissionSink::write` activates and commits before replying; `hand_off` waits all replies; failed reads propagate | Existing tests assert no 2xx during stalled Scribe ACK and no rows after failed provider read, then redelivery admits. However zero-row deactivation returns 2xx without leaving an ordering fence, allowing work to become owed afterward (I1) | FAIL |
| A6: partial fanout replay reuses canonical policy/IDs and activation is idempotent | reservation is durable before stage; `ScribeSink::frames` keys PR input frames by execution UUID; stored observation/pins rebuild same row; `ACTIVATE_SQL` requires pending/infinity | `pg_verifier_executions.rs:4511` asserts first activation returns first binding, replay returns only second, third activation empty; server `interrupted_handoff_completes_the_same_fanout_after_restart` asserts exact IDs and two due rows after recovery | PASS for canonical fanout; source ordering limitation I1 remains |
| A8: newer source cancels all older unfinished work, retaining completed history | `SUPERSEDE_SQL` clears due time and leases for pending/retrying/running; same nonterminal set as existing cancellation. But invocation is skipped when targets are absent | Lifecycle test asserts old queued/running A cancelled, late worker `StaleLease`; completed-baseline test preserves P. No empty-policy assertion; I2 is a reachable exception | FAIL |
| A9: older provider observations cannot override newer decisions / deactivation | PostgreSQL timestamp precedes read; PR advisory lock serializes decisions; `LATEST_SQL` only reads generation observation and first deactivation timestamps | Held-B-after-C test asserts B never returns; close test at `:1230` first admits A then holds an open read before close. Neither tests close before initial admission, repeated deactivation, or newer replay that inserts no generation; I1 | FAIL |
| A10: close/draft/access loss cancel and fence unfinished work while keeping history | `DEACTIVATE_SQL`, `CANCEL_REPOSITORIES_SQL`; access changes cancel in their transaction; `fence_and_release` uses pre-release host | Existing lifecycle and close test assert `pull_request_closed`, `pull_request_draft`, `repository_access_lost`, old pins preserved, late completion `StaleLease`. But a first close can be undone by an older pending read (I1) | FAIL |
| A11: bounded 41-PR admissions progress through ordinary fair queue | existing Outbox reservations and claim ordering; activation not a new queue | `burst_of_forty_one_pull_requests_admits_each_binding_once` asserts all 41 replies 202, exactly 82 rows, exactly two per distinct head, all completed and served head equal to stored head | PASS |
| A12: central queue fences protect sibling/future consumers | activation only pending/infinity; claim only due pending/retrying or expired running; complete/terminate require running and lease token; supersession uses same state fence as existing cancel | SQL and server assert `StaleLease` after supersession/close. Missing source decision in I2 means no fence exists for those consumers to inherit | FAIL |
| A16: extend existing handoff/queue; no second engine/inbox/table; preserve immutable input identity | migration adds only columns/index to verifier_executions; new Admission variant enters existing SubmissionSink; runner uses existing supplied-input/engine path | Source diff confirms no new queue/table/engine. PR frame keyed by execution ID, and replay/partial-fanout assertions above establish nonduplicate queue activation | PASS within shared-state scope |
| A17: ordinary scheduled/group/submitted paths retain owner and behavior | SubmissionSink still runs existing accept_submissions/accept_scheduled/accept_grouped; PR activation is added in same transaction; claim/settlement predicate changes are additive | Existing SQL queue and 129 server verification tests are recorded passed in task evidence; source diff keeps old branches and same reply routing. No independent rerun | PASS within shared-state scope |

## Proposed findings

### I1 — INCORRECT: successful no-work/replay decisions leave no latest-observation fence

**Obligations:** REQ-004/005, INV-003/006, A5/A9/A10; queued activation authority says an observation older than the newest accepted one or last deactivation is obsolete.

**Locations:** `crates/wyrd/wyrd-sql/src/queries/verifier_executions/pull_requests.rs:55-72,79-97`; `crates/wyrd/wyrd-server/src/components/github/pull_requests.rs:239-256,270-295`.

**Source-proven failing path:**

1. A newly opened PR has selected bindings but no execution history yet. Its opened delivery obtains observation time t1 and reads open state; hold its HTTP response using the existing provider-read control.
2. Close the PR. Its close delivery obtains t2 > t1, reads closed state, enters the PR lock and calls `deactivate_pull_request`.
3. `DEACTIVATE_SQL` only UPDATEs existing execution rows. With none, it changes zero rows. The handler commits and positively acknowledges close. No row anywhere records t2 for this PR.
4. Release the original open response. `LATEST_SQL` still returns NULL, `PullRequestHistory::obsolete(t1)` is false, and `reserve_pull_request` creates runnable work from the obsolete open state of a now-closed PR.

This is ordinary out-of-order delivery/provider response handling, specifically required by the task. The existing close test starts by admitting A and therefore has rows on which to store the deactivation; it does not falsify this path.

The same missing high-water invariant exists with history: admit A at t0; hold a closed-state read started at t1; reopen the same source and process it at t2. That newer open observation replays A without updating any timestamp. Releasing the older close at t1 passes the obsolete check (`latest_at` remains t0) and cancels current open work. Likewise repeated closed/draft observations retain only the first `pr_deactivated_at` via COALESCE, so an older open read between those decisions can revive work after the later deactivation. These are one source defect, not separate downstream guards.

**Producer and sibling consumer tracing:** only generation reservation inserts `pr_observed_at`; deactivation and repository cancellation only update existing rows and preserve first deactivation time; replay inserts/updates nothing. `lock_pull_request` derives its entire authority from these fields, and `obsolete` only compares to that derived value. Once stale rows are admitted, normal SubmissionSink activation and queue claim correctly treat them as live; neither should invent a second provider-state authority. No existing queue helper records the missing decision.

**Consequence and materiality:** 2xx-acknowledged close/draft does not prevent a delayed older request starting a new review; newer reopened work can instead be cancelled by an old response. This violates explicit lifecycle correctness and permits obsolete computation/continuation. It is not a cosmetic ordering preference.

**Required testable correction:** repair observation ordering once at the existing PR/queue owner so every successful current-state decision fences older in-flight reads, including zero-execution close/draft, replay of the same current source, and repeated deactivation. Preserve immutable accepted pins, generation IDs, canonical Scribe input bytes and no-new-table/no-inbox constraint. Do not fix this by rechecking independently in the runner or delivery consumers. At minimum use the existing held-read control to prove first-open held across close admits no rows after release; prove held closed response cannot cancel a later reopen; prove repeated deactivation does not admit an intervening older open observation. Existing canonical fanout/replay assertions must remain true. Merely overwriting the frozen generation observation time would damage canonical input replay and is insufficient.

**Correction burden:** one ordering-owner correction plus focused concurrency cases is necessary; consumer guards would duplicate source authority and leave ACK semantics wrong. No new subsystem or unrelated availability guarantee is required.

### I2 — MISSING: disabling future bindings also bypasses source supersession

**Obligations:** REQ-003 supports `verifier = []` to remove future automatic review without disconnecting; REQ-005 / INV-006 / A8/A12 require new source to supersede obsolete unfinished work. Policy selection alone must not sweep open PRs.

**Location:** `crates/wyrd/wyrd-server/src/components/github/pull_requests.rs:274-281` (producer); queue `SUPERSEDE_SQL` / `reserve_pull_request` at `verifier_executions/pull_requests.rs:102-111,564-582` (existing cancellation owner not reached).

**Source-proven failing path:**

1. Under selected policy V1 admit head A; leave one binding running and another queued/reserved.
2. Declare/select policy V2 with an empty verifier list. `settle_repository_selection` changes selected Card only, correctly leaving accepted A unchanged; old exact bindings/Verifier Cards remain active and accepted history remains present.
3. Push head B and process its eligible signed synchronize delivery. Its source differs, so `replay` is false. `targets` finds selected V2 but its `pull_request_bindings` is empty and returns None.
4. Lines 275-278 return success before `reserve_pull_request`, the only admission call that runs `SUPERSEDE_SQL`. A remains pending/running/current with its original lease. The generic claim and complete SQL have no selected-policy or GitHub-source predicate: queued A can run and running A can successfully settle, despite B superseding it.

**Sibling writers/consumers:** `settle_repository_selection` must retain its no-sweep behavior. Existing `access_lost` does cancel, but access is still connected here and no removal/suspension event occurs. Ordinary claim and lease settlement only inherit cancellation already committed by the lifecycle owner; they cannot reconstruct the missing source transition. The current policy-reselection test changes two bindings to one, so it always reaches reservation and misses zero bindings.

**Consequence and materiality:** disabling future automatic review is a documented supported workflow. A subsequent source push leaves the old source runnable, violating the explicit obsolete-work invariant and handing later continuation/delivery consumers an apparently current generation. This is required lifecycle behavior, not a demand to sweep at policy declaration.

**Required testable correction:** decide supersession of the locked PR independently of whether the selected policy creates new targets. Reuse the existing queue cancellation/fencing owner; commit cancellation of unfinished A on B's observation, admit no B executions for an empty policy, preserve completed history and exact pins, and retain no sweep at selection time. Do not create a replacement queue or query GitHub from the worker. Integrate with I1's observation fence so delayed reads cannot revive A after the empty-policy decision. Focused proof: admit queued/running A plus completed history P, select verifier=[], assert selection alone changes no executions, process B, assert A cancelled with supersession and late settlement `StaleLease`, P unchanged, and zero B executions.

**Correction burden:** remove the target-presence dependency from the existing source-transition boundary, retaining the same SQL cancellation owner. A declaration-time sweep would be smaller-looking but violates the approved policy rule; downstream worker guards would duplicate lifecycle authority and fail to fix visible state.

## Verification limits

The inspected tests substantiate healthy canonical handoff, partial fanout, ordinary supersession and cancellation. Task-recorded green commands were not independently rerun by this reviewer and do not cover either proposed failing path. The orchestrator separately reported fresh managed Postgres migrations passing and all 15 PR/connection tests passing; that evidence supports the existing cases, while I1/I2 remain source-proven paths without an executed failing reproduction in this review. No speculative finding is retained for future parked states: the schema currently permits only pending/retrying/running as nonterminal states and all three are cancelled. No Task 005 extraction, parked continuation or remote-write implementation is requested here.

Skipped full surface/security/standards acceptance and independent DB execution; those belong to the assigned reviewers and coordinated root run. Risk: existing green tests do not establish observation ordering or supersession after an empty policy.
