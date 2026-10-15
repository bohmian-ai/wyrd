# Navigate from a hydrated Agent Card to its Prompt Card

- Change ID: `SPEC-agent-prompt`, approved revision 2.
- Completed: 2026-10-15.
- Reviewed target: not supplied; no final-change-review PASS is claimed.
- Completion target: `5448c8312c0fb4348970cdf83678f0aee9d63edb` (cumulative base `c1aac2d132b19848ca1940c3907e1fa68a8d08a5`).
- Delivery reference: not supplied.
- Completion authority: owner-approved closeout on 2026-10-15 waiving final change review.

## Intent and behavior

An Agent declares its Prompt once, and a hydrated Agent navigates to that
Prompt as a registered Prompt Card; a Service no longer lists the Prompt as a
separate component. An explicitly aliased Prompt stays independently reachable.

- Server registration materializes an inline Agent Prompt, including direct
  HTTP input, as its own Prompt Card named `<agent>-prompt` in the Agent's
  space, inside the existing composite registration transaction. The Agent
  persists the exact UID-bearing reference. Path and CardRef Prompts keep their
  authored identities.
- Rust `state.agent(alias).prompt()`, Python `AgentCard.prompt()` and
  TypeScript `agent.prompt()` return the hydrated Prompt Card; `PromptCard.ref`
  (Rust reference accessor) carries the resolved version and UID;
  `prompt_ref`/`promptRef` remain.
- Generic lookup: Python `state[alias]`/`state.get(alias)` return the public
  `HydratedCard` union that narrows with `isinstance`; Rust `get` returns a
  `HydratedCard` enum; TypeScript `get` returns a union narrowing on `kind`.
- Stable failures: `WYRD_SDK_404_UNKNOWN_ALIAS`,
  `WYRD_SDK_400_CARD_KIND_MISMATCH`, and `WYRD_SDK_400_INVALID_STATE_BUNDLE`
  for a bundle whose Agent Prompt has no registered identity. Local reads do
  no IO, registration or version selection. No public state-level
  `agent_prompt` method exists.

## Lasting decisions and constraints

- Protocol doctrine 17 now carries one exception: inline Prompts of registered
  Agent Cards register. Other inline kinds and standalone Workflow loading are
  unchanged.
- The generated Prompt uses its own normal version line: identical content
  reuses its exact identity, changed content gets the next version, and each
  Agent version keeps the Prompt it pinned. Different Agents never share
  implicitly. A generated name that collides with an authored line follows the
  ordinary registry rules; there is no reserved namespace.
- Reapplying an unchanged exact Agent pin reuses the Prompt version that Agent
  already pins. Version selection counts the submitting registration's own
  Pending rows together with Active rows, so one composite selecting the same
  line twice reuses or advances its own earlier choice. Other operations'
  Pending rows remain invisible.
- Cards remain independent: navigation implies no ownership or cascading delete.
- Accepted risk (owner direction): a concurrent deletion of a matching reused
  Prompt during registration can leave an Agent whose Prompt cannot be
  hydrated. No lock, Active recheck or delete-policy change was added.

## Acceptance evidence and limits

| Obligation | Recorded evidence |
|---|---|
| REQ-002/003, AC-001/002: server materialization, identity, versioning, replay, pin preservation, same-transaction reuse, denial and tenancy | `inline_agent_prompt_materializes`, `inline_agent_prompt_reapply_preserves_pin`, `inline_agent_prompt_reuses_transaction_selection`, and all 30 registration route tests pass; Rust journey `agent_prompt_registration_identity` passes |
| REQ-001/004/005, AC-003/004: navigation, generic lookup, offline reads, negative codes | Rust `hydrated_agent_prompt_card_navigation`, Python `test_hydrated_agent_prompt_card_navigation` and unit hydration tests, TypeScript "hydrated agent prompt card navigation" pass |
| REQ-006, AC-005: public types and compile-only contracts | Python `state_contracts.py` and runtime `test_hydrated_card_is_public_runtime_type`; TypeScript `card-types.test-d.ts`; format, lint, typecheck, `check:deps`, `codegen:check`, `check:tenant-isolation` and docs checks recorded passing |

These are retained implementation and task-review results. Task review
dispositioned FIND-TASK-001-1 as accepted risk and closed FIND-2 through FIND-5
after remediations R1 and R2; no independent final change review ran.

## Current authority, owners and tests

- [Protocol authority](../../../architecture/wyrd-design.md), [doctrine](../../../architecture/wyrd-doctrine.mdx) and [user docs](../../../docs/src/content/docs/understand/cards-versions-and-relationships.md).
- [Registration service](../../../crates/wyrd/wyrd-server/src/components/cards/service.rs) and [version resolution](../../../crates/wyrd/wyrd-sql/src/queries/cards/version_resolve.rs).
- [Shared state](../../../crates/shared/wyrd-client/src/state.rs), [Python AgentCard](../../../crates/wyrd/wyrd-cards/src/agent.rs), [Python HydratedCard](../../../sdks/wyrd-sdk-python/python/wyrd/state/_hydrated.py) and [TypeScript SDK](../../../sdks/wyrd-sdk-ts/wyrd/src/index.ts).
- Tests: [server route](../../../crates/wyrd/wyrd-server/tests/integration/pg_card_registration_route.rs), [Rust journey](../../../sdks/wyrd-sdk-rust/tests/integration/register_and_hydrate.rs), [Python journey](../../../sdks/wyrd-sdk-python/tests/integration/test_register_and_hydrate.py), [Python type contract](../../../sdks/wyrd-sdk-python/tests/typecheck/state_contracts.py), [TypeScript journey](../../../sdks/wyrd-sdk-ts/wyrd/tests/integration/register-and-hydrate.test.ts) and [TypeScript type contract](../../../sdks/wyrd-sdk-ts/wyrd/tests/types/card-types.test-d.ts).
