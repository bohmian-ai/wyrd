---
id: TASK-017-R4
kind: remediation
status: proposed
spec: SPEC-verified-change-contract
spec_revision: 72
requirements: [REQ-085, REQ-090, REQ-091, REQ-108, REQ-119, REQ-167, REQ-188, REQ-217, REQ-218, REQ-220, REQ-221, REQ-222, REQ-223, REQ-224, REQ-225, INV-021, AC-039, AC-071, AC-072]
depends_on: [TASK-017-R3]
parent_task: TASK-017
remediates: [FIND-NO-CANONICAL-JOURNEY, FIND-TABLE-NOT-DECLARABLE, FIND-REALTIME-NOT-RECORDED, FIND-NO-REQUEST-JOIN, FIND-REALTIME-ONLY-BINDING, FIND-UNBOUND-EVAL-SILENT, FIND-AGENT-NOT-INVOKABLE, FIND-GATEWAY-UNCORRELATED, FIND-JUDGE-BYPASSES-GATEWAY, FIND-TELEMETRY-SETUP]
---

# TASK-017 R4: One canonical support-desk journey, run the same way in Python, Rust, and TypeScript

## Authority and subject

- Proposed authority: spec revision 72. It is **not yet approved**, and this
  task must not start until the user approves it. Revision 72 follows
  revision 71 (TASK-017-R3), which must be approved and implemented first.
  The proposed spec text is in
  [Spec revision 72](#spec-revision-72-proposed-text) below.
- The user approved storing realtime verdicts (D6) on 2026-10-08, reversing
  the revision 51 and 64 "judge-only" decision for direct execution.
- Original task: [TASK-017](../../tasks/TASK-017-sdk-test-standard-and-cleanup.md).
- Builds on [R3](../TASK-017-r3/TASK-017-R3-principal-role-assignments.md):
  - the four persona Roles;
  - unbound Card tagging (R3 D18);
  - no `flush_bifrost` in SDK journeys (R3 D19);
  - the token-refreshing OTel exporter factories (R3 D21–D22).
- Wyrd has not shipped. Tables, migrations, and contracts are edited in
  place, and no alias or compatibility path is kept. This change edits no
  Bifrost table schema (D8), so no local database reset is needed.

## Why

The SDK journeys prove parts, never the product. No single journey shows a
developer the whole loop:

1. declare a Service;
2. register and download it;
3. run it;
4. verify it continuously and in real time;
5. record its own data;
6. export telemetry;
7. ask Wyrd, over MCP, what happened to one request.

Tracing that loop through the code found ten defects:

| Finding | Defect | Evidence |
|---|---|---|
| FIND-NO-CANONICAL-JOURNEY | No example or journey runs the loop end to end. The parts are spread across `observe_a_run`, `verify_in_real_time`, `scheduled_drift_alerts_operator`, and `otel_export`. No example runs against a server. | `fixtures/README.md` story table; `examples/README.md` |
| FIND-TABLE-NOT-DECLARABLE | A Service cannot declare the table it writes. The caller must build a `TableConfig` and call `register()` in code before `observe.record` stops answering `WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND`. | `ServiceSpec` (`crates/wyrd-spec/src/card/service.rs:12-52`) has no table field; `record_value` (`crates/shared/wyrd-client/src/observe/mod.rs:310`) |
| FIND-REALTIME-NOT-RECORDED | `observe.verify` stores nothing, so a realtime verdict cannot be queried later. | `crates/wyrd/wyrd-server/src/verification/direct.rs:1-8`; `components/verification/service.rs:348-349`; `wyrd-design.md:796-797` |
| FIND-NO-REQUEST-JOIN | A verdict cannot be joined back to the application request that produced its evidence. The managed `run_id` on `vala.verification.results` is the Verifier run. A queued Eval run carries only the observation's `record_id` (`verifier_runs.rs:905`; `eval.rs:566` does not select `run_id`). The direct request carries no run id (`ExecuteVerificationRequest`, `crates/wyrd-spec/src/verification.rs:512`). | as cited |
| FIND-REALTIME-ONLY-BINDING | A Verifier can be called in real time only if it is bound in `verified_by`, and every binding needs `runs_on`. A realtime-only Eval Verifier therefore also runs continuously on every observation. | `VerificationBinding.runs_on` (`crates/wyrd-spec/src/card/verifier.rs:303-315`); REQ-090 |
| FIND-UNBOUND-EVAL-SILENT | An Eval observation written by a principal bound to no Card stages no run (REQ-108, revision 61). Developing with the admin key or a saved login therefore never runs a continuous Verifier. | `ObservationEnqueue::acknowledged` (`crates/wyrd/wyrd-server/src/verification/observations.rs:126-128`) |
| FIND-AGENT-NOT-INVOKABLE | A Service's Agent cannot be run through the gateway from the SDKs. Only a Workflow step can use `llm_route: wyrd_gateway`. Python's standalone `Agent` calls the provider directly, and TypeScript and Rust have no Agent call at all. The adapter that would do it, `WyrdGatewayProvider`, is private to `skald-workflow`. | `crates/skald/skald-workflow/src/route.rs:281,468,543`; `sdks/wyrd-sdk-ts/native/src/cards.rs:355` |
| FIND-GATEWAY-UNCORRELATED | A gateway call row never names its request or Card. No ingress reads a run or Card header, and `calls_batch` writes `card_ref` and `run_id` as null. The capture frame's principal has no Card scope, so Scribe could not resolve a `card_uid` anyway. | `components/gateway/ingress.rs:209-290`; `capture.rs:404-407,587-604`; `workflow/gateway.rs:81-128` |
| FIND-JUDGE-BYPASSES-GATEWAY | The LLM judge calls providers from a registry built from the server's environment, not the gateway. Models must be configured twice, and judge calls have no gateway accounting or capture. | `state.rs:2143-2208`; `verification/eval.rs:98-125,294` |
| FIND-TELEMETRY-SETUP | No SDK sets up telemetry. Only Python stamps `wyrd.run_id`/`wyrd.card_ref` on spans, through `with run`. TypeScript and Rust spans never carry the Run, and every existing exporter uses gRPC with a static key. | `sdks/wyrd-sdk-python/python/wyrd/otel.py`; `sdks/wyrd-sdk-ts/wyrd/src/index.ts:2030,2191-2236`; `crates/shared/wyrd-client/src/observe/eval.rs:211-226` |

## Decisions

| ID | Decision |
|---|---|
| D1 | **One canonical example, run by the journeys.** `examples/support-desk/` holds the Service Cards and one application in each SDK. Each SDK's `support_desk` journey imports and runs that application's functions against `WyrdTestServer`. It never keeps a copy, so the example cannot drift from what the tests prove. |
| D2 | **The same four functions in every language:** `deploy`, `serve`, `wait_for_verdicts`, and `explain`, plus a `main` that runs them for 100 requests. Names follow each language's casing (`waitForVerdicts` in TypeScript). They differ only where the language does. Python opens a Run with `with state.run("agent") as run:`. TypeScript and Rust use `run.scope(...)`. Async is spelled per language. |
| D3 | **A Service declares its tables.** `ServiceSpec.tables: [{name, schema}]`, where `schema` is a JSON Schema in the subset `TableConfig.from_json_schema` already accepts (`wyrd_queue::json_schema_to_fieldspec`). Each entry names `vala.datasets.<name>`. |
| D4 | **The server ensures declared tables when it registers the Service.** Tables are ensured eagerly, following the built-in table rule (`bifrost-design.md:112-121`), never on first write. Registration refuses a declared table whose schema conflicts with an existing table of that name, before any write. Declaring tables also requires `bifrost_table:write`, the same way an Operator-bearing Card requires `operators:invoke`. Removing a Service never drops a table. |
| D5 | **`runs_on` becomes optional.** A binding with no `runs_on` never activates on its own. It can be invoked only by `observe.verify`, `verification.execute`, or a manual `verification.start_run`. That is how a realtime-only Verifier is declared. |
| D6 | **Realtime verdicts are stored** (user-approved). Every completed `observe.verify` / `verification.execute` writes its verdict to `vala.verification.results`, plus `vala.eval.result_items` or `vala.drift.result_features`, through the existing result writer. The write happens after the response is built and never delays or fails the caller. A loss is logged and counted, like the audit outbox. |
| D7 | **One id for a realtime verdict.** The Judgment's `execution_id` is also the stored verdict's `result_id`. There is still no `verifier_runs` row and no Operator dispatch. A realtime Drift verdict's window is the execution's own `[started_at, ended_at]`. |
| D8 | **LOCKED. The managed stamps are the join; no new column.** Result rows in `vala.verification.results`, `vala.eval.result_items`, and `vala.drift.result_features` keep the existing managed `run_id` and `card_uid` columns, with these meanings:<br>• Managed `run_id` = the **application Run** that produced the evidence:<br>  – continuous Eval: the observation's managed `run_id`;<br>  – realtime: the caller's Run (`ExecuteVerificationRequest.run_id`, D9);<br>  – scheduled Drift: null. Scribe already admits a nullable native `run_id` (`vala-bifrost-redux/src/scribe/execution_lanes.rs:389`).<br>• Managed `card_uid` stays the **Verifier** Card; Scribe derives it from the SYSTEM frame's `CardRefScope::own(verifier)` (`components/gateway/capture.rs:403-421`), unchanged.<br>• The payload column `subject_card_uid` names the judged Card, unchanged.<br>• A Verifier run reaches its verdict through the existing `wyrd.verifier_runs.result_id` (`wyrd-sql/src/queries/verifier_runs.rs:360`), which `verification.get_run` returns as `VerificationRun.result_id` (`wyrd-spec/src/verification.rs:372`).<br>No table schema, fingerprint, or migration changes. Today `ResultPayloadBuilder::assemble` stamps `ResultRun.run_id` (the Verifier run) on every row (`verification/results.rs:598-601`); that one value changes. `VerifierAttribution.run_id` stays the Verifier run: it is the peer-wire attribution (`capture.rs:258-316`), not the stamped column. |
| D9 | **Requests to the direct execution route carry the caller's Run.** `ExecuteVerificationRequest` gains an optional `run_id`. Every SDK's `observe.verify` sends its Run's id. The MCP `verification.execute` tool takes the same field. |
| D10 | **An unbound writer's Eval observation runs the subject's bindings.** For a writer bound to no Card (a user, the tenant admin key), each acknowledged Eval observation stages one run for every `observations_ready` binding whose subject is the observation's Card. The activity gate is skipped, because the observation itself is the activity. A Card-bound writer keeps the revision 61 rule: only bindings it owns, gated by activity. |
| D11 | **Gateway calls carry the Run.** Two request headers, `wyrd-run-id` and `wyrd-card-ref`, name the caller's Run and Card on any gateway ingress (OpenAI, Anthropic, and Google dialects). They must come together. The Card is authorized with the same rule as Scribe tagging (R3 D18): an unbound caller may name any registered observation-target Card, and a Card-bound caller only its own scope. Refusal happens before any upstream call. When capture is on, the `vala.gateway.calls` row then carries `run_id` and the Card, which Scribe resolves to `card_uid`. Both headers become reserved transport headers. A stock client may send them too. |
| D12 | **LOCKED. The LLM judge calls the gateway as the principal whose evidence it judges, through the one gateway permission check.**<br>• **One mechanism.** Every gateway call is checked by the existing `GatewayInvocation::decide` (`components/gateway/invocation.rs:623-659`): `gateway:invoke` for the model (and each fallback) against `caller.principal`, staged on the audit outbox. The judge adds no rule, no pre-authorization (`authorized = false`), and no special path. It uses the existing `ServerWyrdGatewayCaller::new(state, caller)` (`gateway/workflow.rs:49`), exactly as a server Workflow's `wyrd_gateway` step does (`workflow/host.rs:344`).<br>• **Which caller.**<br>  – Realtime: the request's own `Caller`.<br>  – Continuous: the principal that wrote the observation. Its id is the managed `principal_id` Scribe stamped on the `vala.eval.observations` row. `BifrostReader::record` (`verification/eval.rs:566`) adds it to the SELECT it already runs, the same way it adds `run_id` (D8). Nothing is stored on `verifier_runs`, and no permission is ever written to a table.<br>• **Principal cache (new; no Postgres per call).** The writer's token is gone by the time a queued run executes, so its authority is looked up through a principal cache.<br>  – Owner: `wyrd-auth`, beside `TokenIssuer::resolve` (`issuance.rs:506-568`).<br>  – Key: `(DataTenantId, PrincipalId)`.<br>  – Value: what an access token carries: the principal's kind, `PermissionSet`, and Card scope.<br>  – Hit: no IO.<br>  – Miss: one resolve through the existing issuance reads (tenant admits credentials, principal active, current roles, `resolve_permissions`, Card scope), with no activity recorded, then insert.<br>  – Expiry: each entry expires after the configured `access_ttl`, so cached authority never outlives what a token minted at the same moment would carry.<br>  – Store: the workspace `mini-moka` (`sync`, `Cargo.toml:149`); no new dependency.<br>  – Sharing: `AppState` holds one shared instance.<br>  – A missing or inactive principal is not cached.<br>  – Regular API requests keep verifying their self-contained JWT with no lookup; they do not use the cache.<br>• **Model and credentials.** The model is the judge Prompt's gateway model identity (`gateway_model(provider, model)`), served with the tenant's configured gateway credentials. The environment-built judge registry (`AppState.judge_providers`, `VerificationRuntimeBuilder::providers`) is deleted.<br>• **Refusal.** A writer that lacks `gateway:invoke`, or is inactive, gets the gateway's own refusal. The judge task fails and the run settles `errored`. There is no fallback to SYSTEM or to another principal.<br>• **Attribution.** The gateway's captured call names the writer as its caller (`CallFacts.caller`, `capture.rs:432`), so judge tokens and cost land on that principal. The call carries the application Run and the subject Card as its correlation (D11). Each run mints one `RequestId` for its judge calls' audit rows. |
| D13 | **`run.invoke(variables)` runs the Run's Agent once through the gateway.** It renders the Agent's Prompt with string variables and runs the Agent's loop (`run_config`) through the public gateway caller as the state's client. It returns the final text and stamps every gateway call with the Run (D11). It needs `gateway:invoke` for the model. An Agent with `tool_names`, or a Run whose Card is not an Agent, is refused before any call. Structured output and tools are out of scope. |
| D14 | **`state.start_telemetry()` sets up tracing in one call.** It installs the process-wide tracer provider. The provider has a batch span processor over R3's token-refreshing OTLP/HTTP span exporter (as the state's client) and a run-correlation processor. That processor stamps `wyrd.run_id` and `wyrd.card_ref` on every span started inside a Run scope. `state.shutdown()` flushes and closes the provider. A second call on the same state does nothing. If the process already has a tracer provider that Wyrd did not install, the call is refused with the new `WYRD_SDK_409_TELEMETRY_PROVIDER_EXISTS`. Callers who own their provider use R3's `span_exporter()` plus `install_run_correlation` instead. Only spans are covered; metrics and logs are not. |
| D15 | **A Run scope is an OpenTelemetry context value in every SDK:** Python `with run`, which already works this way; TypeScript `run.scope(async (run) => ...)`; Rust `run.scope(future)`. Rust's Eval trace capture falls back to the OpenTelemetry context when the current `tracing` span has none. |
| D16 | **MCP is queried with the protocol client each ecosystem has.** Rust uses `rmcp` with `wyrd_mcp::WyrdMcpHttpClient`. Python and TypeScript use the stock `mcp` / `@modelcontextprotocol/sdk` client if it supports the server's only revision, `2026-07-28` (`server/discover`, stateless). Otherwise the example's `explain` sends JSON-RPC `tools/call` over HTTP with the headers that revision requires. Either way, only `bifrost.query` is called, with a fresh access token. No SDK gains an MCP helper. |
| D17 | **The example sets up its own gateway.** `deploy` needs an admin, which local development has (R3 D20). It puts the capture policy `{mode: metadata}` and checks that the Prompt's model is deployed. If the model is not deployed, it fails naming the missing `provider/model` and the README step that adds it. The journeys deploy the model to the test receiver before calling `deploy`. |

## The example

### Layout

```text
examples/support-desk/
  README.md                       # the loop, run steps for each SDK, gateway setup, what each query proves
  service/
    support-desk.yaml             # Service: tables + the agent component and both bindings
    support-agent.yaml            # Agent → ./support-prompt.yaml, run_config.max_iterations: 1
    support-prompt.yaml           # Prompt: provider openai, model gpt-4o-mini, variable ${question}
    answer-quality.yaml           # Eval Verifier: 1 assertion + 1 llm_judge (continuous)
    judge-prompt.yaml             # judge Prompt: provider openai, model gpt-4o-mini, response {passed: bool}
    no-refund-promise.yaml        # Eval Verifier: 1 assertion (realtime)
  python/support_desk.py          # deploy, serve, wait_for_verdicts, explain, main
  typescript/support-desk.ts      # deploy, serve, waitForVerdicts, explain, main
  rust/support_desk.rs            # the same functions (a module), plus main.rs
```

Each language has its own runner in `mise.toml`:
- `examples:support-desk:python`;
- `examples:support-desk:typescript`;
- `examples:support-desk:rust` (a `wyrd-rust-examples` bin).

`check:examples` type-checks or builds all three. The examples README and
`docs/src/content/docs/how-to/` link to it as the first example.

### Service

```yaml
# support-desk.yaml
apiVersion: wyrd/v1
kind: Service
metadata: {space: examples, name: support-desk, version: 1.0.0}
spec:
  description: Answers support questions, verified continuously and in real time.
  service_type: agent
  tables:
    - name: tickets
      schema:
        type: object
        required: [ticket_id, question, answer, latency_ms]
        properties:
          ticket_id:  {type: string}
          question:   {type: string}
          answer:     {type: string}
          latency_ms: {type: integer}
  components:
    - alias: agent
      ref: ./support-agent.yaml
      verified_by:
        - verifier: ./answer-quality.yaml
          runs_on: {kind: observations_ready}
        - verifier: ./no-refund-promise.yaml          # no runs_on: realtime only (D5)
```

```yaml
# answer-quality.yaml (spec.implementation)
kind: eval
spec:
  pass_gate: {kind: all_pass}
  tasks:
    answered:
      kind: assertion
      id: answered
      context_path: $.answer
      operator: not_equals
      expected: ""
    judged:
      kind: llm_judge
      id: judged
      judge_ref: {prompt: ./judge-prompt.yaml, tool_names: [], run_config: {max_iterations: 1}}
      context_path: $.answer
      operator: equals
      expected: {passed: true}
      max_retries: 0
```

```yaml
# no-refund-promise.yaml (spec.implementation)
kind: eval
spec:
  pass_gate: {kind: all_pass}
  tasks:
    no_refund:
      kind: assertion
      id: no_refund
      context_path: $.answer
      operator: not_contains
      expected: refund
```

The implementer confirms the operator spellings against
`crates/wyrd-spec/src/vala/eval/operator.rs` and keeps the meaning.

### Application

The same steps in every language. Python:

```python
def deploy(client: WyrdClient, workdir: Path) -> Path:
    cards = Cards(client)
    Gateway(client).put_capture_policy({"mode": "metadata", "payload_fields": []})
    ref = cards.register_from_path(SERVICE / "support-desk.yaml")  # also creates vala.datasets.tickets
    return cards.hydrate(ref, workdir / "support-desk").destination

def serve(bundle: Path, client: WyrdClient, questions: list[str]) -> list[str]:
    state = WyrdState.from_path(bundle, client)
    state.start_bifrost()
    state.start_telemetry()
    tracer = trace.get_tracer("support-desk")
    run_ids = []
    try:
        for question in questions:
            with state.run("agent") as run, tracer.start_as_current_span("support-desk.request"):
                started = time.monotonic()
                answer = run.invoke({"question": question})
                run.observe.eval({"question": question, "answer": answer})
                run.observe.verify("no-refund-promise", {"answer": answer})
                run.observe.record("vala.datasets.tickets", {
                    "ticket_id": run.run_id, "question": question, "answer": answer,
                    "latency_ms": int((time.monotonic() - started) * 1000)})
                run_ids.append(run.run_id)
    finally:
        state.shutdown()  # drains rows and spans
    return run_ids

def wait_for_verdicts(client: WyrdClient, run_ids: list[str], timeout: float) -> None: ...
    # polls bifrost.query over MCP until vala.verification.results has 2 rows per run
    # (1 continuous answer-quality + 1 realtime no-refund-promise), else raises TimeoutError

def explain(client: WyrdClient, run_id: str) -> RequestEvidence: ...
    # the six queries below, over MCP bifrost.query
```

TypeScript:

```ts
for (const question of questions) {
  await state.run("agent").scope(async (run) => {
    await tracer.startActiveSpan("support-desk.request", async (span) => {
      const answer = await run.invoke({ question });
      run.observe.eval({ question, answer });
      await run.observe.verify("no-refund-promise", { answer });
      await run.observe.record("vala.datasets.tickets", { ticket_id: run.runId, question, answer, latency_ms });
      span.end();
    });
    runIds.push(run.runId);
  });
}
```

Rust:

```rust
for question in &questions {
    let run = state.run_for_card("agent")?;
    run.scope(async {
        let span = tracer.start("support-desk.request");
        let _active = opentelemetry::trace::mark_span_as_active(span);
        let answer = run.invoke([("question", question.as_str())]).await?;
        run.observe().eval(&json!({"question": question, "answer": answer}), Default::default())?;
        run.observe().verify("no-refund-promise", &json!({"answer": answer})).await?;
        run.observe().record("vala.datasets.tickets", &Ticket { ticket_id: run.run_id().to_string(), .. }).await
    }).await?;
    run_ids.push(run.run_id().clone());
}
```

The implementer owns the exact local code. The public calls above are the
contract.

### `explain`: one request, every signal

All of these go through MCP `bifrost.query`, as the client principal. `R` is
one run id and `A` is the Agent Card's uid. `explain` returns one typed record
per table.

| Signal | Query | Expected for one request |
|---|---|---|
| Eval observation | `SELECT record_id, trace_id, card_uid FROM vala.eval.observations WHERE run_id = 'R'` | 1 row, `card_uid = A` |
| Service record | `SELECT ticket_id, latency_ms, card_uid FROM vala.datasets.tickets WHERE run_id = 'R'` | 1 row, `ticket_id = R`, `card_uid = A` |
| Spans | `SELECT span_id, name, trace_id, card_uid FROM vala.traces.spans WHERE run_id = 'R'` | ≥ 1, including `support-desk.request`, every `card_uid = A`, and the observation's `trace_id` among them |
| LLM calls | `SELECT call_id, outcome, card_uid FROM vala.gateway.calls WHERE run_id = 'R'` | 1 row, `card_uid = A` |
| Verdicts | `SELECT result_id, verdict, card_uid, subject_card_uid, source_record_id FROM vala.verification.results WHERE run_id = 'R'` | 2 rows, both `subject_card_uid = A`, each `card_uid` = its Verifier: answer-quality with `source_record_id` = the observation's `record_id`, and no-refund-promise |
| Verdict detail | `SELECT task_id, passed FROM vala.eval.result_items WHERE result_id IN (<the two result_ids>)` | `answered`, `judged`, and `no_refund` |

`main` prints, for the first request, every row above. It also prints the
pass counts of both Verifiers over all 100 requests:

```sql
SELECT card_uid, verdict, COUNT(*) FROM vala.verification.results
 WHERE run_id IN (...) GROUP BY card_uid, verdict
```

## API contract

### Card spec (`crates/wyrd-spec/src/card/service.rs`, `verifier.rs`)

```rust
/// One Bifrost table a Service declares and the server ensures at registration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct ServiceTable {
    /// Table name inside `vala.datasets`; the same name rule `POST /v1/bifrost/tables` applies.
    pub name: String,
    /// JSON Schema of one row, in the subset `TableConfig.from_json_schema` accepts.
    pub schema: serde_json::Value,
}

// ServiceSpec gains, after `verified_by`:
#[serde(default, skip_serializing_if = "Vec::is_empty")]
pub tables: Vec<ServiceTable>,

// VerificationBinding.runs_on becomes:
#[serde(default, skip_serializing_if = "Option::is_none")]
pub runs_on: Option<InlineableRef<TriggerSpec>>,
```

- `wyrd_spec::graph::composition::spec_binding_errors` refuses a duplicate
  table name or an invalid name with `WYRD_SPEC_400_VALIDATION`. Schema
  mapping is checked on the server (wyrd-spec has no Arrow).
- The binding kind check (`composition.rs:164-190`) applies only when
  `runs_on` is present.
- Existing Cards keep their content hash, because both fields are omitted
  when empty.
- Regenerate with `mise run codegen:regen`: the `service_spec.json` and
  `card.json` schemas, Python `_card_types.py`, and TypeScript
  `card-types.ts`.

### Wire changes

| Type | Change |
|---|---|
| `ExecuteVerificationRequest` (`crates/wyrd-spec/src/verification.rs:512`) | `run_id: Option<RunId>` (`wyrd_spec::vala::ids::RunId`), `skip_serializing_if = "Option::is_none"` |
| Gateway ingress headers | `wyrd-run-id` (any non-empty string up to 128 bytes, matching the managed `run_id`) and `wyrd-card-ref` (`space/kind/name@version`, the `CardRef` text grammar). Both or neither. Added to `is_reserved_transport_header` (`crates/wyrd-spec/src/card/workflow.rs:617`). |
| `WyrdGatewayCall` (`skald-workflow`) | gains `subject: Option<GatewayCallSubject { run_id: RunId, card_ref: CardRef }>`. Workflow steps pass `None`, so they are unchanged. |

### Errors

| Condition | Code |
|---|---|
| Declared table conflicts with an existing table's fingerprint | existing `WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH`, with nothing written |
| Declared table schema outside the supported subset | existing `WYRD_VALA_400_SCHEMA_PARSE`, with nothing written |
| Declaring tables without `bifrost_table:write` | existing `WYRD_PERMISSION_403_DENIED_RBAC`, with nothing written |
| Server process has no Bifrost catalog | existing `ScribeRoleUnavailable` 503, as `POST /v1/bifrost/tables` returns |
| Only one of `wyrd-run-id` / `wyrd-card-ref`, or a malformed value | existing `WYRD_SPEC_400_VALIDATION` |
| `wyrd-card-ref` unregistered | existing `WYRD_VALA_403_CARD_UNRESOLVED` |
| `wyrd-card-ref` outside a Card-bound caller's scope, or a control-plane kind | existing `WYRD_VALA_403_BIFROST_CARD_SCOPE` |
| `run.invoke` on an Agent with `tool_names`, or on a non-Agent Run | existing `WYRD_SPEC_400_VALIDATION` |
| `run.invoke` without `gateway:invoke` for the model | existing `WYRD_PERMISSION_403_DENIED_RBAC` |
| `start_telemetry` when a foreign tracer provider is installed | **new** `WYRD_SDK_409_TELEMETRY_PROVIDER_EXISTS` (409, title "Telemetry provider already installed", remediation "Use span_exporter() and install_run_correlation(provider) with your own tracer provider") |

### SDK surfaces

| Operation | Python | TypeScript | Rust (`wyrd_sdk`) |
|---|---|---|---|
| Run the Agent | `run.invoke(variables: Mapping[str, str]) -> str` | `run.invoke(variables: Record<string, string>): Promise<string>` | `run.invoke(variables: impl IntoIterator<Item = (impl AsRef<str>, impl AsRef<str>)>) -> Result<String, WyrdError>` (async) |
| Start telemetry | `state.start_telemetry() -> None` | `state.startTelemetry(): Promise<void>` | `state.start_telemetry() -> Result<(), WyrdError>` (needs the `otel` feature) |
| Scope a Run | `with state.run(alias) as run:` (unchanged) | `run.scope<T>(fn: (run: Run) => Promise<T>): Promise<T>` | `run.scope<F: Future>(fut: F) -> impl Future<Output = F::Output>` |
| Verify | unchanged; now sends the Run id | unchanged; now sends the Run id | unchanged; now sends the Run id |

Notes:
- Python `invoke` releases the GIL and blocks on the shared runtime.
- TypeScript `Run` gains a native `cardRef` text getter, which the
  correlation processor needs.
- Rust `invoke` lives on `wyrd_client::observe::Run`.
- TypeScript's span setup uses `@opentelemetry/sdk-trace-base`,
  `@opentelemetry/context-async-hooks`, and R3's
  `@opentelemetry/exporter-trace-otlp-proto`, all optional
  `peerDependencies`.
- Rust `start_telemetry` lives in `wyrd-client` behind its `otel` feature
  (R3 Rust packaging), using `opentelemetry_sdk::trace::SdkTracerProvider`.
- The Rust correlation processor reads the scope from the span's parent
  `opentelemetry::Context`.

## Capability and reuse map

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Declared tables | `bifrost::service::register_table` (`wyrd-server/src/bifrost/service.rs:87`); `BifrostCatalog::register_dataset` (`bifrost_catalog.rs:822`); `wyrd_queue::json_schema_to_fieldspec` (`wyrd-queue/src/schema.rs:27`); `complete_card_inner` (`components/cards/service.rs:1758`) | `pg_card_registration_route.rs`; `bifrost::service::pg_tests` | The spec cannot declare a table; registration never ensures one | Split `register_table` into its permission check and an inner `ensure_dataset`. Registration validates and fingerprint-checks in `validate_request`, then calls `ensure_dataset` in `complete_card_inner` before `commit_card_activation`. | None; reuses the catalog and the mapper the server already links |
| Realtime-only binding | `VerificationBinding`; `BindingProjector`; scheduler; `ObservationEnqueue` | `pg_card_registration_route.rs`; `eval_verification.rs` | `runs_on` is required | `Option` field; a projection with no activation; scheduler and enqueue skip it | None |
| Unbound Eval runs | `ObservationEnqueue::acknowledged`; `VerifierRunQueue::enqueue_observation_batch` (`wyrd-sql/src/queries/verifier_runs.rs`) | `eval_runs_follow_the_writing_owner` (`eval_verification.rs:2381`) | An unbound writer stages nothing | `ObservationRecord.writer` becomes optional. The insert selects bindings by subject when there is no writer, and skips the activity gate in that branch. | None |
| Application Run on result rows (D8) | Managed `run_id` stamp in `ResultPayloadBuilder::assemble` (`verification/results.rs:598-601`), fed by `ResultRun.run_id` (`results.rs:177-205`); `BifrostReader::record` (`verification/eval.rs:558-617`), called in the Eval engine (`eval.rs:165`); `VerifierReport::Eval` (`verification/engines.rs:27-38`); `runner::stage` (`verification/runner.rs:866-910`) | `results.rs` unit test at `:940`; `verification_runtime.rs:136-165`; `eval_verification.rs:516` | The stamp is the Verifier run. The observation's `run_id` is never selected, and the engine's report does not carry it back to `stage`. | Add `run_id` to the existing record SELECT. Return it through the Eval engine's report to `stage`. Stamp it in `assemble` in place of the Verifier run. Direct execution stamps the request's `run_id`; scheduled Drift stamps null. | None. No column, schema, or migration; the existing managed column changes meaning. |
| Realtime persistence | `VerificationControl::execute` (`components/verification/service.rs:368-446`); `ResultPayloadBuilder`; `GatewayCapture::write_result` (`capture.rs:892`); `wyrd_runtime::outbox::Outbox` | `pg_verification_routes.rs:991-1361` | Nothing is stored | After the Judgment is built, encode with `ResultPayloadBuilder` and stage the batches on an `Outbox<DirectResultSink>` whose sink calls `write_result`. Batch ids are minted once at encode, so retries are deduplicated by Scribe. | One `OutboxSink` impl. The generic outbox is reused, and this is the documented non-blocking pattern (audit, Eval run requests). |
| Gateway correlation | Gateway ingress (`components/gateway/ingress.rs`); `GatewayCallRequest` (`invocation.rs:63`); `CallFacts` (`capture.rs:428`); `calls_batch` (`capture.rs:587`); the R3 D18 Card-scope authorizer | `wyrd-testing --test gateway`; `test_gateway_inference.py` | No header is read; rows are uncorrelated | Parse the headers at ingress, authorize the Card, carry `CallCorrelation` through the request and facts, write `run_id`/`card_ref`, and submit the frame with `CardRefScope::own(&card)` | None |
| Agent via gateway | `skald_agent::Agent::{from_resolved, with_run_config, run_prompt}`; `WyrdGatewayProvider` and `gateway_model` (`skald-workflow/src/route.rs`, private); `PublicWyrdGatewayCaller` (`wyrd-client/src/workflow/gateway.rs`); `WyrdState::agent_prompt` (`state.rs:987`) | `gateway_inference` stories; `SkaldJudgeInvoker::agent_for` (`vala-eval/src/orchestrator/judge.rs:173`) | No public single-Agent gateway path | Make `WyrdGatewayProvider` and `gateway_model` public and add the optional `subject`. `Run::invoke` builds the Agent the way `agent_for` does, with a one-adapter registry. | None; reuses the Workflow's adapter |
| Judge via gateway | `ServerWyrdGatewayCaller` (`components/gateway/workflow.rs:40`); `GatewayInvocation::decide` (`invocation.rs:623`); `SkaldJudgeInvoker` (`vala-eval/src/orchestrator/judge.rs:48,204`); `TokenIssuer::resolve` (`wyrd-auth/src/issuance.rs:506-568`) | `direct_execution_calls_the_judge_provider` (`pg_verification_routes.rs:1361`); `judged_answer_passes_the_llm_judge` journeys | The judge uses a separate provider registry built from the server environment. A queued run has no caller, and the server has no principal cache: commit `696f34c4e` removed the verified-token cache. | The judge's registry becomes the public `WyrdGatewayProvider` over `ServerWyrdGatewayCaller` with the realtime request's `Caller`, or the observation writer's `Caller` from the principal cache. `decide` is unchanged. Delete `judge_providers`. | A principal cache, because a queued run holds no token and Postgres must stay off the per-call path. It reuses the issuance reads and the installed `mini-moka`. |
| Telemetry | Python `wyrd.otel` (`_RunCorrelationProcessor`, `install_run_correlation`); R3 exporter factories; `WyrdState` | `test_otel_export.py`; `test_observation_surface.py` `otlp_tracer` | No one-call setup; no Run scope in TS or Rust | `start_telemetry` composes R3's `span_exporter` with the existing (Python) or a ported (TS, Rust) correlation processor | The TS and Rust processors are ports of the Python one, required by D15 |
| MCP from the example | `wyrd_mcp::WyrdMcpHttpClient` (`crates/wyrd/wyrd-mcp/src/client.rs:63`); `bifrost.query` | `wyrd-mcp/tests/bifrost/mcp/connectivity.rs:107-160` | Python and TS have no client | Stock client, or a minimal JSON-RPC call in the example only (D16) | Example-only code |

## Ordered implementation scenarios

Every server scenario runs with tracing on (`WYRD_LOG=info,wyrd_server=debug`)
when it fails (AGENTS.md §11).

### Scenario 1 — A Service's declared table exists when registration returns

**Behavior.** Registering a Service that declares `tickets` creates
`vala.datasets.tickets` with the mapped schema before the Card is Active.
Registering it again (same or new version, same schema) succeeds. A
conflicting schema, an unsupported schema, or a caller without
`bifrost_table:write` is refused with nothing written (REQ-217, D3, D4).

**RED.** Add to `crates/wyrd/wyrd-server/tests/integration/pg_card_registration_route.rs`:
- `declared_service_table_exists_when_registration_returns`: register, then
  `describe_table` returns the fields;
- `conflicting_declared_table_leaves_no_registration_writes`: pre-register
  `vala.datasets.tickets` with another schema, then expect a 409 and
  `assert_no_registration_writes`;
- `declaring_a_table_requires_bifrost_table_write`: a key without
  `bifrost_table:write` gets a 403 and no writes.

They fail because `tables` is an unknown field (`deny_unknown_fields`).
Also add a wyrd-spec unit test, `service_tables_reject_duplicate_names`.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-server --test integration --test-threads=1 -E 'test(=pg_card_registration_route::declared_service_table_exists_when_registration_returns) | test(=pg_card_registration_route::conflicting_declared_table_leaves_no_registration_writes) | test(=pg_card_registration_route::declaring_a_table_requires_bifrost_table_write)'"
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(/service_tables_reject_duplicate_names$/)'
```

**GREEN.**
- Add `ServiceTable` and `ServiceSpec.tables`, with name and uniqueness
  checks in `spec_binding_errors`.
- In `validate_request`, map each schema with `json_schema_to_fieldspec`
  and compare it with any existing table's fingerprint (via
  `catalog.describe_table`).
- Require `bifrost_table:write` in `register_card_http` when any submitted
  Service declares tables.
- Split `register_table` into its check and `ensure_dataset`, and call
  `ensure_dataset` for each declared table in `complete_card_inner` before
  `commit_card_activation`.
- A transient ensure failure follows the blob branch's reconcile scheduling.
- Regenerate codegen.

**REFACTOR.** `POST /v1/bifrost/tables` and registration share
`ensure_dataset` and the fingerprint comparison. Neither keeps its own copy.
Rerun `bifrost::service::pg_tests`.

### Scenario 2 — A binding without `runs_on` is realtime only

**Behavior.**
- A Service binding with no `runs_on` registers and projects.
- `observe.verify` / `verification.execute` can run it.
- An Eval observation on its subject stages no run for it.
- The scheduler never claims it.

(REQ-220, D5.)

**RED.**
- `binding_without_runs_on_registers_and_never_activates` in
  `pg_card_registration_route.rs`: registers, the projected binding has no
  activation, and `verification.get_binding` reports it.
- `observation_never_runs_a_realtime_only_binding` in
  `crates/wyrd/wyrd-testing/tests/bifrost/server/eval_verification.rs`:
  emit an Eval observation, then the realtime-only binding has zero runs
  while the `observations_ready` binding has one.

Both fail at deserialization, because `runs_on` is required.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-server --test integration --test-threads=1 -E 'test(=pg_card_registration_route::binding_without_runs_on_registers_and_never_activates)'"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=eval_verification::observation_never_runs_a_realtime_only_binding)'"
```

**GREEN.**
- Make `runs_on` optional.
- Run the kind-pairing check only when it is present.
- Make the binding projection's activation columns nullable, editing the
  migration in place.
- The scheduler and the observation insert select only bindings with the
  matching activation, which already excludes a null.

**REFACTOR.** None beyond removing any now-dead "runs_on required" branch.
Rerun Scenario 1.

### Scenario 3 — An unbound writer's Eval observation runs its subject's bindings

**Behavior.**
- An Eval observation written with the admin key, tagged with the Agent
  Card, stages one run for each `observations_ready` binding on that
  subject, with no activity check.
- A Card-bound writer still runs only bindings it owns, gated by activity.
- A replayed batch stages nothing new.

(REQ-221, D10.)

**RED.** In `eval_verification.rs`:
- `unbound_writer_observation_runs_the_subjects_binding`: admin key, one
  observation, then one completed result. It fails today because
  `acknowledged` returns early with no writer.
- `eval_runs_follow_the_writing_owner` must stay green.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=eval_verification::unbound_writer_observation_runs_the_subjects_binding) | test(=eval_verification::eval_runs_follow_the_writing_owner)'"
```

**GREEN.**
- `ObservationRecord.writer` becomes `Option<CardUid>`.
- `acknowledged` stages records for unbound writers too, with the record's
  subject taken from the row's `card_uid`.
- `enqueue_observation_batch` selects bindings by owner when there is a
  writer, else by subject. Only the owner branch keeps the activity gate.
- The `(tenant, binding, record)` uniqueness key is unchanged.

**REFACTOR.** Update the module docs in `observations.rs` to state both
rules. Rerun Scenario 2.

### Scenario 4 — A continuous verdict names the request that produced it

**Behavior.** A continuous Eval result's summary and detail rows carry the
observation's managed `run_id` as their own managed `run_id`. A scheduled
Drift result's rows carry a null `run_id`. On both, the managed `card_uid`
is the Verifier, and `verification.get_run` still reaches the verdict by
`result_id` (REQ-119, D8). No table schema changes: the
`vala-bifrost-redux` schema tests are untouched and must stay green.

**RED.**
- Add `continuous_result_carries_the_observations_run_id` in
  `crates/wyrd/wyrd-testing/tests/bifrost/server/eval_verification.rs`.
  - Record an observation under Run `R`.
  - Wait for the continuous run to complete.
  - Assert that `vala.verification.results` and `vala.eval.result_items`
    `WHERE run_id = 'R'` return that run's `result_id`, with
    `card_uid` = the Verifier and `subject_card_uid` = the writer's Card.
  - It fails today because the rows carry the Verifier run.
- Change the `results.rs` unit assertion at `:940` to expect the run the
  result is attributed to: the application Run when there is one, else
  null. It fails today.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=eval_verification::continuous_result_carries_the_observations_run_id)'"
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/verification::results::tests::/)'
```

(The implementer confirms the `results.rs` test names with `cargo nextest
list` and replaces the pattern with exact `test(=...)` selectors.)

**GREEN.**
- Add `run_id` to the SELECT in `BifrostReader::record`
  (`verification/eval.rs:566`).
- Carry the value from the Eval engine (`eval.rs:165`) back to
  `runner::stage` through its completed report.
- `ResultRun` takes the application Run in place of the Verifier run as
  the value `assemble` stamps (`results.rs:598-601`). `From<&ClaimedRun>`
  supplies null for Drift. The direct path (Scenario 5) supplies the
  request's `run_id`.
- Leave `VerifierAttribution.run_id` as the Verifier run.
- Re-key the existing tests that find results by the Verifier run onto
  `result_id`, taken from the run's `VerificationRun.result_id`:
  - `verification_runtime.rs:136-165`. The scheduled Drift rows now carry
    a null `run_id`, so the identity check asserts `run_id IS NULL`, and
    the join at `:165` joins on `result_id` alone.
  - `assert_unresulted` in `eval_verification.rs:516`. It asserts that no
    row exists for the observation's record (`source_record_id`) under
    that binding.
- Update the docs that say "managed `run_id` is the Verifier run":
  - `vala-bifrost-redux/src/tables/verification/results.rs:23,33`;
  - `tables/eval/result_items.rs:29`;
  - `tables/drift/result_features.rs:30`;
  - the `verification/results.rs` module docs at `:9` and `ResultRun.run_id`
    at `:178`.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(/^verification_runtime::/) | test(/^eval_verification::/)'"
```

**REFACTOR.** None.

### Scenario 5 — A realtime verdict is stored under the caller's Run

**Behavior.**
- `verification.execute` with `run_id = R` returns the same Judgment as
  today.
- Then `vala.verification.results` has a row with
  `result_id = execution_id`, managed `run_id = R`, managed
  `card_uid` = the Verifier, and
  `subject_card_uid = <subject>`, plus its detail rows.
- There is still no `verifier_runs` row and no dispatch.
- When the result write fails, the response is unchanged and the loss is
  counted (REQ-218, D6, D7, D9).

**RED.** In `crates/wyrd/wyrd-server/tests/integration/pg_verification_routes.rs`:
- `direct_execution_stores_its_verdict_under_the_callers_run`: query until
  the row appears, bounded by the test's own deadline.
- `direct_execution_result_loss_never_fails_the_caller`: inject a failing
  result sink through the existing capture test seam
  (`write_result_payload_for_test`), then expect a 200 and the loss counter
  incremented.
- `direct_execution_judges_inline_without_runs` keeps asserting no
  `verifier_runs` row.

The first test fails because no row is written. Also add the wyrd-spec
round-trip test
`execute_request_round_trips_an_optional_run_id`.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-server --test integration --test-threads=1 -E 'test(=pg_verification_routes::direct_execution_stores_its_verdict_under_the_callers_run) | test(=pg_verification_routes::direct_execution_result_loss_never_fails_the_caller) | test(=pg_verification_routes::direct_execution_judges_inline_without_runs)'"
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(/execute_request_round_trips_an_optional_run_id$/)'
```

**GREEN.**
- Add `run_id` to `ExecuteVerificationRequest`.
- `resolve_direct` also reads the tenant SYSTEM principal in its existing
  transaction.
- After the Judgment is built, build a `ResultRun`:
  - `run_id` is the execution id, converted to `VerificationRunId`;
  - no binding, owner, or trigger;
  - Eval: `RunInput::EvalRecord { record_id: execution_id, event_time: started_at }`;
  - Drift: `RunInput::DriftWindow([started_at, ended_at])`.
- Encode it, then stage on the new `Outbox<DirectResultSink>`, counted under
  `outbox_events_lost_total{outbox="direct_verification_results"}`.
- The MCP `verification.execute` tool and the OpenAPI document pick up the
  field from the shared type.
- The SDKs' `observe.verify` sends `run_id`.

**REFACTOR.** Update the doc comments that say nothing is stored:
- `direct.rs:4-7`;
- `service.rs:348-349`;
- `ids.rs:373-377`;
- `observe/verify.rs:7-8` and `observe/mod.rs:336-337`;
- `mcp/verification.rs:141`;
- the Python stubs and the TypeScript docstring at `index.ts:2157`, through
  codegen.

Rerun Scenario 4.

### Scenario 6 — A gateway call row names its request and Card

**Behavior.**
- With capture on, a call carrying `wyrd-run-id: R` and
  `wyrd-card-ref: <agent ref>` writes a `vala.gateway.calls` row with
  `run_id = R` and `card_uid = A`.
- A call with neither header is unchanged.
- Only one header, a malformed value, an unregistered Card, or (for a
  Card-bound key) a foreign Card is refused before any upstream call.

(REQ-222, D11.)

**RED.** Add `capture.rs` tests (module chosen by the implementer) in
`crates/wyrd/wyrd-testing/tests/gateway/`:
- `correlated_call_is_captured_under_its_run_and_card`;
- `half_correlated_call_is_refused_before_upstream`, where the receiver
  sees zero calls;
- `unregistered_card_ref_is_refused_before_upstream`.

The first fails because `run_id` is null.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && WYRD_TEST_GATEWAY_PROVIDER_KEY=sk-native-upstream cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E 'test(/correlated_call_is_captured_under_its_run_and_card$/) | test(/half_correlated_call_is_refused_before_upstream$/) | test(/unregistered_card_ref_is_refused_before_upstream$/)'"
```

**GREEN.**
- Parse the two headers in the shared ingress extraction.
- Authorize the Card through the R3 D18 authorizer.
- Add `correlation: Option<CallCorrelation>` to `GatewayCallRequest` and
  `CallFacts`.
- `calls_batch` writes the two columns.
- `into_frame` uses `CardRefScope::own(&card_ref)` when correlated.
- Reserve both headers.

**REFACTOR.** The OTLP, Scribe, and gateway Card checks call one authorizer;
no copy is made.

### Scenario 7 — The LLM judge calls the gateway

**Behavior.**
- A realtime `llm_judge` task calls the judge model through the gateway
  as the request's caller.
- A continuous one calls it as the principal that wrote the observation.
- The receiver sees the call with the tenant's gateway provider credential.
- With capture on, a `vala.gateway.calls` row names that principal as its
  caller and carries the application Run and the subject Card.
- The gateway's ordinary `decide` runs, and its audit row names the same
  principal.
- A writer without `gateway:invoke` on the judge model gets the gateway's
  stable refusal: the task fails and the run settles `errored`, with no
  fallback.
- An undeployed judge model behaves the same way, and nothing hangs.
- While a principal-cache entry is live, a second continuous run for the
  same writer reads no Postgres for authority.

(REQ-223, D12.)

**RED.**
- Change `direct_execution_calls_the_judge_provider`
  (`pg_verification_routes.rs:1361`) to deploy `openai/gpt-test` to the
  receiver and assert the captured call row's caller and correlation.
- Add, in `pg_verification_routes.rs`:
  - `judge_without_a_deployed_model_settles_as_error`;
  - `continuous_judge_calls_the_gateway_as_the_observation_writer`
    (writer with `gateway:invoke`: call row caller = writer);
  - `continuous_judge_refused_when_the_writer_lacks_gateway_invoke`
    (writer without it: run `errored`, a `Denied` gateway audit decision
    for the writer, no upstream call).
- Add unit tests in `wyrd-auth` for the principal cache:
  - a hit performs no read;
  - an entry expires after `access_ttl`;
  - an inactive principal is not cached.
  The implementer places them and pins exact `test(=...)` selectors with
  `cargo nextest list`.

All fail while the judge uses `judge_providers`.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-server --test integration --test-threads=1 -E 'test(=pg_verification_routes::direct_execution_calls_the_judge_provider) | test(=pg_verification_routes::judge_without_a_deployed_model_settles_as_error) | test(=pg_verification_routes::continuous_judge_calls_the_gateway_as_the_observation_writer) | test(=pg_verification_routes::continuous_judge_refused_when_the_writer_lacks_gateway_invoke)'"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-auth -E 'test(/principal_cache/)'"
```

**GREEN.**
- Make `WyrdGatewayProvider` and `gateway_model` public in `skald-workflow`.
- In `wyrd-auth`:
  - expose a resolve-by-principal-id path that reuses the issuance reads in
    `TokenIssuer::resolve` and records no activity;
  - add the principal cache over it, using the workspace `mini-moka` with
    entries expiring at `access_ttl`.
- `AppState` holds one cache.
- `EvalEngine` builds its judge registry from `WyrdGatewayProvider` over
  `ServerWyrdGatewayCaller::new(state, caller)`:
  - direct execution passes the request `Caller`;
  - a continuous run adds `principal_id` to `BifrostReader::record`'s SELECT
    and builds the writer's `Caller` from the cache.
- The subject is the application Run and the subject Card (D11).
- Delete `AppState.judge_providers`, `VerificationRuntimeBuilder::providers`,
  and their environment wiring.
- The test harness deploys the judge model wherever it configured a judge
  provider before, and grants the observation writer `gateway:invoke` on
  it. Every existing judge test (`judged_answer_passes_the_llm_judge` in all
  three SDKs and the Eval server tests) stays green.

**REFACTOR.** Remove the now-unused provider-registry plumbing from the
verification runtime builder.

### Scenario 8 — `run.invoke` runs the Agent through the gateway

**Behavior.**
- `run.invoke({"question": q})` on an Agent Run returns the model's text.
- The call is made as the state's client and carries the Run (Scenario 6
  headers).
- A `viewer` client is refused with `WYRD_PERMISSION_403_DENIED_RBAC`.
- An Agent with tools, or a non-Agent Run, is refused with
  `WYRD_SPEC_400_VALIDATION` before any call.

(REQ-224, D13.)

**RED.**
- A Rust unit test in `crates/shared/wyrd-client/src/observe/tests.rs`:
  `invoke_refuses_an_agent_with_tools` (no server).
- The journey `support_desk::agent_answers_through_the_gateway_under_its_run`
  in each SDK (Scenario 10). It fails because `invoke` does not exist.

```bash
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(/observe::tests::invoke_refuses_an_agent_with_tools$/)'
```

**GREEN.**
- `Run::invoke` in `wyrd-client`:
  - resolve the Agent card and its Prompt;
  - check `tool_names` is empty;
  - build the skald Agent with its run config;
  - build a one-adapter registry, `WyrdGatewayProvider` over
    `PublicWyrdGatewayCaller` with the state's client, model
    `gateway_model(prompt)`, and subject (run id, subject CardRef);
  - `run_prompt` with the variables, and return the output text.
- `PublicWyrdGatewayCaller` sends the two headers when `subject` is set.
- Python binding: a `PyRun.invoke` that releases the GIL.
- TypeScript: an async native `invoke`, plus the JS wrapper.
- Stubs and declarations come from codegen.

**REFACTOR.** None.

### Scenario 9 — `start_telemetry` exports spans stamped with the Run

**Behavior.**
- After `start_telemetry()`, a span started inside a Run scope reaches
  `vala.traces.spans` with `run_id = R` and `card_uid = A`, using the
  state's client token, which renews (R3).
- A span outside any scope has a null `run_id`.
- A second call does nothing.
- A foreign global provider is refused with
  `WYRD_SDK_409_TELEMETRY_PROVIDER_EXISTS`.

(REQ-225, D14, D15.)

**RED.**
- The journey `support_desk::spans_carry_their_request` in each SDK
  (Scenario 10).
- Python unit test `test_start_telemetry_refuses_a_foreign_provider`, in a
  subprocess so the global provider stays clean.
- TypeScript unit `start telemetry refuses a foreign provider`.
- Rust unit `observe::tests::scope_stamps_run_on_spans`, using an in-memory
  exporter, which proves the processor reads the context value.

```bash
mise exec -- bash -lc 'cd sdks/wyrd-sdk-python && uv run python -m pytest -q tests/unit/test_otel.py::test_start_telemetry_refuses_a_foreign_provider'
mise exec -- bash -lc 'cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/unit/otel.test.ts -t "start telemetry refuses a foreign provider"'
mise exec -- cargo nextest run --locked -p wyrd-client --lib --features otel -E 'test(/observe::tests::scope_stamps_run_on_spans$/)'
```

(Unit file paths are the implementer's choice. Keep the exact selector.)

**GREEN.**
- **Python:** `WyrdState.start_telemetry` calls a `wyrd.otel` helper that
  builds the `TracerProvider` with `BatchSpanProcessor(span_exporter(client))`
  and `_RunCorrelationProcessor`, then sets it globally.
- **TypeScript:** `startTelemetry` registers an
  `AsyncLocalStorageContextManager` and a `BasicTracerProvider` with
  `spanExporter({ client })` and a correlation processor reading the scope
  key. `run.scope` runs its function in `context.with(...)`.
- **Rust:** `start_telemetry` builds an `SdkTracerProvider` with the
  batch-exported R3 span exporter and a correlation `SpanProcessor`, and sets
  it globally. `Run::scope` wraps the future with `FutureExt::with_context`.
  `active_span_identity` falls back to `Context::current()`.
- In every SDK, `shutdown()` flushes and closes the provider Wyrd installed.
- Register the new catalog code.

**REFACTOR.** Python's `with run` and the new processors share one
attribute-key definition per language. Rerun `test_otel_export.py` and
`test_observation_surface.py`.

### Scenario 10 — The canonical example runs the whole loop in every SDK

**Behavior.** Each SDK's `support_desk` journey:
- deploys the receiver-backed model;
- calls the example's `deploy`;
- calls `serve` with 100 questions, where 10 contain "refund" and the
  receiver answers those with a refund promise;
- calls `wait_for_verdicts`;
- calls `explain` over MCP for one passing and one failing request.

It asserts the [`explain` table](#explain-one-request-every-signal):
- 100 + 100 verdict rows;
- `no-refund-promise` failed exactly 10 times;
- `answer-quality` passed 100 times.

(AC-071, FIND-NO-CANONICAL-JOURNEY.)

**RED.** Add `support_desk` to `fixtures/README.md`. It registers
`examples/support-desk/service/support-desk.yaml`, plus
`fixtures/cards/support_desk/conflicting-tickets.yaml` for the refusal. Add
these tests:

| Test | Asserts |
|---|---|
| `declared_table_exists_after_deploy` | `vala.datasets.tickets` describes with the four fields before `start_bifrost` |
| `agent_answers_through_the_gateway_under_its_run` | one request: the answer equals the receiver's text; one `vala.gateway.calls` row with that `run_id` and `card_uid = A` |
| `spans_carry_their_request` | `support-desk.request` span with `run_id`; the Eval observation's `trace_id` matches it |
| `every_signal_of_a_request_joins_by_run` | the full `explain` table for a passing and a failing request |
| `verdict_counts_cover_every_request` | 100 continuous passes; 90 realtime passes and 10 failures |
| `conflicting_table_schema_is_refused` | registering `conflicting-tickets.yaml` gets `WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH` |
| `viewer_cannot_invoke_the_agent` | `WYRD_PERMISSION_403_DENIED_RBAC` from `run.invoke` with a `viewer` key |

The files are:
- Python: `sdks/wyrd-sdk-python/tests/integration/test_support_desk.py`;
- TypeScript: `sdks/wyrd-sdk-ts/wyrd/tests/integration/support-desk.test.ts`;
- Rust: `sdks/wyrd-sdk-rust/tests/integration/support_desk.rs`, which includes
  the example module by `#[path]`. That avoids a crate cycle with
  `wyrd-rust-examples`.

All of them fail while the example does not exist.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_support_desk.py"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise run ts:build && mise run ts:build:testing && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/support-desk.test.ts"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E 'test(/^support_desk::/)'"
```

The verification lane runs each test by its exact name. For example:
`-E 'test(=support_desk::every_signal_of_a_request_joins_by_run)'`, pytest
`::test_every_signal_of_a_request_joins_by_run`, and vitest
`-t "every signal of a request joins by run"`.

**GREEN.**
- Write the example (layout above) and its README.
- Add the `mise` runners and `check:examples` coverage.
- Add the optional example dependencies:
  - Python: `mcp` in a new `examples` dependency group, only if D16's check
    passes;
  - TypeScript: `@modelcontextprotocol/sdk` as an example dev dependency,
    under the same check;
  - Rust: `wyrd-mcp` and `rmcp` dev-dependencies of `wyrd-sdk-rust` and
    dependencies of `wyrd-rust-examples`.
- Add the story to `test:bifrost:journey:python` and `:typescript`.

**REFACTOR.** None. Delete nothing that another story still proves.

### Scenario 11 — Stories that asserted realtime verdicts are not stored

**Behavior.** `verify_records_no_observation` still proves that verify
writes no observation. A new assertion in the same test proves the verdict
is stored under the caller's Run (REQ-218, AC-039).

**RED.** Extend `verify_records_no_observation` in all three SDKs:
- Python `tests/integration/test_verify_in_real_time.py:96`;
- TypeScript `verify-in-real-time.test.ts:168`;
- Rust `verify_in_real_time.rs:436`.

The new assertion: one `vala.verification.results` row with
managed `run_id = run_id`. It fails before Scenario 5 lands.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_verify_in_real_time.py::test_verify_records_no_observation"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise run ts:build && mise run ts:build:testing && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/verify-in-real-time.test.ts -t 'verify records no observation'"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E 'test(=verify_in_real_time::verify_records_no_observation)'"
```

**GREEN.** Done by Scenario 5. Rename each test to
`verify_stores_its_verdict_but_no_observation`, here and in
`fixtures/README.md`.

**REFACTOR.** None.

## Spec revision 72 (proposed text)

- **REQ-220**, new (revises REQ-090 and REQ-091): a binding contains one required
  `verifier`, an optional `runs_on`, and zero or more `on_failure`
  Operators. A binding without `runs_on` never activates on its own. It runs
  only when invoked through `observe.verify`, `verification.execute`, or a
  manual `verification.start_run`. The `schedule`/`observations_ready` kind
  pairing applies when `runs_on` is present.
- **REQ-221**, new (revises REQ-108; amends revision 61): a Card-bound writer's Eval
  observation runs only bindings that Card owns, gated by owner activity. An
  unbound writer's Eval observation (a user or the tenant admin key) runs
  every `observations_ready` binding whose subject is the observation's
  Card, with no activity gate, because the observation is the activity.
- **REQ-218**, new (revises REQ-167 and REQ-188):
  - Direct execution still creates no durable run, dispatches no Operator,
    applies no sampling, reads no Bifrost, and does not require Bifrost
    startup.
  - Every completed judgment is stored after the response is built: one
    `vala.verification.results` row plus its detail rows, with
    `result_id = execution_id`.
  - The write never delays or fails the caller. A loss is logged and
    counted.
  - The request may carry the caller's `run_id`, which every SDK's
    `observe.verify` sends.
- **REQ-085 / REQ-119 / REQ-121**, revised:
  - A result row is written for every completed Verifier run and every
    completed direct execution.
  - REQ-119's "managed `run_id` MUST be the exact verification run ID" is
    replaced. On every result row (summary and detail), the managed
    `run_id` is the application Run that produced the evidence:
    - continuous Eval: the observation's managed `run_id`;
    - direct execution: the request's `run_id`;
    - otherwise null.
  - The managed `card_uid` remains the Verifier Card, and
    `subject_card_uid` names the judged Card.
  - A Verifier run reaches its verdict by `result_id`, which
    `verification.get_run` returns.
  - For a direct execution, `binding_id` and `owner_card_uid` are null.
  - A realtime Drift result's window is the execution's own start and end.
- **INV-021**, note: a direct execution has exactly one result. Its retries
  reuse the batch ids minted at encode, in memory. Durability across
  process loss is not promised for direct results.
- **REQ-217**, new: **Declared tables.** A Service may declare
  `tables: [{name, schema}]`, where `schema` is a JSON Schema row in the
  subset `TableConfig.from_json_schema` accepts.
  - Registration needs `bifrost_table:write` when tables are declared.
  - It refuses a conflicting or unsupported schema before any write.
  - It ensures every `vala.datasets.<name>` exists before the Service
    becomes Active.
  - Removing a Service never drops a table.
- **REQ-222**, new: **Gateway calls carry the Run.**
  - Any gateway ingress accepts `wyrd-run-id` and `wyrd-card-ref` together.
  - The Card is authorized by the REQ-215 tagging rule, and refusal happens
    before any upstream call.
  - A captured call row carries `run_id` and the Card's `card_uid`.
  - Both headers are reserved transport headers.
- **REQ-223**, new: **The LLM judge uses the gateway.**
  - Judge Prompts run through the in-process gateway, using the judge
    Prompt's gateway model and the tenant's gateway credentials.
  - The caller is:
    - the request's caller, for a realtime judgment;
    - the principal that wrote the observation, for a continuous one.
  - The gateway authorizes the judge call with the same `gateway:invoke`
    check it applies to every caller. There is no pre-authorization or
    fallback principal, and a refusal fails the task.
  - Admission, accounting, capture, and audit apply and name that caller.
  - There is no separate judge provider configuration.
  - A queued run takes the writer's authority from a server principal
    cache. The cache holds no entry longer than the access-token TTL and
    adds no Postgres read on a hit.
- **REQ-224**, new: **`run.invoke`.** A Run on an Agent Card runs that Agent
  once through the gateway with string variables, as the state's client,
  stamped with the Run, and returns the final text. It is identical in
  Python, TypeScript, and Rust. An Agent with tools is refused.
- **REQ-225**, new: **`start_telemetry` and Run scopes.**
  - `state.start_telemetry()` installs the process tracer provider. It
    exports spans over OTLP/HTTP with a per-export token and stamps
    `wyrd.run_id` / `wyrd.card_ref` on every span started inside a Run
    scope.
  - A Run scope is Python `with run`, TypeScript `run.scope(fn)`, or Rust
    `run.scope(future)`.
  - `shutdown()` flushes the provider.
  - A foreign provider answers `WYRD_SDK_409_TELEMETRY_PROVIDER_EXISTS`.
- **AC-039**, revised: the direct-execution journeys prove that no durable
  run, dispatch, or observation is created, and that exactly one verdict row
  is stored under the caller's Run.
- **AC-071**, new:
  - `examples/support-desk` is the canonical example.
  - In each SDK, the `support_desk` journey runs it against a real server:
    one Service with one Agent, a gateway Prompt, one continuous Eval
    Verifier (assertion plus LLM judge), one realtime Verifier, and one
    declared table; 100 requests.
  - It proves, over MCP `bifrost.query` alone, that the Eval observation,
    service record, spans, gateway call, and both verdicts of one request
    join by its `run_id` and carry the Agent's `card_uid`.
- **AC-072**, new: server tests prove each of the following:
  - declared-table creation and its refusals;
  - realtime-only bindings;
  - unbound-writer Eval runs;
  - the application Run as the managed `run_id` on both result kinds;
  - stored realtime verdicts that never block the caller;
  - gateway correlation and its refusals;
  - judge calls through the gateway.
- **Architecture:**
  - `architecture/wyrd-design.md:796-797` (judging stores its verdict),
    plus the declared-table and gateway-correlation rules;
  - `architecture/bifrost-design.md` §"Table and row identity" (declared
    user tables are ensured at registration);
  - `changes/active/verified-change-contract/architecture/logic/table_schema.md`
    and `docs/src/content/docs/bifrost/schema.svx`: the managed `run_id`
    on result tables is the application Run (no new column).
- **Cross-change note:** `SPEC-bifrost-variant` (REQ-206) reshapes
  `details` in the same table. This change adds no column and edits no
  table schema, so it does not conflict; its owner is told that the managed
  `run_id` on result rows now names the application Run.
- **Revision history entry:** "Revision 72: one canonical journey"
  (FIND-NO-CANONICAL-JOURNEY, FIND-TABLE-NOT-DECLARABLE,
  FIND-REALTIME-NOT-RECORDED, FIND-NO-REQUEST-JOIN,
  FIND-REALTIME-ONLY-BINDING, FIND-UNBOUND-EVAL-SILENT,
  FIND-AGENT-NOT-INVOKABLE, FIND-GATEWAY-UNCORRELATED,
  FIND-JUDGE-BYPASSES-GATEWAY, FIND-TELEMETRY-SETUP). It records the user's
  2026-10-08 approval of stored realtime verdicts.

## Acceptance criteria

- `examples/support-desk` runs with `mise run examples:support-desk:<lang>`
  against a local server using only the admin key (or a saved login), after
  the README's gateway step.
- Every Scenario 10 test passes in all three SDKs with the same names and
  outcomes. No journey calls `flush_bifrost`.
- Every server scenario test above passes. The existing tests named in
  Scenarios 3, 5, and 7 stay green.
- The SDK surfaces table holds in all three SDKs.
- `codegen:check` is clean after regeneration.
- The error catalog has exactly one new code.

## Write set and consumer closure

Paths are guidance.

- **Spec and contracts:**
  - `crates/wyrd-spec/src/card/{service,verifier}.rs`,
    `graph/composition.rs`, `verification.rs`, `ids.rs` (docs), the error
    catalog (new SDK code);
  - regenerated schemas and card types.
- **Server:**
  - `components/cards/{routes,service}.rs`;
  - `bifrost/service.rs`;
  - `components/verification/service.rs`;
  - `verification/{direct,eval,results,observations,mod}.rs`;
  - `components/gateway/{ingress,invocation,capture,workflow}.rs`;
  - `state.rs` (delete `judge_providers`; hold the principal cache);
  - `mcp/verification.rs` (text);
  - the binding projection migration (edited in place).
- **Auth:** `crates/wyrd/wyrd-auth/src/issuance.rs` (resolve by principal id,
  no activity) and the principal cache module; `crates/wyrd/wyrd-auth/Cargo.toml`
  (`mini-moka = { workspace = true }`).
- **SQL:** `crates/wyrd/wyrd-sql/src/queries/verifier_runs.rs`
  (`ObservationRecord`, enqueue).
- **Bifrost:** `crates/vala/vala-bifrost-redux/src/tables/verification/results.rs`
  and `tables/mod.rs`.
- **Skald:** `crates/skald/skald-workflow/src/{route,lib}.rs` (public adapter
  and `subject`).
- **Client:**
  - `crates/shared/wyrd-client/src/{observe/mod.rs,observe/eval.rs,observe/verify.rs,state.rs,workflow/gateway.rs}`;
  - `otel.rs` (from R3) and its `start_telemetry` and processor.
- **Bindings:**
  - `sdks/wyrd-sdk-python/src/{observe,state}/mod.rs` and
    `python/wyrd/otel.py`, plus stubs through codegen;
  - `sdks/wyrd-sdk-ts/native/src/cards.rs`,
    `wyrd/src/{index,otel}.ts`, `package.json`, plus declarations.
- **Test harness:** `crates/wyrd/wyrd-testing/src/server.rs` (deploy the
  judge model instead of the judge registry, and grant the writer
  `gateway:invoke` on it), and the Python and TypeScript
  harness mirrors.
- **Tests:** every file named in the scenarios; `fixtures/README.md`;
  `fixtures/cards/support_desk/`.
- **Example:** `examples/support-desk/**`; `examples/README.md`;
  `examples/rust/Cargo.toml` (bin); `mise.toml` (runners, `check:examples`,
  the Bifrost journey lists).
- **Docs:**
  - `architecture/wyrd-design.md`, `architecture/bifrost-design.md`;
  - the docs site: Bifrost schema, verification how-to, gateway
    correlation, the support-desk example page;
  - `changes/active/verified-change-contract/{spec.md,architecture/logic/table_schema.md}`.

## Constraints and non-goals

- **Out of scope:**
  - tool-calling or structured output in `run.invoke`;
  - correlation for Workflow steps;
  - an SDK MCP helper or new MCP tools;
  - metrics or log setup in `start_telemetry`;
  - `tracing`-crate span correlation in Rust (only OpenTelemetry API spans);
  - table layout or compaction options in `tables`;
  - dropping or evolving declared tables;
  - a durable outbox for direct results;
  - an `origin` column on results;
  - UI display of `tables`;
  - Operators, Drift, or Triggers in the example.
- **Kept unchanged:** the static `x-wyrd-api-key` OTLP path, and the existing
  `otel_export` story.
- **Not added:**
  - environment-configured judge providers, or any fallback to them;
  - compatibility for old `vala.verification.results` data or for a
    required `runs_on`.
- **Working rules:**
  - Do not weaken, skip, or ignore any test, and do not hand-edit generated
    stubs, declarations, or schemas.
  - Rustdoc every touched Rust item, with `# Errors` (AGENTS.md §16).
  - Python tests are top-level `def test_*`.

## Verification

Run every named test above by exact selector. Then run:

- `mise run codegen:check`, `mise run check:deps`,
  `mise run check:tenant-isolation`, `mise run check:examples`;
- `mise run test:cards:integration`;
- `mise run test:principals:integration` (the OpenAPI document shows
  `ExecuteVerificationRequest.run_id`);
- `mise run test:gateway:journey`;
- `mise run test:bifrost:journey:mcp`;
- `mise run test:sql`;
- `mise run fmt`, `mise run lints`, `mise run py:lints`,
  `mise run py:typecheck`, `mise run docs:check`;
- `mise run examples:support-desk:python`, `:typescript`, and `:rust`
  against a local server (manual proof, recorded as evidence);
- `git grep -n "judge_providers\|flush_bifrost\|flushBifrost" crates sdks
  examples`, which must find only Bifrost publication engine tests;
- `git diff --check`, then the tracked and untracked diff audit.

The three SDK verify lanes and `test:bifrost` run once on the integrated
candidate at change review.

## Material stop conditions

- **Ensure needs a peer RPC.** In a targeted topology, the pod serving
  `/v1/cards` may have no Bifrost catalog. If registration must ensure
  tables in such a pod, stop: that is a cross-service decision.
- **No MCP client fits.** Neither stock client supports `2026-07-28` and a
  minimal JSON-RPC call cannot satisfy the server's required headers.
- **The writer cannot be rebuilt as a caller.** The issuance reads cannot
  resolve a principal by id without a grant, or the stamped `principal_id`
  is not the writing principal.
- **Context propagation fails.** A language runtime cannot carry the Run
  scope through its async context (for example, TypeScript without
  `AsyncLocalStorage`).
- **The Run cannot be stamped.** The observation's `run_id` cannot reach
  `runner::stage` through the existing report, or a reader depends on result
  rows carrying the Verifier run in a way `result_id` cannot replace.

## Authority links

- [spec.md](../../spec.md) (revision 70 approved; 71 and 72 proposed)
- [R3](../TASK-017-r3/TASK-017-R3-principal-role-assignments.md)
- `AGENTS.md`, `architecture/wyrd-design.md`, `architecture/bifrost-design.md`,
  `architecture/agent-rules.md`, `TESTING.md`

## Implementation evidence

| Finding / AC | Implementation | Verification | Result |
|---|---|---|---|
| FIND-TABLE-NOT-DECLARABLE (REQ-217) | | | |
| FIND-REALTIME-ONLY-BINDING (REQ-220) | | | |
| FIND-UNBOUND-EVAL-SILENT (REQ-221) | | | |
| FIND-NO-REQUEST-JOIN (REQ-119, D8) | | | |
| FIND-REALTIME-NOT-RECORDED (REQ-218, AC-039) | | | |
| FIND-GATEWAY-UNCORRELATED (REQ-222) | | | |
| FIND-JUDGE-BYPASSES-GATEWAY (REQ-223) | | | |
| FIND-AGENT-NOT-INVOKABLE (REQ-224) | | | |
| FIND-TELEMETRY-SETUP (REQ-225) | | | |
| FIND-NO-CANONICAL-JOURNEY (AC-071) | | | |
| Server proofs (AC-072) | | | |
