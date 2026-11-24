---
id: TASK-007
kind: implementation
status: proposed
spec: SPEC-verification-workflow-ui
spec_revision: 8
supporting_spec: SPEC-authorization-model
supporting_spec_revision: 9
requirements: [REQ-002, REQ-005, REQ-006, REQ-009, REQ-014, REQ-020, REQ-021, AC-002, AC-005, AC-007, AC-008, AC-009, AC-014]
depends_on: [TASK-002, TASK-005]
---

# Retained PR context and independent GitHub delivery status

## Outcome and activation

A person reopens a PR execution after its scheduling row is gone and still sees
the same PR/generation, recorded verdict, outputs and GitHub delivery outcome.
Provider failure or retry changes delivery facts, never the verification
judgment. Another Repository's execution remains indistinguishable from unknown.

Approved revision 8 explicitly assigns the bounded backend read correction to
this task under REQ-021/AC-014, preserving current retained-parent authority and
existing response fields. Record the integrated dependency inputs before
implementation; approval is not proof that queue-free inspection already works.

This task includes the backend read correction as its first test-first scenario;
its UI scenarios consume that output. This is one complete delivery-inspection
outcome, not a new backend service or a separate placeholder prerequisite task.
TASK-005 supplies PR context/navigation and TASK-002 the shared inspector/status
loader. The backend scenario can be prepared independently of TASK-005, but
integrated UI completion consumes it. Nothing here blocks 005 or 006.

## Existing owners and connected implementation

Use the [agreed retained-status correction](../design/resume-005-007.md#maintainer-agreement--retained-status-correction)
and selected TASK-007 design. Current `VerificationControl::get_execution`
initializes `pull_request`/`github_delivery` absent when no queue row remains
and loads recorded delivery only when PR context is present. This is why the
retained data needs its existing projection corrected; receipt cleanup alone
does not prove queue-free inspection.

Extend the existing status/retained record reader and GitHub delivery reader.
After existing caller authorization, obtain exact PR source/generation/member
association from accepted Bifrost input records and recorded delivery from the
existing canonical records for that execution's accepted state/revision.
Keep the selected result identity and retained parent; never select an arbitrary
latest result, consult live GitHub, recreate a queue row or start verification.
No new endpoint, response field, migration, storage family or provider workflow.

The returned existing `VerificationExecutionState` is consumed unchanged by
HTTP, Rust/Python/TypeScript SDKs, MCP and the shared Node/MCP UI reader.
Current grants and sensitive-content checks apply before disclosure, including
after source Card/queue cleanup; missing/unprovable parents remain hidden.
The correction must preserve ordinary non-PR/direct/status behavior. It does
not move identities, Card definitions, permissions or live coordination out of
PostgreSQL and does not promise operation through a PostgreSQL outage. Retained
evidence cannot depend on leftover scheduling rows; eventual Bifrost visibility
must remain an explicit availability condition.

Then project `ExecutionData.status.github_delivery` into the existing execution/
PR context: pending/delivered/skipped/failed, recorded reason and sanitized anchor
problems, separate from status/verdict and equal ordinary retained publications.
No GitHub-configuration permission is required for an otherwise authorized
execution read. Non-PR work has no delivery section. Reuse existing exact
selection and refresh; do not add a second delivery read, poller, provider retry
button, credential or specialized report dashboard.

A delivery value of failed is a successful read of provider failure. A refused
status read is not failed verification or a provider outcome. The existing
wire returns one status projection: do not promise separately denied delivery
inspection. Keep independent readable parts where current loader boundaries
allow, without showing revoked content as current or inventing empty success.
Private record-reading/query/helper choices remain implementation-owned.

## Ordered acceptance scenarios and evidence

Each entry is the evidence record: append assertions, exact focused commands,
observed RED/GREEN and limits in place. Preserve real source records and callers;
fixtures alone cannot establish queue-free retained projection.

1. **Reconstruct the same status after queue cleanup.** Admit real PR work,
   complete it, record actual provider delivery and retain its inputs/accepted
   state/selected result/outcome in Bifrost. Capture the public status, then
   remove the terminal scheduling row through the existing owner-supported
   cleanup or appropriate test seam while keeping Postgres available.
   As a Repository-scoped reader reopen the execution: PR source/generation/
   members, verdict/result identity and recorded delivery match the pre-cleanup
   read. Retained progress/publications remain readable; no queue row, new
   execution or provider request is created. Include the accepted failed/skipped
   outcome variants needed to prevent a non-delivered fact becoming pending.
   **Map:** REQ-002/005/006/009/020/021, AC-002/005/007/014;
   authorization-model REQ-006 and architecture's retained execution/delivery
   contract. **Tier/owner:** real-server `pg_github_delivery.rs` and existing
   inspection tests with repository Postgres/Bifrost and controlled provider.
   Current queue-free absent PR/delivery fields should produce behavioral RED.
   Fix only this read projection before later UI scenarios.
   **Evidence (2026-11-24).** Dependency inputs: TASK-002 inspector/loader and
   TASK-005 PR context at `e031fbd11`. Assertions:
   `pg_github_delivery.rs` `after_queue_cleanup` (deletes the terminal row via
   the superuser pool, asserts one row deleted, re-reads and compares
   `status`/`pull_request`/`result`/`github_delivery`, no new execution, queue
   or `delivery_requests` row) used by
   `a_delivered_execution_reads_the_same_after_queue_cleanup`,
   `a_new_head_during_delivery_skips_every_later_write` (skipped/superseded)
   and `a_permanent_refusal_is_a_visible_failed_outcome` (failed/github_refused).
   Command (`<sel>` = each `test(=pg_github_delivery::…)`):
   `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration -E "<sel>"'`.
   RED: all three failed at `pg_github_delivery.rs:634` "pull_request is
   retained" (left `null`). GREEN after `service.rs` reads the PR association
   from the Scribe-acknowledged input record (`ExecutionRecord::pull_request`,
   `inputs.rs`) when no queue row remains, returning
   `WYRD_VALA_503_QUERY_VISIBILITY_UNAVAILABLE` if that record is not visible:
   those three, `a_recorded_outcome_outlives_its_retry_receipts` and
   `pg_github_pull_requests::pull_request_inspection_requires_repository_read`
   pass; non-PR queue-free regressions
   `pg_durable_verifier_work::failures_read_back_from_state_instead_of_the_queue_payload`
   and `pg_deferred_evidence::task_input_waits_are_reported_while_an_independent_step_runs`
   pass. Commit `5215b54d9`. **Mechanical deviation:** no owner deletes terminal
   `wyrd.verifier_executions` rows, so the server tier deletes the row directly
   and SDK/UI tiers use the new test seam `remove_settled_execution_row`.
   **FAIL — open gap (see Handoff):** an execution cancelled before it ran
   (superseded, closed, draft, access lost, public cancel of pending work,
   refused member) writes no checkpoint, so after cleanup
   `retained()` cannot prove its parent and returns
   `WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND` even for the tenant admin.


2. **Preserve authority and public consumers.** Against those same retained
   records, readers without the Repository parent, a shared-Verifier-only
   reader and another tenant obtain ordinary not-found/no hidden data. A
   minimal custom Repository Card-read role succeeds without Verifier-definition
   or GitHub-configuration access. Revocation affects the next read. Observe the
   corrected existing status through HTTP and real Rust/Python/TypeScript SDK/
   MCP consumers; preserve ordinary/direct non-PR reads and explicit retained
   availability failures.
   **Map:** REQ-005/009/014/020/021, AC-007/008/014; authorization-model REQ-004/006/007.
   **Tier/owner:** current server inspection, SDK verification and MCP journey
   owners; reuse their fixtures/runners and add actual selectors before RED.
   A new projection bypass or consumer loss should fail; unchanged authorization/
   decoding can be already-green regression proof with no manufactured churn.
   **Evidence (2026-11-24).** Server: `pull_request_inspection_requires_repository_read`
   (`pg_github_pull_requests.rs`) after queue-row delete asserts verifier-only
   reader 404, a directly granted custom role holding only
   `executions:read` on the Repository's exact scope (no Verifier or GitHub
   configuration grant) reads equal `status`/`pull_request`/`result`/`github_delivery`,
   `revoke_role` refuses it within `AUTHORITY_PROPAGATION` as
   `WYRD_PERMISSION_403_DENIED_RBAC` (route-level, names no execution), and
   another tenant's admin reads 404. Same command as scenario 1 with
   `test(=pg_github_pull_requests::pull_request_inspection_requires_repository_read)`: GREEN
   (commits `9f07f9c70`, `0c0a6ac3a`). Diagnoses: (a) an immediate read after revoke
   returned 200 — cause: cache refresh is notification-driven, bounded at 5s
   (`wyrd-security-posture.md` "Current authority and freshness"); (b) still 200
   after 5s — cause: `bootstrap_user` binds roles with `RoleSource::Idp` and
   `revoke_role` deletes only `source = 'direct'` bindings, silently returning
   `Ok`; fixed in the test by a direct `grant_role`. **Risk:** the same silent
   no-op makes `crates/wyrd/wyrd-testing/tests/bifrost/server/verification_runtime.rs:3073`
   prove nothing about its revoked runner (outside this task's write set).
   Consumers (already-green regression proof, added after the server fix; the
   Rust SDK selector FAILED at `deferred_evidence.rs:530` with `service.rs`
   restored to `5215b54d9~1`, establishing assertion strength):
   Rust `WYRD_REG_E2E=1 mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=deferred_evidence::pg_tests::a_person_approves_the_pending_signoff)'"`;
   Python `… -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q tests/integration/test_deferred_evidence.py -k approves"`;
   TypeScript `… -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/deferred-evidence.test.ts -t 'approves the pending signoff'"`;
   MCP `… -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(/github::.*an_agent_approves_a_pending_signoff/)'"`.
   All GREEN (commit `71ceaa21e`). `py:typecheck` found the Python
   `ExecutionState` stub lacked `github_delivery`; added `GithubDelivery`/`AnchorProblem`
   TypedDicts. `fmt`, `lints`, `py:format`, `py:lints`, `py:typecheck`,
   `ts:lints`, `ts:typecheck` pass. Not run: `check:deps`, `check:tenant-isolation`.


3. **Render recorded delivery independently.** Load that corrected status through
   shared Node/MCP UI executors and the inspector. Present pending/delivered/
   skipped/failed with recorded reason/problems, separately from the unchanged
   verdict and equal ordinary publications. Non-PR work has no section. Missing
   details or an unreadable Verifier definition does not erase readable delivery;
   a status refusal is a status refusal, not a fabricated provider state.
   **Map:** REQ-005/006/009/014, AC-002/005/008. **Tier/owner:** component cases in
   `ui-library.test.ts` and actual records through
   `ui-library.integration.test.ts`. Missing delivery rendering should produce
   RED; no new read op is needed.
   **Evidence (2026-11-24).** Component: `ui-library.test.ts` "a pull request
   execution shows its recorded GitHub delivery apart from its verdict, and
   other work shows none" covers delivered, failed (`github_refused` + anchor
   problem), skipped (`superseded`), pending (never "queued"), refused Verifier
   definition, no section for non-PR work, one status read only, and a refused
   status read showing its code with no delivery. Command:
   `cd crates/wyrd/wyrd-server/wyrd-ui && pnpm --filter wyrd-ai-ui build && pnpm exec vitest run src/lib/ui-library.test.ts -t "a pull request execution shows its recorded GitHub delivery"`.
   RED: `[data-github-delivery]` null at `ui-library.test.ts:4185`. GREEN after a
   "GitHub delivery" fact in `VerificationExecution.svelte` (StatusMark + one new
   `delivered` mark in `observe.ts`; reason and anchor words are display
   mapping). `mise run ui:check` GREEN (119 tests). Commit `ef5c858eb`.
   Real records: the uncommitted TASK-007 test in `ui-library.integration.test.ts`
   (`WYRD_UI_INTEGRATION=1`, `removeSettledExecutionRow`, Node and MCP executors,
   Repository family viewer) is **FAIL**: the superseded member reads 404 after
   cleanup (scenario 1 gap).


4. **Follow actual provider outcomes without re-verifying.** Use existing
   backend-controlled success, permanent refusal, lost-answer/retry/recovery and
   superseded/access-removed skip scenarios. The browser/shared hosts consume
   their actual retained delivery facts; recorded verification result/outputs
   stay attributable and no UI read triggers a retry, recomputation or provider
   write. Distinguish the earlier outcome from any newer execution's delivery.
   **Map:** REQ-002/006/009/014, AC-005/007/008. **Tier/owner:** existing server
   delivery fixtures, UI host integration and production BFF/browser journeys.
   Reuse backend reconciliation evidence and add RED for missing connected UI
   association/refresh behavior, not a second retry engine.
   **Evidence (2026-11-24).** Not complete. Observed in the real UI host: an
   execution superseded before any delivery attempt records nothing, so its
   delivery is `pending` indefinitely (matches `wyrd-design.md` "pending until
   recorded"; only public cancellation and revocation deliver a cancelled
   outcome). The TS GitHub mock exposes no refusal/outage control, so
   failed/retry outcomes in UI hosts rest on the server fixtures in scenario 1.


5. **Reopen pinned history and compare the approved context.** Follow TASK-005's
   earlier set/member link after queue cleanup, refresh/copy/reload and return
   while the PR changes. Exact execution/delivery association remains pinned;
   inaccessible reads disclose no provider detail. Exercise Node/MCP/custom/
   built-artifact read parity and desktop/mobile, both themes, keyboard/touch,
   long/error/unavailable states. Use existing provider/PR context composition,
   not a new management page.
   **Map:** REQ-002/009/014/021, AC-005/007/008/009/014. **Tier/owner:** existing host and
   production-browser integration tests plus visual comparison. Missing history/
   delivery wiring should produce RED; record AC-009 comparison evidence and get
   the maintainer's ruling on remaining mock differences under
   [wyrd-ui](../../../../.agents/skills/wyrd-ui/SKILL.md#matching-an-approved-mock).
   **Evidence (2026-11-24).** Not started: blocked on the scenario 1 gap (a
   pinned earlier-set member is exactly the cancelled-before-start case). The
   approved mock has no GitHub delivery element; AC-009 comparison and the
   maintainer's ruling are pending.


## Verification and handoff

Server owners: `components/verification/service.rs`, its retained inspection/
input readers and `components/github/delivery.rs`. Test owners include
`pg_github_delivery.rs`, existing inspection/verification SDK and MCP journeys.
Preserve the existing receipt-retention regression, which is a different case
from scheduling-row cleanup:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration -E "test(=pg_github_delivery::a_recorded_outcome_outlives_its_retry_receipts)"'
```

Add new server and shipped-consumer selectors before their first RED run; do not
invent test names in planning. Use current integration runners in TESTING.md
and mise. UI owners are `ui-library.test.ts`, `ui-pages.test.ts`,
`ui-library.integration.test.ts` and `ui-pages.integration.test.ts`.
Read `wyrd-ui`; prepare current `ts:build`, `ts:build:testing`,
`build:server` and UI build before managed-Postgres UI integration with
`WYRD_UI_INTEGRATION=1`. Run focused affected/new tests, `mise run ui:check`,
UI build, and catalog regeneration/`mise run codegen:check` if schemas change.
For changed Rust run `mise run fmt`, `mise run lints` and applicable
`check:deps`/`check:tenant-isolation`; preserve existing SDK wire contracts.

Record any added/moved external waits and justify their owner boundary. No
verification execution, gateway, Oracle or Scribe hot-path change is assigned;
a need to modify those paths returns to the approved scope rather than widening
this status-read repair. Add no benchmark or unrelated broad gate. Full
shipped-surface lanes belong to integrated change review.

Planning and specification approval are not implementation, runtime
qualification or independent task acceptance.
