# TASK-004 R3 System-Resilience Review

## Immutable subject and scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `f17726fb25df1fa513875dca8d92f0073340ee0a`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 13
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Remediations: `TASK-004-R1-close-accepted-job-gaps.md` and
  `TASK-004-R2-align-revision-and-source-contracts.md`
- Review method: static inspection of the complete base-to-candidate diff,
  current owners and callers, applicable architecture, prior validated
  findings, and recorded verification evidence. Per instruction, no build,
  test, Cargo, or mise command was run.

The candidate resolved at the requested commit before this review. The R2
delta from `e86831e5a` to the candidate changes only active change records,
three Prompt examples, module-level Rust imports/bare type spelling, and the
Oracle lifecycle documentation. It does not change executable runtime logic,
wire behavior, configuration, tests, or deployment topology.

## Deployed topology and changed runtime paths

`wyrd-server` is the sole network-serving process. Its `AppState` contains one
process-local `Arc<WorkflowRuns>`; the HTTP Workflow routes construct a
`WorkflowRunHost` over that shared state. A create authenticates at the `/v1`
boundary, makes and transactionally audits `workflows:run`, scopes its
idempotency key by verified tenant and effective principal, then either replays,
waits on the one existing preparation, or gives a new reservation to tracked
process-owned work (`components/workflow/host.rs:60-123`; `runs.rs:258-345`).
The accepted run therefore outlives the submitting connection but not the
server process or replica that accepted it.

The tracked preparation reads the exact active graph through the existing
tenant-scoped Cards owner, resolves only selected tenant-qualified external
bindings, rejects unsupported routes/tools before acceptance, and performs the
CPU-bound Skald hydration/prepare stage on the Workflow tracker's blocking
pool (`host.rs:290-399`). After atomic acceptance, the same Skald executor owns
the DAG. Snapshot replacement is synchronous and terminal publication occurs
only after the run's query owners have drained (`host.rs:245-287`;
`runs.rs:565-688`).

Each model call reaches the existing in-process `GatewayInvocation` with
`authorized = false`, so current gateway deployment, credential, admission,
per-model authorization, accounting, and capture remain gateway-owned
(`components/gateway/workflow.rs:70-138`). Each built-in query runs as a
tracked per-run tool owner; the waiter may disappear, but the owner retains the
Oracle response and completes settlement. Card reads continue through the
authorized and audited exact-ref Cards boundary (`components/workflow/tools.rs:35-98,139-219,247-311`).

`BoundedQuery` opens through the ordinary query service and
`RunningQueryControls`, collects only a complete terminal-safe result, and on
every consumer error invokes `cancel_and_settle` before returning
(`query/collect.rs:270-324,373-399`). Opening cancellation and post-open
settlement share the query's original deadline; neither starts a new cleanup
budget (`oracle/lifecycle_controls.rs:34-160`). A distributed Analytical leader
holds follower grant streams. Dropping those streams is the follower release;
followers settle their own graph asynchronously and the leader does not await
a release acknowledgement (`vala-bifrost-redux/src/oracle/analytical.rs:2492-2501,2688-2774`;
`architecture/bifrost-design.md:382-413`).

Shutdown uses one process deadline. Workflow admission closes and every
preparation/run is signalled and drained first while gateway, Oracle/query, and
Bifrost services are still available. MCP and gateway tracked settlement then
drain inside the remaining, never restarted, budget; an incomplete Workflow,
MCP, or gateway drain makes shutdown non-clean rather than silently claiming
success (`app/server.rs:746-826`).

## Failure and recovery assessment

| Failure or recovery path | Source behavior and system effect | Proof assessment | Result |
|---|---|---|---|
| Submitter disconnect or cancelled matching waiter | Preparation is spawned on the process tracker before the handler waits. Its reservation owns the key, global/tenant active slots, cancellation child, outcome channel, and a tracker token, so a dropped HTTP future or waiter cannot cancel or duplicate work (`host.rs:99-122`; `runs.rs:258-345,535-635`). | Recorded Scenario 2 covers creator disconnect, waiter cancellation, matching replay, lost acceptance response, tenant-key isolation, exact accepted/provider counts, and failure/shutdown wake-up. | PASS |
| Registry, binding, validation, or blocking-preparation failure | No public run exists before `Reservation::accept`. Any preparation error publishes one structured failure and removes the key and capacity; shutdown/drop follows the same exact-once release path. The blocking closure stays counted by the tracker even if its awaiting preparation is cancelled (`host.rs:264-278,318-399`; `runs.rs:347-370,609-635`). | R1 owner tests and Scenarios 1-2 are recorded green, including drain before task spawn and caller-abandoned blocking work. | PASS |
| Capacity pressure and expensive graph preparation | Global and per-tenant active ceilings are checked under the short run-state lock. Step, edge, body-byte, input, step-result, and aggregate-run ceilings are applied before or during ownership of bounded state; graph preparation runs outside the state lock (`runs.rs:258-345`; `host.rs:318-399`). Cards, gateway, and Bifrost remain separate owners and are not held behind that lock. | Scenario 7 records graph/input/run refusals, a 1024-step admissible graph, sibling Cards/gateway/query serviceability during held preparation, bounded terminalization, and slot reuse. | PASS |
| Provider or external-gateway outage/refusal | The Skald run receives a typed step failure under its total deadline. In-process gateway work remains owned by the gateway after Workflow signalling; Workflow does not wait for gateway accounting/capture/audit and cannot mutate after terminalization. External binding resolution and endpoint suitability precede acceptance where required (`host.rs:333-379`; `gateway/workflow.rs:70-138`). | Scenario 5 records all supported dialect projections, capability refusal before upstream dispatch, fallback isolation, deadline and cancellation isolation, tenant-qualified external bindings, and secret separation. Existing gateway journeys supply the owner-level outage/settlement proof. | PASS |
| Explicit cancel, total deadline, or shutdown during DAG work | One cancellation tree covers preparation, Skald execution, and query-owner child tokens. Skald aborts/drains its work; Workflow then drains its query owners before committing the complete terminal snapshot and releasing active capacity (`host.rs:245-287`; `tools.rs:87-98`; `runs.rs:650-688`). A cancel request records the signal synchronously before waiting, so client disconnect cannot revoke it (`host.rs:144-172`; `runs.rs:390-415`). | Scenarios 4, 6, and 7 record cancel/deadline races, queued/running shutdown, complete terminal snapshots, near-ceiling terminalization, later slot reuse, and sibling availability. | PASS |
| Query cancellation while the distributed stream is still opening | Cancellation routes once to the registered local or current-ready remote owner while retaining the same open future. The owner signal and open are bounded by the original query deadline; on expiry the open is dropped and the path reports incomplete rather than claiming cleanup (`lifecycle_controls.rs:118-160,300-339`). | R1 records the exact original-deadline owner test, the scheduled-query regression journey, and the forwarded Workflow journey. | PASS |
| Query consumer error, malformed/partial stream, or waiter loss after open | The collector requests stream cancellation and retains the response. It accepts rows only after schema, validates the sole terminal and emitted row count, requires Arrow EOS and clean response EOF for success, and otherwise awaits `cancel_and_settle` before the tool owner finishes (`collect.rs:373-524`). No partial rows reach the model. | Scenario 4 records EOF-after-schema/batch, terminal/result bounds, object denial, cancellation owner join, no data disclosure, and a successful later query. | PASS |
| Analytical follower delay or pod loss | Peer loss is terminal for the selected attempt. Leader-side settlement joins its own attempt/exchanges, closes participant grant streams, and releases its graph. Followers observe stream close and independently cancel/join their graph. Membership must exclude the dead pod before a later cut can omit it; there is no leader acknowledgement, retry timer, graph-drain poll, or idle-refusal protocol (`analytical.rs:2492-2501,2688-2804`; `bifrost-design.md:382-421`). | The recorded forwarded journey covers cancel, deadline, and held-follower pod loss, exact outcome class, no rows/model success on loss, surviving-owner baseline recovery, membership refresh, sibling query availability, and a later successful query. | PASS |
| Process crash, restart, or rolling replacement | Runs and idempotency are deliberately process-local. A crash loses in-memory runs and does not resume/replay provider calls. A non-owning or replacement replica returns the same not-found response; deployment therefore requires request affinity rather than cross-replica recovery (`architecture/wyrd-design.md:445-455`; `runs.rs:212-239`). | Scenario 6 records restart loss with no resumed calls. This is the approved V1 availability boundary, not an unhandled recovery promise. | PASS |
| Graceful process shutdown | Workflow admission closes under the same lock used by admission; every visible reservation already owns a tracker token. Workflows drain before their gateway/query dependencies. The shared deadline bounds all later subsystem drains, and failure to drain is surfaced in the process terminal result (`runs.rs:418-432`; `app/server.rs:746-826`). | Scenario 6 records preparation waiters receiving 503, queued and running runs reaching complete cancelled snapshots, and a clean Workflow drain. | PASS |

## Affected capabilities and availability boundary

- Workflow create/get/cancel, Cards graph pinning, Skald DAG execution, in-process
  gateway calls, external gateway calls, built-in Cards/query tools, and
  distributed Oracle query settlement are the materially affected paths.
- A failed preparation affects only that create/key and releases its slots. A
  failed provider or tool operation affects its run/step. A failed query
  participant fails the selected query attempt. None of these paths is used as
  permission to crash `wyrd-server` or stop unrelated Cards, gateway, or query
  service.
- Shutdown timeout is the process boundary: remaining tracked work prevents a
  clean shutdown claim, but no second per-subsystem deadline extends shutdown.
- Restart and non-owning-replica lookup are intentionally unavailable for this
  process-local V1 resource; the system makes no durability or cross-replica
  recovery claim.

## Standing decisions and drift screen

The fixed human decisions are preserved and were not re-litigated:

- deleted Oracle graph-drain polling and supervisor idle refusal remain absent;
- grant-stream close is follower release, and the leader neither sends nor
  awaits a follower release acknowledgement;
- the foreign-tenant journeys do not require a model step because the harness
  provisions gateway credentials only for the fixture tenant.

The cumulative runtime uses established mechanisms: Axum request handling,
Tokio cancellation tokens/task tracking, watch channels, one monotonic
deadline, process-local in-memory job ownership, existing gateway/query/Card
owners, and bounded resource admission. The R2 remediation adds no mechanism,
check, file outside the required review/change records, setting, option,
dependency, parser, transport, lifecycle protocol, or custom recovery system.
No nonstandard mechanism lacking an established repository or comparable
widely used project precedent was found in this round.

## Review findings

No material system-resilience finding is proposed. The reachable failure and
recovery paths remain within the approved process-local availability,
integrity, deadline, and sibling-service boundaries.

## Verification limits

- No commands that compile or execute code were run. All verification results
  are candidate-recorded evidence and were checked against the named source and
  assertions rather than reproduced.
- The recorded evidence includes all seven real-server Workflow scenarios,
  the forwarded three-Oracle Workflow query journey, relevant focused owner
  tests, the Bifrost/server/gateway/shared/principals lanes, codegen and boundary
  checks, formatting, lints, and `git diff --check`.
- Real process crash is represented by restart/non-resumption evidence; the
  approved V1 contract intentionally has no durable recovery. Live removal of
  a gateway deployment mid-run is not directly journeyed, but the runtime
  source calls the ordinary gateway admission with `authorized = false`, and
  the gateway owner's existing journeys are the recorded proof for that live
  decision boundary.

## Overall result

**PASS**
