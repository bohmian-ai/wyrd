---
id: SPEC-responses-input-forms
revision: 1
status: approved
---

# The gateway accepts every Responses input form the Agents SDK sends

## Objective and user value

An OpenAI Agents SDK agent using the Responses API calls models through the
Wyrd gateway with no client-side workaround.

## Approval

Approved by the human owner on 2026-10-14 under SPEC-example-journey-fixes r6
AC-004 (the example runs clean with no workarounds). Split into its own change
by owner direction on 2026-10-14.

## Defect

The gateway checks a `/v1/responses` body by decoding it as Skald's
`OpenAiResponsesRequest`, which accepted only the typed message form and string
tool results. The Agents SDK sends two other forms OpenAI accepts, so every
Agents SDK Responses agent got `WYRD_GATEWAY_400_INVALID_REQUEST` before any
provider call:

- the easy input message `{"role": "user", "content": "go"}` (no `type`, text
  `content`);
- a tool result whose `output` is content parts
  (`[{"type": "input_text", "text": ...}]`).

## Required behavior

### REQ-001 Responses input forms

- An easy input message decodes as the typed message it stands for.
- A function call output accepts text or content parts and keeps the form the
  caller sent.
- Both forms reach the provider unchanged.

## Invariants

- INV-001 Tenant isolation holds on every changed path.

## Acceptance obligations

- AC-001 A unit test decodes the captured Agents SDK body shape.
- AC-002 Format, lints, codegen check, and the owning `mise` lanes pass.

## Open material decisions

None. A TypeScript gateway journey that drives the Agents SDK
`OpenAIResponsesModel` with a tool through `WyrdTestServer` is not written yet
and is recorded as a gap in the pull request.
