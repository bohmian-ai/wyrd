---
id: TASK-006
status: complete
approved_spec_revision: 5
requirements: [REQ-004, REQ-005, REQ-015, REQ-016]
acceptance: [AC-003, AC-011]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007]
depends_on: [TASK-001, TASK-002, TASK-003]
---

# Admit PR generations through the verification queue

Completed following the [PASS verdict](../review/006-pr-admission-r2/verdict.md) for `fb8da5ad7e87f784cde39513cb8605dba8bbf1a2`. FIND-006-1 through FIND-006-7 are closed.

Implement **approved-runtime-and-github.md → Webhook ingress** and
**approved-code-review-support.md → Successive commits and pipeline ownership**.
Use **bursts-and-successive-commits.md** for the fair-queue/burst rationale,
not as authority for a new SLA or cache subsystem.

## Actual sources and complete owner workflow

Input is a signature-verified provider PR/source delivery for task 002's connected,
selected repository. Resolve its stable host/repository identity to task 003's
selected registered Repository and normal PR bindings. Provider delivery ID
identifies replay; PR base/head/source identity and current provider state identify
the source generation. Do not read TOML/policy from the PR head. Current tenant
execution authority is derived and constrained through existing authority owners,
not saved personal OAuth credentials or provider metadata.

Existing `verification/submissions.rs::SubmissionSink` reserves bounded staging,
accepts Scribe-acknowledged inputs, batches per tenant into
`VerifierExecutionQueue::accept_submissions`, and answers AdmissionReply only
after commit. `runner.rs` then claims fairly and loads the same engine, with
Bifrost RunState recovery. Extend this complete pipeline for frozen PR fanout;
do not build a webhook engine, inbox table, second queue or detached finalizer.

Freeze Repository, bound Verifier/Trigger, authority, source repository/PR/base/head
and reviewed source identity before durable handoff. Stage a stable canonical
Scribe batch with stable internal delivery/execution identities for its frozen
binding fanout. Redelivery/recovery reuses the first accepted batch/policy and
settles each binding exactly once, even when only part reached queue commit.
An in-process reservation is not durable acceptance. Return positive webhook
receipt only after required admissions commit, or durable obsolete/refusal
settlement establishes no work owed; before that, failure/no answer must remain
retryable. Do not silently resample a newer policy on retry.

GitHub does not automatically redeliver failed deliveries: extend the same App
maintenance/connection owner for bounded failed-delivery recovery and progress
using existing control state. Recovery must rediscover the original canonical
handoff and queue identities, without a new webhook inbox or generic broker.

## Lifecycle, cancellation and handoff

Open/reopen/ready and actual head/base changes admit non-draft PRs; metadata-only
edits do not. Draft conversion, close, repository removal, suspension/disconnect
cancel affected unfinished queued/running/parked work, preserving history. Policy
selection changes only future admissions, with no sweep of all open PRs.

Confirm current provider PR source outside webhook DAG computation before old
out-of-order source can become current. New head supersedes obsolete unfinished
work: coalesce replaced queued heads and fence running/parked generations before
any new continuation or current delivery. Keep original accepted pins/history;
P completed, A running, B pushed cancels A and leaves P as usable baseline for
B. Task 005 performs that baseline selection, rather than admitting from A's
partial output. Already-sent remote IO may finish; retain pins and reconcile,
never promise cross-provider atomic cancellation.

Output is an ordinary durable ExecutionId per frozen binding with accepted PR
activation/generation, visible execution/binding status and cancellation fences.
Task 005 reads the pins, tasks 007/008 test them before continuation, and task 009
rechecks them before each provider write. Centralize these checks at the existing
queue/authority owners so disconnect and supersession cover later wait/delivery
extensions too. Public inspection/CLI/SDK/MCP projection of PR admission and
supersession belongs here, including guide to event eligibility/diagnosis.

Amend **wyrd-design.md → Trigger / Verifier queued activation** to authorize closed
pull_request (inline and referenced Trigger) and exact source/policy ownership;
remove the contradictory blanket external-push exclusion only for this agreed
activation. Document complete consequential ACK and failed-delivery recovery at
existing execution/operations owners. Keep ordinary scheduled/group submissions.

## Proof

**AC-003:** Through the real connect→declare outputs from 002/003, send 41 eligible
PRs under bounded ingress and observe committed recoverable executions, fair
queue progression and typed status. Duplicate a delivery, interrupt partial
binding fanout at canonical ACK/queue commit and redeliver after restart; assert
same frozen policy/IDs and complete fanout, not duplicate runs. Send delayed old
source events after B, replace queued B by C, supersede running A, and apply
draft/close/access loss; assert stale work cannot become current and accepted
history stays inspectable. Before task 005 lands use an ordinary declared queued
Verifier for admission proof, not a fabricated binding; task 005 supplies the
actual extraction consumer seam. Tasks 007/008/009 extend this same source journey
to parked continuation and remote write races.

**AC-011 (admission):** Public inspection and docs explain eligibility, accepted
pins and cancellation without a hidden orchestrator or UI.

Tier 1: `wyrd-testing` existing server verification runtime owner and all shipped
SDK/MCP journey surfaces (README runners), local GitHub protocol transport and
real server/Postgres. Tier 2: SQL `integration` queue and server route owners for
conditional fanout/generation/crash handoffs via `mise run test:sql` and affected
`mise run test:wyrd`; tier 3: signature/eligibility decision branches. Record
focused commands for new tests after creation. Use existing deterministic crash
and execution controls where applicable; no synthetic host-load benchmark or
arbitrary completion-time target. Run README boundary/contract checks.

## Obligations

| ID | Source | Observation | Planned assertion |
|---|---|---|---|
| A1 | REQ-004; INV-001; AC-003; ¶Actual sources 1 | A signature-verified `pull_request` delivery for a repository the tenant connected (through the real connect flow) and selected resolves, by host and repository identity, to the selected Repository and its `pull_request` bindings; unsigned or mis-signed deliveries, unconnected installations and unselected repositories admit nothing; policy never comes from the PR head | real-flow connection; one execution per binding; 401 for bad signatures; no rows for an unconnected installation or unselected repository; frozen policy is the selected Card version |
| A2 | REQ-004; INV-005; ¶Actual sources 3; ¶Lifecycle 4 | Each execution freezes the Repository version, the bound Verifier and its inline or referenced Trigger, its authority, and the PR's host, repository, number, base and head ref and commit, and head repository before handoff; a later policy selection affects only later generations and sweeps no open PR | served pins equal GitHub's source; frozen Verifier, Repository version, Trigger form and declaring principal per binding; re-declare keeps open rows; the next generation freezes the new version |
| A3 | REQ-004; AC-003; ¶Lifecycle 1 | Open, reopen, ready and actual head or base changes admit non-draft PRs; drafts and metadata-only edits do not | unit eligibility branches; base-change edit supersedes and pins the new base; draft and metadata edits admit nothing |
| A4 | REQ-004; INV-001; ¶Actual sources 1 | Authority is the selecting declaration's principal constrained by current permission; a refusal settles durably as `errored` with the refusal code, answers 2xx and leaves no runnable work | suspended declaring principal: errored rows with `WYRD_AUTH_401_CREDENTIAL_REVOKED`, 202, empty claim |
| A5 | INV-003; ¶Actual sources 3 | A 2xx answer follows only committed admissions or durable obsolete/refusal settlement; any failure before that (Scribe ACK, GitHub read, authority store, staging) is not acknowledged and stays retryable | rows are due when 202 arrives; a stalled Scribe ACK across a restart and a failed GitHub read are not answered 2xx and a redelivery admits; obsolete and refusal answers 202 |
| A6 | INV-003; INV-004; AC-003; ¶Actual sources 3 | Duplicate deliveries, redeliveries after a policy re-selection, and recovery after a restart that interrupted a partly committed fanout reuse the first batch's frozen policy and execution IDs and settle each binding exactly once; if the source moved on meanwhile, recovery admits the current source and cancels the interrupted generation | partial activation, Scribe stall, restart, recovery: same IDs, each binding due once; with a new head: interrupted rows superseded, new rows due; duplicate and post-re-declare redeliveries change nothing |
| A7 | INV-003; ¶Actual sources 4 | The GitHub App owner recovers failed deliveries with a bounded, paginated scan of its delivery log, redelivering only those without a 2xx attempt and stopping at the redelivery bound, with no inbox or broker | only the unanswered delivery is redelivered; an answered one never; the third redelivery is the last; next-page and API-origin unit branches |
| A8 | REQ-005; INV-006; AC-003; ¶Lifecycle 2 | A new head or base cancels every unfinished older generation, fences its running leases and keeps pins and history; P completed, A running, B pushed cancels A and leaves P completed; the unfinished set is every non-terminal state, so parked work added later is covered | queued B replaced by C; running A's late settlement is `StaleLease`; P stays completed |
| A9 | REQ-005; INV-006; ¶Lifecycle 2 | A delayed observation of an older source cannot become current: the provider is read after the observation time and an observation not newer than the last accepted one or the last deactivation is obsolete | held read of B released after C admits nothing; SQL obsolete and post-deactivation branches |
| A10 | REQ-004; INV-006; ¶Lifecycle 1 | Draft conversion, close, repository removal, suspension, uninstall and disconnect cancel the affected unfinished queued and running work with a stable code and keep history | each event ends unfinished rows with its code; a running execution closed under its worker is fenced; earlier settled rows unchanged |
| A11 | REQ-015; AC-003 | Forty-one eligible PRs under the existing bounded ingress each commit one recoverable execution per binding that progresses through the existing fair queue to a typed terminal status | 41 concurrent deliveries: 82 rows, each served `completed` with its head; the shared staging bound refuses before staging |
| A12 | INV-006; ¶Lifecycle 2–3 | Pins and cancellation fences are held at the existing queue and authority owners, so later baseline, continuation and provider-write consumers inherit supersession and disconnect | source review of the queue-owned path; fenced late settlement |
| A13 | REQ-016; AC-011; ¶Lifecycle 3 | Execution inspection projects PR activation (repository, number, base and head ref and commit, head repository, delivery) and cancellation codes on HTTP, MCP, Rust, Python and TypeScript SDKs and CLI; binding status (newest execution) is readable over HTTP and MCP; the MCP guide states eligibility and diagnosis | each surface reads the pins and cancellation code of the execution named by the binding; guide content review |
| A14 | AC-011 | Docs explain eligibility, accepted pins, cancellation and diagnosis without a UI, consistently with the implementation | content review against tests; `docs:check` |
| A15 | ¶Lifecycle 4 | `wyrd-design.md` authorizes `pull_request` activation through inline and referenced Triggers with exact source and policy ownership and removes the external-push exclusion only for it; ACK and failed-delivery recovery are documented at the execution and operations owners without promising cross-provider atomic cancellation | content review |
| A16 | INV-002; INV-007; ¶Actual sources 2 | No new table, queue, engine, inbox or broker; inputs go through the existing Scribe hand-off; tenant isolation holds; no credential is served | migration review; `check:deps`, `check:tenant-isolation`; served status carries no mock secret |
| A17 | ¶Lifecycle 4 | Ordinary scheduled, group and submitted executions keep their behavior | existing verification route and SQL queue suites |
| A18 | INV-001; ¶Actual sources 1 | A delivery admits only into the tenant connected to its installation, never another tenant reaching the same repository | two connected tenants: each installation's delivery writes rows only in its own tenant |

## Evidence

Commits `115fcef0f..HEAD` on `task/github-verifier-pipelines/TASK-004-pinned-pr-preparation`.

### Process exceptions and RED

- **Unexplained run.** One combined run (migrate output silenced, then server, SDK, CLI, MCP, Python and TS selectors) failed every Postgres-backed test in under 0.3s, MCP in 0.014s and pytest at setup. The pattern points to an unmigrated database, but the migrate output was discarded. Two reruns with migration output visible passed every selector (15/15 server, each journey 1/1), so no code changed for it.
- **Checker remediation.** The checker's added cases (unconnected installation, full pins, frozen Verifier and Trigger, base change, refusal code, recovery bound, partial fanout, uninstall, no credentials, page links) passed on first run against existing behavior; the refusal-code assertion first ran against a placeholder to read the real code. The second checker's cases (failed GitHub read via a new `fail_next_pull_request_read` mock control, close under a running worker, observation before close, recovery after a new head, two-tenant routing, frozen authority, head ref on every surface) also passed on first run.

- **Production before test.** I wrote the queue reservation, activation and supersession work (`115fcef0f`) and the GitHub admission owner, failed-delivery recovery and mock (`03c909780`) before their tests. Their first runs carry no RED. The SQL tests passed on first run. The server Tier 2 tests' first failures were test-setup defects (wrong suspension table; NULL `next_attempt_at` on settled rows; a lost-answer test that redelivered with a 2xx first), not missing behavior.
- **RED, disconnect cancellation.** `pg_github_pull_requests::suspension_disconnect_and_ineligible_pull_requests_owe_no_work` failed: `left: [("pending", None, "b2"), ("pending", None, "b2")]`, `right: [("cancelled", Some("repository_access_lost"), "b2"), …]`.
  - Diagnosis: `RELEASE_SQL` (`wyrd-sql/src/queries/github_installations.rs:107`) sets `host = NULL` and `RETURNING` yields the post-update row, so `fence_and_release` never called `access_lost`.
  - Fix site: `GithubConnections::fence_and_release` (`github/service.rs:368`) now takes the host from the locked pre-release row. Its only callers are disconnect and the uninstall path, and both pass (`pg_github_connection::*` 4/4).
- **Mechanical deviations.**
  - A6: the crash boundary uses the existing `stall_next_bifrost_write_into` + `restart_bound` controls, with one binding activated through the queue before the stall to model a partly committed fanout.
  - A11: the 41-PR proof runs at Tier 2 with the verification runtime on. The SDK journeys prove each surface's projection with one PR.

### Obligations

Paths: server `pg_github_pull_requests.rs` is `crates/wyrd/wyrd-server/tests/integration/pg_github_pull_requests.rs`; SQL `pg_verifier_executions.rs` is `crates/wyrd/wyrd-sql/tests/integration/pg_verifier_executions.rs`. "Server lane" is `scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(/pg_github_pull_requests/) | test(/pg_github_connection/)'`, 15/15 passed.

| Obligation and required cases | Implementation evidence | Proof location | Verification evidence | Result |
|---|---|---|---|---|
| A1: real connect, signed selected delivery → selected bindings; bad signature, unconnected, unselected admit nothing; no head policy | `github/service.rs:631` signature before dispatch; `github/pull_requests.rs:213` installation token for the connected slot, `:224` the only GitHub read is the PR itself (no contents read, so no head policy), `:314` `selected_repository` → `pull_request_bindings` | `pg_github_pull_requests.rs:324` `install` (GitHub install page); `:1089` 401 for None/`sha256=00`, installation 999 and repository 4343 admit nothing; `:374` one row per binding frozen to the selected Repository 1.0.0 | server lane; source review of `:188-312` | PASS |
| A2: Repository, Verifier, inline/referenced Trigger, authority and full PR source frozen; reselection only for later generations | `verifier_executions/pull_requests.rs:564` `reserve_pull_request` | `:374` served `pull_request` equals host, 4242, 7, base main/b1, head feature/a1, head repo 4242, delivery; `frozen` gives a-review inline and z-review referenced Trigger; every row's `sub_principal_id` is the selecting declaration's; `:1009` re-declare keeps open rows, next generation freezes a-review under 1.0.1 | server lane; `mise run test:sql` (230+7+88+3) | PASS |
| A3: open/reopen/ready/head/base change admit; draft and metadata edit do not | `github/pull_requests.rs:103` `observable`, `Observed::of` | unit `pull_requests.rs:568,605`; `:1009` base edit to `release` supersedes b2 and pins `release`; `:374` metadata edit and draft sync admit nothing, ready and reopen admit; `:1089` draft open admits nothing | `nextest -p wyrd-server --lib -E 'test(/github::/)'`; server lane | PASS |
| A4: declaring principal's authority; refusal errored with code, 2xx, nothing runnable | `verification/authority.rs` `accountable`; per-binding `audit::authorize` (`github/pull_requests.rs:314`) | `:886` both rows `errored` with `WYRD_AUTH_401_CREDENTIAL_REVOKED`, 202, empty claim; SQL `pg_verifier_executions.rs:4653` | server lane; test:sql | PASS |
| A5: 2xx only after commit or durable settlement; failures unacknowledged and retryable | `verification/submissions.rs` activates before commit, replies after; every admission error propagates to the webhook as non-2xx (`github/pull_requests.rs:224` read, `:346` authority store `ServiceUnavailable`) | `:374` rows due when 202 arrives, obsolete read answered 202; `:811` stalled Scribe ACK across restart never 202; `:1230` failed GitHub read not 2xx, no rows, redelivery admits; `:886` refusal 202; staging bound: `pg_verification_routes.rs:536` (shared `SubmissionSink`) | server lane; `pg_verification_*` (129 passed) | PASS |
| A6: duplicates, redeliveries and post-restart recovery reuse first policy and IDs; moved source admits current | ExecutionInputs keyed by `execution_id`; idempotent `activate_pull_requests` (`verifier_executions/pull_requests.rs:655`); recovery is an ordinary redelivery | `:811` one binding activated before the stall, restart, recovery: same IDs, both due once; `:845` head moved before recovery: interrupted IDs `pull_request_superseded`, a2 rows due; `:374` duplicate and manual redelivery leave rows identical; `:1009` redelivery after re-declare keeps first IDs; SQL `:4511` | server lane; `nextest -p wyrd-sql --test integration -E 'test(/pull_request_/)'` 2/2 | PASS |
| A7: bounded, paginated scan; only deliveries without 2xx; stops at bound | `github/pull_requests.rs:493` `recover_deliveries` (`MAX_REDELIVERIES`); `app.rs:317` page loop capped by `MAX_PAGES` (`:34`), `next_page`, `same_api` | `:374` lost answer → 1, answered delivery never redelivered, 0 after success, then 1, 1, 0 after three redeliveries; unit `app.rs:670` (rel=next only), `:694` (foreign origin, scheme, path refused); page cap by source review | server lane; `nextest -p wyrd-server --lib -E 'test(/github::app::tests/)'` 2/2 | PASS |
| A8: new head/base cancels older unfinished work, fences running, keeps history; P/A/B; unfinished set complete | supersede in the reservation transaction (`verifier_executions/pull_requests.rs:75-133`) over `pending, retrying, running`, every non-terminal state the schema CHECK allows | `:937` P completed kept, A superseded, late completion `StaleLease`; `:374` running A `terminate` → `StaleLease`, queued B replaced by C, A's pins served | server lane | PASS |
| A9: delayed older observation cannot become current, before a newer generation or a deactivation | observation time before the GitHub read; obsolete check under the per-PR lock (`verifier_executions/pull_requests.rs:402`) | `:374` held read of B released after C → 202, no rows; `:1230` open state read before a close released after it → 202, no rows; SQL `:4511` older-than-newest branch | server lane; test:sql | PASS |
| A10: draft, close, removal, suspension, uninstall, disconnect cancel queued and running work, history kept | `DEACTIVATE_SQL` and repository cancel clear leases of `pending, retrying, running` (`verifier_executions/pull_requests.rs:79-122`); `access_lost` from removal, suspension and `fence_and_release` | `:374` draft, close, `repositories_removed`, history kept; `:1230` close fences a running execution (`StaleLease`); `:1089` suspend, disconnect; `:1200` uninstall; CLI `github_journey.rs:351` disconnect | server lane; CLI journey | PASS |
| A11: 41 PRs under bounded ingress, one recoverable execution per binding, fair progression, typed status | existing `SubmissionSink` bound and runner; claim orders by due time (`verifier_executions.rs:428`) | `:678` 41 concurrent deliveries → 82 rows, each served `completed` with its head; staging bound `pg_verification_routes.rs:536` | server lane | PASS |
| A12: pins and fences at existing queue and authority owners for later consumers | every supersede, deactivate and access-loss cancel is a `VerifierExecutionQueue` method that clears leases like `cancel`; pins in `verifier_executions.pull_request` | `verifier_executions/pull_requests.rs:75-655`; fenced settlement at `:374`, `:937`, `:1230` | source review; server lane | PASS |
| A13: PR activation and codes on HTTP, MCP, Rust, Python, TS, CLI; binding status on HTTP and MCP; guide | `wyrd-spec` `PullRequestActivation`; generated `card-types.ts`; authored `.pyi`; `mcp/guide.rs:190`; docs name `verification.get_binding` / `GET /v1/verification/bindings/{id}` for the newest execution | Rust `repository_declaration.rs:594`; Python `test_repository_declaration.py:179`; TS `repository-declaration.test.ts:152`; MCP `github.rs:215` (binding through `verification.get_binding`); CLI `github_journey.rs:180`; HTTP `:374` — each asserts number, repository, base, head ref and commit, head repository, non-empty delivery and cancellation code | Rust journey 1/1; pytest 1 passed; vitest 1 passed; MCP 1/1; CLI 1/1; `codegen:check` | PASS |
| A14: docs explain eligibility, pins, cancellation, diagnosis, consistent with behavior | `declare-a-repository.md:70` §5 (binding read, base-branch supersession, uninstall cause, per-replica recovery bound, redelivery reuse only for the same source, closed or draft admits nothing), If-it-fails row; `how-verification-works.md` Bindings | content review against A3, A6, A7, A10 tests | `mise run docs:check` passed | PASS |
| A15: design and operations amendments | `architecture/wyrd-design.md:2225` (observed and deactivating actions, inline and referenced Trigger, access-loss causes, redelivery qualification, push exclusion narrowed); `architecture/operations/reliability-and-recovery.md:285`; no atomic cross-provider cancellation is promised | content review | content review | PASS |
| A16: no new table, queue, engine, inbox or broker; isolation; no credential served | migration adds columns and an index to `wyrd.verifier_executions` only (`20261114000000_pull_request_admission.sql`); inputs via existing Scribe `ExecutionInputs` | `:374` served status contains none of `mock.secrets()` | `check:deps`, `check:tenant-isolation`, `check:examples`, `lints` passed | PASS |
| A17: scheduled, group and submitted executions unchanged | `BindingActivation` keeps schedule and group-end columns; generic `CANCEL_SQL` unchanged | `pg_card_declaration_route`, `pg_verification_*` server modules; SQL queue suite | 129 server tests passed; test:sql passed | PASS |
| A18: delivery admits only into its connected tenant | slot lookup by host and installation; rows written through that tenant's `TenantConn` | `:1301` tenant B connected to installation 202 reaching 4242: its delivery admits nothing in A; A's delivery admits 2 rows in A and none in B | server lane | PASS |
Also run: `mise run fmt`, `mise run lints`, `git diff --check`, `scripts/postgres/with-test-postgres.sh -- … nextest -p wyrd-server --lib` 602/602 including the two new `github::app` tests, and `pg_github_connection` 4/4. Non-goals stayed out: no task 005 extraction, no parked continuation, no remote check writes, no UI.
