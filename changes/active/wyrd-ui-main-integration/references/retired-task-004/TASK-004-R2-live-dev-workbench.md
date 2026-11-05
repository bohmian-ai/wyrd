---
id: TASK-004-R2
kind: remediation
status: superseded
spec: SPEC-wyrd-ui-main-integration
spec_revision: 16
requirements: [REQ-007, REQ-015, REQ-020, REQ-021, REQ-022, REQ-023, REQ-026, REQ-028, REQ-030, REQ-033, REQ-034, REQ-037, REQ-038, REQ-039, REQ-040, AC-008, AC-010, AC-011, AC-013, AC-016, AC-017, AC-018, AC-019, AC-020, AC-021, AC-022, AC-023]
depends_on: [TASK-003, TASK-004, TASK-004-R1, TASK-027]
parent_task: TASK-004
remediates: []
---

> Retired on 2026-11-02 by the caller-directed revision 18 replacement.
> Historical instructions and evidence only; do not execute this task.
> Current owners: [Task 01](../../tasks/TASK-004-revision/TASK-01-mockserver.md)
> and [Task 02](../../tasks/TASK-004-revision/TASK-02-live-observe-workflow.md).
> No historical implementation claim becomes review PASS or visual acceptance.


# TASK-004-R2 — Investigate the canonical development workflow in a live Traces UI

> **Stage 1 rejected by caller.** Current Stage 1 correction is owned ONLY by
> [TASK-004-R3](TASK-004-R3-minimal-runtime-mock.md), against draft revision 17.
> All Stage 1 instructions/proofs below that require verification snapshots,
> new calls fields, fixture-dependent routing or preserved real-provider identity
> on mock attempts are superseded and must not be executed. The revision-16
> stage record is retained as historical evidence, not acceptance. Do not start
> Stage 2 from its completion claim. Later SQL/UI outcomes remain unchanged.

## Outcome and Value

One caller-directed remediation delivers a complete development/review loop:
`mise run dev:full` starts the real Wyrd environment and canonical Python app,
executes 1,000 application requests through an internal Rust gateway mock,
and leaves an authenticated live Traces workbench available for investigation.
The developer sees and filters real retained spans, opens a trace, selects a
span and inspects its actual data. Logs, metrics and verification evidence are
also emitted and verified through public reads; their full UI pages remain
separate work.

This is a new caller-directed outcome, not another review verdict on R1.
Preserve its implemented components, source edits and evidence. Revision 15
was approved on 2026-10-30; Stage 1 has since been implemented under a design
the caller rejected. [Revision 16](../../spec.md) approves the runtime correction on 2026-10-30.
The task is in progress; Stage 1 correction is authorized. Stages 2–5 keep their existing outcomes. This records no stage PASS
or visual acceptance. The early-live exception still permits contextual trace
component acceptance without unrelated family/gallery gates.

## What Changes and What Must Stay True

### One existing gateway, with an internal mock

Use the runtime mock contract in REQ-037: `with wyrd.mock("support-desk"):`
selects configured fixtures through `wyrd-gateway-mock`. The same tenant, key,
deployment and model support real and mocked calls concurrently. Existing
`AsyncOpenAI` construction and committed Agent/Prompt Cards remain unchanged.
The server replaces only provider dispatch; normal auth, deployment credential
resolution, routing, admission, capture and ledger settlement stay active.

Implement Python, Rust and TypeScript scope projections, including native
verification requests. Scope is captured before token refresh/native dispatch;
queued verification retains an immutable fixture snapshot with its execution.
Scope restoration, concurrent isolation, retries and queue restart cannot cause
real-provider fallback. A Run never owns mock mode. Reuse existing transport,
execution metadata and server gateway caller; no separate inference route,
fixture CRUD service, deployment adapter, tenant or key mode is introduced.

Fixtures are operator gateway configuration with the existing stateless rule
format, buffered Chat Completions responses, tool continuations and JSON judge
answers. Matching is generic; example names occur only in fixtures. The real
application executes tools and the real verifier writes outcomes. Unsupported
or unmatched calls refuse explicitly. Preserve actual provider identity with
separate fixture name/digest provenance; mock usage is synthetic, attempts are
non-billable and monetary cost is zero. Local dev provisions ordinary dummy
provider credentials without an external key. Preserve all supported OSes.

### Canonical example and normal dev lifecycle

Use `bohmian-ai/wyrd-example-py` as the application owner. The reviewed base is
`03428e1a4a35c8f23f71a95cae08e956ca317c9b`; record any canonical changes as a
reviewable patch with its base until publication is authorized. The launcher
must reproduce that effective source rather than depend on an unrecorded
dirty sibling checkout. Deliver both Wyrd orchestration changes and canonical
app/driver changes; a plan to update the template later is incomplete. Do not
copy the app into this repository or silently use the existing support-desk
mirror instead. No remote commit/push is authorized by this task.

Build and resolve the current checkout's host-compatible **production** SDK
wheel. Prove the imported wheel's origin/digest, not only `0.4.0`; the pinned
example's exclusive vendored Linux wheel must not win resolution. Configure
the canonical app and judge deployments, declare/pull real Cards and start
the normal app using the existing dev tenant/credential setup. Enable the
named scope in the canonical app request handler/invocation, under its dev
profile. Scoping the separate batch driver cannot affect another process.
Keep the authenticated OpenAI client block unchanged; native direct/queued
verification must inherit the same scope.

The default batch dispatches exactly 1,000 application HTTP requests, with
bounded concurrency and documented expected outcomes. Include real support
tool/final/judge workflows, verification failures, ordinary HTTP-only traces,
HTTP errors and missing optional/GenAI/service/Card values. Add actual span,
metric and log emission through the current SDK-installed OTel providers.
Do not write synthetic rows directly to Bifrost or force test-only server
flushes. The app uses public flush/shutdown and execution wait/readback.

Report requests, completed Runs, expected/unexpected failures, gateway
attempts, direct/queued results and retained signals separately. A response
count alone is not batch completion. Confirm expected terminal verifications
and retained public evidence with bounded waits; missing data or unexpected
failure makes batch status fail. Preserve server/UI availability for diagnosis,
and expose setup/batch readiness separately from server health. Execute once
per backend lifecycle; Vite hot reload does not rerun it. Preserve and disclose
the existing ephemeral reset/credential replacement; reuse current process and
Postgres cleanup owners on interruption and partial failure.

### Real shell, trace routes and SQL filtering

Deliver the full TASK-012 shell/control outcome and TASK-020 Main/Spans/
TraceDrawer workflow here. Use `/t/[tenantKey]/observe`, the actual browser
session and existing `/ui/load` BFF with shared portable loaders/executors.
Preserve retained login, logout, tenant switching/settings and theme behavior.
Unbuilt destinations are visibly unavailable; no mock pages or dead links
stand in for their later tasks. Ask/saved-view controls remain deferred.

Connect time presets/custom ranges/timezone display, SQL WHERE, facets,
Traces/Spans switching, three overview charts, row opening, shareable exact
trace/span selection, drawer closing/return state and all existing detail and
waterfall interactions. One resolved range/effective predicate drives charts,
facets and lists. Live pauses/resumes and holds rows/counts while reading or
investigating; the new-row notice and resume action work against new retained
records. Stale or cancelled results never overwrite newer state. Exact trace
reads retain authorized cross-workload/untagged/out-of-list-range evidence.

Implement REQ-040's editable SQL **predicate**, including its narrow validation
read and `where_sql` shared input closure. It must remain signal-mode trace
loading, with canonical trace IDs/counts/navigation. `RecordTable.sql` is not
a substitute. Facet conditions combine with the authored predicate using
AND; parameterized values, fixed query structure, range and authorization are
preserved. Invalid/injected/unknown-column predicates fail before dependent
reads and remain visibly unapplied. Reuse the installed server SQL parser;
do not handwrite a second SQL grammar or claim full-query validation proves
fragment safety. Project the read through Rust/Python/TypeScript SDKs,
HTTP/MCP, Node/MCP executors and generated catalog inputs.

Complete chart Open in Query at `/t/[tenantKey]/query` with a bounded exact-
query inspector: preserve executed SQL and typed binds, authorized re-run,
cancel, terminal/error/refusal states. Reuse existing query/load/result owners.
This closes the navigation consumer required by this workflow, without taking
on TASK-019's catalog/history/chart-authoring work. RecordDetail logs links
preserve the exact selected trace/span; the destination may use this task's
authenticated bounded related-log inspection, not an absent Logs page or a
fake-success action. Full Logs, Metrics, Query, Cards and Verify pages are
outside this assignment.

### Acceptance and scope transfers

The caller inspects the actual live workflow against Main, Spans, TraceDrawer
and TimePicker on desktop/mobile in light/dark. Match approved visual regions;
record every difference and accept none silently. Generic HTTP data with no
GenAI attributes must be usable and no record name determines styling/status.
The gallery remains supporting isolated-state evidence. This early slice does
not wait for unrelated families or TASK-011; it does not waive their eventual
portability, first-class journeys or full-library/MCP shipping requirements.

Under approved revision 15, TASK-012/020 are superseded by this assignment,
with all their original obligations retained here. This task owns first real-
browser/BFF lifecycle support; TASK-011 and later page tasks reuse it. TASK-019
records the exact-query inspector as delivered prerequisite and keeps its
remaining work. No acceptance or immutable review record is rewritten as PASS.

## Approach

1. Correct Stage 1 forward: remove deployment-based mock selection, add runtime
   SDK/header scope and execution snapshot propagation, and prove real/mock
   concurrency through unchanged clients and direct/queued judge workflows.
2. Deliver the canonical driver/telemetry changes and current-SDK resolution,
   then connect them to existing dev startup/readiness/cleanup.
3. Extend the existing query owner with the approved predicate contract and
   close its SDK/host/shared-loader consumers before page composition.
4. Assemble the real shell/trace/query-inspection hosts and connect their
   actions/navigation through the existing authenticated data path.
5. Verify the integrated 1,000-request lifecycle, real browser investigation and
   failure/tenant/cleanup behavior; present it for caller inspection.

## Staged Execution — One Stage Per Invocation

These are execution checkpoints inside this one task, not separate tasks or a
new workflow controller. Complete them in the numbered order under approved revision 16. Each invocation implements **only the requested stage**, records its
proof and stops. Passing a stage does not authorize starting the next one.

| Stage | Bounded deliverable | Required proof before stopping |
|---|---|---|
| **1 — Internal gateway mock** | Scenario 1: runtime scopes/header, configured rules, SDK/native transport, accepted verification snapshots, dispatch, provenance and accounting. No canonical batch or page implementation yet. | V1/V2/V11–V14 and the affected existing gateway regressions; recorded normal tool/final/judge exchanges, concurrency, refusal, authorization/capture and no upstream contact. |
| **2 — Canonical dev workflow** | Scenario 2: canonical base+patch, actual local production SDK, normal dev bootstrap, three-signal telemetry and default 1,000-request driver. Reuse the current UI until Stage 4. | V7; recorded actual 1,000-request counts, terminal verifications, retained spans/metrics/logs, source/wheel identity and lifecycle/failure behavior. The dataset is inspectable through existing public reads; live trace-page acceptance is still pending. |
| **3 — Shared SQL predicate support** | Scenario 3: server validation read, first-class projections and shared trace chart/facet/list planning. No new page/host composition yet. | V3–V6 and V10; supported/refused predicates, all-client/host parity, fixed bounds/binds, counts/navigation semantics and no dependent reads after refusal. |
| **4 — Functional live Traces UI** | Scenario 4: approved shell, real routes/BFF, time/SQL/facets, row/waterfall selection, live hold/resume, URL state and exact query/related-log inspection. Own the real-browser lifecycle here. | V8/V9 plus affected earlier proofs; an actual authenticated browser can complete the workflow against retained data, including negative tenant and ordinary HTTP-only cases. |
| **5 — Integrated verification and caller handoff** | Verification-only closure of all four scenarios on the final source: reproduce normal dev startup, fresh 1,000-request dataset and the complete live investigation. No manufactured RED or optional new feature. | Revalidate proof invalidated by later changes, run the required final owner checks, record final source/patch/wheel/browser identity and batch/readback evidence, and present the named visual checkpoints for caller approval and independent task review. |

At every stop, append a compact stage record to this task: stage number,
source/diff identity, completed obligations, exact commands and actual outcomes,
remaining blockers, and the next unstarted stage. Carry forward earlier proof
only when its source/dependencies remain valid. Keep the task `in_progress`
while stages remain; do not return whole-task `IMPLEMENTED` for a partial stage.
Do not automatically commit, push, begin another stage or mark human acceptance.

Each fresh backend lifecycle recreates its ephemeral dataset. Stage 2 proves
reproducibility; it does not promise that its database survives a later restart.
Stages 4/5 may rerun normal dev startup. Stage 5 must use the **final** current
SDK build, not assume Stage 2's wheel remains current after subsequent SDK edits.
A discovered defect in an earlier stage may be repaired as the smallest
necessary dependency correction, with its affected checks rerun and the change
recorded; this does not authorize starting unrelated later-stage work.

Copyable instruction for Claude, replacing `<N>` with the stage number:

```text
Use wyrd-implement to execute Stage <N> only from
changes/active/wyrd-ui-main-integration/tasks/TASK-004-R2-live-dev-workbench.md.
Read approved revision 16 and preceding stage evidence first. Use wyrd-ui when
editing the UI. Complete that stage's required proof, update its stage record,
and stop. Do not start the next stage, commit, push or claim the whole task done.
```

No stage is recorded complete against revision 16:

- [ ] Stage 1 — rejected; correction owned by TASK-004-R3
- [ ] Stage 2 — canonical dev workflow
- [ ] Stage 3 — shared SQL predicate support
- [ ] Stage 4 — functional live Traces UI
- [ ] Stage 5 — integrated verification and caller handoff

## Stage 1 Correction Handoff — Existing Implementation

Correct forward from `d87d24882` and `930a2de96`. Do not reset, revert the whole
commits or discard unrelated working-tree changes. Do not mark the old stage
complete merely by renaming `local-mock` to `openai`.

| Keep / adapt | Remove / replace |
|---|---|
| Stateless matcher in `wyrd-gateway/src/adapter/mock.rs`, protocol validation and concurrency/no-network assertions | `ProviderAdapter::Mock`, mock-only deployment validation/SDK admin DTOs and the `auth: none` restriction |
| Existing gateway dispatch, real deployment credentials and ledger/capture owners | The mixed mock/network deployment-plan ban and adapter-based mock detection |
| Synthetic-usage, non-billable and zero-cost handling, rewired to captured runtime scope | Replacement of actual provider identity with `provider="mock"`; use explicit fixture provenance |
| Useful existing regression coverage and normal gateway journey scaffolding | Tests that assert rejected deployment selection; replace with same-target scoped/unscoped journeys |

The correction must reach all of these boundaries in Stage 1:

- **Configuration/ingress/dispatch:** extend server `GatewayConfig` with named
  fixtures; parse and validate the header at authenticated ingress; carry scope
  through `GatewayCallRequest`/attempt dispatch. Credential resolution still
  happens normally; refusal cannot escape through retry/fallback to the network.
- **SDK scopes and native calls:** Python `_http_auth.py::HttpAuth._context`
  covers OpenAI HTTP only. Python `src/trace.rs::block_on` and TypeScript
  `native/src/trace.rs` capture context before native work; shared Rust transport
  retains it per operation. Close Python/TS/Rust exports, stubs and native
  declarations. Use checked scope propagation, not best-effort trace extraction.
- **Verification:** extend direct/enqueue acceptance, existing `Submission`,
  persisted execution metadata, queue claim/reconstruction and hosted
  `Execution`/`ServerWyrdGatewayCaller`. Freeze fixture content/name/digest at
  acceptance and include identity in deduplication. Never put this on a Run or
  `AcceptedAuthority`; unrelated work and authorization retain their meanings.
- **Evidence/contracts:** close `GatewayCallPayloadV1`, attempt span fields,
  gateway call table schema/builders/readers and settlement. Preserve real
  provider/model identity; nullable fixture provenance distinguishes mocked
  calls. Apply normal migration/schema-evolution handling for existing rows.
- **Doctrine and projections:** replace the rejected mock-deployment passages
  added by the two commits in `architecture/wyrd-design.md` and
  `architecture/wyrd-security-posture.md`; update source contracts and regenerate
  derived TypeScript/OpenAPI/catalog artifacts. Do not hand-edit generated output.

**Existing stage record:** the implementer reported V1/old V2 and six gateway
selectors passing on the two commits. These runs were not repeated during this
planning revision and do not prove runtime scope or queued propagation. The
reported lint/codegen/Python checks and final stage record were unfinished.
Stage 1 remains incomplete against revision 16. Append actual corrected source,
commands, results and outstanding work after implementation; preserve old
results as historical evidence with their original source identity.

## Ordered Implementation Scenarios

### Scenario 1 — Runtime scope replaces dispatch without changing deployments

**Behavior.** The unchanged OpenAI client and native SDK direct/queued
verification use named fixtures inside the runtime scope; unscoped calls to the
same tenant/key/deployment/model remain real, including concurrent calls. Nested
scope, exception exit and retries preserve the captured choice. Accepted queued
work uses its frozen snapshot after scope exit, restart or config replacement;
idempotency cannot conflate real/mock/different fixtures. Authorization and
credential resolution stay active. Unmatched, malformed or unsupported scoped
calls never fall back to network. Provenance preserves provider identity and
adds the fixture marker; mock attempts cost zero and are non-billable.

**RED.** V1/V2 pin dispatch and public ingress. Add V11–V14 for Python auth-hook
and native verification, TypeScript scope, Rust transport and durable queued
execution. Old mock-deployment tests do not satisfy these assertions. Prove
same-target scoped/unscoped concurrency, missing-credential refusal, no upstream
contact for mocks, restored real dispatch and accepted snapshot isolation.

**GREEN.** Apply the correction inventory below through existing config,
transport, dispatch, execution/queue and evidence owners. Close exports, stubs,
native declarations and doctrine together. Rerun affected gateway regressions.

**REFACTOR.** Retain stateless matching/protocol validation and remove rejected
adapter/admin machinery. Reuse existing runtime context and execution metadata;
no global client mutation, Run flag or new fixture registry service.

### Scenario 2 — dev:full produces the canonical retained dataset and stays inspectable

**Behavior.** Fresh normal dev startup loads the recorded canonical revision/
patch and current production SDK, configures the internal mock and all three
OTel signals, then dispatches exactly 1,000 app requests. Expected scenario
outcomes, completed verifications and real retained observations match the
batch report. No external model credential or network call occurs; automatically configured
local dummy credentials still exercise ordinary resolution. Hot reload
does not reseed. Missing source/wheel, incompatible configuration, partial app
startup, incomplete batch and interruption fail clearly and clean up owned
children without masquerading as a completed dataset. Server/UI remain usable
for review after successful completion and visible batch failure.

**RED.** Add the planned lifecycle journey V7 using current local-development
owners. Small focused cases establish failures during iteration; the final
normal-path evidence must exercise all 1,000 requests. No test-only server or
direct table seeder can produce GREEN for the development workflow.

**GREEN.** Extend the canonical app/driver and existing dev startup owners;
resolve the actual local production wheel and use real public workflows for
declaration, invocation, waits, flush and retained readback. Rerun V1/V2 and
V7 as affected, recording actual source/wheel identity and batch totals.
SQL/page proofs belong to the later stages.

**REFACTOR.** Reuse current bootstrap, saved credential, database and shutdown
lifecycles. Keep example application behavior canonical and orchestration thin;
remove duplicated mirror/seeding logic and unnecessary configuration.

### Scenario 3 — SQL WHERE filters all trace views without replacing their semantics

**Behavior.** Valid expressions with comparison, Boolean composition, nulls,
IN/BETWEEN and LIKE/ILIKE yield the same effective filtering across charts,
facets and root/flat lists. Facet values bind exactly; counts/row navigation
remain trace-aware. Invalid statements, comments, subqueries, unknown columns,
attempted range escape and denied table access fail visibly before dependent
reads. Exact trace evidence remains independent of the source list predicate.
The narrow validation read behaves identically through all first-class clients,
HTTP and MCP; no frontend-only authority is introduced.

**RED.** Add the planned parser/public-SDK tests in V3–V6 and portable-loader
regression V10. Browser integration assertions V8/V9 follow in Stage 4. Prove
accepted and rejected expression cases, typed binds,
permission refusal and no dependent read after failed validation.

**GREEN.** Reuse the canonical server SQL parser/query/catalog authorization
owner, add the specified read/projections and extend existing portable signal
loaders. Keep full query execution on its existing owner; rerun prior GREENs.

**REFACTOR.** Remove duplicate predicate/range construction; pages own host
state and composition, not SQL planning, parsing or domain shaping.

### Scenario 4 — The browser performs a complete live trace investigation

**Behavior.** A user signs in through the normal browser-session flow and
investigates the canonical batch in Main/Spans/TraceDrawer layout. Range and
SQL/facet changes update real reads; a row opens the exact trace; waterfall
selection changes detail; close/back/reload preserves valid state. New records
arriving while paused/open do not move the list; resume shows them. Query and
related-log actions have actual authorized destinations. Empty/denied/error,
cross-tenant, long/narrow content and ordinary HTTP-only records stay truthful.
The browser contains no provider/server credentials or fixture production data.

**RED.** Add V8's focused control regression and V9's real-browser journey.
The latter must navigate the running production BFF/server and observe actual
requests/retained data; jsdom or imported handlers cannot establish GREEN.

**GREEN.** Assemble existing shared components/loaders through the existing
BFF, add the shell/route/action and URL state plus bounded inspection
destinations. Reuse this task's browser lifecycle for future pages/MCP proof.
Rerun affected earlier scenarios; record source/browser identity and actual
server/browser observations.

**REFACTOR.** Preserve component portability, cancellation, exact IDs and
shared host boundaries. Remove page-level query/shaping duplication. Keep
canvas fidelity and accessibility intact while simplifying composition.

## Acceptance Criteria

| Spec obligations | Required observation / failure |
|---|---|
| REQ-037; AC-020 | V1/V2/V11–V14 show runtime selection on unchanged clients, same-target real/mock concurrency, native direct/queued propagation and immutable accepted snapshots. Mock upstream contact, lost credentials checks, cross-scope contamination, fabricated billing or real fallback fails. |
| REQ-038; AC-021 | V7 and the recorded normal dev run identify canonical source plus actual local production wheel, exactly 1,000 requests, expected terminal results and retained spans/metrics/logs. Missing/partial evidence or unrecorded dirty/mirrored source fails. |
| REQ-040; AC-023 | V3–V6/V8–V10 prove typed public predicate validation and shared signal application; syntax injection, escaped bounds, fixed-schema SQL substitution, client drift or dependent reads after refusal fail. |
| REQ-007/015/021/028/030/034/039; AC-008/013/022 | V8/V9 show the complete authenticated live workbench, exact selections, filter/time/live semantics, inspection actions and terminal failure behavior. A static WyrdView display or unavailable required action fails. |
| REQ-020/022/023/026; AC-010/011/016–019 | Individually inspect Main, Spans, TraceDrawer and TimePicker with real HTTP/Agent data, both themes and desktop/mobile; confirm canonical identities, typed/null data, authorized full trace evidence, keyboard/touch/long content. Record caller acceptance separately from agent screenshots and automated proof. |

Do not mark this task complete at mock unit tests, populated gallery, successful
HTTP responses or 1,000 dispatched requests alone. Completion is the real
repeatable dev workflow **and** working browser investigation, with independent
task review and the caller's explicit visual acceptance.

## Implementation Reference

| Existing owner / inspected consumers | Precise extension and reuse reason |
|---|---|
| Existing gateway config/rule types; `ProviderAdapter` and gateway-admin projections | Move fixture selection out of deployment adapters into configured named sets plus request scope. Remove rejected Mock-only admin contracts; normal deployment contracts remain. |
| `wyrd-gateway/src/engine.rs::ProviderDispatch`, `GatewayEngine`; `adapter/http.rs::HttpProviderDispatch`; `wyrd-server/.../gateway/owner.rs::Gateway::from_config` and `invocation.rs::GatewayInvocation` | Internal mock attempt execution under current routing/permissions/admission/capture/ledger owners. Existing `skald-runtime::MockProvider` FIFO/echo behavior does not supply the concurrent persisted fixture contract. |
| `gateway/src/adapter/tests.rs`, `engine.rs` tests; `wyrd-testing/tests/gateway/compatible.rs`; SDK auth/native verification journeys | Extend actual request-scope and dispatch journeys; public mocking must not depend on a test-only feature. |
| Python `_http_auth.py::HttpAuth._context`, native `trace.rs::block_on`; TS `state.httpAuth`, native `trace.rs`; shared `transport/trace.rs` | Existing runtime bridges capture context before native spawning. Extend once for checked mock scope; changing only HttpAuth misses native verification. |
| Verification `service.rs`, hosted `blocks.rs::Execution`, `runner.rs`; `wyrd-sql/src/queries/verifier_executions.rs`; server gateway `caller.rs` | Reuse acceptance, persisted staging/claim and hosted caller to carry an execution snapshot; current authority captures identity only and Run lookup is incorrect. |
| `wyrd-spec/src/gateway/record.rs`, `vala-bifrost-redux/src/tables/gateway/calls.rs` | Existing call/attempt payloads and retained table lack fixture provenance; close schema/builders/readers together rather than overload provider identity. |
| `mise.toml::dev:full`, `scripts/postgres/dev-server.sh`, `with-test-postgres.sh`; Python `test_local_development.py` | Extend one real startup/readiness/credential/cleanup owner for the canonical process and batch. No duplicate server launcher or test-server substitute. |
| Canonical pin `src/app.py`, `src/desk.py`, `scripts/deploy.py`, `test/conftest.py` | Actual app, authenticated gateway client, Cards and example protocol behavior. Missing canonical batch/log/metric/HTTP-only coverage is added there; fixture responses inform configuration, not a gateway-specific business implementation. |
| Current `state.start_telemetry`, `state.flush/shutdown`; `python/wyrd/otel.py`; `agent_readiness/live.py::build_wheel` | Reuse existing three-signal providers, public durability and actual production-wheel build precedent. Its `UV_FIND_LINKS` alone does not override the canonical exclusive vendor source. |
| `vala-bifrost-redux/src/oracle/planner.rs::OraclePlanner::validate_query`, Oracle parser; server query/catalog routes; `wyrd-client::Bifrost` and SDK/MCP projections | Reuse the parser and authorized table schema for a new bounded expression read. Existing full-SELECT validation is not fragment validation; `query/floor.rs` is not a demonstrated production expression owner. |
| UI `view.ts::remoteLoader`, `/t/[tenantKey]/ui/load/+server.ts`, `WyrdView`, `observe.ts`, existing components/catalog/actions | Extend shared inputs/loaders once and connect real route/host state. These owners already implement real BFF reads, periodic reload/cancellation, root counts, exact evidence and typed actions. |
| Current tenant layout/session routes; `identity_ui_e2e.rs::start_bff`; UI Vitest/include owner and `ui-library.integration.test.ts` | Add approved shell and first real-browser driver/lifecycle. Existing AppBridge helper uses jsdom and does not prove deployed browser navigation. Minimal test-only browser dependency is already permitted by REQ-034/INV-004. |

Architecture closure belongs to the same task: update owning gateway/public
surface doctrine, security/mock attribution and Bifrost/query validation text
alongside contracts. Keep pure types in `wyrd-spec`, runtime behavior in server
owners and thin first-class projections in clients. No Arrow/DataFusion or IO
dependency enters `wyrd-spec` or client tiers. Regenerate schemas/OpenAPI/catalog
and native declarations from their owners; author Python stubs and verify all
binding/export layers when changed. Read directory AGENTS and `wyrd-ui` before
implementation; source paths guide discovery rather than private code layout.

## Verification and Evidence

V1 exists in the old implementation and must be adapted. Other new selectors
below are **planned**, except the three named gateway regressions.
Packages, existing test modules/targets and runner setup were inspected.
Implementers confirm actual newly added selectors before RED and record any
equivalent correction. No test/build/startup was run during this planning pass.

**V1 — mock dispatch, concurrency and no upstream call (existing; adapt):**

```bash
mise exec -- cargo nextest run --locked -p wyrd-gateway --lib -E 'test(=adapter::tests::mock_chat_fixtures_are_stateless_and_do_not_call_upstream)'
```

**V2 — same-target runtime mock/inference/capture journey (replacement planned):**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E "test(=compatible::request_scoped_mock_preserves_real_deployment_and_auth)"'
```

**V3 — server expression parsing/bounds (planned in existing planner test owner):**

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::planner::tests::sql_predicates_are_single_bounded_expressions)'
```

**V4 — public Rust/HTTP query-predicate journey (planned):**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=query::predicate_validation_preserves_authorized_query_structure)"'
```

**V5 — Python predicate projection (planned in existing Bifrost test owner):**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/bifrost/test_bifrost_e2e.py::test_sql_predicate_validation_matches_public_contract'
```

**V6 — TypeScript/Node/MCP predicate parity (planned):**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts -t "^TASK-004-R2: Predicate parity across hosts$"'
```

**V7 — normal canonical dev lifecycle and 1,000 requests (planned):**

```bash
mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/test_local_development.py::test_dev_full_canonical_mock_workflow
```

V7 drives the actual dev launcher and its owned ephemeral database; it does not
start a second WyrdTestServer. Reuse normal local-development prerequisites.
Record effective canonical source/patch, platform and SDK wheel digest; no
skip due to a missing old `gateway.json` can establish success.

**V8 — shared/page controls and navigation (planned files under current include):**

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.test.ts -t '^TASK-004-R2: Live trace controls and navigation$'
```

**V9 — actual authenticated browser exploration (planned):**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t "^TASK-004-R2: Authenticated live trace exploration$"'
```

V9 reuses the actual production BFF/server lifecycle and a real browser engine,
with normal login, underprivileged and foreign-tenant cases. It may use a small
functional workload during RED/GREEN; final V7/manual evidence establishes the
actual 1,000-request dataset. Record browser/source identity, sanitized request/
trace/screenshot evidence and visible retained-data observations. Fake local
login without `locals.browserSession`, AppBridge jsdom or imported handlers
alone cannot satisfy this proof.

**V10 — shared trace predicate planning, before page implementation (planned):**

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-004-R2: Shared trace predicate planning$'
```

**V11 — Python runtime scope and unchanged OpenAI/native verification (planned):**

```bash
mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/client/test_http_auth.py::test_mock_scope_snapshots_restores_and_isolates
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/gateway/test_client_http_auth.py::test_runtime_mock_reaches_openai_and_native_verification'
```

V11 covers long-lived `client.http_auth` and `state.http_auth`, nested/exception
restoration, concurrent real/mock calls, token refresh/retries, `asyncio.to_thread`
and native direct/enqueue calls. Use a preexisting Run inside the scope and a
Run created inside then reused outside; neither retains a hidden mode.

**V12 — TypeScript runtime scope/native bridge (planned in existing test owner):**

```bash
mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/unit/wyrd-client.test.ts -t '^runtime mock scope isolates requests and native verification$'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/gateway-inference.test.ts -t "^runtime mock scope reaches OpenAI and native verification$"'
```

Test native handoff as well as HTTP auth, nested restoration and concurrent
scoped/unscoped calls; JS-only header assertions cannot establish native parity.
The integration selector uses the existing `serverTest` lifecycle in
`gateway-inference.test.ts`, with current public SDK and testing addons built
through the existing native/package setup. Exercise the actual TypeScript
OpenAI auth and direct/queued verification paths against the server; a Rust
journey cannot substitute for the TypeScript/native lifetime boundary.

**V13 — Rust scoped transport propagation (planned in existing context owner):**

```bash
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=transport::trace::tests::gateway_mock_scope_is_request_local)'
```

Prove scoped future selection, independent concurrent requests, restoration and
captured retry context without changing persistent client headers. V2/V14 must
use the public Rust scope surface as well as raw-header negative cases.

**V14 — accepted queued verification snapshot and isolation (planned):**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=verification_runtime::queued_mock_context_is_immutable_and_execution_scoped)"'
```

Accept through real SDK/server, exit the client scope, change fixture config,
reconstruct/restart the worker, and complete with the accepted snapshot. Assert
no upstream contact, correct judge outcome/provenance, distinct real/mock and
changed-fixture idempotency, and independent subsequent work on the same Run.
Reusing only an in-memory caller cannot prove durable propagation.

**Existing gateway regressions affected by dispatch changes:**

```bash
mise exec -- cargo nextest run --locked -p wyrd-gateway --lib -E 'test(=adapter::tests::compatible_passthrough_rewrites_model_and_uses_provider_credential)'
mise exec -- cargo nextest run --locked -p wyrd-gateway --lib -E 'test(=engine::tests::unsafe_failures_never_retry_and_credentials_fail_closed)'
mise exec -- cargo nextest run --locked -p wyrd-gateway --lib -E 'test(=engine::tests::deadline_and_cancellation_bound_in_flight_attempts)'
```

Use [verification owners](../verification.md) for current SDK,
testing bindings, server, Postgres, migrations and UI package setup. Build the
canonical app with the production wheel separately from testing-enabled test
runners. Run affected R1 regressions by their recorded exact selectors when
shared components/loaders change; preserve unchanged evidence with reasons.
Run Rust format/lints, Python format/lints/types and UI check/build for touched
owners, plus `codegen:check`, changed binding/native declaration checks,
production-wheel boundary and `git diff --check`. Use narrow changed-owner
lanes; no aggregate gate, live provider credential or synthetic host load.

**Manual completion checkpoint:** run normal `mise run dev:full`, wait for the
reported batch/verification/retention outcome, sign in through real recovery/SSO
session setup and demonstrate the entire investigation workflow to the caller.
Record per-region acceptance against the four named boards, with supported
themes/sizes and keyboard/touch checks. No action substitutes for this live
human review.

## Material Stop Conditions

Revision 16 is approved. The obsolete deployment contract is not an acceptable
fallback. Execute only the explicitly requested stage. Stop only the affected work for an actual conflict with architecture, required protocol,
authorization/tenant boundaries or source availability; finish independent
in-scope work and report the precise evidence. Do not replace the canonical
app, invent an SDK override that its exclusive source ignores, use a paid
provider fallback, weaken the 1,000-request outcome, hide unsupported SQL,
invent server data, mark an absent route working or call uninspected UI accepted.

Real canonical source access/publication and human inspection are distinct
from implementation choices. If remote publication is unavailable, deliver the
specified reproducible base+patch; do not push or use a hidden mutable checkout.
Private helper layout, fixture organization, bounded concurrency tuning and
test scaffolding remain implementation-owned within the fixed contract.

## Authority Links

- [Approved spec revision 16](../../spec.md), [task plan](../../plan.md), [current R1 evidence](TASK-004-R1-canvas-fidelity.md).
- [Absorbed shell task](../../tasks/TASK-012-canvas-shell.md), [absorbed trace task](../../tasks/TASK-020-traces-page.md), [remaining Query work](../../tasks/TASK-019-query-page.md).
- [Canvas](../canvas/README.md), [verification setup](../verification.md).
- [Gateway/Agent authority](../../../../../architecture/wyrd-design.md), [security](../../../../../architecture/wyrd-security-posture.md), [Bifrost](../../../../../architecture/bifrost-design.md).
- [Execution](../../../../implementation-execution.md), [spec/task workflow](../../../../spec-driven-development.md), [testing](../../../../../TESTING.md).
- [Canonical source baseline](https://github.com/bohmian-ai/wyrd-example-py/tree/03428e1a4a35c8f23f71a95cae08e956ca317c9b).

## Planning Evidence

Source inspection confirmed the existing dispatch, lifecycle, telemetry, BFF,
component, query and test owners above; no application code, build, service or
test was executed during task planning. New selectors are planned rather than
reported as existing passes. An independent comprehension pass read the draft
spec/task and affected ownership references together. Its obsolete TASK-011
browser-tooling references were reconciled; required contracts and the single
integrated outcome remain explicit. This is planning evidence, not an
implementation PASS. The subsequent readiness review approved the technical
plan, and the caller approved revision 15 on 2026-10-30. Stage 1 was subsequently implemented under the now-rejected deployment
contract; its reported test passes are historical evidence only. No stage is
recorded complete against revision 16. This revision records no runtime or
visual result.

Revision-16 independent comprehension checked the revised spec and this task
together. The reader found no material contradiction or missing explanation in
the changed behavior, limits, invariants, queue proof, forward correction or
approval boundary. This is document comprehension, not source validation,
readiness approval or an implementation test result. Static packet checks and
`git diff --check` passed; no runtime code or existing commits were changed.

The caller approved revision 16 on 2026-10-30 and requested the Stage 1
implementer handoff. This authorizes the correction, not a stage PASS, commit,
push or automatic progression to Stage 2.

## Historical Stage 1 Record — Revision 16 Implementation Rejected

**Source identity.** `HEAD` `930a2de9628486bf4ae9870cd45ec3bd987b1e07` plus the
uncommitted working tree; tree `76fd8ce5f842005b9eab9b01acd8ec77d941f9fc`
(`git write-tree` of every path except `changes/`, through a scratch index;
it includes the caller's pre-existing wyrd-ui edits). Nothing committed or
pushed. New files: `crates/wyrd-spec/src/gateway/mock.rs`,
`crates/wyrd/wyrd-sql/migrations/20261027000000_verification_gateway_mock.sql`,
`sdks/wyrd-sdk-python/python/wyrd/_mock.py`,
`sdks/wyrd-sdk-ts/wyrd/tests/integration/runtime-mock.test.ts`.

**Correction applied.** Removed `ProviderAdapter::Mock`, the mock-only
deployment validation and admin DTOs, the `auth: none` restriction, the
mixed-plan ban and `provider="mock"`. Kept the stateless matcher and the
response validation. Added:

- the `wyrd-gateway-mock` header and `[gateway] mocks` server config;
- scope carried through ingress, routing, which keeps only the candidates a
  fixture answers, and dispatch, where credentials still resolve and
  attempts are non-billable;
- `mock_fixture {name, digest}` on the calls payload, the attempt span and
  the Bifrost calls column;
- a `wyrd.verifier_executions.gateway_mock` snapshot, frozen at direct or
  enqueue acceptance and part of `RequestKey`;
- Rust `with_gateway_mock`/`current_gateway_mock`;
- Python `wyrd.mock` (`HttpAuth._context` plus native `CallerScope`);
- TypeScript `withMock` (`httpAuth` plus native `trace::spawn`);
- doctrine in `wyrd-design.md` (gateway ingress) and
  `wyrd-security-posture.md` (credentials).

Nothing is stored on a Run or `AcceptedAuthority`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| V1 stateless dispatch, no upstream | `wyrd-gateway/src/adapter/{mock,http}.rs`, `routing.rs`, `engine.rs` | `mise exec -- cargo nextest run --locked -p wyrd-gateway --lib` → 68 passed (incl. `adapter::tests::mock_chat_fixtures_are_stateless_and_do_not_call_upstream`, `engine::tests::mocked_attempts_resolve_credentials_but_are_never_billable`, `routing::tests::mock_scope_keeps_only_answered_candidates`) | PASS |
| V2 same-target scoped/unscoped journey: auth, 403 reader, refusals (unknown, malformed, empty, repeated, unmatched without echo, stream, embeddings), capture and provenance, one upstream call | `wyrd-testing/tests/gateway/compatible.rs` | `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E "test(=compatible::request_scoped_mock_preserves_real_deployment_and_auth)"'` → 1 passed | PASS |
| V11 Python auth hook and native direct/queued verification | `_mock.py`, `client/_http_auth.py`, `src/trace.rs` (`CallerScope`), `src/client.rs`, `src/state/mod.rs`, `src/testing.rs` | `uv run python -m pytest -q tests/unit/client/test_http_auth.py` → 5 passed; with Postgres, after `mise run py:setup`: `uv run python -m pytest -q -m integration tests/integration/gateway/test_client_http_auth.py::test_runtime_mock_reaches_openai_and_native_verification` → 1 passed | PASS |
| V12 TypeScript scope, HTTP and native | `wyrd/src/index.ts` (`withMock`), `native/src/trace.rs`, `native-testing/src/lib.rs` | `pnpm exec vitest run tests/unit` → 83 passed; with Postgres: `pnpm exec vitest run tests/integration/runtime-mock.test.ts` → 1 passed | PASS |
| V13 Rust transport scope is request-local | `wyrd-client/src/transport/{trace,http}.rs` | `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=transport::trace::tests::gateway_mock_scope_is_request_local)'` → 1 passed | PASS |
| V14 queued snapshot is immutable through scope exit, retry, restart and config change, and part of the execution key | `verification/{submissions,runner}.rs`, `wyrd-sql/queries/verifier_executions.rs`, migration | `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=verification_runtime::queued_mock_context_is_immutable_and_execution_scoped)"'` → 1 passed | PASS |
| Owner lanes | server, SQL, gateway, Rust SDK | `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server -p wyrd-sql -p wyrd-gateway -p wyrd-ai --lib --tests --no-fail-fast'` → 1093 passed, 177 skipped; `-p wyrd-spec -p wyrd-sql -p vala-bifrost-redux -p wyrd-client --lib -E 'test(mock) \| test(=queries::verifier_executions::tests::keyed_execution_id_is_stable_and_content_scoped) \| test(calls)'` → 12 passed | PASS |
| Format, lint, contracts | — | `mise run fmt`, `mise run lints`, `py:format`, `py:lints`, `py:typecheck`, `ts:format`, `ts:lints`, `ts:typecheck`, `codegen:regen` then `codegen:check`, `check:deps`, `check:tenant-isolation` and `git diff --check` all exit 0. `ts:napi:check` exits 1 only because the regenerated `index.d.ts` (+`gatewayMockNameError`, `setGatewayMockSource`) is uncommitted; `ts:build` reproduces it exactly. | PASS (napi check clears on commit) |

**Diagnoses.**

- *V14 `Query(RowNotFound)`.*
  - Symptom: the settle read found no execution row.
  - Evidence: a `WYRD_LOG=info,wyrd_server=debug,sqlx=debug` trace showed
    enqueue answering before the writer lane committed (`submissions.rs`
    acceptance path).
  - Cause: the test read the row straight after `enqueue` returned.
  - Fix site: the test's `settle` now waits through the existing
    `await_queued` helper. No production change.
- *V14 inconclusive verdicts.*
  - Symptom: every judge reported `inconclusive`.
  - Cause 1: the Verifier Prompt lacked `response_format: "verdict"`.
  - Cause 2: a `passed` verdict without citations into the original evidence
    is downgraded to inconclusive (`vala-verify/src/blocks/agent.rs`).
  - Fix site: the test fixtures now use the verdict format and uncited
    `failed`/`inconclusive` outcomes, so they stay distinguishable.
- *TS typecheck (existing).*
  - Symptom: `tests/unit/bifrost-query.test.ts` failed to typecheck.
  - Cause: its fake native stream lacked the `collect` method that
    `NativeBifrostQueryStream` now declares.
  - Fix site: added a throwing `collect` to the fake.

**Limitations.**

- Only buffered OpenAI Chat Completions to OpenAI-protocol deployments is
  mockable; everything else is refused.
- Group and scheduled binding executions, and MCP verification tools, always
  dispatch for real.
- The Rust SDK has no provider-client auth hook. A caller composing gateway
  requests reads `current_gateway_mock` and sets the header itself.
- The built-in `bifrost.gateway.calls` schema gains a nullable `mock_fixture`
  column in place, like the envelope columns added in `2dfe39609`. Bifrost has
  no built-in schema evolution: an existing physical table fails startup
  reconciliation with a metadata mismatch, so existing datasets must be
  recreated. Fresh test and dev lifecycles are unaffected. A real upgrade path
  for built-in tables is a Bifrost design decision outside this stage.
- The Postgres `gateway_mock` column comes through an ordinary new migration;
  existing execution rows read null, meaning real dispatch.

**Non-goals held.** No canonical batch, dev workflow, SQL predicate or UI page
work. The pre-existing caller edits in `changes/`, the wyrd-ui components and
the gallery were left untouched.

**Current next action:** TASK-004-R3 correction; Stage 2 is not authorized.
