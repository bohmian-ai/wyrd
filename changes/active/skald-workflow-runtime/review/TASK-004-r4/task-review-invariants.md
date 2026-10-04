# TASK-004 invariant review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `5cde1b48aab0d70d8686ee8fb5f2978e26cd7f58`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 13
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Remediations: `TASK-004-R1-close-accepted-job-gaps.md`,
  `TASK-004-R2-align-revision-and-source-contracts.md`, and
  `TASK-004-R3-close-step-attempt-and-import-gaps.md`
- Review method: static inspection of the complete cumulative diff, current
  source, producers, sibling consumers, lifecycle/failure paths, applicable
  authority, prior validated ledgers, and recorded evidence. No build, test,
  Cargo, or mise command was run.

`.codegraph/` is absent, so immutable Git objects and direct source inspection
were used. The candidate was
`5cde1b48aab0d70d8686ee8fb5f2978e26cd7f58` before and after this review.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence reviewed | Result |
|---|---|---|---|
| REQ-057, REQ-059, AC-030, AC-031: Workflow is neither a principal nor a WyrdState root; existing graph/loading/runtime owners remain authoritative | The host captures a verified `Caller`, pins through the server Cards owner, and executes one Skald `PreparedWorkflowRun`; shared-client hydration extends the existing owner (`components/workflow/host.rs:230-399`; `components/cards/resolve.rs`; `skald-workflow/src/workflow.rs`; `wyrd-client/src/cards/hydrate/workflow.rs`) | Recorded client-tier, shared, Wyrd, tenant-isolation, codegen, Python, and TypeScript evidence | PASS |
| REQ-014, REQ-015, REQ-029, INV-001, INV-005, AC-004, AC-009: the exact active registered graph is bounded, validated, pinned before acceptance, and executed through Skald | `Preparation::prepare` pins in one tenant transaction, applies route/tool suitability, hydrates and prepares on tracked blocking work, then `Reservation::accept` publishes; execution consumes only that prepared graph (`host.rs:290-399`; `cards/resolve.rs`; `skald-workflow/src/workflow.rs`) | Scenarios 1, 3, and 7 and recorded registration journeys | PASS |
| REQ-017, REQ-045, REQ-050, INV-019, INV-023, AC-019, AC-028: graph, input, step-result, run, terminal-reserve, and concurrency bounds are explicit and sibling services remain available | `ServerWorkflowConfig`, `GraphBounds`, and `WorkflowExecutionLimits` feed the existing owners; graph work is outside the state lock and terminal capacity is reserved in `RunLedger::new` (`config.rs`; `host.rs:308-399`; `cards/resolve.rs`; `skald-workflow/src/run.rs:55-107`) | Scenario 7 records refusal, aggregate overflow, deepest admitted graph, near-ceiling terminal outcomes, and Cards/gateway/Bifrost sibling availability | PASS |
| REQ-018–023, REQ-048, INV-013, AC-008, AC-020, AC-022: one bounded executor produces deterministic whole snapshots and no terminal snapshot has pending/running work | One `JoinSet` owns ready steps; joined outcomes settle the ledger before `RunLedger::finish` changes only still-pending steps to `Unstarted`; the server stores whole snapshots and commits terminal state after tool-owner drain (`skald-workflow/src/workflow.rs:248-380`; `run.rs:221-281`; `workflow/runs.rs:638-688`; `workflow/host.rs:244-287`) | Executor tests and Scenarios 6 and 7, including terminal races and complete snapshots | PASS |
| Revision 13 step-attempt invariant and `FIND-TASK-004-19`: every published `Running` step reserves attempt 1; interrupted active work is `Cancelled` with timestamps and never regresses to `Unstarted` | The scheduler stores 1 in the shared counter before `RunLedger::step_started` and callback publication; `step_started` records status/start/attempt together; first polling stores 1 again and later retries advance it; every interrupted/aborted join calls `step_cancelled`; `finish` rewrites only `Pending` (`workflow.rs:273-290,336-380,448-536`; `run.rs:125-136,183-189,221-250`) | R3 extends `prepared_run_keeps_its_id` to inspect every published Running snapshot and corrects `bounded_attempt_lifecycle` to prove pre-poll cancellation, attempt one, and retained timestamps; recorded exact selectors and `test:skald` pass | PASS |
| REQ-019, REQ-034A, INV-018: admission, reservation, blocking preparation, execution, and shutdown remain under one process owner | `WorkflowRuns::admit` installs a tracker token under the same table lock used to close admission; tracked blocking work remains counted after waiter cancellation; reservation acceptance/failure/drop settles key and capacity exactly once (`workflow/runs.rs:258-367,530-635`; `host.rs:255-399`) | Owner concurrency tests plus Scenarios 2 and 6 | PASS |
| REQ-030, REQ-034C, INV-013, INV-018, AC-021: scoped idempotency shares one preparation/run, never caches failure, and replay cannot replace accepted authority | `RunKey` includes tenant/principal/key; `KeyState` separates preparation from acceptance; promotion installs the run and key atomically; failure removes both and wakes all waiters (`workflow/runs.rs:47-87,275-343,563-627`; `host.rs:75-123`) | Scenario 2 covers fan-in, conflict, tenant/principal scope, waiter cancellation, shutdown, retry, disconnect, and lost response | PASS |
| REQ-032, REQ-033, AC-010: create/replay/get/cancel each authenticate, authorize, and audit before key lookup, admission, or disclosure | `WorkflowRunHost` calls canonical `audit::authorize` before admission or owned lookup; run lookup is tenant/principal qualified and malformed IDs map to the common not-found result (`workflow/host.rs:75-203`; `workflow/runs.rs:369-415`) | Scenarios 1, 3, and 6 plus recorded principals/audit lanes | PASS |
| REQ-032A, REQ-057, INV-022, AC-027: accepted authority is token-free, immutable, bounded to the pinned run, and cannot widen; later HTTP operations authorize afresh | `Preparation` owns a cloned verified `Caller`, while gateway/tool adapters derive only fresh request IDs; replay returns existing state and no bearer/refresh secret enters run storage (`workflow/host.rs:75-203,230-399`; `gateway/workflow.rs:34-131`; `workflow/tools.rs:42-107`) | Scenario 3 covers expiry, revocation, fresh request denial, pinning, replay non-replacement, no widening, and live gateway refusal; design/security authorities record the boundary | PASS |
| REQ-034, REQ-036A, REQ-038–043, INV-009–012, INV-020, AC-011A, AC-014–017: provider identity, route, fallback, deadline, cancellation, and live gateway governance survive source-to-sink projection | Adjacent-tagged `ProviderRequest` persists and binds by named variant; `ServerWyrdGatewayCaller` projects the exact variant, including distinct Gemini/Vertex branches, and delegates to the existing gateway owner with captured caller and separate cancellation (`skald-spec/src/request.rs`; `skald-spec/src/prompt.rs`; `gateway/workflow.rs`) | Recorded tagged round trips/schema generation and Scenario 5 dialect, capability, fallback, deadline, cancellation, external-binding, and stored Vertex cases | PASS |
| REQ-052, INV-006, INV-008, AC-025: only declared built-ins execute, and Cards/query reads retain captured tenant/principal/scopes while canonical owners authorize and audit | Suitability rejects unknown/duplicate names; per-Agent resolution exposes only `bifrost.query` and `cards.get`; Card reads enter `get_card_by_ref_for`; query reads enter shared `BoundedQuery` (`workflow/host.rs:431-468`; `workflow/tools.rs:37-333`) | Scenario 4 includes permissions, malformed/bounded input, SQL floor, inaccessible objects, real foreign tenant, terminal faults, and Card audit | PASS |
| INV-021 and TASK-004 query ownership: waiter loss signals a tracked owner, original deadline bounds opening/settlement, and run terminalization follows owner drain | Query owners run on `RunTools.owners`; a waiter drop guard cancels the child token; `open_cancellable` and `ResultCollector` retain/settle the same response under the original deadline; `RunTools::drain` precedes terminal commit (`workflow/tools.rs:42-107,171-215`; `query/collect.rs:259-399`; `oracle/lifecycle_controls.rs:118-168,310-337`; `workflow/host.rs:285-287`) | Query-control and collector units, Scenario 4, and forwarded cancel/deadline/pod-loss journey with recovery | PASS |
| Follower graph release follows the approved grant-stream-close contract; no leader acknowledgement or deleted graph-drain mechanism returns | `AnalyticalGraphLifecycle` retains participant grants until settle and drops them as leader-side release; followers settle asynchronously; architecture and owner docs agree (`vala-bifrost-redux/src/oracle/analytical.rs:2492-2506,2690-2784`; `architecture/bifrost-design.md:382-405`) | Forwarded Oracle journey records owner settlement, surviving baselines, and later query success | PASS |
| Shutdown closes Workflow admission and drains tracked work before dependent gateway/query/Bifrost owners under the shared remaining deadline | `WorkflowRuns::drain` closes under the table lock, signals the cancellation tree, closes the tracker, and waits only to the passed deadline; server shutdown orders this before dependent owners (`workflow/runs.rs:417-433`; `app/server.rs:744-830`) | Scenario 6 and recorded server/Bifrost shutdown evidence | PASS |
| REQ-034B, INV-013: runs are intentionally bounded, process-local, owner-qualified, retained for 24 hours, and lost on restart | `WorkflowRuns` uses only bounded in-memory maps, tenant/global ceilings, fixed retention, active-slot accounting, and terminal-only eviction; there is no durable queue/table/lease/recovery (`workflow/runs.rs:89-229,369-433,638-688`) | Scenario 6 covers eviction, expiry, restart loss, hidden-run equivalence, queued/running shutdown | PASS |
| R1/R2/R3 close `FIND-TASK-004-1` through `-20` without reopening earlier behavior | R1 corrections remain at lifecycle/query/security/tool/gateway owners; R2 retains provider-tagged examples, Revision 13 authority, bare interfaces, and grant-stream documentation; R3 closes the attempt producer and four named import sites (`skald-workflow/src/{workflow,run}.rs`; `wyrd-spec/src/card/prompt/mod.rs:264`; `oracle/lifecycle_controls.rs:7,341`; `pg_workflow_runs.rs:12`; `peer_cluster.rs:22,1795`) | Prior ledgers plus R3 recorded focused selectors, format/lint, Wyrd, Skald, and Oracle evidence | PASS |
| Non-goals and standing DRIFT direction: no Workflow principal, durable job machinery, second graph/query/audit engine, bearer renewal, arbitrary tool platform, compatibility reader, follower-release protocol, bespoke check, setting, or option | The cumulative diff composes standard existing owners and native mechanisms: `TaskTracker`, cancellation tokens, watch channels, serde adjacent tagging, JSON Schema, and existing journey fixtures. Deleted graph-drain polling/supervisor refusal remain deleted. R3 adds no mechanism or enforcement surface. | Complete changed-file inventory and cumulative diff review | PASS |

## Invariant traces

- **Published step state:** planning produces a valid index and an atomic attempt
  counter. Scheduling binds first; successful binding reserves counter value 1,
  writes the ledger's `Running`/start/attempt tuple, and publishes that whole
  snapshot before spawning. The first poll consumes the reservation as attempt
  1; retries publish higher counts only when begun. Cancellation, deadline,
  explicit abort, and pre-poll abort all rejoin through `settle`, which converts
  the already-started ledger entry to `Cancelled` and adds `ended_at`. Only
  never-scheduled `Pending` entries become `Unstarted` at finish.
- **Accepted authority:** verified authentication produces a token-free
  `Caller`; create authorizes/audits it, exact graph preparation completes, and
  acceptance captures it with the run. Gateway, Card, and query calls reuse its
  bounded tenant/principal/scope attribution while their existing owners apply
  their live decision and audit rules. Replay never replaces it, and terminal
  completion drops the prepared dependencies.
- **Run ownership:** admission installs the idempotency key, active slot,
  cancellation child, outcome channel, and tracker token under one lock. The
  preparation and its blocking work remain tracked; acceptance atomically
  transfers the slot to one queued run; nonterminal snapshots replace whole;
  tool owners drain; one terminal commit releases capacity and starts bounded
  retention.
- **Query ownership:** the Agent waiter owns only a result wait and cancellation
  guard. The tracked owner retains the stream, validates exactly one terminal
  and clean EOF, settles failures through the shared controls under the original
  deadline, and finishes before Workflow terminal commit. MCP remains the
  sibling consumer of the same bounded collector.
- **Provider identity:** authored adjacent-tagged JSON persists through Prompt
  Cards, binding, hydration, and gateway projection. The named variant, not body
  shape, selects Gemini versus Vertex, so identical native bodies cannot change
  provider or credential owner.
- **Analytical lifecycle:** the leader owns participant grant streams and
  releases them by drop; followers observe closure and asynchronously settle.
  The leader joins its own structured work but never waits for a follower
  acknowledgement. Test-side baseline observation does not add a production
  protocol.

## Prior-finding closure

- `FIND-TASK-004-1` through `-18` remain closed at their previously validated
  source owners and proof seams.
- `FIND-TASK-004-19` is closed at the invalid-state producer: the scheduler and
  ledger establish attempt one before publishing `Running`, and settlement no
  longer preserves or rewrites an active step as zero-attempt `Unstarted`.
- `FIND-TASK-004-20` is closed at all four named sites with module-scope imports
  and bare interface types. The Unix trait import is module-scoped and gated;
  no scanner, allow attribute, check, setting, or option was added.

The binding human decisions remain intact: graph-drain polling and supervisor
idle refusal stay deleted; follower release is stream close with no leader
acknowledgement; the foreign-tenant journey does not require a model step the
fixture cannot credential; and a published `Running` step reserves attempt one
and settles `Cancelled` if interrupted.

## Review findings

No material invariant findings.

## Open questions

None affecting correctness or acceptance.

## Verification notes

- This review did not run builds, tests, Cargo, or mise, as explicitly
  required. Recorded evidence was treated as a claim and checked against the
  current implementation and named assertions.
- R3 records the two exact Skald test selectors passing, plus `test:skald`,
  format, lints, the affected Wyrd packages (including all `pg_workflow_runs`
  journeys), and the Oracle journey lane. The new assertions directly cover
  published Running attempts and pre-poll cancellation; existing assertions
  retain first-success and retry-exhaustion totals.
- Earlier recorded evidence covers all seven real-server Workflow scenarios,
  owner-level reservation/blocking/query tests, forwarded Oracle
  cancel/deadline/pod-loss recovery, MCP queries, Bifrost, principals, gateway,
  language SDKs, codegen, client-tier, tenant-isolation, PyO3 scope, unwrap
  audit, format, and lints.
- The approved foreign-tenant harness limit remains explicit: real
  authentication, registry, run, Card, query, and idempotency boundaries are
  crossed, while model execution is unavailable because gateway credentials
  exist only for the fixture tenant.

## Overall result

**PASS**
