# TASK-004 invariant review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `d4d4e2da53abfc677abdb804e71517c3b6849f49`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 13
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Remediations: TASK-004 R1 through R4 in the preceding review directories
- Review method: static inspection of the complete cumulative diff, current
  source, producers, sibling consumers, lifecycle and failure paths,
  applicable authority, prior validated ledgers, and recorded evidence. No
  build, test, Cargo, or mise command was run.

`.codegraph/` is absent, so immutable Git objects, `rg`, and direct source
inspection were used. The candidate resolved to
`d4d4e2da53abfc677abdb804e71517c3b6849f49` before this report was written.

## Navigation map

| Producer or owner | State or value | Main consumers and proof seams |
|---|---|---|
| `WorkflowRunHost` / `Preparation` | authenticated caller, request hash, timeout, pinned graph, execution dependencies | `WorkflowRuns::admit`, Cards pinning, `Workflow::prepare`, `Reservation::accept`, server run journeys |
| `WorkflowRuns` / `Reservation` / `AcceptedRun` | scoped key, active capacity, preparation ownership, snapshots, cancellation, retention | create waiters and replay, GET/cancel, shutdown drain, lifecycle journeys |
| `WorkflowExecutor` / `RunLedger` | one prepared deadline, attempt counters, ready/running steps, deterministic terminal snapshot | `PreparedWorkflowRun`, transition observer, server terminal commit, executor tests |
| `RunTools` / `QueryTool` | captured caller, query-owner tracker, shared one-time-bound prepared deadline | `BoundedQuery`, `RunningQueryControls`, Oracle, built-in-tool and forwarded-query journeys |
| `ServerWyrdGatewayCaller` | provider-tagged request, stored fallback, remaining timeout, cancellation, correlation | `GatewayInvocation`, provider dialect journeys, live gateway authorization and admission |
| Analytical lifecycle | participant grant streams and graph settlement | follower stream-close release, running-query terminal, forwarded-query recovery |

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence reviewed | Result |
|---|---|---|---|
| REQ-057, REQ-059, AC-030, AC-031: Workflow is neither a principal nor a WyrdState root; existing graph/loading/runtime owners remain authoritative | The host captures a verified `Caller`, pins through the server Cards owner, and executes one Skald `PreparedWorkflowRun`; shared-client hydration extends the existing owner (`components/workflow/host.rs:75-123,290-400`; `components/cards/resolve.rs`; `skald-workflow/src/workflow_surface.rs:556-597`) | Recorded client-tier, shared, Wyrd, tenant-isolation, codegen, Python, and TypeScript evidence | PASS |
| REQ-014, REQ-015, REQ-029, INV-001, INV-005, AC-004, AC-009: the exact active registered graph is bounded, validated, pinned before acceptance, and executed through Skald | `Preparation::prepare` pins in one tenant transaction, applies route/tool suitability, hydrates and prepares on tracked blocking work, then `Reservation::accept` publishes; execution consumes only the prepared graph (`host.rs:264-287,309-400`; `cards/resolve.rs`; `workflow_surface.rs:572-597`) | Scenarios 1, 3, and 7 and recorded registration journeys | PASS |
| REQ-017, REQ-045, REQ-050, INV-019, INV-023, AC-019, AC-028: graph, input, step-result, run, terminal-reserve, and concurrency bounds are explicit and sibling services remain available | Server config and graph/execution bounds feed their existing owners; graph work is outside the state lock and `RunLedger::new` reserves terminal capacity (`config.rs`; `host.rs:318-399`; `run.rs:56-107`) | Scenario 7 records refusal, aggregate overflow, deepest admitted graph, near-ceiling terminal outcomes, and Cards/gateway/Bifrost sibling availability | PASS |
| REQ-018–023, REQ-048, INV-013, AC-008, AC-020, AC-022: one bounded executor produces deterministic complete snapshots and no terminal snapshot has pending/running work | One `JoinSet` owns ready steps; joined outcomes settle the ledger; only pending steps become `Unstarted`; tool owners drain before the server commits terminal state (`workflow.rs:251-385`; `run.rs:221-280`; `host.rs:244-287`; `workflow/runs.rs:638-688`) | Executor evidence and Scenarios 6 and 7, including terminal races and complete snapshots | PASS |
| R3 / `FIND-TASK-004-19`: every published `Running` step reserves attempt one; interrupted published work is `Cancelled` with retained timestamps | Scheduling stores attempt one before `step_started` and publication; settlement converts interrupted/aborted work to `Cancelled`; `finish` rewrites only `Pending` (`workflow.rs:274-305,342-385,438-536`; `run.rs:125-189,221-250`) | R3 records `prepared_run_keeps_its_id` and `bounded_attempt_lifecycle`, including pre-poll interruption | PASS |
| REQ-019, REQ-034A, INV-018: admission, reservation, blocking preparation, execution, and shutdown remain under one process owner | `WorkflowRuns::admit` installs the tracker token and reservation under the table lock; tracked blocking work remains counted after waiter loss; reservation acceptance/failure/drop settles key and capacity once (`workflow/runs.rs:217-367,519-635`; `host.rs:255-400`) | Owner concurrency tests plus Scenarios 2 and 6 | PASS |
| REQ-030, REQ-034C, INV-013, INV-018, AC-021: scoped idempotency shares one preparation/run, never caches failure, and replay cannot replace accepted authority | `RunKey` includes tenant/principal/key; key state separates preparation from acceptance; promotion installs the run and key atomically; failure removes both and wakes waiters (`workflow/runs.rs:47-87,258-367,563-627`; `host.rs:91-123`) | Scenario 2 covers fan-in, conflict, tenant/principal scope, waiter cancellation, shutdown, disconnect, and lost response | PASS |
| REQ-032, REQ-033, AC-010: create/replay/get/cancel each authenticate, authorize, and audit before key lookup, admission, or disclosure | `WorkflowRunHost` calls canonical `audit::authorize` before admission or owned lookup; lookup remains tenant/principal-qualified and malformed IDs map to not-found (`host.rs:75-203`; `workflow/runs.rs:369-415`) | Scenarios 1, 3, and 6 plus recorded principals/audit lanes | PASS |
| REQ-032A, REQ-057, INV-022, AC-027: accepted authority is token-free, immutable, bounded to the pinned run, and cannot widen; later HTTP operations authorize afresh | `Preparation` owns a cloned verified `Caller`; gateway/tool adapters derive fresh request IDs only; replay returns existing state and no bearer/refresh secret enters retained run state (`host.rs:75-203,230-400`; `gateway/workflow.rs:34-131`; `workflow/tools.rs:43-123`) | Scenario 3 covers expiry, revocation, fresh request denial, pinning, replay non-replacement, no widening, and live gateway refusal | PASS |
| REQ-034, REQ-036A, REQ-038–043, INV-009–012, INV-020, AC-011A, AC-014–017: provider identity, route, fallback, deadline, cancellation, and live gateway governance survive source-to-sink projection | Adjacent-tagged `ProviderRequest` persists the selected dialect; `ServerWyrdGatewayCaller` projects the exact variant, including separate Gemini and Vertex branches, and enters the existing gateway owner with captured caller and separate cancellation (`skald-spec/src/request.rs:18-59`; `gateway/workflow.rs:54-131,182-250`) | Recorded tagged round trips/schema generation and Scenario 5 dialect, capability, fallback, deadline, cancellation, external-binding, and stored-Vertex cases | PASS |
| REQ-052, INV-006, INV-008, AC-025: only declared built-ins execute, and Cards/query reads retain captured tenant/principal/scopes while canonical owners authorize and audit | Suitability rejects unknown/duplicate names; per-Agent resolution exposes only `bifrost.query` and `cards.get`; Card reads enter `get_card_by_ref_for`; queries enter shared `BoundedQuery` (`host.rs:431-468`; `workflow/tools.rs:98-319`) | Scenario 4 covers permissions, malformed/bounded input, SQL floor, inaccessible objects, the approved real foreign-tenant boundary, terminal faults, and Card audit | PASS |
| INV-021 and TASK-004 query ownership: waiter loss signals a tracked owner, the original query deadline bounds opening/settlement, and run terminalization follows owner drain | Query owners run on `RunTools.owners`; the waiter drop guard cancels the child token; shared collector/control owners retain and settle the response; `RunTools::drain` precedes terminal commit (`workflow/tools.rs:107-114,188-233`; `query/collect.rs:259-399`; `oracle/lifecycle_controls.rs:35-162`; `host.rs:285-287`) | Query-control/collector evidence, Scenario 4, and forwarded cancel/deadline/pod-loss journey with recovery | PASS |
| R4 / `FIND-TASK-004-21`: every run-bound tool clone consumes the exact deadline fixed by `PreparedWorkflowRun`; no earlier tool-only sampler remains | `WorkflowExecutor::new` is the sole sampler; `PreparedWorkflowRun::deadline` exposes that instant; `RunTools` shares an `Arc<OnceLock<Instant>>`; `Preparation::prepare` binds it after preparation and before acceptance; `QueryTool` clips against the bound value (`workflow.rs:140-208`; `workflow_surface.rs:672-713`; `host.rs:374-400`; `workflow/tools.rs:43-96,188-215`) | The new paused-clock owner test proves clone equality after a one-second preparation interval; the updated forwarded journey gives the post-tool continuation an answer and still requires run-level `TimedOut` | PASS |
| R4 / `FIND-TASK-004-21`: omitted and explicit longer query deadlines clip to the prepared-run boundary, while an explicit shorter deadline remains shorter | The implementation uses `requested.min(remaining)` and uses `remaining` when omitted (`workflow/tools.rs:200-215`) | No focused assertion exercises a positive explicit shorter or longer `deadline_ms` through `QueryTool`. The R4 unit test only checks bound-instant equality (`workflow/tools.rs:434-471`); the Oracle journey omits `deadline_ms`; the server journey checks only zero as invalid (`tests/pg_workflow_runs.rs:1942-1993`). Recorded package-wide lane success does not supply the task's required direct proof. | **FAIL** (`INV-R5-001`) |
| Follower graph release follows the approved grant-stream-close contract; no leader acknowledgement or deleted graph-drain mechanism returns | The leader retains participant grant streams until settlement and drops them as release; followers settle asynchronously; no acknowledgement is awaited (`vala-bifrost-redux/src/oracle/analytical.rs:2492-2513,2690-2784`; `architecture/bifrost-design.md`) | Forwarded Oracle journey records owner settlement, surviving baselines, and later query success | PASS |
| Shutdown closes Workflow admission and drains tracked work before dependent gateway/query/Bifrost owners under the shared remaining deadline | `WorkflowRuns::drain` closes under the table lock, signals cancellation, closes its tracker, and waits only to the passed deadline; server shutdown orders it before dependent owners (`workflow/runs.rs:417-433`; `app/server.rs`) | Scenario 6 and recorded server/Bifrost shutdown evidence | PASS |
| REQ-034B, INV-013: runs are bounded, process-local, owner-qualified, retained for 24 hours, and lost on restart | `WorkflowRuns` contains only bounded in-memory maps, active-slot accounting, fixed retention, and terminal-only eviction; there is no durable queue/table/lease/recovery (`workflow/runs.rs:89-229,369-433,638-688`) | Scenario 6 covers eviction, expiry, restart loss, hidden-run equivalence, and queued/running shutdown | PASS |
| R1–R4 preserve closure of prior findings without reopening a shared source | R1 corrections remain at lifecycle/query/security/tool/gateway owners; R2 retains provider-tagged examples, Revision 13 authority, bare interfaces, and grant-stream documentation; R3 fixes the attempt producer and import sites; R4 removes the independent deadline sampler and binds the prepared deadline once | Prior ledgers and the recorded focused/broader evidence, subject to `INV-R5-001` for R4's explicit-deadline proof obligation | **FAIL** |
| Non-goals and standing DRIFT direction: no Workflow principal, durable job machinery, second graph/query/audit engine, bearer renewal, arbitrary tool platform, compatibility reader, follower-release protocol, bespoke check, setting, option, or unprecedented mechanism | The cumulative diff composes established owners and standard/native mechanisms: `JoinSet`, `TaskTracker`, cancellation tokens, watch channels, `OnceLock`, serde adjacent tagging, JSON Schema, and existing journey fixtures. Deleted graph-drain polling and supervisor idle refusal remain deleted. The R4 bind adds no task, service, setting, option, checker, dependency, or harness. | Complete changed-file inventory and cumulative diff review | PASS |

## Invariant traces

- **Prepared deadline:** Skald completes synchronous plan construction, samples
  the total deadline once in `WorkflowExecutor::new`, and returns a
  `PreparedWorkflowRun`. Before acceptance, the server binds that exact instant
  into the `OnceLock` shared by the `RunTools` value and every resolver/tool
  clone hydrated earlier. `QueryTool` reads only that bound instant. There is
  no remaining earlier tool-only clock sample.
- **Published step state:** successful binding reserves attempt one before the
  ledger publishes `Running`. First polling consumes attempt one and retries
  advance it. Cancellation, deadline, and abort rejoin through settlement,
  which records `Cancelled`; only never-scheduled `Pending` entries become
  `Unstarted`.
- **Accepted authority:** verified authentication produces a token-free
  `Caller`; create authorizes/audits it; exact graph preparation completes;
  acceptance captures it with the run. Gateway, Card, and query calls reuse
  its bounded tenant/principal/scope attribution while their existing owners
  apply their own decisions and audits. Replay never replaces it.
- **Run ownership:** admission installs the scoped key, capacity slot,
  cancellation child, outcome channel, and tracker token under one lock.
  Preparation and blocking work stay tracked; acceptance transfers the slot
  to one queued run; whole snapshots replace; tool owners drain; terminal
  commit releases capacity and starts bounded retention.
- **Query ownership:** the Agent waiter owns only a completed-value wait and a
  cancellation guard. The tracked owner retains the stream, validates terminal
  plus EOF, and performs bounded settlement before Workflow terminal commit.
  MCP and scheduled-query siblings retain their own deadline ownership.
- **Provider identity:** adjacent tagging persists provider identity through
  Prompt storage, binding, hydration, and gateway projection. The named
  variant, not body shape, distinguishes Gemini from Vertex.
- **Analytical lifecycle:** the leader drops participant grant streams as the
  release; followers observe closure and settle asynchronously. The leader
  never waits for a follower release acknowledgement.

## Proposed findings

### `INV-R5-001` — MISSING — R4's explicit-deadline closure proof is absent

- **Violated obligation:** TASK-004-R4 requires focused proof that an omitted
  query deadline and an explicit longer deadline clip to the prepared run's
  exact boundary, while an explicit shorter deadline remains shorter. The task
  explicitly requires the proof to exercise the gap; package-wide green lanes
  are not a substitute.
- **Location:**
  `crates/wyrd/wyrd-server/src/components/workflow/tools.rs:434-471` and
  `crates/wyrd/wyrd-testing/tests/bifrost/oracle/workflow.rs:134-226`.
- **Evidence:** the new owner test advances time and proves all clones contain
  `prepared.deadline()`, but it never invokes `QueryTool` with any
  `deadline_ms`. The updated Oracle journey sends an argument object containing
  only `sql`. Repository-wide search finds no positive explicit
  `deadline_ms` in a Workflow tool call; the server journey's only such tool
  case is `deadline_ms: 0`, which proves rejection rather than clipping. The
  recorded `test:wyrd` result therefore cannot prove the two explicit positive
  branches claimed in R4's implementation-evidence table.
- **Observable consequence:** `requested.min(remaining)` is present and appears
  correct, but a regression that ignores, extends, or prematurely clips an
  explicit positive deadline can pass every cited R4 assertion. The immutable
  task has not supplied its required closure evidence, so `FIND-TASK-004-21`
  is not fully closed for acceptance.
- **Required testable correction:** extend an existing Workflow tool owner test
  or existing real-server/query journey—without a new test binary, harness,
  clock service, setting, option, or checker—to drive the actual `QueryTool`
  path with (1) omitted, (2) greater-than-remaining, and (3)
  less-than-remaining positive deadlines. Observe the query request or its
  existing controlled timing boundary and prove the first two use the one
  prepared-run deadline while the third uses the shorter caller deadline.
  Retain the current paused-clock equality test and forwarded-Oracle
  settlement/recovery journey.

## Prior-finding closure

- `FIND-TASK-004-1` through `FIND-TASK-004-20` remain closed at their validated
  source owners and proof seams.
- R4 closes the implementation source of `FIND-TASK-004-21`: the independent
  tool deadline sampler is gone, one standard `OnceLock` distributes the
  already-owned prepared deadline, and all run-bound tool clones read it.
- `FIND-TASK-004-21` is not fully closed as a task acceptance matter because
  the mandatory explicit-shorter/explicit-longer focused proof is absent
  (`INV-R5-001`).

The binding human decisions remain intact and were not relitigated: Oracle
graph-drain polling and supervisor idle refusal stay deleted; follower release
is grant-stream close with no leader acknowledgement; the foreign-tenant
journey does not require an uncredentialed model step; published `Running`
reserves attempt one and interrupted published work settles `Cancelled`; and
built-in queries use the prepared run's one-time-bound deadline.

## Open questions

None affecting the finding or correction boundary.

## Verification notes

- This review did not run builds, tests, Cargo, or mise, as explicitly
  required. Recorded evidence was treated as a claim and checked against the
  candidate source and named assertions.
- R4 records the focused prepared-deadline test, format/lints, Skald, Wyrd
  server, and Oracle journey lanes as passing. Those results support the
  implementation correction and regression coverage, but none of their cited
  assertions exercises a positive explicit shorter or longer Workflow-tool
  deadline.
- Earlier recorded evidence covers the seven real-server Workflow scenarios,
  reservation/blocking/query owner tests, forwarded Oracle
  cancel/deadline/pod-loss recovery, MCP queries, Bifrost, principals, gateway,
  language SDKs, codegen, client-tier, tenant-isolation, PyO3 scope, unwrap
  audit, format, and lints.
- The approved foreign-tenant harness limit remains explicit: real
  authentication, registry, run, Card, query, and idempotency boundaries are
  crossed, while model execution is unavailable because gateway credentials
  exist only for the fixture tenant.

## Overall result

**FAIL**
