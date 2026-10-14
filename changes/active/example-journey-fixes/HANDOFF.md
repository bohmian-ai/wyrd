# Handoff: example-journey-fixes

All work stopped on 2026-10-14. All agents were stopped. Nothing is pushed. This file covers every branch involved.

## What works

`ejf-int` at 587b0d76f is the integration branch. TASK-007, TASK-009 and TASK-011 are merged into it. Built from this branch, `wyrd-example-ts` runs end to end:

- `npm run ask`: 3 answers delivered and the bait question blocked.
- `npm test`: 3 passed.
- `npm run query`: the verdicts and the blocked answer joined to its `support-agent` span.
- `tsc --noEmit`: clean.

The example repo commit is `8f5ad31` in `wyrd-example-ts`. It is not pushed.

## Branches

| Branch | Head | State |
|---|---|---|
| `ejf-int` | 587b0d76f | Integration. Merged and working. |
| `task/TASK-008` | 28f760b6a | WIP, unverified. Implements deployment addressing (`<deployment>/<model>`; a deployment serves all of its provider's models unless it lists `models`; weighted routing removed; `gateway:invoke` open by default). Still to do: migrate `examples/support-desk`, and fix the Python `NotRequired` break on 3.10 with a `total=False` TypedDict base. Do not add Prompt `spec.deployment`. |
| `task/agent-prompt` | cca791ab6 | WIP, unverified. Adds `state.agentPrompt("agent")` to the TS and Python SDKs over the existing `WyrdState::resolve_agent_prompt` (`crates/shared/wyrd-client/src/state.rs`). |
| `spec-r7` | 7c374417f | Draft Prompt authoring revision. It is not approved. Open decisions OD-R7-1 to OD-R7-5 are inside it. |
| `task/prompt-output` | 8eb7f8ef9 | Abandoned. Do not merge. |
| `spec-r6` | | Approved spec r6 (REQ-009 gateway addressing, REQ-010 trace processor). |

## Next steps, in order

1. Finish `task/agent-prompt`, review it, and merge it into `ejf-int`. Then in the example, delete the `- alias: prompt` component from `service/support-api.yaml` and call `state.agentPrompt("agent")` in `src/agent.ts`. Registration already resolves and registers the Agent's Prompt, so the example should not list it.
2. Finish `task/TASK-008`, review it, and merge it. Then recreate the local gateway deployment as `openai` (no `models`, no `routing_weight`), which matches the README.
3. Rebuild the server and the TS SDK from `ejf-int`, then run the example README from a clean Postgres.

## Known defects, not fixed

- A revoked key returns 400 instead of 401.
- TASK-011 has no TS journey test.
- `WyrdAgentsTraceProcessor.shutdown` and `forceFlush` are no-ops.
- Python `GatewayAuth` ignores `config.toml`.
- A CLI broken pipe returns 500.
- The `has_max_length` operator requires a redundant `expected`.
- A declarative Prompt cannot declare a structured output, so an LLM judge Prompt must use the native form, which repeats the model and the variables. The example uses two assertion Verifiers for this reason. `spec-r7` is the draft fix.
- `examples/support-desk` and the workflow fixtures still use the native Prompt form.

## Local state on the dev machine

Scratch worktrees are under the session scratchpad in `/tmp`, so they do not survive a reboot. Every branch above is committed, so nothing is lost if they go. The live demo used a scratch copy of the example with Cards bumped to 1.2.0 in tenant `acme`, because the platform credential for a fresh `setup` was not saved.
