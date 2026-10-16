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
- drift samples;
- the inputs and results of every verification;
- custom data in your own tables.

You write observations through the Rust, Python, or TypeScript SDK, through OpenTelemetry instrumentation such as an agent framework's spans, or through the HTTP API. Each observation is tied to the Card and run that produced it, so a verdict can always point back to exactly what ran.

If the data already lives elsewhere, such as Prometheus or a SQL warehouse, a Source Card describes where it is. The server does not read Source Cards yet; see [Read data where it already lives](#read-data-where-it-already-lives).

The examples continue the `support-desk` service from [How verification works](/understand/how-verification-works/).

## Your code runs an exact version

Your application starts from a bundle of the exact Service version it was deployed with: `wyrd get` downloads the Service and every Card it references, and the SDK loads it from disk with no network call. Because the bundle is pinned, every observation can name the exact Card versions that produced it. [Record a run and observations](/build-and-verify/observe/record-a-run-and-observations/) walks through it in all three SDKs.

## A run is one invocation

A run is one execution of your service, such as answering one customer question. Every observation recorded inside it shares the same `run_id`. This is the support desk's request loop:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
with state.run("agent") as run:
    answer = run.invoke({"question": question})
    run.observe.record(
        "vala.datasets.tickets",
        {"ticket_id": ticket_id, "question": question, "answer": answer, "refund": "refund" in question},
    )
```

</LangTab>
<LangTab lang="rust">

```rust
let run = state.run_for_card("agent")?;
let answer = run.invoke(&[("question", question)]).await?;
run.observe()
    .record(
        "vala.datasets.tickets",
        &json!({ "ticket_id": ticket_id, "question": question, "answer": answer, "refund": question.contains("refund") }),
    )
    .await?;
```

</LangTab>
<LangTab lang="typescript">

```typescript
const run = state.run("agent");
const answer = await run.invoke({ question });
await run.observe.record("vala.datasets.tickets", {
  ticket_id: ticketId,
  question,
  answer,
  refund: question.includes("refund"),
});
```

</LangTab>
</LangTabs>

`state.run("agent")` scopes the run to the `agent` component, so the ticket row is about `support-agent@1.0.0`. The ticket row, the Agent's trace spans, and its gateway call all share one `run_id`, so one question reads back as one invocation. When one request touches several components, `run.for_card(alias)` scopes the same run to each in turn. A `session_id` groups the runs of one conversation and must be a UUID.

Opening a run makes no network call and creates nothing on the server. A run exists only as the `run_id` on the rows it produced.

## What each observation lands in

Each kind of observation has its own table in Bifrost:

| You call | Lands in | Read by |
| --- | --- | --- |
| `observe.submit(verifier, samples)` on a drift Verifier | `vala.drift.observations`, one row per feature | That Verifier, on its schedule |
| `observe.verify` and `observe.submit` on other Verifiers | `vala.verification.execution_inputs` and `vala.verification.results` | Your queries; see [How verification works](/understand/how-verification-works/) |
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
| `wyrd_ingested_at` | The server | When did Bifrost receive it? |

Event time lives in each table's own time column: `created_at` for drift samples, `start_time_unix_nano` for spans, and `time_unix_nano` for logs and metrics. A table registered through the Bifrost client can name a `time_column`; a table declared on a Service cannot, and uses `wyrd_ingested_at`. There is no shared event-time column across tables.

`card_uid` is checked, not trusted. A service's credential is bound to the Cards in its own graph, and the server rejects an observation about any other Card with `WYRD_VALA_403_BIFROST_CARD_SCOPE`, so one service cannot write evidence about another.

## Recording is buffered

`observe.*` calls return as soon as the row is queued, so they add almost no time to a request. The trade-off is that a queued row is not yet stored. `shutdown` sends everything still queued and returns when the server has it; a process that exits without it can lose its last rows. If the queue fills because the server is slow or unreachable, the call fails with `WYRD_CLIENT_429_QUEUE_FULL` instead of blocking your request.

Stored is not the same as queryable. Bifrost acknowledges a write before the rows are published for queries, and a query reads the published data plus recent rows on a best-effort basis. A row that is missing seconds after `flush` returned is usually not lost. [Bifrost](/understand/bifrost/) explains the gap.

## Two ways to write your own data

Inside a running service, `observe.record` writes a row tied to the current run and Card, which is what you want for anything you will join to observations or verdicts. Declare the table under the Service's `spec.tables` so it is versioned with the code that writes it.

Outside a service, such as a backfill, a batch job, or a notebook, use the Bifrost client directly: register a table from a type you already have, insert rows, and query them with SQL. Those rows carry no run. [Write and query custom data with Bifrost](/build-and-verify/observe/write-and-query-custom-data-with-bifrost/) covers it.

Both paths write only to `vala.datasets`. The drift, telemetry, and verification tables are written by Wyrd.

## Query the evidence

Everything above is queryable with SQL from any SDK or the `bifrost.query` MCP tool. This joins each verdict back to the ticket the same run recorded:

```sql
SELECT t.run_id, t.question, t.answer, r.verdict, r.verifier_version
FROM vala.datasets.tickets AS t
JOIN vala.verification.results AS r ON r.run_id = t.run_id
WHERE t.ticket_id = $1
```

Bifrost keeps every observation and verdict, so a question like "which model version was running when this verdict failed, and what did it see?" is one query, months later.

## Read data where it already lives

<Aside type="caution" title="Not read yet">You can register Source Cards today, but the server does not read from them. The shape below is the registered contract, not a working ingest path.</Aside>

If the data is already in another system, describe it with a Source Card instead of copying it into Bifrost. A Source names the kind of data and how to reach it; it never holds a secret, only the name of the server environment variable that does.

```yaml title="sources/latency-metrics.yaml"
apiVersion: wyrd/v1
kind: Source
metadata:
  space: default
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
