---
id: TASK-005A
kind: implementation
status: proposed
spec: SPEC-verification-workflow-ui
spec_revision: 7
requirements: [REQ-019, REQ-020, AC-013]
depends_on: []
---

# Complete backend admission before PR review-set UI

## Outcome and scope

Deliver the three backend inputs TASK-005 needs: an explicit Wyrd PR-wide fresh
review through HTTP, Rust/Python/TypeScript SDKs and MCP; retained membership
including refused executions; and proof that authorized existing reads supply
complete current/history membership. Approved revision 7 defines the behavior
in REQ-019/020 and AC-013. The
[agreed design and source evidence](../design/pr-review-set-backend-readiness.md)
explain the missing paths.

Extend existing admission and retention owners; do not change verification
execution or other hot paths. No GitHub Re-run hook, UI rerun button, aggregate
verdict, dedicated history API, new storage family or migration is assigned.
If existing storage/coordination cannot deliver the contract, return
SPEC_REVISION_REQUIRED with the precise missing capability before expanding scope.

## Connected implementation

Reuse the GitHub admission owner in
`crates/wyrd/wyrd-server/src/components/github/pull_requests.rs`, the execution
queue's `pull_requests.rs` in `wyrd-sql`, and existing verification input/event
Scribe handoff. Preserve their source-event replay, policy/authority pins,
connection fencing and supersession behavior.

The authenticated request supplies host, repository ID, PR number and an
idempotency key. Existing admission resolves connected access, selected policy
and current provider source under REQ-019's authorization. It returns accepted
generation/member IDs; existing verification records retain request replay
identity and complete membership with source, policy and actual member status.
TASK-005 consumes those authorized records for navigation. Private wire names,
metadata layout and query mechanics remain implementation-owned.

## Ordered scenarios and implementation evidence

Exercise AC-013 through real public operations and managed Postgres/Bifrost.
Record actual assertion locations, focused commands and RED/GREEN results here.

1. **Retain every member — REQ-020.** Admit mixed runnable/refused and all-refused
   bindings through the real PR path. Authorized retained reads include every
   member with exact generation, policy and status; refused members never run.
   Start with this reproduction: the inspected handoff skips refused inputs.

   - Proof: `every_admitted_member_is_retained_with_its_status` (`crates/wyrd/wyrd-server/tests/integration/pg_github_pull_requests.rs:2234`).
     Mixed PR 21 (deleted Verifier `z-review`) and all-refused PR 22 (suspended
     declaring editor) are read through the SDK Bifrost query as a separate
     reader. `assert_complete` (`:2206`) checks the exact member set,
     generation, selected policy UID and `members`. `await_newest_as` checks
     each refusal's `errored` status, and `claim` checks that only the
     runnable member is claimable.
   - Command [C1]: `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_LOG=info,wyrd_server=debug cargo nextest run --locked -p wyrd-server --test integration -E "test(/^pg_github_pull_requests::/)"'`.
   - RED (`f4ce139ce^` plus the test): `only 1 of 2 members retained` at the
     retained-read deadline. `hand_off` staged only reserved rows.
   - GREEN at `f4ce139ce`: `hand_off` stages refused members and their
     `errored` status. `reserve_pull_request` records `members`, and the
     inputs read as `authority IS NULL AS refused`.
   - Intended contract change: the existing pins in the lifecycle test and
     the `wyrd-sql` test `pull_request_generations_reserve_activate_and_supersede`
     (`crates/wyrd/wyrd-sql/tests/integration/pg_verifier_executions.rs:4400`)
     now expect the reserved `members`.
   - Status: PASS.
2. **Request and retry — REQ-019.** Admit two bindings and request another set at
   the same head through the new Wyrd operation. A distinct key creates a new
   generation; retries, concurrent retries and restart preserve its member IDs.
   Change policy, then advance the head and retry an older request: accepted
   sets keep their pins, supersession fences older work, and retry cannot
   reactivate it. Ordinary source-event redelivery still replays. Denied or
   ineligible requests accept no fresh work; provider/authority unavailability
   remains a typed retryable failure. Exercise the shipped SDK/MCP projections.

   - Operation: `POST /v1/integrations/github/pull-requests/reviews`
     (`GithubPullRequestReviewRequest {host, repository_id, number}` plus a
     required `Idempotency-Key`) answers 202 with
     `GithubPullRequestReviewResponse {generation, executions}`. The same
     operation is `GithubConnection::review_pull_request` (Rust),
     `review_pull_request` (Python), `reviewPullRequest` (TypeScript) and MCP
     `github_connection.review_pull_request`. It is listed for callers with
     any `verifier:run`. It authorizes `cards:read`, then `verifier:read` and
     `verifier:run` on every selected Verifier, or on every stored member for
     a key replay. Each check is audited as `github_connection.review_pull_request`.
     `PullRequestActivation` gains optional `request_key` and `members`;
     `delivery_id` is now optional. Ineligible requests answer
     `WYRD_GITHUB_409_PULL_REQUEST_NOT_REVIEWABLE` with `details.reason`
     `not_found`, `closed`, `draft` or `no_bindings`.
   - Proof: `explicit_review_requests_admit_new_generations_and_replay_by_key`
     (`crates/wyrd/wyrd-server/tests/integration/pg_github_pull_requests.rs:2406`) covers:
     - a distinct key at an unchanged head creating a new generation that
       supersedes the old one;
     - sequential, concurrent and post-restart retries returning identical
       members;
     - a policy change applying only to later keys;
     - delivery redelivery still replaying;
     - an old key retried after the head advances, returning its generation
       without reactivating it.
   - Proof: `refused_review_requests_accept_nothing` (`crates/wyrd/wyrd-server/tests/integration/pg_github_pull_requests.rs:2544`) covers:
     - a viewer refused, with no rows;
     - another host as `WYRD_GITHUB_400_INVALID_REQUEST`;
     - closed, draft, not found and no bindings as 409 with a reason, with no
       rows;
     - GitHub 502 as `WYRD_GITHUB_503_PROVIDER_UNAVAILABLE`, and a missing
       SYSTEM actor as `WYRD_SERVER_503_SERVICE_UNAVAILABLE`, both through
       raw HTTP (`review_once`) with no rows, then accepted on a retry under
       the same key.
   - Command: C1.
   - Projections:
     - MCP `an_agent_reads_the_selected_repository_policy`
       (`crates/wyrd/wyrd-mcp/tests/bifrost/mcp/github.rs:227`): listing,
       review, pins, key retry, a viewer neither offered nor granted it.
       Command: `cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E "test(/^github::pg_tests::/)"` (4/4).
     - Rust SDK `pull_requests_admit_pinned_executions_and_supersede_them`
       (`sdks/wyrd-sdk-rust/tests/integration/repository_declaration.rs:598`).
       Command: `cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(/^repository_declaration::/)"` (7/7).
     - Python `test_a_review_request_admits_a_new_generation_and_replays_by_key`
       (`sdks/wyrd-sdk-python/tests/integration/test_repository_declaration.py:219`).
       Command: `uv run python -m pytest -q tests/integration/test_repository_declaration.py -k test_a_review_request_admits_a_new_generation_and_replays_by_key` (1 passed).
     - TypeScript "a review request admits a new generation and replays by key"
       (`sdks/wyrd-sdk-ts/wyrd/tests/integration/repository-declaration.test.ts:195`).
       Command: `pnpm exec vitest run tests/integration/repository-declaration.test.ts -t "a review request admits a new generation and replays by key"` (1 passed).
     - All four ran under `with-test-postgres.sh` after `db:migrate:all:inner`.
   - TDD: **VIOLATED.** Production code for the request, route, MCP tool and
     client came before this scenario's failing tests, so no RED was
     recorded for it.
   - Diagnoses (test input only; no production change):
     1. *Symptom:* `WYRD_REGISTRY_400_IDEMPOTENCY_KEY_INVALID` instead of
        `WYRD_GITHUB_400_INVALID_REQUEST`. *Cause:* key `host-41` is under
        `IdempotencyKey`'s 8-character minimum. *Fix site:* the test input,
        now `host-key-41`. Read-only diagnostician confirmed no other key is
        affected.
     2. *Symptom:* the provider-unavailable request returned accepted.
        *Evidence:* trace shows the 503 followed about 170 ms later by
        `generation admitted`. *Cause:* `HttpTransport` retries a keyed 5xx
        once (`crates/shared/wyrd-client/src/transport/http.rs`, `send_with_retry`),
        and the one-shot mock failure was consumed. *Fix site:* the test, which
        now observes the typed answer through raw HTTP. Diagnostician
        confirmed client and server behave as designed.
     3. *Symptom:* suspending SYSTEM failed with SQLSTATE 23514. *Cause:*
        `auth_service_accounts_system_active_check` keeps SYSTEM active.
        *Fix site:* the test lifts and restores that check in its own fixture
        database to reach `AuthorityRefusal::Unavailable`.
   - Status: PASS (behavior); TDD VIOLATED.
3. **Read complete current/history membership — REQ-020.** Interrupt member
   handoff and recover. Existing authorized Bifrost/domain reads distinguish
   incomplete visibility from complete membership and resolve latest/history
   without regressing on delayed older records. Include history outside the
   default Verify range and denied/foreign callers: no unauthorized membership
   or counts appear. Do not substitute browser-authored membership or per-member
   detail fanout for the retained read input TASK-005 will consume.

   - Reader rule, proven over `bifrost.verification.execution_inputs`
     (origin `pull_request`):
     - `recorded_at` is the generation's PostgreSQL `pr_observed_at`, which
       strictly increases per PR.
     - The current set is the generation with the latest `recorded_at`.
     - A set is complete only when every execution in its `members` has a
       record.
     - `current` and `of` (`crates/wyrd/wyrd-server/tests/integration/pg_github_pull_requests.rs:2697`) apply this rule.
   - Proof: `retained_reads_resolve_current_and_history_membership`
     (`crates/wyrd/wyrd-server/tests/integration/pg_github_pull_requests.rs:2736`) covers:
     - a stalled handoff whose domain read
       (`GET /v1/verification/executions/{id}`) names both members while
       fewer than two are visible, so the set is never complete;
     - release completing that set;
     - a newer set becoming current with every earlier set still complete,
       and current resolution unchanged when records are read oldest-last;
     - a `recorded_at` bound selecting only history;
     - a caller without `bifrost_query:read` refused rows and counts with
       `WYRD_PERMISSION_403_DENIED_RBAC`, and one scoped to `bifrost.logs`
       refused with `WYRD_VALA_403_QUERY_FORBIDDEN`;
     - a foreign tenant's viewer seeing no rows and a count of 0.
   - Proof: `a_lost_review_set_superseded_before_its_retry_is_still_retained`
     (`crates/wyrd/wyrd-server/tests/integration/pg_github_pull_requests.rs:2962`): a set whose handoff is lost to a restart and superseded by
     a newer key is fully retained when its key is retried, and the newer set
     stays current.
   - **Authorization: FAIL — SPEC_REVISION_REQUIRED.** The tests prove only
     the tenant boundary and table-level refusals. Existing Bifrost reads are not enough: `bifrost.verification.execution_inputs` is gated only by table-level `bifrost_query:read`. The Oracle object check (`crates/wyrd/wyrd-server/src/query/service.rs:213`) gates the table, and the only payload gate covers gateway-call columns (`service.rs:370`). So a caller with table query permission but no `verifier:read` on a member's Verifier, or no `cards:read`, can read that member's pins and count members. REQ-020 and `architecture/bifrost-design.md` require tenant, Card, exact Verifier and sensitive-payload checks before membership or counts are disclosed. Closing this needs Verifier- and Card-scoped authorization on the retained read, either as an Oracle row policy or as a domain read of a whole set. This task excludes a dedicated history API, and per-member detail reads cannot stand in for it.
   - Command: C1, now 26 tests passing, plus `pg_github_delivery::` (38 in
     total).
   - RED (lost set): `only 3 of 4 members retained`. `hand_off` skipped
     superseded members. GREEN at `64fb741fe`: `hand_off` also restages
     cancelled members, and their activation is a no-op.
   - The retained-reads test passed when first run. Its behavior was
     delivered by scenario 1 together with the existing `recorded_at`
     mapping, so no RED was manufactured.
   - Limitation: `SubmissionSink::admission_lane` puts every PR admission in
     lane 0, so one process acknowledges a tenant's PR inputs in arrival
     order. An older set cannot land after a newer one in-process. Non-regression
     is therefore proven by the ordering key, the reversed read and the lost
     set's late retry, not by a cross-replica race.
   - Diagnoses:
     1. *Symptom:* the newer review timed out while an older write was held.
        *Cause:* `admission_lane` returns lane 0 for `Admission::PullRequest`
        (`crates/wyrd/wyrd-server/src/verification/submissions.rs:176`), and
        the outbox allows one in-flight write per lane. *Fix site:* the test
        sequence. Lane selection is outside this task, and the read-only
        diagnostician advised no server change here.
     2. *Symptom:* a no-query reader got `WYRD_PERMISSION_403_DENIED_RBAC`,
        not `WYRD_VALA_403_QUERY_FORBIDDEN`. *Cause:* `/v1/query` admits on
        the coarse capability first (`crates/wyrd/wyrd-server/src/query/service.rs:213`).
        *Fix site:* the test, which now covers both refusals. Confirmed by a
        diagnostician.
     3. *Symptom:* the lost-set test hung at `held.wait_entered()`. *Cause:*
        its raw request sent `Authorization: Bearer` rather than
        `X-Wyrd-Access-Token`
        (`crates/wyrd/wyrd-server/src/components/auth/token_extract.rs:46`),
        so it got 401 before admission. *Fix site:* the test header.
        Diagnosed by a read-only Codex diagnostician.
   - Status: retention, completeness, latest, history and tenant isolation
     PASS. Exact Verifier and Card authorization of the retained read FAIL
     (SPEC_REVISION_REQUIRED).

## Verification and handoff

Reuse `crates/wyrd/wyrd-server/tests/integration/pg_github_pull_requests.rs`,
`GithubAppMock`, `WyrdTestServer` and existing SDK/MCP journey owners. Record new
focused selectors before RED. Run the affected admission slice through the
managed Postgres/migration setup used by `mise run test:wyrd`; add
`mise run test:sql` for changed SQL behavior. Run `mise run fmt`, `mise run lints`,
`mise run check:tenant-isolation` and applicable contract/codegen checks.

Evidence: `mise run fmt`, `mise run lints`, `mise run check:tenant-isolation`,
`mise run codegen:check`, `mise run docs:check`, `git diff --check`,
`ts:build`/`ts:napi:check` (after committing `index.d.ts`), `ts:typecheck`,
`ts:lints`, `ts:format:check`, `py:format:check`, `py:lints` and
`py:typecheck` all pass. The `wyrd-sql` `pg_verifier_executions` slice passes
41/41.

`py:typecheck` was also red in
`tests/integration/test_verification_capacity.py`, where ty drops the
`isinstance` narrowing after a `pytest.raises` block. A declared local fixes
it; that test passes.

Non-goals are unchanged: no migration, no architecture edit, no GitHub
Re-run hook, no UI, no aggregate verdict, no history endpoint and no new
storage. Commits: `f4ce139ce`, `f591bb541`, `33ca54865`, `84e64a174` and
`64fb741fe`.

Record the delivered operation, integrated revision and connected journey
results here and in TASK-005 before activating it. Task status alone is not
proof that its backend inputs are available.
