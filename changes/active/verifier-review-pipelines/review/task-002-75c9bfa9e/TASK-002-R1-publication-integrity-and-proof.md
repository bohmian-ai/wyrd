---
id: TASK-002-R1
kind: remediation
status: ready
spec: SPEC-verifier-review-pipelines
spec_revision: 18
parent_task: TASK-002
remediates: [FIND-TASK-002-1, FIND-TASK-002-2, FIND-TASK-002-3, FIND-TASK-002-4, FIND-TASK-002-5, FIND-TASK-002-6, FIND-TASK-002-7]
---

# Correct hosted publication identity, receiving authorization, and acceptance proof

Route directly to `$wyrd-implement`. This task packages independently validated findings; it authorizes no new product or security decision.

## Immutable acceptance context

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/code-review-workflow`.
Approved authority: `changes/active/verifier-review-pipelines/spec.md`, approved baseline revision 18 plus explicitly approved corrections and its `output-publication.md` / `execution-state-and-durability.md` supplements. Current spec frontmatter revision 21/draft does not authorize unrelated wait/approval work.
Original task: `changes/active/verifier-review-pipelines/tasks/TASK-002-agent-tooling.md`.
Original base: `990a85dc7155ff0d4de1acc17e4aeaa7a42d582f`.
Reviewed candidate and remediation baseline: `75c9bfa9e07163099eac40d5a556c4447c6a519d`.
Original implementation range includes `973b78e5e` itself. Prior verdict/ledger: sibling `verdict.md` and `findings-validation.md`; preserve both and all discovery reports. A remediation review compares the new candidate to this candidate and carries source-backed unaffected evidence forward.

The outcome is one original-call effect per producer, distinct accounted attempts after retained effects, the existing File-domain decision at its receiving boundary, and accurate focused proof on every required surface. Existing write/publish behavior and owner reuse remain the foundation. The diagnoses and selected corrections below are the acceptance context, not optional redesign suggestions.

## Diagnosis, selected correction, and focused acceptance proof


### FIND-TASK-002-1 — Original publication call can acquire a second name

**CONFIRMED; INCORRECT; source DATA-001.** Violates REQ-047 and the publication supplement's explicit conflicting-same-call-arguments refusal.

Provider identity passes through `skald-agent/src/loop_runtime.rs:632`, `vala-verify/src/blocks/tools.rs:272`, and hosted `tools.rs:342–368` into Publications. Its path preflight and both path/JSON acceptance call the same RunState receipt helper (`publications.rs:241–309`). `state.rs:965–968` only selects the incoming producer/name. Accepting call `p1` with name `a`, then `p1` with name `b`, finds no `b` and appends a second accepted publication at `state.rs:793–803`. This is reachable: the loop accepts repeated IDs, and the new replay journey deliberately emits them. Both entries become retained collection/recovery content. Materiality is explicit idempotency/integrity, not general provider hardening.

**Minimal correction:** enforce both producer/name uniqueness and producer/original-call argument identity in the existing shared receipt owner, using already-retained call ID, name and digest. A matching call can replay only its original name/arguments; an occupied name from another call still conflicts. Preserve fresh-attempt isolation, preflight before file work, atomic acceptance, budgets and durable-before-receipt. No cache, new persisted field/index, adapter ledger or uploader. Burden is one shared-owner correction plus focused proof.

**Closure:** extend the existing Agent replay case with sequential same-ID changed-name/value calls; assert typed conflict and only original retention. Keep identical replay/different-call same-name/fresh-attempt assertions. One focused owner test against restored existing entries proves recovery uses the same rule; no new restart harness.

### FIND-TASK-002-2 — Capacity deferral forgets an attempt that already retained publication effects

**CONFIRMED; INCORRECT; source INV-002-1.** Violates REQ-047 separate fresh-attempt attribution and DUR-005 conserved bounds.

The new producer uses engine `BlockCall.attempt` (`blocks/agent.rs:451–472`). After successful publish, a later code.run can hit `hosted/sandbox.rs:202–205` admission refusal. `Invocation::run:386–416` preserves typed Busy; RecordedTool raises refusal and `AgentExecutor::invoke:410–426` returns Busy. `blocks/engine.rs:875–879` calls forget. `state.rs:1019–1030` removes the unsettled invocation without checking publications. Accepted publications survive. Queue defers (`runner.rs:1707`), and restored attempts come solely from surviving invocations (`runner.rs:832–843`). The next model run can therefore be attempt 1 again: its fresh call conflicts with the retained name, or reused original identity returns old content. Repeated deferrals also lose block attempt accounting. The selected limits test never exercises this order.

**Minimal correction:** extend the existing RunState invocation/forget invariant: an invocation with retained publications cannot be removed and its producer reused by fresh work. Retain that consumed attempt so existing restoration and retry exhaustion account for it; keep the current no-spend behavior for refusal before retained work. Check the action, unavailable-definition and requirement forget callers against this rule. Requirements cannot publish. Keep acknowledged content, queue deferral, existing scheduler/recovery and shared authority. No extra session lifecycle or state store. This enforces already-approved attempt conservation rather than defining new retry semantics.

**Closure:** deterministic existing server-owned test publishes, forces later sandbox Busy, then recovers. Assert old bytes remain inspectible and fresh permitted work uses the next attempt and may reuse the name. With no remaining retry, it must not restart attempt 1. A before-effects Busy control still defers without spending an attempt. Use existing capacity controls, not synthetic load.

### FIND-TASK-002-3 — File publication omits its receiving permission decision/audit

**REVISED; MISSING; sources SEC-001, FOLLOWUP-SEC-1.** Violates REQ-023 and `wyrd-design.md:1793–1799,2009–2015`; security posture requires downstream decisions against accepted authority.

Normal write→path publish authorizes only exact tool invoke (`tools.rs:233–242`). Publications path→complete (`publications.rs:126–231`) uses tenant-only StorageCaller, initializes/uploads/completes Files and records inventory without a File-domain decision. Storage's caller projection (`storage/routes.rs:711–718`) expressly assumes the receiving caller already evaluated/audited it. Public evidence planning and follow-ups reuse `VerificationControl::authorize_evidence` (`verification/service.rs:298–305,1017–1047`; storage routes `651–667`). Exact verifier admission means supported calls already hold its any-scope verifier:run requirement; the failure is the omitted later audited boundary, not a proved public privilege escalation. Run admission, tool provenance and object inventory are not that audit event.

**Minimal correction:** reuse authorize_evidence under Publications' accepted Caller at the shared receiving path/File publication boundary, before replay return or snapshot/upload work. One request/directory bundle needs one domain decision, like evidence planning. Keep tool invoke separate. JSON creates no File and gains no invented File decision. Preserve tenant-only lower storage, inventory, completion, accepted descriptors, lease fencing, ACK order and best-effort audit; no decision per object/internal SQL operation and no durable audit wait. Update existing direct backend fixtures that currently construct callers with no verifier grant, including pg_durable_verifier_work callers, so success tests use valid accepted authority; retain a focused denial case. This is reuse of an existing mandatory permission owner, not a new policy.

**Closure:** existing hosted owner/RecordingScribe proof observes separate tool and File Allowed events for a real path/tree publication with exact-UID verifier grant and unchanged actor/delegation. Tool-only direct receiving-owner denial precedes File IO; it is not a public admission bypass claim. Replay still returns original receipt without new upload work. Do not require live grants or audit persistence before receipt.

### FIND-TASK-002-4 — Explicit public-surface closure remains partial

**REVISED; MISSING; source B-001.** TASK-002 Public surface closure and REQ-032 explicitly require these proofs.

Python `test_hosted_agent_tools.py:104–212` and TS `hosted-agent-tools.test.ts:268–406` prove real file/tree/JSON/null discovery and invoke denial, but have no invalid-selection registration or invoke-granted underlying-domain refusal. MCP `verification.rs:3393–3595` proves successful tool publication and execution-read denial, not invalid selection or in-run invoke/domain refusal. Existing CLI `durable_verifier_work.rs:303–355,618+` seeds publications through VerificationFixture, so it cannot prove hosted tool dispatch. Existing route download and OpenAPI tests prove backend response/permission shapes; the full candidate diff adds no new tool-contract assertions there. This is an explicit evidence deliverable gap, not a claim all these production projections fail.

**Minimal correction:** extend existing language/MCP stories with the missing descriptor and invoke/domain refusal rows, reusing checked-in invalid fixtures and existing domain tools/authorization owners. Invalid selection must stop before provider dispatch; denied calls return no protected bytes. A public File-grant-removal-in-run case is not achievable under accepted snapshot authority and must not be invented. Reuse a real Agent-created execution for thin in-process CLI listing/file-tree-JSON inspection/refusal. Extend existing route/served-schema tests only for the new canonical tool identity/selection/error projection assertions. Preserve existing backend route/page tests as sufficient for their unchanged obligations. No SDK storage code or another full surface workflow.

**Closure:** run only changed named language/MCP/CLI/route/schema cases. Map each missing row to its exact assertion; do not substitute a test title or rerun every foundation test.

### FIND-TASK-002-5 — Lifecycle/publication evidence needs the required failures at the correct test tier

**REVISED; MISSING / VIOLATION; sources B-002, SSR-002.** TASK-002 names lost reply, unavailable inventory and recovery; DUR-008 requires real Agent publication/retry/inspection. TESTING.md:279–289,330–339 prohibits SDK internal controls, sleep and digest computation.

The replay script (`hosted_agent_tools.rs:337–346`) returns both identical JSON receipts; it loses no reply. `publications_follow_shared_run_state:1434–1511` restarts with inventory available, uses a five-second sleep, flushes Bifrost, accesses server.state and expires deadlines using VerificationFixture. The backend `pg_durable_verifier_work.rs:872–1004` independently proves unavailable inventory with exact accepted descriptors, but not a real Agent-produced recovered consumer seam. This source supports the production design; it does not replace the task's explicit additional seam proof. SDK digest helpers at Rust `683–685`, Python `99–101`, TS `236–238` and Rust's receipt hash assertion compute values forbidden by the checklist. These are one proof-placement/completeness correction, not a request for a new runtime.

**Minimal correction:** retain public SDK submit/verify/read/discovery outcomes; move private crash/lease/flush and deterministic disconnect/attempt assertions to the existing server Postgres tier. Extend that same owner proof with actual Agent-produced accepted content, inventory discovery unavailable while exact-object reads remain available, recovered consumer bytes and no settled-producer replay. Complement it with the existing invocation/publication seam losing a reply after acceptance, then replaying original call/attempt to the original receipt without another acceptance/upload. Existing Task-001 WAL/fence/loader cases remain sufficient for their foundation responsibilities. Replace SDK digest calculations with fixture-owned expected digests or move algorithm checks to the owner tier; preserve typed plan lengths/digests and content assertions. No public resume API, harness framework, polling workaround or duplicated lifecycle.

**Closure:** changed named server seam/recovery and affected SDK cases only. Observe accepted receipt loss/replay, exact bytes with inventory unavailable, no duplicate publication, no settled drafter replay, deterministic disconnect, and conforming public proof bodies. This does not require a new fault matrix across every SDK.

### FIND-TASK-002-6 — fs.write budget refusal has no public Agent-loop negative proof

**CONFIRMED; MISSING; source SSR-004.** Violates task scenario/Proof 1 and the repository rule that a required user negative path cannot be unit-only.

Workspace::write enforces cumulative replacement-aware budget at `workspace.rs:1097–1108`; direct workspace tests prove the calculation. The public limits script `hosted_agent_tools.rs:275–309` successfully writes files and refuses oversized JSON publication. The test `:1335–1414` checks publication_size, never work_budget; task evidence admits write budgets are unit-only. An invocation-wiring/refusal-recording defect could leave the unit green.

**Minimal correction/closure:** extend the existing real Agent scratch/limits case with a bounded work ceiling, cumulative overflow refusal and unchanged prior file readback; assert work_budget and the failed record/no returned bytes. Include replacement accounting at the boundary if necessary to distinguish per-file from cumulative behavior. Reuse existing configuration/story; keep the direct owner test. Only this changed journey plus the named workspace test and affected checks are needed.

### FIND-TASK-002-7 — Changed Rust helper documentation/imports fail mandatory rules

**CONFIRMED; VIOLATION; source SSR-001 and its rule table.** Root AGENTS and agent_docs/rust.md:122–129,234–240 apply to changed private/test items.

`hosted_agent_tools.rs::builder:643–647` parses with expect; `reply:270–465` indexes results at 398; `hosted/tools/pg_tests.rs::invocation:34–60` has setup expects and is materially changed. Each lacks # Panics. The Rust digest helper and MCP journey's local imports (`verification.rs:3397–3400`) do not meet the rare generic-function trait exception. These are explicit mandatory merge rules, independent of runtime probability.

**Minimal correction/closure:** document actual remaining panic invariants on these changed helpers and move remaining imports to module scope. Deleting/moving forbidden digest helpers under FIND-5 may eliminate their imports. Check the same changed items after proof movement; do not sweep unrelated old helpers. Static changed-item inspection plus normal affected formatting/lint checks suffices; batch with behavior/proof changes, with no style-only round.


## Constraints and preserved behavior

Keep the existing Agent loop, ToolRegistry/ToolResolver, ToolSession/RecordedTool, Workspace writer/budget, Publications/File upload lifecycle, RunState and queue/recovery owners. Fix the shared cause once; do not add downstream name guards, another ledger, uploader, permission identity, resolver, scheduler or test framework. Existing accepted publications remain immutable, attributed, inspectible and retained; do not delete them to reset an attempt. Retain same-name conflict and fresh-attempt isolation, JSON/null, file/tree paths and empty directories, bounded resources, fencing, durable ACK before receipt, direct queue-free execution, cancellation and tenant separation.

Accepted execution authority remains a snapshot. Arbitrary role changes do not refresh it mid-run. Route the contradictory original private task sentence about removing Invoke/File grants between calls to `$wyrd-plan` under the existing stronger security authority; distinguish initial narrowed/delegated authority, later issuance, principal revocation/cancellation and per-call checks. This is task correction, not permission to implement live grant refresh. If the human later requests new live semantics, stop that new scope for specification/architecture revision. No such revision is required for these seven fixes.

Scratch-only code.run and same-invocation scratch reads are permitted by the approved publication supplement. Do not revert or redesign them based on the implementation self-assessment. Standalone code still receives no write/publish tools, checks cannot publish, and remote MCP/GitHub integration work remains excluded. No new sensitive-capture policy or speculative sanitizer.

Original production-before-RED is a confirmed historical deviation. Preserve it truthfully: no fabricated RED, rewritten history, test-only production churn, or new approval stage. Correct the current evidence/status through the task's normal evidence update without erasing the original admission or this review. It must distinguish supplied execution results, precise inspected assertions and remaining limitations; all-PASS is not accepted completion.

## Execution and verification

Implement one failing scenario at a time with genuine focused RED, minimum GREEN and refactor only when earned. For an already-correct/static obligation, record verification-only or static proof. Use existing owners and test targets, confirm each actual selector before execution, and record nonzero selection. New proof names remain implementation-owned: record their exact focused commands before RED. Do not spend a cycle rebuilding unrelated Task-001 fault matrices.

Existing confirmed entry points to extend are below. Run only those whose assertions/write set change; when private lifecycle assertions move to the server tier, retain public SDK outcomes and replace the removed private proof with the deterministic server case before claiming closure. Use new exact server selectors for the retained-effect Busy and receiving-audit cases after confirming their registration. Cargo-backed commands sharing target run sequentially.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=hosted_agent_tools::agent_publish_retries_use_host_identity)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=hosted_agent_tools::agent_writes_private_workspace_files)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=hosted_agent_tools::tool_limits_release_capacity_and_preserve_provenance)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=hosted_agent_tools::publications_follow_shared_run_state)"'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::hosted::workspace::tests::workspace_writes_private_scratch)'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_hosted_agent_tools.py::test_agent_publishes_file_directory_and_json'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_hosted_agent_tools.py::test_tool_calls_need_their_own_grant'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/hosted-agent-tools.test.ts -t "^agent_publishes_file_directory_and_json$"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all --test-threads=1 -E "test(=verification::pg_tests::hosted_agent_tools_agent_publishes_file_directory_and_json)"'
```

The CLI/route/schema corrections extend existing public tests cited in FIND-4; list their selected exact assertions/commands and any new server proof selectors before RED. This task does not mandate executing unrelated existing cases merely because the discovery reports mention them. Use the repository Postgres wrapper/migrations and actual server target/feature setup from `tasks/verification.md`; never select zero tests or bypass the owning test controls.

Apply relevant formatting/lint/type checks from the existing `tasks/verification.md#owner-checks` to the actual write set. Run contract generation/boundary checks only if their inputs change; source fixes here need no new contract generation. Preserve docs/generator evidence limits until the actual owning checks run. No `gate`, full user-journey suite, full language integration suite, synthetic load, or repeated broad aggregates in this remediation; combined journeys run at change review.

Closure requires each of FIND-TASK-002-1 through -7 to satisfy its focused proof above, accurate evidence for the modified tests/checks, and no new source/authority drift. It does not require speculative adjacent hardening. Do not commit or push unless the caller asks.

## Obligations

| ID | Source line | Required observation | Planned assertion |
|---|---|---|---|
| O1.1 | 35, 37 | A call ID already accepted under name `a` cannot publish name `b` (typed `publication_name` conflict) in the real Agent loop | Rust journey `agent_publish_retries_use_host_identity`: sequential same-ID changed-name call refused, only original listed |
| O1.2 | 35, 37 | The same call ID with a changed value conflicts and the original retained value is unchanged | same journey: same-ID changed-value call refused; `replayed` reads its original value |
| O1.3 | 37 | Identical replay, different-call same-name conflict, and fresh-attempt reuse keep their behavior | same journey retains its existing replay/attempt assertions; owner test below |
| O1.4 | 37 | Restored entries use the same rule | `state::tests` owner test over a state rebuilt from an accepted snapshot |
| O2.1 | 45, 47 | A Busy after a retained publication keeps the attempt; the old content stays inspectible; the next run uses attempt 2 and may reuse the name | new server pg test in `pg_durable_verifier_work` |
| O2.2 | 47 | With no retry left the recovered run does not restart attempt 1 | same test: retries 0 step ends interrupted with no model call |
| O2.3 | 47 | A Busy before any retained effect still defers without spending an attempt | same test: control step's attempt is forgotten and reruns as attempt 1 |
| O2.4 | 45 | Action Busy, unavailable-definition and requirement `forget` callers all follow the rule | static: one shared `RunState::forget` owner serves all three |
| O3.1 | 55, 57 | A real path/tree publication stages a tool Allowed and a File Allowed decision under an exact-UID verifier grant with unchanged actor/delegation | `hosted::tools::pg_tests` RecordingScribe test |
| O3.2 | 57 | A tool-only caller is denied at the receiving owner before any File IO | same test: denial answers before a missing path's `workspace_missing` |
| O3.3 | 57 | A replay returns the original receipt without new upload work | same test: replay with the scratch file removed returns the receipt |
| O3.4 | 55 | Direct backend success fixtures use valid accepted authority; a focused denial remains | `pg_durable_verifier_work` callers hold the exact verifier grant; denial in O3.2 |
| O3.5 | 55 | JSON creates no File and gains no File decision | same pg_tests test: JSON publish stages only the tool decision |
| O4.1 | 63, 65 | Python: invalid selection refused before provider dispatch | `test_hosted_agent_tools.py` new case |
| O4.2 | 63, 65 | Python: invoke-granted but domain-denied call returns no protected bytes | `test_hosted_agent_tools.py` |
| O4.3 | 63, 65 | TS: invalid selection refused before provider dispatch | `hosted-agent-tools.test.ts` |
| O4.4 | 63, 65 | TS: invoke-granted domain refusal | `hosted-agent-tools.test.ts` |
| O4.5 | 63, 65 | MCP: invalid selection and in-run invoke/domain refusal | `wyrd-mcp verification::pg_tests` |
| O4.6 | 65 | CLI: listing and file/tree/JSON inspection and refusal over a real Agent-created execution | Rust SDK journey in-process CLI case |
| O4.7 | 65 | Route/served-schema assertions for the canonical tool identity/selection/error projection | existing route/OpenAPI tests |
| O5.1 | 75, 77 | Server tier: actual Agent-produced content, inventory unavailable while exact-object reads stay available, recovered consumer bytes, no settled-producer replay | server pg test |
| O5.2 | 75, 77 | A reply lost after acceptance replays the original call/attempt to the original receipt with no new acceptance/upload | server pg test |
| O5.3 | 75, 77 | Deterministic direct disconnect starts no later Agent call, without sleep | server tier |
| O5.4 | 73, 75 | The Rust SDK journey keeps public outcomes only: no sleep, flush, `server.state`, or fixture deadline expiry | `publications_follow_shared_run_state` rewrite |
| O5.5 | 73, 75 | SDK journeys compute no digests: Rust `digest`, receipt hash, Python, TS use fixture-owned expected values | journey edits |
| O6.1 | 85 | Real Agent loop: cumulative `work_budget` refusal with replacement accounting, failed record with no returned bytes, prior file unchanged | `tool_limits_release_capacity_and_preserve_provenance` |
| O7.1 | 93 | `# Panics` on changed `builder`, `reply`, `pg_tests::invocation`; MCP journey imports at module scope; digest helper gone | static inspection + fmt/lints |
| O8.1 | 100, 104 | Contradictory grant-removal sentence routed to `$wyrd-plan`; evidence keeps the original admission and states limits | evidence section |

## Remediation Evidence

Candidate: commits `cc4165193..28f07d99c` on `task/verifier-review-pipelines/TASK-002`
(base `75c9bfa9e`). Server-tier commands use the repository wrapper:
`mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --run-ignored=all --test-threads=1 -E "test(=<name>)"'`.
SDK/MCP commands are the Execution block above with the exact selectors named
below. Each selector was confirmed to select exactly the tests named (nonzero).

RED (missing behavior, before production change):

- FIND-1: `hosted_agent_tools::agent_publish_retries_use_host_identity` failed at
  "the call cannot take another name or value" (same call ID with a changed
  name/value was accepted). GREEN after `cc4165193` (`RunState` receipt rule).
- FIND-2: `pg_durable_verifier_work::retained_publications_keep_their_busy_attempt`
  failed at "an attempt with retained work stays spent; one without is
  forgotten". GREEN after `5debbfecd` (`RunState::forget`).
- FIND-3: `verification::hosted::tools::pg_tests::file_publications_decide_their_file_permission`
  failed at "the tool and File decisions are separate; JSON makes no File
  decision". GREEN after `5991b0365`; the ten existing `pg_durable_verifier_work`
  backend cases passed with exact-verifier authority.

Verification-only (behavior already correct; the new assertions passed on first
run against unchanged production): FIND-4, FIND-5, FIND-6. FIND-6 was
additionally mutation-checked: removing the replacement credit
(`saturating_add(replaced)`) in `Workspace::write` failed the test at "a
replacement is charged only what it adds"; the mutation was reverted.

| Obligation | Implementation evidence | Assertion | Verification evidence | Result |
|---|---|---|---|---|
| O1.1 changed-name same call ID refused | `verification/state.rs` receipt rule (`cc4165193`) | `sdks/wyrd-sdk-rust/tests/integration/hosted_agent_tools.rs:1089-1095` (`publication_name` for `answered[2..]`), listing `:1113` | `agent_publish_retries_use_host_identity` RED→PASS | PASS |
| O1.2 changed-value conflicts; original unchanged | same | `hosted_agent_tools.rs:1089-1095`; `replayed` reads original value `:1135-1145` | same | PASS |
| O1.3 identical replay / different-call / fresh attempt retained | same | `hosted_agent_tools.rs:1080-1087` (original receipt), `:1100-1121` per-attempt attribution; `state.rs:1436-1470` | journey PASS; `verification::state::tests::a_call_keeps_its_original_publication` PASS | PASS |
| O1.4 restored entries use the same rule | `RunState` rebuilt from accepted snapshot | `crates/wyrd/wyrd-server/src/verification/state.rs:1406-1470` | `cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::state::tests::a_call_keeps_its_original_publication)'` PASS | PASS |
| O2.1 Busy after retained publication keeps attempt; old content inspectible; attempt 2 | `RunState::forget` keeps attempts with retained publications (`5debbfecd`) | `pg_durable_verifier_work.rs:1600-1635` | `retained_publications_keep_their_busy_attempt` RED→PASS | PASS |
| O2.2 no retry left: not restarted, no model call | same | `pg_durable_verifier_work.rs:1644-1656` | same | PASS |
| O2.3 Busy before retained effect still forgotten | same | `pg_durable_verifier_work.rs:1658-1663` | same | PASS |
| O2.4 every forget caller follows the rule | single owner `state.rs:1022` (`RunState::forget`); callers `vala-verify/src/blocks/engine.rs:389,785,877,881` all route through it | static inspection | grep of `forget(` callers | PASS |
| O3.1 tool Allowed + File Allowed under exact-UID verifier grant, unchanged actor/delegation | `authorize_evidence` File decision in `hosted/publications.rs` (`5991b0365`); delegated, SYSTEM-executed caller added in `0ac4a0589` | `hosted/tools/pg_tests.rs:372-396` (each row's subject/actor equals `caller.attribution()`; each row's detail equals the caller chain's `DelegationAttribution` projection, `28f07d99c`), `:416-426` (delegated caller with distinct actor), `:465-475` (`files("allowed")`) | `file_publications_decide_their_file_permission` RED→PASS | PASS |
| O3.2 tool-only caller denied before File IO | same | `pg_tests.rs:466-480` (denial on a missing path, not `workspace_missing`) | same | PASS |
| O3.3 replay returns receipt with no upload | same | `pg_tests.rs:434-447` ("a replay needs no scratch and uploads nothing") | same | PASS |
| O3.4 backend fixtures hold exact verifier grant | `pg_durable_verifier_work.rs` callers via `Leased::publisher` | ten existing cases plus O2/O5 cases | f3-fixtures 10/10 PASS; final 3/3 PASS | PASS |
| O3.5 JSON creates no File decision | same | `pg_tests.rs:441-450` | same | PASS |
| O4.1 Python invalid selection refused pre-dispatch | fixtures under `cards/hosted_agent_tools/` | `sdks/wyrd-sdk-python/tests/integration/test_hosted_agent_tools.py:225-243` | `test_hosted_agent_tools.py::test_refused_selections_never_reach_the_model` 1 passed (verification-only) | PASS |
| O4.2 Python invoke-granted, domain-denied: no bytes | existing | `sdks/wyrd-sdk-python/tests/integration/test_verifier_building_blocks.py:706-712` (`no_domain_grant`), `assert_tools` `:347` (DENIED, sha None, 0 bytes) | existing assertion cited (see limits); `test_tool_calls_need_their_own_grant` 1 passed | PASS |
| O4.3 TS invalid selection refused pre-dispatch | same fixtures | `sdks/wyrd-sdk-ts/wyrd/tests/integration/hosted-agent-tools.test.ts:415-437` | vitest `-t "^refused_selections_never_reach_the_model$"` 1 passed (verification-only) | PASS |
| O4.4 TS invoke-granted domain refusal | existing | `sdks/wyrd-sdk-ts/wyrd/tests/integration/verifier-building-blocks.test.ts:772-776` (`no_domain_grant`), `assertTools` `:333` | existing assertion cited (see limits) | PASS |
| O4.5 MCP invalid selection + in-run invoke/domain refusal | `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs` (domain case `47e8933d8`) | `x-write-inputs` workspace refusal `:3525-3543`; NARROWED-RUN all DENIED, nothing published `:3672-3693` (invoke refusal); NO-QUERY-RUN over existing `tool-review` with every tool but no `bifrost_query:read`: `cards.get` returns bytes, `bifrost.query` DENIED with 0 bytes and no sha `:3749-3760` (domain refusal); invalid fixtures `refused.code()` `:3765-3789`; no "NEVER CALLED" `:3790-3796` | `verification::pg_tests::hosted_agent_tools_agent_publishes_file_directory_and_json` 1 passed | PASS |
| O4.6 CLI listing, file/tree/JSON, refusal over real Agent execution | `wyrd_ai::cli::execution` in Rust journey | `hosted_agent_tools.rs:981-1037` (`WYRD_VERIFICATION_404_PUBLICATION_NOT_FOUND`) | `agent_publishes_file_directory_and_json` PASS | PASS |
| O4.7 route + served-schema tool identity/selection/error | route and OpenAPI tests | `crates/wyrd/wyrd-server/tests/integration/pg_verification_routes.rs:2388-2447` (`WYRD_SPEC_400_VALIDATION`, rule `agent_tools`, reason, 404 not persisted, `tool_names` read back); `pg_openapi_contract.rs:1688-1710` (`ToolCallRecord.tool` enum dotted names) | `pg_verification_routes::agent_tool_selections_validate_before_persistence` + `pg_openapi_contract::verification_contract_publishes_eleven_typed_operations_and_the_local_upload` 2 passed | PASS |
| O5.1 server: Agent content, inventory unavailable, exact reads, recovered consumer bytes, no producer replay | `pg_durable_verifier_work.rs` | `:1745-1756`, `:1771-1787` (reason "the evidence inventory cannot be read now"), `:1845-1905` | `agent_publications_reach_a_recovered_consumer` PASS | PASS |
| O5.2 lost reply replays original receipt, no new acceptance/upload | same | `:1797-1817` (evidence dir chmod 000, empty workspace, revision unchanged) | same | PASS |
| O5.3 direct disconnect: no later Agent call, no sleep | `HeldRevisions` gate | `:1960-1979` | `dropped_agent_work_makes_no_later_call` PASS | PASS |
| O5.4 Rust journey public outcomes only | rewrite of `publications_follow_shared_run_state` | `hosted_agent_tools.rs:1579-` (no sleep/flush/`server.state`/deadline expiry) | `publications_follow_shared_run_state` PASS; `grep` finds none | PASS |
| O5.5 SDK journeys compute no digests | Rust consts `hosted_agent_tools.rs:66-76`; Python `DIGESTS` `test_hosted_agent_tools.py:107`; TS `DIGESTS` `hosted-agent-tools.test.ts:236`; MCP `DESK_*_SHA256` `verification.rs:3380` | static | journeys PASS | PASS |
| O6.1 cumulative `work_budget` with replacement credit; failed record; prior file unchanged | existing `Workspace::write` | `hosted_agent_tools.rs:1505-1545` | `tool_limits_release_capacity_and_preserve_provenance` PASS; mutant FAIL | PASS |
| O7.1 `# Panics` on changed helpers; MCP imports module scope; digest helper gone | `hosted_agent_tools.rs` `builder`, `reply`; `pg_tests.rs:36-41` `invocation`; `verification.rs:24-49` | static changed-item inspection (every new/changed fn with expect/assert documents `# Panics`); the new block-local `PermissionsExt` import moved to module scope (`e1c25ff88`) | `mise run fmt`, `mise run lints` 0 | PASS |
| O8.1 grant-removal sentence routed; admission kept; limits stated | this section; TASK-002 evidence note | below | — | PASS |

Owner checks on the final candidate: `mise run fmt`, `mise run lints` 0 (one
`clone_on_copy` fixed in `3f1d378c0`); `check:deps`, `check:tenant-isolation`
0; `py:lints`, `py:typecheck`, `ruff format --check` 0; `ts:format:check`,
`ts:lints`, `ts:typecheck` 0; `git diff --check 75c9bfa9e` clean. No contract
inputs changed, so `codegen:check` was not run. Final focused reruns: server
3/3 (then 2/2 after `e1c25ff88`), Rust journey 5/5, lib 2/2, File-decision
pg_tests 1/1 after `28f07d99c`, MCP journey 1/1 after `47e8933d8`, Python grant
test 1/1, `vala-verify --lib` 60/60; `mise run lints` 0 after the last commit.

Task correction routed to `$wyrd-plan` (O8.1): the original TASK-002 sentence
about removing Invoke/File grants between calls contradicts the stronger
security authority that accepted execution authority is a snapshot. Planning
should restate it as: initial narrowed/delegated authority, later issuance,
principal revocation/cancellation, and per-call checks against the accepted
snapshot. No live grant refresh was implemented.

Deviations and limits:

- **Historical TDD order.** The original TASK-002 production change
  (`973b78e5e`) preceded its journeys. That admission stands; this remediation
  does not rewrite it. FIND-1/2/3 here have genuine RED; FIND-4/5/6 are
  verification-only.
- **Route code.** `POST /v1/cards` reports invalid selections as
  `WYRD_SPEC_400_VALIDATION` with `details.rule = "agent_tools"`; the SDK
  loader path reports `WYRD_LOADER_400_INVALID_ENVELOPE`. Both are the existing
  stable projections; the test pins each.
- **MCP invalid selection.** MCP has no declare tool, so the MCP journey refuses
  invalid fixtures through its client and proves in-run invoke and domain
  refusals over MCP. The domain case reuses the existing `tool-review`
  Verifier fixture rather than widening the publish-desk Agents.
- **Domain refusal (O4.2/O4.4).** The publish-desk tools' only domain grant is
  `verifier:run`, which every admitted caller holds; no public File-grant
  removal was invented. The invoke-granted domain-denied case is cited from the
  existing building-block journeys on both SDKs.
- **Checker.** A fresh read-only checker named O3.1 (actor/delegation unasserted)
  and O4.5 (no MCP domain refusal), plus a block-local import, an unpinned
  refusal reason, and a stale `forget` rustdoc in `vala-verify`; each was
  resolved in `0ac4a0589`, `47e8933d8`, `e1c25ff88`. A second fresh checker
  found O4.5 proven and O3.1 checking only the presence of a delegation detail;
  `28f07d99c` compares its exact value.
- **Unit-level budget proof.** `workspace_writes_private_scratch` still covers
  unsafe parents; the cumulative budget is now also proved in the real Agent
  loop (O6.1).

Non-goals stayed excluded: no live grant refresh, remote MCP/GitHub work,
sanitizer, new ledger, uploader, resolver, or scheduler. No unrelated file
changed (the two pre-existing `.agents/skills` edits are not in the candidate).
