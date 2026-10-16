---
title: Record a run and observations
description: Load your Service's bundle, record rows inside a run, verify or submit them to a Verifier, and read them back by run ID.
pillar: wyrd
section: Build and verify
topic: Observe
order: 1
status: runnable
---

Load the exact Service version you deployed, wrap each request in a run, record what each component saw and did, and hand it to a Verifier with `verify` or `submit`. Verifiers judge these runs, and every row points back to the Card version and run that produced it. This is the path for application code; to load or query tables directly, use the [Bifrost client](/build-and-verify/observe/write-and-query-custom-data-with-bifrost/).

[Runs, observations, and retained evidence](/understand/runs-observations-and-evidence/) explains the model behind these calls.

## Prerequisites

- A registered Service, such as `support-desk` from [Cards, versions, and relationships](/understand/cards-versions-and-relationships/).
- A credential bound to that Service version, so the server can attribute what it records. A tenant administrator issues one:

  ```bash
  wyrd auth issue-key --kind Service --name support-desk --version 1.0.0 --space default
  ```

  The key prints once. Give it to the service as `WYRD_API_KEY`, alongside `WYRD_SERVER_URL`.
- The SDK for your language: `pip install wyrd-ai`, `npm install wyrd-ai`, or the `wyrd-ai` crate.

## 1. Download the bundle

`wyrd get` writes the Service and every Card it references, including model weights and datasets, to a local directory. Do this at build or deploy time, not per request:

```bash
wyrd get --space default --kind Service --name support-desk --version 1.0.0 \
  --output-dir ./support-desk
```

## 2. Load it and start Bifrost

Once, at startup:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
from wyrd import WyrdState

state = WyrdState.from_path("./support-desk")
state.start_bifrost()
```

</LangTab>
<LangTab lang="rust">

```rust
use wyrd_ai::state::WyrdState;

let state = WyrdState::from_path("./support-desk")?;
state.start_bifrost().await?;
```

</LangTab>
<LangTab lang="typescript">

```typescript
import { WyrdState } from "wyrd-ai";

const state = WyrdState.fromPath("./support-desk");
await state.startBifrost();
```

</LangTab>
</LangTabs>

`from_path` reads local files only. `start_bifrost` is the first server call: it connects the writer and fails at startup if the server is unreachable or your credential cannot write observations.

## 3. Record, verify, and submit

Open a run, then call `observe` on it. Everything in one run shares a `run_id`.

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
from wyrd import verify

with state.run() as run:
    # Store a row
    run.observe.record("vala.datasets.answers", {"question": question, "answer": answer})

    # Run a Verifier now and wait for the result
    result = run.observe.verify("my-check", answer=answer)
    print(result.verdict)

    # Queue a Verifier and return immediately
    execution = run.observe.submit("my-review", document=verify.File("report.pdf"))
    print(execution.id)
```

</LangTab>
<LangTab lang="rust">

```rust
use serde_json::json;
use wyrd_ai::verify::{File, Input, Inputs};

let run = state.run();

// Store a row
run.observe()
    .record("vala.datasets.answers", &json!({ "question": question, "answer": answer }))
    .await?;

// Run a Verifier now and wait for the result
let result = run
    .observe()
    .verify("my-check", Inputs::named([("answer", Input::from(json!(answer)))]))
    .await?;
println!("{:?}", result.outcome.verdict);

// Queue a Verifier and return immediately
let submission = run
    .observe()
    .submit("my-review", Inputs::named([("document", Input::from(File::path("report.pdf")))]))
    .await?;
```

</LangTab>
<LangTab lang="typescript">

```typescript
import { verify } from "wyrd-ai";

const run = state.run();

// Store a row
await run.observe.record("vala.datasets.answers", { question, answer });

// Run a Verifier now and wait for the result
const result = await run.observe.verify("my-check", { answer });
console.log(result.outcome.verdict);

// Queue a Verifier and return immediately
const submitted = await run.observe.submit("my-review", { document: verify.File.path("report.pdf") });
```

</LangTab>
</LangTabs>

| Call | Does | Returns |
| --- | --- | --- |
| `record(table, row)` | Queues one row for a `vala.datasets` table | Nothing; the row is stored at the next flush |
| `verify(name, inputs)` | Runs a Verifier and waits for it | The verdict (`passed`, `failed`, or `inconclusive`) and one result per requirement |
| `submit(name, inputs)` | Queues a Verifier on the server | An execution whose `id` reads the result later |

`verify` and `submit` take a Verifier bound to the run's Card, and one input per slot that Verifier declares. `verify.File` fills a file slot. A failed verdict is a normal return, not an error.

To run on one component instead of the Service, use `run.for_card("<alias>")`; it shares the same `run_id`.

Read a submitted result back from any process:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
execution = verify.Execution.connect(execution_id, client)
result = execution.wait(30)
```

</LangTab>
<LangTab lang="rust">

```rust
let execution = Execution::connect_with(id, state.client()?).await?;
let result = execution.wait(Duration::from_secs(30)).await?;
```

</LangTab>
<LangTab lang="typescript">

```typescript
const execution = await verify.Execution.connect(executionId, { client });
const result = await execution.wait({ timeoutMs: 30_000 });
```

</LangTab>
</LangTabs>

`status()` on the execution returns its state without waiting.

## 4. Shut down cleanly

Queued rows are not stored yet. Flush before you depend on them, and shut down once when the process stops:

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

`shutdown` sends everything still queued and returns once the server has it. A process killed without it can lose its last rows. `flush` does the same without closing the writer, for tests and batch jobs. With `start_telemetry`, both first flush the tracer provider it installed, and still drain the writer if that flush fails.

## 5. Read a run back

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
from wyrd.bifrost import Bifrost

rows = Bifrost().sql(
    "SELECT question, answer, card_uid FROM vala.datasets.answers WHERE run_id = $1",
    [run.run_id],
)
print(rows.to_pandas())
```

</LangTab>
<LangTab lang="rust">

```rust
use wyrd_ai::{Bifrost, bifrost::{BifrostOptions, QueryParam}};

let rows = Bifrost::connect(BifrostOptions::default())
    .await?
    .sql(
        "SELECT question, answer, card_uid FROM vala.datasets.answers WHERE run_id = $1",
        &[QueryParam::String(run.run_id().to_string())],
    )
    .await?;
```

</LangTab>
<LangTab lang="typescript">

```typescript
import { Bifrost } from "wyrd-ai";

const bifrost = await Bifrost.connect();
const rows = await bifrost.sql(
  "SELECT question, answer, card_uid FROM vala.datasets.answers WHERE run_id = $1",
  [run.runId],
);
```

</LangTab>
</LangTabs>

The agent's spans read back the same way, by run:

```sql
SELECT name, gen_ai_request_model, gen_ai_usage_input_tokens, gen_ai_usage_output_tokens
FROM vala.traces.spans
WHERE run_id = $1
```

`Bifrost().trace_timeline(trace_id)` (`traceTimeline` in TypeScript) returns one trace's spans with their parent ids, status, and attributes.

The row has `card_uid` set to the Card version that ran. Spans in `vala.traces.spans` and gateway calls in `vala.gateway.calls` carry the same `run_id`, so one join reads the whole request back. Rows become queryable a short time after `flush` returns. An empty result right after a flush is not proof they are missing; retry first.

## What can go wrong

| Error code | When | Cause | Fix |
| --- | --- | --- | --- |
| `WYRD_SDK_400_BIFROST_NOT_STARTED` | At the call | `observe` before `start_bifrost` | Start Bifrost at startup |
| `WYRD_SDK_409_BIFROST_CLOSED` | At the call | `observe` after `shutdown` | Shut down only at process exit |
| `WYRD_SDK_404_UNKNOWN_ALIAS` | At the call | `run` or `for_card` with an alias not in the bundle | Use an alias from the Service's `components` |
| `WYRD_SDK_400_INVALID_OBSERVATION` | At the call | Empty or nested drift features, or `record` to a table outside `vala.datasets` | Send flat values; record only to your own tables |
| `WYRD_CLIENT_429_QUEUE_FULL` | At the call | The server is unreachable or slow and the queue filled | Decide whether to drop or retry. The SDK never blocks your request |
| `WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND` | At `record` | The table was never registered | Declare it in `spec.tables` or register it |
| `WYRD_VALA_403_BIFROST_CARD_SCOPE` | At `flush` or `shutdown` | The credential is bound to a different Card than the one recorded about | Use the key issued for this Service version |
| `WYRD_PERMISSION_403_DENIED_RBAC` | At `flush` or `shutdown` | The credential lacks `bifrost_record:write` | Use a `workload` credential |
| `WYRD_SDK_409_TELEMETRY_PROVIDER_EXISTS` | At `start_telemetry` | The application already installed a tracer provider | Don't call `start_telemetry`; keep your own provider |
| `WYRD_SDK_504_TELEMETRY_FLUSH_TIMEOUT` | At `flush` or `shutdown` | The tracer provider `start_telemetry` installed did not flush in time | Check that the server is reachable. The writer still drained; `details.native_code` names its failure, if any |
| `WYRD_SDK_503_TELEMETRY_FLUSH_FAILED` | At `flush` or `shutdown` | That provider failed to flush. In TypeScript this includes a refused span export | Check that the credential may write telemetry. `details.native_code` names any writer failure |

Card-scope and permission refusals arrive at `flush` or `shutdown`, not at the `observe` call, because the server checks them when the batch lands. The Python OpenTelemetry SDK logs a refused span export rather than failing its flush, so look for it in the log. Call `flush` in a smoke test after deploying so a wrong key fails there instead of silently at exit.
