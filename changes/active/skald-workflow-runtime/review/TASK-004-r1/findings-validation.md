# Structured Ponytail Validation — TASK-004

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `96e993a16706d2fb759e4cdb7371ff490b198a35`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, revision 12
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Review mode: source, cumulative diff, applicable authority, discovery reports, follow-up report, and recorded evidence only. No build, test, Cargo, or mise command was run.

The candidate identity was rechecked after validation and remained unchanged. `.codegraph/` is absent, so repository source and Git were used directly.

## Validation method and coverage

The complete base-to-candidate changed-file inventory and cumulative diff were inspected. For each proposed finding, the cited producer, consumer, sibling consumers, full owner body, reachable callers, governing requirement, and named evidence were checked. Corrections were reduced through the required ladder: delete the candidate behavior when possible, otherwise reuse an existing Wyrd owner, then a standard/native mechanism, then an installed dependency. No retained correction adds a new service, framework, queue, transport, configuration option, standalone check, or bespoke lifecycle mechanism.

The human standing direction controls this ledger. In particular, the deleted Oracle byte-polling and release-refusal mechanism is not restored: it is neither native structured ownership nor a conventional comparable-project mechanism, and source tracing did not establish executable work surviving the existing attempt/exchange/participant joins.

## Proposal-by-proposal validation

| Discovery proposal | Validation | Final disposition |
|---|---|---|
| `DCL-001` | The reservation is inserted by `WorkflowRuns::admit` before `WorkflowRunHost::create` calls `WorkflowRuns::spawn`. Concurrent `drain` can cancel admission, close an empty tracker, and complete before the late spawn. `TaskTracker::close` does not reject later spawns. | **CONFIRMED** as `FIND-TASK-004-1`. |
| `DCL-002` | The tracked preparation awaits a bare `tokio::task::spawn_blocking`. Cancellation drops its `JoinHandle`, which detaches already-started blocking work from the tracker shutdown waits on. | **CONFIRMED** as `FIND-TASK-004-2`. |
| `QSET-001` | `open_cancellable` receives no absolute query deadline. After cancellation wins it awaits remote lifecycle fanout, whose peers each use a fresh three-second bound, and then awaits the same open without an outer original-deadline bound. | **CONFIRMED** as `FIND-TASK-004-3`. |
| `QSET-002` | The pod-loss branch skips both post-terminal baseline recovery and a later query, and accepts only a broad `WYRD_VALA_*` class. The task expressly requires post-loss recovery and honest settlement proof. | **CONFIRMED** as `FIND-TASK-004-4`. |
| `SEC-TEN-1` | No second authenticated tenant is created in `pg_workflow_runs.rs`; a random foreign binding tenant and a same-tenant second principal do not cross the credential-derived tenant boundary. | **REVISED** and consolidated as `FIND-TASK-004-5`. |
| `MAINT-001` | The eight new concrete `AgentTool` declaration methods have no item rustdoc. The repository expressly requires rustdoc for new and materially modified trait implementation methods. | **REVISED** only to deduplicate with `STD-004-002`; retained as `FIND-TASK-004-6`. |
| `STD-004-001` | The deleted mechanism polled aggregate reserved bytes and rejected release; it joined no task, stream, transport, or executable owner. Attempts, drivers, exchanges, participants, worker, and cache are settled separately, while a late memory reservation remains charged through the shared reference-counted memory root. No reachable surviving CPU/IO path was proved. | **REJECTED**. Restoring the byte poll/refusal and its mechanism-specific tests would be unsupported bespoke drift. |
| `STD-004-002` | Same directly visible documentation violation as `MAINT-001`. | **REVISED** and deduplicated into `FIND-TASK-004-6`. |
| `STD-004-003` | Both built-ins advertise only `{ "type": "object" }` despite returning known JSON contracts. `AgentTool` defines `output_schema` as the schema of returned values, repository agent authority requires typed outputs, and TASK-004 requires exact built-in schemas. | **CONFIRMED** as `FIND-TASK-004-7`. |
| `SYS-004-001` | Source does not show executable query descendants surviving the existing attempt/driver/exchange/participant settlement. Late reference-owned memory remains charged to the governed root; slot admission and memory accounting are separate bounded controls. | **REJECTED** for the same source-backed reason as `STD-004-001`. No polling, new readiness guard, or capacity owner is required. |
| `BEH-004-001` | The named admission journey omits inactive and cross-tenant roots; the authority journey proves token expiry and revocation but not pinned Card mutation, no widening to a newly granted resource/replay, or live gateway refusal. These cases are explicit in AC-004, AC-009, and AC-027. | **REVISED**: cross-tenant proof joins `FIND-TASK-004-5`; the remaining pinned/captured-authority proof is `FIND-TASK-004-8`. |
| `BEH-004-002` | Scenario 2 covers same-principal fan-in, conflict, principal scope, failed-preparation retry, and creator disconnect. It does not cover tenant scope, waiter cancellation, shutdown waking all waiters once, or lost-acceptance recovery with observation/provider counts. AC-021 and the task require them. | **CONFIRMED** as `FIND-TASK-004-9`. |
| `BEH-004-003` | The Agent journey covers both happy paths, malformed/unknown arguments, non-SELECT SQL, a result ceiling, an unavailable declaration, and one under-scoped same-tenant caller. It omits the task's remaining bound, inaccessible-object, independent-permission, and malformed/partial-terminal Agent paths. | **REVISED**: second-tenant proof joins `FIND-TASK-004-5`; remaining cases are `FIND-TASK-004-10`. |
| `BEH-004-004` | The new in-process adapter has five projection/decoding branches plus fallback/deadline/cancellation behavior. The server journey executes only one OpenAI-shaped governed route and one OpenAI-compatible external route. AC-011A and AC-014–017 expressly require in-process fallback and protocol/Vertex proof. | **CONFIRMED** as `FIND-TASK-004-11`. |
| `BEH-004-005` | The lifecycle journey covers cancel, timeout, one cancel/completion race, tenant retention, expiry, hidden-run equivalence, restart loss, running shutdown, and a preparation already registered with the tracker. It omits the other explicit AC-022 races and global/queued cases. | **REVISED** to exclude separately diagnosed tracker ownership defects; retained as `FIND-TASK-004-12`. |
| `BEH-004-006` | Scenario 7 proves step/edge/body/input/step-result bounds plus Cards and gateway sibling service. It does not prove an admitted deep graph, sibling Bifrost service, aggregate-run overflow, or near-ceiling failure/cancel/deadline terminal reserve required by AC-028. | **CONFIRMED** as `FIND-TASK-004-13`. |
| `INVREV-001` | REQ-032A explicitly says the accepted-job boundary must be reflected in security authority before completion. The candidate changes neither `wyrd-security-posture.md` nor `wyrd-design.md`, while the former's current request/token text does not describe the accepted-run exception and limits. | **CONFIRMED** as `FIND-TASK-004-14`. |

The focused follow-up was necessary because discovery reports materially conflicted over Oracle graph release. Its resolution is accepted after independent source tracing: executable lifecycle owners are joined, the removed mechanism observed only reference-owned byte residue, and its restoration would violate the standing DRIFT direction.

## Final deduplicated finding ledger

### FIND-TASK-004-1 — CONFIRMED — VIOLATION: reservation publication can outrun preparation tracking

- **Discovery sources:** `DCL-001`
- **Violated obligation:** Spec normative create steps 6 and 11, REQ-034A, INV-018, and TASK-004 Scenario 2 require the tracked preparation future to own the reservation before the handler can suspend and require shutdown to drain all preparations.
- **Location:** `crates/wyrd/wyrd-server/src/components/workflow/host.rs:98-112`; `crates/wyrd/wyrd-server/src/components/workflow/runs.rs:263-328,331-338,388-399`.
- **Evidence and reachability:** `admit` publishes `KeyState::Preparing` and consumes capacity, returns to `create`, and only then calls `spawn`. `drain` can run in that interval, cancel admission, close an empty tracker, and return clean. The late spawn is accepted by `TaskTracker` and runs after the claimed drain.
- **Observable consequence:** shutdown can proceed to gateway/Bifrost teardown while a preparation reservation and late preparation still exist; waiter publication and capacity release occur after the owner reported itself drained.
- **Decision-complete correction:** make reservation publication and registration with the existing `WorkflowRuns.tasks` one coordinated `WorkflowRuns` owner operation. The tracked preparation must exist before its reservation is observable; if shutdown wins, install no reservation and return the existing unavailable error. Reuse the current table lock, `TaskTracker`, cancellation token, and preparation outcome. Do not add an actor, queue, option, lifecycle service, or custom check.
- **Focused closure proof:** an owner-level concurrency test must show that drain cannot complete between reservation installation and task ownership, and that the shutdown-losing admission publishes unavailable and releases/wakes exactly once. The existing server shutdown journey remains the end-to-end proof.

### FIND-TASK-004-2 — CONFIRMED — VIOLATION: blocking preparation is detached from Workflow shutdown ownership

- **Discovery sources:** `DCL-002`
- **Violated obligation:** REQ-034A, INV-018, and TASK-004's bounded preparation/shutdown ordering require all Workflow preparation work to remain owned by the Workflow drain under the one process deadline.
- **Location:** `crates/wyrd/wyrd-server/src/components/workflow/host.rs:262-266,299-388`; `crates/wyrd/wyrd-server/src/components/workflow/runs.rs:388-399`.
- **Evidence and reachability:** cancellation of `Preparation::run` drops the future awaiting bare `tokio::task::spawn_blocking`; an already-started blocking closure is detached, retains the graph/dependencies/tools/state, and is absent from `WorkflowRuns.tasks`.
- **Observable consequence:** Workflow drain may report success while preparation CPU and server dependencies survive outside the shared shutdown budget.
- **Decision-complete correction:** spawn the blocking closure through the existing `WorkflowRuns` `TaskTracker` (`TaskTracker::spawn_blocking` is already supplied by the installed native lifecycle dependency) and keep it counted even when the outer preparation is cancelled. It must not accept/publish after cancellation, but clean drain cannot complete until it ends.
- **Focused closure proof:** deterministically hold the blocking preparation, cancel/drain Workflow, and prove clean drain waits until release while the reservation and waiters still settle exactly once.

### FIND-TASK-004-3 — CONFIRMED — INCORRECT: cancellation during query open can exceed the original deadline

- **Discovery sources:** `QSET-001`
- **Violated obligation:** TASK-004 query ownership across step abort and INV-021 require opening cancellation and response retention to remain under the original query deadline; shutdown may shorten, never restart, that budget.
- **Location:** `crates/wyrd/wyrd-server/src/oracle/lifecycle_controls.rs:118-140,279-305`; `crates/wyrd/wyrd-server/src/oracle/lifecycle_transport.rs:18-20,127-152,234-258`; callers in `query/collect.rs:228-256` and `query/scheduled.rs:132-178`.
- **Evidence and reachability:** `open_cancellable` has no deadline parameter. Cancellation awaits `cancel_owner`, whose remote peers each receive a fresh three-second timeout, and then awaits `open` without an original-deadline `timeout_at`. A query with less time remaining can therefore keep the Workflow owner and capacity beyond its accepted bound.
- **Observable consequence:** cancellation, deadline, or shutdown while a forwarded Analytical stream is opening can overrun the run/query deadline and delay terminal commit/capacity release.
- **Decision-complete correction:** pass the already-existing absolute request/query deadline into `open_cancellable` and bound the whole cancel-during-open phase with it, including remote signalling and retention of the same open future. On expiry return the existing honest incomplete/unavailable/timeout outcome. Do not add a new timeout setting, retry, transport, or cleanup owner.
- **Focused closure proof:** extend the existing lifecycle-control unit seam with a delayed cancellation owner whose ordinary transport bound exceeds the remaining query deadline; prove one cancellation attempt, the same open future while budget remains, and completion at the original deadline without a fresh budget.

### FIND-TASK-004-4 — CONFIRMED — MISSING: forwarded pod-loss evidence skips recovery and exact settlement proof

- **Discovery sources:** `QSET-002`
- **Violated obligation:** TASK-004 Scenarios 4 and 6 require honest owner-loss settlement, owner completion before Workflow capacity release, post-loss baseline recovery, and later sibling availability.
- **Location:** `crates/wyrd/wyrd-testing/tests/bifrost/oracle/workflow.rs:139-157,220-249`.
- **Evidence:** the `PodKill` branch skips the baseline loop and later ordinary query under `if cause != TerminalCause::PodKill`; its error assertion accepts any `WYRD_VALA_*` value with no columns.
- **Observable consequence:** stranded surviving-owner capacity/readiness or an incorrect generic failure classification can pass the named journey.
- **Decision-complete correction:** extend the existing `PeerCluster` pod-loss branch, with no new fixture/probe/setting, to wait for surviving owners' expected baseline, execute the later query through the surviving topology, and assert the exact existing settlement class for the lost forwarded owner while proving no rows reach the model.
- **Focused closure proof:** the existing `workflow_forwarded_query_settles_before_the_run_ends` selector must fail on stranded capacity, unavailable later query, partial rows, or the wrong stable owner-loss result.

### FIND-TASK-004-5 — REVISED — MISSING: no real second-tenant journey crosses the security boundary

- **Discovery sources:** `SEC-TEN-1`, part of `BEH-004-001`, part of `BEH-004-003`
- **Violated obligation:** AC-009, AC-018, AC-025, and REQ-032 require real cross-tenant root refusal, indistinguishable run lookup/cancel, and no cross-tenant tool data.
- **Location:** `crates/wyrd/wyrd-server/tests/pg_workflow_runs.rs:855-1083,1096-1207,1316-1489,1604-1796`.
- **Evidence:** the file provisions only the server fixture's tenant. Same-tenant principals and a binding configured with a random foreign tenant UUID do not exercise credential-to-tenant derivation, tenant connection selection, or tenant-qualified run/tool paths.
- **Observable consequence:** a regression in verified tenant routing can leave every recorded TASK-004 journey green while exposing another tenant's Workflow, run, Card, or Bifrost data.
- **Decision-complete correction:** use the repository's existing second-tenant provisioning/authentication path in the current server journeys. Prove that the second tenant cannot submit the first tenant's exact active Workflow without upstream/tool work; its get/cancel return the same stable body as unknown; and its built-in Card/query calls reveal no first-tenant object or row. Reuse current clients, registry, Cards, Bifrost, and auth fixtures; add no tenant simulator, option, security gate, or standalone check.
- **Focused closure proof:** extend the existing Scenario 1, 4, and 6 selectors with those authenticated second-tenant cases and preserve their provider/audit/no-data assertions.

### FIND-TASK-004-6 — REVISED — VIOLATION: concrete built-in declaration methods lack mandatory rustdoc

- **Discovery sources:** `MAINT-001`, `STD-004-002`
- **Violated obligation:** `AGENTS.md` §16 and `architecture/agent-rules.md` require substantive rustdoc for every new or materially modified Rust item, including trait implementation methods.
- **Location:** `crates/wyrd/wyrd-server/src/components/workflow/tools.rs:147,151,155,159,238,242,246,261`.
- **Evidence:** both concrete `AgentTool` implementations document `invoke`, but not their `name`, `description`, `input_schema`, or `output_schema` items.
- **Observable consequence:** the provider-visible declarations fail a hard repository rule and leave their concrete stability/shape implicit at the change site.
- **Decision-complete correction:** add concise item rustdoc to the eight existing methods describing the concrete returned declaration and relevant invariant. Add no helper, wrapper, lint allowance, check, or documentation file.
- **Focused closure proof:** source inspection plus the existing recorded rustdoc/lint lanes; no new test or check is warranted.

### FIND-TASK-004-7 — CONFIRMED — INCORRECT: built-in output schemas erase known result contracts

- **Discovery sources:** `STD-004-003`
- **Violated obligation:** `AgentTool::output_schema`, agent-harness Tool Contracts, TASK-004 Scenario 4, and AC-025 require exact typed built-in tool contracts.
- **Location:** `crates/wyrd/wyrd-server/src/components/workflow/tools.rs:159-161,261-263`; actual query result at `query/collect.rs:287-604`; actual Card result at `tools.rs:271-289`.
- **Evidence:** `bifrost.query` and `cards.get` both advertise only an unconstrained object, although one returns the closed `{columns, rows, terminal}` shape and the other the canonical Card envelope.
- **Observable consequence:** agent/schema consumers accept shapes the implementation never returns and cannot reason from the declaration about the actual values.
- **Decision-complete correction:** project the existing collector result and Card-envelope schema owners into these two `output_schema` methods using the repository's existing JSON Schema facilities. Do not introduce a parallel contract type solely for generation, a new generator, or a new check; if a narrow standard JSON Schema projection is required for dynamic row values, keep it in the existing tool owner and match actual serialization exactly.
- **Focused closure proof:** focused assertions beside the existing tool tests must compare required fields, closure, and Card envelope shape against the concrete declarations and invocation results.

### FIND-TASK-004-8 — REVISED — MISSING: admission and accepted-authority journeys omit explicit pinned/live-governance cases

- **Discovery sources:** remaining part of `BEH-004-001`
- **Violated obligation:** REQ-029, REQ-032A, AC-004, AC-009, AC-027, and TASK-004 Scenarios 1 and 3.
- **Location:** `crates/wyrd/wyrd-server/tests/pg_workflow_runs.rs:855-1083,1209-1300`.
- **Evidence:** admission does not submit an inactive root. The authority journey proves token expiry and grant revocation, but not Card mutation/deactivation after acceptance, denial of a newly granted resource under captured scopes, replay non-widening while execution is active, or current gateway deployment/credential refusal.
- **Observable consequence:** the required distinction between pinned graph/captured authority and live gateway admission is not proven end to end.
- **Decision-complete correction:** extend the two existing journeys using current registry, role/token, gateway, and deterministic upstream controls: refuse an inactive root before work; mutate/deactivate a dependency after acceptance without changing the pinned run; prove a resource outside the accepted scope stays denied and replay does not widen it; and make a later gateway call fail under removed/invalid current eligibility or credentials. Add no policy cache, refresh mechanism, gateway substitute, or test framework.
- **Focused closure proof:** the existing `admission_is_audited_and_side_effect_free_on_refusal` and `accepted_authority_outlives_submission_only` selectors must directly assert those cases and unchanged/no-upstream boundaries.

### FIND-TASK-004-9 — CONFIRMED — MISSING: idempotency/preparation journey does not close AC-021

- **Discovery sources:** `BEH-004-002`
- **Violated obligation:** REQ-030, REQ-034C, INV-018, AC-021, and TASK-004 Scenario 2.
- **Location:** `crates/wyrd/wyrd-server/tests/pg_workflow_runs.rs:1085-1207`.
- **Evidence:** the journey omits tenant-key isolation, cancellation of one matching waiter while peers survive, shutdown waking all preparation waiters exactly once, and lost-acceptance-response recovery with accepted-observation/provider-call counts.
- **Observable consequence:** duplicate work, stranded callers, or incorrect scoped deduplication can pass despite equivalent final output.
- **Decision-complete correction:** extend the current Scenario 2 journey with existing auth tenants, preparation gate, request cancellation, shutdown path, audit/observation inspection, and upstream counters. Do not add a scheduler, durable deduplication, or a separate idempotency harness.
- **Focused closure proof:** the existing `tracked_preparation_replay_and_disconnect` selector must prove all AC-021 cases and exact counts.

### FIND-TASK-004-10 — REVISED — MISSING: Agent tool journey omits required bound, object-denial, and terminal-negative paths

- **Discovery sources:** remaining part of `BEH-004-003`
- **Violated obligation:** REQ-052, INV-006, INV-008, INV-022, AC-020, AC-025, and TASK-004 Scenario 4.
- **Location:** `crates/wyrd/wyrd-server/tests/pg_workflow_runs.rs:1302-1489`; supporting forwarded journey `crates/wyrd/wyrd-testing/tests/bifrost/oracle/workflow.rs`.
- **Evidence:** the real Agent path omits oversized SQL and numeric limits, inaccessible Card/table objects, callers independently lacking one tool permission, and malformed terminal/partial-row failure. The `tenant` argument case proves closed JSON input only.
- **Observable consequence:** regressions in Agent-path bounds, object authorization, or no-partial-result integrity can pass the named journey.
- **Decision-complete correction:** extend Scenario 4 through the existing Cards/query owners, controlled model, query fault seams, and data fixtures. Do not add a second decoder, query engine, permission layer, or tool harness.
- **Focused closure proof:** the existing `declared_tools_use_captured_scopes_and_owned_services` selector must assert stable redacted failures, no unauthorized/partial data, and the existing audit path for each required negative.

### FIND-TASK-004-11 — CONFIRMED — MISSING: the new in-process gateway adapter lacks required direct protocol and fallback proof

- **Discovery sources:** `BEH-004-004`
- **Violated obligation:** REQ-034, REQ-036A, REQ-038/039/041/042/043, INV-010/012/020, AC-011A, AC-014–017, and TASK-004 Scenario 5.
- **Location:** `crates/wyrd/wyrd-server/src/components/gateway/workflow.rs:81-250`; `crates/wyrd/wyrd-server/tests/pg_workflow_runs.rs:1492-1602`.
- **Evidence:** `NativeCall::project` and `Answer::decode` have distinct OpenAI Chat, OpenAI Responses, Anthropic, Gemini, and Vertex branches, but the server journey exercises one OpenAI governed route and one OpenAI-compatible external binding; it does not prove in-process fallback, remaining timeout, cancellation, or internal Vertex success.
- **Observable consequence:** adapter-specific model placement, dialect projection/decoding, fallback isolation, or deadline/cancellation can regress while public gateway tests and the one OpenAI server case stay green.
- **Decision-complete correction:** extend the existing server route journey with current deterministic gateway fixtures to directly exercise each adapter branch required by AC-015, internal Vertex success, pre-upstream incompatibility refusal, stored fallback isolation, remaining deadline, and separate cancellation. Reuse `GatewayInvocation` and current provider fixtures; add no transport, dialect registry, option, or compatibility surface.
- **Focused closure proof:** the existing `server_routes_keep_gateway_and_external_ownership` selector must observe exact upstream protocol/model/fallback behavior and refusal/cancellation counts.

### FIND-TASK-004-12 — REVISED — MISSING: lifecycle journey omits explicit AC-022 races and global/queued shutdown cases

- **Discovery sources:** remaining part of `BEH-004-005`
- **Violated obligation:** REQ-018–023, REQ-034A–C, INV-013/018/019, AC-018, AC-022, and TASK-004 Scenario 6.
- **Location:** `crates/wyrd/wyrd-server/tests/pg_workflow_runs.rs:1604-1796`.
- **Evidence:** the journey omits completion versus deadline, GET during snapshot replacement, global oldest-first eviction, shutdown of an accepted queued run, and terminal snapshot inspection after shutdown. Its preparation shutdown case begins only after the task reached the existing gate, so it also cannot close `FIND-TASK-004-1` or `-2`.
- **Observable consequence:** status regression/partial snapshot, incorrect global retention, or queued/shutdown terminalization can escape the acceptance gate.
- **Decision-complete correction:** extend the current deterministic lifecycle journey with existing run gates, retention configuration, and real `BoundServer` shutdown path. Keep tracker ownership corrections in their owners; do not add an actor, clock service, durability layer, or alternate shutdown budget.
- **Focused closure proof:** the existing `lifecycle_races_retention_and_shutdown` selector must assert every AC-022 race/case and complete terminal snapshots.

### FIND-TASK-004-13 — CONFIRMED — MISSING: graph/snapshot proof omits deep admission, Bifrost sibling service, aggregate overflow, and terminal reserve

- **Discovery sources:** `BEH-004-006`
- **Violated obligation:** REQ-017, REQ-045, REQ-050, INV-019/023, AC-028, and TASK-004 Scenario 7.
- **Location:** `crates/wyrd/wyrd-server/tests/pg_workflow_runs.rs:1798-1934`.
- **Evidence:** the journey covers step/edge/resolved-byte/input/step-result bounds and Cards/gateway siblings, but not a deep admissible graph, sibling Bifrost traffic, aggregate `max_run_bytes` overflow, or near-ceiling failure/cancellation/deadline terminal reserve.
- **Observable consequence:** stack growth, Bifrost starvation, aggregate accounting errors, or an inability to form complete terminal snapshots can pass.
- **Decision-complete correction:** extend Scenario 7 using the existing graph builders, server Bifrost fixture, run limits, and deterministic cancellation/deadline controls. Do not add a CPU pool, scheduler, transport surrogate, setting, or synthetic host load.
- **Focused closure proof:** the existing `graph_and_snapshot_limits_preserve_sibling_services` selector must prove deep-but-valid admission, all three sibling surfaces, aggregate overflow, and complete bounded terminals for failure/cancel/deadline near the limit.

### FIND-TASK-004-14 — CONFIRMED — MISSING: accepted-job authority is absent from governing design/security authority

- **Discovery sources:** `INVREV-001`
- **Violated obligation:** REQ-032A explicitly requires the accepted-job boundary in security authority before completion; Wyrd authority must describe durable public/security behavior.
- **Location:** `architecture/wyrd-design.md`; `architecture/wyrd-security-posture.md:176-183`; implementation at `components/workflow/host.rs:75-203,230-388`.
- **Evidence:** the candidate implements token-free captured execution authority and fresh later-request authorization, but neither governing authority describes that an accepted run intentionally survives token expiry/grant change, is bounded to its run/graph/deadline, cannot widen, and retains owner-specific gateway/tool authorization.
- **Observable consequence:** operators and maintainers following governing authority can incorrectly treat revocation/current-request rules as stopping an already accepted job or can miss its bounded exception.
- **Decision-complete correction:** update the existing `wyrd-design.md` and `wyrd-security-posture.md` sections—no new document—to state the accepted-job lifetime, token/secret non-retention, no-widening boundary, fresh authentication for later HTTP requests, and continued gateway/Card/Bifrost owner authorization/audit.
- **Focused closure proof:** source review must show both existing authorities agree with REQ-032A; `accepted_authority_outlives_submission_only` remains the behavioral proof after `FIND-TASK-004-8` closes its missing cases.

## Validated ledger summary

| Stable ID | Status | Classification |
|---|---|---|
| `FIND-TASK-004-1` | CONFIRMED | VIOLATION |
| `FIND-TASK-004-2` | CONFIRMED | VIOLATION |
| `FIND-TASK-004-3` | CONFIRMED | INCORRECT |
| `FIND-TASK-004-4` | CONFIRMED | MISSING |
| `FIND-TASK-004-5` | REVISED | MISSING |
| `FIND-TASK-004-6` | REVISED | VIOLATION |
| `FIND-TASK-004-7` | CONFIRMED | INCORRECT |
| `FIND-TASK-004-8` | REVISED | MISSING |
| `FIND-TASK-004-9` | CONFIRMED | MISSING |
| `FIND-TASK-004-10` | REVISED | MISSING |
| `FIND-TASK-004-11` | CONFIRMED | MISSING |
| `FIND-TASK-004-12` | REVISED | MISSING |
| `FIND-TASK-004-13` | CONFIRMED | MISSING |
| `FIND-TASK-004-14` | CONFIRMED | MISSING |

No retained correction requires a new material product, public API, architecture, security, compatibility, cross-service, concurrency-semantics, resource-ownership, or persistent-data decision. The bounded implementation and proof work fits the approved revision. The validated ledger is therefore non-empty and remediation is decision-complete within the approved task.

## Verification limits

- No build, test, Cargo, or mise command was run, per the review instruction.
- Recorded candidate evidence was treated as a claim and checked against what the named tests actually assert.
- The Oracle byte-residue dispute was resolved from source ownership and authority, not downgraded to a verification limit.
- No required report or source was missing, and no unresolved reviewer disagreement remains.
