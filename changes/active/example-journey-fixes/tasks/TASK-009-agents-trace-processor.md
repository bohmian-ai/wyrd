# TASK-009 OpenAI Agents SDK trace processor

Spec: `changes/active/example-journey-fixes/spec.md` r6, REQ-010 and AC-007.

## Outcome

```ts
import { setTraceProcessors } from "@openai/agents";
import { startTelemetry, WyrdAgentsTraceProcessor } from "wyrd-ai";

startTelemetry();
setTraceProcessors([new WyrdAgentsTraceProcessor()]);
```

```python
from agents import set_trace_processors
from wyrd.otel import WyrdAgentsTraceProcessor  # place it with the existing otel surface

set_trace_processors([WyrdAgentsTraceProcessor()])
```

## Scope

- TypeScript: `sdks/wyrd-sdk-ts/wyrd/src/` next to `otel.ts`. Implement the
  `@openai/agents` `TracingProcessor` interface (read its `.d.ts` in
  `node_modules` for the exact method set: trace and span start/end,
  shutdown, forceFlush). Convert each SDK span to an OpenTelemetry span via
  the global tracer provider (`@opentelemetry/api`), keeping SDK parent/child
  structure (map SDK span ids to OTel contexts). Generation/response spans
  set `gen_ai.request.model`, `gen_ai.usage.input_tokens`,
  `gen_ai.usage.output_tokens`; agent, function/tool, and handoff spans use
  their SDK names. Spans started within `Run.scope` must inherit the active
  context so the existing run-context processor stamps `wyrd.run_id`.
  `@openai/agents` is an optional peer dependency; use `import type` only so
  importing `wyrd-ai` without it works.
- Python: same processor for `openai-agents`
  (`agents.tracing.TracingProcessor`), in the Python SDK package beside its
  OTel surface, optional dependency (extra), exported and stubbed per
  AGENTS.md section 8.
- `startTelemetry()` and its Python equivalent do not change and do not
  reference the processor.
- Docs: one short section in the existing OTel/telemetry guide under `docs/`.

## Evidence

- TS and Python integration journeys against `WyrdTestServer`: an Agents SDK
  agent whose model is a local fake (no live provider; use the SDK's model
  interface or the existing gateway fixture) runs inside `Run.scope` with
  `startTelemetry()` and the processor installed; read `vala.traces.spans`
  back through Bifrost and assert the agent and generation spans exist, share
  one trace, carry `wyrd.run_id`, and the generation span carries
  `gen_ai.request.model`. Follow `tests/integration/otel-export.test.ts` and
  `tests/integration/test_otel_export.py` for read-back.
- Unit: importing `wyrd-ai` / `wyrd` without the Agents SDK succeeds.
- Lanes: `mise run fmt`, TS lint/typecheck/unit and the touched integration
  test, `mise run py:format`, `py:lints`, `py:test:unit`, `py:typecheck`,
  `codegen:check` if stubs are generated, `docs:check`.
