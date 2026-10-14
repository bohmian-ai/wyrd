---
id: SPEC-agent-prompt
revision: 1
status: approved
---

# WyrdState returns the Prompt an Agent uses

## Objective and user value

An application that runs an Agent reads that Agent's Prompt through the Agent
alone. The Service lists only the Agent; it does not also list the Prompt the
Agent already references.

## Approval

Owner direction 2026-10-14: "components shouldn't need to list support prompt
because support agent already references it. It should be automatically
resolved and registered."

## Current behavior

- `wyrd apply` already registers the Prompt an Agent references, and hydration
  already writes it into the bundle.
- An Agent Card's `spec.prompt` is either an inline Prompt or an exact Card
  reference (kind, name, version, space, uid). `state.agent(alias)` returns the
  typed Agent Card, so a reference does not give the caller the Prompt itself.
- The Rust client resolves it (`WyrdState::resolve_agent_prompt`,
  `crates/shared/wyrd-client/src/state.rs`), but only behind the `internal`
  feature. The TypeScript and Python SDKs have no equivalent, so an
  application must list the Prompt as its own Service component and read it by
  that alias.

## Required behavior

### REQ-001 Resolve an Agent's Prompt through the Agent

- Rust `WyrdState::agent_prompt(alias)` is public and is the single owner of
  Agent Prompt resolution. `Run::invoke` uses it.
- TypeScript `state.agentPrompt(alias)` and Python `state.agent_prompt(alias)`
  project it and return the Prompt spec, for an inline Prompt and for a
  referenced Prompt Card alike.
- Errors: an unknown alias is `WYRD_SDK_404_UNKNOWN_ALIAS`; an alias that is
  not an Agent is `WYRD_SDK_400_CARD_KIND_MISMATCH`; a reference whose target
  is missing from the bundle or is not a Prompt Card is
  `WYRD_SDK_400_INVALID_STATE_BUNDLE`.
- Resolution reads the local bundle only. It makes no server call.

## Invariants

- INV-001 No durable behavior moves into an SDK; TypeScript and Python project
  the Rust owner.

## Acceptance obligations

- AC-001 Register-and-hydrate journeys in TypeScript and Python: a Service that
  lists only its Agent hydrates, and `agentPrompt` / `agent_prompt` returns the
  Agent's Prompt; a non-Agent alias is refused with the kind-mismatch code.
- AC-002 Format, lints, codegen check, `ts:napi:check`, and the owning `mise`
  lanes pass.

## Open material decisions

None.
