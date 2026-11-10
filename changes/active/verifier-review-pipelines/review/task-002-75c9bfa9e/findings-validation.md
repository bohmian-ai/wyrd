# TASK-002 independent findings validation

Recommended verdict: **FIX_REQUIRED**. Seven deduplicated, source-validated correction boundaries remain. No new blocker was discovered by this validator. No specification revision is required by the retained corrections.

## Subject, independence and coverage

First full review of `990a85dc7155ff0d4de1acc17e4aeaa7a42d582f...75c9bfa9e07163099eac40d5a556c4447c6a519d`; observed HEAD is the candidate. No prior TASK-002 acceptance is carried forward. Read the complete review skill, immutable subject, all nine selected discovery/follow-up reports, claim comparison and human claims. Read the original task, relevant approved baseline-18/publication/durability obligations, applicable root/directory rules, architecture authority map, security/tool/File sections, testing and maintainer/Rust guides. Unrelated draft revision-21 wait/approval provisions were excluded.

Used `mise tasks ls --hidden` and CodeGraph before code navigation. Independently inspected the cumulative production diff and actual shared receipt, acceptance, forgetting, invocation, recording, sandbox, evidence and authorization bodies/callers. Inspected the complete affected new public journey bodies and focused existing HTTP/OpenAPI/CLI/backend assertions. This is validation of the reported gaps, not another comprehensive discovery pass. Runtime commands were not rerun; implementation-recorded green checks remain supplied execution evidence. Root's independent full-range diff check is clean.

Specialist applicability is complete: behavior covers the task and all surfaces; invariant covers call/attempt/shared-state lifecycle; repo covers cross-tier mandatory rules; maintainer compares whole owners/workflows; system covers direct/queued cancellation/recovery; separate security, persistent-data and concurrency reports cover the sensitive boundaries. Their scopes and actual source/test inventories substantiate coverage. UI and provider protocol specialists are inapplicable: no UI or external authentication/provider protocol contract changes. No selected report is missing. General system PASS does not close the invariant review's specifically traced publish-then-Busy path. The focused File-domain follow-up resolves a real authority distinction and adds no extra policy.

## Proposal dispositions

| Proposal | Disposition | Source-backed resolution |
|---|---|---|
| DATA-001 | CONFIRMED | Shared `state.rs::receipt:960–975` searches producer/name before considering call ID. Same original call with a new name bypasses its old argument digest. FIND-TASK-002-1. |
| INV-002-1 | CONFIRMED | In-session code admission can return Busy after publication. Engine forget removes the invocation while leaving its publication; runner reconstructs attempts only from invocations. FIND-TASK-002-2. |
| SEC-001, FOLLOWUP-SEC-1 | REVISED, consolidated | Missing receiving File decision/audit is real; neither cross-tenant escape nor live role revocation is proved. Existing exact run admission supplies an operation-level File grant, but does not supply the later decision/audit. FIND-TASK-002-3. |
| B-001 | REVISED | Explicit surface proofs are incomplete. Preserve already-proven runtime file/tree/JSON/null and refusal assertions; add only missing descriptor/invoke/domain and thin CLI/HTTP/schema coverage. Do not repeat full storage/recovery suites. FIND-TASK-002-4. |
| B-002 | REVISED | Identical successful duplicate calls and a normal-inventory restart are not the named lost-reply/unavailable-inventory Agent seam proofs. Reuse and extend the existing owner cases, respecting test tiers. Combined with SSR-002 as FIND-TASK-002-5. |
| SSR-002 | CONFIRMED, consolidated | New SDK lifecycle proof uses forbidden internal controls and sleep; SDK digest computations violate the express public-test checklist. Correct proof placement while closing B-002, rather than adding another lifecycle harness. FIND-TASK-002-5. |
| SSR-004 | CONFIRMED | New fs.write work-budget refusal is unit-only; the public limits test refuses oversized JSON publication, not a write. FIND-TASK-002-6. |
| SSR-001 and its bounded import corrections | CONFIRMED | New/materially changed panic-capable helpers lack mandatory panic docs; the cited function imports are outside the generic-trait exception. Batch in existing correction edits. FIND-TASK-002-7. |
| SSR-003 | REVISED: confirmed historical deviation, no code-remediation finding | Production preceded the selected public RED. Task evidence explicitly admits this; current green cannot restore history. Preserve the deviation as a failed historical execution row; no fabricated RED, history rewrite or production churn. Genuine RED/GREEN applies to actual corrections. This is not an eighth behavioral defect or a new approval stage. |
| Maintainer/system/concurrency: no findings | Validated within scope | Existing owners remain in use. Their PASS results do not waive the specific defects/proof gaps above. |

## Final stable ledger

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

## Authority resolutions and excluded human concerns

* **Grant changes:** security posture:145–157 freezes accepted authority and says later grant changes do not refresh it. Existing per-call checks against that snapshot are correct. Task wording about removing Invoke/File grants between calls conflicts with stronger authority. Route that private wording/evidence correction through wyrd-plan, distinguishing initial narrowed/delegated authority, later issuance, principal revocation/cancellation and per-call accepted decisions. No new resolver, grant intersection, policy engine or automatic specification revision. If the human actually requests live role-change semantics, that would require revising approved security behavior; none is inferred here. The current named revocation journey proves narrowing/delegation and cancellation, not principal/credential revocation.
* **Scratch:** output-publication.md explicitly permits bounded code.run to generate binary scratch and output-only Agents without artificial mounts. Reading same-invocation scratch follows that operation; the owning design now states it. The candidate's catalog predicates and matching validator/runtime checks implement that approved supplement. REJECT the proposed drift/SPEC_REVISION_REQUIRED interpretation of scratch-only code.run/read access. Standalone code receives neither tool; checks cannot publish.
* **Capture:** no concrete source-proven credential disclosure or governing tool sanitization policy was proposed. Existing ContextCapture does not cover these tool arguments. Do not introduce a new capture policy, guessed secret-key filter or blanket history deletion.
* **TDD:** the production-before-RED history is confirmed and must remain visible. It cannot be closed by rerunning a base test or a new approval flow. Record current evidence accurately and genuine focused RED/GREEN for retained behavioral fixes; do not relabel the historical sequence.
* **Whole-workflow duplication:** independently compared loop→RecordedTool→Invocation→Publications→Workspace freeze/File completion→RunState→TenantEvidence mount, including sibling invoke tools, extract/write, direct/queued composition and storage callers. New invoke_call supplies missing provider identity through the existing loop; Role carries context; record consolidates two entry points; HostedTools shares the original RunState; write extends the existing lock/budget owner. No duplicate registry, snapshot, store, scheduler or recovery workflow is established. REJECT optional simplifications or abstraction removal lacking a material defect.
* **Evidence labels:** the original task's all-PASS table is not an acceptance verdict. Preserve it immutable and use this reconciled matrix; do not rewrite the original packet to conceal gaps.

## Reconciled acceptance and closure recommendations

| Obligation | Reconciled result | Evidence / remaining closure |
|---|---|---|
| Typed write, safe parents/paths, atomic regular-file replacement, shared writer/budget and isolated scratch | PASS implementation; required public budget proof FAIL | Workspace write and tests; real scratch assertions; FIND-6 |
| Publish one source, null, logical name, host producer; arbitrary file/tree names and immutable snapshots | PASS | Invocation args/dispatch; Publications/Workspace owners; actual public receipt/manifest/content assertions |
| Replay/conflicting original call; distinct fresh attempts and conserved recovery bounds | FAIL | FIND-1, FIND-2; identical replay and ordinary retry proofs otherwise valid |
| Dedicated tool then called-domain authorization/audit under accepted authority | FAIL File path boundary | FIND-3; other resource decisions and snapshot policy remain valid |
| Descriptor selection before model, identity, no check publication/no standalone interface | PASS implementation/Rust proof; mirrored proof FAIL | Catalog/validator/ToolSession and selected-tools assertions; FIND-4 |
| Direct/queued share state; exact descriptors; missing publication blocks consumer; cancellation fences | PASS for inspected implementation and ordinary paths | Shared Arc, TenantEvidence published body, NO-PUBLISH/cancellation/readback assertions |
| Lost reply, unavailable inventory, recovered Agent consumer and disconnect evidence at correct tier | FAIL proof | FIND-5; Task-001 source/tests retained as foundation evidence |
| Equivalent Python/TS/CLI/MCP and HTTP/OpenAPI closure | FAIL proof | FIND-4; existing happy-path typed content and backend route/page assertions carry into remediation |
| Bounded records, paging, receipts/digests; SDK proof conventions | PASS implementation; proof convention FAIL | RecordedTool full body/paged-read assertions; FIND-5 |
| No second lifecycle/store/loop; thin language projections; no remote integration acquisition | PASS | Whole-workflow comparison and full changed-owner diff; no manifest/dependency changes |
| Changed-item rustdoc/import rules | FAIL | FIND-7 |
| Original TDD sequence | Historical FAIL/deviation | SSR-003; irreparable history, no fabricated test fix |
| Format/lints/codegen/deps/tenant/targeted execution claims | Supplied green evidence, not reviewer reruns | Root independently checked diff; docs:check and ts:napi:check have no recorded result and remain evidence limits, not asserted failing commands |

Package one bounded remediation task around FIND-TASK-002-1 through -7; route the invalid private authority sentence to the planning owner separately without mutating this review's authority. Production changes stay within RunState and the existing receiving publication boundary; most remaining work is focused proof repair. Require true focused RED/GREEN for new behavioral corrections, static documentation closure, and only named changed tests/owning narrow checks. Full multi-surface suites and broad aggregates belong at final change review, not this remediation.

No previous finding closures or convergence assessment apply on this first review. No unresolved authority dispute requires BLOCKED, and none of the retained corrections changes the approved public/persisted/resource-ownership contract.

Packet SHA-256 at validation, rechecked unchanged after report creation (HEAD also unchanged): spec `5af4975a59d8aaba3702e5c2fde40d34b074760456a293cf61f30cd907a5e683`; task `7c1773159a2d348d097a46fd5876f748bb32d1569447b102b664657117690bbe`; publication supplement `a02ceaf9c6f40f07e5995925161c64dc38fb7563471e993d3212810533b773e9`; durability supplement `08c4bf050e2f73cf5bb4eabcd91d412c3e73cf727b1a415165d504e9cb804d9f`.

Skipped runtime reruns and broad suites; source and authority stayed immutable. Remaining risks are the retained replay/attempt-integrity and File-audit defects plus explicit proof gaps.
