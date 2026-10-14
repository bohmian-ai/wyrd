---
id: SPEC-agents-trace-processor
revision: 1
status: approved
---

# OpenAI Agents SDK trace processor

## Objective and user value

Wyrd ships a trace processor the user passes to the OpenAI Agents SDK, so agent, model, and tool spans land in `vala.traces.spans` under the Run.

## Approval

Approved by the human owner on 2026-10-14 as part of SPEC-example-journey-fixes
r6. Split into its own change by owner direction on 2026-10-14 so each problem
is reviewed and verified on its own. The requirement text is carried over
unchanged.

## Required behavior

### REQ-010 OpenAI Agents SDK trace processor (r6)

Owner directive 2026-10-14: Wyrd works with OpenTelemetry instrumentation as
a standard OTLP backend, and provides a trace processor that the user gives
to the Agents SDK. Wyrd stays framework agnostic and never registers it.

- TypeScript `wyrd-ai` exports `WyrdAgentsTraceProcessor`, implementing the
  `@openai/agents` `TracingProcessor` interface. Python `wyrd` exports the
  same processor for `openai-agents`. Each converts Agents SDK traces and
  spans into OpenTelemetry spans on the active global tracer provider,
  preserving parent and child structure. Generation and response spans carry
  OpenTelemetry GenAI attributes (`gen_ai.request.model`,
  `gen_ai.usage.input_tokens`, `gen_ai.usage.output_tokens`).
- The user installs it with the SDK's own API, for example
  `setTraceProcessors([new WyrdAgentsTraceProcessor()])`. `startTelemetry()`
  neither exports nor registers it, and Wyrd never patches the Agents SDK.
- `@openai/agents` and `openai-agents` are optional dependencies; importing
  `wyrd-ai` or `wyrd` without them works.
- With `startTelemetry()` running inside `Run.scope`, the converted spans
  carry `wyrd.run_id` and `wyrd.card_uid` like any other span and land in
  `vala.traces.spans`.

## Invariants

- INV-001 Tenant isolation holds on every changed path.

## Acceptance obligations

- AC-001 Each REQ has a user-journey test (Rust, Python, and TypeScript where
  the surface ships in all three; MCP/HTTP where agent-facing) that fails on
  the base and passes on the candidate, booting the server the way users do
  where the defect was harness-masked (REQ-003 uses the official TypeScript
  MCP SDK; REQ-007 drives the CLI).
- AC-003 Format, lints, codegen check, and the owning `mise` lanes pass.
- AC-007 (r6) TypeScript and Python journeys: an Agents SDK agent run with
  `WyrdAgentsTraceProcessor` installed and `startTelemetry()` running inside
  `Run.scope` produces spans in `vala.traces.spans` carrying the Run ID and
  GenAI model attribute. Importing the SDK without the Agents SDK installed
  succeeds.

## Open material decisions

None.
