---
title: Write and query custom data with Bifrost
description: Connect to Bifrost directly to register a table, load rows, and query any table you can read with SQL.
pillar: wyrd
section: Build and verify
topic: Observe
order: 2
draft: true
status: runnable
---

Bifrost is Wyrd's analytical warehouse, and you can use it directly: register a table from a type you already have, load rows into it, and query it, or any table you can read, with SQL. Use this for backfills, batch jobs, notebooks, and analysis. Inside a running service, prefer [`observe.record`](/build-and-verify/observe/record-a-run-and-observations/), which ties each row to the run and Card that produced it.

[Bifrost](/understand/bifrost/) explains the storage model and when written rows become queryable.

## Prerequisites

- A reachable server and a saved credential from [Set up Wyrd](/get-started/setup/). The client reads the same endpoint and credential as every other SDK call.
- Network access to both server ports. Writes go over gRPC and queries over HTTP. The client takes the gRPC address from `[client].grpc_url` in your config, then `WYRD_GRPC_URL`, else the HTTP host on port 50051.
- Permissions: `bifrost_table:write` to register a table, `bifrost_record:write` to insert, `bifrost_query:read` to query. `workload` holds all three; `viewer` can only query.

## 1. Register a table

Tables live in `vala.datasets`. Declare one from a type, and the type's fields become the columns:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
from pydantic import BaseModel
from wyrd.bifrost import Bifrost, TableConfig


class TicketOutcome(BaseModel):
    ticket_id: str
    resolved: bool
    handle_time_s: float


bifrost = Bifrost(TableConfig(TicketOutcome, "vala.datasets.ticket_outcomes"))
print(bifrost.register())  # "created" or "already_exists"
```

</LangTab>
<LangTab lang="rust">

```rust
use wyrd_sdk::{Bifrost, bifrost::TableConfig};

#[derive(serde::Serialize, schemars::JsonSchema)]
struct TicketOutcome {
    ticket_id: String,
    resolved: bool,
    handle_time_s: f64,
}

let table = TableConfig::from_model::<TicketOutcome>("vala.datasets.ticket_outcomes")?;
let bifrost = Bifrost::from_env().await?;
bifrost.use_table(table);
bifrost.register().await?;
```

</LangTab>
<LangTab lang="typescript">

```typescript
import { z } from "zod";
import { Bifrost, TableConfig } from "wyrd-ai";

const TicketOutcome = z.object({
  ticket_id: z.string(),
  resolved: z.boolean(),
  handle_time_s: z.number(),
});

const bifrost = await Bifrost.connect({
  table: TableConfig.fromJsonSchema("vala.datasets.ticket_outcomes", TicketOutcome),
});
console.log(await bifrost.register()); // "created" or "already_exists"
```

</LangTab>
</LangTabs>

Registering the same schema again returns `already_exists` and changes nothing. A different schema for an existing table fails with `WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH`; tables cannot be altered or dropped, so pick a new name. `card_uid`, `run_id`, and names starting with `wyrd_` are reserved.

By default the table is partitioned on `wyrd_ingested_at`, the time Bifrost received each row. If your rows carry their own event time and you will filter on it, name it as the time column. It must be a non-null timestamp:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
TableConfig(TicketOutcome, "vala.datasets.ticket_outcomes", time_column="closed_at")
```

</LangTab>
<LangTab lang="rust">

```rust
// The layout type is not re-exported from wyrd_sdk yet; add wyrd-spec as a dependency.
use wyrd_spec::vala::api::{PhysicalLayoutWire, TimeGranularityWire};

let table = TableConfig::from_model::<TicketOutcome>("vala.datasets.ticket_outcomes")?
    .with_layout(PhysicalLayoutWire {
        partition_granularity: TimeGranularityWire::Day,
        time_column: Some("closed_at".into()),
        sort_keys: vec![],
        bloom_columns: vec![],
    });
```

</LangTab>
<LangTab lang="typescript">

```typescript
TableConfig.fromJsonSchema("vala.datasets.ticket_outcomes", TicketOutcome, {
  layout: { timeColumn: "closed_at" },
});
```

</LangTab>
</LangTabs>

The table's layout is fixed at registration.

If the schema is already JSON Schema or Arrow, use `TableConfig.from_json_schema` or `TableConfig.from_arrow` instead of a type. To write to a table someone else registered, `TableConfig.describe` reads its declaration from the server.

## 2. Insert rows

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
for outcome in outcomes:
    bifrost.insert(TicketOutcome(ticket_id=outcome.id, resolved=outcome.resolved, handle_time_s=outcome.seconds))
bifrost.flush()
```

</LangTab>
<LangTab lang="rust">

```rust
use wyrd_sdk::bifrost::Correlation;

for outcome in &outcomes {
    let row = TicketOutcome { ticket_id: outcome.id.clone(), resolved: outcome.resolved, handle_time_s: outcome.seconds };
    bifrost.insert(serde_json::to_vec(&row)?, Correlation::default())?;
}
bifrost.flush().await?;
```

</LangTab>
<LangTab lang="typescript">

```typescript
for (const outcome of outcomes) {
  bifrost.insert({ ticket_id: outcome.id, resolved: outcome.resolved, handle_time_s: outcome.seconds });
}
await bifrost.flush();
```

</LangTab>
</LangTabs>

`insert` queues the row and returns; `flush` returns once the server has every queued row. Python accepts a model instance, a mapping, or a JSON string. If the queue fills, `insert` fails with `WYRD_CLIENT_429_QUEUE_FULL` rather than dropping the row.

For a large load you already have in Arrow, `write_batch(table, batch)` sends one batch and returns once it is stored, with no `flush` needed. Build the batch against `TableConfig.describe(...).arrow_schema`.

Rows written this way carry no run. To tie a row to a Card, pass a correlation with its `card_uid`; your credential must be allowed to write about that Card.

When you are done writing, call `shutdown()` to drain and stop the writer.

## 3. Query with SQL

`sql` runs one `SELECT` over any table your credential can read, not only your own. Bind values with `$1`, `$2`, and so on; they are sent as typed data and never spliced into the query:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
result = bifrost.sql(
    "SELECT resolved, avg(handle_time_s) AS avg_s, count(*) AS n "
    "FROM vala.datasets.ticket_outcomes WHERE handle_time_s > $1 GROUP BY resolved",
    [10.0],
)
print(result.to_pandas())
```

Pass `model=` to get validated rows back instead: `bifrost.sql(query, params, model=TicketOutcome)` returns a list of `TicketOutcome`. `to_arrow()` and `to_polars()` also work.

</LangTab>
<LangTab lang="rust">

```rust
use wyrd_sdk::bifrost::QueryParam;

#[derive(serde::Deserialize)]
struct Summary {
    resolved: bool,
    avg_s: f64,
    n: i64,
}

let rows: Vec<Summary> = bifrost
    .sql_as(
        "SELECT resolved, avg(handle_time_s) AS avg_s, count(*) AS n \
         FROM vala.datasets.ticket_outcomes WHERE handle_time_s > $1 GROUP BY resolved",
        &[QueryParam::Float(10.0)],
    )
    .await?;
```

`sql` returns Arrow batches instead of typed rows.

</LangTab>
<LangTab lang="typescript">

```typescript
const Summary = z.object({ resolved: z.boolean(), avg_s: z.number(), n: z.bigint() });

const rows = await bifrost.sql(
  `SELECT resolved, avg(handle_time_s) AS avg_s, count(*) AS n
     FROM vala.datasets.ticket_outcomes WHERE handle_time_s > $1 GROUP BY resolved`,
  [10.0],
  Summary,
);
```

Without a schema, `sql` returns a `QueryResult`; call `toArrow()` on it.

</LangTab>
</LangTabs>

For a result too large to hold in memory, `stream` yields Arrow batches as they arrive.

Bifrost is read-only from SQL: `INSERT`, `UPDATE`, `DELETE`, and DDL are refused. Write through `insert` or `write_batch`.

## 4. Find and describe tables

`describe_table(namespace, name)` (`describeTable` in TypeScript) returns a table's columns, including the managed columns Wyrd adds, and its time column:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
print(bifrost.describe_table("vala.datasets", "ticket_outcomes"))
```

</LangTab>
<LangTab lang="rust">

```rust
let description = bifrost.describe_table("vala.datasets", "ticket_outcomes").await?;
```

</LangTab>
<LangTab lang="typescript">

```typescript
console.log(await bifrost.describeTable("vala.datasets", "ticket_outcomes"));
```

</LangTab>
</LangTabs>

Listing every table is not in the SDKs yet; use the `bifrost.list_tables` [MCP tool](/reference/mcp-tools/) or the HTTP API. Both need `bifrost_table:read`.

The tables you can read besides your own:

| Namespace | Holds |
| --- | --- |
| `vala.drift`, `vala.eval` | Observations and per-feature or per-task results |
| `vala.verification` | One row per verdict |
| `vala.traces`, `vala.metrics`, `vala.logs` | OpenTelemetry data |
| `vala.gateway` | AI Gateway calls. Request and response payload columns also need `gateway_payload:read` |
| `vala.system` | The audit log |
| `vala.datasets` | Your own tables |

Only `vala.datasets` accepts writes from you.

## What can go wrong

| Error code | Cause | Fix |
| --- | --- | --- |
| `WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH` | The table exists with a different schema | Use a new table name |
| `WYRD_VALA_400_BIFROST_RESERVED_COLUMN` | A field is named `card_uid`, `run_id`, or `wyrd_*` | Rename the field |
| `WYRD_VALA_400_BIFROST_INVALID_PHYSICAL_LAYOUT` | `time_column` is missing, nullable, or not a timestamp | Point it at a required timestamp field |
| `WYRD_VALA_412_NO_ACTIVE_TABLE` | `insert` before a table was set | Pass a `TableConfig` when connecting, or call `use_table` |
| `WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND` | Writing to a table that was never registered | Call `register` first |
| `WYRD_PERMISSION_403_DENIED_RBAC` | The credential lacks the permission for that step | See the prerequisites |
| An empty result right after `flush` | The rows are stored but not yet queryable | Retry; see [Bifrost](/understand/bifrost/#written-versus-queryable) |
