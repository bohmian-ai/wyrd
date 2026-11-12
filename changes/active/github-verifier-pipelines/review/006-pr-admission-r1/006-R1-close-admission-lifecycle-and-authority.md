---
id: TASK-006-R1
kind: remediation
status: ready
spec: SPEC-github-verifier-pipelines
spec_revision: 5
parent_task: TASK-006
remediates: [FIND-006-1, FIND-006-2, FIND-006-3, FIND-006-4, FIND-006-5, FIND-006-6, FIND-006-7]
---

# Close PR admission lifecycle and authority gaps

Route directly to `$wyrd-implement`. Implement the seven independently validated findings without changing approved behavior or adding a new subsystem.

## Authority and immutable review context

- Approved spec path: `changes/active/github-verifier-pipelines/spec.md`, **revision 5 approved**, available as `git show 0f08e6c9f:changes/active/github-verifier-pipelines/spec.md`. Current draft revision 6 does not govern this task; the task index retains revision 5.
- Original task: `changes/active/github-verifier-pipelines/tasks/006-pr-admission-and-supersession.md`.
- Original base: `2165fe606efe498db4e0455866d29236e85d889c`.
- Reviewed candidate / remediation starting point: `efda8cb2c59c8206b52dbe1d92fcae1ba34be430`.
- [Verdict](verdict.md), [independently validated ledger](findings-validation.md), [review scope](scope.md), and [verification evidence](verification.md) retain acceptance context.

The governing owners are root/nested AGENTS.md, the Trigger/queued activation and inspection sections of `architecture/wyrd-design.md`, execution authorization/GitHub association in `architecture/wyrd-security-posture.md`, and existing operations shutdown/drain authority. Read the corresponding topic guides. This packet selects corrections within that authority; it does not authorize new product, public API, permission or persistence decisions.

## Diagnosis and selected correction

### FIND-006-1: ordering is lost when an accepted observation writes no generation

**Current failure.** Server `components/github/pull_requests.rs:207–295` takes and commits PostgreSQL time, reads GitHub, and only then locks the PR. SQL `queries/verifier_executions/pull_requests.rs:55–97,329–344,383–455,493–515` derives the latest decision from execution creation and first-deactivation timestamps. A replay writes nothing; first close/draft with no rows writes nothing; repeated deactivation does not advance its first timestamp. A held first open can therefore admit after a later acknowledged close; a held intermediate B can replace A after a newer replay of A; an intervening open can survive a newer repeated draft decision. These violate A9/A10, INV-006 and the explicit newest-observation contract.

**Correction.** Reuse the existing per-PR transaction advisory lock to serialize the bounded provider observation and decision. Mint the installation token before acquiring that transaction. Acquire the PR lock before taking observation time and reading the provider PR; retain it through decision/commit. Hold no installation-slot or execution-row locks during provider IO. After the read, validate current slot authority as FIND-006-4 requires, refresh history under the same PR lock, then decide and commit. Release the transaction before Scribe staging or admission-reply waits.

This protects even zero-history decisions without new durable state. Keep original frozen timestamps, IDs, source pins and canonical Scribe bytes immutable. Advancing timestamps on execution rows cannot solve the zero-row case; rewriting canonical time would break replay. A map, sentinel execution, new table/inbox or downstream source guards are unnecessary. The meaningful tradeoff is holding one tenant connection and same-PR advisory lock across one existing deadline-bounded GET; other PRs retain distinct lock keys, and token minting/handoff stay outside it.

**Proof.** Use existing held-read controls to prove serialized same-PR observation/commit and no stale admission after newer close/draft/replay/repeated deactivation. Preserve generation, cancellation and stale-lease assertions. The two existing held-read sequences in `pg_github_pull_requests.rs:568` and `:1237` await the second webhook before releasing the first response; adapt their synchronization to launch the second concurrently and release/join in serialized order. Otherwise the corrected lock lifetime would deadlock the old test choreography. This adaptation follows the diagnosed lock lifetime; it must not weaken semantic assertions or add sleeps/timeouts to hide failure. Recheck canonical partial-handoff recovery and exact original IDs/pins.

### FIND-006-2: no replacement bindings incorrectly means no supersession

**Current failure.** In server `pull_requests.rs:274–278,319–331`, a newly selected empty policy causes targets to return no work before queue supersession at SQL `pull_requests.rs:102–113,564–643`. Supported flow: accept queued/running A, select `verifier=[]` without a sweep, then push B. A retains due work and a valid lease and can settle as current. This violates REQ-003/005 and A8/A12.

**Correction.** Make the existing queue-owned source transition independent of replacement target count. On B, cancel/fence unfinished A with `pull_request_superseded` even when B has no targets; create no replacement execution. Use existing deactivation metadata to mark the old current generation inactive for replay when no replacement is stored, preserving its frozen pins/time, settled outcomes and supersession code. Otherwise a later return to A can reuse cancelled A as though active. Integrate with FIND-006-1 serialization. Keep declaration selection itself free of sweeps; add no worker-side policy check or fabricated execution.

**Proof.** Keep completed P, queued/running A; select an empty policy and assert selection leaves rows unchanged; observe B and assert A cancelled, late settlement StaleLease, P/history unchanged and zero B executions. Re-enable/reopen legitimately and prove inactive cancelled A is not replayed as current. The correction is a local queue transition, rather than a new policy workflow.

### FIND-006-3: rate-limited PR reads are permanently acknowledged as no work

**Current failure.** `github/app.rs:439–463` maps every 403 to absence; `pull_requests.rs:224–230` returns success and webhook answers 202. Recovery `:493–522` excludes any delivery with a 2xx attempt. GitHub documents 403 for primary/secondary rate limits, so required admission or close/draft cancellation can be silently lost. Existing failed-read proof uses 502. This violates A5/A7 and INV-003.

**Correction.** Remove 403 from absence classification and use existing decode/unavailable error handling. Preserve separate existing 404 behavior. No new rate-limit parser, retry subsystem or refusal policy is needed; consequential non-2xx leaves existing redelivery/recovery responsible.

**Proof.** A protocol-mocked rate-limited 403 on PR GET yields non-2xx and no new fanout; a successful redelivery creates exactly one expected frozen fanout. The shared reader also covers deactivation and interrupted recovery, so do not add separate downstream fixes.

### FIND-006-4: stale access can create work after disconnect/removal/suspension

**Current failure.** Token minting rechecks connection revision before PR GET, but server `github/pull_requests.rs:213–295` never validates installation access at its final write. Hold first open GET; disconnect/removal/suspension commits cancellation over no rows; release GET. The handler reserves and activates under the still-registered policy after access loss returned success. Existing cancellation and principal fencing cannot detect this new reservation. Access writers are `github/service.rs:368–407,623–705,721–760,876–919` and the installation SQL owner. This violates REQ-001/004, INV-001/006 and A1/A10/A12.

**Correction.** Carry the exact existing `GithubSlotBinding` identity/revision associated with token mint/read, plus the signed routed installation identity. At the final admission transaction, lock and validate host/installation, binding/revision, repository selection and unsuspended status through commit. A replaced/reconnected slot must not adopt a delayed delivery routed for the old association. Reuse existing connection-changed retryable errors or confirmed no-access outcome; uncertainty admits no work. Retain the token method's public SecretString-returning API and mint-time fence; propagate captured binding internally without duplicating minting or authority resolution.

Required lock order with FIND-006-1: **PR advisory lock → bounded provider GET without slot/execution-row locks → slot lock/check → fresh history and execution writes → commit → Scribe handoff**. Access mutations already take slot then execution rows and never the PR advisory lock. Do no execution-row locking/writing before slot validation, and hold no slot lock across provider IO. Loss during GET commits and the final check refuses; loss after reservation commit cancels rows and existing activation predicates cannot revive them. This is one creating-boundary authority check, not repeated runner/result guards.

**Proof.** Hold first open GET, commit access loss, release, assert no new claimable work. Cover disconnect and shared removal/suspension authority; include replacement/reconnect so old delayed work cannot transfer. Preserve reservation-before-loss cancellation and prove subsequent activation cannot revive it. Reuse existing mock and connection controls.

### FIND-006-5: PR inspection lacks current repository-read authorization

**Current failure.** Shared `components/verification/service/inspection.rs:329–376` checks exact verifier:read only. `service.rs:259–305` now exposes PR repository/base/head metadata. Security posture `:254–258` requires repository read in addition; the existing feature gate is cards:read, used by binding inspection. A verifier-only caller can read PR state and sibling retained inspection. Invocation/publication listing and publication selection/download/content share retained through accepted. IDs confer no permission.

**Correction.** Apply and audit existing cards:read once at shared retained inspection for PR executions, alongside exact verifier:read. Determine applicability from immutable queue PR origin/pins, or accepted input `InputOrigin::PullRequest` when the queue row is absent, using the existing SYSTEM reader/InputResolver and recorded owner/subject audit context. Do not retrieve source Cards, invent permissions/public metadata or add transport-specific checks. Missing required retained applicability evidence must preserve existing unavailability, rather than default to non-PR. Keep ordinary non-PR verifier-only reads and the separate cancellation authorization contract unchanged.

**Proof.** Deny a verifier-only caller for PR execution status and one representative sibling read; adding cards:read permits typed inspection. Exercise retained PR state without a queue row using the existing retained-read fixture. Preserve ordinary non-PR read behavior and normal audit decisions. The shared gate is required security, and avoids duplicated HTTP/SDK/CLI/MCP checks.

### FIND-006-6: recovery IO can consume the shared shutdown drain budget

**Current failure.** `github/pull_requests.rs:527–543` polls shutdown only outside an awaited recovery tick. Page listing and sequential redelivery continue during cancellation. Several bounded slow requests can exceed the shared process deadline; supervisor waits at `app/supervise.rs:298–316`, before producer/Scribe/Bifrost drains in `app/server.rs:800–828,878–895`. A provider backlog during replacement can therefore suppress unrelated accepted-write drains. This is a concrete lifecycle regression, not a new SLA.

**Correction.** Make in-progress recovery selectable against shutdown in the existing worker, following the nearby Scribe publication worker. Drop maintenance when shutdown wins and stop initiating subsequent requests. GitHub's delivery log remains progress authority; already-sent IO may complete and existing scan/idempotency handles it. Keep errors local and preserve existing shutdown ordering; add no queue, elected scanner or drain-budget extension.

**Proof.** Hold a listing/redelivery response, cancel, and assert the worker joins without releasing that response or starting later redelivery. Use existing deterministic controls, extending the mock minimally only if required. Existing shared drain tracing is in validation; if exercising server shutdown, preserve the flush of an ordinary accepted Scribe write. No load benchmark or whole-journey run is needed.

### FIND-006-7: changed-item panic contracts fail the required rustdoc gate

**Current failure.** Root AGENTS and `agent_docs/rust.md` expressly require Panics docs on changed items, including tests/helpers. Verified omissions:

- Server `components/github/app.rs:670,694`: new assertion/expect tests.
- Server `components/github/pull_requests.rs:552`: event fixture decode invariant.
- Server `tests/integration/pg_github_pull_requests.rs:171`: bound URL invariant.
- SQL `tests/integration/pg_verifier_executions.rs:4457,4486`: ID fixture and SYSTEM-provisioning panic paths.
- Python `src/testing.rs:1052` and TS `native-testing/src/github.rs:100`: wrappers inherit scripted-PR precondition panic from the mock.
- MCP `tests/bifrost/mcp/github.rs:215`: materially changed assertion-bearing journey.

**Correction/proof.** Document actual panic conditions on this bounded changed-item inventory and equivalent omissions in the same touched set. Preserve runtime error/validation behavior; add no wrapper validators or suppressions. Batch with substantive fixes. Regenerate N-API declarations through their existing owner if comment changes affect them; Python stubs remain authored. Source inspection and relevant format/stub/generated-parity checks suffice; no new runtime test for docs alone.

## Acceptance and preserved behavior

| Finding | Required observable closure |
|---|---|
| FIND-006-1 | Same-PR provider observation/decision serializes; no delayed stale work after newer no-row/replay/repeated-deactivation decisions; frozen replay bytes/IDs remain stable. |
| FIND-006-2 | Empty future policy performs no declaration-time sweep; next source cancels/fences older unfinished work with no new target runs; inactive cancelled generation cannot replay as current. |
| FIND-006-3 | Rate-limited 403 yields retryable non-2xx; successful retry admits exactly once. |
| FIND-006-4 | Access revocation/replacement orders with admission; old association cannot create or revive runnable work after access loss. |
| FIND-006-5 | Every shared PR inspection path requires current exact Verifier and repository read, including retained no-queue state; ordinary non-PR reads keep their contract. |
| FIND-006-6 | Held provider recovery cannot block worker cancellation or existing shared durable shutdown. |
| FIND-006-7 | Named changed-item panic contracts pass the explicit source gate. |

Keep existing Scribe/SubmissionSink/queue/runner owners, bounded staging, exact frozen fanout and policy, tenant isolation, current declared authority, cancellation codes, completed history, ordinary scheduled/group/submitted behavior, failed-delivery bounds and five public surface projections. Do not add tables, broker/inbox, sentinel runs, generic observation stores, permission categories, compatibility paths, source Card retrieval for inspection, UI, extraction, parked continuation or provider-result delivery. No optional SQL row-mapping cleanup is authorized by the rejected proposal. No new material decision is required by the selected approach; surface any genuine conflict with approved authority instead of expanding scope.

## Focused verification and handoff

Use existing owner test targets and repository-managed Postgres. Add direct regression scenarios at the owning server/SQL/retained-read/lifecycle test seams described above; record exact focused nextest commands for new tests before RED. Run each named new regression individually, then only affected owner slices. Existing held-read test choreography changes must retain the semantic assertions and record the lock-order diagnosis above.

The existing narrow server PR/connection slice is:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E "test(/pg_github_pull_requests/) | test(/pg_github_connection/)"'
```

For changed pure GitHub branches/worker tests, use `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/github::/)'`. If SQL transition code changes, run the focused existing SQL PR slice with managed Postgres and `mise exec -- cargo nextest run --locked -p wyrd-sql --test integration -E 'test(/pull_request_/)'`. Run only directly affected retained-inspection tests beyond these slices; discover and record their exact selectors rather than running all verification tests.

Finish required Rust format/lints and `git diff --check`; use dependency/tenant checks for changed authority/SQL paths. Run Python authored-stub lint/parity and the owning N-API generation/parity checks only where touched documentation/declarations require them. No full SDK/MCP/CLI journey suites or broad aggregate belong to this remediation; integrated change review runs them once. Do not repeat already-green unrelated lanes.

Record the new immutable candidate, direct failure/closure evidence and changed write set. Next review retains this original task, verdict, validated findings, remediation task and prior candidate; carry forward unaffected acceptance and review closure plus changed/affected paths.

## Obligations

Contract: the original Task 006 (A1–A18) plus the seven validated findings above. Rows record the closure each finding requires; A-rows the review already passed are carried forward unchanged and only re-run where this diff touches them.

| ID | Promise (source) | Required cases | Planned proof |
|---|---|---|---|
| R1-1 | Same-PR provider observations and their decisions serialize, so a newer accepted decision is never undone by an older read (FIND-006-1; A9/A10, INV-006) | held first open vs. later close over zero rows; held intermediate source vs. later replay of the current source; held ready vs. later repeated draft; the later delivery waits on the PR lock while the held read is outstanding; frozen IDs/pins and canonical replay unchanged; the two existing held-read sequences keep their final assertions | server `pg_github_pull_requests` regression test with held reads and the PR lock wait observed in `pg_locks`; existing lifecycle, close-fencing and interrupted-handoff tests |
| R1-2 | A new source supersedes unfinished older work even when the selected policy binds nothing, and the cancelled generation is never replayed as current (FIND-006-2; REQ-003/005, A8/A12) | empty policy selection leaves rows unchanged; next source cancels queued and running A with `pull_request_superseded`, late settlement is StaleLease, completed P unchanged, zero B executions; re-enabled policy and a return to A admit a new generation | server regression test |
| R1-3 | A rate-limited PR read is a retryable failure, not "no work" (FIND-006-3; A5/A7, INV-003) | 403 PR read answers non-2xx and admits nothing; redelivery admits exactly one fanout; 404 keeps its no-work meaning | server regression test with a 403 from the mock; existing ineligible/hidden tests for 404 |
| R1-4 | Access loss or replacement during a provider read cannot create runnable work for the old association (FIND-006-4; REQ-001/004, INV-001/006, A1/A10/A12/A18) | removal, suspension and uninstall-then-reconnect while the first open read is held admit nothing; reservation-before-loss is cancelled and activation cannot revive it; token method keeps its public signature | server regression test; SQL activation predicate test |
| R1-5 | Every shared retained inspection of a PR execution requires repository read (`cards:read`) as well as exact `verifier:read`, decided and audited once (FIND-006-5; security posture execution reads) | verifier-only caller refused for PR status and one sibling read; adding `cards:read` permits; retained PR state with no queue row decided from the accepted input origin; non-PR verifier-only reads unchanged | server regression test; existing verification route tests |
| R1-6 | In-progress failed-delivery recovery yields to shutdown (FIND-006-6) | held delivery-log read; cancelling the worker joins without releasing it and no later redelivery starts | server regression test with a held delivery-log read |
| R1-7 | Changed items document their panic conditions (FIND-006-7) | the nine named items and equivalent omissions in the touched set | source inspection; format, stub and N-API parity checks where touched |
| R1-P | Preserved behavior: existing owners, frozen fanout and policy, tenant isolation, cancellation codes, completed history, ordinary admissions, failed-delivery bounds, five public projections; no new table, broker, sentinel run, permission or UI | — | focused server PR/connection slice, SQL PR slice, affected verification inspection tests, lints, boundary checks, diff audit |
