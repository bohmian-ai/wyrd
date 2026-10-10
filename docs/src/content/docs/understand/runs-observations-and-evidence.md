---
title: Runs, observations, and retained evidence
description: How Wyrd records what a declared service actually does, and ties each observation to the version that ran.
pillar: wyrd
section: Understand
order: 3
draft: true
---

Observe is how you record what the declared system actually does. Observations go straight into Bifrost, the Wyrd data plane. You can record:

- telemetry: traces, metrics, and logs;
- drift measurements;
- agent and model eval results;
- custom data in your own tables.

You write observations through the Rust, Python, or TypeScript SDK, through OpenTelemetry instrumentation, or through the HTTP API. Each observation is tied to the Card and run that produced it, so a verdict can always point back to exactly what ran.

If the data already lives elsewhere, such as Prometheus or a SQL warehouse, a Source Card will let Wyrd read it there. Wyrd never writes to an external system.

<Aside type="note" title="Coming soon">Source Cards can be registered today, but the server does not read from them yet.</Aside>

The examples continue the `support-copilot` service from [How verification works](/understand/how-verification-works/).

## Load the service your code is running

Your application starts from the exact Service version it was deployed with. `wyrd get` downloads that Service and every Card it references, including model weights and datasets, into a local bundle:

```bash
wyrd get --space retention --kind Service --name support-copilot --version 1.2.0 \
  --output-dir ./support-copilot
```

Your application loads the bundle once at startup and connects to Bifrost:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python title="app.py"
from wyrd import WyrdState

state = WyrdState.from_path("./support-copilot")
state.start_bifrost()
```

</LangTab>
<LangTab lang="rust">

```rust title="main.rs"
use wyrd_sdk::state::WyrdState;

let state = WyrdState::from_path("./support-copilot")?;
state.start_bifrost().await?;
```

</LangTab>
<LangTab lang="typescript">

```typescript title="app.ts"
import { WyrdState } from "@wyrd/sdk";

const state = WyrdState.fromPath("./support-copilot");
await state.startBifrost();
```

</LangTab>
</LangTabs>

Loading the bundle reads local files only; it makes no network call. Starting Bifrost is the first call to the server: it connects the writer and checks that the drift and eval observation tables exist and that your credential can reach them, so a misconfigured service fails at startup instead of on its first request. It does not check which Cards your credential may write about. An observation outside that scope is refused when it is flushed (see [What each observation lands in](#what-each-observation-lands-in)).

In Python, loading a built-in scikit-learn, XGBoost, LightGBM, or CatBoost model also needs `trusted_artifact_hashes`, the artifact hash you verified for that model, before the SDK will deserialize it.

## A run is one invocation

A run is one execution of your service, such as handling one support ticket. Every observation recorded inside it shares the same `run_id`. The highlighted lines are the Wyrd calls; everything else is your own code.

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python {2,5,8-11}
def handle_ticket(ticket):
    with state.run() as run:
        features = {"tenure_months": ticket.tenure_months, "monthly_spend": ticket.monthly_spend}
        risk = churn_score(features)  # your model code
        run.for_card("churn_model").observe.drift(features)

        answer, tool_calls = triage_answer(ticket, risk)  # your agent code
        run.for_card("triage").observe.eval(
            {"response": answer, "tool_call_names": [call.name for call in tool_calls]},
            session_id=ticket.session_id,
        )
    return answer
```

</LangTab>
<LangTab lang="rust">

```rust {2,5,9-12}
fn handle_ticket(state: &WyrdState, ticket: &Ticket) -> Result<String, WyrdError> {
    let run = state.run();
    let features = Features { tenure_months: ticket.tenure_months, monthly_spend: ticket.monthly_spend };
    let risk = churn_score(&features); // your model code
    run.for_card("churn_model")?.observe().drift(&features, None)?;

    let (answer, tool_calls) = triage_answer(ticket, risk); // your agent code
    let tool_call_names: Vec<&str> = tool_calls.iter().map(|call| call.name.as_str()).collect();
    run.for_card("triage")?.observe().eval(
        &json!({ "response": answer, "tool_call_names": tool_call_names }),
        EvalObservationOptions { session_id: Some(ticket.session_id), ..Default::default() },
    )?;
    Ok(answer)
}
```

</LangTab>
<LangTab lang="typescript">

```typescript {2,5,8-11}
function handleTicket(ticket: Ticket): string {
  const run = state.run();
  const features = { tenure_months: ticket.tenureMonths, monthly_spend: ticket.monthlySpend };
  const risk = churnScore(features); // your model code
  run.forCard("churn_model").observe.drift(features);

  const { answer, toolCalls } = triageAnswer(ticket, risk); // your agent code
  run.forCard("triage").observe.eval(
    { response: answer, tool_call_names: toolCalls.map((call) => call.name) },
    { sessionId: ticket.sessionId },
  );
  return answer;
}
```

</LangTab>
</LangTabs>

`for_card(alias)` gives you a view of the same run scoped to one component. Each observation records which Card it is about, so the drift row above is about `churn-classifier` and the eval row is about `triage`, while both share one `run_id`. That is how one ticket can touch several components and still be read back as one invocation. A `session_id` groups the runs of one conversation and must be a UUID.

In Python, the `with` block also tags every OpenTelemetry span started inside it with the run, so your traces line up with your observations. `observe` calls work with or without it, and leaving the block does not flush anything. Rust and TypeScript have no block form; open the run and use it.

Opening a run makes no network call and creates nothing on the server. A run exists only as the `run_id` on the rows it produced.

## What each observation lands in

Each kind of observation has its own table in Bifrost:

| You call | Lands in | Read by |
| --- | --- | --- |
| `observe.drift(features)` | `vala.drift.observations`, one row per feature | `drift` Verifiers on their schedule |
| `observe.eval(context)` | `vala.eval.observations` | `eval` Verifiers, as each row is committed |
| `observe.record(table, row)` | Your own `vala.datasets.<name>` table | Your queries |
| OpenTelemetry spans, metrics, and logs | `vala.traces.spans`, `vala.metrics.points`, and `vala.logs.records` | Your queries |
| Calls through the AI Gateway | `vala.gateway.calls` and `vala.traces.spans` | Your queries |

Every row also carries columns you do not write:

| Column | Set by | Answers |
| --- | --- | --- |
| `card_uid` | The server, from the Card the view is scoped to | Which exact Card version was this about? |
| `run_id` | The SDK, per run | Which invocation produced it? |
| `principal_id` | The server, from your credential | Which service or person wrote it? |
| `wyrd_request_id` | The server, per write request | Which write delivered it, and which audit record goes with it? |
| `wyrd_event_time` | The SDK, when the row is recorded | When did it happen? |
| `wyrd_ingested_at` | The server | When did Bifrost receive it? |

`card_uid` is checked, not trusted. A service's credential is scoped to the Cards in its own graph, and the server rejects an observation about any other Card with `WYRD_VALA_403_BIFROST_CARD_SCOPE`, so one service cannot write evidence about another.

## Custom data

Anything that does not fit drift or eval goes in a table you define. The simplest place to declare it is the Service itself, so the table is versioned with the code that writes to it:

```yaml title="service.yaml" {7-15}
apiVersion: wyrd/v1
kind: Service
metadata:
  space: retention
  name: support-copilot
spec:
  tables:
    - name: ticket_outcomes
      schema:
        type: object
        properties:
          ticket_id: { type: string }
          resolved: { type: boolean }
          handle_time_s: { type: number }
        required: [ticket_id, resolved, handle_time_s]
  components:
    # ...
```

`name` becomes `vala.datasets.ticket_outcomes`, and `schema` is a JSON Schema object. Wyrd creates the table when the Service is registered, before the Service becomes active. Re-applying an identical schema changes nothing; a different schema for an existing table fails with `WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH`. Tables are never dropped.

You can also register a table from code, with the schema taken from a type you already have:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
from pydantic import BaseModel
from wyrd.bifrost import Bifrost, TableConfig


class TicketOutcome(BaseModel):
    ticket_id: str
    resolved: bool
    handle_time_s: float


Bifrost(TableConfig(TicketOutcome, table="vala.datasets.ticket_outcomes")).register()
```

</LangTab>
<LangTab lang="rust">

```rust
use wyrd_sdk::{Bifrost, WyrdClient, bifrost::TableConfig};

#[derive(serde::Serialize, schemars::JsonSchema)]
struct TicketOutcome {
    ticket_id: String,
    resolved: bool,
    handle_time_s: f64,
}

let table = TableConfig::from_model::<TicketOutcome>("vala.datasets.ticket_outcomes")?;
Bifrost::connect_with_table(&WyrdClient::from_global()?, table).await?.register().await?;
```

</LangTab>
<LangTab lang="typescript">

```typescript
import { z } from "zod";
import { Bifrost, TableConfig } from "@wyrd/sdk";

const TicketOutcome = z.object({
  ticket_id: z.string(),
  resolved: z.boolean(),
  handle_time_s: z.number(),
});

const table = TableConfig.fromJsonSchema("vala.datasets.ticket_outcomes", TicketOutcome);
await (await Bifrost.connect({ table })).register();
```

</LangTab>
</LangTabs>

Once the table exists, record rows from any run:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
run.observe.record(
    "vala.datasets.ticket_outcomes",
    {"ticket_id": ticket.id, "resolved": True, "handle_time_s": 41.7},
)
```

</LangTab>
<LangTab lang="rust">

```rust
run.observe()
    .record(
        "vala.datasets.ticket_outcomes",
        &TicketOutcome { ticket_id: ticket.id.clone(), resolved: true, handle_time_s: 41.7 },
    )
    .await?;
```

</LangTab>
<LangTab lang="typescript">

```typescript
await run.observe.record("vala.datasets.ticket_outcomes", {
  ticket_id: ticket.id,
  resolved: true,
  handle_time_s: 41.7,
});
```

</LangTab>
</LangTabs>

The first `record` for a table looks the table up on the server; later calls reuse it. `record` writes only to `vala.datasets` tables, and a table that was never registered fails with `WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND`.

## Recording is buffered

`observe.*` calls return as soon as the row is queued, so they add almost no time to a request. The trade-off is that a queued row is not yet stored. Shut the state down once when the application stops; shutdown sends everything still queued and returns when the server has it. A process that exits without it can lose its last rows.

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
import atexit

atexit.register(state.shutdown)
```

</LangTab>
<LangTab lang="rust">

```rust
state.shutdown().await?;
```

</LangTab>
<LangTab lang="typescript">

```typescript
await state.shutdown();
```

</LangTab>
</LangTabs>

For a test or a batch job, `flush()` sends everything queued without closing the writer.

If the queue is full because the server is unreachable or slow, `observe.*` raises `WYRD_CLIENT_429_QUEUE_FULL` instead of blocking your request.

## Query the evidence

Everything above is queryable with SQL, from any SDK. This joins one conversation's verdicts back to what the agent was given:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
from wyrd.bifrost import Bifrost

bifrost = Bifrost()
evidence = bifrost.sql(
    """
    SELECT o.run_id, o.context, r.verdict, r.verifier_version
    FROM vala.eval.observations AS o
    JOIN vala.verification.results AS r ON r.source_record_id = o.record_id
    WHERE o.session_id = $1
    """,
    [session_id],
)
print(evidence.to_pandas())
```

</LangTab>
<LangTab lang="rust">

```rust
use wyrd_sdk::{Bifrost, WyrdClient, bifrost::QueryParam};

let bifrost = Bifrost::connect(&WyrdClient::from_global()?).await?;
let evidence = bifrost
    .sql(
        "SELECT o.run_id, o.context, r.verdict, r.verifier_version \
         FROM vala.eval.observations AS o \
         JOIN vala.verification.results AS r ON r.source_record_id = o.record_id \
         WHERE o.session_id = $1",
        &[QueryParam::String(session_id.to_string())],
    )
    .await?;
```

</LangTab>
<LangTab lang="typescript">

```typescript
import { Bifrost } from "@wyrd/sdk";

const bifrost = await Bifrost.connect();
const evidence = await bifrost.sql(
  `SELECT o.run_id, o.context, r.verdict, r.verifier_version
     FROM vala.eval.observations AS o
     JOIN vala.verification.results AS r ON r.source_record_id = o.record_id
    WHERE o.session_id = $1`,
  [sessionId],
);
```

</LangTab>
</LangTabs>

Bifrost keeps every observation and verdict, so a question like "which model version was running when this verdict failed, and what did it see?" is one query, months later.

## Read data where it already lives

<Aside type="note" title="Coming soon">The server does not read from Source Cards yet. The shape below is the registered contract.</Aside>

If the data is already in another system, describe it with a Source Card instead of copying it into Bifrost. A Source names the kind of data and how to reach it; it never holds a secret, only the name of the server environment variable that does.

```yaml title="sources/latency-metrics.yaml"
apiVersion: wyrd/v1
kind: Source
metadata:
  space: retention
  name: latency-metrics
spec:
  source:
    kind: metrics
    connection:
      vendor: prometheus
      endpoint: https://prometheus.internal.example.com
      auth: { scheme: env, env: PROMETHEUS_TOKEN }
```

| Source `kind` | Reads | Vendors |
| --- | --- | --- |
| `object_store` | Files, such as Parquet | `gs://`, `s3://`, `az://`, `file://` |
| `sql_warehouse` | Rows | BigQuery, Snowflake, Postgres |
| `metrics` | Time series | Prometheus, Datadog, CloudWatch |
| `logs` | Log records | Loki, Elasticsearch, Splunk |
| `traces` | Spans | Tempo, Datadog APM, Jaeger |
