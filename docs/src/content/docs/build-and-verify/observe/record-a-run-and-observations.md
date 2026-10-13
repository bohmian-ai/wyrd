---
title: Record a run and observations
description: Load your Service's bundle, record eval observations and custom rows inside a run, and read them back by run ID.
pillar: wyrd
section: Build and verify
topic: Observe
order: 1
draft: true
status: runnable
---

Load the exact Service version you deployed, wrap each request in a run, and record what each component saw and did. Verifiers judge these rows, and every row points back to the Card version and run that produced it. This is the path for application code; to load or query tables directly, use the [Bifrost client](/build-and-verify/observe/write-and-query-custom-data-with-bifrost/).

[Runs, observations, and retained evidence](/understand/runs-observations-and-evidence/) explains the model behind these calls.

## Prerequisites

- A registered Service, such as `support-desk` from [Cards, versions, and relationships](/understand/cards-versions-and-relationships/).
- A credential bound to that Service version, so the server can attribute what it records. A tenant administrator issues one:

  ```bash
  wyrd auth issue-key --kind Service --name support-desk --version 1.0.0 --space default
  ```

  The key prints once. Give it to the service as `WYRD_API_KEY`, alongside `WYRD_SERVER_URL`.
- The SDK for your language: `pip install wyrd`, `npm install @wyrd/sdk`, or the `wyrd-sdk` crate.

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
use wyrd_sdk::state::WyrdState;

let state = WyrdState::from_path("./support-desk")?;
state.start_bifrost().await?;
```

</LangTab>
<LangTab lang="typescript">

```typescript
import { WyrdState } from "@wyrd/sdk";

const state = WyrdState.fromPath("./support-desk");
await state.startBifrost();
```

</LangTab>
</LangTabs>

`from_path` reads local files only. `start_bifrost` is the first server call: it connects the writer and fails at startup if the server is unreachable or your credential cannot write observations.

## 3. Record inside a run

Open one run per request with `run(alias)`, which scopes it to one component. Every row records which Card it is about, and all of them share one `run_id`. This is the support desk's handler:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
def answer_question(ticket_id: str, question: str) -> str:
    with state.run("agent") as run:
        answer = run.invoke({"question": question})
        run.observe.record(
            "vala.datasets.tickets",
            {"ticket_id": ticket_id, "question": question, "answer": answer, "refund": "refund" in question},
        )
        run.observe.eval({"answer": answer})
    return answer
```

</LangTab>
<LangTab lang="rust">

```rust
use serde_json::json;
use wyrd_sdk::observe::EvalObservationOptions;

async fn answer_question(state: &WyrdState, ticket_id: &str, question: &str) -> Result<String, WyrdError> {
    let run = state.run_for_card("agent")?;
    let answer = run.invoke(&[("question", question)]).await?;
    let observe = run.observe();
    observe
        .record(
            "vala.datasets.tickets",
            &json!({ "ticket_id": ticket_id, "question": question, "answer": answer, "refund": question.contains("refund") }),
        )
        .await?;
    observe.eval(&json!({ "answer": answer }), EvalObservationOptions::default())?;
    Ok(answer)
}
```

</LangTab>
<LangTab lang="typescript">

```typescript
async function answerQuestion(ticketId: string, question: string): Promise<string> {
  const run = state.run("agent");
  const answer = await run.invoke({ question });
  await run.observe.record("vala.datasets.tickets", {
    ticket_id: ticketId,
    question,
    answer,
    refund: question.includes("refund"),
  });
  run.observe.eval({ answer });
  return answer;
}
```

</LangTab>
</LangTabs>

`invoke` runs the Agent's Prompt through the AI Gateway, so the model call lands in `vala.gateway.calls` under the same run. It needs a gateway deployment for the Prompt's model. If your service calls its model itself, skip `invoke` and record what it did; the observations work the same.

When one request touches several components, such as an agent and a model that scores the customer, `run.for_card(alias)` rescopes the same run to each one, so the rows still share one `run_id`. `eval` also takes a `session_id`, a UUID that groups the runs of one conversation.

What each call is for:

| Call | Records | Use it when |
| --- | --- | --- |
| `observe.drift(features)` | One flat row of feature values, stored one row per feature | A [drift Verifier](/build-and-verify/verify/monitor-drift/) should compare these values with a baseline |
| `observe.eval(context)` | One JSON record of what the agent was given and did | An [eval Verifier](/build-and-verify/verify/evaluate-agent-behavior/) should judge it |
| `observe.record(table, row)` | One row in your own `vala.datasets` table | You want the data for your own queries |
| `observe.verify(name, input)` | Nothing to observe; returns a verdict now | Your code needs the answer before it responds. See [Verify a request in real time](/build-and-verify/verify/verify-a-request-in-real-time/) |

`record` needs the table to exist first. Declare it under the Service's `spec.tables` so it is created on registration, or register it with the [Bifrost client](/build-and-verify/observe/write-and-query-custom-data-with-bifrost/#1-register-a-table).

The Python `with` block tags every OpenTelemetry span started inside it with the run, so traces line up with observations. It does not flush. In TypeScript, `run.scope(fn)` does the same for spans started in `fn`; in Rust, `run.scope(future)`.

`drift` and `eval` return once the row is queued, and add almost no time to a request. `record` is async in Rust and TypeScript because the first call for a table looks it up on the server.

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

`shutdown` sends everything still queued and returns once the server has it. A process killed without it can lose its last rows. `flush` does the same without closing the writer, for tests and batch jobs.

## 5. Read a run back

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
from wyrd.bifrost import Bifrost

rows = Bifrost().sql(
    "SELECT ticket_id, answer, card_uid FROM vala.datasets.tickets WHERE run_id = $1",
    [run.run_id],
)
print(rows.to_pandas())
```

</LangTab>
<LangTab lang="rust">

```rust
use wyrd_sdk::{Bifrost, bifrost::QueryParam};

let rows = Bifrost::from_env()
    .await?
    .sql(
        "SELECT ticket_id, answer, card_uid FROM vala.datasets.tickets WHERE run_id = $1",
        &[QueryParam::String(run.run_id().to_string())],
    )
    .await?;
```

</LangTab>
<LangTab lang="typescript">

```typescript
import { Bifrost } from "@wyrd/sdk";

const bifrost = await Bifrost.connect();
const rows = await bifrost.sql(
  "SELECT ticket_id, answer, card_uid FROM vala.datasets.tickets WHERE run_id = $1",
  [run.runId],
);
```

</LangTab>
</LangTabs>

The ticket row has `card_uid` set to the `support-agent` version that ran. The eval row in `vala.eval.observations` and the gateway call in `vala.gateway.calls` carry the same `run_id`, so one join reads the whole request back. Rows become queryable a short time after `flush` returns. An empty result right after a flush is not proof they are missing; retry first.

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

Card-scope and permission refusals arrive at `flush` or `shutdown`, not at the `observe` call, because the server checks them when the batch lands. Call `flush` in a smoke test after deploying so a wrong key fails there instead of silently at exit.
