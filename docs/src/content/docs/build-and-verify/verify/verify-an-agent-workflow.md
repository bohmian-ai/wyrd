---
title: Verify an agent's workflow
description: Catch an agent that skipped a required step even though its answer looks right, by checking the trace of the work behind the answer.
pillar: wyrd
section: Build and verify
topic: Verify
order: 2.5
---

Check that a refund agent looked up the order before it answered, not only that its answer looks right.

The agent in this guide has a bug: when it remembers an order from earlier in the conversation, it skips the lookup. Its reply is word for word the same either way, so a check on the reply alone cannot tell the two runs apart. The agent and its order record are synthetic, and no model is called.

## Before you start

Finish [Set up Wyrd](/get-started/setup/). Work in its `quickstart` directory with the server running.

## 1. Add tracing

The agent records each step as an OpenTelemetry span, and Wyrd stores the spans as its trace. Add the tracing packages, and Pydantic or Zod for the Verifier's inputs:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```bash
uv add "wyrd-ai[otel,pydantic]==0.4.2"
```

</LangTab>
<LangTab lang="rust">

```bash
cargo add wyrd-ai@0.4.2 --features otel
cargo add opentelemetry@0.31 tracing@0.1 tracing-opentelemetry@0.32
```

</LangTab>
<LangTab lang="typescript">

```bash
pnpm add zod @opentelemetry/api @opentelemetry/context-async-hooks @opentelemetry/core @opentelemetry/resources @opentelemetry/sdk-trace-base @opentelemetry/exporter-trace-otlp-proto @opentelemetry/exporter-logs-otlp-proto @opentelemetry/exporter-metrics-otlp-proto
```

</LangTab>
</LangTabs>

## 2. Write the agent and its Verifier

The `refund-workflow` Verifier takes the agent's `reply` and the `trace` of the work behind it, declared as one model whose `trace` field holds the trace ID. It has two requirements:

| Requirement | What it checks |
| --- | --- |
| `reply-names-order` | The reply contains `Order 1042` |
| `looked-up-order` | The trace holds a span named `look-up-order` |

Save the program:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

<CodeFromFile file="examples/refund-agent/refund_agent.py" lang="python" title="refund_agent.py" />

</LangTab>
<LangTab lang="rust">

<CodeFromFile file="examples/refund-agent/refund_agent.rs" lang="rust" title="src/main.rs" />

</LangTab>
<LangTab lang="typescript">

<CodeFromFile file="examples/refund-agent/refund_agent.ts" lang="typescript" title="refund_agent.ts" />

</LangTab>
</LangTabs>

The agent answers the same request twice: once with the lookup, once with a remembered order. It returns each reply with its trace ID. The program then verifies three cases: the complete run, the skipped run, and the complete reply sent without its trace.

## 3. Run it

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```bash
uv run refund_agent.py
```

</LangTab>
<LangTab lang="rust">

```bash
cargo run
```

</LangTab>
<LangTab lang="typescript">

```bash
node refund_agent.ts
```

</LangTab>
</LangTabs>

<CodeFromFile file="examples/refund-agent/output.txt" lang="text" title="output" />

All three replies pass `reply-names-order`. Only the trace separates them:

- **complete** passes, because its trace holds the lookup.
- **skipped** fails `looked-up-order`, because its trace has no `look-up-order` span.
- **no trace** is `inconclusive`: with no trace there is no evidence about what ran, so Wyrd does not decide. An inconclusive verdict is never a pass.

<Aside type="note" title="Spans must be stored first">

The program flushes its spans before it verifies. A trace with no stored span is `inconclusive` under `verify`, so flush, or `submit` the check, which retries while the spans arrive. [Trace evidence](/understand/how-verification-works/#trace-evidence) explains both.

</Aside>

## If it fails

| You see | Fix |
| --- | --- |
| `complete` and `skipped` are both `inconclusive` on `looked-up-order` | The spans were not stored before the check. Keep the flush before the first `verify` |
| `WYRD_SDK_409_TELEMETRY_PROVIDER_EXISTS` | Your application already installed a tracer provider. Add Wyrd's span exporter to it instead, as in [ASGI apps](/connect/integrate-asgi/) |
| An `ImportError` or missing module naming OpenTelemetry | Add the tracing packages from step 1 |

## Next

[Investigate a failed verification](/build-and-verify/verify/run-a-verification-and-inspect-its-evidence/) reads the stored evidence behind the `skipped` failure. [Verifier recipes](/build-and-verify/verify/verifier-recipes/#trace) shows more trace checks, such as span position and attributes. [Establish sufficient evidence](/build-and-verify/observe/establish-sufficient-evidence/) covers when missing evidence should hold a decision.
