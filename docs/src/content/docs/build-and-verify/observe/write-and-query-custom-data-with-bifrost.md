---
title: Write and query custom data with Bifrost
description: Declare a Service dataset, flush rows through the Rust SDK, and read bounded evidence through MCP.
pillar: wyrd
section: Build and verify
topic: Observe
order: 3
draft: true
status: runnable
---

Declare custom data on a Service Card, write rows through Bifrost, then discover the table and query it through MCP. The server owns the table schema and retained evidence.

## Prerequisites

- A reachable server and a saved credential from [Set up Wyrd](/get-started/setup/).
- The Rust SDK (`wyrd-sdk-rust`, imported as `wyrd_sdk`), Tokio, and `serde_json` in your application.
- Card registration permission, Bifrost ingest permission for the dataset, `bifrost_table:read` for discovery/description, and `bifrost_query:read` covering its table.
- An agent connected through [MCP](/connect/connect-an-agent-through-mcp/) for the read steps.

## 1. Declare the dataset

Save this as `investigation-service.yaml`. It is the schema used by the MCP evidence journey:

```yaml
apiVersion: wyrd/v1
kind: Service
metadata:
  name: investigation-service
  version: 1.0.0
  space: default
spec:
  tables:
    - name: investigation_events
      schema:
        type: object
        properties:
          id:
            type: integer
          value:
            type: string
        required: [id, value]
        additionalProperties: false
```

## 2. Register, write, and flush

Run this in your Rust application's Tokio runtime. `WyrdClient::from_global` reads your ordinary saved configuration and credential; configure both HTTP and gRPC endpoints for your server.

```rust
use std::path::Path;
use wyrd_sdk::{Bifrost, WyrdClient};
use wyrd_sdk::bifrost::{Correlation, TableConfig};
use wyrd_sdk::cards::Cards;

async fn write_rows() -> Result<(), Box<dyn std::error::Error>> {
    let client = WyrdClient::from_global()?;
    let cards = Cards::with_client(client.clone());
    Box::pin(cards.register_from_path(Path::new("investigation-service.yaml"))).await?;
    let bifrost = Bifrost::connect(&client).await?;
    bifrost.use_table(
        TableConfig::describe(&client, "vala.datasets.investigation_events").await?
    );
    for id in [1, 2] {
        bifrost.insert(
            serde_json::to_vec(&serde_json::json!({"id": id, "value": "observed"}))?,
            Correlation::default(),
        )?;
    }
    bifrost.flush().await?;
    bifrost.shutdown().await?;
    Ok(())
}
```

The write uses no Card correlation. Attributing rows to a Card requires a credential authorized for that Card; an unbound key cannot supply an arbitrary `card_uid`. Registration declares the table before the writer describes it. Flushing and shutting down complete the writer lifecycle before reading.

## 3. Discover and describe

Ask your agent to call these tools, in order. JSON blocks here are MCP argument objects, not shell commands.

`bifrost.list_tables`:

```json
{}
```

Find `namespace: "vala.datasets"`, `name: "investigation_events"`. Then call `bifrost.describe_table`:

```json
{"namespace":"vala.datasets","name":"investigation_events"}
```

Check `user_fields` contains `id` and `value`; use the described names and types when querying. Discover and describe every table used in a join.

## 4. Query and interpret

Call `bifrost.query`:

```json
{"sql":"SELECT id, value FROM vala.datasets.investigation_events WHERE id >= 1 ORDER BY id LIMIT 20","deadline_ms":10000,"max_rows":100,"max_bytes":65536}
```

The result has `columns`, positional `rows`, and `terminal`. In a fresh table containing only these inserts, rows are `[[1,"observed"],[2,"observed"]]`: column zero is `id`, column one is `value`. Require `terminal.outcome: "success"` before using the result as complete evidence.

For telemetry, first describe `vala.traces.spans` and `vala.logs.records`, then correlate their binary `trace_id` and `span_id` fields. This template requires an actual hex trace ID and a time window in Unix nanoseconds:

```sql
SELECT l.severity_text, l.event_name, s.name, s.status_code
FROM vala.logs.records l
JOIN vala.traces.spans s
  ON l.trace_id = s.trace_id AND l.span_id = s.span_id
WHERE s.trace_id = X'<trace_id>'
  AND s.start_time_unix_nano >= 1767225600000000000
  AND s.start_time_unix_nano < 1798761600000000000
LIMIT 20
```

That window is calendar year 2026; replace it for your incident. See [verification evidence](/build-and-verify/verify/run-a-verification-and-inspect-its-evidence/) for subject Card and result-ID pivots.

## Refusals and empty evidence

An empty `rows` array means the query found no matching retained rows. It is not a passing verdict. Non-SELECT SQL, unknown fields, unauthorized tables, and caller-supplied identity fields are refused. Gateway request/response payload columns also require `gateway_payload:read`.

Exceeding `max_rows` or `max_bytes` returns `WYRD_VALA_413_QUERY_RESULT_TOO_LARGE` with no partial successful rows. Narrow the projection and time range; do not describe the refusal as a truncated success. A failed terminal or `isError` also means the query did not supply complete evidence. [MCP limits](/reference/mcp-tools/#query-limits) lists the defaults and ceilings.
