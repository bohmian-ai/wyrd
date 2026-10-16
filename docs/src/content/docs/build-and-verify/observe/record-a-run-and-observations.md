---
title: Record a run and observations
description: Load your Service's bundle, record rows inside a run, verify or submit them to a Verifier, and read them back by run ID.
pillar: wyrd
section: Build and verify
topic: Observe
order: 1
draft: true
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

## 3. Record inside a run

Open one run per request. `state.run()` scopes it to the Service; `state.run("agent")` or `run.for_card("agent")` scopes it to one component. Every view of a run shares one `run_id`, and every row records which Card it is about. This is the support desk's handler from the [quickstart](/get-started/quickstart/):

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

<CodeFromFile file="examples/support-desk-app/python/desk.py" lang="python" region="answer" title="desk.py" />

</LangTab>
<LangTab lang="rust">

<CodeFromFile file="examples/support-desk-app/rust/lib.rs" lang="rust" region="answer" title="lib.rs" />

</LangTab>
<LangTab lang="typescript">

<CodeFromFile file="examples/support-desk-app/typescript/server.ts" lang="typescript" region="answer" title="server.ts" />

</LangTab>
</LangTabs>

The Python and TypeScript handlers call the model through the gateway with the OpenAI Agents SDK, and pass `wyrd-run-id` so the gateway files the call under the run. The Rust handler uses `run.invoke`, which runs the Agent's Prompt through the gateway for you. Either way the model call lands in `vala.gateway.calls` with the run's `run_id`. If your service calls its model directly, record what it did; observations work the same.

`run` gives you three calls:

| Call | Does | Returns |
| --- | --- | --- |
| `observe.record(table, row)` | Queues one row for your own `vala.datasets` table | Nothing; the row is stored at the next flush |
| `observe.verify(name, **inputs)` | Runs a bound Verifier on the inputs and waits for it | The verdict and requirement breakdown |
| `observe.submit(name, **inputs)` | Queues a bound Verifier on the server | An execution handle you can read later, from any process |

`record` needs the table to exist first. Declare it under the Service's `spec.tables` so it is created on registration, or register it with the [Bifrost client](/build-and-verify/observe/write-and-query-custom-data-with-bifrost/#1-register-a-table).

The Python `with` block tags every OpenTelemetry span started inside it with the run, so traces line up with observations. It does not flush. In TypeScript, `run.scope(fn)` does the same for spans started in `fn`; in Rust, `run.scope(future)`.

`record` is async in Rust and TypeScript because the first call for a table looks it up on the server.

### Trace an agent framework

Agent frameworks describe each run as OpenTelemetry spans: the agent, every model call with its model and token counts, and every tool call. Send them to Wyrd and run the agent inside a run, and its spans land in `vala.traces.spans` with the run's `run_id` and the Card's `card_uid`, next to the run's observations.

Install the SDK's OpenTelemetry support with your framework:

| Framework | Install |
| --- | --- |
| Pydantic AI | `pip install 'wyrd-ai[otel]' 'pydantic-ai-slim[openai]'` |
| Google ADK | `pip install 'wyrd-ai[otel]' google-adk` |
| OpenAI Agents SDK, Python | `pip install 'wyrd-ai[otel,agents]'` |
| OpenAI Agents SDK, TypeScript | `npm install wyrd-ai @openai/agents zod @opentelemetry/api @opentelemetry/sdk-trace-base @opentelemetry/exporter-trace-otlp-proto @opentelemetry/exporter-logs-otlp-proto @opentelemetry/exporter-metrics-otlp-proto @opentelemetry/context-async-hooks @opentelemetry/core` |

The framework calls its model provider with the provider's own key, such as `OPENAI_API_KEY` for OpenAI models or `GOOGLE_API_KEY` for Gemini, read from the environment. Never give it `WYRD_API_KEY`: the Wyrd credential only sends spans and observations to Wyrd.

Each recipe below starts from the `state` loaded in [step 2](#2-load-it-and-start-bifrost). Call `start_telemetry()` (`startTelemetry()`) once at startup, before you build any agent. It installs the global tracer provider, exporting as the state's client: the same `WYRD_SERVER_URL` and credential as everything else, with a fresh access token on every export. Then build the agent. Each `answer` returns the reply and the run's `run_id`, which [reads the spans back](#read-the-agents-spans-back).

The Python recipes share one tool:

```python
def lookup_order(order_id: str) -> str:
    """Return an order's shipping status."""
    return f"Order {order_id} shipped yesterday."
```

**Pydantic AI** emits spans itself. Turn its instrumentation on without message content:

```python
from pydantic_ai import Agent
from pydantic_ai.models.instrumented import InstrumentationSettings

state.start_telemetry()
Agent.instrument_all(InstrumentationSettings(include_content=False))
agent = Agent("openai:gpt-4o", tools=[lookup_order])


async def answer(question: str) -> tuple[str, str]:
    with state.run("agent") as run:
        result = await agent.run(question)
    return result.output, run.run_id
```

`include_content=False` drops prompts, completions, tool arguments, and tool results. Pydantic AI still records each tool's name, description, and JSON schema in `gen_ai.tool.definitions` and `model_request_parameters`.

**Google ADK** emits spans itself, under the instrumentation scope `gcp.vertex.agent`. It records message content in its spans by default; set `ADK_CAPTURE_MESSAGE_CONTENT_IN_SPANS=false` in the process environment, and leave `OTEL_INSTRUMENTATION_GENAI_CAPTURE_MESSAGE_CONTENT` unset.

```python
from google.adk.agents import LlmAgent
from google.adk.runners import InMemoryRunner
from google.genai import types

state.start_telemetry()
agent = LlmAgent(name="assistant", model="gemini-2.5-flash", tools=[lookup_order])
runner = InMemoryRunner(agent=agent)


async def answer(question: str) -> tuple[str, str]:
    session = await runner.session_service.create_session(app_name=runner.app_name, user_id="support")
    message = types.Content(role="user", parts=[types.Part(text=question)])
    reply = ""
    with state.run("agent") as run:
        async for event in runner.run_async(user_id="support", session_id=session.id, new_message=message):
            if event.is_final_response() and event.content and event.content.parts:
                reply = event.content.parts[0].text or ""
    return reply, run.run_id
```

**The OpenAI Agents SDK** records its own traces rather than OpenTelemetry spans. Give it `WyrdAgentsTraceProcessor`, and each agent, tool, handoff, and model call becomes a span named by the OpenTelemetry GenAI conventions, such as `invoke_agent assistant`, `execute_tool lookup_order`, and `chat gpt-4o`. The processor records names, models, token counts, and an error status, never inputs or outputs. Wyrd never installs it for you.

<LangTabs langs={['python', 'typescript']}>
<LangTab lang="python">

```python
from agents import Agent, Runner, function_tool, set_trace_processors
from wyrd.otel import WyrdAgentsTraceProcessor

state.start_telemetry()
set_trace_processors([WyrdAgentsTraceProcessor()])
agent = Agent(name="assistant", model="gpt-4o", tools=[function_tool(lookup_order)])


async def answer(question: str) -> tuple[str, str]:
    with state.run("agent") as run:
        result = await Runner.run(agent, question)
    return str(result.final_output), run.run_id
```

</LangTab>
<LangTab lang="typescript">

```typescript
import { Agent, run, setTraceProcessors, tool } from "@openai/agents";
import { z } from "zod";
import { WyrdAgentsTraceProcessor } from "wyrd-ai";

await state.startTelemetry();
setTraceProcessors([new WyrdAgentsTraceProcessor()]);
const lookupOrder = tool({
  name: "lookup_order",
  description: "Return an order's shipping status.",
  parameters: z.object({ orderId: z.string() }),
  execute: async ({ orderId }) => `Order ${orderId} shipped yesterday.`,
});
const agent = new Agent({ name: "assistant", model: "gpt-4o", tools: [lookupOrder] });

async function answer(question: string): Promise<[string, string]> {
  const agentRun = state.run("agent");
  const result = await agentRun.scope(() => run(agent, question));
  return [String(result.finalOutput), agentRun.runId];
}
```

</LangTab>
</LangTabs>

`set_trace_processors` replaces the SDK's default exporter to OpenAI; use `add_trace_processor` (`addTraceProcessor`) to keep it. In TypeScript, `OpenAIChatCompletionsModel` reports no token counts on its spans; `OpenAIResponsesModel` does.

A span started inside the run keeps the run even when it finishes after the block exits, such as a streamed response you consume later. Nested and concurrent runs each keep their own `run_id`, and spans outside any run export without one.

None of this needs the AI Gateway. To send the model calls through it as well, give the framework's OpenAI client `http_auth` (`httpAuth`), as in [Call Wyrd from a stock client](/connect/call-wyrd-from-a-stock-client/#openai).

These recipes are tested with Pydantic AI 2.56, Google ADK 2.12, OpenAI Agents SDK 0.23 for Python, and OpenAI Agents SDK 0.20 for TypeScript.

#### Read the agent's spans back

Ask a question, flush, and read the run's spans back with the same ambient credential:

<LangTabs langs={['python', 'typescript']}>
<LangTab lang="python">

```python
import asyncio

from wyrd.bifrost import Bifrost

reply, run_id = asyncio.run(answer("Where is order 1234?"))
state.flush()
spans = Bifrost().sql(
    "SELECT name, gen_ai_usage_input_tokens, gen_ai_usage_output_tokens FROM vala.traces.spans WHERE run_id = $1",
    [run_id],
)
```

</LangTab>
<LangTab lang="typescript">

```typescript
import { Bifrost } from "wyrd-ai";

const [reply, runId] = await answer("Where is order 1234?");
await state.flush();
const spans = await (await Bifrost.connect()).sql(
  "SELECT name, gen_ai_usage_input_tokens, gen_ai_usage_output_tokens FROM vala.traces.spans WHERE run_id = $1",
  [runId],
);
```

</LangTab>
</LangTabs>

`flush` exports the spans and drains the writer, and leaves both open, so the agent keeps working afterward. As with any row, spans become queryable a short time after `flush` returns.

#### Keep your own tracer provider

If your application already installs a tracer provider, `start_telemetry` refuses with `WYRD_SDK_409_TELEMETRY_PROVIDER_EXISTS`. Add Wyrd's exporter and run correlation to your provider instead:

<LangTabs langs={['python', 'typescript']}>
<LangTab lang="python">

```python
from opentelemetry import trace
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import BatchSpanProcessor
from wyrd import WyrdClient
from wyrd.otel import install_run_correlation, span_exporter

provider = TracerProvider()
provider.add_span_processor(BatchSpanProcessor(span_exporter(WyrdClient())))
install_run_correlation(provider)
trace.set_tracer_provider(provider)
```

</LangTab>
<LangTab lang="typescript">

```typescript
import { context, trace } from "@opentelemetry/api";
import { AsyncLocalStorageContextManager } from "@opentelemetry/context-async-hooks";
import { BasicTracerProvider, BatchSpanProcessor } from "@opentelemetry/sdk-trace-base";
import { WyrdClient } from "wyrd-ai";
import { runCorrelationProcessor, spanExporter } from "wyrd-ai/otel";

const provider = new BasicTracerProvider({
  spanProcessors: [runCorrelationProcessor(), new BatchSpanProcessor(spanExporter(WyrdClient.connect()))],
});
trace.setGlobalTracerProvider(provider);
context.setGlobalContextManager(new AsyncLocalStorageContextManager().enable());
```

</LangTab>
</LangTabs>

You own this provider: `shutdown` on the state never flushes it, so shut it down yourself at exit.

Either provider samples by the standard `OTEL_TRACES_SAMPLER` and `OTEL_TRACES_SAMPLER_ARG` variables; a span the sampler drops is never exported. The server stores a span only when the credential may attribute it: a span naming a Card outside the credential's scope or tenant is rejected on its own, and the rest of the export is stored.

## 4. Verify inside the run

`verify` and `submit` are how a run gets judged. Both take the name of a Verifier bound to the run's subject, plus one keyword per input slot the Verifier declares. Both record the inputs as evidence under the run, so the verdict joins back to the ticket, the trace, and the model call.

### Verify now

Use `verify` when your code needs the verdict before it replies. It blocks until the Verifier finishes. The support desk checks every answer this way, once on the Service and once on the agent:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

<CodeFromFile file="examples/support-desk-app/python/desk.py" lang="python" region="check" title="desk.py" />

</LangTab>
<LangTab lang="rust">

<CodeFromFile file="examples/support-desk-app/rust/lib.rs" lang="rust" region="check" title="lib.rs" />

</LangTab>
<LangTab lang="typescript">

<CodeFromFile file="examples/support-desk-app/typescript/server.ts" lang="typescript" region="check" title="server.ts" />

</LangTab>
</LangTabs>

The view decides which bindings apply. `no-refund-promise` is bound to the Service, so it runs on `run`. `ticket-complete` is bound to the agent component, so it runs on `run.for_card("agent")`. Ask a view for a Verifier that isn't bound to its Card and the call fails with `WYRD_SDK_404_UNKNOWN_VERIFIER`.

The result carries `verdict` (`passed`, `failed`, or `inconclusive`) and one entry per requirement with its `outcome`, `reason`, and `citations`. A failed verdict is a normal return, not an error. What to do about it is your call: the support desk returns it to the caller, and a stricter service would swap the answer for a handoff.

Keep `verify` for checks that are quick. Each call blocks the request for the whole execution, so a Verifier that calls a model belongs in `submit`.

### Submit for later

Use `submit` when a slower check, such as a model judge, should run in the background. It uploads the inputs, queues one execution on the server, and returns at once. The support desk submits the refund decision letter its agent wrote:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

<CodeFromFile file="examples/support-desk-app/python/desk.py" lang="python" region="submit" title="desk.py" />

</LangTab>
<LangTab lang="rust">

<CodeFromFile file="examples/support-desk-app/rust/lib.rs" lang="rust" region="submit" title="lib.rs" />

</LangTab>
<LangTab lang="typescript">

<CodeFromFile file="examples/support-desk-app/typescript/server.ts" lang="typescript" region="submit" title="server.ts" />

</LangTab>
</LangTabs>

`verify.File` fills a `media` input slot, from a path or, as here, from bytes; the file is hashed and uploaded during the call. The handle's `id` is all anyone needs to read the result later. Store it, return it to your caller, or hand it to a worker:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

<CodeFromFile file="examples/support-desk-app/python/app.py" lang="python" region="status" title="app.py" />

</LangTab>
<LangTab lang="rust">

<CodeFromFile file="examples/support-desk-app/rust/lib.rs" lang="rust" region="status" title="lib.rs" />

</LangTab>
<LangTab lang="typescript">

<CodeFromFile file="examples/support-desk-app/typescript/server.ts" lang="typescript" region="status" title="server.ts" />

</LangTab>
</LangTabs>

`status()` returns the execution's `status` (`pending`, `running`, `retrying`, `completed`, `cancelled`, `timed_out`, or `errored`), plus `result` once it completed or `error` once it failed. To block until it is done instead, call `execution.wait(timeout)`, which polls and returns the same result `verify` would.

On a drift Verifier, `submit` records samples for its schedule rather than queueing an execution, and returns `Recorded`. [Monitor drift](/build-and-verify/verify/monitor-drift/) covers that path.

## 5. Shut down cleanly

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

## 6. Read a run back

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
from wyrd.bifrost import Bifrost

rows = Bifrost().sql(
    "SELECT question, answer, card_uid FROM vala.datasets.tickets WHERE run_id = $1",
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
        "SELECT question, answer, card_uid FROM vala.datasets.tickets WHERE run_id = $1",
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
  "SELECT question, answer, card_uid FROM vala.datasets.tickets WHERE run_id = $1",
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

The ticket row has `card_uid` set to the `support-agent` version that ran. The Agent's spans in `vala.traces.spans` and the gateway call in `vala.gateway.calls` carry the same `run_id`, so one join reads the whole request back. Rows become queryable a short time after `flush` returns. An empty result right after a flush is not proof they are missing; retry first.

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
| `WYRD_SDK_409_TELEMETRY_PROVIDER_EXISTS` | At `start_telemetry` | The application already installed a tracer provider | [Keep your own tracer provider](#keep-your-own-tracer-provider) |
| `WYRD_SDK_504_TELEMETRY_FLUSH_TIMEOUT` | At `flush` or `shutdown` | The tracer provider `start_telemetry` installed did not flush in time | Check that the server is reachable. The writer still drained; `details.native_code` names its failure, if any |
| `WYRD_SDK_503_TELEMETRY_FLUSH_FAILED` | At `flush` or `shutdown` | That provider failed to flush. In TypeScript this includes a refused span export | Check that the credential may write telemetry. `details.native_code` names any writer failure |

Card-scope and permission refusals arrive at `flush` or `shutdown`, not at the `observe` call, because the server checks them when the batch lands. The Python OpenTelemetry SDK logs a refused span export rather than failing its flush, so look for it in the log. Call `flush` in a smoke test after deploying so a wrong key fails there instead of silently at exit.
