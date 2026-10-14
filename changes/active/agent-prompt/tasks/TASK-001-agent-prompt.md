# TASK-001 Resolve an Agent's Prompt in Rust, TypeScript, and Python

Spec: `changes/active/agent-prompt/spec.md` r1, REQ-001.

## Starting point

Unverified work-in-progress exists on branch `task/agent-prompt`, commit
`cca791ab6`. It drops the `internal` gate on `WyrdState::agent_prompt`, makes
it the single owner (`Run::invoke` calls it), adds the napi and PyO3
projections, the TypeScript method, the Python stub, docs, and both journeys.
Cherry-pick it as the base, then finish:

1. `mise run ts:build` and commit the regenerated `sdks/wyrd-sdk-ts/wyrd/index.d.ts`.
2. Revert any formatting-only change in files the feature does not touch.
3. Run the evidence below and fix what fails.

## Evidence

- `sdks/wyrd-sdk-ts/wyrd/tests/integration/register-and-hydrate.test.ts`:
  "agent prompt resolves through the agent".
- `sdks/wyrd-sdk-python/tests/integration/test_register_and_hydrate.py`:
  `test_agent_prompt_resolves_through_the_agent`.
- Lanes: `mise run fmt`, `mise run lints`, `mise run codegen:check`,
  `mise run ts:napi:check`, `mise run ts:test:integration`,
  `mise run py:test:integration`, `mise run py:typecheck`.

## Follow-up outside this repository

In `wyrd-example-ts`, remove the `prompt` component from
`service/support-api.yaml` and read the Prompt with
`state.agentPrompt("agent")` in `src/agent.ts`.
